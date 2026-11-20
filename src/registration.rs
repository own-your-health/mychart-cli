//! What to enter in Epic's developer portal to register an app for mychart
//! (`mychart registration`), and whether this install has its client ids.
//! docs/registration.md walks through the same pages; a test keeps its API
//! list equal to [`APIS_KEPT`] and [`APIS_OUTSIDE`].

use serde_json::{json, Value};

use crate::config::{Config, PUBLISHED_REDIRECT};

/// Where the walkthrough is, for a binary installed without the repository.
pub const DOC_URL: &str = "https://github.com/piekstra/mychart-cli/blob/main/docs/registration.md";

/// Epic's developer portal.
pub const PORTAL: &str = "https://fhir.epic.com";

/// The pages published beside the forwarding page.
pub const TERMS_URL: &str = "https://piekstra.github.io/mychart-cli/terms";
pub const PRIVACY_URL: &str = "https://piekstra.github.io/mychart-cli/privacy";
pub const REPO_URL: &str = "https://github.com/piekstra/mychart-cli";

/// The app's Summary (500 characters at most).
pub const SUMMARY: &str = "A command-line tool that downloads a person's own records, and their \
children's through proxy access, from MyChart through Epic's patient-access FHIR API. It only \
reads. Records stay on the person's computer.";

/// One field of one page of the portal, with what to enter.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Field {
    pub field: &'static str,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}

/// One page of the portal, in the order they come.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Page {
    pub page: &'static str,
    pub fields: Vec<Field>,
}

fn f(field: &'static str, value: impl Into<String>) -> Field {
    Field {
        field,
        value: value.into(),
        note: None,
    }
}

fn n(field: &'static str, value: impl Into<String>, note: &'static str) -> Field {
    Field {
        field,
        value: value.into(),
        note: Some(note),
    }
}

/// The redirect URIs to list on the app, one row each: the listener on
/// both loopback names, and the https page production needs.
pub fn redirect_uris(cfg: &Config) -> Vec<String> {
    let port = cfg.redirect_port();
    let page = cfg
        .redirect_uri
        .clone()
        .filter(|u| u.starts_with("https://"))
        .unwrap_or_else(|| PUBLISHED_REDIRECT.to_string());
    vec![
        format!("http://127.0.0.1:{port}/callback"),
        format!("http://localhost:{port}/callback"),
        page,
    ]
}

/// The portal's pages and what to enter on each.
pub fn pages(cfg: &Config) -> Vec<Page> {
    let api_count = APIS_KEPT.len() + APIS_OUTSIDE.len();
    vec![
        Page {
            page: "Sign Up",
            fields: vec![
                n("First Name, Last Name", "Your own", "Sign up as an individual, not a company"),
                f("Company Email", "Your own email address"),
                f("Phone", "Optional"),
                n(
                    "Company Name",
                    "Your own name",
                    "Epic does not say where it appears; assume a patient signing in may see it",
                ),
                f("Website", format!("Optional: {REPO_URL}")),
                f("Country, Address, City, State, Zip Code", "Your own, for Epic"),
                n(
                    "open.epic Terms of Use",
                    "Accept",
                    "Confirm the email Epic sends; if the account shows as awaiting review, wait for Epic's email",
                ),
            ],
        },
        Page {
            page: "Create",
            fields: vec![
                n(
                    "Application Name",
                    "A name you recognise",
                    "MyChart shows it on the consent page",
                ),
                f("Application Audience", "Patients"),
                f("Automatic Client Distribution", "USCDI v3"),
                f("Public Documentation URL", REPO_URL),
                n(
                    "Incoming APIs",
                    format!("The {api_count} APIs `mychart registration --apis` lists"),
                    "Nothing can be added after Save & Ready for Production",
                ),
                f("Outgoing APIs", "None"),
                n(
                    "Endpoint URI",
                    redirect_uris(cfg).join("\n"),
                    "One row each; Epic compares them character for character, and production needs the https row",
                ),
                n(
                    "Can Register Dynamic Clients",
                    "Checked",
                    "Each install registers its own key; mychart keeps no secret",
                ),
                f("How will dynamic clients request an access token?", "JWT Bearer Grant Type"),
                f("Is Confidential Client", "Unchecked"),
            ],
        },
        Page {
            page: "Edit",
            fields: vec![
                f("FHIR version", "R4"),
                n(
                    "SMART scope version",
                    "SMART v1",
                    "mychart asks for `openid fhirUser`; Epic adds the selected APIs' scopes",
                ),
                f("FHIR ID generation", "Use Unconstrained FHIR IDs"),
                f("Summary", SUMMARY),
                n(
                    "Description",
                    "What mychart does, in a paragraph: it downloads the person's own records, \
                     only reads, keeps them on their computer, signs in through a browser window \
                     of its own, and loses access when the person removes it under MyChart's \
                     linked apps or runs `mychart auth logout`",
                    "2000 characters at most",
                ),
                f("Intended Purposes", "Individuals' Access to their EHI"),
                f("Intended Users", "Individual/Caregiver"),
                n(
                    "Terms and Conditions URL",
                    TERMS_URL,
                    "Or a page of your own; it cannot change after Ready for Production",
                ),
                n(
                    "Additional Disclosure Info URL",
                    PRIVACY_URL,
                    "Optional, or a page of your own; it cannot change after Ready for Production",
                ),
                f("Thumbnail, Screenshots", "Leave empty"),
                n(
                    "I accept the terms of use of open.epic.",
                    "Tick before each save",
                    "Then Save & Ready for Sandbox; not Ready for Production until the sandbox works",
                ),
            ],
        },
        Page {
            page: "Data Use Questionnaire",
            fields: vec![
                f("Who develops the app", "An individual developer"),
                f("Business associate agreement", "None"),
                f("Source", "Open source"),
                f("Where user data is stored", "Locally, on the user's device"),
                f("Who else can access it", "No one"),
                f("Whether the complete record is available to the user", "Yes"),
                f("Whether the user can delete all of it", "Yes"),
                f("What is kept after the app is deleted", "Nothing"),
                f("Other uses of the data", "None"),
                f("Other individuals' data", "None"),
                f("How long data is stored", "Indefinitely: until the user deletes the files"),
                n(
                    "Access log",
                    "None",
                    "Answer both tabs; do not choose Lock my answers",
                ),
            ],
        },
    ]
}

/// The commands that store what the portal gives, in order.
pub fn store_commands() -> Vec<(&'static str, String)> {
    vec![
        (
            "After Save & Ready for Sandbox",
            "mychart config set sandbox_client_id <Non-Production Client ID>".into(),
        ),
        (
            "Check the sandbox has it",
            "mychart auth probe --org sandbox".into(),
        ),
        (
            "After Save & Ready for Production",
            "mychart config set client_id <Client ID>".into(),
        ),
        (
            "Production takes only the https redirect",
            format!("mychart config set redirect_uri {PUBLISHED_REDIRECT}"),
        ),
        (
            "Check an organization has it",
            "mychart auth probe --org <ID>".into(),
        ),
    ]
}

/// Whether this install has its client ids, for `info` and `auth status`.
/// Names the config keys, never their values.
pub fn client_status(cfg: &Config) -> Value {
    let production = cfg.client_id.is_some();
    let sandbox = cfg.sandbox_client_id.is_some();
    let mut v = json!({
        "client_id": production,
        "sandbox_client_id": sandbox,
        "redirect_uri": cfg.redirect_uri.is_some(),
    });
    if !production || !sandbox {
        v["remedy"] =
            format!(
            "register an app with Epic: `mychart registration` prints what to enter ({DOC_URL}), \
             then `mychart config set {} <ID>`",
            if production { "sandbox_client_id" } else { "client_id" }
        )
            .into();
    }
    v
}

/// The human line for [`client_status`].
pub fn client_line(cfg: &Config) -> String {
    let state = |set: bool| if set { "configured" } else { "not configured" };
    format!(
        "production {}, sandbox {}",
        state(cfg.client_id.is_some()),
        state(cfg.sandbox_client_id.is_some())
    )
}

/// The Incoming APIs to select: every R4 API on the form whose
/// `autodownload-types` include `USCDIv3` and whose `consumer-types`
/// include `Patient`, as the form names them (docs/registration.md).
pub const APIS_KEPT: &[&str] = &[
    "AllergyIntolerance.Read (Patient Chart) (R4)",
    "AllergyIntolerance.Search (Patient Chart) (R4)",
    "Binary.Read (Clinical Notes) (R4)",
    "Binary.Read (Generated CDAs) (R4)",
    "Binary.Read (Labs) (R4)",
    "Binary.Read (Study) (R4)",
    "Binary.Search (Clinical Notes) (R4)",
    "Binary.Search (Generated CDAs) (R4)",
    "Binary.Search (Labs) (R4)",
    "Binary.Search (Study) (R4)",
    "CarePlan.Read (Encounter) (R4)",
    "CarePlan.Read (Longitudinal) (R4)",
    "CarePlan.Search (Encounter) (R4)",
    "CarePlan.Search (Longitudinal) (R4)",
    "CareTeam.Read (Longitudinal CareTeam) (R4)",
    "CareTeam.Search (Longitudinal CareTeam) (R4)",
    "Condition.Read (Care Plan Problem) (R4)",
    "Condition.Read (Encounter Diagnosis) (R4)",
    "Condition.Read (Health Concerns) (R4)",
    "Condition.Read (Problems) (R4)",
    "Condition.Search (Care Plan Problem) (R4)",
    "Condition.Search (Encounter Diagnosis) (R4)",
    "Condition.Search (Health Concerns) (R4)",
    "Condition.Search (Problems) (R4)",
    "Coverage.Read (Patient Insurance Information) (R4)",
    "Coverage.Search (Patient Insurance Information) (R4)",
    "Device.Read (Implants) (R4)",
    "Device.Search (Implants) (R4)",
    "DiagnosticReport.Read (Results) (R4)",
    "DiagnosticReport.Search (Results) (R4)",
    "DocumentReference.Read (Clinical Notes) (R4)",
    "DocumentReference.Read (Generated CDAs) (R4)",
    "DocumentReference.Read (Labs) (R4)",
    "DocumentReference.Search (Clinical Notes) (R4)",
    "DocumentReference.Search (Generated CDAs) (R4)",
    "DocumentReference.Search (Labs) (R4)",
    "Encounter.Read (Patient Chart) (R4)",
    "Encounter.Search (Patient Chart) (R4)",
    "ExplanationOfBenefit.Read (Claim) (R4)",
    "ExplanationOfBenefit.Search (Claim) (R4)",
    "Goal.Read (Care Plan Goal) (R4)",
    "Goal.Read (Patient) (R4)",
    "Goal.Search (Care Plan Goal) (R4)",
    "Goal.Search (Patient) (R4)",
    "Immunization.Read (Patient Chart) (R4)",
    "Immunization.Search (Patient Chart) (R4)",
    "Location.Read (Organizational Directory) (R4)",
    "Location.Search (Organizational Directory) (R4)",
    "Media.Read (Study) (R4)",
    "Media.Search (Study) (R4)",
    "Medication.Read (Organization Med List) (R4)",
    "Medication.Search (Organization Med List) (R4)",
    "MedicationDispense.Read (Fill Status) (R4)",
    "MedicationDispense.Search (Fill Status) (R4)",
    "MedicationRequest.Read (Signed Medication Order) (R4)",
    "MedicationRequest.Search (Signed Medication Order) (R4)",
    "Observation.Read (Assessments) (R4)",
    "Observation.Read (Labs) (R4)",
    "Observation.Read (SDOH Assessments) (R4)",
    "Observation.Read (SmartData Elements) (R4)",
    "Observation.Read (Social History) (R4)",
    "Observation.Read (Study Finding) (R4)",
    "Observation.Read (Vital Signs) (R4)",
    "Observation.Search (Assessments) (R4)",
    "Observation.Search (Labs) (R4)",
    "Observation.Search (SDOH Assessments) (R4)",
    "Observation.Search (SmartData Elements) (R4)",
    "Observation.Search (Social History) (R4)",
    "Observation.Search (Study Finding) (R4)",
    "Observation.Search (Vital Signs) (R4)",
    "Organization.Read (Organizational Directory) (R4)",
    "Organization.Search (Organizational Directory) (R4)",
    "Patient.Read (Demographics) (R4)",
    "Patient.Search (Demographics) (R4)",
    "Practitioner.Read (Organizational Directory) (R4)",
    "Practitioner.Search (Organizational Directory) (R4)",
    "PractitionerRole.Read (Organizational Directory) (R4)",
    "PractitionerRole.Search (Organizational Directory) (R4)",
    "Procedure.Read (Orders) (R4)",
    "Procedure.Read (SDOH Intervention) (R4)",
    "Procedure.Read (Surgeries) (R4)",
    "Procedure.Search (Orders) (R4)",
    "Procedure.Search (SDOH Intervention) (R4)",
    "Procedure.Search (Surgeries) (R4)",
    "Provenance.Read (R4)",
    "RelatedPerson.Read (Friends and Family) (R4)",
    "RelatedPerson.Read (Proxy) (R4)",
    "RelatedPerson.Search (Friends and Family) (R4)",
    "RelatedPerson.Search (Proxy) (R4)",
    "ServiceRequest.Read (Community Resource ServiceRequest) (R4)",
    "ServiceRequest.Read (Orders) (R4)",
    "ServiceRequest.Search (Community Resource ServiceRequest) (R4)",
    "ServiceRequest.Search (Orders) (R4)",
    "Specimen.Read (Patient Chart) (R4)",
    "Specimen.Search (Patient Chart) (R4)",
];

pub const APIS_OUTSIDE: &[&str] = &[
    "AllergyIntolerance.Read (Outside Record) (R4)",
    "AllergyIntolerance.Search (Outside Record) (R4)",
    "Binary.Read (Outside Record - Clinical Notes) (R4)",
    "Binary.Search (Outside Record - Clinical Notes) (R4)",
    "CarePlan.Read (Outside Record) (R4)",
    "CarePlan.Search (Outside Record) (R4)",
    "CareTeam.Read (Outside Record) (R4)",
    "CareTeam.Search (Outside Record) (R4)",
    "Condition.Read (Outside Record Encounter Diagnosis) (R4)",
    "Condition.Read (Outside Record Health Concern) (R4)",
    "Condition.Read (Outside Record Problems) (R4)",
    "Condition.Search (Outside Record Encounter Diagnosis) (R4)",
    "Condition.Search (Outside Record Health Concern) (R4)",
    "Condition.Search (Outside Record Problems) (R4)",
    "Coverage.Read (Outside Record) (R4)",
    "Coverage.Search (Outside Record) (R4)",
    "Device.Read (Outside Record) (R4)",
    "Device.Search (Outside Record) (R4)",
    "DiagnosticReport.Read (Outside Record Results) (R4)",
    "DiagnosticReport.Search (Outside Record Results) (R4)",
    "DocumentReference.Read (Outside Record - Clinical Notes) (R4)",
    "DocumentReference.Search (Outside Record - Clinical Notes) (R4)",
    "Encounter.Read (Outside Record) (R4)",
    "Encounter.Search (Outside Record) (R4)",
    "Goal.Read (Outside Record) (R4)",
    "Goal.Search (Outside Record) (R4)",
    "Immunization.Read (Outside Record) (R4)",
    "Immunization.Search (Outside Record) (R4)",
    "Location.Read (Outside Record) (R4)",
    "Location.Search (Outside Record) (R4)",
    "Medication.Read (Outside Record) (R4)",
    "Medication.Search (Outside Record) (R4)",
    "MedicationDispense.Read (Outside Record) (R4)",
    "MedicationDispense.Search (Outside Record) (R4)",
    "MedicationRequest.Read (Outside Record) (R4)",
    "MedicationRequest.Search (Outside Record) (R4)",
    "Observation.Read (Outside Record Activities of Daily Living) (R4)",
    "Observation.Read (Outside Record Occupation) (R4)",
    "Observation.Read (Outside Record Pregnancy Status) (R4)",
    "Observation.Read (Outside Record Results) (R4)",
    "Observation.Read (Outside Record Screening Assessment) (R4)",
    "Observation.Read (Outside Record SDOH Assessment) (R4)",
    "Observation.Read (Outside Record Sexual Orientation) (R4)",
    "Observation.Read (Outside Record Smoking Status) (R4)",
    "Observation.Read (Outside Record Vital Signs) (R4)",
    "Observation.Search (Outside Record Activities of Daily Living) (R4)",
    "Observation.Search (Outside Record Occupation) (R4)",
    "Observation.Search (Outside Record Pregnancy Status) (R4)",
    "Observation.Search (Outside Record Results) (R4)",
    "Observation.Search (Outside Record Screening Assessment) (R4)",
    "Observation.Search (Outside Record SDOH Assessment) (R4)",
    "Observation.Search (Outside Record Sexual Orientation) (R4)",
    "Observation.Search (Outside Record Smoking Status) (R4)",
    "Observation.Search (Outside Record Vital Signs) (R4)",
    "Organization.Read (Outside Record) (R4)",
    "Organization.Search (Outside Record) (R4)",
    "Practitioner.Read (Outside Record) (R4)",
    "Practitioner.Search (Outside Record) (R4)",
    "PractitionerRole.Read (Outside Record) (R4)",
    "PractitionerRole.Search (Outside Record) (R4)",
    "Procedure.Read (Outside Record) (R4)",
    "Procedure.Search (Outside Record) (R4)",
    "QuestionnaireResponse.Read (Outside Record) (R4)",
    "QuestionnaireResponse.Search (Outside Record) (R4)",
    "RelatedPerson.Read (Outside Record) (R4)",
    "RelatedPerson.Search (Outside Record) (R4)",
    "ServiceRequest.Read (Outside Record) (R4)",
    "ServiceRequest.Search (Outside Record) (R4)",
    "Specimen.Read (Outside Record) (R4)",
    "Specimen.Search (Outside Record) (R4)",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The API names under one `###` heading of docs/registration.md.
    fn doc_group(doc: &str, heading: &str) -> Vec<String> {
        let start = doc.find(heading).expect(heading);
        let rest = &doc[start + heading.len()..];
        let end = rest.find("\n### ").unwrap_or(rest.len());
        rest[..end]
            .lines()
            .filter_map(|l| l.strip_prefix("- "))
            .filter(|l| l.ends_with(" (R4)"))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_api_list_is_the_docs() {
        let doc = include_str!("../docs/registration.md");
        assert_eq!(
            doc_group(doc, "### Records the organization keeps"),
            APIS_KEPT
        );
        assert_eq!(
            doc_group(doc, "### Copies other organizations sent"),
            APIS_OUTSIDE
        );
    }

    #[test]
    fn every_field_is_in_the_docs() {
        let doc = include_str!("../docs/registration.md");
        for page in pages(&Config::default()) {
            for field in page.fields {
                let name = field.field.split(", ").next().unwrap();
                assert!(
                    doc.contains(name),
                    "docs/registration.md lacks {:?}",
                    field.field
                );
            }
        }
        assert!(SUMMARY.len() <= 500);
    }

    #[test]
    fn the_redirects_follow_the_port_and_keep_an_https_page() {
        let mut cfg = Config::default();
        assert_eq!(
            redirect_uris(&cfg),
            [
                "http://127.0.0.1:7468/callback",
                "http://localhost:7468/callback",
                PUBLISHED_REDIRECT
            ]
        );
        cfg.redirect_port = Some(8123);
        cfg.redirect_uri = Some("http://127.0.0.1:8123/callback".into());
        assert_eq!(redirect_uris(&cfg)[1], "http://localhost:8123/callback");
        assert_eq!(redirect_uris(&cfg)[2], PUBLISHED_REDIRECT);
    }

    #[test]
    fn client_status_names_keys_never_values() {
        let mut cfg = Config::default();
        let v = client_status(&cfg);
        assert_eq!(v["client_id"], false);
        assert!(v["remedy"]
            .as_str()
            .unwrap()
            .contains("config set client_id"));
        cfg.client_id = Some("not-a-real-id".into());
        cfg.sandbox_client_id = Some("not-a-real-sandbox-id".into());
        let v = client_status(&cfg);
        assert_eq!(v["client_id"], true);
        assert!(v.get("remedy").is_none());
        assert!(!v.to_string().contains("not-a-real"));
    }
}
