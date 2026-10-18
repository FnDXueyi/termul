//! Engine-neutral CDP logic over a [`CdpTransport`]. WebView2's
//! `CallDevToolsProtocolMethod` is the only transport today; a future
//! CDP-speaking runtime reuses this module unchanged.
//!
//! Only commands that never reached the page may be retried on another
//! channel. A script that ran (page exception) or may have run (no
//! reply) could double-apply side effects when re-executed: fill re-fires
//! input/change events, press Enter re-clicks. So only
//! [`EngineError::retryable`] failures are retried.

use serde_json::{json, Value};
use std::sync::atomic::{AtomicU8, Ordering};

use super::{EngineError, EngineErrorKind, EngineFuture, Point};

pub mod input;

/// Closed CDP method allow-list: a transport can issue nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdpMethod {
    RuntimeEvaluate,
    InputDispatchMouseEvent,
    InputDispatchKeyEvent,
    InputInsertText,
    PageCaptureScreenshot,
}

impl CdpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            CdpMethod::RuntimeEvaluate => "Runtime.evaluate",
            CdpMethod::InputDispatchMouseEvent => "Input.dispatchMouseEvent",
            CdpMethod::InputDispatchKeyEvent => "Input.dispatchKeyEvent",
            CdpMethod::InputInsertText => "Input.insertText",
            CdpMethod::PageCaptureScreenshot => "Page.captureScreenshot",
        }
    }
}

/// Issues one CDP command against a tab and resolves with the raw JSON
/// result object.
pub trait CdpTransport: Send + Sync {
    fn call<'a>(&'a self, tab: &'a str, method: CdpMethod, params: Value)
        -> EngineFuture<'a, Value>;
}

/// Issue-state for a UI-thread closure racing an issue-phase timeout
/// (see [`issue_timeout_retryable`]).
pub const CALL_PENDING: u8 = 0;
pub const CALL_ISSUED: u8 = 1;
pub const CALL_CANCELLED: u8 = 2;

/// Classify an issue-phase timeout: the awaiting task races the queued
/// closure for the pending state. Winning (PENDING → CANCELLED) proves the
/// closure has not issued the command and now never will, so a retry is
/// safe. Losing means the command is already out and may run.
pub fn issue_timeout_retryable(state: &AtomicU8) -> bool {
    state
        .compare_exchange(CALL_PENDING, CALL_CANCELLED, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

/// `Runtime.evaluate` params. `returnByValue` ships the completion value
/// back as JSON; `awaitPromise` resolves promise results before replying.
pub fn evaluate_params(expression: &str) -> Value {
    json!({
        "expression": expression,
        "returnByValue": true,
        "awaitPromise": true,
    })
}

/// Parse a `Runtime.evaluate` reply: the completion value (an absent
/// `value` key means `undefined` → null, parity with the JS bridge), or a
/// terminal failure. Protocol errors, page exceptions and malformed
/// replies all mean the command was processed, so the script may have run.
pub fn parse_evaluate_result(out: &Value) -> Result<Value, EngineError> {
    if out.get("error").is_some_and(Value::is_object) {
        return Err(EngineError::new(EngineErrorKind::Protocol, false));
    }
    // A JSON `null` exceptionDetails must not enter the exception branch.
    if out.get("exceptionDetails").is_some_and(Value::is_object) {
        return Err(EngineError::new(EngineErrorKind::PageException, false));
    }
    match out.get("result") {
        Some(r) if r.is_object() => Ok(r.get("value").cloned().unwrap_or(Value::Null)),
        _ => Err(EngineError::new(EngineErrorKind::Malformed, false)),
    }
}

/// `Runtime.evaluate` through `transport`.
pub async fn evaluate(
    transport: &dyn CdpTransport,
    tab: &str,
    expression: &str,
) -> Result<Value, EngineError> {
    let out = transport
        .call(tab, CdpMethod::RuntimeEvaluate, evaluate_params(expression))
        .await?;
    parse_evaluate_result(&out)
}

/// Trusted left click at `at` via `Input.dispatchMouseEvent`.
pub async fn click_at(
    transport: &dyn CdpTransport,
    tab: &str,
    at: Point,
) -> Result<(), EngineError> {
    for kind in ["mousePressed", "mouseReleased"] {
        transport
            .call(
                tab,
                CdpMethod::InputDispatchMouseEvent,
                json!({
                    "type": kind,
                    "x": at.x,
                    "y": at.y,
                    "button": "left",
                    "clickCount": 1,
                }),
            )
            .await?;
    }
    Ok(())
}

/// `Page.captureScreenshot` → PNG bytes.
pub async fn capture_screenshot(
    transport: &dyn CdpTransport,
    tab: &str,
) -> Result<Vec<u8>, EngineError> {
    let out = transport
        .call(tab, CdpMethod::PageCaptureScreenshot, json!({ "format": "png" }))
        .await?;
    decode_screenshot(&out)
}

/// Decode a `Page.captureScreenshot` reply's base64 `data`.
pub fn decode_screenshot(out: &Value) -> Result<Vec<u8>, EngineError> {
    use base64::Engine;
    let b64 = out
        .get("data")
        .and_then(Value::as_str)
        .ok_or(EngineError::new(EngineErrorKind::Malformed, false))?;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| EngineError::new(EngineErrorKind::Malformed, false))
}

#[cfg(test)]
mod tests;
