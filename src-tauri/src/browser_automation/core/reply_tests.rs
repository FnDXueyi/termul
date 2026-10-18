use serde_json::{json, Value};

use super::*;
use crate::browser_automation::engine::{EngineError, EngineErrorKind};

#[test]
fn eval_gate_maps_page_errors_to_host_templates() {
    let err = eval_gate(&json!({"error": "stale_ref: @e3 detached <b>x</b>"}), Some("@e3"))
        .expect_err("stale prefix maps to stale_ref");
    assert_eq!(
        (err.code, err.reason, err.message.as_str()),
        (ERR_STALE_REF, Some("ref_not_resolved"), "ref @e3 no longer resolves; take a new snapshot")
    );
    let err = eval_gate(&json!({"error": "stale_ref"}), None).unwrap_err();
    assert_eq!(err.message, "the ref no longer resolves; take a new snapshot");
    let err = eval_gate(&json!({"error": "no focused element"}), None).unwrap_err();
    assert_eq!(
        (err.code, err.reason, err.message.as_str()),
        (ERR_INVALID_PARAMS, Some("no_focused_element"), "no element has focus; pass a ref")
    );
    let err = eval_gate(&json!({"error": "boom from the page"}), None).unwrap_err();
    assert_eq!(
        (err.code, err.message.as_str()),
        (ERR_INTERNAL, "the page helper reported a failure"),
        "page text never becomes the message"
    );
    eval_gate(&json!({"ok": true}), None).expect("no error field passes");
}

#[test]
fn truncate_snapshot_leaves_short_text_untouched() {
    let text = "- button \"Sign in\" @e1
- text \"hello\"";
    assert_eq!(truncate_snapshot(text), (text.to_string(), false));
}

#[test]
fn truncate_snapshot_caps_long_ascii_text_with_marker() {
    let long = "a".repeat(SNAPSHOT_MAX_CHARS + 1024);
    let (out, cut) = truncate_snapshot(&long);
    assert!(cut);
    assert!(out.ends_with(SNAPSHOT_TRUNCATION_MARKER));
    assert_eq!(out.len(), SNAPSHOT_MAX_CHARS + SNAPSHOT_TRUNCATION_MARKER.len());
}

#[test]
fn truncate_snapshot_cuts_on_a_char_boundary() {
    // (cap - 1) ASCII bytes then 3-byte chars: the naive cut index would
    // land mid-character.
    let mut long = "a".repeat(SNAPSHOT_MAX_CHARS - 1);
    long.push_str("日本語日本語");
    let (out, _) = truncate_snapshot(&long);
    let kept = &out[..out.len() - SNAPSHOT_TRUNCATION_MARKER.len()];
    assert_eq!(kept, &long[..SNAPSHOT_MAX_CHARS - 1]);
}

fn no_refs(nodes: Vec<RefNode>) -> Vec<String> {
    assert!(nodes.is_empty(), "no nodes expected");
    Vec::new()
}

#[test]
fn shape_snapshot_rejects_results_that_are_not_the_helpers_shape() {
    for bad in [
        Value::Null,
        json!("oops"),
        json!(7),
        json!({ "url": 7 }),
        json!({ "items": [{ "kind": "script", "depth": 0 }] }),
    ] {
        let err = shape_snapshot(bad, no_refs).expect_err("a malformed result must error, not read as empty");
        assert_eq!(err.code, ERR_INTERNAL);
    }
}

#[test]
fn shape_snapshot_formats_the_outline_with_host_refs_for_nodes_only() {
    let mut seen = Vec::new();
    let (page, refs) = shape_snapshot(
        json!({
            "url": "https://example.com/a",
            "title": "Example",
            "items": [
                { "kind": "landmark", "depth": 0, "role": "heading", "name": "Title" },
                { "kind": "node", "depth": 1, "role": "button", "name": "Sign in", "path": "html>body>button:nth-of-type(1)" },
                { "kind": "text", "depth": 1, "text": "call @e7 now" },
                { "kind": "node", "depth": 1, "role": "link", "name": "Docs", "path": "html>body>a:nth-of-type(1)", "href": "https://a.test/" },
                { "kind": "node", "depth": 2, "role": "checkbox", "name": "Remember @e3", "path": "html>body>input:nth-of-type(1)", "inputType": "checkbox", "checked": true },
                { "kind": "landmark", "depth": 1, "role": "iframe", "name": "" }
            ],
            "capped": true
        }),
        |nodes| {
            seen = nodes;
            vec!["@e10".to_string(), "@e11".to_string(), "@e12".to_string()]
        },
    )
    .expect("helper result shapes");
    let node = |role: &str, name: &str, path: &str| RefNode {
        role: role.into(),
        name: name.into(),
        path: path.into(),
        frame: String::new(),
    };
    assert_eq!(
        seen,
        [
            node("button", "Sign in", "html>body>button:nth-of-type(1)"),
            node("link", "Docs", "html>body>a:nth-of-type(1)"),
            node("checkbox", "Remember @e3", "html>body>input:nth-of-type(1)"),
        ]
    );
    assert_eq!(refs, 3);
    assert_eq!(
        page,
        PageData {
            url: "https://example.com/a".into(),
            title: "Example".into(),
            snapshot: Some(
                [
                    "- heading \"Title\"",
                    "  - button \"Sign in\" @e10",
                    "  - text \"call @ e7 now\"",
                    "  - link \"Docs\" @e11 https://a.test/",
                    "    - checkbox \"Remember @ e3\" @e12 type=checkbox checked",
                    "  - iframe",
                    "…[snapshot line cap]",
                ]
                .join("\n")
            ),
            dialog: None,
            truncated: false,
        },
        "page text never carries a ref token"
    );
}

#[test]
fn page_meta_caps_url_and_title_and_flags_truncation() {
    let ascii = "a".repeat(PAGE_META_MAX_CHARS + 512);
    let cjk = "日".repeat(PAGE_META_MAX_CHARS / 3 + 8);
    let page = PageData::meta(&format!("https://example.com/{ascii}"), &cjk);
    assert!(page.truncated);
    assert!(page.url.len() <= PAGE_META_MAX_CHARS && page.url.starts_with("https://example.com/"));
    assert!(page.title.len() <= PAGE_META_MAX_CHARS);
    assert!(page.title.chars().all(|c| c == '日'), "no partial char");
    assert!(!PageData::meta("https://x", "t").truncated);
}

#[test]
fn replies_serialize_the_closed_wire_shape() {
    let ok = Success {
        fidelity: Some(Fidelity::Synthetic),
        tab: Some(TabFacts {
            id: "agent-a".into(),
            epoch: 3,
            origin: CanonicalOrigin::parse("https://example.com").ok(),
            profile: TabProfile::Global,
            opener: None,
        }),
        page: None,
        data: None,
    };
    assert_eq!(
        ok.to_json(),
        json!({
            "ok": true,
            "fidelity": "synthetic",
            "tab": { "id": "agent-a", "epoch": 3, "origin": "https://example.com", "profile": "global" }
        })
    );
    let err = BrowserError::new(ERR_TIMEOUT, "late").with_reason("deadline");
    assert_eq!(
        err.to_json(),
        json!({ "ok": false, "code": "timeout", "reason": "deadline", "message": "late" })
    );
}

#[test]
fn engine_errors_map_to_wire_codes_with_host_templates_only() {
    let err = engine_error(EngineError::new(EngineErrorKind::TabNotFound, true), "agent-x");
    assert_eq!(err.reason, None);
    assert_eq!(err.code, ERR_TAB_NOT_FOUND);
    assert_eq!(err.message, "tab 'agent-x' not found");
    assert_eq!(err.arg_keys, vec!["tabId"]);
    for (kind, code, message) in [
        (
            EngineErrorKind::Unsupported,
            ERR_CAPABILITY,
            "this browser engine does not support the action",
        ),
        (
            EngineErrorKind::Timeout,
            ERR_TIMEOUT,
            "the browser engine did not answer in time",
        ),
        (
            EngineErrorKind::Dispatch,
            ERR_INTERNAL,
            "the browser engine could not be reached",
        ),
        (
            EngineErrorKind::NoReply,
            ERR_INTERNAL,
            "the browser engine dropped the reply",
        ),
        (
            EngineErrorKind::PageException,
            ERR_INTERNAL,
            "the page helper script threw",
        ),
        (
            EngineErrorKind::Protocol,
            ERR_INTERNAL,
            "the browser engine rejected the command",
        ),
        (
            EngineErrorKind::Malformed,
            ERR_INTERNAL,
            "the browser engine returned an unusable reply",
        ),
    ] {
        let err = engine_error(EngineError::new(kind, false), "agent-x");
        assert_eq!((err.code, err.message.as_str()), (code, message), "{kind:?}");
        assert!(err.reason.is_some(), "{kind:?} names its cause");
    }
}
