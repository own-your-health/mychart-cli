//! `mychart documents list|download` (documents/v1) over Epic's
//! `DocumentReference` and `Binary`.
//!
//! A document is one attachment: `<DocumentReference id>` for the first,
//! `<DocumentReference id>#<n>` for the n-th after it (a clinical note
//! comes as HTML and RTF, two attachments of one DocumentReference).

use std::path::{Path, PathBuf};

use base64::Engine as _;
use pk_cli_core::{output, CliError};
use pk_cli_documents::{
    fs_safe, verify_download, DownloadBatch, Paged, SavedDocument, SkippedDocument,
};
use pk_med_records::Skip;
use serde_json::Value;

use super::{transport, Ctx};
use crate::cli::{DocumentsCmd, PatientArgs};
use crate::fhir::{Failure, Fhir};

pub fn run(ctx: &Ctx, cmd: &DocumentsCmd) -> Result<(), CliError> {
    match cmd {
        DocumentsCmd::List { who, range } => {
            range.validate()?;
            let found = list(ctx, who, range)?;
            let mut page = Paged::new(
                "document",
                found.documents.into_iter().map(|d| d.document).collect(),
            );
            page.total = Some(page.items.len() as u64);
            if ctx.json() {
                output::json(&with_withheld(super::to_json(&page)?, &found.withheld)?);
            } else {
                page.emit(false);
            }
            Ok(())
        }
        DocumentsCmd::Download {
            id,
            all,
            output,
            who,
            range,
        } => {
            range.validate()?;
            if *all {
                download_all(ctx, who, range, output)
            } else {
                download_one(ctx, who, id.as_deref().unwrap_or_default(), output)
            }
        }
    }
}

/// What `documents list` found: the documents, and what the server said it
/// left out of the searches.
pub struct Found {
    pub documents: Vec<Listed>,
    /// medical-common DESIGN §1.4's `forbidden` skips, which §1.7 carries as
    /// `withheld` in `document-list/v1` and `document-download-batch/v1`.
    pub withheld: Vec<Skip>,
}

/// `dto` with `withheld` (DESIGN §1.7), unless there is none. `documents/v1`
/// has no field for it; a reader that does not know it ignores it.
fn with_withheld(mut dto: Value, withheld: &[Skip]) -> Result<Value, CliError> {
    if !withheld.is_empty() {
        dto["withheld"] = super::to_json(&withheld)?;
    }
    Ok(dto)
}

/// A listed document and where its bytes are.
pub struct Listed {
    pub document: pk_cli_documents::Document,
    pub url: String,
    pub content_type: Option<String>,
}

fn text(v: &Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .find_map(|p| v.pointer(p).and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The most of a document id a file name keeps.
const MAX_STEM: usize = 100;

/// A file name's part for a document id. An Epic id can run past FHIR's 64
/// characters (docs/api.md), and a file name past 255 bytes cannot be
/// written, so a long id keeps its first characters and gains a hash of the
/// whole id, which keeps two long ids with the same start apart.
fn file_stem(id: &str) -> String {
    use sha2::{Digest, Sha256};
    let safe = fs_safe(id);
    if safe.len() <= MAX_STEM {
        return safe;
    }
    let digest = Sha256::digest(id.as_bytes());
    let hash: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{hash}", &safe[..MAX_STEM - 17])
}

/// The file extension for a MIME type, and the `verify_download` label.
pub fn extension(content_type: Option<&str>) -> (&'static str, Option<&'static str>) {
    let ct = content_type
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match ct.as_str() {
        "application/pdf" => ("pdf", Some("pdf")),
        "text/html" => ("html", Some("html")),
        "text/rtf" | "application/rtf" => ("rtf", Some("rtf")),
        "text/plain" => ("txt", Some("txt")),
        "application/xml" | "text/xml" | "application/hl7-v3+xml" => ("xml", Some("xml")),
        "image/jpeg" => ("jpg", None),
        "image/png" => ("png", None),
        "image/tiff" => ("tif", None),
        "image/gif" => ("gif", None),
        _ => ("bin", None),
    }
}

/// The documents of one DocumentReference.
pub fn documents_of(v: &Value) -> Vec<Listed> {
    let Some(id) = v["id"].as_str() else {
        return Vec::new();
    };
    let date = text(v, &["/date", "/context/period/start"])
        .map(|d| d.chars().take(10).collect::<String>());
    let name = text(v, &["/description", "/type/text", "/type/coding/0/display"]);
    let category = text(
        v,
        &[
            "/category/0/text",
            "/category/0/coding/0/display",
            "/category/0/coding/0/code",
        ],
    );
    v["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let a = &c["attachment"];
            Some((a["url"].as_str()?.to_string(), a))
        })
        .enumerate()
        .map(|(n, (url, a))| {
            let content_type = a["contentType"].as_str().map(str::to_string);
            let (ext, _) = extension(content_type.as_deref());
            let doc_id = if n == 0 {
                id.to_string()
            } else {
                format!("{id}#{}", n + 1)
            };
            let mut d = pk_cli_documents::Document::new(
                doc_id,
                name.clone()
                    .or_else(|| a["title"].as_str().map(str::to_string))
                    .unwrap_or_else(|| "Document".into()),
            );
            d.date = date.clone();
            d.category = category.clone();
            d.file = Some(format!(
                "{}{}.{ext}",
                date.as_deref()
                    .map(|d| format!("{}_", fs_safe(d)))
                    .unwrap_or_default(),
                file_stem(&d.id)
            ));
            Listed {
                document: d,
                url,
                content_type,
            }
        })
        .collect()
}

fn in_range(date: Option<&str>, range: &pk_cli_documents::RangeArgs) -> bool {
    let Some(d) = date else {
        return range.since.is_none() && range.until.is_none();
    };
    range.since.as_deref().is_none_or(|s| d >= s) && range.until.as_deref().is_none_or(|u| d <= u)
}

pub fn list(
    ctx: &Ctx,
    who: &PatientArgs,
    range: &pk_cli_documents::RangeArgs,
) -> Result<Found, CliError> {
    let c = ctx.signed_in(who)?;
    let t = transport()?;
    let live = ctx.live(&t, &c)?;
    let fhir = Fhir::new(&t, &live)?;
    list_in(ctx, &fhir, &c.id, range)
}

/// The patient's documents in `range`, newest first, through one sign-in.
fn list_in<T: crate::fhir::Wire>(
    ctx: &Ctx,
    fhir: &Fhir<T>,
    patient: &str,
    range: &pk_cli_documents::RangeArgs,
) -> Result<Found, CliError> {
    let mut out: Vec<Listed> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut warned = std::collections::BTreeSet::new();
    let mut withheld = crate::export::Withheld::default();
    for extra in [&[("category", "clinical-note")][..], &[][..]] {
        let mut params = vec![("patient".to_string(), patient.to_string())];
        params.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        let (summary, r) = fhir.search_each("DocumentReference", &params, |r| {
            if seen.insert(r.id.clone()) {
                if let Ok(v) = r.value() {
                    out.extend(documents_of(&v));
                }
            }
            Ok(())
        });
        if summary.malformed > 0 {
            ctx.note(format!(
                "warning: {} DocumentReference entries were not FHIR resources",
                summary.malformed
            ));
        }
        withheld.add(
            "DocumentReference",
            &crate::export::describe("DocumentReference", &params),
            &summary.withheld,
        );
        for line in summary.withheld {
            if warned.insert(line.clone()) {
                ctx.note(format!(
                    "warning: the server left out some DocumentReference results: {line}"
                ));
            }
        }
        match r {
            Ok(()) => {}
            Err(Failure::Skip { code, reason }) => ctx.note(format!(
                "warning: DocumentReference search {extra:?} refused ({code}: {reason})"
            )),
            Err(f) => return Err(f.into_error()),
        }
    }
    out.retain(|d| in_range(d.document.date.as_deref(), range));
    out.sort_by(|a, b| {
        b.document
            .date
            .cmp(&a.document.date)
            .then(a.document.id.cmp(&b.document.id))
    });
    if let Some(n) = range.limit {
        out.truncate(n as usize);
    }
    Ok(Found {
        documents: out,
        withheld: withheld.skips(),
    })
}

/// Fetch one document's bytes and check they are what its type says.
fn fetch<T: crate::fhir::Wire>(fhir: &Fhir<T>, d: &Listed) -> Result<Vec<u8>, Failure> {
    let b = fhir.binary(&d.url)?;
    let v = b.value().unwrap_or_default();
    let Some(data) = v["data"].as_str() else {
        return Err(Failure::Skip {
            code: "no_file",
            reason: "the Binary carries no data".into(),
        });
    };
    let compact: String = data.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact.as_bytes())
        .map_err(|e| Failure::Skip {
            code: "verify_failed",
            reason: format!("the Binary's data is not base64: {e}"),
        })?;
    let ct = v["contentType"].as_str().or(d.content_type.as_deref());
    match extension(ct).1 {
        Some(label) => verify_download(&bytes, Some(label)),
        None if bytes.is_empty() => Err(CliError::Upstream("the document is empty".into())),
        None => Ok(()),
    }
    .map_err(|e| Failure::Skip {
        code: "verify_failed",
        reason: e.message().to_string(),
    })?;
    Ok(bytes)
}

fn target(output: &Path, d: &Listed) -> PathBuf {
    if output.is_dir() {
        output.join(d.document.file.as_deref().unwrap_or("document.bin"))
    } else {
        output.to_path_buf()
    }
}

fn download_one(ctx: &Ctx, who: &PatientArgs, id: &str, output: &Path) -> Result<(), CliError> {
    let id = id.strip_suffix("#1").unwrap_or(id);
    let c = ctx.signed_in(who)?;
    let t = transport()?;
    let live = ctx.live(&t, &c)?;
    let fhir = Fhir::new(&t, &live)?;
    let docs = list_in(ctx, &fhir, &c.id, &pk_cli_documents::RangeArgs::default())?.documents;
    let Some(d) = docs.iter().find(|d| d.document.id == id) else {
        return Err(
            CliError::NotFound(format!("no document `{id}` for this patient"))
                .with_remedy("list them with `mychart documents list`"),
        );
    };
    let bytes = fetch(&fhir, d).map_err(Failure::into_error)?;
    let path = target(output, d);
    crate::fsutil::write_private(&path, &bytes)?;
    let saved =
        SavedDocument::from_document(&d.document, path.display().to_string(), bytes.len() as u64);
    pk_cli_documents::emit(&saved, ctx.json());
    Ok(())
}

fn download_all(
    ctx: &Ctx,
    who: &PatientArgs,
    range: &pk_cli_documents::RangeArgs,
    dir: &Path,
) -> Result<(), CliError> {
    if dir.exists() && !dir.is_dir() {
        return Err(CliError::Usage(format!(
            "--all saves into a directory; {} is a file",
            dir.display()
        )));
    }
    let c = ctx.signed_in(who)?;
    let t = transport()?;
    let live = ctx.live(&t, &c)?;
    let fhir = Fhir::new(&t, &live)?;
    let found = list_in(ctx, &fhir, &c.id, range)?;
    let docs = found.documents;
    crate::fsutil::create_private_dir(dir)?;
    let (mut saved, mut skipped) = (Vec::new(), Vec::new());
    for d in &docs {
        match fetch(&fhir, d) {
            Ok(bytes) => {
                let path = target(dir, d);
                crate::fsutil::write_private(&path, &bytes)?;
                saved.push(SavedDocument::from_document(
                    &d.document,
                    path.display().to_string(),
                    bytes.len() as u64,
                ));
            }
            Err(Failure::Skip { code, reason }) => {
                skipped.push(SkippedDocument::new(d.document.id.clone(), reason).with_code(code))
            }
            Err(f) => return Err(f.into_error()),
        }
    }
    let batch = DownloadBatch::new(dir.display().to_string(), saved).with_skipped(skipped);
    if ctx.json() {
        output::json(&with_withheld(super::to_json(&batch)?, &found.withheld)?);
    } else {
        println!(
            "saved {} documents to {} ({} skipped)",
            batch.count,
            batch.dir,
            batch.skipped.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn withheld_is_added_to_the_output_only_when_there_is_some() {
        let dto = serde_json::json!({"schema": "document-list/v1", "items": []});
        assert_eq!(with_withheld(dto.clone(), &[]).unwrap(), dto);
        let skip = Skip {
            resource_type: Some("DocumentReference".into()),
            id: None,
            search: None,
            reason: "left out".into(),
            code: "forbidden".into(),
        };
        let out = with_withheld(dto, &[skip]).unwrap();
        assert_eq!(out["schema"], "document-list/v1");
        assert_eq!(
            out["withheld"],
            serde_json::json!([{"resource_type": "DocumentReference", "reason": "left out", "code": "forbidden"}])
        );
    }

    #[test]
    fn each_attachment_is_a_document_named_safely() {
        let v: Value = serde_json::json!({
            "resourceType": "DocumentReference", "id": "doc-1", "date": "2026-01-06T10:00:00Z",
            "type": {"text": "Progress note"},
            "category": [{"coding": [{"code": "clinical-note"}]}],
            "content": [
                {"attachment": {"contentType": "text/html", "url": "Binary/b1"}},
                {"attachment": {"contentType": "text/rtf", "url": "Binary/b2"}},
                {"attachment": {"contentType": "text/plain"}}
            ]
        });
        let docs = documents_of(&v);
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].document.id, "doc-1");
        assert_eq!(docs[1].document.id, "doc-1#2");
        assert_eq!(
            docs[0].document.file.as_deref(),
            Some("2026-01-06_doc-1.html")
        );
        assert_eq!(
            docs[1].document.file.as_deref(),
            Some("2026-01-06_doc-1_2.rtf")
        );
        assert_eq!(docs[0].document.category.as_deref(), Some("clinical-note"));
        assert_eq!(docs[0].document.name, "Progress note");
    }

    #[test]
    fn a_long_id_keeps_its_start_and_a_hash_in_the_file_name() {
        assert_eq!(file_stem("doc-1#2"), "doc-1_2");
        let exact = "a".repeat(MAX_STEM);
        assert_eq!(file_stem(&exact), exact);
        let one = format!("{}x", "e".repeat(200));
        let two = format!("{}y", "e".repeat(200));
        let (a, b) = (file_stem(&one), file_stem(&two));
        assert_eq!(a.len(), MAX_STEM);
        assert!(a.starts_with(&"e".repeat(MAX_STEM - 17)), "{a}");
        assert_ne!(a, b);
        assert_eq!(a, file_stem(&one));
    }
}
