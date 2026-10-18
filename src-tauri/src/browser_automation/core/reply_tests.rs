use serde_json::{json, Value};

use super::*;
use crate::browser_automation::engine::{EngineError, EngineErrorKind};

#[test]
fn eval_gate_maps_stale_ref_prefix() {
    let err = eval_gate(json!({"error": "stale_ref: @e3 detached"}))
        .expect_err("stale prefix maps to stale_ref");
    assert_eq!(err.code, ERR_STALE_REF);
    let err = eval_gate(json!({"error": "boom"})).expect_err("other errors internal");
    assert_eq!(err.code, ERR_INTERNAL);
    eval_gate(json!({"ok": true})).expect("no error field passes");
}

#[test]
fn truncate_snapshot_leaves_short_text_untouched() {
    let text = "- button \"Sign in\" @e1\n- text \"hello\"";
    assert_eq!(truncate_snapshot(text.to_string()), text);
}

#[test]
fn truncate_snapshot_caps_long_ascii_text_with_marker() {
    let long = "a".repeat(SNAPSHOT_MAX_CHARS + 1024);
    let out = truncate_snapshot(long);
    assert!(
        out.ends_with(SNAPSHOT_TRUNCATION_MARKER),
        "marker must be appended: {:?}",
        &out[out.len().saturating_sub(40)..]
    );
    let kept = &out[..out.len() - SNAPSHOT_TRUNCATION_MARKER.len()];
    assert_eq!(kept.len(), SNAPSHOT_MAX_CHARS);
    assert!(
        out.len() <= SNAPSHOT_MAX_CHARS + SNAPSHOT_TRUNCATION_MARKER.len(),
        "reply stays inside the child reply bound"
    );
}

#[test]
fn truncate_snapshot_cuts_on_a_char_boundary() {
    // (cap - 1) ASCII bytes then 3-byte chars: the naive cut index would
    // land mid-character.
    let mut long = "a".repeat(SNAPSHOT_MAX_CHARS - 1);
    long.push_str("日本語日本語");
    let out = truncate_snapshot(long.clone());
    assert!(out.ends_with(SNAPSHOT_TRUNCATION_MARKER));
    let kept = &out[..out.len() - SNAPSHOT_TRUNCATION_MARKER.len()];
    // Cut backs up to the last ASCII byte instead of splitting the char.
    assert_eq!(kept.len(), SNAPSHOT_MAX_CHARS - 1);
    assert_eq!(kept, &long[..SNAPSHOT_MAX_CHARS - 1]);
    assert!(kept.is_ascii());
}

#[test]
fn shape_snapshot_rejects_non_object_results() {
    for bad in [Value::Null, json!("oops"), json!(7), json!(true)] {
        let err = shape_snapshot(&bad, "agent-t", 3)
            .expect_err("non-object eval result must error, not read as empty");
        assert_eq!(err.code, ERR_INTERNAL);
    }
}

#[test]
fn shape_snapshot_passes_short_fields_through() {
    let reply = shape_snapshot(
        &json!({
            "url": "https://example.com/a",
            "title": "Example",
            "text": "- button \"x\" @e1",
            "refs": 2
        }),
        "agent-t",
        7,
    )
    .expect("object result shapes");
    assert_eq!(reply["tabId"], "agent-t");
    assert_eq!(reply["epoch"], 7);
    assert_eq!(reply["url"], "https://example.com/a");
    assert_eq!(reply["title"], "Example");
    assert_eq!(reply["snapshot"], "- button \"x\" @e1");
    assert_eq!(reply["refs"], 2);
}

#[test]
fn shape_snapshot_defaults_missing_fields() {
    let reply =
        shape_snapshot(&json!({}), "agent-t", 1).expect("empty object still shapes");
    assert_eq!(reply["snapshot"], "");
    assert!(reply["url"].is_null());
    assert!(reply["title"].is_null());
    assert_eq!(reply["refs"], 0);
}

#[test]
fn shape_snapshot_caps_page_controlled_url_and_title() {
    let ascii = "a".repeat(SNAPSHOT_META_MAX_CHARS + 512);
    // 3-byte chars straddling the cap: the cut must back up to a boundary.
    let cjk = "日".repeat(SNAPSHOT_META_MAX_CHARS / 3 + 8);
    let reply = shape_snapshot(
        &json!({
            "url": format!("https://example.com/{ascii}"),
            "title": cjk,
            "text": "t",
            "refs": 0
        }),
        "agent-t",
        1,
    )
    .expect("object result shapes");
    let url = reply["url"].as_str().expect("url stays a string");
    assert!(url.len() <= SNAPSHOT_META_MAX_CHARS);
    assert!(url.starts_with("https://example.com/"));
    let title = reply["title"].as_str().expect("title stays a string");
    assert!(title.len() <= SNAPSHOT_META_MAX_CHARS);
    assert!(title.chars().all(|c| c == '日'), "no partial char");
    // Non-string page values pass through untouched (only strings are
    // page-controlled bloat vectors).
    let reply = shape_snapshot(&json!({"url": 7, "title": false}), "agent-t", 1)
        .expect("object result shapes");
    assert_eq!(reply["url"], 7);
    assert_eq!(reply["title"], false);
}

#[test]
fn engine_errors_map_to_wire_codes_with_host_templates_only() {
    let err = engine_error(EngineError::new(EngineErrorKind::TabNotFound, true), "agent-x");
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
    }
}
