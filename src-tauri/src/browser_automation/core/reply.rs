//! Wire codes, engine-error mapping and snapshot shaping.

use serde_json::{json, Value};

use super::BrowserError;
use crate::browser_automation::engine::{EngineError, EngineErrorKind};

/// Stable wire error codes (agents match on these, not the message text).
pub const ERR_CAPABILITY: &str = "capability_unavailable";
pub const ERR_CONFIRMATION: &str = "confirmation_required";
pub const ERR_TAB_NOT_FOUND: &str = "tab_not_found";
pub const ERR_STALE_REF: &str = "stale_ref";
pub const ERR_INVALID_PARAMS: &str = "invalid_params";
pub const ERR_TIMEOUT: &str = "timeout";
pub const ERR_INTERNAL: &str = "internal";

/// Snapshot text cap — keeps replies inside the child's reply bound.
pub(super) const SNAPSHOT_MAX_CHARS: usize = 48 * 1024;
/// Trailing marker appended when snapshot text is cut at the cap.
pub(super) const SNAPSHOT_TRUNCATION_MARKER: &str = "\n…[snapshot truncated]";
/// Cap for page-controlled snapshot metadata (`url`, `title`) — keeps the
/// reply inside the child's reply bound even on an adversarial page.
pub(super) const SNAPSHOT_META_MAX_CHARS: usize = 4 * 1024;

/// Map an engine failure on `tab` to a wire code and a host template.
/// Engine errors carry no message, so nothing page- or protocol-derived
/// reaches the reply.
pub(super) fn engine_error(err: EngineError, tab: &str) -> BrowserError {
    let (code, message) = match err.kind {
        EngineErrorKind::TabNotFound => {
            return BrowserError::tab_not_found(tab).with_arg_keys(&["tabId"]);
        }
        EngineErrorKind::Unsupported => (
            ERR_CAPABILITY,
            "this browser engine does not support the action",
        ),
        EngineErrorKind::Timeout => (ERR_TIMEOUT, "the browser engine did not answer in time"),
        EngineErrorKind::Dispatch => (ERR_INTERNAL, "the browser engine could not be reached"),
        EngineErrorKind::NoReply => (ERR_INTERNAL, "the browser engine dropped the reply"),
        EngineErrorKind::PageException => (ERR_INTERNAL, "the page helper script threw"),
        EngineErrorKind::Protocol => (ERR_INTERNAL, "the browser engine rejected the command"),
        EngineErrorKind::Malformed => (ERR_INTERNAL, "the browser engine returned an unusable reply"),
    };
    BrowserError::new(code, message)
}

/// Map a page-side `{error: "…"}` result to the typed wire codes.
/// `stale_ref:` prefixes come straight back as `stale_ref` so agents can
/// take a fresh snapshot; anything else is internal.
pub(super) fn eval_gate(out: Value) -> Result<(), BrowserError> {
    if let Some(e) = out.get("error").and_then(Value::as_str) {
        if e.starts_with("stale_ref") {
            return Err(BrowserError::new(ERR_STALE_REF, e));
        }
        return Err(BrowserError::new(ERR_INTERNAL, e));
    }
    Ok(())
}

/// Hard cut at `max` bytes on a UTF-8 char boundary.
fn cut_at_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

/// Snapshot text cap: hard cut at `SNAPSHOT_MAX_CHARS` on a char boundary,
/// with a trailing marker so the agent knows the text was truncated.
pub(super) fn truncate_snapshot(text: String) -> String {
    if text.len() > SNAPSHOT_MAX_CHARS {
        let mut out = cut_at_char_boundary(&text, SNAPSHOT_MAX_CHARS).to_string();
        out.push_str(SNAPSHOT_TRUNCATION_MARKER);
        out
    } else {
        text
    }
}

/// Cap a page-controlled string field. Metadata fields get a plain cut —
/// no truncation marker (that is snapshot-text only); non-string values
/// pass through untouched.
fn cap_snapshot_field(value: &Value, max: usize) -> Value {
    match value.as_str() {
        Some(s) => Value::String(cut_at_char_boundary(s, max).to_string()),
        None => value.clone(),
    }
}

/// Guard and shape a snapshot eval result into the wire reply. The eval
/// result must be a JSON object — anything else (null, string, …) is an
/// `internal` error rather than a silently empty snapshot.
pub(super) fn shape_snapshot(out: &Value, tab_id: &str, epoch: u64) -> Result<Value, BrowserError> {
    if !out.is_object() {
        return Err(BrowserError::internal(
            "snapshot eval result was not an object",
        ));
    }
    Ok(json!({
        "tabId": tab_id,
        "epoch": epoch,
        "url": cap_snapshot_field(out.get("url").unwrap_or(&Value::Null), SNAPSHOT_META_MAX_CHARS),
        "title": cap_snapshot_field(out.get("title").unwrap_or(&Value::Null), SNAPSHOT_META_MAX_CHARS),
        "snapshot": truncate_snapshot(
            out.get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        "refs": out.get("refs").cloned().unwrap_or(json!(0)),
    }))
}

#[cfg(test)]
#[path = "reply_tests.rs"]
mod tests;
