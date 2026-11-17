//! Black-box tests of the binary's surface. None reaches the Keychain: the
//! service is an empty test one, the config and cache are temp dirs, and
//! every case either renders help or info, works on the empty index, or is
//! refused by argument checks before any credential read. The directory
//! commands run against the mock organization on 127.0.0.1.

#[path = "support/mock_epic.rs"]
mod mock_epic;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

const PUBLISHED: &str = mychart::config::PUBLISHED_REDIRECT;

/// A fresh scratch dir per call. The counter, not the clock, makes it
/// unique: parallel tests read the same instant often enough on macOS.
fn tmp() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mychart-cli-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mychart(home: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("mychart").unwrap();
    cmd.env(
        "MYCHART_KEYCHAIN_SERVICE",
        "piekstra.mychart.hermetic-tests",
    )
    .env("XDG_CONFIG_HOME", home.join("config"))
    .env("XDG_CACHE_HOME", home.join("cache"))
    .env("MYCHART_BROWSER", "/usr/bin/false")
    .env_remove("MYCHART_CONFIG")
    .env_remove("MYCHART_PROFILE")
    .env_remove("NO_COLOR");
    cmd
}

/// [`mychart`] as the profile `acct1`, which it adds first.
fn as_acct1(home: &std::path::Path) -> Command {
    mychart(home)
        .args(["profiles", "add", "acct1"])
        .assert()
        .success();
    let mut cmd = mychart(home);
    cmd.env("MYCHART_PROFILE", "acct1");
    cmd
}

fn json_of(out: &[u8]) -> Value {
    serde_json::from_slice(out).unwrap_or_else(|e| {
        panic!(
            "not one JSON document ({e}): {}",
            String::from_utf8_lossy(out)
        )
    })
}

#[test]
fn every_command_renders_help() {
    let home = tmp();
    for args in [
        vec!["--help"],
        vec!["orgs", "--help"],
        vec!["orgs", "refresh", "--help"],
        vec!["orgs", "list", "--help"],
        vec!["orgs", "search", "--help"],
        vec!["auth", "--help"],
        vec!["auth", "login", "--help"],
        vec!["auth", "status", "--help"],
        vec!["auth", "logout", "--help"],
        vec!["patients", "--help"],
        vec!["patients", "list", "--help"],
        vec!["records", "--help"],
        vec!["records", "export", "--help"],
        vec!["documents", "--help"],
        vec!["documents", "list", "--help"],
        vec!["documents", "download", "--help"],
        vec!["api", "--help"],
        vec!["config", "--help"],
        vec!["config", "path", "--help"],
        vec!["config", "show", "--help"],
        vec!["config", "set", "--help"],
        vec!["config", "unset", "--help"],
        vec!["auth", "probe", "--help"],
        vec!["profiles", "--help"],
        vec!["profiles", "list", "--help"],
        vec!["profiles", "add", "--help"],
        vec!["profiles", "remove", "--help"],
        vec!["profiles", "migrate", "--help"],
        vec!["self-update", "--help"],
        vec!["completions", "--help"],
        vec!["info", "--help"],
    ] {
        mychart(&home).args(&args).assert().success();
    }
}

#[test]
fn version_and_completions() {
    let home = tmp();
    mychart(&home)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
    for shell in ["bash", "zsh", "fish"] {
        mychart(&home)
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("mychart"));
    }
}

#[test]
fn info_declares_both_profiles() {
    let home = tmp();
    let out = mychart(&home)
        .args(["info", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["schema"], "cli-info/v1");
    assert_eq!(v["name"], "mychart");
    let profiles: Vec<&str> = v["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        profiles.contains(&"health-records/v1") && profiles.contains(&"documents/v1"),
        "{profiles:?}"
    );
    assert_eq!(v["auth"]["method"], "browser-session");
    // Login commands take --profile, which a records store checks for.
    assert!(v["capabilities"]
        .as_array()
        .unwrap()
        .contains(&Value::from(pk_med_records::LOGIN_PROFILES)));
}

#[test]
fn nothing_signed_in_reads_as_such() {
    let home = tmp();
    let out = as_acct1(&home)
        .args(["auth", "status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["schema"], "auth-status/v1");
    assert_eq!(v["authenticated"], false);
    assert_eq!(v["patients"], serde_json::json!([]));

    let out = as_acct1(&home)
        .args(["patients", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["schema"], "patient-list/v1");
    assert_eq!(v["items"], serde_json::json!([]));

    as_acct1(&home)
        .args(["records", "export", "--ndjson"])
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("auth login"));
    for args in [
        vec!["records", "export", "-o", "x", "--json"],
        vec!["documents", "list", "--json"],
        vec!["api", "GET", "Patient/x", "--json"],
    ] {
        let out = as_acct1(&home)
            .args(&args)
            .assert()
            .code(3)
            .get_output()
            .stdout
            .clone();
        assert!(
            json_of(&out)["error"]["message"]
                .as_str()
                .unwrap()
                .contains("auth login"),
            "{args:?}"
        );
    }
    let out = as_acct1(&home)
        .args(["auth", "logout", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(
        (&v["settings_cleared"], &v["profiles_kept"]),
        (&Value::Bool(false), &Value::Bool(true))
    );
}

#[test]
fn bad_arguments_are_refused_before_anything_else() {
    let home = tmp();
    for (args, code) in [
        (vec!["records", "export"], 2),
        (
            vec!["records", "export", "--ndjson", "--since", "2026-13-01"],
            2,
        ),
        (vec!["records", "export", "--ndjson", "-o", "x"], 2),
        (vec!["records", "export", "--ndjson", "--json"], 2),
        (vec!["api", "POST", "Patient"], 2),
        (vec!["api", "DELETE", "Patient/x"], 2),
        (
            vec!["auth", "login", "--org", "sandbox", "--label", "has space"],
            2,
        ),
        (
            vec!["auth", "login", "--org", "sandbox", "--timeout", "0"],
            2,
        ),
        (vec!["auth", "login", "--org", "sandbox"], 2),
        (vec!["documents", "download"], 2),
        (
            vec![
                "documents",
                "list",
                "--since",
                "2026-02-01",
                "--until",
                "2026-01-01",
            ],
            2,
        ),
        (vec!["config", "set", "redirect_port", "80"], 2),
        (vec!["config", "set", "redirect_host", "example.com"], 2),
        (
            vec![
                "config",
                "set",
                "redirect_uri",
                "http://pages.example.org/cb",
            ],
            2,
        ),
        (
            vec![
                "config",
                "set",
                "directory_url",
                "http://example.com/brands",
            ],
            2,
        ),
        (vec!["config", "set", "no_such_key", "x"], 2),
        (vec!["auth", "probe"], 2),
        (vec!["orgs", "list", "--state", "XYZ"], 2),
        (vec!["orgs", "list", "--limit", "0"], 2),
        (vec!["nonsense"], 2),
    ] {
        as_acct1(&home).args(&args).assert().code(code);
    }
    let out = as_acct1(&home)
        .args(["auth", "login", "--org", "sandbox", "--json"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["error"]["code"], "usage");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("sandbox_client_id"));
}

#[test]
fn config_round_trips() {
    let home = tmp();
    as_acct1(&home)
        .args([
            "config",
            "set",
            "client_id",
            "00000000-0000-0000-0000-000000000001",
        ])
        .assert()
        .success();
    as_acct1(&home)
        .args(["config", "set", "redirect_port", "8123"])
        .assert()
        .success();
    // The published page forwards to 7468 only: with another port it is
    // refused, and the file keeps what it had.
    let refused = as_acct1(&home)
        .args(["config", "set", "redirect_uri", PUBLISHED])
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&refused).contains("8123"));
    as_acct1(&home)
        .args([
            "config",
            "set",
            "redirect_uri",
            "https://pages.example.org/callback",
        ])
        .assert()
        .success();
    let out = as_acct1(&home)
        .args(["config", "show", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["client_id"], "00000000-0000-0000-0000-000000000001");
    assert_eq!(v["redirect_port"], 8123);
    assert_eq!(v["redirect_uri"], "https://pages.example.org/callback");
    as_acct1(&home)
        .args(["config", "unset", "redirect_uri"])
        .assert()
        .success();
    // In the other order: the page at 7468, then another port.
    as_acct1(&home)
        .args(["config", "unset", "redirect_port"])
        .assert()
        .success();
    as_acct1(&home)
        .args(["config", "set", "redirect_uri", PUBLISHED])
        .assert()
        .success();
    as_acct1(&home)
        .args(["config", "set", "redirect_port", "8123"])
        .assert()
        .code(2);
    let out = as_acct1(&home)
        .args(["config", "show", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["redirect_uri"], PUBLISHED);
    assert!(v.get("redirect_port").is_none(), "{v}");
    as_acct1(&home)
        .args(["config", "unset", "redirect_uri"])
        .assert()
        .success();
    as_acct1(&home)
        .args(["config", "unset", "redirect_port"])
        .assert()
        .success();
    let out = as_acct1(&home)
        .args(["config", "path", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(json_of(&out)["path"]
        .as_str()
        .unwrap()
        .ends_with("mychart/config.json"));
    let out = as_acct1(&home)
        .args(["auth", "logout", "--forget", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(
        (
            &v["settings_cleared"],
            &v["profiles_kept"],
            &v["config_cleared"]
        ),
        (&Value::Bool(true), &Value::Bool(true), &Value::Bool(true))
    );
    let out = as_acct1(&home)
        .args(["config", "show", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(json_of(&out).get("client_id").is_none());
}

#[test]
fn the_directory_downloads_once_and_resolves_organizations() {
    let mock = mock_epic::MockEpic::start();
    let home = tmp();
    mychart(&home)
        .args(["config", "set", "directory_url", &mock.directory_url()])
        .assert()
        .success();
    let out = mychart(&home)
        .args(["orgs", "refresh", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["schema"], "org-refresh/v1");
    assert_eq!(v["organizations"], 1);
    assert_eq!(v["refreshed"], true);
    let out = mychart(&home)
        .args(["orgs", "refresh", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(json_of(&out)["refreshed"], false);

    let out = mychart(&home)
        .args(["orgs", "search", "pediatrics", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["schema"], "org-list/v1");
    assert_eq!(v["items"][0]["id"], mock_epic::ORG_ID);
    assert_eq!(v["items"][0]["vendor"], "epic");
    assert!(v["items"][0]["matched"]
        .as_str()
        .unwrap()
        .contains("Pediatrics"));

    let out = mychart(&home)
        .args(["orgs", "list", "--json", "--state", "ny"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ids: Vec<String> = json_of(&out)["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&mock_epic::ORG_ID.to_string()));
    mychart(&home)
        .args(["orgs", "search", "no such clinic anywhere"])
        .assert()
        .code(4);
    mychart(&home)
        .args(["orgs", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains(mock_epic::ORG_NAME));
}

#[test]
fn an_unreachable_directory_is_an_upstream_error() {
    let home = tmp();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    mychart(&home)
        .args([
            "config",
            "set",
            "directory_url",
            &format!("http://127.0.0.1:{port}/Endpoints/Brands"),
        ])
        .assert()
        .success();
    mychart(&home).args(["orgs", "list"]).assert().code(5);
}

#[test]
fn no_color_is_accepted() {
    let home = tmp();
    as_acct1(&home)
        .env("NO_COLOR", "1")
        .args(["patients", "list"])
        .assert()
        .success();
}

/// A command that uses a sign-in needs `--profile` (or MYCHART_PROFILE):
/// without one it is exit 2, naming the remedy or the configured profiles,
/// before the Keychain or the network.
#[test]
fn a_login_command_needs_its_profile() {
    let home = tmp();
    let out = mychart(&home)
        .args(["auth", "status", "--json"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let v = json_of(&out);
    assert_eq!(v["error"]["code"], "usage");
    assert!(v.to_string().contains("profiles add"), "{v}");
    as_acct1(&home);
    for args in [
        vec!["auth", "status"],
        vec!["auth", "logout"],
        vec!["patients", "list"],
        vec!["records", "export", "--ndjson"],
        vec!["documents", "list"],
        vec!["api", "GET", "Patient/x"],
        vec!["auth", "login", "--org", "sandbox"],
    ] {
        mychart(&home)
            .args(&args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("configured: acct1"));
        mychart(&home)
            .args(&args)
            .args(["--profile", "acct9"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("no profile `acct9`"));
    }
    // Commands that use no sign-in take none.
    mychart(&home).args(["orgs", "--help"]).assert().success();
    mychart(&home).args(["config", "path"]).assert().success();
    mychart(&home).args(["info"]).assert().success();
}

#[test]
fn profiles_round_trip() {
    let home = tmp();
    let list = |home: &std::path::Path| {
        json_of(
            &mychart(home)
                .args(["profiles", "list", "--json"])
                .assert()
                .success()
                .get_output()
                .stdout
                .clone(),
        )
    };
    let v = list(&home);
    assert_eq!(v["schema"], "profile-list/v1");
    assert_eq!(v["profiles"], serde_json::json!([]));
    for bad in ["Has Space", "UPPER", "", "-lead", &"a".repeat(33)] {
        mychart(&home)
            .args(["profiles", "add", bad])
            .assert()
            .code(2);
    }
    for name in ["adult2-org", "acct1"] {
        mychart(&home)
            .args(["profiles", "add", name])
            .assert()
            .success();
    }
    let v = list(&home);
    assert_eq!(
        v["profiles"],
        serde_json::json!([{"name": "acct1"}, {"name": "adult2-org"}])
    );
    // `config show` names the profiles and counts their patients, nothing
    // more.
    let shown = json_of(
        &mychart(&home)
            .args(["config", "show", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    );
    assert_eq!(
        shown["profiles"],
        serde_json::json!([{"name": "acct1", "patients": 0}, {"name": "adult2-org", "patients": 0}])
    );
    mychart(&home)
        .args(["profiles", "remove", "acct1"])
        .assert()
        .code(6);
    mychart(&home)
        .args(["profiles", "remove", "acct1", "--force"])
        .assert()
        .success();
    assert_eq!(
        list(&home)["profiles"],
        serde_json::json!([{"name": "adult2-org"}])
    );
    mychart(&home)
        .args(["profiles", "remove", "acct1", "--force"])
        .assert()
        .code(4);
    // Nothing from 0.4 to move: exit 0, and the profile is there.
    let v = json_of(
        &mychart(&home)
            .args(["profiles", "migrate", "acct3", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    );
    assert_eq!(v["schema"], "profile-migrate/v1");
    assert_eq!(
        (v["moved"].as_u64(), v["contexts"].as_u64()),
        (Some(0), Some(0))
    );
    assert_eq!(list(&home)["profiles"].as_array().unwrap().len(), 2);
}

/// The config holds each profile's patient contexts, so every write leaves
/// it owner-only, a file that was readable by others included.
#[cfg(unix)]
#[test]
fn the_config_is_written_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let home = tmp();
    let path = json_of(
        &mychart(&home)
            .args(["config", "path", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )["path"]
        .as_str()
        .unwrap()
        .to_string();
    let path = std::path::Path::new(&path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "{}").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    for args in [
        vec!["profiles", "add", "acct1"],
        vec!["config", "set", "scope", "openid fhirUser"],
        vec!["config", "unset", "scope"],
    ] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
        mychart(&home).args(&args).assert().success();
        assert_eq!(mode(path), 0o600, "{args:?}");
    }
    let shown = json_of(
        &mychart(&home)
            .args(["profiles", "list", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    );
    assert_eq!(shown["profiles"], serde_json::json!([{"name": "acct1"}]));
}
