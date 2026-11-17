//! Where sign-ins are kept: the CLI's one-key vault in the binary
//! (`pk_cli_auth::vault`: one Keychain item, its key, and an encrypted file
//! per sign-in), an in-memory map in the tests. Each entry is one
//! `pk_med_smart::SmartSession`, as JSON, named
//! `<profile>/<organization>/<patient>` as `pk_med_smart::ProfileSessions`
//! names it.
//!
//! [`Sessions`] is one profile's `SessionStore`, which pk-med-smart's
//! renewer reads and writes through. It is `Send + Sync` and owns its
//! vault, so a FHIR client's token source can hold it for a whole export.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pk_cli_config::profile::ProfileName;
use pk_cli_core::{CliError, Reason};
use pk_cli_secrets::Secret;
use pk_med_smart::{decode_session, SessionStore, SmartSession, SESSION_FORMAT};

/// For an item a newer mychart stored: a sign-in with this release would
/// replace it with an older shape.
pub const UPDATE_REMEDY: &str = "update mychart with `mychart self-update`: a newer mychart \
     stored this sign-in";

/// What to do about a stored sign-in that does not read.
pub const UNREADABLE_REMEDY: &str = "sign in again with `mychart auth login --org <ORG>`; the new \
     sign-in replaces it (a sign-in mychart 0.1 stored does not read in 0.2)";

pub trait Vault: Send + Sync {
    fn read(&self, name: &str) -> Result<Option<String>, CliError>;
    fn write(&self, name: &str, value: &str) -> Result<(), CliError>;
    /// `true` when something was removed.
    fn delete(&self, name: &str) -> Result<bool, CliError>;
    /// The shared vault behind this one, for what pk-med-smart does on it
    /// directly (moving old Keychain items in, listing a profile's
    /// entries). `None` in memory.
    fn shared(&self) -> Option<&pk_cli_auth::vault::Vault> {
        None
    }
}

impl Vault for pk_cli_auth::vault::Vault {
    fn read(&self, name: &str) -> Result<Option<String>, CliError> {
        Ok(self.get(name)?.map(|s| s.expose().to_string()))
    }
    fn write(&self, name: &str, value: &str) -> Result<(), CliError> {
        self.put(name, &Secret::new(value))
    }
    fn delete(&self, name: &str) -> Result<bool, CliError> {
        pk_cli_auth::vault::Vault::delete(self, name)
    }
    fn shared(&self) -> Option<&pk_cli_auth::vault::Vault> {
        Some(self)
    }
}

/// The vault entry of a session `account` (`smart/<org>/<patient>`, as
/// `pk_med_smart::account` makes it) in `profile`:
/// `<profile>/<org>/<patient>`, the name `pk_med_smart::ProfileSessions`
/// gives it.
pub fn entry_name(profile: &ProfileName, account: &str) -> Result<String, CliError> {
    let rest = account.strip_prefix("smart/").ok_or_else(|| {
        CliError::Other("a session account must be one `pk_med_smart::account` made".into())
    })?;
    Ok(format!("{profile}/{rest}"))
}

/// One profile's sign-ins in a [`Vault`], one `SmartSession` per account.
/// A present entry that does not read is an error (pk-med-smart's, which
/// names a line and a column and never a value), never `None`: treating it
/// as absent would read as an unexplained logout.
#[derive(Clone)]
pub struct Sessions {
    vault: Arc<dyn Vault>,
    profile: ProfileName,
}

impl Sessions {
    pub fn new(vault: Arc<dyn Vault>, profile: ProfileName) -> Self {
        Sessions { vault, profile }
    }

    pub fn profile(&self) -> &ProfileName {
        &self.profile
    }

    /// The stored text of `account`'s session, as it is.
    pub fn read(&self, account: &str) -> Result<Option<String>, CliError> {
        self.vault.read(&entry_name(&self.profile, account)?)
    }
}

impl SessionStore for Sessions {
    fn load(&self, account: &str) -> Result<Option<SmartSession>, CliError> {
        match self.read(account)? {
            None => Ok(None),
            // Exit 3 like any sign-in that no longer works: the remedy is
            // a new sign-in, which replaces the item.
            Some(text) => decode_session(&text).map(Some).map_err(|e| {
                let newer = e.remedy() == Some(pk_med_smart::remedy::UPDATE);
                CliError::Auth(e.message().to_string())
                    .because(Reason::CredentialUnreadable)
                    .with_remedy(if newer {
                        UPDATE_REMEDY
                    } else {
                        UNREADABLE_REMEDY
                    })
            }),
        }
    }

    fn save(&self, session: &SmartSession) -> Result<(), CliError> {
        let mut value = serde_json::to_value(session)
            .map_err(|_| CliError::Other("a sign-in did not serialize".into()))?;
        // What is written is this release's shape, and its stamp says so.
        value["format"] = SESSION_FORMAT.into();
        // A renewal's answer without `expires_in` is stored with its hour,
        // as a sign-in's is.
        if session.tokens.expires_at.is_none() {
            value["tokens"]["expires_at"] = crate::session::expiry(&session.tokens).into();
        }
        self.vault.write(
            &entry_name(&self.profile, &session.account())?,
            &value.to_string(),
        )
    }

    fn delete(&self, account: &str) -> Result<bool, CliError> {
        self.vault.delete(&entry_name(&self.profile, account)?)
    }
}

/// An in-memory vault for tests.
#[derive(Default)]
pub struct MemoryVault {
    items: Mutex<HashMap<String, String>>,
}

impl MemoryVault {
    fn items(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.items.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn len(&self) -> usize {
        self.items().len()
    }

    pub fn is_empty(&self) -> bool {
        self.items().is_empty()
    }

    pub fn accounts(&self) -> Vec<String> {
        let mut a: Vec<String> = self.items().keys().cloned().collect();
        a.sort();
        a
    }
}

impl Vault for MemoryVault {
    fn read(&self, account: &str) -> Result<Option<String>, CliError> {
        Ok(self.items().get(account).cloned())
    }

    fn write(&self, account: &str, value: &str) -> Result<(), CliError> {
        self.items().insert(account.into(), value.into());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<bool, CliError> {
        Ok(self.items().remove(account).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adult1() -> ProfileName {
        ProfileName::parse("adult1").unwrap()
    }

    #[test]
    fn an_unreadable_item_is_an_error_not_a_logout() {
        let v: Arc<dyn Vault> = Arc::new(MemoryVault::default());
        let sessions = Sessions::new(v.clone(), adult1());
        assert!(sessions.load("smart/o/p").unwrap().is_none());
        // A mychart 0.1 item: another shape entirely.
        v.write("adult1/o/p", r#"{"version":1,"access_token":"x"}"#)
            .unwrap();
        let e = sessions.load("smart/o/p").unwrap_err();
        assert_eq!(e.exit_code(), 3);
        assert!(e.to_string().contains("sign in again"), "{e}");
        assert!(!e.to_string().contains("\"x\""), "{e}");
        assert!(sessions.delete("smart/o/p").unwrap());
        assert!(!sessions.delete("smart/o/p").unwrap());
    }

    /// An item a newer mychart stored asks for an update, not a sign-in
    /// that would replace it with an older shape.
    #[test]
    fn an_item_a_newer_release_stored_asks_for_an_update() {
        let v: Arc<dyn Vault> = Arc::new(MemoryVault::default());
        let sessions = Sessions::new(v.clone(), adult1());
        let newer = format!(
            r#"{{"format":{},"organization":"o","patient":"p","fhir_base":"https://fhir.example.org/R4","token_endpoint":"https://fhir.example.org/token","client_id":"c","scope":"openid","tokens":{{"access_token":"synthetic","token_type":"Bearer","obtained_at":0}}}}"#,
            SESSION_FORMAT + 1
        );
        v.write("adult1/o/p", &newer).unwrap();
        let e = sessions.load("smart/o/p").unwrap_err();
        assert_eq!(e.exit_code(), 3);
        assert_eq!(e.remedy(), Some(UPDATE_REMEDY));
        assert!(e.to_string().contains("mychart self-update"), "{e}");
        assert!(!e.to_string().contains("synthetic"), "{e}");
    }

    /// The names and the text are pk-med-smart's, so what `profiles
    /// migrate` moves in through `ProfileSessions` reads here, and the
    /// other way round.
    #[test]
    fn a_profile_entry_is_the_one_profile_sessions_names_and_reads() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Arc::new(pk_cli_auth::vault::MemoryKeys::new());
        let shared = Arc::new(pk_cli_auth::vault::Vault::new(keys.clone(), dir.path()));
        let theirs = pk_med_smart::ProfileSessions::new(&shared, adult1());
        assert_eq!(
            entry_name(&adult1(), "smart/o/p").unwrap(),
            theirs.entry_name("smart/o/p").unwrap()
        );
        assert!(entry_name(&adult1(), "o/p").is_err());
        let ours = Sessions::new(shared.clone(), adult1());
        // Nothing stored: no key read, so a signed-out run raises no dialog.
        assert!(ours.load("smart/o/p").unwrap().is_none());
        assert_eq!(keys.reads(), 0);
        let session: SmartSession = serde_json::from_value(serde_json::json!({
            "format": SESSION_FORMAT,
            "organization": "o",
            "patient": "p",
            "fhir_base": "https://fhir.example.org/R4",
            "token_endpoint": "https://fhir.example.org/token",
            "client_id": "c",
            "scope": "openid",
            "tokens": {"access_token": "synthetic", "token_type": "Bearer", "obtained_at": 1}
        }))
        .unwrap();
        ours.save(&session).unwrap();
        let back = theirs
            .load("smart/o/p")
            .unwrap()
            .expect("pk-med-smart reads it");
        assert_eq!(back.tokens.access_token, session.tokens.access_token);
        assert_eq!(theirs.accounts().unwrap(), ["smart/o/p"]);
        assert!(ours.delete("smart/o/p").unwrap());
        assert!(theirs.accounts().unwrap().is_empty());
    }
}
