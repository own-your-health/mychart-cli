//! Non-secret settings (`~/.config/mychart/config.json`).
//!
//! The client ids are configuration, never code: each household registers
//! its own app with Epic (docs/registration.md) and sets the ids it was
//! issued. For a public client they are not secrets.

use pk_cli_config::profile::Profiles;
use pk_cli_core::CliError;
use serde::{Deserialize, Serialize};

/// The loopback port the sign-in's redirect comes back to. Epic matches the
/// registered redirect URI, so the port is fixed (and configurable) rather
/// than picked at random per sign-in. The listener is
/// `http://127.0.0.1:<port>/callback` (RFC 8252 §7.3, `pk_cli_auth`), and
/// the redirect too unless `redirect_uri` names an https page that forwards
/// to it.
pub const DEFAULT_REDIRECT_PORT: u16 = 7468;

/// The forwarding page this repository publishes (`site/callback.html`),
/// which sends the answer to `DEFAULT_REDIRECT_PORT` and no other.
pub const PUBLISHED_REDIRECT: &str = "https://piekstra.github.io/mychart-cli/callback";

/// The scopes a sign-in asks for, as Epic's tutorial gives them for a
/// standalone launch: Epic adds the resource scopes of the APIs the app was
/// registered for, and the patient context comes with a patient-facing
/// app. Dynamic registration, not `offline_access`, makes access outlast
/// the hour.
pub const DEFAULT_SCOPE: &str = "openid fhirUser";

pub const KEYS: &str = "client_id, sandbox_client_id, redirect_port, redirect_uri, scope, \
     directory_url, dynamic_registration";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The production client id Epic issued to this household's app.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// The non-production client id, for Epic's sandbox (`--org sandbox`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_port: Option<u16>,
    /// The https page registered as the app's redirect, which passes the
    /// answer on to the listener on `redirect_port`: Epic's production
    /// organizations take only https redirects (docs/api.md). Unset, the
    /// redirect is the listener's own address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Where `orgs refresh` downloads Epic's directory from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory_url: Option<String>,
    /// Register a per-install dynamic client after sign-in, so access lasts
    /// past the first hour (default true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_registration: Option<bool>,
    /// A setting mychart 0.1 had and 0.2 does not use: the redirect is
    /// always on 127.0.0.1. Kept as read so `auth login` can say that a
    /// stored `localhost` no longer applies; `config unset` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_host: Option<String>,
    /// One profile per MyChart account at one organization, each holding
    /// its patient contexts (`session::ProfileSettings`). Names are the
    /// household's own and stay in this file.
    #[serde(default, skip_serializing_if = "Profiles::is_empty")]
    pub profiles: Profiles,
}

impl Config {
    pub fn redirect_port(&self) -> u16 {
        self.redirect_port.unwrap_or(DEFAULT_REDIRECT_PORT)
    }

    pub fn scope(&self) -> &str {
        self.scope.as_deref().unwrap_or(DEFAULT_SCOPE)
    }

    pub fn dynamic_registration(&self) -> bool {
        self.dynamic_registration.unwrap_or(true)
    }

    /// Where the sign-in's answer arrives (RFC 8252 §7.3).
    pub fn listener_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.redirect_port())
    }

    /// The redirect URI every sign-in sends: the forwarding page when one is
    /// set, otherwise the listener.
    pub fn redirect_uri(&self) -> String {
        self.redirect_uri
            .clone()
            .unwrap_or_else(|| self.listener_uri())
    }

    /// Refuse a sign-in the forwarding page could not finish: the published
    /// page forwards to the default port only, so with another
    /// `redirect_port` the answer would go to a port nothing of this sign-in
    /// listens on. A page hosted elsewhere is its owner's to match.
    pub fn check_redirect(&self) -> Result<(), CliError> {
        let port = self.redirect_port();
        if self.redirect_uri.as_deref() == Some(PUBLISHED_REDIRECT) && port != DEFAULT_REDIRECT_PORT
        {
            return Err(CliError::Usage(format!(
                "the page at {PUBLISHED_REDIRECT} forwards the sign-in to port \
                 {DEFAULT_REDIRECT_PORT}, but redirect_port is {port}"
            ))
            .with_remedy(format!(
                "`{bin} config unset redirect_port` to use {DEFAULT_REDIRECT_PORT}, or \
                 `{bin} config set redirect_uri <URI>` with a page of your own that \
                 forwards to {port}",
                bin = crate::commands::BIN
            )));
        }
        Ok(())
    }

    /// At an organization other than the sandbox with no https page set, a
    /// note naming the published one, since Epic production requires https
    /// redirects. It also catches a config.json that mychart 0.3 rewrote,
    /// which drops the key it does not know.
    pub fn missing_page_note(&self, sandbox: bool) -> Option<String> {
        (!sandbox && self.redirect_uri.is_none()).then(|| {
            format!(
                "note: this sign-in's redirect is {}. Epic production organizations \
                 require https redirects; if MyChart shows an OAuth2 error, \
                 `{} config set redirect_uri {PUBLISHED_REDIRECT}`",
                self.redirect_uri(),
                crate::commands::BIN
            )
        })
    }

    /// A `redirect_host` 0.1 stored that names another host than the one
    /// 0.2 sends, so the app's registration may not match.
    pub fn stale_redirect_host(&self) -> Option<&str> {
        self.redirect_host.as_deref().filter(|h| *h != "127.0.0.1")
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), CliError> {
        match key {
            "client_id" => self.client_id = Some(client_id(value)?),
            "sandbox_client_id" => self.sandbox_client_id = Some(client_id(value)?),
            "redirect_port" => {
                let port: u16 = value.parse().ok().filter(|p| *p >= 1024).ok_or_else(|| {
                    CliError::Usage(format!("`{value}` is not a port from 1024 to 65535"))
                })?;
                self.redirect_port = Some(port);
            }
            "redirect_uri" => self.redirect_uri = Some(forwarding_page(value)?),
            "scope" => self.scope = Some(value.trim().to_string()),
            "directory_url" => {
                pk_cli_core::transport::require_secure_url(value)?;
                self.directory_url = Some(value.to_string());
            }
            "redirect_host" => {
                return Err(CliError::Usage(format!(
                    "redirect_host was removed in mychart 0.2: the listener is always {}",
                    self.listener_uri()
                ))
                .with_remedy(
                    "register that URI on the app at fhir.epic.com (docs/registration.md); \
                     `mychart config unset redirect_host` clears a value 0.1 stored",
                ))
            }
            "dynamic_registration" => {
                self.dynamic_registration = Some(
                    value
                        .parse()
                        .map_err(|_| CliError::Usage(format!("`{value}` is not true or false")))?,
                )
            }
            other => return Err(unknown(other)),
        }
        Ok(())
    }

    pub fn unset(&mut self, key: &str) -> Result<(), CliError> {
        match key {
            "client_id" => self.client_id = None,
            "sandbox_client_id" => self.sandbox_client_id = None,
            "redirect_port" => self.redirect_port = None,
            "redirect_uri" => self.redirect_uri = None,
            "scope" => self.scope = None,
            "directory_url" => self.directory_url = None,
            "dynamic_registration" => self.dynamic_registration = None,
            "redirect_host" => self.redirect_host = None,
            other => return Err(unknown(other)),
        }
        Ok(())
    }
}

/// A page that forwards the sign-in's answer: `https`, with a host and no
/// user, no fragment (RFC 6749 §3.1.2), as Epic registers it. pk-cli-auth
/// checks the same at sign-in; checking here too fails `config set` rather
/// than the next sign-in.
fn forwarding_page(value: &str) -> Result<String, CliError> {
    let v = value.trim();
    let authority = v
        .split_once("://")
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("https"))
        .map(|(_, rest)| rest.split(['/', '?', '#', '\\']).next().unwrap_or_default());
    let ok = authority.is_some_and(|a| !a.is_empty() && !a.contains('@'))
        && !v.contains('#')
        && v.len() <= 2000
        && v.bytes().all(|b| b.is_ascii_graphic());
    if !ok {
        return Err(CliError::Usage(
            "redirect_uri is the https page registered on the app that forwards the sign-in \
             to this computer: https, with a host, no user, no fragment and no spaces"
                .into(),
        )
        .with_remedy(format!(
            "`{BIN} config unset redirect_uri` sends the listener's own address instead",
            BIN = crate::commands::BIN
        )));
    }
    Ok(v.to_string())
}

fn unknown(key: &str) -> CliError {
    CliError::Usage(format!("unknown config key `{key}` (known: {KEYS})"))
}

/// A client id is printable ASCII without spaces; anything else is a paste
/// gone wrong.
fn client_id(value: &str) -> Result<String, CliError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 200 || !v.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(CliError::Usage(
            "a client id is the printable string Epic shows on the app's page, with no spaces"
                .into(),
        ));
    }
    Ok(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_set_and_unset() {
        let mut c = Config::default();
        c.set("client_id", " 00000000-0000-0000-0000-000000000001 ")
            .unwrap();
        assert_eq!(
            c.client_id.as_deref(),
            Some("00000000-0000-0000-0000-000000000001")
        );
        c.set("redirect_port", "8123").unwrap();
        assert_eq!(c.redirect_port(), 8123);
        assert!(c.set("redirect_port", "80").is_err());
        assert!(c.set("client_id", "has space").is_err());
        assert!(c.set("directory_url", "http://example.com/x").is_err());
        c.set("directory_url", "http://127.0.0.1:9/brands").unwrap();
        c.set("dynamic_registration", "false").unwrap();
        assert!(!c.dynamic_registration());
        // The redirect host is always 127.0.0.1, and setting it says so.
        let e = c.set("redirect_host", "localhost").unwrap_err();
        assert_eq!(e.exit_code(), 2);
        assert!(
            e.to_string().contains("removed in mychart 0.2")
                && e.to_string().contains("http://127.0.0.1:8123/callback"),
            "{e}"
        );
        // A value 0.1 stored is read only to warn, and unset clears it.
        let mut old: Config =
            serde_json::from_str(r#"{"redirect_host":"localhost","redirect_port":7468}"#).unwrap();
        assert_eq!(old.stale_redirect_host(), Some("localhost"));
        old.unset("redirect_host").unwrap();
        assert_eq!(old.stale_redirect_host(), None);
        let same: Config = serde_json::from_str(r#"{"redirect_host":"127.0.0.1"}"#).unwrap();
        assert_eq!(same.stale_redirect_host(), None);
        c.unset("redirect_port").unwrap();
        assert_eq!(c.redirect_port(), DEFAULT_REDIRECT_PORT);
        assert_eq!(c.set("nope", "x").unwrap_err().exit_code(), 2);
    }

    #[test]
    fn redirect_uri_is_an_https_page_and_falls_back_to_the_listener() {
        let mut c = Config::default();
        assert_eq!(c.redirect_uri(), "http://127.0.0.1:7468/callback");
        c.set("redirect_uri", &format!(" {PUBLISHED_REDIRECT} "))
            .unwrap();
        assert_eq!(c.redirect_uri(), PUBLISHED_REDIRECT);
        assert_eq!(c.listener_uri(), "http://127.0.0.1:7468/callback");
        c.check_redirect().unwrap();
        for bad in [
            "http://pages.example.test/callback",
            "http://127.0.0.1:7468/callback",
            "https:///callback",
            // A user in the authority (a dummy domain, for
            // tests/no_personal_data.rs).
            "https://someone@pages.example.org/callback",
            "https://@pages.example.org/callback",
            "https://pages.example.test/callback#top",
            "https://pages.example.test/call back",
            "pages.example.test/callback",
        ] {
            let e = c.set("redirect_uri", bad).unwrap_err();
            assert_eq!(e.exit_code(), 2, "{bad}");
            assert!(!e.to_string().contains("pages.example"), "{e}");
        }
        assert_eq!(
            c.redirect_uri(),
            PUBLISHED_REDIRECT,
            "a refused value changes nothing"
        );
        // The published page forwards to the default port only.
        c.set("redirect_port", "8123").unwrap();
        let e = c.check_redirect().unwrap_err();
        assert_eq!(e.exit_code(), 2);
        assert!(e.to_string().contains("8123"), "{e}");
        // A page elsewhere is its owner's to match.
        c.set("redirect_uri", "https://pages.example.test/callback")
            .unwrap();
        c.check_redirect().unwrap();
        c.unset("redirect_uri").unwrap();
        assert_eq!(c.redirect_uri(), "http://127.0.0.1:8123/callback");
    }

    #[test]
    fn a_production_sign_in_without_a_page_is_told_the_published_one() {
        let mut c = Config::default();
        let note = c.missing_page_note(false).expect("a note");
        assert!(note.contains("http://127.0.0.1:7468/callback"), "{note}");
        assert!(note.contains(PUBLISHED_REDIRECT), "{note}");
        assert_eq!(c.missing_page_note(true), None, "the sandbox takes http");
        c.set("redirect_uri", PUBLISHED_REDIRECT).unwrap();
        assert_eq!(c.missing_page_note(false), None);
    }
}
