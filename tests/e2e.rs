//! Sign-in end to end against the mock Epic organization, in-process: the
//! directory download, discovery with `Epic-Client-ID`, the standalone
//! launch through a stand-in browser, the code exchange, dynamic client
//! registration, the JWT-bearer grant, renewal, proxy access and logout.
//! The vault is in memory, so no Keychain is read on any platform; the
//! sign-in window is a stand-in too (Chrome is not run), and every command
//! runs as the profile `acct1`.

#[path = "support/mock_epic.rs"]
mod mock_epic;

use std::path::Path;

use mock_epic::{MockEpic, Pick, CHILD_PATIENT, CLIENT_ID, ORG_ID, PROXY_USER, SELF_PATIENT};
use mychart::cli::{AuthCmd, LoginArgs, LogoutArgs, PatientsCmd};
use mychart::commands::{self, Ctx};
use mychart::config::Config;
use std::sync::Arc;

use mychart::session::{self, Live};
use mychart::vault::{MemoryVault, Sessions};
use pk_cli_auth::refresh_lock::RefreshLock;
use pk_cli_auth::vault::MemoryKeys;
use pk_cli_config::profile::ProfileName;
use pk_cli_config::ConfigStore;
use pk_cli_core::{CliError, CommonArgs};
use pk_cli_http::BlockingTransport;
use pk_med_smart::{SessionStore, SmartSession};

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A browser that follows the authorize redirect back to the loopback
/// listener, as MyChart would after the person signs in.
fn browser(url: &str) -> bool {
    let url = url.to_string();
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let resp = client.get(&url).send().unwrap();
        assert_eq!(
            resp.status().as_u16(),
            302,
            "authorize refused: {}",
            resp.text().unwrap_or_default()
        );
        let back = resp.headers()["location"].to_str().unwrap().to_string();
        client.get(back).send().unwrap();
    });
    true
}

/// A browser for an app whose redirect is an https page: it follows the
/// authorize redirect to the page, which (as `site/callback.html` does)
/// passes the query on to the listener on `port`.
fn browser_through(page: &'static str, port: u16) -> impl Fn(&str) -> bool {
    move |url: &str| {
        let url = url.to_string();
        std::thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            let resp = client.get(&url).send().unwrap();
            assert_eq!(resp.status().as_u16(), 302, "authorize refused");
            let back = resp.headers()["location"].to_str().unwrap().to_string();
            let query = back
                .strip_prefix(page)
                .expect("the answer goes to the page")
                .to_string();
            client
                .get(format!("http://127.0.0.1:{port}/callback{query}"))
                .send()
                .unwrap();
        });
        true
    }
}

/// The profile every test runs as.
fn p() -> ProfileName {
    ProfileName::parse("acct1").unwrap()
}

fn ctx(dir: &Path, mock: &MockEpic, configure: impl FnOnce(&mut Config)) -> Ctx {
    let store = ConfigStore::new("mychart").with_override(Some(dir.join("config.json")));
    let mut cfg = Config {
        client_id: Some(CLIENT_ID.into()),
        directory_url: Some(mock.directory_url()),
        redirect_port: Some(free_port()),
        ..Config::default()
    };
    cfg.profiles.add(p());
    configure(&mut cfg);
    store.save(&cfg).unwrap();
    Ctx {
        common: CommonArgs {
            json: true,
            quiet: true,
            ..CommonArgs::default()
        },
        profile: Some(p().to_string()),
        store,
        vault: Arc::new(MemoryVault::default()),
        legacy: Arc::new(MemoryKeys::new()),
        lock: RefreshLock::at(dir.join("token-refresh.lock")),
        cache_dir: dir.join("cache"),
        forget_browser: Box::new(|_: &ProfileName| Ok(false)),
        // No Chrome here: the sign-in falls back to `browser`, the stand-in
        // that plays MyChart, as the binary falls back to the default one.
        window: Box::new(|_: &commands::Opening<'_>| {
            Err(CliError::Other("no Chrome in the tests".into()))
        }),
        browser: Box::new(browser),
        vault_note: None,
    }
}

fn login(ctx: &Ctx, label: &str) {
    commands::auth::run(
        ctx,
        &AuthCmd::Login(LoginArgs {
            org: ORG_ID.into(),
            label: Some(label.into()),
            no_browser: false,
            timeout: 1,
        }),
    )
    .unwrap();
}

fn sessions(ctx: &Ctx) -> Sessions {
    Sessions::new(ctx.vault.clone(), p())
}

fn stored(ctx: &Ctx, patient: &str) -> SmartSession {
    sessions(ctx)
        .load(&session::account(ORG_ID, patient))
        .unwrap()
        .expect("a stored sign-in")
}

fn store(ctx: &Ctx, s: &SmartSession) {
    sessions(ctx).save(s).unwrap();
}

fn live(ctx: &Ctx, c: &session::Context) -> Result<Live<BlockingTransport>, CliError> {
    Live::start(
        &commands::transport().unwrap(),
        sessions(ctx),
        &ctx.lock,
        c,
        None,
    )
}

/// The context's session with a usable token, renewed when due.
fn fresh(ctx: &Ctx, c: &session::Context) -> Result<SmartSession, CliError> {
    live(ctx, c)?.fresh()
}

#[test]
fn a_sign_in_registers_the_install_and_renews_without_the_browser() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");

    let s = stored(&ctx, SELF_PATIENT);
    assert!(s.dynamic_client.is_some());
    assert!(s.renewable());
    assert_eq!(s.client_id, CLIENT_ID);
    // The session keeps the claim as Epic sent it; the index, `Type/id`.
    assert!(s
        .fhir_user
        .as_deref()
        .is_some_and(|u| u.ends_with(&format!("/Patient/{SELF_PATIENT}"))));
    assert_eq!(
        mock.get(|st| (st.registrations, st.jwt_bearer_grants)),
        (1, 1)
    );
    // `openid` is asked for, so the authorization carried a nonce, which
    // the ID token echoed (pk-med-smart refuses one that does not).
    assert_eq!(mock.get(|st| st.nonces), 1);

    let contexts = ctx.contexts(&p()).unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].label.as_deref(), Some("adult1"));
    assert_eq!(contexts[0].relationship.as_deref(), Some("self"));
    assert_eq!(
        contexts[0].fhir_user.as_deref(),
        Some(format!("Patient/{SELF_PATIENT}").as_str())
    );
    assert_eq!(contexts[0].organization_name, mock_epic::ORG_NAME);

    // An expired access token renews through the JWT-bearer grant.
    let mut expired = s;
    expired.tokens.expires_at = Some(0);
    store(&ctx, &expired);
    let renewed = fresh(&ctx, &contexts[0]).unwrap();
    assert!(session::is_fresh(&renewed, session::now()));
    assert_ne!(renewed.tokens.access_token, expired.tokens.access_token);
    assert_eq!(mock.get(|st| st.jwt_bearer_grants), 2);
    assert_eq!(
        stored(&ctx, SELF_PATIENT).tokens.access_token,
        renewed.tokens.access_token
    );

    // A fresh token is used as is.
    fresh(&ctx, &contexts[0]).unwrap();
    assert_eq!(mock.get(|st| st.jwt_bearer_grants), 2);

    // The profile's PatientContext: the Keychain account, never the token,
    // and the stored sign-in's state under `status`.
    let row = commands::auth::patient_dto(&contexts[0], Some(&stored(&ctx, SELF_PATIENT)));
    assert_eq!(
        row["session"],
        format!("smart/{}/{SELF_PATIENT}", mock_epic::ORG_ID)
    );
    assert_eq!(row["kind"], "patient");
    assert_eq!(row["relationship"], "self");
    assert_eq!(row.pointer("/status/authenticated"), Some(&true.into()));
    assert_eq!(row.pointer("/status/renewable"), Some(&true.into()));
    assert!(commands::auth::usable(&row));
    assert!(!row.to_string().contains(&renewed.tokens.access_token));
    let bare = commands::auth::patient_dto(&contexts[0], None);
    assert!(bare.get("status").is_none() && !commands::auth::usable(&bare));

    commands::patients::run(&ctx, &PatientsCmd::List { status: true }).unwrap();
    commands::auth::run(&ctx, &AuthCmd::Status).unwrap();
}

/// Epic's production organizations take only https redirects: the app
/// registers a page that passes the answer on to the listener. The address
/// and the code exchange name the page (the mock refuses an exchange whose
/// redirect differs from the authorization's), and the code still arrives
/// on the listener's port.
#[test]
fn a_sign_in_through_an_https_page_names_the_page_and_hears_the_answer_on_the_port() {
    const PAGE: &str = "https://pages.example.test/mychart-cli/callback";
    let mock = MockEpic::start();
    mock.set(|st| st.redirect_page = Some(PAGE.into()));
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut ctx = ctx(dir.path(), &mock, |c| {
        c.redirect_port = Some(port);
        c.redirect_uri = Some(PAGE.into());
    });
    ctx.browser = Box::new(browser_through(PAGE, port));
    login(&ctx, "adult1");
    let s = stored(&ctx, SELF_PATIENT);
    assert!(s.dynamic_client.is_some());
    assert_eq!(mock.get(|st| st.registrations), 1);
}

/// `auth probe` reads Epic's three answers from the authorize endpoint
/// alone: no browser, no listener, no Keychain, and no code exchanged.
#[test]
fn a_probe_tells_a_known_client_from_an_unknown_one_and_a_refused_redirect() {
    let mock = MockEpic::start();
    mock.set(|st| st.redirect_page = Some("https://pages.example.org/callback".into()));
    let probe = |configure: &dyn Fn(&mut Config)| {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ctx(dir.path(), &mock, configure);
        ctx.browser = Box::new(|_: &str| panic!("no browser may open"));
        let v = commands::auth::probe_org(&ctx, ORG_ID).unwrap();
        assert!(ctx.contexts(&p()).unwrap().is_empty(), "nothing stored");
        v
    };
    let known = probe(&|_| {});
    assert_eq!(known["answer"], "known", "{known}");
    assert_eq!(known["client_key"], "client_id");
    assert_eq!(known["status"], 302);
    assert!(known.get("error").is_none(), "{known}");
    let page = probe(&|c| c.redirect_uri = Some("https://pages.example.org/callback".into()));
    assert_eq!(page["answer"], "known", "{page}");
    assert_eq!(page["redirect_uri"], "https://pages.example.org/callback");

    let unknown = probe(&|c| c.client_id = Some("00000000-0000-0000-0000-000000000009".into()));
    assert_eq!(unknown["answer"], "unknown", "{unknown}");
    assert_eq!(unknown["status"], 200);

    let refused = probe(&|c| c.redirect_uri = Some("https://pages.example.org/elsewhere".into()));
    assert_eq!(refused["answer"], "refused", "{refused}");
    assert_eq!(refused["error"], "4");

    assert_eq!(mock.get(|st| st.registrations), 0);
    assert!(mock.get(|st| st.hits.is_empty()), "no FHIR read");
}

/// The page this repository publishes forwards to 7468 only: with another
/// port the sign-in is refused before anything is sent or opened.
#[test]
fn the_published_page_with_another_port_is_refused_before_the_network() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = ctx(dir.path(), &mock, |c| {
        c.redirect_port = Some(8123);
        c.redirect_uri = Some(mychart::config::PUBLISHED_REDIRECT.into());
    });
    ctx.browser = Box::new(|_: &str| panic!("no browser may open"));
    let err = commands::auth::run(
        &ctx,
        &AuthCmd::Login(LoginArgs {
            org: ORG_ID.into(),
            label: None,
            no_browser: false,
            timeout: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("8123"), "{err}");
    assert!(mock.get(|st| st.hits.is_empty() && st.registrations == 0));
}

#[test]
fn a_parent_reaches_a_child_through_proxy_access() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    mock.set(|st| st.pick = Pick::Child);
    login(&ctx, "child1");

    let contexts = ctx.contexts(&p()).unwrap();
    assert_eq!(contexts.len(), 2);
    let child = session::pick(&p(), &contexts, Some("child1"), None).unwrap();
    assert_eq!(child.id, CHILD_PATIENT);
    assert_eq!(child.relationship.as_deref(), Some("proxy"));
    assert_eq!(child.fhir_user.as_deref(), Some(PROXY_USER));
    assert!(stored(&ctx, CHILD_PATIENT).renewable());
    // Two people held: a command that reads records must be told which.
    assert_eq!(
        session::pick(&p(), &contexts, None, None)
            .unwrap_err()
            .exit_code(),
        2
    );

    commands::auth::run(
        &ctx,
        &AuthCmd::Logout(LogoutArgs {
            org: None,
            patient: Some("child1".into()),
            forget: false,
        }),
    )
    .unwrap();
    let left = ctx.contexts(&p()).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, SELF_PATIENT);
    assert!(sessions(&ctx)
        .load(&session::account(ORG_ID, CHILD_PATIENT))
        .unwrap()
        .is_none());

    commands::auth::run(
        &ctx,
        &AuthCmd::Logout(LogoutArgs {
            org: None,
            patient: None,
            forget: true,
        }),
    )
    .unwrap();
    assert!(ctx.contexts(&p()).unwrap().is_empty());
    // --forget clears the settings and keeps the profiles.
    let cfg = ctx.config().unwrap();
    assert!(cfg.client_id.is_none());
    assert!(cfg.profiles.contains(&p()));
}

#[test]
fn an_ended_access_period_asks_for_a_new_sign_in() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let c = ctx.contexts(&p()).unwrap().remove(0);
    let mut s = stored(&ctx, SELF_PATIENT);
    s.tokens.expires_at = Some(0);
    store(&ctx, &s);
    mock.set(|st| st.refuse_jwt_bearer = true);
    let Err(e) = fresh(&ctx, &c) else {
        panic!("a refused renewal returned a session")
    };
    assert_eq!(e.exit_code(), 3, "{e}");
    assert!(e.to_string().contains("auth login"), "{e}");
}

#[test]
fn without_dynamic_registration_the_sign_in_lasts_its_hour() {
    let mock = MockEpic::start();
    mock.set(|st| st.offer_registration = false);
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let s = stored(&ctx, SELF_PATIENT);
    assert!(s.dynamic_client.is_none());
    assert!(!s.renewable());
    assert!(session::is_fresh(&s, session::now()));
    assert_eq!(mock.get(|st| st.registrations), 0);
}

#[test]
fn dynamic_registration_can_be_turned_off() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |c| c.dynamic_registration = Some(false));
    login(&ctx, "adult1");
    assert!(!stored(&ctx, SELF_PATIENT).renewable());
    assert_eq!(mock.get(|st| st.registrations), 0);
}

#[test]
fn a_sign_in_without_a_client_id_stops_before_the_network() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |c| c.client_id = None);
    let e = commands::auth::run(
        &ctx,
        &AuthCmd::Login(LoginArgs {
            org: "sandbox".into(),
            label: None,
            no_browser: true,
            timeout: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(e.exit_code(), 2);
    assert!(format!("{e:?}").contains("sandbox_client_id"), "{e:?}");
}

#[test]
fn an_unknown_organization_is_not_found() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    let e = commands::auth::run(
        &ctx,
        &AuthCmd::Login(LoginArgs {
            org: "No Such Clinic".into(),
            label: None,
            no_browser: true,
            timeout: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(e.exit_code(), 4, "{e}");
}

fn export_stream(
    ctx: &Ctx,
    patient: &str,
    since: Option<&str>,
) -> (Vec<serde_json::Value>, Vec<String>) {
    let c = ctx
        .contexts(&p())
        .unwrap()
        .into_iter()
        .find(|c| c.id == patient)
        .unwrap();
    let t = commands::transport().unwrap();
    let live = live(ctx, &c).unwrap();
    let fhir = mychart::fhir::Fhir::new(&t, &live).unwrap();
    let mut buf = Vec::new();
    let mut w =
        pk_med_records::RecordsWriter::start(&mut buf, &commands::records::header(&c, since))
            .unwrap();
    mychart::export::run(&fhir, &mut w, &c.id, since, &|_| {}).unwrap();
    w.finish("2026-01-01T00:00:00Z", None).unwrap();
    let text = String::from_utf8(buf).unwrap();
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let values = lines
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (values, lines)
}

#[test]
fn an_export_is_one_records_stream_with_documents_after_their_references() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let (values, lines) = export_stream(&ctx, SELF_PATIENT, None);

    let header = &values[0];
    assert_eq!(header["schema"], "records-stream/v1");
    assert_eq!(header["source"]["cli"], "mychart");
    assert_eq!(header["organization"]["id"], ORG_ID);
    assert_eq!(header["organization"]["vendor"], "epic");
    assert_eq!(header["patient"]["id"], SELF_PATIENT);
    assert_eq!(header["patient"]["label"], "adult1");
    assert_eq!(header["patient"]["relationship"], "self");
    assert_eq!(header["fhir_version"], "4.0.1");
    assert!(header.get("since").is_none());
    assert!(header.get("since_unbounded").is_none(), "{header}");

    let trailer = values.last().unwrap();
    assert_eq!(trailer["schema"], "records-stream-end/v1");
    assert_eq!(trailer["complete"], true);

    // Counts agree with the lines, by type, as medrec checks them.
    let body = &values[1..values.len() - 1];
    let mut counted = std::collections::BTreeMap::<String, u64>::new();
    for v in body {
        *counted
            .entry(v["resourceType"].as_str().unwrap().to_string())
            .or_default() += 1;
    }
    let trailer_counts: std::collections::BTreeMap<String, u64> =
        serde_json::from_value(trailer["counts"].clone()).unwrap();
    assert_eq!(counted, trailer_counts);
    let expect = [
        ("AllergyIntolerance", 1),
        ("Binary", 3),
        ("Condition", 1),
        ("DiagnosticReport", 1),
        ("DocumentReference", 1),
        ("Media", 1),
        ("Medication", 1),
        ("MedicationRequest", 1),
        ("Observation", 3),
        ("Patient", 1),
        ("Practitioner", 1),
        ("Specimen", 1),
    ];
    assert_eq!(
        trailer_counts,
        expect
            .iter()
            .map(|(k, v)| (k.to_string(), *v as u64))
            .collect()
    );

    // The server's text, minified: 13.50 stays 13.50.
    assert!(
        lines.iter().any(|l| l.contains(r#""value":13.50"#)),
        "decimal rewritten"
    );
    assert!(lines.iter().all(|l| !l.contains("\n  ")));
    // Each Binary follows its DocumentReference.
    let pos = |t: &str, id: &str| {
        body.iter()
            .position(|v| v["resourceType"] == t && v["id"] == id)
            .unwrap()
    };
    assert!(pos("DocumentReference", "doc-1") < pos("Binary", "bin-pdf-1"));
    assert!(pos("DocumentReference", "doc-1") < pos("Binary", "bin-html-1"));
    // Specimen and Media are read from what names them, Epic's searches of
    // them taking only `_id`; a Media's file follows it.
    assert_eq!(pos("Media", "media-1") + 1, pos("Binary", "bin-media-1"));
    let hits = |path: &str| mock.get(|st| st.hits.get(path).copied());
    assert_eq!(hits("Specimen"), None);
    assert_eq!(hits("Media"), None);
    assert_eq!(hits("Specimen/spec-1"), Some(1));
    assert_eq!(hits("Media/media-1"), Some(1));
    assert_eq!(body[0]["resourceType"], "Patient");
    // No other patient's records.
    assert!(body.iter().all(|v| !v.to_string().contains(CHILD_PATIENT)));

    // Refused searches are skips, not the end of the export.
    let skipped = trailer["skipped"].as_array().unwrap();
    let skip = |search: &str| {
        skipped
            .iter()
            .find(|s| s["search"] == search)
            .map(|s| s["code"].as_str().unwrap().to_string())
    };
    assert_eq!(
        skip("Observation?category=sdoh").as_deref(),
        Some("unsupported")
    );
    assert_eq!(skip("Device").as_deref(), Some("forbidden"));
}

#[test]
fn since_narrows_dated_searches() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let (values, _) = export_stream(&ctx, SELF_PATIENT, Some("2026-02-01"));
    assert_eq!(values[0]["since"], "2026-02-01");
    let unbounded: Vec<&str> = values[0]["since_unbounded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for t in [
        "Patient",
        "AllergyIntolerance",
        "Condition",
        "Medication",
        "Practitioner",
    ] {
        assert!(unbounded.contains(&t), "{t} in {unbounded:?}");
    }
    for t in ["Observation", "DocumentReference", "Binary", "Encounter"] {
        assert!(!unbounded.contains(&t), "{t} in {unbounded:?}");
    }
    let trailer = values.last().unwrap();
    assert_eq!(trailer["counts"]["Observation"], 2);
    assert_eq!(trailer["counts"]["AllergyIntolerance"], 1);
}

#[test]
fn an_export_to_a_directory_writes_one_private_file() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    mock.set(|st| st.pick = Pick::Child);
    login(&ctx, "child1");
    let out = dir.path().join("export");
    commands::records::run(
        &ctx,
        &mychart::cli::RecordsCmd::Export(mychart::cli::ExportArgs {
            who: mychart::cli::PatientArgs {
                patient: Some("child1".into()),
                org: None,
            },
            since: None,
            ndjson: false,
            output: Some(out.clone()),
        }),
    )
    .unwrap();
    let path = out.join("records.ndjson");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.lines().next().unwrap().contains(CHILD_PATIENT));
    assert!(text.contains(r#""relationship":"proxy""#));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn export_to_a_file_is_for_synthetic_servers_only() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    ctx.update_profile(&p(), |s| {
        s.upsert(session::Context {
            id: "pat-0001".into(),
            organization: "1000-1".into(),
            organization_name: "Example Health".into(),
            fhir_base: "https://fhir.example.org/api/FHIR/R4".into(),
            label: Some("adult9".into()),
            relationship: Some("self".into()),
            fhir_user: None,
            signed_in_at: "2026-01-01T00:00:00Z".into(),
        })
    })
    .unwrap();
    let e = commands::records::run(
        &ctx,
        &mychart::cli::RecordsCmd::Export(mychart::cli::ExportArgs {
            who: mychart::cli::PatientArgs {
                patient: Some("adult9".into()),
                org: None,
            },
            since: None,
            ndjson: false,
            output: Some(dir.path().join("x")),
        }),
    )
    .unwrap_err();
    assert_eq!(e.exit_code(), 2);
    assert!(format!("{e:?}").contains("medrec import"), "{e:?}");
    assert!(!dir.path().join("x").exists());
}

#[test]
fn documents_list_and_download_verify_the_bytes() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let who = mychart::cli::PatientArgs::default();
    let docs = commands::documents::list(&ctx, &who, &pk_cli_documents::RangeArgs::default())
        .unwrap()
        .documents;
    let ids: Vec<&str> = docs.iter().map(|d| d.document.id.as_str()).collect();
    assert_eq!(ids, ["doc-1", "doc-1#2"]);
    assert_eq!(docs[0].document.date.as_deref(), Some("2026-01-06"));

    let out = dir.path().join("docs");
    commands::documents::run(
        &ctx,
        &mychart::cli::DocumentsCmd::Download {
            id: None,
            all: true,
            output: out.clone(),
            who: who.clone(),
            range: pk_cli_documents::RangeArgs::default(),
        },
    )
    .unwrap();
    let pdf = std::fs::read(out.join("2026-01-06_doc-1.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let html = std::fs::read_to_string(out.join("2026-01-06_doc-1_2.html")).unwrap();
    assert!(html.contains("Synthetic visit note"));

    let one = dir.path().join("one.html");
    commands::documents::run(
        &ctx,
        &mychart::cli::DocumentsCmd::Download {
            id: Some("doc-1#2".into()),
            all: false,
            output: one.clone(),
            who: who.clone(),
            range: pk_cli_documents::RangeArgs::default(),
        },
    )
    .unwrap();
    assert!(one.exists());
    let e = commands::documents::run(
        &ctx,
        &mychart::cli::DocumentsCmd::Download {
            id: Some("nope".into()),
            all: false,
            output: dir.path().join("nope"),
            who,
            range: pk_cli_documents::RangeArgs::default(),
        },
    )
    .unwrap_err();
    assert_eq!(e.exit_code(), 4);
}

#[test]
fn api_reads_and_refuses_writes_and_other_hosts() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let api = |method: &str, path: &str| {
        commands::api::run(
            &ctx,
            &mychart::cli::ApiArgs {
                method: method.into(),
                path: path.into(),
                who: Default::default(),
            },
        )
    };
    api("GET", &format!("Patient/{SELF_PATIENT}")).unwrap();
    assert_eq!(api("POST", "Patient").unwrap_err().exit_code(), 2);
    assert_eq!(
        api("GET", "https://elsewhere.example/Patient/x")
            .unwrap_err()
            .exit_code(),
        2
    );
    assert_eq!(
        api("GET", "Observation/no-such").unwrap_err().exit_code(),
        4
    );
    assert_eq!(
        api("GET", &format!("Patient/{CHILD_PATIENT}"))
            .unwrap_err()
            .exit_code(),
        3
    );
}

fn skips(trailer: &serde_json::Value) -> Vec<(String, String, String)> {
    trailer["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let text = |k: &str| s[k].as_str().unwrap_or_default().to_string();
            (text("resource_type"), text("id"), text("code"))
        })
        .collect()
}

#[test]
fn results_the_server_withholds_are_one_skip_each() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    const OUTSIDE: &str = "Client not authorized for Condition - Outside Record Problems. \
         Search results of this type have not been included.";
    const SMARTDATA: &str = "User not authorized for Observation - SmartData Elements. \
         Search results of this type have not been included.";
    mock.set(|st| {
        // Every Condition search, as Epic repeats it in each.
        st.withhold.push(("Condition".into(), OUTSIDE.into()));
        st.withhold
            .push(("Observation?category=laboratory".into(), SMARTDATA.into()));
    });
    let (values, _) = export_stream(&ctx, SELF_PATIENT, None);
    let trailer = values.last().unwrap();
    assert_eq!(trailer["complete"], true);
    // What was served is kept.
    assert_eq!(trailer["counts"]["Condition"], 1);
    assert_eq!(trailer["counts"]["Observation"], 3);
    let withheld: Vec<(String, String)> = trailer["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["reason"].as_str().unwrap().contains("left out"))
        .map(|s| {
            assert_eq!(s["code"], "forbidden", "{s}");
            assert!(s.get("id").is_none(), "{s}");
            // Its searches were served, so it names none as left out.
            assert!(s.get("search").is_none(), "{s}");
            let text = |k: &str| s[k].as_str().unwrap().to_string();
            (text("resource_type"), text("reason"))
        })
        .collect();
    assert_eq!(withheld.len(), 2, "{withheld:?}");
    assert_eq!(withheld[0].0, "Condition");
    assert!(withheld[0].1.starts_with(
        "the server left out some results of Condition?category=problem-list-item and 2 more: "
    ));
    assert!(withheld[0].1.contains("Outside Record Problems"));
    assert_eq!(withheld[1].0, "Observation");
    assert!(withheld[1]
        .1
        .starts_with("the server left out some results of Observation?category=laboratory: "));
    assert!(withheld[1].1.contains("SmartData Elements"));
}

#[test]
fn documents_list_carries_what_the_server_withheld() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    const NOTES: &str = "Client not authorized for DocumentReference - Outside Record - \
         Clinical Notes. Search results of this type have not been included.";
    // Both DocumentReference searches, as Epic repeats it.
    mock.set(|st| st.withhold.push(("DocumentReference".into(), NOTES.into())));
    let who = mychart::cli::PatientArgs::default();
    let found =
        commands::documents::list(&ctx, &who, &pk_cli_documents::RangeArgs::default()).unwrap();
    // What was served is listed.
    assert_eq!(found.documents.len(), 2);
    let withheld = serde_json::to_value(&found.withheld).unwrap();
    assert_eq!(
        withheld,
        serde_json::json!([{
            "resource_type": "DocumentReference",
            "reason": format!(
                "the server left out some results of \
                 DocumentReference?category=clinical-note and 1 more: {NOTES}"
            ),
            "code": "forbidden",
        }])
    );
}

#[test]
fn an_export_goes_past_files_elsewhere_and_answers_that_are_not_fhir() {
    use serde_json::json;
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    mock.set(|st| {
        let me = json!({"reference": format!("Patient/{SELF_PATIENT}")});
        // A note whose file is on another server.
        let far = json!({
            "resourceType": "DocumentReference", "id": "doc-far", "subject": me,
            "date": "2026-02-01T00:00:00Z",
            "category": [{"coding": [{"code": "clinical-note"}]}],
            "content": [{"attachment": {"contentType": "application/pdf",
                "url": "https://elsewhere.example.org/api/FHIR/R4/Binary/bin-far"}}]
        });
        st.resources
            .entry("DocumentReference".into())
            .or_default()
            .push((far.clone(), far.to_string()));
        // A search entry that is not a resource.
        let goal = json!({"id": "goal-x", "subject": me});
        st.resources
            .entry("Goal".into())
            .or_default()
            .push((goal.clone(), goal.to_string()));
        // An organization only the Patient names.
        let patients = st.resources.get_mut("Patient").unwrap();
        let (p, text) = patients
            .iter_mut()
            .find(|(p, _)| p["id"] == SELF_PATIENT)
            .unwrap();
        p["managingOrganization"] = json!({"reference": "Organization/org-1"});
        *text = p.to_string();
        let org = json!({"resourceType": "Organization", "id": "org-1", "name": "Example Clinic"});
        st.resources
            .entry("Organization".into())
            .or_default()
            .push((org.clone(), org.to_string()));
        st.faults
            .push(mock_epic::Fault::not_fhir("Practitioner/prac-1"));
    });

    let (values, _) = export_stream(&ctx, SELF_PATIENT, None);
    let trailer = values.last().unwrap();
    assert_eq!(trailer["complete"], true);
    let skipped = skips(trailer);
    assert!(
        skipped.contains(&("Binary".into(), "bin-far".into(), "no_file".into())),
        "{skipped:?}"
    );
    assert!(
        skipped.contains(&("Practitioner".into(), "prac-1".into(), "upstream".into())),
        "{skipped:?}"
    );
    assert!(
        skipped
            .iter()
            .any(|(t, _, c)| t == "Goal" && c == "upstream"),
        "{skipped:?}"
    );
    // The searches after the documents ran, and the Patient's reference was read.
    assert_eq!(trailer["counts"]["MedicationRequest"], 1);
    assert_eq!(trailer["counts"]["DocumentReference"], 2);
    assert_eq!(trailer["counts"]["Organization"], 1);
    assert!(trailer["counts"].get("Goal").is_none());
    // Nor was the file fetched from this server under its id, as a path
    // rewritten onto the base would have been.
    assert_eq!(mock.get(|st| st.hits.get("Binary/bin-far").copied()), None);
}

#[test]
fn a_failing_binary_is_retried_in_place_and_fetched_once() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");

    // One 500 is retried at once and the export carries the file.
    mock.set(|st| {
        st.faults
            .push(mock_epic::Fault::failing("Binary/bin-pdf-1", 1))
    });
    let (values, _) = export_stream(&ctx, SELF_PATIENT, None);
    let trailer = values.last().unwrap();
    assert_eq!(trailer["counts"]["Binary"], 3);
    assert!(skips(trailer).iter().all(|(t, _, _)| t != "Binary"));

    // A file that keeps failing is one skip; doc-1 coming back from the
    // second DocumentReference search does not fetch it again.
    mock.set(|st| {
        st.hits.clear();
        st.faults
            .push(mock_epic::Fault::failing("Binary/bin-pdf-1", 100));
    });
    let (values, _) = export_stream(&ctx, SELF_PATIENT, None);
    let trailer = values.last().unwrap();
    assert_eq!(trailer["counts"]["Binary"], 2);
    let binary_skips: Vec<_> = skips(trailer)
        .into_iter()
        .filter(|(t, _, _)| t == "Binary")
        .collect();
    assert_eq!(
        binary_skips,
        [(
            "Binary".to_string(),
            "bin-pdf-1".to_string(),
            "upstream".to_string()
        )]
    );
    assert_eq!(mock.get(|st| st.hits["Binary/bin-pdf-1"]), 3);
    assert_eq!(mock.get(|st| st.hits["Binary/bin-html-1"]), 1);
}

#[test]
fn a_failed_first_grant_keeps_the_registered_install() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    mock.set(|st| st.refuse_jwt_bearer = true);
    login(&ctx, "adult1");
    // The registered client is kept, its spent initial token due, and the
    // refused first grant recorded.
    let s = stored(&ctx, SELF_PATIENT);
    assert!(s.dynamic_client.is_some());
    assert_eq!(s.tokens.expires_at, Some(0));
    assert!(s.refused_at.is_some());
    assert!(!session::usable(&s, session::now()));
    assert_eq!(mock.get(|st| st.registrations), 1);

    let c = ctx.contexts(&p()).unwrap().remove(0);
    let Err(e) = fresh(&ctx, &c) else {
        panic!("a refused grant returned a session")
    };
    assert_eq!(e.exit_code(), 3);
    assert!(stored(&ctx, SELF_PATIENT).refused_at.is_some());

    // A refusal is not final: the next use tries the grant once more.
    mock.set(|st| st.refuse_jwt_bearer = false);
    let renewed = fresh(&ctx, &c).unwrap();
    assert!(session::is_fresh(&renewed, session::now()));
    assert!(stored(&ctx, SELF_PATIENT).refused_at.is_none());
    assert_eq!(mock.get(|st| st.registrations), 1);
}

#[test]
fn a_revoked_token_is_renewed_once_and_the_export_goes_on() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let grants = mock.get(|st| st.jwt_bearer_grants);
    mock.set(|st| st.forget_tokens());
    let (values, _) = export_stream(&ctx, SELF_PATIENT, None);
    let trailer = values.last().unwrap();
    assert_eq!(trailer["complete"], true);
    assert_eq!(trailer["counts"]["Patient"], 1);
    assert_eq!(mock.get(|st| st.jwt_bearer_grants), grants + 1);
    assert!(stored(&ctx, SELF_PATIENT).refused_at.is_none());
}

/// An export of `patient` that may fail: the error, if any.
fn export_result(ctx: &Ctx, patient: &str) -> Result<(), CliError> {
    let c = ctx
        .contexts(&p())
        .unwrap()
        .into_iter()
        .find(|c| c.id == patient)
        .unwrap();
    let t = commands::transport().unwrap();
    let live = live(ctx, &c)?;
    let fhir = mychart::fhir::Fhir::new(&t, &live)?;
    let mut buf = Vec::new();
    let mut w =
        pk_med_records::RecordsWriter::start(&mut buf, &commands::records::header(&c, None))
            .unwrap();
    mychart::export::run(&fhir, &mut w, &c.id, None, &|_| {})
}

#[test]
fn a_refused_renewal_shows_in_status_and_its_token_is_not_sent_again() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    mock.set(|st| {
        st.forget_tokens();
        st.refuse_jwt_bearer = true;
    });
    let e = export_result(&ctx, SELF_PATIENT).unwrap_err();
    assert_eq!(e.exit_code(), 3, "{e}");

    let c = ctx.contexts(&p()).unwrap().remove(0);
    let s = stored(&ctx, SELF_PATIENT);
    assert!(s.refused_at.is_some());
    let row = commands::auth::patient_dto(&c, Some(&s));
    assert_eq!(row["status"]["authenticated"], false, "{row}");
    assert!(row["status"]["refused_at"].is_string(), "{row}");

    // The next export asks for a grant first (refused again, so none is
    // counted) and sends no FHIR request.
    let reads: usize = mock.get(|st| st.hits.values().sum());
    let grants = mock.get(|st| st.jwt_bearer_grants);
    let e = export_result(&ctx, SELF_PATIENT).unwrap_err();
    assert_eq!(e.exit_code(), 3);
    assert_eq!(mock.get(|st| st.hits.values().sum::<usize>()), reads);
    assert_eq!(mock.get(|st| st.jwt_bearer_grants), grants);
}

#[test]
fn a_401_to_a_just_renewed_token_ends_the_sign_in() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let grants = mock.get(|st| st.jwt_bearer_grants);
    mock.set(|st| {
        st.faults.push(mock_epic::Fault {
            path: format!("Patient/{SELF_PATIENT}"),
            status: 401,
            body: "{}".into(),
            times: 2,
            start: None,
        })
    });
    let e = export_result(&ctx, SELF_PATIENT).unwrap_err();
    assert_eq!(e.exit_code(), 3, "{e}");
    assert_eq!(mock.get(|st| st.jwt_bearer_grants), grants + 1);
    assert!(stored(&ctx, SELF_PATIENT).refused_at.is_some());
}

#[test]
fn the_registered_install_renews_even_when_a_refresh_token_is_held() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let c = ctx.contexts(&p()).unwrap().remove(0);
    let mut s = stored(&ctx, SELF_PATIENT);
    // A refresh token from the code exchange, issued to the app's client id.
    s.tokens.refresh_token = Some("issued-to-the-app".into());
    s.tokens.expires_at = Some(0);
    store(&ctx, &s);
    let before = mock.get(|st| st.jwt_bearer_grants);
    let renewed = fresh(&ctx, &c).unwrap();
    assert!(session::is_fresh(&renewed, session::now()));
    assert_eq!(mock.get(|st| st.jwt_bearer_grants), before + 1);
}

#[test]
fn a_patient_index_naming_another_server_is_refused() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let mut c = ctx.contexts(&p()).unwrap().remove(0);
    c.fhir_base = "https://elsewhere.example.org/api/FHIR/R4".into();
    ctx.update_profile(&p(), |s| s.upsert(c)).unwrap();
    let hits = mock.get(|st| st.hits.values().sum::<usize>());
    let Err(e) = commands::api::run(
        &ctx,
        &mychart::cli::ApiArgs {
            method: "GET".into(),
            path: format!("Patient/{SELF_PATIENT}"),
            who: mychart::cli::PatientArgs::default(),
        },
    ) else {
        panic!("a request went out under an edited index")
    };
    assert_eq!(e.exit_code(), 3);
    assert!(e.to_string().contains("patient index"), "{e}");
    assert_eq!(mock.get(|st| st.hits.values().sum::<usize>()), hits);
}

#[test]
fn an_organization_without_https_is_refused_before_the_network() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    let mut index = commands::orgs::index(&ctx, false).unwrap();
    let mut plain = index.orgs[0].clone();
    plain.id = "9000-9".into();
    plain.name = "Plain Example Clinic".into();
    plain.fhir_base = "http://fhir.example.org/api/FHIR/R4".into();
    plain.address = plain.fhir_base.clone();
    index.orgs.push(plain);
    mychart::fsutil::write_private(
        &mychart::orgs::index_path(&ctx.cache_dir),
        &serde_json::to_vec(&index).unwrap(),
    )
    .unwrap();
    let Err(e) = commands::auth::run(
        &ctx,
        &AuthCmd::Login(LoginArgs {
            org: "9000-9".into(),
            label: None,
            no_browser: true,
            timeout: 1,
        }),
    ) else {
        panic!("a sign-in started over plain http")
    };
    assert_eq!(e.exit_code(), 2);
    assert!(e.to_string().contains("only https"), "{e}");
}

#[test]
fn a_401_to_a_token_another_process_replaced_uses_the_new_one_without_a_grant() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let c = ctx.contexts(&p()).unwrap().remove(0);
    let held = stored(&ctx, SELF_PATIENT).tokens.access_token.clone();
    let grants = mock.get(|st| st.jwt_bearer_grants);
    let live = live(&ctx, &c).unwrap();

    // This process was refused an older token; the Keychain holds a newer one.
    let s = live.after_refusal("an-older-token").unwrap();
    assert_eq!(s.tokens.access_token, held);
    assert_eq!(mock.get(|st| st.jwt_bearer_grants), grants);

    // A 401 to that token too ends this run, but another process has stored
    // a newer one meanwhile, and that one is not marked.
    let mut newer = stored(&ctx, SELF_PATIENT);
    newer.tokens.access_token = "a-newer-token".into();
    store(&ctx, &newer);
    let Err(e) = live.after_refusal(&held) else {
        panic!("a 401 to the token renewed to returned a session")
    };
    assert_eq!(e.exit_code(), 3);
    assert!(e.to_string().contains("another process"), "{e}");
    let kept = stored(&ctx, SELF_PATIENT);
    assert!(kept.refused_at.is_none());
    assert!(session::usable(&kept, session::now()));
}

#[test]
fn a_401_with_no_way_to_renew_marks_the_sign_in_refused() {
    let mock = MockEpic::start();
    mock.set(|st| st.offer_registration = false);
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let s = stored(&ctx, SELF_PATIENT);
    assert!(!s.renewable());
    let c = ctx.contexts(&p()).unwrap().remove(0);

    let Err(e) = live(&ctx, &c)
        .unwrap()
        .after_refusal(&s.tokens.access_token)
    else {
        panic!("a refused token that cannot be renewed returned a session")
    };
    assert_eq!(e.exit_code(), 3);
    assert!(stored(&ctx, SELF_PATIENT).refused_at.is_some());

    // Refused and not renewable: refused before any request.
    let reads: usize = mock.get(|st| st.hits.values().sum());
    let Err(e) = fresh(&ctx, &c) else {
        panic!("a refused sign-in returned a session")
    };
    assert_eq!(e.exit_code(), 3);
    assert!(e.message().contains("has ended"), "{e}");
    assert!(e.to_string().contains("auth login"), "{e}");
    assert_eq!(mock.get(|st| st.hits.values().sum::<usize>()), reads);
}

#[test]
fn a_sign_in_stored_by_0_1_asks_for_a_new_one_and_logout_removes_it() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let c = ctx.contexts(&p()).unwrap().remove(0);
    // mychart 0.1's item: its own shape under the same account.
    ctx.vault
        .write(
            &mychart::vault::entry_name(&p(), &c.account()).unwrap(),
            r#"{"version":1,"access_token":"an-old-token","expires_at":0,"patient":"x"}"#,
        )
        .unwrap();

    let held = commands::auth::stored(&ctx, &p(), &c).unwrap();
    let row = commands::auth::status_row(&c, &held);
    assert_eq!(row["status"]["authenticated"], false, "{row}");
    assert!(row["status"]["unreadable"].is_string(), "{row}");
    assert_eq!(row["credential_in_keychain"], true);
    assert!(!row.to_string().contains("an-old-token"), "{row}");
    commands::auth::run(&ctx, &AuthCmd::Status).unwrap();

    let hits = mock.get(|st| st.hits.values().sum::<usize>());
    let e = export_result(&ctx, SELF_PATIENT).unwrap_err();
    assert_eq!(e.exit_code(), 3, "{e}");
    assert!(e.to_string().contains("auth login"), "{e}");
    assert!(!e.to_string().contains("an-old-token"), "{e}");
    assert_eq!(mock.get(|st| st.hits.values().sum::<usize>()), hits);

    commands::auth::run(
        &ctx,
        &AuthCmd::Logout(LogoutArgs {
            org: None,
            patient: None,
            forget: false,
        }),
    )
    .unwrap();
    assert!(ctx
        .vault
        .read(&mychart::vault::entry_name(&p(), &c.account()).unwrap())
        .unwrap()
        .is_none());
}

#[test]
fn an_id_token_without_this_sign_ins_nonce_is_refused_and_nothing_is_stored() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    mock.set(|st| st.wrong_nonce = true);
    let e = commands::auth::run(
        &ctx,
        &AuthCmd::Login(LoginArgs {
            org: ORG_ID.into(),
            label: Some("adult1".into()),
            no_browser: false,
            timeout: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(e.exit_code(), 3, "{e}");
    assert!(e.to_string().contains("nonce"), "{e}");
    // Refused at the exchange: no install registered, nothing kept.
    assert_eq!(mock.get(|st| (st.nonces, st.registrations)), (1, 0));
    assert!(ctx.contexts(&p()).unwrap().is_empty());
    assert!(sessions(&ctx)
        .load(&session::account(ORG_ID, SELF_PATIENT))
        .unwrap()
        .is_none());
}

#[test]
fn a_search_page_that_fails_keeps_the_pages_before_it_and_is_one_skip() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    // The laboratory search has three results, two to a page: its second
    // page is refused.
    mock.set(|st| {
        st.faults
            .push(mock_epic::Fault::on_page("Observation", 2, 400))
    });
    let (values, _) = export_stream(&ctx, SELF_PATIENT, None);
    let observations: Vec<&str> = values
        .iter()
        .filter(|v| v["resourceType"] == "Observation")
        .filter_map(|v| v["id"].as_str())
        .collect();
    assert!(
        observations.contains(&"obs-lab-1") && observations.contains(&"obs-lab-2"),
        "{observations:?}"
    );
    assert!(!observations.contains(&"obs-lab-3"), "{observations:?}");
    let trailer = values.last().unwrap();
    let laboratory: Vec<&serde_json::Value> = trailer["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| {
            s["search"]
                .as_str()
                .is_some_and(|q| q.contains("laboratory"))
        })
        .collect();
    assert_eq!(laboratory.len(), 1, "{trailer}");
    assert_eq!(laboratory[0]["code"], "unsupported");
    assert!(
        laboratory[0]["reason"]
            .as_str()
            .unwrap()
            .starts_with("page 2 failed, after 1 that are kept"),
        "{}",
        laboratory[0]
    );
    // A refused search is a skip, not the end of the export.
    assert_eq!(trailer["complete"], true);
}

/// `records export -o <out>` for the signed-in adult, as the command runs
/// it, short of printing and exiting.
fn export_dir(ctx: &Ctx, out: &Path) -> commands::records::DirExport {
    let c = ctx
        .contexts(&p())
        .unwrap()
        .into_iter()
        .find(|c| c.id == SELF_PATIENT)
        .unwrap();
    let t = commands::transport().unwrap();
    let live = live(ctx, &c).unwrap();
    let fhir = mychart::fhir::Fhir::new(&t, &live).unwrap();
    let file = pk_med_records::create_export_file(out).unwrap();
    commands::records::export_to_dir(
        file,
        &fhir,
        &commands::records::header(&c, None),
        &c.id,
        None,
        &|_| {},
    )
    .unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut n: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    n.sort();
    n
}

#[test]
fn an_export_to_a_directory_cut_short_keeps_the_last_complete_one() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let out = dir.path().join("export");
    let first = export_dir(&ctx, &out);
    assert!(first.error.is_none() && first.finished.trailer.complete);
    assert_eq!(first.path, out.join("records.ndjson"));
    assert!(first.kept.is_none());
    let before = std::fs::read(out.join("records.ndjson")).unwrap();

    // Partway through, the token is refused, and so is its renewal.
    mock.set(|st| {
        st.refuse_jwt_bearer = true;
        st.faults.push(mock_epic::Fault {
            path: "Condition".into(),
            status: 401,
            body: "{}".into(),
            times: 1,
            start: None,
        });
    });
    let cut = export_dir(&ctx, &out);
    assert_eq!(cut.error.as_ref().map(CliError::exit_code), Some(3));
    assert!(!cut.finished.trailer.complete);
    assert_eq!(cut.path, out.join("records.incomplete.ndjson"));
    assert_eq!(cut.kept, Some(out.join("records.ndjson")));
    assert_eq!(std::fs::read(out.join("records.ndjson")).unwrap(), before);
    assert_eq!(
        names(&out),
        ["records.incomplete.ndjson", "records.ndjson"],
        "and no temporary file"
    );
    // What was read before the refusal is there, and the trailer says the
    // stream is cut short.
    let text = std::fs::read_to_string(&cut.path).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(lines
        .iter()
        .any(|v| v["resourceType"] == "Patient" && v["id"] == SELF_PATIENT));
    assert_eq!(lines.last().unwrap()["complete"], false);

    // The next complete export replaces records.ndjson and removes the
    // incomplete one.
    mock.set(|st| st.refuse_jwt_bearer = false);
    login(&ctx, "adult1");
    let again = export_dir(&ctx, &out);
    assert!(again.error.is_none() && again.kept.is_none());
    assert_eq!(names(&out), ["records.ndjson"]);
}

#[cfg(unix)]
#[test]
fn an_export_to_a_directory_refuses_a_symbolic_link_before_the_network() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let out = dir.path().join("export");
    std::fs::create_dir(&out).unwrap();
    let elsewhere = dir.path().join("elsewhere.ndjson");
    std::fs::write(&elsewhere, "not an export\n").unwrap();
    std::os::unix::fs::symlink(&elsewhere, out.join("records.ndjson")).unwrap();
    let requests = mock.get(|st| (st.hits.values().sum::<usize>(), st.jwt_bearer_grants));
    let e = commands::records::run(
        &ctx,
        &mychart::cli::RecordsCmd::Export(mychart::cli::ExportArgs {
            who: mychart::cli::PatientArgs {
                patient: Some("adult1".into()),
                org: None,
            },
            since: None,
            ndjson: false,
            output: Some(out.clone()),
        }),
    )
    .unwrap_err();
    assert_eq!(e.exit_code(), 2, "{e}");
    assert!(e.to_string().contains("symbolic link"), "{e}");
    assert_eq!(
        std::fs::read_to_string(&elsewhere).unwrap(),
        "not an export\n"
    );
    assert!(std::fs::symlink_metadata(out.join("records.ndjson"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(names(&out), ["records.ndjson"]);
    assert_eq!(
        mock.get(|st| (st.hits.values().sum::<usize>(), st.jwt_bearer_grants)),
        requests,
        "refused before any request"
    );
}

#[test]
fn a_token_answer_without_expires_in_lasts_an_hour_and_shows_when() {
    for registered in [false, true] {
        let mock = MockEpic::start();
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx(dir.path(), &mock, |c| {
            c.dynamic_registration = Some(registered)
        });
        mock.set(|st| st.omit_expires_in = true);
        login(&ctx, "adult1");
        // Without registration the sign-in's own token is kept; with it,
        // the first JWT-bearer grant's.
        let s = stored(&ctx, SELF_PATIENT);
        assert_eq!(s.dynamic_client.is_some(), registered);
        assert_eq!(
            s.tokens.expires_at,
            Some(s.tokens.obtained_at + session::UNSTATED_LIFETIME_SECS),
            "registered: {registered}"
        );
        let c = ctx.contexts(&p()).unwrap().pop().unwrap();
        let row = commands::auth::patient_dto(&c, Some(&s));
        assert!(row["status"]["expires_at"].is_string(), "{row}");
        assert_eq!(row["status"]["authenticated"], true);
    }
}

/// Epic's unconstrained ids run past FHIR's 64 characters: a sign-in keeps
/// such a patient, reads it, and exports it.
#[test]
fn a_patient_id_past_64_characters_signs_in_and_exports() {
    use serde_json::json;
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    let long = format!("eLong{}", "x".repeat(195));
    mock.set(|st| {
        st.grant_patient = Some(long.clone());
        let patient = json!({"resourceType": "Patient", "id": long});
        let allergy = json!({
            "resourceType": "AllergyIntolerance",
            "id": "allergy-long-1",
            "patient": {"reference": format!("Patient/{long}")}
        });
        for r in [patient, allergy] {
            let kind = r["resourceType"].as_str().unwrap().to_string();
            let text = r.to_string();
            st.resources.entry(kind).or_default().push((r, text));
        }
    });
    login(&ctx, "adult1");
    let s = stored(&ctx, &long);
    assert_eq!(s.patient.as_deref(), Some(long.as_str()));
    let (values, _) = export_stream(&ctx, &long, None);
    let trailer = values.last().unwrap();
    assert_eq!(trailer["complete"], true, "{trailer}");
    assert_eq!(trailer["counts"]["Patient"], 1, "{trailer}");
    assert_eq!(trailer["counts"]["AllergyIntolerance"], 1, "{trailer}");
}

/// A granted patient id that is not a FHIR id (`..` would climb a request
/// path) is refused, and nothing is kept.
#[test]
fn a_granted_patient_id_that_is_not_a_fhir_id_is_refused() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    mock.set(|st| st.grant_patient = Some("..".into()));
    let e = commands::auth::run(
        &ctx,
        &AuthCmd::Login(LoginArgs {
            org: ORG_ID.into(),
            label: Some("adult1".into()),
            no_browser: false,
            timeout: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(e.exit_code(), 5, "{e}");
    assert!(e.to_string().contains("not a FHIR id"), "{e}");
    assert!(ctx.contexts(&p()).unwrap().is_empty());
    assert_eq!(mock.get(|st| st.registrations), 0);
}

/// References written absolute on this server, or with a `_history`
/// version, are followed like relative ones. One on another server is not
/// read as this server's: the mock holds a `Location/loc-far` that must not
/// reach the stream. (Reads by reference go only to this server's base, so
/// the sign-in cannot travel to another host this way; the unit test in
/// src/export.rs covers the base comparison.)
#[test]
fn absolute_and_versioned_references_on_this_server_are_read() {
    use serde_json::json;
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let base = mock.fhir_base();
    mock.set(|st| {
        let me = json!({"reference": format!("Patient/{SELF_PATIENT}")});
        let goal = json!({
            "resourceType": "Goal", "id": "goal-refs", "subject": me,
            "expressedBy": {"reference": format!("{base}/Practitioner/prac-abs")},
            "addresses": [{"reference": "Organization/org-v/_history/2"}],
            "outcomeReference": [{"reference": "https://elsewhere.example.org/R4/Location/loc-far"}]
        });
        let prac = json!({"resourceType": "Practitioner", "id": "prac-abs"});
        let org = json!({"resourceType": "Organization", "id": "org-v"});
        // What a misread of the other server's reference would fetch here.
        let far = json!({"resourceType": "Location", "id": "loc-far"});
        for r in [goal, prac, org, far] {
            let kind = r["resourceType"].as_str().unwrap().to_string();
            let text = r.to_string();
            st.resources.entry(kind).or_default().push((r, text));
        }
    });
    let (values, _) = export_stream(&ctx, SELF_PATIENT, None);
    let ids: Vec<String> = values
        .iter()
        .filter_map(|v| {
            Some(format!(
                "{}/{}",
                v["resourceType"].as_str()?,
                v["id"].as_str()?
            ))
        })
        .collect();
    for want in [
        "Goal/goal-refs",
        "Practitioner/prac-abs",
        "Organization/org-v",
    ] {
        assert!(ids.iter().any(|i| i == want), "{want} missing from {ids:?}");
    }
    assert!(
        !ids.iter().any(|i| i == "Location/loc-far"),
        "the other server's Location was read here: {ids:?}"
    );
    assert_eq!(
        mock.get(|st| st.hits.get("Location/loc-far").copied()),
        None
    );
    assert_eq!(values.last().unwrap()["complete"], true);
}

/// A reference whose id FHIR does not allow is one skip in the trailer,
/// naming the type and never the id, and is not requested.
#[test]
fn a_reference_with_an_id_fhir_does_not_allow_is_a_skip() {
    use serde_json::json;
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    mock.set(|st| {
        let me = json!({"reference": format!("Patient/{SELF_PATIENT}")});
        let goal = json!({
            "resourceType": "Goal", "id": "goal-bad-ref", "subject": me,
            "expressedBy": {"reference": "Practitioner/has space"},
            "addresses": [{"reference": "Practitioner/has space"}]
        });
        let text = goal.to_string();
        st.resources
            .entry("Goal".into())
            .or_default()
            .push((goal, text));
    });
    let (values, lines) = export_stream(&ctx, SELF_PATIENT, None);
    let trailer = values.last().unwrap();
    assert_eq!(trailer["complete"], true);
    let refused: Vec<_> = skips(trailer)
        .into_iter()
        .filter(|(t, _, code)| t == "Practitioner" && code == "invalid_reference")
        .collect();
    assert_eq!(
        refused,
        [(
            "Practitioner".into(),
            String::new(),
            "invalid_reference".into()
        )]
    );
    assert!(!lines.last().unwrap().contains("has space"));
    assert_eq!(
        mock.get(|st| st.hits.keys().filter(|k| k.contains("has")).count()),
        0
    );
}

/// A command that uses a sign-in names its profile: none, or one not
/// configured, is exit 2 naming the configured ones, before the vault or
/// the organization is reached.
#[test]
fn a_command_without_its_profile_is_refused_before_the_vault_or_the_network() {
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = ctx(dir.path(), &mock, |_| {});
    login(&ctx, "adult1");
    let hits = mock.get(|st| st.hits.values().sum::<usize>());
    for profile in [None, Some("acct9")] {
        ctx.profile = profile.map(str::to_string);
        let refusals = [
            commands::auth::run(&ctx, &AuthCmd::Status).unwrap_err(),
            commands::patients::run(&ctx, &PatientsCmd::List { status: true }).unwrap_err(),
            commands::api::run(
                &ctx,
                &mychart::cli::ApiArgs {
                    method: "GET".into(),
                    path: format!("Patient/{SELF_PATIENT}"),
                    who: mychart::cli::PatientArgs::default(),
                },
            )
            .unwrap_err(),
            commands::auth::run(
                &ctx,
                &AuthCmd::Login(LoginArgs {
                    org: ORG_ID.into(),
                    label: None,
                    no_browser: true,
                    timeout: 1,
                }),
            )
            .unwrap_err(),
        ];
        for e in refusals {
            assert_eq!(e.exit_code(), 2, "{e}");
            assert!(e.to_string().contains("acct1"), "{e}");
        }
    }
    assert_eq!(mock.get(|st| st.hits.values().sum::<usize>()), hits);
}

/// The sign-in opens in the profile's own window, with no page before it
/// at an organization that is not Kaiser Permanente's, and the window is
/// closed once the answer has arrived.
#[test]
fn a_sign_in_opens_the_profiles_window_and_closes_it_when_the_answer_arrives() {
    use std::sync::Mutex;
    struct Closes(Arc<Mutex<bool>>);
    impl Drop for Closes {
        fn drop(&mut self) {
            *self.0.lock().unwrap() = true;
        }
    }
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = ctx(dir.path(), &mock, |_| {});
    let opened: Arc<Mutex<Vec<(String, usize)>>> = Arc::default();
    let closed: Arc<Mutex<bool>> = Arc::default();
    let (o, c) = (opened.clone(), closed.clone());
    ctx.window = Box::new(move |w: &commands::Opening<'_>| {
        o.lock()
            .unwrap()
            .push((w.profile.to_string(), w.before.len()));
        browser(w.address);
        Ok(Box::new(Closes(c.clone())) as commands::Window)
    });
    ctx.browser = Box::new(|_: &str| panic!("the default browser opens only without the window"));
    login(&ctx, "adult1");
    assert_eq!(*opened.lock().unwrap(), [("acct1".to_string(), 0)]);
    assert!(*closed.lock().unwrap(), "the window outlived the sign-in");
    assert_eq!(ctx.contexts(&p()).unwrap().len(), 1);
}

/// mychart 0.4's sign-ins (a Keychain item each, listed in contexts.json)
/// move into a profile with no request to the organization; a second run
/// finds nothing left, and the moved sign-in exports.
#[test]
fn profiles_migrate_moves_the_sign_ins_mychart_0_4_kept() {
    use pk_cli_auth::vault::KeyStore;
    use pk_cli_secrets::Secret;
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = ctx(dir.path(), &mock, |_| {});
    let keys = Arc::new(MemoryKeys::new());
    ctx.vault = Arc::new(pk_cli_auth::vault::Vault::new(
        keys.clone(),
        dir.path().join("vault"),
    ));
    let legacy = Arc::new(MemoryKeys::new());
    ctx.legacy = legacy.clone();
    login(&ctx, "adult1");
    // Make it 0.4's: the item in the Keychain, the row in contexts.json.
    let c = ctx.contexts(&p()).unwrap().remove(0);
    let text = sessions(&ctx).read(&c.account()).unwrap().unwrap();
    legacy.set(&c.account(), &Secret::new(text)).unwrap();
    sessions(&ctx).delete(&c.account()).unwrap();
    ctx.update_profile(&p(), |s| s.remove(&c.organization, &c.id))
        .unwrap();
    let index = session::Index::beside(&ctx.store.path().unwrap());
    index.save(std::slice::from_ref(&c)).unwrap();
    let hits = mock.get(|st| st.hits.values().sum::<usize>());

    let migrate = |ctx: &Ctx| {
        commands::profiles::run(
            ctx,
            &mychart::cli::ProfilesCmd::Migrate {
                name: p().to_string(),
                org: None,
                patient: vec![],
            },
        )
    };
    migrate(&ctx).unwrap();
    assert_eq!(mock.get(|st| st.hits.values().sum::<usize>()), hits);
    assert!(legacy.get(&c.account()).unwrap().is_none());
    assert!(!index.path().exists());
    let moved = ctx.contexts(&p()).unwrap();
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].id, c.id);
    assert_eq!(moved[0].label.as_deref(), Some("adult1"));

    migrate(&ctx).unwrap();
    assert_eq!(ctx.contexts(&p()).unwrap().len(), 1);
    export_result(&ctx, SELF_PATIENT).unwrap();
}

/// `profiles remove` asks first (exit 6 when it cannot), then deletes the
/// profile's sign-ins, its browser profile and the profile.
#[test]
fn profiles_remove_takes_the_sign_ins_and_the_browser_profile_with_it() {
    use std::sync::Mutex;
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = ctx(dir.path(), &mock, |_| {});
    // The browser profile a sign-in in acct1 would have left, under the
    // test's own root: pk-cli-browser removes it there, never the owner's.
    let root = dir.path().join("data");
    let kept = root.join("piekstra.mychart").join("browser-profile-acct1");
    std::fs::create_dir_all(&kept).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert!(pk_cli_browser::named_profile_exists_in(&root, "mychart", "acct1").unwrap());
    let forgot: Arc<Mutex<Vec<String>>> = Arc::default();
    let (f, r) = (forgot.clone(), root.clone());
    ctx.forget_browser = Box::new(move |name: &ProfileName| {
        f.lock().unwrap().push(name.to_string());
        pk_cli_browser::forget_named_profile_in(&r, "mychart", name.as_str())
            .map_err(CliError::from)
    });
    login(&ctx, "adult1");
    let account = ctx.contexts(&p()).unwrap()[0].account();
    let remove = |force| mychart::cli::ProfilesCmd::Remove {
        name: p().to_string(),
        force,
    };
    let e = commands::profiles::run(&ctx, &remove(false)).unwrap_err();
    assert_eq!(e.exit_code(), 6, "{e}");
    assert!(sessions(&ctx).load(&account).unwrap().is_some());

    assert!(
        kept.exists(),
        "a refused removal removed the browser profile"
    );
    commands::profiles::run(&ctx, &remove(true)).unwrap();
    assert!(sessions(&ctx).load(&account).unwrap().is_none());
    assert_eq!(*forgot.lock().unwrap(), ["acct1"]);
    assert!(!kept.exists());
    assert!(!ctx.config().unwrap().profiles.contains(&p()));
    let e = commands::profiles::run(&ctx, &remove(true)).unwrap_err();
    assert_eq!(e.exit_code(), 4, "{e}");
}

/// A profile is one login: rows of 0.4's index from two logins are refused
/// before any Keychain read, `--org` moves one login's, and the other's stay
/// in `contexts.json` for its own profile, which empties it.
#[test]
fn profiles_migrate_moves_one_login_and_leaves_the_others() {
    use pk_cli_auth::vault::KeyStore;
    use pk_cli_secrets::Secret;
    let mock = MockEpic::start();
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = ctx(dir.path(), &mock, |_| {});
    let keys = Arc::new(MemoryKeys::new());
    ctx.vault = Arc::new(pk_cli_auth::vault::Vault::new(
        keys.clone(),
        dir.path().join("vault"),
    ));
    let legacy = Arc::new(MemoryKeys::new());
    ctx.legacy = legacy.clone();
    login(&ctx, "adult1");
    let a = ctx.contexts(&p()).unwrap().remove(0);
    let text = sessions(&ctx).read(&a.account()).unwrap().unwrap();
    sessions(&ctx).delete(&a.account()).unwrap();
    ctx.update_profile(&p(), |s| s.remove(&a.organization, &a.id))
        .unwrap();
    // Another login, at another organization: invented.
    let b = session::Context {
        id: "pb".into(),
        organization: "9999".into(),
        organization_name: "Other Health".into(),
        label: Some("adult2".into()),
        fhir_user: Some("Patient/pb".into()),
        ..a.clone()
    };
    for c in [&a, &b] {
        legacy
            .set(&c.account(), &Secret::new(text.clone()))
            .unwrap();
    }
    let index = session::Index::beside(&ctx.store.path().unwrap());
    index.save(&[a.clone(), b.clone()]).unwrap();
    let migrate = |name: &str, org: Option<&str>, patient: &[&str]| {
        commands::profiles::run(
            &ctx,
            &mychart::cli::ProfilesCmd::Migrate {
                name: name.into(),
                org: org.map(str::to_string),
                patient: patient.iter().map(|p| p.to_string()).collect(),
            },
        )
    };

    let key_reads = keys.reads();
    let e = migrate("acct1", None, &[]).unwrap_err();
    assert_eq!(e.exit_code(), 2, "{e}");
    assert!(e.to_string().contains("2 logins"), "{e}");
    assert_eq!((legacy.reads(), keys.reads()), (0, key_reads));
    assert_eq!(index.load().unwrap().len(), 2);
    assert_eq!(
        migrate("acct1", None, &["nobody"]).unwrap_err().exit_code(),
        4
    );
    assert_eq!(
        migrate("acct1", Some("8888"), &[]).unwrap_err().exit_code(),
        4
    );

    migrate("acct1", Some(ORG_ID), &[]).unwrap();
    assert_eq!(ctx.contexts(&p()).unwrap()[0].id, a.id);
    assert!(legacy.get(&a.account()).unwrap().is_none());
    assert_eq!(index.load().unwrap(), std::slice::from_ref(&b));
    assert!(legacy.get(&b.account()).unwrap().is_some());

    migrate("acct2", Some("Other Health"), &["adult2"]).unwrap();
    let acct2 = ProfileName::parse("acct2").unwrap();
    assert_eq!(ctx.contexts(&acct2).unwrap()[0].id, "pb");
    assert_eq!(ctx.contexts(&p()).unwrap().len(), 1);
    assert!(!index.path().exists());
}
