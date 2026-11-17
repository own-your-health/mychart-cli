//! The pages under `site/`, published to GitHub Pages. `callback.html` is the
//! https redirect Epic sends a sign-in's answer to; it passes the query on to
//! mychart's listener. These checks keep it to that: every inline block
//! allowed by its hash and nothing else, no referrer, nothing loaded from
//! anywhere, and the listener's address.

use base64::Engine;
use sha2::{Digest, Sha256};

const CALLBACK: &str = include_str!("../site/callback.html");
const INDEX: &str = include_str!("../site/index.html");

fn between<'a>(page: &'a str, open: &str, close: &str) -> Vec<&'a str> {
    page.split(open)
        .skip(1)
        .map(|rest| rest.split_once(close).expect("a closed element").0)
        .collect()
}

fn csp(page: &str) -> &str {
    let marker = "http-equiv=\"Content-Security-Policy\" content=\"";
    let start = page.find(marker).expect("a Content-Security-Policy") + marker.len();
    let len = page[start..].find('"').unwrap();
    &page[start..start + len]
}

fn hash(block: &str) -> String {
    let digest = Sha256::digest(block.as_bytes());
    format!(
        "'sha256-{}'",
        base64::engine::general_purpose::STANDARD.encode(digest)
    )
}

#[test]
fn the_callback_page_allows_only_its_own_script_and_style() {
    let policy = csp(CALLBACK);
    assert!(policy.starts_with("default-src 'none';"), "{policy}");
    for directive in ["base-uri 'none'", "form-action 'none'"] {
        assert!(policy.contains(directive), "{policy}");
    }
    let scripts = between(CALLBACK, "<script>", "</script>");
    let styles = between(CALLBACK, "<style>", "</style>");
    assert_eq!((scripts.len(), styles.len()), (1, 1));
    assert!(
        policy.contains(&format!("script-src {};", hash(scripts[0]))),
        "the script's hash is {}: update the policy",
        hash(scripts[0])
    );
    assert!(
        policy.contains(&format!("style-src {};", hash(styles[0]))),
        "the style's hash is {}: update the policy",
        hash(styles[0])
    );
}

#[test]
fn the_callback_page_sends_the_query_to_the_listener_and_nowhere_else() {
    let listener = format!(
        "\"http://127.0.0.1:{}/callback\" + query",
        mychart::config::DEFAULT_REDIRECT_PORT
    );
    let script = between(CALLBACK, "<script>", "</script>")[0];
    assert!(
        script.contains(&format!("location.replace({listener})")),
        "{script}"
    );
    assert!(CALLBACK.contains("<meta name=\"referrer\" content=\"no-referrer\">"));
    for page in [CALLBACK, INDEX] {
        for loads in [
            " src=",
            "<link",
            "<img",
            "<iframe",
            "fetch(",
            "XMLHttpRequest",
        ] {
            assert!(!page.contains(loads), "a page loads something: {loads}");
        }
    }
}
