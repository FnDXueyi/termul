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

#[test]
fn shape_snapshot_rejects_non_object_results() {
    for bad in [Value::Null, json!("oops"), json!(7), json!(true)] {
        let err = shape_snapshot(&bad).expect_err("non-object eval result must error, not read as empty");
        assert_eq!(err.code, ERR_INTERNAL);
    }
}

#[test]
fn shape_snapshot_puts_page_text_under_page_and_counts_refs() {
    let (page, refs) = shape_snapshot(&json!({
        "url": "https://example.com/a",
        "title": "Example",
        "text": "- button \"x\" @e1",
        "refs": 2
    }))
    .expect("object result shapes");
    assert_eq!(
        page,
        PageData {
            url: "https://example.com/a".into(),
            title: "Example".into(),
            snapshot: Some("- button \"x\" @e1".into()),
            truncated: false,
        }
    );
    assert_eq!(refs, 2);

    let (page, refs) = shape_snapshot(&json!({"url": 7, "refs": "many"})).unwrap();
    assert_eq!((page.url.as_str(), page.title.as_str(), refs), ("", "", 0), "non-strings read as empty");
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
