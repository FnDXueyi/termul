use parking_lot::Mutex;
use serde_json::{json, Value};

use super::*;
use crate::browser_automation::engine::{assets, EngineFuture};

#[test]
fn a_helper_call_passes_agent_strings_as_a_by_value_argument() {
    let script = assets::fill("html>body:nth-of-type(1)", "\"); leak(); (\"");
    let params = call_params(&script, 7);
    assert_eq!(params["executionContextId"], 7);
    assert_eq!(
        params["arguments"],
        json!([{ "value": { "path": "html>body:nth-of-type(1)", "value": "\"); leak(); (\"" } }])
    );
    let declaration = params["functionDeclaration"].as_str().unwrap();
    assert_eq!(declaration, format!("function (p) {{\n{}\n}}", assets::FILL.body));
    assert_eq!(params["returnByValue"], true, "value must ride the reply");
    assert_eq!(params["awaitPromise"], true, "promises must resolve first");
}

#[test]
fn cdp_parse_evaluate_result_returns_value_shapes() {
    let v = parse_evaluate_result(&json!({
        "result": {"type": "string", "value": "complete"}
    }))
    .expect("string value parses");
    assert_eq!(v, json!("complete"));

    let v = parse_evaluate_result(&json!({
        "result": {"type": "object", "value": {"items": [], "refs": 1}}
    }))
    .expect("object value parses");
    assert_eq!(v["refs"], 1);

    let v = parse_evaluate_result(&json!({
        "result": {"type": "boolean", "value": true}
    }))
    .expect("boolean value parses");
    assert_eq!(v, json!(true));

    // An `undefined` return has no `value` key and reads as null.
    let v = parse_evaluate_result(&json!({"result": {"type": "undefined"}}))
        .expect("undefined completion parses");
    assert!(v.is_null());
}

#[test]
fn cdp_parse_evaluate_result_maps_exceptions_and_malformed_replies() {
    let err = parse_evaluate_result(&json!({
        "exceptionDetails": {
            "text": "Uncaught",
            "exception": {"type": "object", "subtype": "error",
                          "description": "TypeError: boom"}
        }
    }))
    .expect_err("page exceptions are errors");
    assert_eq!(err, EngineError::new(EngineErrorKind::PageException));

    let err = parse_evaluate_result(&json!({"exceptionDetails": {"text": "Uncaught"}}))
        .expect_err("text-only exceptions are errors");
    assert_eq!(err.kind, EngineErrorKind::PageException);

    let err = parse_evaluate_result(&json!({})).expect_err("reply without result is malformed");
    assert_eq!(err, EngineError::new(EngineErrorKind::Malformed));
}

#[test]
fn cdp_parse_evaluate_result_maps_protocol_error_replies() {
    let err = parse_evaluate_result(&json!({
        "error": {"code": -32601, "message": "Method not found"}
    }))
    .expect_err("protocol error replies are errors");
    assert_eq!(err, EngineError::new(EngineErrorKind::Protocol));

    // A non-object error body is not a protocol error: it falls through
    // to result parsing and surfaces as malformed.
    let err = parse_evaluate_result(&json!({"error": "boom"}))
        .expect_err("non-object error body has no result either");
    assert_eq!(err.kind, EngineErrorKind::Malformed);
}

#[test]
fn cdp_parse_evaluate_result_ignores_null_exception_details() {
    let v = parse_evaluate_result(&json!({
        "exceptionDetails": null,
        "result": {"type": "string", "value": "complete"}
    }))
    .expect("a JSON null exceptionDetails must not enter the exception branch");
    assert_eq!(v, json!("complete"));
}

#[test]
fn cdp_parse_evaluate_result_rejects_non_object_result() {
    let err = parse_evaluate_result(&json!({"result": "nope"}))
        .expect_err("non-object result must be an error");
    assert_eq!(err.kind, EngineErrorKind::Malformed);
}

/// Records every call and answers each method from its scripted reply
/// (an empty object when none is scripted).
struct RecordingTransport {
    calls: Mutex<Vec<(String, &'static str, Value)>>,
    replies: Vec<(CdpMethod, Result<Value, EngineError>)>,
}

impl CdpTransport for RecordingTransport {
    fn call<'a>(
        &'a self,
        tab: &'a str,
        method: CdpMethod,
        params: Value,
    ) -> EngineFuture<'a, Value> {
        self.calls
            .lock()
            .push((tab.to_string(), method.as_str(), params));
        let out = self
            .replies
            .iter()
            .find(|(m, _)| *m == method)
            .map_or(Ok(json!({})), |(_, r)| r.clone());
        Box::pin(async move { out })
    }
}

fn transport(replies: Vec<(CdpMethod, Result<Value, EngineError>)>) -> RecordingTransport {
    RecordingTransport {
        calls: Mutex::new(Vec::new()),
        replies,
    }
}

fn isolated_world_replies(call: Result<Value, EngineError>) -> Vec<(CdpMethod, Result<Value, EngineError>)> {
    vec![
        (
            CdpMethod::PageGetFrameTree,
            Ok(json!({ "frameTree": { "frame": { "id": "MAIN" } } })),
        ),
        (
            CdpMethod::PageCreateIsolatedWorld,
            Ok(json!({ "executionContextId": 42 })),
        ),
        (CdpMethod::RuntimeCallFunctionOn, call),
    ]
}

#[tokio::test]
async fn click_at_presses_then_releases_left_button_at_the_point() {
    let t = transport(vec![]);
    click_at(&t, "agent-a", Point { x: 10.5, y: 20.0 })
        .await
        .expect("click succeeds");
    let calls = t.calls.lock();
    let kinds: Vec<&str> = calls.iter().map(|c| c.2["type"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["mousePressed", "mouseReleased"]);
    for (tab, method, params) in calls.iter() {
        assert_eq!(tab, "agent-a");
        assert_eq!(*method, "Input.dispatchMouseEvent");
        assert_eq!(params["x"], 10.5);
        assert_eq!(params["y"], 20.0);
        assert_eq!(params["button"], "left");
    }
}

#[tokio::test]
async fn evaluate_calls_the_helper_in_the_main_frames_isolated_world() {
    let t = transport(isolated_world_replies(Ok(
        json!({ "result": { "type": "string", "value": "complete" } }),
    )));
    assert_eq!(
        evaluate(&t, "agent-a", &assets::ready_state()).await,
        Ok(json!("complete"))
    );
    let calls = t.calls.lock();
    let methods: Vec<&str> = calls.iter().map(|c| c.1).collect();
    assert_eq!(
        methods,
        ["Page.getFrameTree", "Page.createIsolatedWorld", "Runtime.callFunctionOn"]
    );
    assert_eq!(calls[1].2, json!({ "frameId": "MAIN", "worldName": "termul_agent" }));
    assert_eq!(calls[2].2["executionContextId"], 42);
}

#[tokio::test]
async fn a_failure_before_the_helper_call_never_calls_it() {
    let lost = EngineError::new(EngineErrorKind::NoReply);
    let mut replies = isolated_world_replies(Ok(json!({})));
    replies[1].1 = Err(lost);
    let t = transport(replies);
    assert_eq!(evaluate(&t, "agent-a", &assets::snapshot()).await, Err(lost));
    assert!(t.calls.lock().iter().all(|c| c.1 != "Runtime.callFunctionOn"));
    let t = transport(vec![(CdpMethod::PageGetFrameTree, Ok(json!({})))]);
    assert_eq!(
        evaluate(&t, "agent-a", &assets::snapshot()).await,
        Err(EngineError::new(EngineErrorKind::Malformed))
    );
    assert!(t.calls.lock().iter().all(|c| c.1 != "Runtime.callFunctionOn"));

    let ran = EngineError::new(EngineErrorKind::Timeout);
    let t = transport(isolated_world_replies(Err(ran)));
    assert_eq!(evaluate(&t, "agent-a", &assets::snapshot()).await, Err(ran));
}

#[tokio::test]
async fn capture_screenshot_decodes_png_bytes_and_rejects_missing_data() {
    let t = transport(vec![(
        CdpMethod::PageCaptureScreenshot,
        Ok(json!({"data": "iVBORw=="})),
    )]);
    assert_eq!(
        capture_screenshot(&t, "agent-a").await,
        Ok(vec![0x89, 0x50, 0x4e, 0x47])
    );
    assert_eq!(t.calls.lock()[0].2, json!({"format": "png"}));

    let t = transport(vec![]);
    assert_eq!(
        capture_screenshot(&t, "agent-a").await,
        Err(EngineError::new(EngineErrorKind::Malformed))
    );
}
