//! The eval round trip shared by the WebKit adapters. Both engines call a
//! function body in a named script world with named string arguments, on
//! the UI thread, and answer through a native completion callback. The
//! helper's value comes back as JSON text (see [`call_body`]).

use serde_json::Value;

use super::assets::Script;
use super::{
    CapabilityId, CapabilityTable, EngineError, EngineErrorKind, EngineFacts, EngineTraits,
    Support,
};

/// The argument name the JSON-encoded parameters are bound to.
pub const PARAMS_ARG: &str = "termulParams";

/// The function body a WebKit engine runs for `script`: it decodes the
/// parameters and returns the helper's value as JSON text, all inside the
/// isolated world. A helper may return a promise; both engines await the
/// body's promise before they complete the call. The body is static; the
/// parameters travel as [`params_json`] under [`PARAMS_ARG`].
pub fn call_body(script: &Script) -> String {
    format!(
        "return Promise.resolve((function (p) {{\n{}\n}})(JSON.parse({PARAMS_ARG}))).then(function (r) {{\n  return r === undefined ? 'null' : JSON.stringify(r);\n}});",
        script.helper.body
    )
}

/// The parameters as one JSON string argument.
pub fn params_json(script: &Script) -> String {
    script.params.to_string()
}

/// What both WebKit adapters implement today: isolated-world eval, with
/// every input verb run as synthetic DOM events by the helpers.
pub fn isolated_facts() -> EngineFacts {
    EngineFacts {
        capabilities: CapabilityTable::unavailable("not implemented on this engine yet")
            .with(CapabilityId::IsolatedEval, Support::Trusted)
            .with(CapabilityId::PointerClick, Support::Synthetic)
            .with(CapabilityId::TrustedKey, Support::Synthetic)
            .with(CapabilityId::PointerMove, Support::Synthetic)
            .with(CapabilityId::Wheel, Support::Synthetic)
            .with(
                CapabilityId::Screenshot,
                Support::Unavailable("screenshots are not implemented on this engine yet"),
            ),
        traits: EngineTraits {
            isolated_world: true,
            background_render: false,
            network_observe: false,
        },
    }
}

/// An engine with no isolated world runs no helper at all.
#[cfg(any(test, target_os = "macos"))]
pub fn no_world_facts(reason: &'static str) -> EngineFacts {
    EngineFacts {
        capabilities: CapabilityTable::unavailable(reason),
        traits: EngineTraits {
            isolated_world: false,
            background_render: false,
            network_observe: false,
        },
    }
}

/// How the engine completed one call: the helper's JSON text, or why the
/// engine refused or failed it.
pub type CallResult = Result<String, EngineErrorKind>;

/// Parse a completed call. Every failure here is terminal: the engine
/// processed the call, so the helper may have run.
pub fn parse(result: CallResult) -> Result<Value, EngineError> {
    let json = result.map_err(|kind| EngineError::new(kind, false))?;
    serde_json::from_str(&json).map_err(|_| EngineError::new(EngineErrorKind::Malformed, false))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use ui::{call, Completion};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod ui {
    use serde_json::Value;
    use tauri::webview::PlatformWebview;
    use tokio::sync::oneshot;

    use super::{parse, CallResult};
    use crate::browser_automation::budgets::EVAL_STEP;
    use crate::browser_automation::engine::{EngineError, EngineErrorKind};
    use crate::browser_tab_manager::BrowserTabManager;

    /// Where an adapter's completion callback sends its [`CallResult`].
    pub type Completion = oneshot::Sender<CallResult>;

    /// Run `issue` on the UI thread that owns `tab`'s webview and await the
    /// completion it hands to the engine. A tokio oneshot, so the wait never
    /// blocks the host-MCP runtime.
    pub async fn call<F>(tabs: &BrowserTabManager, tab: &str, issue: F) -> Result<Value, EngineError>
    where
        F: FnOnce(&PlatformWebview, Completion) + Send + 'static,
    {
        let webview = tabs
            .webview(tab)
            .map_err(|_| EngineError::new(EngineErrorKind::TabNotFound, true))?;
        let (tx, rx) = oneshot::channel();
        webview
            .with_webview(move |platform| issue(&platform, tx))
            .map_err(|_| EngineError::new(EngineErrorKind::Dispatch, true))?;
        match tokio::time::timeout(EVAL_STEP, rx).await {
            Ok(Ok(result)) => parse(result),
            Ok(Err(_)) => Err(EngineError::new(EngineErrorKind::NoReply, false)),
            Err(_) => Err(EngineError::new(EngineErrorKind::Timeout, false)),
        }
    }
}

#[cfg(test)]
#[path = "string_eval_tests.rs"]
mod tests;
