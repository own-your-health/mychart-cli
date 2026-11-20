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

/// Whether an authorize endpoint is Kaiser Permanente's (`fhir.kp.org`).
pub fn is_kaiser(authorization_endpoint: &str) -> bool {
    reqwest::Url::parse(authorization_endpoint)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .as_deref()
        == Some("fhir.kp.org")
}

/// The host of Kaiser Permanente's MyChart pages.
const KP_PAGES: &str = "healthy.kaiserpermanente.org";

/// The path, after the region, of the page Kaiser Permanente's authorize
/// endpoint sends the browser to.
const KP_START: &str = "/Authentication/OAuth/Start";

/// Kaiser Permanente's front end answers 500 ("Page Not Found") at
/// `https://healthy.kaiserpermanente.org/<region>/Authentication/OAuth/Start`,
/// where `fhir.kp.org` sends every region but Washington, with or without a
/// kp.org session; the same path in lowercase reaches Epic's MyChart and the
/// sign-in completes (docs/api.md, Kaiser Permanente). For a redirect to
/// that page, the lowercase address with the same query, and the page as it
/// is without its query, to check it still fails. `None` for any other
/// address.
pub fn kaiser_start(location: &str) -> Option<(String, String)> {
    let url = reqwest::Url::parse(location).ok()?;
    if url.scheme() != "https" || url.host_str()? != KP_PAGES {
        return None;
    }
    let region = url.path().strip_suffix(KP_START)?.strip_prefix('/')?;
    if region.is_empty() || region.contains('/') {
        return None;
    }
    let mut lower = url.clone();
    lower.set_path(&url.path().to_ascii_lowercase());
    let mut page = url.clone();
    page.set_query(None);
    page.set_fragment(None);
    Some((lower.to_string(), page.to_string()))
}

/// The address a sign-in's window opens: `authorize`, the authorize
/// address, except at Kaiser Permanente's organizations while their Start
/// page fails ([`kaiser_start`]), where it is that page in lowercase. `get`
/// sends one GET without following its redirect and answers its status and
/// `Location`, or `None` when the request failed: one for `authorize`, as
/// `auth probe` sends it (no `code` is issued without a session), and one
/// for the Start page without its query. The page counts as fixed only when
/// it answers below 400, so a refusal such as 403 or 429 is not mistaken
/// for a fix. Any other answer, and any failure before the Start page is
/// known, opens `authorize` as it is.
pub fn address_to_open(
    authorization_endpoint: &str,
    authorize: &str,
    get: impl Fn(&str) -> Option<(u16, Option<String>)>,
) -> String {
    if !is_kaiser(authorization_endpoint) {
        return authorize.to_string();
    }
    let Some((300..=399, Some(location))) = get(authorize) else {
        return authorize.to_string();
    };
    let location = match reqwest::Url::parse(authorize).and_then(|a| a.join(&location)) {
        Ok(u) => u.to_string(),
        Err(_) => return authorize.to_string(),
    };
    let Some((lower, page)) = kaiser_start(&location) else {
        return authorize.to_string();
    };
    match get(&page) {
        // Fixed: the page answers as it is.
        Some((status, _)) if status < 400 => authorize.to_string(),
        _ => lower,
    }
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

    const AUTHORIZE: &str =
        "https://fhir.kp.org/KPPolarisPortal/esb-envlbl/190/oauth2/authorize?state=s&aud=a";
    const START: &str =
        "https://healthy.kaiserpermanente.org:443/fhirxx/Authentication/OAuth/Start?state=s&x=Y";

    #[test]
    fn kaisers_start_page_opens_in_lowercase_with_its_query() {
        let (lower, page) = kaiser_start(START).unwrap();
        assert_eq!(
            lower,
            "https://healthy.kaiserpermanente.org/fhirxx/authentication/oauth/start?state=s&x=Y"
        );
        assert_eq!(
            page,
            "https://healthy.kaiserpermanente.org/fhirxx/Authentication/OAuth/Start"
        );
        for other in [
            "https://healthy.kaiserpermanente.org/fhirxx/authentication/oauth/start?state=s",
            "https://healthy.kaiserpermanente.org/Authentication/OAuth/Start?state=s",
            "https://healthy.kaiserpermanente.org/a/b/Authentication/OAuth/Start",
            "http://healthy.kaiserpermanente.org/fhirxx/Authentication/OAuth/Start",
            "https://mychart.example.org/fhirxx/Authentication/OAuth/Start",
            "https://pages.example.org/cb?code=c&state=s",
        ] {
            assert_eq!(kaiser_start(other), None, "{other}");
        }
    }

    #[test]
    fn only_a_failing_kaiser_start_page_changes_the_address() {
        use std::cell::RefCell;
        let asked = RefCell::new(Vec::new());
        let answers = |start: Option<(u16, Option<String>)>,
                       page: Option<(u16, Option<String>)>| {
            asked.borrow_mut().clear();
            let get = |u: &str| {
                asked.borrow_mut().push(u.to_string());
                if u == AUTHORIZE {
                    start.clone()
                } else {
                    page.clone()
                }
            };
            address_to_open(
                "https://fhir.kp.org/KPPolarisPortal/esb-envlbl/190/oauth2/authorize",
                AUTHORIZE,
                get,
            )
        };
        let lower = kaiser_start(START).unwrap().0;
        assert_eq!(
            answers(Some((302, Some(START.into()))), Some((500, None))),
            lower
        );
        assert_eq!(asked.borrow().len(), 2);
        // The page cannot be checked: lowercase still works.
        assert_eq!(answers(Some((302, Some(START.into()))), None), lower);
        // Kaiser fixed the page.
        for fixed in [200, 302] {
            assert_eq!(
                answers(Some((302, Some(START.into()))), Some((fixed, None))),
                AUTHORIZE
            );
        }
        // A refusal is not a fix.
        for refused in [403, 429] {
            assert_eq!(
                answers(Some((302, Some(START.into()))), Some((refused, None))),
                lower
            );
        }
        // Washington's own MyChart, an error page, no answer.
        let other = "https://mychart.example.org/MyChart/Authentication/OAuth/Start?state=s";
        assert_eq!(answers(Some((302, Some(other.into()))), None), AUTHORIZE);
        assert_eq!(answers(Some((200, None)), None), AUTHORIZE);
        assert_eq!(answers(None, None), AUTHORIZE);
        // A relative redirect is taken from the authorize address's host.
        assert_eq!(
            answers(
                Some((
                    302,
                    Some("/fhirxx/Authentication/OAuth/Start?state=s".into())
                )),
                None
            ),
            AUTHORIZE
        );
        // Not Kaiser: no request at all.
        asked.borrow_mut().clear();
        let none = address_to_open(
            "https://fhir.example.org/oauth2/authorize",
            "https://fhir.example.org/oauth2/authorize?state=s",
            |_| panic!("asked another organization"),
        );
        assert_eq!(none, "https://fhir.example.org/oauth2/authorize?state=s");
    }

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
