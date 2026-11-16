//! What `records export` asks an Epic organization for, in order: the
//! patient, every search mychart's app registers (Epic splits several
//! resources by category), each `DocumentReference`'s and
//! `DiagnosticReport`'s `Binary` right after it, then the medications,
//! practitioners, organizations, locations, specimens and media the records
//! point at, each `Media`'s `Binary` right after it.
//!
//! A refused search, an unreadable answer or a file on another server is a
//! skip in the trailer, never the end of the export; only a sign-in that
//! stops working, a network that does, or a stream that cannot be written
//! ends it.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use pk_cli_core::CliError;
use serde_json::Value;

use crate::fhir::{Failure, Fhir, RawResource, Wire};
use pk_med_fhir::ReferenceTarget;
use pk_med_records::stream::skip::{FORBIDDEN, INVALID_REFERENCE};
use pk_med_records::{RecordsWriter, Skip};

/// The searches, as (resource type, extra parameters, whether `--since`
/// narrows it by `date`). The Observation categories are those of the
/// Observation APIs the app registers (docs/registration.md); Epic answers
/// a category of an API a client lacks (`LDA`, `periodontal`, …) with 400,
/// "not valid for any authorized sub-resource". A category an organization
/// does not serve also answers 400, an `unsupported` skip.
pub type SearchPlan = (&'static str, &'static [(&'static str, &'static str)], bool);

pub const SEARCHES: &[SearchPlan] = &[
    ("AllergyIntolerance", &[], false),
    ("Condition", &[("category", "problem-list-item")], false),
    ("Condition", &[("category", "encounter-diagnosis")], false),
    ("Condition", &[("category", "health-concern")], false),
    ("Observation", &[("category", "laboratory")], true),
    ("Observation", &[("category", "vital-signs")], true),
    ("Observation", &[("category", "social-history")], true),
    ("Observation", &[("category", "survey")], true),
    ("Observation", &[("category", "sdoh")], true),
    ("Observation", &[("category", "imaging")], true),
    (
        "Observation",
        &[("category", "care-experience-preference")],
        true,
    ),
    ("DiagnosticReport", &[], true),
    ("DocumentReference", &[("category", "clinical-note")], true),
    ("DocumentReference", &[], true),
    ("Encounter", &[], true),
    ("Immunization", &[], true),
    ("MedicationRequest", &[], false),
    ("MedicationDispense", &[], false),
    ("Procedure", &[], true),
    // Epic's Procedure categories include SDOH intervention.
    ("Procedure", &[("category", "410606002")], true), // SNOMED CT SDOH intervention
    // CarePlan requires a category: longitudinal, encounter-level, and US
    // Core's assess-plan.
    ("CarePlan", &[("category", "38717003")], false), // SNOMED CT longitudinal
    ("CarePlan", &[("category", "734163000")], false), // SNOMED CT encounter-level
    ("CarePlan", &[("category", "assess-plan")], false),
    ("CareTeam", &[], false),
    ("Goal", &[], false),
    ("Coverage", &[], false),
    // Claims; on the form's USCDI v3 list (docs/registration.md).
    ("ExplanationOfBenefit", &[], false),
    // Questionnaire answers other organizations sent (Outside Record).
    ("QuestionnaireResponse", &[], false),
    ("Device", &[], false),
    ("ServiceRequest", &[], false),
    ("RelatedPerson", &[], false),
];

/// Types read by reference once the searches are done. Epic's Specimen and
/// Media searches take only `_id` ("supports only the _id search
/// parameter"), so they are read from what names them: `Observation.specimen`
/// and `ServiceRequest.specimen`, `DiagnosticReport.media`.
pub const REFERENCED: &[&str] = &[
    "Medication",
    "Practitioner",
    "PractitionerRole",
    "Organization",
    "Location",
    "Specimen",
    "Media",
];

/// Types whose attachments are fetched as their `Binary`, right after them.
fn has_files(resource_type: &str) -> bool {
    matches!(
        resource_type,
        "DocumentReference" | "DiagnosticReport" | "Media"
    )
}

/// Reads by reference one export makes at most.
pub const REFERENCE_CAP: usize = 2000;

/// The query for one search.
pub fn params(
    patient: &str,
    extra: &[(&str, &str)],
    dated: bool,
    since: Option<&str>,
) -> Vec<(String, String)> {
    let mut p = vec![("patient".to_string(), patient.to_string())];
    p.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    if let (true, Some(d)) = (dated, since) {
        p.push(("date".into(), format!("ge{d}")));
    }
    p
}

fn describe(resource_type: &str, params: &[(String, String)]) -> String {
    let q: Vec<String> = params
        .iter()
        .filter(|(k, _)| k != "patient")
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if q.is_empty() {
        resource_type.to_string()
    } else {
        format!("{resource_type}?{}", q.join("&"))
    }
}

/// The types `--since` leaves unbounded, for the header's
/// `since_unbounded`: the Patient, every type with a search that carries no
/// date, and the types read by reference.
pub fn since_unbounded() -> Vec<String> {
    let mut types: BTreeSet<&str> = BTreeSet::from(["Patient"]);
    types.extend(
        SEARCHES
            .iter()
            .filter(|(_, _, dated)| !dated)
            .map(|(t, _, _)| *t),
    );
    types.extend(REFERENCED.iter().copied());
    types.into_iter().map(str::to_string).collect()
}

/// `Binary/<id>` for an attachment URL on this server, relative or absolute.
pub fn binary_id(url: &str) -> Option<String> {
    let tail = url
        .trim_end_matches('/')
        .rsplit_once("Binary/")
        .map(|(_, id)| id)?;
    (!tail.is_empty() && !tail.contains('/') && !tail.contains('?')).then(|| tail.to_string())
}

/// The attachment URLs of a DocumentReference (`content[].attachment`) or
/// a DiagnosticReport (`presentedForm[]`).
pub fn attachment_urls(v: &Value) -> Vec<String> {
    let mut urls = Vec::new();
    for c in v["content"].as_array().into_iter().flatten() {
        if let Some(u) = c.pointer("/attachment/url").and_then(Value::as_str) {
            urls.push(u.to_string());
        }
    }
    for f in v["presentedForm"].as_array().into_iter().flatten() {
        if let Some(u) = f["url"].as_str() {
            urls.push(u.to_string());
        }
    }
    // Media's `content` is one Attachment.
    if let Some(u) = v.pointer("/content/url").and_then(Value::as_str) {
        urls.push(u.to_string());
    }
    urls
}

/// What an export's resources point at, of the types in [`REFERENCED`] on
/// this server.
#[derive(Debug, Default)]
pub struct References {
    /// (type, id) to read.
    pub read: BTreeSet<(String, String)>,
    /// References whose id or version FHIR does not allow, once per
    /// (type, id): each becomes one skip, which names the type and the id's
    /// length, never the id.
    pub refused: BTreeMap<(String, String), String>,
}

/// Collect `v`'s references to the types in [`REFERENCED`] on this server
/// (`base`): relative (`Type/id`), absolute under `base`, or either with a
/// `_history` version, as pk-med-fhir's [`ReferenceTarget`] reads them. A
/// versioned one is read as the current resource. One on another server is
/// not followed: the sign-in is not sent there.
pub fn references(v: &Value, base: &str, out: &mut References) {
    match v {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("reference") {
                match ReferenceTarget::read(r) {
                    Ok(Some(t)) => {
                        if t.is_under(base) && REFERENCED.contains(&t.resource_type) {
                            out.read
                                .insert((t.resource_type.to_string(), t.id.to_string()));
                        }
                    }
                    Err(refused) => {
                        if REFERENCED.contains(&refused.resource_type) && refused.is_under(base) {
                            out.refused
                                .entry((refused.resource_type.to_string(), refused.id.to_string()))
                                .or_insert_with(|| refused.to_string());
                        }
                    }
                    Ok(None) => {}
                }
            }
            for v in map.values() {
                references(v, base, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|v| references(v, base, out)),
        _ => {}
    }
}

fn skip_of(
    resource_type: &str,
    id: Option<&str>,
    search: Option<String>,
    code: &str,
    reason: String,
) -> Skip {
    Skip {
        resource_type: Some(resource_type.to_string()),
        id: id.map(str::to_string),
        search,
        reason,
        code: code.to_string(),
    }
}

/// Run the export into `w`. `note` hears one line per search.
pub fn run<T: Wire, W: std::io::Write>(
    fhir: &Fhir<T>,
    w: &mut RecordsWriter<W>,
    patient: &str,
    since: Option<&str>,
    note: &dyn Fn(String),
) -> Result<(), CliError> {
    let mut refs = References::default();
    let mut tried_binaries = HashSet::new();
    // (type, what the server left out) → the searches it said so in.
    let mut withheld: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    match fhir.read(&format!("Patient/{patient}")) {
        Ok(p) => {
            if let Ok(v) = p.value() {
                // generalPractitioner, managingOrganization.
                references(&v, fhir.base(), &mut refs);
            }
            w.resource(&p)?;
        }
        Err(Failure::Skip { code, reason }) => {
            w.skip(skip_of("Patient", Some(patient), None, code, reason));
        }
        Err(f) => return Err(f.into_error()),
    }

    for (resource_type, extra, dated) in SEARCHES {
        let p = params(patient, extra, *dated, since);
        let search = describe(resource_type, &p);
        let mut n = 0usize;
        let (summary, result) = fhir.search_each(resource_type, &p, |r| {
            let value = r.value().ok();
            // A resource another search already wrote has had its files
            // fetched after it.
            if !w.resource(&r)? {
                return Ok(());
            }
            n += 1;
            if let Some(v) = &value {
                references(v, fhir.base(), &mut refs);
                if has_files(&r.resource_type) {
                    binaries(fhir, w, v, &mut tried_binaries)?;
                }
            }
            Ok(())
        });
        for line in &summary.withheld {
            withheld
                .entry((resource_type.to_string(), line.clone()))
                .or_default()
                .push(search.clone());
        }
        // Entries on the pages read that were not resources, whether or
        // not a later page failed.
        if summary.malformed > 0 {
            w.skip(skip_of(
                resource_type,
                None,
                Some(search.clone()),
                "upstream",
                format!(
                    "{} search entries were not FHIR resources",
                    summary.malformed
                ),
            ));
        }
        match result {
            Ok(()) => {
                if summary.truncated {
                    w.skip(skip_of(
                        resource_type,
                        None,
                        Some(search.clone()),
                        "page_cap",
                        format!("stopped after {} pages", summary.pages),
                    ));
                }
                note(format!("{search}: {n}"));
            }
            Err(Failure::Skip { code, reason }) => {
                note(format!("{search}: {n}, then skipped ({code})"));
                w.skip(skip_of(resource_type, None, Some(search), code, reason));
            }
            Err(f) => return Err(f.into_error()),
        }
    }

    // One skip per thing left out, though Epic repeats it in every search
    // that touches the sub-resource.
    // No `search`: records-stream/v1's `search` names a search left out,
    // and these were served. The reason names them instead.
    for ((t, line), searches) in withheld {
        let also = match searches.len() {
            1 => String::new(),
            n => format!(" and {} more", n - 1),
        };
        w.skip(skip_of(
            &t,
            None,
            None,
            FORBIDDEN,
            format!(
                "the server left out some results of {}{also}: {line}",
                searches[0]
            ),
        ));
    }
    for ((t, _), reason) in refs.refused {
        w.skip(skip_of(&t, None, None, INVALID_REFERENCE, reason));
    }
    let mut read = 0;
    for (t, id) in refs.read {
        if w.written(&t, &id) {
            continue;
        }
        if read == REFERENCE_CAP {
            w.skip(skip_of(
                &t,
                None,
                None,
                "page_cap",
                format!("stopped after {REFERENCE_CAP} reads by reference"),
            ));
            break;
        }
        read += 1;
        match fhir.read(&format!("{t}/{id}")) {
            Ok(r) => {
                if w.resource(&r)? && has_files(&t) {
                    if let Ok(v) = r.value() {
                        binaries(fhir, w, &v, &mut tried_binaries).map_err(Failure::into_error)?;
                    }
                }
            }
            Err(Failure::Skip { code, reason }) => {
                w.skip(skip_of(&t, Some(&id), None, code, reason))
            }
            Err(f) => return Err(f.into_error()),
        }
    }
    if read > 0 {
        note(format!("referenced resources: {read}"));
    }
    Ok(())
}

/// Each attachment's Binary, written right after the resource naming it.
/// A Binary is fetched once per export: one that failed stays a skip rather
/// than landing later after some other resource.
fn binaries<T: Wire, W: std::io::Write>(
    fhir: &Fhir<T>,
    w: &mut RecordsWriter<W>,
    v: &Value,
    tried: &mut HashSet<String>,
) -> Result<(), Failure> {
    for url in attachment_urls(v) {
        let Some(id) = binary_id(&url) else {
            w.skip(skip_of(
                "Binary",
                None,
                None,
                "unsupported",
                "an attachment URL that is not a Binary on this server".into(),
            ));
            continue;
        };
        if !tried.insert(id.clone()) {
            continue;
        }
        match fhir.binary(&url) {
            Ok(b) => {
                // A Binary without an id is written with the one its
                // attachment URL names. A Binary holds no decimal, so the
                // round trip through a Value changes nothing else.
                let b = if b.id.is_empty() {
                    let mut v = b.value()?;
                    v["id"] = id.clone().into();
                    RawResource::from_value(&v)?
                } else {
                    b
                };
                w.resource(&b)?;
            }
            Err(Failure::Skip { code, reason }) => {
                w.skip(skip_of("Binary", Some(&id), None, code, reason))
            }
            Err(f) => return Err(f),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unbounded_types_are_the_undated_searches_and_the_reads() {
        let types = since_unbounded();
        let mut sorted = types.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(types, sorted, "sorted, once each");
        assert!(types.contains(&"Patient".to_string()));
        for (t, _, dated) in SEARCHES {
            // A type searched both ways is still unbounded by its undated search.
            let undated = SEARCHES.iter().any(|(u, _, d)| u == t && !d);
            assert_eq!(types.contains(&t.to_string()), undated || !dated, "{t}");
        }
        for t in REFERENCED {
            assert!(types.contains(&t.to_string()), "{t}");
        }
        assert!(!types.contains(&"Binary".to_string()));
    }

    #[test]
    fn binary_ids_come_from_relative_and_absolute_urls() {
        assert_eq!(binary_id("Binary/abc").as_deref(), Some("abc"));
        assert_eq!(
            binary_id("https://fhir.example.org/api/FHIR/R4/Binary/e1.2-x").as_deref(),
            Some("e1.2-x")
        );
        assert_eq!(binary_id("https://elsewhere.example/doc.pdf"), None);
        assert_eq!(binary_id("Binary/"), None);
    }

    #[test]
    fn references_are_collected_for_the_followed_types_only() {
        let v: Value = serde_json::json!({
            "medicationReference": {"reference": "Medication/m1"},
            "requester": {"reference": "Practitioner/p1"},
            "subject": {"reference": "Patient/x"},
            "performer": [{"actor": {"reference": "Organization/o1"}}],
            // Absolute on this server (the host in another case), and
            // versioned: read as the current resource.
            "recorder": {"reference": "https://FHIR.example.org/R4/Practitioner/p2"},
            "location": [{"location": {"reference": "Location/l1/_history/3"}}],
            "asserter": {"reference": "https://fhir.example.org/R4/PractitionerRole/r1/_history/2"},
            // Another server, a contained resource, a search: not followed.
            "custodian": {"reference": "https://other.example.org/R4/Organization/o9"},
            "contained": [{"reference": "#med1"}],
            "basedOn": [{"reference": "Practitioner?identifier=x"}],
        });
        let mut out = References::default();
        references(&v, "https://fhir.example.org/R4", &mut out);
        let got: Vec<String> = out.read.iter().map(|(t, i)| format!("{t}/{i}")).collect();
        assert_eq!(
            got,
            [
                "Location/l1",
                "Medication/m1",
                "Organization/o1",
                "Practitioner/p1",
                "Practitioner/p2",
                "PractitionerRole/r1"
            ]
        );
        assert!(out.refused.is_empty());
    }

    /// A reference whose id FHIR does not allow is kept for one skip, once,
    /// and never read; one on another server is neither.
    #[test]
    fn a_reference_with_an_id_fhir_does_not_allow_is_kept_for_a_skip() {
        let long = "x".repeat(pk_med_fhir::MAX_ID_LEN + 1);
        let v: Value = serde_json::json!({
            "requester": {"reference": "Practitioner/has space"},
            "recorder": {"reference": "Practitioner/has space"},
            "performer": [{"actor": {"reference": format!("https://fhir.example.org/R4/Organization/{long}")}}],
            "location": {"reference": "Location/l1/_history/bad version"},
            "custodian": {"reference": "https://other.example.org/R4/Organization/has space"},
            "subject": {"reference": "Patient/has space"},
        });
        let mut out = References::default();
        references(&v, "https://fhir.example.org/R4", &mut out);
        assert!(out.read.is_empty(), "{:?}", out.read);
        let types: Vec<&str> = out.refused.keys().map(|(t, _)| t.as_str()).collect();
        assert_eq!(types, ["Location", "Organization", "Practitioner"]);
        for reason in out.refused.values() {
            assert!(
                !reason.contains("has space") && !reason.contains(&long),
                "{reason}"
            );
        }
    }

    #[test]
    fn since_narrows_dated_searches_only() {
        assert_eq!(
            params("p", &[("category", "laboratory")], true, Some("2026-01-01")),
            [
                ("patient".into(), "p".into()),
                ("category".into(), "laboratory".into()),
                ("date".into(), "ge2026-01-01".into())
            ]
        );
        assert_eq!(params("p", &[], false, Some("2026-01-01")).len(), 1);
    }
}
