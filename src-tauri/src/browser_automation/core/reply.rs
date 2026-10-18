//! The reply contract: frozen wire codes plus a host `reason`, `fidelity`
//! on every mutating reply, host facts under `tab`, and page-derived text
//! only under the closed, untrusted `page` object.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::dialogs::DialogView;
use super::ports::{CanonicalOrigin, TabProfile};
use super::tabs::RefNode;
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
    /// For a popup tab, the agent tab whose page opened it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opener: Option<String>,
}

/// Page-derived data. Untrusted: the page controls every field, and refs
/// are the only host tokens inside `snapshot`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PageData {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    /// The script dialog blocking the tab, or the last one the host closed
    /// on its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialog: Option<DialogView>,
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
            dialog: None,
            truncated: url_cut || title_cut,
        }
    }
}

/// Page-controlled text outside the snapshot, cut to the metadata cap.
pub(super) fn cap_page_text(text: &str) -> String {
    cap(text, PAGE_META_MAX_CHARS).0.to_string()
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

/// A helper that could not do its job: `{error: "…"}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HelperFailure {
    error: String,
}

/// What an input helper (click, fill, type, press, scroll, hover) returns
/// when it ran: exactly `{ok: true}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionDone {
    ok: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ActionReply {
    Done(ActionDone),
    Failed(HelperFailure),
}

/// The centre of an element in main-frame CSS px.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub(super) struct Rect {
    pub cx: f64,
    pub cy: f64,
}

/// Whether a checkbox, switch or radio button is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckState {
    pub checked: bool,
    pub radio: bool,
}

/// What the resolve helper reports for an element that still exists.
/// `check` is null for an element that cannot be checked.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(super) struct ResolvedElement {
    pub rect: Rect,
    pub role: String,
    pub name: String,
    #[serde(default)]
    pub check: Option<CheckState>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CheckReply {
    State(CheckState),
    Failed(HelperFailure),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ResolveReply {
    Found(ResolvedElement),
    Failed(HelperFailure),
}

fn malformed_helper_result() -> BrowserError {
    BrowserError::new(ERR_INTERNAL, "the page helper returned an unusable result")
        .with_reason("page_helper_malformed")
}

/// Parse an input helper's result. Only `{ok: true}` is success: any other
/// shape is `internal`, so an action that may not have happened never
/// reports ok.
pub(super) fn action_result(out: Value, ref_id: Option<&str>) -> Result<(), BrowserError> {
    match serde_json::from_value::<ActionReply>(out) {
        Ok(ActionReply::Done(ActionDone { ok: true })) => Ok(()),
        Ok(ActionReply::Failed(failure)) => Err(helper_failure(&failure.error, ref_id)),
        Ok(ActionReply::Done(_)) | Err(_) => Err(malformed_helper_result()),
    }
}

/// Parse the resolve helper's result: the element's centre, role and name,
/// or the failure it reported.
pub(super) fn resolve_result(out: Value, ref_id: Option<&str>) -> Result<ResolvedElement, BrowserError> {
    match serde_json::from_value::<ResolveReply>(out) {
        Ok(ResolveReply::Found(element)) => Ok(element),
        Ok(ResolveReply::Failed(failure)) => Err(helper_failure(&failure.error, ref_id)),
        Err(_) => Err(malformed_helper_result()),
    }
}

/// Parse the checked-state helper's result.
pub(super) fn check_result(out: Value, ref_id: Option<&str>) -> Result<CheckState, BrowserError> {
    match serde_json::from_value::<CheckReply>(out) {
        Ok(CheckReply::State(state)) => Ok(state),
        Ok(CheckReply::Failed(failure)) => Err(helper_failure(&failure.error, ref_id)),
        Err(_) => Err(malformed_helper_result()),
    }
}

/// The wire error of a helper that reported `{error: "…"}`. Any other
/// shape is left to the parser of that helper's own result.
pub(super) fn reported_failure(out: &Value, ref_id: Option<&str>) -> Result<(), BrowserError> {
    match HelperFailure::deserialize(out) {
        Ok(failure) => Err(helper_failure(&failure.error, ref_id)),
        Err(_) => Ok(()),
    }
}

/// Map a helper's `{error: "…"}` to a wire code. Only the code reaches the
/// reply, never the helper's text: a `stale_ref` prefix maps to
/// `stale_ref` (take a fresh snapshot), anything else to `internal`.
fn helper_failure(e: &str, ref_id: Option<&str>) -> BrowserError {
    if e.starts_with("stale_ref") {
        let message = match ref_id {
            Some(r) => format!("ref {r} no longer resolves; take a new snapshot"),
            None => "the ref no longer resolves; take a new snapshot".to_string(),
        };
        return BrowserError::new(ERR_STALE_REF, message).with_reason("ref_not_resolved");
    }
    if e == "no focused element" {
        return BrowserError::invalid("no element has focus; pass a ref").with_reason("no_focused_element");
    }
    BrowserError::new(ERR_INTERNAL, "the page helper reported a failure").with_reason("page_helper_error")
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

/// The page outline the snapshot helper returns. Every string in it is
/// page-controlled; the structure is the helper's.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct PageSnapshot {
    url: String,
    title: String,
    items: Vec<SnapshotItem>,
    /// The helper stopped at its item cap.
    capped: bool,
}

/// One line of the outline. Only `Node` items are actionable.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SnapshotItem {
    Node {
        depth: usize,
        role: String,
        name: String,
        /// The helper's structural locator for the element.
        path: String,
        #[serde(default)]
        href: Option<String>,
        #[serde(default, rename = "inputType")]
        input_type: Option<String>,
        #[serde(default)]
        checked: Option<bool>,
    },
    Landmark {
        depth: usize,
        role: String,
        name: String,
    },
    Text {
        depth: usize,
        text: String,
    },
}

const SNAPSHOT_MAX_INDENT: usize = 32;
const SNAPSHOT_CAP_LINE: &str = "…[snapshot line cap]";

/// Page text as one outline-safe line. Whitespace and control characters,
/// line breaks included, collapse to one space, so page text can never
/// start a line of its own. Text that reads like a ref (`@e12`, or `@e+12`,
/// which ref parsing also accepts) gets a space after the `@`, so the refs
/// the host minted are the only ref tokens in the outline.
fn outline_text(text: &str) -> String {
    let flat = text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = String::with_capacity(flat.len());
    let mut rest = flat.as_str();
    while let Some(at) = rest.find("@e") {
        let ref_like = rest[at + 2..].starts_with(|c: char| c.is_ascii_digit() || c == '+');
        out.push_str(&rest[..at]);
        out.push_str(if ref_like { "@ e" } else { "@e" });
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// Shape a snapshot helper result into page data plus the ref count: one
/// outline line per item, with host refs minted by `mint` for its nodes in
/// document order. A result that is not the helper's shape is an
/// `internal` error rather than a silently empty snapshot.
pub(super) fn shape_snapshot(
    out: Value,
    mint: impl FnOnce(Vec<RefNode>) -> Vec<String>,
) -> Result<(PageData, usize), BrowserError> {
    let snapshot: PageSnapshot = serde_json::from_value(out)
        .map_err(|_| BrowserError::internal("snapshot eval result was malformed"))?;
    let (text, count) = format_snapshot(&snapshot, mint);
    let mut page = PageData::meta(&snapshot.url, &snapshot.title);
    let (text, cut) = truncate_snapshot(&text);
    page.snapshot = Some(text);
    page.truncated |= cut;
    Ok((page, count))
}

fn format_snapshot(
    snapshot: &PageSnapshot,
    mint: impl FnOnce(Vec<RefNode>) -> Vec<String>,
) -> (String, usize) {
    let nodes: Vec<RefNode> = snapshot
        .items
        .iter()
        .filter_map(|item| match item {
            SnapshotItem::Node { role, name, path, .. } => Some(RefNode {
                role: role.clone(),
                name: name.clone(),
                path: path.clone(),
                frame: String::new(),
            }),
            _ => None,
        })
        .collect();
    let mut tokens = mint(nodes).into_iter();
    let count = tokens.len();
    let mut lines = Vec::with_capacity(snapshot.items.len() + 1);
    for item in &snapshot.items {
        let line = match item {
            SnapshotItem::Node {
                depth,
                role,
                name,
                href,
                input_type,
                checked,
                ..
            } => {
                let token = tokens.next().unwrap_or_default();
                let mut line = format!("{}- {} \"{}\" {token}", indent(*depth), outline_text(role), outline_text(name));
                if let Some(href) = href {
                    line.push(' ');
                    line.push_str(&outline_text(href));
                }
                if let Some(t) = input_type {
                    line.push_str(" type=");
                    line.push_str(&outline_text(t));
                }
                match checked {
                    Some(true) => line.push_str(" checked"),
                    Some(false) => line.push_str(" unchecked"),
                    None => {}
                }
                line
            }
            SnapshotItem::Landmark { depth, role, name } => match outline_text(name).as_str() {
                "" => format!("{}- {}", indent(*depth), outline_text(role)),
                name => format!("{}- {} \"{}\"", indent(*depth), outline_text(role), name),
            },
            SnapshotItem::Text { depth, text } => {
                format!("{}- text \"{}\"", indent(*depth), outline_text(text))
            }
        };
        lines.push(line);
    }
    if snapshot.capped {
        lines.push(SNAPSHOT_CAP_LINE.to_string());
    }
    (lines.join("\n"), count)
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth.min(SNAPSHOT_MAX_INDENT))
}

#[cfg(test)]
#[path = "reply_tests.rs"]
mod tests;
