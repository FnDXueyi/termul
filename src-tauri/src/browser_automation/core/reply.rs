//! The reply contract: frozen wire codes plus a host `reason`, `fidelity`
//! on every mutating reply, host facts under `tab`, and page-derived text
//! only under the closed, untrusted `page` object.

use serde::Serialize;
use serde_json::{json, Value};

use super::ports::{CanonicalOrigin, TabProfile};
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
/// Cap for page-controlled metadata (`url`, `title`) — keeps the reply
/// inside the child's reply bound even on an adversarial page.
pub(super) const PAGE_META_MAX_CHARS: usize = 4 * 1024;

/// Whether a mutating action used real engine input or page-script events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    Trusted,
    Synthetic,
}

/// Host facts about the tab an action ran on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TabFacts {
    pub id: String,
    pub epoch: u64,
    /// Host-canonical origin of the tab's current URL; null when it has no
    /// http(s) origin.
    pub origin: Option<CanonicalOrigin>,
    pub profile: TabProfile,
}

/// Page-derived data. Untrusted: the page controls every field, and refs
/// are the only host tokens inside `snapshot`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PageData {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    /// True when any field above was cut to its cap.
    pub truncated: bool,
}

impl PageData {
    /// Page metadata, each field capped.
    pub(super) fn meta(url: &str, title: &str) -> Self {
        let (url, url_cut) = cap(url, PAGE_META_MAX_CHARS);
        let (title, title_cut) = cap(title, PAGE_META_MAX_CHARS);
        Self {
            url: url.to_string(),
            title: title.to_string(),
            snapshot: None,
            truncated: url_cut || title_cut,
        }
    }
}

/// A successful action. `fidelity` is set exactly when the verb mutates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Success {
    pub fidelity: Option<Fidelity>,
    pub tab: Option<TabFacts>,
    pub page: Option<PageData>,
    pub data: Option<Value>,
}

#[derive(Serialize)]
struct Wire<'a> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fidelity: Option<Fidelity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tab: Option<&'a TabFacts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<&'a PageData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<&'a Value>,
}

impl Success {
    pub fn to_json(&self) -> Value {
        json!(Wire {
            ok: true,
            code: None,
            reason: None,
            message: None,
            fidelity: self.fidelity,
            tab: self.tab.as_ref(),
            page: self.page.as_ref(),
            data: self.data.as_ref(),
        })
    }
}

impl BrowserError {
    /// The failure reply. The message is a host template plus the
    /// agent's own argument values; never page or engine text.
    pub fn to_json(&self) -> Value {
        json!(Wire {
            ok: false,
            code: Some(self.code),
            reason: self.reason,
            message: Some(&self.message),
            fidelity: None,
            tab: None,
            page: None,
            data: None,
        })
    }
}

/// Map an engine failure on `tab` to a wire code, a reason and a host
/// template. Engine errors carry no message, so nothing page- or
/// protocol-derived reaches the reply.
pub(super) fn engine_error(err: EngineError, tab: &str) -> BrowserError {
    let (code, reason, message) = match err.kind {
        EngineErrorKind::TabNotFound => {
            return BrowserError::tab_not_found(tab).with_arg_keys(&["tabId"]);
        }
        EngineErrorKind::Unsupported => (
            ERR_CAPABILITY,
            "engine_unsupported",
            "this browser engine does not support the action",
        ),
        EngineErrorKind::Timeout => (
            ERR_TIMEOUT,
            "engine_timeout",
            "the browser engine did not answer in time",
        ),
        EngineErrorKind::Dispatch => (
            ERR_INTERNAL,
            "engine_unreachable",
            "the browser engine could not be reached",
        ),
        EngineErrorKind::NoReply => (
            ERR_INTERNAL,
            "engine_no_reply",
            "the browser engine dropped the reply",
        ),
        EngineErrorKind::PageException => (
            ERR_INTERNAL,
            "page_exception",
            "the page helper script threw",
        ),
        EngineErrorKind::Protocol => (
            ERR_INTERNAL,
            "engine_protocol_error",
            "the browser engine rejected the command",
        ),
        EngineErrorKind::Malformed => (
            ERR_INTERNAL,
            "engine_malformed_reply",
            "the browser engine returned an unusable reply",
        ),
    };
    BrowserError::new(code, message).with_reason(reason)
}

/// Map a helper script's `{error: "…"}` result to a wire code. The text
/// comes from the page's main world, so it never reaches the reply: a
/// `stale_ref` prefix maps to `stale_ref` (take a fresh snapshot),
/// anything else to `internal`.
pub(super) fn eval_gate(out: &Value, ref_id: Option<&str>) -> Result<(), BrowserError> {
    let Some(e) = out.get("error").and_then(Value::as_str) else {
        return Ok(());
    };
    if e.starts_with("stale_ref") {
        let message = match ref_id {
            Some(r) => format!("ref {r} no longer resolves; take a new snapshot"),
            None => "the ref no longer resolves; take a new snapshot".to_string(),
        };
        return Err(BrowserError::new(ERR_STALE_REF, message).with_reason("ref_not_resolved"));
    }
    if e == "no focused element" {
        return Err(BrowserError::invalid("no element has focus; pass a ref").with_reason("no_focused_element"));
    }
    Err(BrowserError::new(ERR_INTERNAL, "the page helper reported a failure").with_reason("page_helper_error"))
}

/// Hard cut at `max` bytes on a UTF-8 char boundary; true when cut.
fn cap(s: &str, max: usize) -> (&str, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    (&s[..cut], true)
}

/// Snapshot text cap: hard cut at `SNAPSHOT_MAX_CHARS` on a char boundary,
/// with a trailing marker so the agent knows the text was truncated.
pub(super) fn truncate_snapshot(text: &str) -> (String, bool) {
    match cap(text, SNAPSHOT_MAX_CHARS) {
        (kept, true) => (format!("{kept}{SNAPSHOT_TRUNCATION_MARKER}"), true),
        (kept, false) => (kept.to_string(), false),
    }
}

/// Guard and shape a snapshot eval result into page data plus the ref
/// count. The result must be a JSON object — anything else (null, string,
/// …) is an `internal` error rather than a silently empty snapshot.
pub(super) fn shape_snapshot(out: &Value) -> Result<(PageData, u64), BrowserError> {
    if !out.is_object() {
        return Err(BrowserError::internal("snapshot eval result was not an object"));
    }
    let field = |k: &str| out.get(k).and_then(Value::as_str).unwrap_or_default();
    let mut page = PageData::meta(field("url"), field("title"));
    let (text, cut) = truncate_snapshot(field("text"));
    page.snapshot = Some(text);
    page.truncated |= cut;
    let refs = out.get("refs").and_then(Value::as_u64).unwrap_or(0);
    Ok((page, refs))
}

#[cfg(test)]
#[path = "reply_tests.rs"]
mod tests;
