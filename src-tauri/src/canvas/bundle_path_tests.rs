use super::bundle_path::{is_bundle_path, redact_canvas_path};

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn the_token_segment_of_a_bundle_path_is_replaced_and_the_rest_kept() {
    for (path, logged) in [
        (
            format!("/canvas/cv1/{TOKEN}/app.js"),
            "/canvas/cv1/[redacted]/app.js",
        ),
        (
            format!("/canvas/cv1/{TOKEN}/assets/deep/app_bg.wasm"),
            "/canvas/cv1/[redacted]/assets/deep/app_bg.wasm",
        ),
        (format!("/canvas/cv1/{TOKEN}/"), "/canvas/cv1/[redacted]/"),
        (format!("/canvas/cv1/{TOKEN}"), "/canvas/cv1/[redacted]"),
    ] {
        assert_eq!(redact_canvas_path(&path), logged);
    }
}

#[test]
fn paths_without_a_token_segment_come_back_unchanged() {
    for path in [
        "/canvas/open",
        "/canvas/close",
        "/canvas/read-document",
        "/canvas/resolve-conflict",
        "/canvas/cv1",
        "/canvas/cv1/",
        "/canvas/",
        "/fs/write",
        "/ws",
        "/",
    ] {
        assert_eq!(redact_canvas_path(path), path);
    }
}

#[test]
fn bundle_paths_are_every_canvas_path_that_is_not_an_api_route() {
    assert!(is_bundle_path("/canvas/cv1"));
    assert!(is_bundle_path("/canvas/cv1/token/app.js"));
    assert!(!is_bundle_path("/canvas/open"));
    assert!(!is_bundle_path("/canvas/crashed"));
    assert!(!is_bundle_path("/git/status"));
}
