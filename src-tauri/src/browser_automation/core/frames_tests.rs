use serde_json::{json, Value};

use crate::browser_automation::core::tests::{harness_with, webview2_like, FakeEngine};
use crate::browser_automation::engine::assets::Script;
use crate::browser_automation::engine::{ChildFrame, EngineError, EngineErrorKind, InputMode, Point};

const PAYMENTS: &str = "html>body>iframe:nth-of-type(1)";

/// The fake main outline, plus a reachable and an unreachable frame item
/// when the fake engine reaches frames.
pub(crate) fn with_frame_items(mut items: Vec<Value>, frames: bool) -> Vec<Value> {
    if frames {
        items.push(json!({ "kind": "frame", "depth": 0, "name": "Payments", "path": PAYMENTS }));
        items.push(json!({ "kind": "frame", "depth": 0, "name": "Ads", "path": "html>body>iframe:nth-of-type(2)" }));
    }
    items
}

/// What the fake engine answers inside a child frame.
pub(crate) fn answer_in_frame(known: bool, script: &Script) -> Result<Value, EngineError> {
    if !known {
        return Err(EngineError::new(EngineErrorKind::FrameGone));
    }
    Ok(match script.helper.name {
        "snapshot" => {
            assert_eq!(script.params["inFrame"], true, "frame paths must pin the frame's document");
            json!({
                "url": "https://pay.example/widget",
                "title": "Pay",
                "items": [
                    { "kind": "landmark", "depth": 0, "role": "form", "name": "" },
                    { "kind": "node", "depth": 1, "role": "button", "name": "Pay now", "path": "frame:1.5|html>body>button:nth-of-type(1)" }
                ],
                "capped": false
            })
        }
        "resolve" => json!({ "rect": { "cx": 5.0, "cy": 6.0 }, "role": "button", "name": "Pay now" }),
        _ => json!({ "ok": true }),
    })
}

fn payments_frame() -> ChildFrame {
    ChildFrame {
        id: "F-pay".into(),
        parent: None,
        owner_path: PAYMENTS.into(),
    }
}

fn ref_of(outline: &str, line_start: &str) -> String {
    let line = outline.lines().find(|l| l.trim_start().starts_with(line_start)).unwrap_or_else(|| panic!("{outline}"));
    line.split_whitespace().find(|w| w.starts_with("@e")).unwrap().to_string()
}

#[tokio::test]
async fn a_reachable_cross_origin_frame_is_spliced_in_and_its_refs_act_inside_it() {
    let engine = FakeEngine::new(webview2_like());
    engine.frames.lock().push(payments_frame());
    let h = harness_with(engine);
    h.call_as("s1", "navigate", json!({ "url": "https://example.com/shop" })).await;
    let snap = h.call_as("s1", "snapshot", json!({})).await;
    let outline = snap["page"]["snapshot"].as_str().unwrap().to_string();
    assert_eq!(
        outline,
        [
            "- button \"Sign in\" @e1",
            "- iframe \"Payments\"",
            "  - form",
            "    - button \"Pay now\" @e2",
            "- iframe \"Ads\" (cross-origin frame, not readable on this engine)",
        ]
        .join("\n")
    );
    assert_eq!(snap["data"]["refs"], 2);

    let pay = ref_of(&outline, "- button \"Pay now\"");
    let click = h.call_as("s1", "click", json!({ "ref": pay })).await;
    assert_eq!((click["ok"].as_bool(), click["fidelity"].as_str()), (Some(true), Some("trusted")), "{click}");
    assert_eq!(*h.engine.clicks.lock(), [(Point { x: 105.0, y: 206.0 }, InputMode::Trusted)]);
    let fill = h.call_as("s1", "fill", json!({ "ref": pay, "value": "4111" })).await;
    assert_eq!(fill["ok"], true, "{fill}");
}

#[tokio::test]
async fn a_ref_into_a_frame_that_is_gone_is_stale() {
    let engine = FakeEngine::new(webview2_like());
    engine.frames.lock().push(payments_frame());
    let h = harness_with(engine);
    h.call_as("s1", "navigate", json!({ "url": "https://example.com/shop" })).await;
    let snap = h.call_as("s1", "snapshot", json!({})).await;
    let pay = ref_of(snap["page"]["snapshot"].as_str().unwrap(), "- button \"Pay now\"");
    h.engine.frames.lock().clear();
    let click = h.call_as("s1", "click", json!({ "ref": pay })).await;
    assert_eq!((click["code"].as_str(), click["reason"].as_str()), (Some("stale_ref"), Some("frame_gone")), "{click}");
    assert!(h.engine.clicks.lock().is_empty(), "nothing may be clicked through a stale frame ref");
}
