//! What to enter in Epic's developer portal to register an app for mychart
//! (`mychart registration`, pk-med-smart's `registration/v1`), and whether
//! this install has its client ids. docs/registration.md walks through the
//! same pages; tests keep its API list equal to [`APIS_KEPT`] and
//! [`APIS_OUTSIDE`] and every label here in it.

use pk_cli_core::CliError;
use pk_med_smart::registration::FormField;
use pk_med_smart::{
    missing_client, with_client, ClientStatus, ClientType, Registration, ScopeSet, StoredIn,
};

use crate::config::{Config, PUBLISHED_REDIRECT};

const BIN: &str = "mychart";

/// The walkthrough, by its path in the repository.
pub const DOC: &str = "docs/registration.md";

/// The walkthrough, for a binary installed without the repository.
pub const DOC_URL: &str = "https://github.com/piekstra/mychart-cli/blob/main/docs/registration.md";

/// Epic's developer portal.
pub const PORTAL: &str = "https://fhir.epic.com";

/// Who registers the app.
pub const VENDOR: &str = "Epic on FHIR";

/// The pages published beside the forwarding page.
pub const TERMS_URL: &str = "https://piekstra.github.io/mychart-cli/terms";
pub const PRIVACY_URL: &str = "https://piekstra.github.io/mychart-cli/privacy";
pub const REPO_URL: &str = "https://github.com/piekstra/mychart-cli";

/// The app's Summary (500 characters at most).
pub const SUMMARY: &str = "A command-line tool that downloads a person's own records, and their \
children's through proxy access, from MyChart through Epic's patient-access FHIR API. It only \
reads. Records stay on the person's computer.";

/// The two client ids an app has: one for Epic's sandbox, one for every
/// production organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Production,
    Sandbox,
}

impl Which {
    /// The registration's name in `registration/v1` errors and `client`.
    pub fn name(self) -> &'static str {
        match self {
            Which::Production => "production",
            Which::Sandbox => "sandbox",
        }
    }

    /// The config key that holds its client id.
    pub fn key(self) -> &'static str {
        match self {
            Which::Production => "client_id",
            Which::Sandbox => "sandbox_client_id",
        }
    }

    /// The command that stores it.
    pub fn store(self) -> String {
        format!("{BIN} config set {} <CLIENT_ID>", self.key())
    }

    fn configured(self, cfg: &Config) -> bool {
        match self {
            Which::Production => cfg.client_id.is_some(),
            Which::Sandbox => cfg.sandbox_client_id.is_some(),
        }
    }
}

/// The redirect URIs to list on the app, in the order mychart sends them:
/// the listener (every sign-in until `redirect_uri` is set), then the https
/// page production needs.
pub fn redirect_uris(cfg: &Config) -> Vec<String> {
    let page = cfg
        .redirect_uri
        .clone()
        .filter(|u| u.starts_with("https://"))
        .unwrap_or_else(|| PUBLISHED_REDIRECT.to_string());
    vec![
        format!("http://127.0.0.1:{}/callback", cfg.redirect_port()),
        page,
    ]
}

fn url(label: &str, value: &str, note: &str) -> FormField {
    FormField {
        label: label.into(),
        value: value.into(),
        note: Some(note.into()),
    }
}

/// What to enter in Epic's portal for this install's port and scope. The
/// fields come from the app's Create page, its Edit page and its Data Use
/// Questionnaire, in that order.
pub fn registration(cfg: &Config) -> Registration {
    let api_count = APIS_KEPT.len() + APIS_OUTSIDE.len();
    let mut reg = Registration::new(
        BIN,
        VENDOR,
        PORTAL,
        DOC,
        ClientType::Public,
        format!(
            "{} && {}",
            Which::Sandbox
                .store()
                .replace("<CLIENT_ID>", "<NON_PRODUCTION_CLIENT_ID>"),
            Which::Production.store()
        ),
    )
    .scopes(&ScopeSet::parse(cfg.scope()));
    for uri in redirect_uris(cfg) {
        reg = reg.redirect_uri(uri);
    }
    reg = reg
        .field_with_note(
            "Application Name",
            "A name you recognise",
            "MyChart shows it on the consent page",
        )
        .field("Application Audience", "Patients")
        .field("Automatic Client Distribution", "USCDI v3")
        .field_with_note(
            "Incoming APIs",
            format!("The {api_count} APIs `{BIN} registration --apis` lists"),
            "nothing can be added after Save & Ready for Production",
        )
        .field("Outgoing APIs", "None")
        .field_with_note(
            "Endpoint URI",
            "One row per redirect URI above",
            "choose the scheme from the row's list, then type the rest without it",
        )
        .field_with_note(
            "Can Register Dynamic Clients",
            "Checked",
            "each install registers a key of its own; mychart keeps no secret",
        )
        .field(
            "How will dynamic clients request an access token?",
            "JWT Bearer Grant Type",
        )
        .field("Is Confidential Client", "Unchecked")
        .field("FHIR version", "R4")
        .field_with_note(
            "SMART scope version",
            "SMART v1",
            "Epic adds the selected APIs' scopes to the ones above",
        )
        .field("FHIR ID generation", "Use Unconstrained FHIR IDs")
        .field("Summary", SUMMARY)
        .field_with_note(
            "Description",
            "What mychart does, in a paragraph: it downloads the person's own records, only \
             reads, keeps them on their computer, signs in through a browser window of its own, \
             and loses access when the person removes it under MyChart's linked apps or runs \
             `mychart auth logout`",
            "2000 characters at most",
        )
        .field("Intended Purposes", "Individuals' Access to their EHI")
        .field("Intended Users", "Individual/Caregiver")
        .field("Thumbnail, Screenshots", "Leave empty")
        .field_with_note(
            "I accept the terms of use of open.epic.",
            "Tick before each save",
            "then Save & Ready for Sandbox; Ready for Production once the sandbox works",
        )
        .field("Who develops the app", "An individual developer")
        .field("Business associate agreement", "None")
        .field("Source", "Open source")
        .field("Where user data is stored", "Locally, on the user's device")
        .field("Who else can access it", "No one")
        .field(
            "Whether the complete record is available to the user",
            "Yes",
        )
        .field("Whether the user can delete all of it", "Yes")
        .field("What is kept after the app is deleted", "Nothing")
        .field("Other uses of the data", "None")
        .field("Other individuals' data", "None")
        .field(
            "How long data is stored",
            "Indefinitely: until the user deletes the files",
        )
        .field_with_note(
            "Access log",
            "None",
            "answer both tabs; do not choose Lock my answers",
        )
        .note(format!(
            "The guide, for an install without the repository: {DOC_URL}."
        ))
        .note(
            "Sign up first as an individual, not a company: your own name, email and address, \
             Company Name your own name (a patient signing in may see it), then confirm the email \
             Epic sends.",
        )
        .note(
            "The fields are on the app's Create page, then its Edit page after the first Save, \
             then the Data Use Questionnaire; save the Edit page before opening the \
             questionnaire, which drops unsaved edits.",
        )
        .note(format!(
            "Production takes only https redirects, and Save & Ready for Production stays \
             disabled without the https row; then run `{BIN} config set redirect_uri \
             {PUBLISHED_REDIRECT}`."
        ))
        .note(
            "After Ready for Production only redirect URIs and JWK Set URLs can change, and the \
             app cannot be deactivated, so test in the sandbox first.",
        )
        .note(format!(
            "Epic's sandbox knew a new app about 3 hours after Save & Ready for Sandbox, and \
             production organizations within 12 hours, most within one; `{BIN} auth probe --org \
             <ID>` says when."
        ));
    reg.urls = vec![
        url("Public Documentation URL", REPO_URL, "on the Create page"),
        url(
            "Terms and Conditions URL",
            TERMS_URL,
            "or a page of your own; it cannot change after Ready for Production",
        ),
        url(
            "Additional Disclosure Info URL",
            PRIVACY_URL,
            "optional, or a page of your own; it cannot change after Ready for Production",
        ),
    ];
    reg
}

/// The error a sign-in or probe gives when `which` has no client id: exit 2,
/// reason `no-client`, naming the guide, `mychart registration` and the one
/// command that stores it.
pub fn missing(cfg: &Config, which: Which) -> CliError {
    let mut reg = registration(cfg);
    reg.store = which.store();
    missing_client(&reg, Some(which.name()))
}

/// Whether `which` has a client id, for `info` and `auth status`. The id
/// stays out: Epic's guidelines ask that it be kept to the app.
pub fn client_status(cfg: &Config, which: Which) -> ClientStatus {
    let status = if which.configured(cfg) {
        ClientStatus::configured(ClientType::Public, StoredIn::Config)
    } else {
        ClientStatus::none()
    };
    status.registration(which.name())
}

/// `dto` (`cli-info/v1`, `auth-status/v1`) with `client`, production's
/// [`client_status`], and `sandbox_client`, the sandbox's.
pub fn with_clients<T: serde::Serialize>(
    dto: &T,
    cfg: &Config,
) -> Result<serde_json::Value, CliError> {
    let mut v = with_client(dto, &client_status(cfg, Which::Production))?;
    v["sandbox_client"] = serde_json::to_value(client_status(cfg, Which::Sandbox))
        .map_err(|e| CliError::Other(format!("serializing the client: {e}")))?;
    Ok(v)
}

/// The human lines for [`client_status`]: production's always, and the
/// sandbox's when it is configured, since it is optional.
pub fn client_lines(cfg: &Config) -> Vec<String> {
    let reg = registration(cfg);
    let mut lines = vec![client_status(cfg, Which::Production).text(&reg)];
    if Which::Sandbox.configured(cfg) {
        lines.push(client_status(cfg, Which::Sandbox).text(&reg));
    }
    if Which::Production.configured(cfg) && cfg.redirect_uri.is_none() {
        lines.push(format!(
            "production takes only an https redirect: `{BIN} config set redirect_uri {PUBLISHED_REDIRECT}`"
        ));
    }
    lines
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
        let reg = registration(&Config::default());
        for field in reg.urls.iter().chain(&reg.fields) {
            let name = field.label.split(", ").next().unwrap();
            assert!(
                doc.contains(name),
                "docs/registration.md lacks {:?}",
                field.label
            );
        }
        for uri in &reg.redirect_uris {
            assert!(
                doc.contains(uri.as_str()),
                "docs/registration.md lacks {uri}"
            );
        }
        assert!(SUMMARY.len() <= 500);
        let v = serde_json::to_value(&reg).unwrap();
        assert_eq!(v["schema"], "registration/v1");
        assert_eq!(v["client_type"], "public");
        assert_eq!(v["scopes"], serde_json::json!(["openid", "fhirUser"]));
    }

    #[test]
    fn the_redirects_follow_the_port_and_keep_an_https_page() {
        let mut cfg = Config::default();
        assert_eq!(
            redirect_uris(&cfg),
            ["http://127.0.0.1:7468/callback", PUBLISHED_REDIRECT]
        );
        cfg.redirect_port = Some(8123);
        cfg.redirect_uri = Some("http://127.0.0.1:8123/callback".into());
        assert_eq!(
            redirect_uris(&cfg),
            ["http://127.0.0.1:8123/callback", PUBLISHED_REDIRECT]
        );
    }

    #[test]
    fn a_missing_client_names_its_own_store_command() {
        let cfg = Config::default();
        let e = missing(&cfg, Which::Sandbox);
        assert_eq!(e.exit_code(), 2);
        assert_eq!(e.reason(), Some(pk_med_smart::registration::NO_CLIENT));
        let remedy = e.remedy().unwrap();
        assert!(remedy.contains(DOC), "{remedy}");
        assert!(remedy.contains("mychart registration"), "{remedy}");
        assert!(
            remedy.contains("config set sandbox_client_id <CLIENT_ID>"),
            "{remedy}"
        );
        assert!(!remedy.contains("config set client_id"), "{remedy}");
        assert!(e.message().contains("sandbox"), "{}", e.message());
    }

    #[test]
    fn client_status_names_registrations_never_ids() {
        let mut cfg = Config::default();
        let v = serde_json::to_value(client_status(&cfg, Which::Production)).unwrap();
        assert_eq!(v["configured"], false);
        assert_eq!(v["registration"], "production");
        assert_eq!(client_lines(&cfg).len(), 1);
        cfg.client_id = Some("not-a-real-id".into());
        cfg.sandbox_client_id = Some("not-a-real-sandbox-id".into());
        let v = serde_json::to_value(client_status(&cfg, Which::Sandbox)).unwrap();
        assert_eq!(v["configured"], true);
        assert_eq!(v["stored_in"], "config");
        assert!(!v.to_string().contains("not-a-real"));
        let lines = client_lines(&cfg);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines.iter().all(|l| !l.contains("not-a-real")));
        cfg.redirect_uri = Some(PUBLISHED_REDIRECT.into());
        assert_eq!(client_lines(&cfg).len(), 2);
    }
}
