//! Command implementations. `main` builds a [`Ctx`] and dispatches; the
//! tests build one over an in-memory vault, a scratch lock and cache, and a
//! stand-in browser and window.

pub mod api;
pub mod auth;
pub mod config;
pub mod documents;
pub mod orgs;
pub mod patients;
pub mod profiles;
pub mod records;

use std::path::PathBuf;
use std::sync::Arc;

use pk_cli_auth::refresh_lock::RefreshLock;
use pk_cli_auth::vault::KeyStore;
use pk_cli_config::profile::{self, ProfileName};
use pk_cli_config::ConfigStore;
use pk_cli_core::{CliError, CommonArgs};
use pk_cli_http::BlockingTransport;
use pk_cli_secrets::CredentialStore;
use pk_med_smart::SmartSignIn;

use crate::config::Config;
use crate::session::{Context, Index, ProfileSettings};
use crate::vault::{Sessions, Vault};

pub const BIN: &str = "mychart";
pub const REPO: &str = "piekstra/mychart-cli";

/// Names the Keychain service a run uses instead of `piekstra.mychart`. For
/// the test suite and for dogfooding error paths: a service that holds
/// nothing reads as signed out without a Keychain prompt. The vault's
/// entries move with it, to `vault.<service>/` beside the config file, so
/// such a run never reads the real vault's files either, and `profiles
/// remove` leaves browser profiles alone.
pub const KEYCHAIN_SERVICE_ENV: &str = "MYCHART_KEYCHAIN_SERVICE";

/// The environment's `--profile`.
pub const PROFILE_ENV: &str = "MYCHART_PROFILE";

/// What a sign-in's window opens: the profile's own browser profile, the
/// pages it needs first (a kp.org session for Kaiser Permanente's Epic),
/// and the sign-in's address.
pub struct Opening<'a> {
    pub profile: &'a ProfileName,
    pub sign_in: &'a SmartSignIn,
    pub before: &'a [&'a str],
}

/// What keeps a sign-in's window open; dropping it closes the window.
pub type Window = Box<dyn std::any::Any>;

/// Opens a sign-in's window.
pub type OpenWindow = Box<dyn Fn(&Opening<'_>) -> Result<Window, CliError>>;

/// Removes a profile's browser profile; whether there was one.
pub type ForgetBrowser = Box<dyn Fn(&ProfileName) -> Result<bool, CliError>>;

pub struct Ctx {
    pub common: CommonArgs,
    /// `--profile`, before it is checked against the config.
    pub profile: Option<String>,
    pub store: ConfigStore,
    /// Where sign-ins are kept: the CLI's vault.
    pub vault: Arc<dyn Vault>,
    /// Where mychart 0.4 kept sign-ins, one Keychain item each: what
    /// `profiles migrate` moves into the vault.
    pub legacy: Arc<dyn KeyStore>,
    pub lock: RefreshLock,
    pub cache_dir: PathBuf,
    /// Removes a profile's browser profile; whether there was one. A test's
    /// removes one under its own directory, never the owner's.
    pub forget_browser: ForgetBrowser,
    /// Opens a sign-in in the profile's own Chrome window.
    pub window: OpenWindow,
    /// Opens an address in the default browser, when the window cannot
    /// open; returns whether something was launched.
    pub browser: Box<dyn Fn(&str) -> bool>,
    /// Said before a command uses the vault, when the config file is not
    /// beside it: profiles' sign-ins and browser profiles are this
    /// computer's, whichever config names them.
    pub vault_note: Option<String>,
}

impl Ctx {
    /// The binary's context: the vault (with its key in the Keychain, or in
    /// `MYCHART_KEYCHAIN_SERVICE`), the family's refresh lock,
    /// `~/.cache/mychart`, Chrome, and the system browser.
    pub fn for_binary(
        common: CommonArgs,
        config: Option<PathBuf>,
        profile: Option<String>,
    ) -> Result<Self, CliError> {
        let store = ConfigStore::new(BIN).with_override(config);
        let vault = match test_service() {
            Some(service) => {
                let path = store.path()?;
                let dir = path.parent().unwrap_or(std::path::Path::new("."));
                pk_cli_auth::vault::Vault::new(
                    CredentialStore::new(service.clone()),
                    dir.join(format!("vault.{service}")),
                )
            }
            None => pk_cli_auth::vault::Vault::for_binary(BIN)?,
        };
        let vault_note = match test_service() {
            Some(_) => None,
            None => vault_note(&store.path()?, vault.dir()),
        };
        Ok(Ctx {
            common,
            profile,
            store,
            vault: Arc::new(vault),
            legacy: Arc::new(credential_store()),
            lock: RefreshLock::for_binary(BIN)?,
            cache_dir: crate::orgs::cache_dir()?,
            // A test-service run never removes a real browser profile.
            forget_browser: if test_service().is_some() {
                Box::new(|_: &ProfileName| Ok(false))
            } else {
                Box::new(|p: &ProfileName| {
                    pk_cli_browser::forget_named_profile(BIN, p.as_str()).map_err(CliError::from)
                })
            },
            window: Box::new(|o: &Opening<'_>| {
                pk_med_smart::SignInWindow::open(BIN, o.profile, o.sign_in, o.before)
                    .map(|w| Box::new(w) as Window)
            }),
            browser: Box::new(crate::browser::open_browser),
            vault_note,
        })
    }

    pub fn json(&self) -> bool {
        self.common.json
    }

    pub fn config(&self) -> Result<Config, CliError> {
        self.store.load()
    }

    /// mychart 0.4's index, for `profiles migrate`.
    pub fn legacy_index(&self) -> Result<Index, CliError> {
        Ok(Index::beside(&self.store.path()?))
    }

    /// The profile this command runs as: `--profile`, else
    /// `MYCHART_PROFILE`, configured. Missing or unknown is exit 2, before
    /// any Keychain read or request.
    pub fn profile_name(&self) -> Result<ProfileName, CliError> {
        let cfg = self.config()?;
        let p = profile::resolve(self.profile.as_deref(), PROFILE_ENV, &cfg.profiles, BIN)?;
        self.note_vault();
        Ok(p)
    }

    /// `p`'s patient contexts.
    pub fn contexts(&self, p: &ProfileName) -> Result<Vec<Context>, CliError> {
        let settings: ProfileSettings = self.config()?.profiles.settings(p)?;
        Ok(settings.contexts)
    }

    /// Save the config owner-only (0600) and whole, through
    /// [`crate::fsutil::write_private`]: it holds each profile's patient
    /// contexts. cli-common v0.33.0's `ConfigStore::save` writes in place at
    /// the umask's mode, which leaves the file readable by other users.
    pub fn save_config(&self, cfg: &Config) -> Result<(), CliError> {
        let body = serde_json::to_string_pretty(cfg)
            .map_err(|e| CliError::Other(format!("serializing config: {e}")))?;
        crate::fsutil::write_private(&self.store.path()?, body.as_bytes())
    }

    /// Change `p`'s settings and save the config.
    pub fn update_profile(
        &self,
        p: &ProfileName,
        change: impl FnOnce(&mut ProfileSettings),
    ) -> Result<(), CliError> {
        let mut cfg = self.config()?;
        let mut settings: ProfileSettings = cfg.profiles.settings(p)?;
        change(&mut settings);
        cfg.profiles.set_settings(p, &settings)?;
        self.save_config(&cfg)
    }

    /// `p`'s sign-ins in the vault.
    pub fn sessions(&self, p: &ProfileName) -> Sessions {
        Sessions::new(self.vault.clone(), p.clone())
    }

    /// The signed-in patient `who` names in the command's profile (the
    /// only one, when the profile holds one).
    pub fn signed_in(
        &self,
        who: &crate::cli::PatientArgs,
    ) -> Result<(ProfileName, Context), CliError> {
        let p = self.profile_name()?;
        let contexts = self.contexts(&p)?;
        let c = crate::session::pick(&p, &contexts, who.patient.as_deref(), who.org.as_deref())?
            .clone();
        Ok((p, c))
    }

    /// [`Self::vault_note`], when there is one.
    pub fn note_vault(&self) {
        if let Some(n) = &self.vault_note {
            self.note(n);
        }
    }

    pub fn note(&self, msg: impl AsRef<str>) {
        if !self.common.quiet {
            eprintln!("{}", msg.as_ref());
        }
    }

    /// [`Self::note`] as a closure a FHIR client can own.
    pub fn notes(&self) -> impl Fn(&str) + Send + Sync + 'static {
        let quiet = self.common.quiet;
        move |line: &str| {
            if !quiet {
                eprintln!("{line}")
            }
        }
    }

    /// `c`'s sign-in in profile `p` for a run of requests, renewed first
    /// when it is due.
    pub fn live(
        &self,
        t: &BlockingTransport,
        p: &ProfileName,
        c: &Context,
    ) -> Result<crate::session::Live<BlockingTransport>, CliError> {
        crate::session::Live::start(
            t,
            self.sessions(p),
            &self.lock,
            c,
            Some(std::sync::Arc::new(self.notes())),
        )
    }
}

/// The note for a config file that is not beside the vault (`--config`,
/// `MYCHART_CONFIG` or `XDG_CONFIG_HOME` elsewhere): the vault and the
/// browser profiles belong to this computer's user, not to a config file.
pub fn vault_note(config: &std::path::Path, vault: &std::path::Path) -> Option<String> {
    (config.parent() != vault.parent()).then(|| {
        format!(
            "note: {} is not beside this computer's vault: its profiles' sign-ins are in {} and \
             their browser profiles in the user's data directory, shared with any config file \
             that names the same profile",
            config.display(),
            vault.display()
        )
    })
}

fn test_service() -> Option<String> {
    std::env::var(KEYCHAIN_SERVICE_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
}

pub fn credential_store() -> CredentialStore {
    match test_service() {
        Some(service) => CredentialStore::new(service),
        None => CredentialStore::for_binary(BIN),
    }
}

/// The transport for an organization's FHIR and OAuth endpoints: no
/// redirects, the family's 60 s timeout.
pub fn transport() -> Result<BlockingTransport, CliError> {
    BlockingTransport::new()
}

/// A DTO as JSON for stdout; a failure is an error with an exit code, never
/// a `null` that reads as success.
pub fn to_json<T: serde::Serialize>(dto: &T) -> Result<serde_json::Value, CliError> {
    serde_json::to_value(dto).map_err(|e| CliError::Other(format!("serializing the output: {e}")))
}

/// RFC 3339 for unix seconds.
pub fn rfc3339(secs: i64) -> String {
    pk_cli_core::dates::fmt_rfc3339(secs)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn only_a_config_away_from_the_vault_is_noted() {
        let vault = Path::new("/home/u/.config/mychart/vault");
        assert_eq!(
            super::vault_note(Path::new("/home/u/.config/mychart/config.json"), vault),
            None
        );
        let n = super::vault_note(Path::new("/tmp/alt/config.json"), vault).unwrap();
        assert!(
            n.contains("/tmp/alt/config.json") && n.contains(vault.to_str().unwrap()),
            "{n}"
        );
    }
}
