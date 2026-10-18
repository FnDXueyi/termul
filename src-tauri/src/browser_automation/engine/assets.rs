//! Page helper scripts the core hands to [`super::BrowserEngine::evaluate`].
//!
//! Each helper is a static function body over one parameter object `p`,
//! built from the files in `assets/` at compile time. Agent strings reach
//! a helper only inside `p`, which every engine passes as a native call
//! argument, so no agent text is ever part of script source. Helpers keep
//! no state between calls: an element is named by the structural path the
//! snapshot reported for it, and the core decides whether that path still
//! names the element its ref was minted for.

use serde_json::{json, Value};

/// The isolated script world every helper runs in.
pub const WORLD_NAME: &str = "termul_agent";

macro_rules! helper {
    ($name:literal) => {
        Helper {
            name: $name,
            body: concat!(
                include_str!("assets/prelude.js"),
                include_str!(concat!("assets/", $name, ".js"))
            ),
        }
    };
}

/// One static helper: a function body over the parameter object `p`
/// whose return value is JSON-serializable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Helper {
    pub name: &'static str,
    pub body: &'static str,
}

pub const SNAPSHOT: Helper = helper!("snapshot");
pub const RESOLVE: Helper = helper!("resolve");
pub const CLICK: Helper = helper!("click");
pub const FILL: Helper = helper!("fill");
pub const TYPE: Helper = helper!("type");
pub const PRESS: Helper = helper!("press");
pub const SCROLL: Helper = helper!("scroll");
pub const HOVER: Helper = helper!("hover");
pub const CONTAINS_TEXT: Helper = helper!("contains_text");
pub const READY_STATE: Helper = helper!("ready_state");
pub const FACTS: Helper = helper!("facts");
pub const PREPARE_TEXT: Helper = helper!("prepare_text");
pub const FOCUSED_EDITABLE: Helper = helper!("focused_editable");
pub const VIEWPORT: Helper = helper!("viewport");

/// A helper with its parameters, ready for an engine.
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub helper: Helper,
    pub params: Value,
}

impl Script {
    fn new(helper: Helper, params: Value) -> Self {
        Self { helper, params }
    }
}

/// `document.readyState`, to settle after a navigation.
pub fn ready_state() -> Script {
    Script::new(READY_STATE, json!({}))
}

/// The page outline: structured items, with one `node` per actionable
/// element carrying its role, name and structural path.
pub fn snapshot() -> Script {
    Script::new(SNAPSHOT, json!({}))
}

/// Scroll the element at `path` into view and report its centre, role and
/// name.
pub fn resolve(path: &str) -> Script {
    Script::new(RESOLVE, json!({ "path": path }))
}

/// Synthetic pointer and click events on the element at `path`.
pub fn click(path: &str) -> Script {
    Script::new(CLICK, json!({ "path": path }))
}

/// Replace the value of the field at `path`, with input and change events.
pub fn fill(path: &str, value: &str) -> Script {
    Script::new(FILL, json!({ "path": path, "value": value }))
}

/// Append `text` to the field at `path`, or to the focused element.
pub fn type_text(path: Option<&str>, text: &str) -> Script {
    Script::new(TYPE, json!({ "path": path, "text": text }))
}

/// Key events for `key` on the element at `path`, or the focused element.
pub fn press(path: Option<&str>, key: &str) -> Script {
    Script::new(PRESS, json!({ "path": path, "key": key }))
}

/// Scroll the element at `path` into view, or the page by `dy` px.
pub fn scroll(path: Option<&str>, dy: f64) -> Script {
    Script::new(SCROLL, json!({ "path": path, "dy": dy }))
}

/// Synthetic hover events on the element at `path`.
pub fn hover(path: &str) -> Script {
    Script::new(HOVER, json!({ "path": path }))
}

/// Whether the page's visible text contains `text`.
pub fn contains_text(text: &str) -> Script {
    Script::new(CONTAINS_TEXT, json!({ "text": text }))
}

/// Actionability facts for the element at `path`, measured over two
/// animation frames; parsed by [`super::input::parse_facts`]. It finds the
/// element whether or not it is shown, so a hidden target reports
/// `visible: false` instead of reading as gone. An element outside the
/// viewport is scrolled into view, and that round reports it unstable.
pub fn facts(path: &str) -> Script {
    Script::new(FACTS, json!({ "path": path }))
}

/// After a trusted click focused the text control at `path`: select its
/// whole content (`replace`) or put the caret at its end. Fails with
/// `not_focused` when the click did not focus it.
pub fn prepare_text(path: &str, replace: bool) -> Script {
    Script::new(PREPARE_TEXT, json!({ "path": path, "replace": replace }))
}

/// Whether the focused element takes typed text.
pub fn focused_editable() -> Script {
    Script::new(FOCUSED_EDITABLE, json!({}))
}

/// The viewport size, for page-level wheel input.
pub fn viewport() -> Script {
    Script::new(VIEWPORT, json!({}))
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;
