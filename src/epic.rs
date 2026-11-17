//! What Epic does differently from SMART as written, for pk-med-smart.
//!
//! - Discovery: `/metadata` names the registration endpoint only to a
//!   request carrying `Epic-Client-ID` with the app's client id, so it is
//!   asked first and with that header.
//! - Renewal: a registered client's JWT-bearer grant carries no `scope`.
//!   Epic's documentation sends `grant_type`, `assertion` and `client_id`.
//!
//! docs/api.md has Epic's wording for both.

use pk_med_smart::{SmartSession, VendorQuirks};

/// Epic's quirks. `client_id` is the app's own (the one in the config),
/// needed only for discovery.
#[derive(Debug, Clone, Default)]
pub struct EpicQuirks {
    pub client_id: Option<String>,
}

impl EpicQuirks {
    pub fn for_client(client_id: &str) -> Self {
        EpicQuirks {
            client_id: Some(client_id.to_string()),
        }
    }
}

/// What an organization's authorize endpoint answered this app's client id
/// and redirect, with nobody signing in (`auth probe`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProbeAnswer {
    /// A redirect without `error`: on to MyChart's sign-in.
    Known,
    /// Epic's "OAuth2 Error" page: the organization does not know the
    /// client id.
    Unknown,
    /// A redirect carrying `error`: the client id is known and this request
    /// was refused.
    Refused,
    /// Anything else.
    Unrecognized,
}

/// Epic's answers as seen on 2026-11-16 (docs/api.md): an unknown client id
/// gets a 200 "OAuth2 Error" page, at the sandbox and in production alike; a
/// known one a 302 to MyChart's sign-in; a known one with a `redirect_uri`
/// the app does not list the same 302 with `error=4` in its query. The
/// error is cut to 40 characters of `A-Z a-z 0-9 - . _`, since it comes from
/// the server.
pub fn probe_answer(
    status: u16,
    location: Option<&str>,
    body: &str,
) -> (ProbeAnswer, Option<String>) {
    if (300..400).contains(&status) {
        let base = reqwest::Url::parse("https://authorize.invalid/").expect("a base URL");
        let Some(url) = location.and_then(|l| base.join(l).ok()) else {
            return (ProbeAnswer::Unrecognized, None);
        };
        return match url.query_pairs().find(|(k, _)| k == "error") {
            Some((_, e)) => {
                let e: String = e
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || "-._".contains(*c))
                    .take(40)
                    .collect();
                (ProbeAnswer::Refused, Some(e))
            }
            None => (ProbeAnswer::Known, None),
        };
    }
    if status == 200 && body.contains("OAuth2 Error") {
        return (ProbeAnswer::Unknown, None);
    }
    (ProbeAnswer::Unrecognized, None)
}

impl VendorQuirks for EpicQuirks {
    fn discovery_headers(&self) -> Vec<(String, String)> {
        self.client_id
            .iter()
            .map(|id| ("Epic-Client-ID".to_string(), id.clone()))
            .collect()
    }

    fn metadata_first(&self) -> bool {
        true
    }

    fn refresh_scope(&self, _session: &SmartSession) -> Option<String> {
        None
    }
}

/// The `fhirUser` as `Type/id` ("Patient/abc", "RelatedPerson/x"). Epic may
/// send it as an absolute URL; the type and id are kept. It decides only
/// how a context is labelled.
pub fn fhir_user_reference(raw: &str) -> Option<String> {
    let mut parts = raw.trim_end_matches('/').rsplitn(3, '/');
    let id = parts.next()?;
    let kind = parts.next()?;
    (!id.is_empty() && kind.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
        .then(|| format!("{kind}/{id}"))
}

/// `self` when the signed-in user is the patient, `proxy` when it is someone
/// else (a parent with proxy access), unknown without a `fhirUser`.
pub fn relationship(fhir_user: Option<&str>, patient: &str) -> Option<&'static str> {
    let user = fhir_user?;
    Some(if user == format!("Patient/{patient}") {
        "self"
    } else {
        "proxy"
    })
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_probe_reads_epics_three_answers() {
        use super::{probe_answer, ProbeAnswer::*};
        let unknown = "<html><head><title>OAuth2 Error</title></head><body>\
                       Something went wrong trying to authorize the client.</body></html>";
        assert_eq!(probe_answer(200, None, unknown), (Unknown, None));
        let start = "/mychart-fhir/Authentication/OAuth/Start?response_type=code&client_id=x";
        assert_eq!(probe_answer(302, Some(start), ""), (Known, None));
        assert_eq!(
            probe_answer(302, Some(&format!("{start}&error=4")), ""),
            (Refused, Some("4".into()))
        );
        assert_eq!(
            probe_answer(302, Some("https://pages.example.org/cb?code=c&state=s"), ""),
            (Known, None)
        );
        assert_eq!(
            probe_answer(302, Some("/x?error=%0Abad%20%3Cscript%3E"), ""),
            (Refused, Some("badscript".into()))
        );
        assert_eq!(probe_answer(302, None, ""), (Unrecognized, None));
        assert_eq!(
            probe_answer(200, None, "<title>MyChart</title>"),
            (Unrecognized, None)
        );
        assert_eq!(
            probe_answer(500, None, "OAuth2 Error"),
            (Unrecognized, None)
        );
    }

    use super::*;

    #[test]
    fn discovery_names_the_client_and_renewal_sends_no_scope() {
        let q = EpicQuirks::for_client("app-1");
        assert_eq!(
            q.discovery_headers(),
            vec![("Epic-Client-ID".to_string(), "app-1".to_string())]
        );
        assert!(q.metadata_first());
        assert!(EpicQuirks::default().discovery_headers().is_empty());
    }

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(path).unwrap()
    }

    /// Epic's `/metadata`, as answered to a request with `Epic-Client-ID`,
    /// names the registration endpoint pk-med-smart registers at.
    #[test]
    fn epic_metadata_names_the_three_endpoints() {
        let c = pk_med_smart::SmartConfiguration::from_capability_statement(&fixture(
            "epic_metadata.json",
        ))
        .unwrap();
        assert_eq!(
            c.authorization_endpoint,
            "https://fhir.example.org/oauth2/authorize"
        );
        assert_eq!(c.token_endpoint, "https://fhir.example.org/oauth2/token");
        assert_eq!(
            c.registration_endpoint.as_deref(),
            Some("https://fhir.example.org/oauth2/register")
        );
        assert!(c.supports_s256());
        let well_known =
            pk_med_smart::SmartConfiguration::parse(&fixture("smart_configuration.json")).unwrap();
        assert_eq!(
            well_known.token_endpoint,
            "https://fhir.example.org/oauth2/token"
        );
    }

    #[test]
    fn fhir_user_and_relationship() {
        assert_eq!(
            fhir_user_reference("https://fhir.example.org/api/FHIR/R4/Patient/p1").as_deref(),
            Some("Patient/p1")
        );
        assert_eq!(
            fhir_user_reference("RelatedPerson/r1").as_deref(),
            Some("RelatedPerson/r1")
        );
        assert_eq!(fhir_user_reference("p1"), None);
        assert_eq!(relationship(Some("Patient/p1"), "p1"), Some("self"));
        assert_eq!(relationship(Some("RelatedPerson/r1"), "p1"), Some("proxy"));
        assert_eq!(relationship(None, "p1"), None);
    }
}
