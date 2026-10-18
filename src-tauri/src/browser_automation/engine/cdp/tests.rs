use std::sync::atomic::AtomicU8;

use parking_lot::Mutex;
use serde_json::{json, Value};

use super::*;
use crate::browser_automation::engine::EngineFuture;

#[test]
fn issue_timeout_cancels_queued_closure_only_before_it_issues() {
    // Timeout wins the race: the closure never issued the command, so the
    // timeout cancels it and a retry is safe.
    let pending = AtomicU8::new(CALL_PENDING);
    assert!(issue_timeout_retryable(&pending));
    // The cancelled state is now terminal for any later observer.
    assert!(!issue_timeout_retryable(&pending));

    // Closure wins the race (already issued): the command is out and may
    // run, so the failure is terminal.
    let issued = AtomicU8::new(CALL_ISSUED);
    assert!(!issue_timeout_retryable(&issued));
}

#[test]
fn cdp_evaluate_params_pin_method_expression_and_flags() {
    let params = evaluate_params("(function(){ return 1; })()");
    assert_eq!(params["expression"], "(function(){ return 1; })()");
    assert_eq!(params["returnByValue"], true, "value must ride the reply");
    assert_eq!(params["awaitPromise"], true, "promises must resolve first");
    assert_eq!(CdpMethod::RuntimeEvaluate.as_str(), "Runtime.evaluate");
}

#[test]
fn cdp_parse_evaluate_result_returns_value_shapes() {
    let v = parse_evaluate_result(&json!({
        "result": {"type": "string", "value": "complete"}
    }))
    .expect("string value parses");
    assert_eq!(v, json!("complete"));

    let v = parse_evaluate_result(&json!({
        "result": {"type": "object", "value": {"text": "- button \"x\" @e1", "refs": 1}}
    }))
    .expect("object value parses");
    assert_eq!(v["refs"], 1);

    let v = parse_evaluate_result(&json!({
        "result": {"type": "boolean", "value": true}
    }))
    .expect("boolean value parses");
    assert_eq!(v, json!(true));

    // `undefined` completion: no `value` key → null, parity with the
    // bridge's `JSON.stringify(undefined → null)` reply.
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
    assert_eq!(err, EngineError::new(EngineErrorKind::PageException, false));

    let err = parse_evaluate_result(&json!({"exceptionDetails": {"text": "Uncaught"}}))
        .expect_err("text-only exceptions are errors");
    assert_eq!(err.kind, EngineErrorKind::PageException);

    let err = parse_evaluate_result(&json!({})).expect_err("reply without result is malformed");
    assert_eq!(err, EngineError::new(EngineErrorKind::Malformed, false));
}

#[test]
fn cdp_parse_evaluate_result_maps_protocol_error_replies() {
    let err = parse_evaluate_result(&json!({
        "error": {"code": -32601, "message": "Method not found"}
    }))
    .expect_err("protocol error replies are errors");
    assert_eq!(err, EngineError::new(EngineErrorKind::Protocol, false));

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

/// Records every call and answers from a fixed reply.
struct RecordingTransport {
    calls: Mutex<Vec<(String, &'static str, Value)>>,
    reply: Result<Value, EngineError>,
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
        let out = self.reply.clone();
        Box::pin(async move { out })
    }
}

fn transport(reply: Result<Value, EngineError>) -> RecordingTransport {
    RecordingTransport {
        calls: Mutex::new(Vec::new()),
        reply,
    }
}

#[tokio::test]
async fn click_at_presses_then_releases_left_button_at_the_point() {
    let t = transport(Ok(json!({})));
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
async fn evaluate_parses_the_transport_reply_and_propagates_transport_errors() {
    let t = transport(Ok(json!({"result": {"type": "string", "value": "complete"}})));
    assert_eq!(
        evaluate(&t, "agent-a", "document.readyState").await,
        Ok(json!("complete"))
    );
    assert_eq!(t.calls.lock()[0].1, "Runtime.evaluate");

    let undelivered = EngineError::new(EngineErrorKind::Dispatch, true);
    let t = transport(Err(undelivered));
    assert_eq!(evaluate(&t, "agent-a", "1").await, Err(undelivered));
}

#[tokio::test]
async fn capture_screenshot_decodes_png_bytes_and_rejects_missing_data() {
    let t = transport(Ok(json!({"data": "iVBORw=="})));
    assert_eq!(
        capture_screenshot(&t, "agent-a").await,
        Ok(vec![0x89, 0x50, 0x4e, 0x47])
    );
    assert_eq!(t.calls.lock()[0].2, json!({"format": "png"}));

    let t = transport(Ok(json!({})));
    assert_eq!(
        capture_screenshot(&t, "agent-a").await,
        Err(EngineError::new(EngineErrorKind::Malformed, false))
    );
}
