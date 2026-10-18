//! Engine-neutral CDP logic over a [`CdpTransport`]. WebView2's
//! `CallDevToolsProtocolMethod` is the only transport today; a future
//! CDP-speaking runtime reuses this module unchanged.
//!
//! Helpers run in an isolated world of the main frame: the page shares
//! its DOM with that world but none of its globals, prototypes or
//! wrappers, and their value comes back as the call's own return value.

use serde_json::{json, Value};

use super::assets::{Script, WORLD_NAME};
use super::{EngineError, EngineErrorKind, EngineFuture, Point};

pub mod input;

/// Closed CDP method allow-list: a transport can issue nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdpMethod {
    PageGetFrameTree,
    PageCreateIsolatedWorld,
    RuntimeCallFunctionOn,
    InputDispatchMouseEvent,
    InputDispatchKeyEvent,
    InputInsertText,
    PageCaptureScreenshot,
}

impl CdpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            CdpMethod::PageGetFrameTree => "Page.getFrameTree",
            CdpMethod::PageCreateIsolatedWorld => "Page.createIsolatedWorld",
            CdpMethod::RuntimeCallFunctionOn => "Runtime.callFunctionOn",
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

/// `Runtime.callFunctionOn` params: the helper as a function of one
/// argument, called in the isolated context with its parameters passed by
/// value. `returnByValue` ships the result back as JSON.
pub fn call_params(script: &Script, context_id: i64) -> Value {
    json!({
        "functionDeclaration": format!("function (p) {{\n{}\n}}", script.helper.body),
        "executionContextId": context_id,
        "arguments": [{ "value": script.params }],
        "returnByValue": true,
        "awaitPromise": true,
    })
}

/// Parse a `Runtime.callFunctionOn` reply: the return value (an absent
/// `value` key means `undefined`, read as null), or a
/// terminal failure. Protocol errors, page exceptions and malformed
/// replies all mean the command was processed, so the script may have run.
pub fn parse_evaluate_result(out: &Value) -> Result<Value, EngineError> {
    if out.get("error").is_some_and(Value::is_object) {
        return Err(EngineError::new(EngineErrorKind::Protocol));
    }
    // A JSON `null` exceptionDetails must not enter the exception branch.
    if out.get("exceptionDetails").is_some_and(Value::is_object) {
        return Err(EngineError::new(EngineErrorKind::PageException));
    }
    match out.get("result") {
        Some(r) if r.is_object() => Ok(r.get("value").cloned().unwrap_or(Value::Null)),
        _ => Err(EngineError::new(EngineErrorKind::Malformed)),
    }
}

/// The main frame's id from a `Page.getFrameTree` reply.
pub fn main_frame_id(tree: &Value) -> Option<&str> {
    tree.pointer("/frameTree/frame/id").and_then(Value::as_str)
}

/// The isolated world's context id from a `Page.createIsolatedWorld` reply.
pub fn world_context_id(world: &Value) -> Option<i64> {
    world.get("executionContextId").and_then(Value::as_i64)
}

/// Run `script` in the main frame's isolated world. The world is looked
/// up per call: each new document gets a new context, and asking for the
/// named world again returns the current document's one.
pub async fn evaluate(
    transport: &dyn CdpTransport,
    tab: &str,
    script: &Script,
) -> Result<Value, EngineError> {
    let malformed = EngineError::new(EngineErrorKind::Malformed);
    let tree = transport
        .call(tab, CdpMethod::PageGetFrameTree, json!({}))
        .await?;
    let frame = main_frame_id(&tree).ok_or(malformed)?;
    let world = transport
        .call(
            tab,
            CdpMethod::PageCreateIsolatedWorld,
            json!({ "frameId": frame, "worldName": WORLD_NAME }),
        )
        .await?;
    let context = world_context_id(&world).ok_or(malformed)?;
    let out = transport
        .call(tab, CdpMethod::RuntimeCallFunctionOn, call_params(script, context))
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
        .ok_or(EngineError::new(EngineErrorKind::Malformed))?;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| EngineError::new(EngineErrorKind::Malformed))
}

#[cfg(test)]
mod tests;
