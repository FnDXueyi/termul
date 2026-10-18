//! JS-eval bridge: fire-and-forget `webview.eval`, with the result posted
//! back by page script through the `browser_agent_eval_result` command.
//!
//! The only eval channel on macOS and Linux today, and the single fallback
//! on Windows. Page script can see and forge this channel, so it reports
//! its capabilities as synthetic.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::oneshot;
use uuid::Uuid;

#[cfg(not(target_os = "windows"))]
use super::{BrowserEngine, EngineFacts, EngineFuture, InputMode, Point};
#[cfg(any(test, not(target_os = "windows")))]
use super::{CapabilityId, CapabilityTable, Support};
use super::{EngineError, EngineErrorKind, EngineTraits};
use crate::browser_tab_manager::BrowserTabManager;

/// Bridge reply budget (the URL poller shows IPC is sub-ms; 20s is generous).
const EVAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Both current adapters run helpers in the page's main world.
pub const MAIN_WORLD_TRAITS: EngineTraits = EngineTraits {
    isolated_world: false,
    background_render: false,
    network_observe: false,
};

type Waiter = oneshot::Sender<Result<Value, ()>>;

pub struct JsBridge {
    tabs: Arc<BrowserTabManager>,
    pending: parking_lot::Mutex<HashMap<String, Waiter>>,
}

impl JsBridge {
    pub fn new(tabs: Arc<BrowserTabManager>) -> Self {
        Self {
            tabs,
            pending: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub async fn evaluate(&self, tab: &str, expr: &str) -> Result<Value, EngineError> {
        let webview = self
            .tabs
            .webview(tab)
            .map_err(|_| EngineError::new(EngineErrorKind::TabNotFound, true))?;
        let nonce = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(nonce.clone(), tx);
        webview.eval(wrap_eval(tab, &nonce, expr)).map_err(|_| {
            self.pending.lock().remove(&nonce);
            EngineError::new(EngineErrorKind::Dispatch, true)
        })?;
        match tokio::time::timeout(EVAL_TIMEOUT, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(()))) => Err(EngineError::new(EngineErrorKind::PageException, false)),
            Ok(Err(_)) => {
                self.pending.lock().remove(&nonce);
                Err(EngineError::new(EngineErrorKind::NoReply, false))
            }
            Err(_) => {
                self.pending.lock().remove(&nonce);
                Err(EngineError::new(EngineErrorKind::Timeout, false))
            }
        }
    }

    /// Resolve a waiter from `browser_agent_eval_result`. False when the
    /// nonce is unknown (stale or unrelated page report).
    pub fn resolve(&self, nonce: &str, ok: bool, value: Option<String>) -> bool {
        let Some(tx) = self.pending.lock().remove(nonce) else {
            return false;
        };
        let parsed = value
            .map(|s| serde_json::from_str(&s).unwrap_or(Value::String(s)))
            .unwrap_or(Value::Null);
        let _ = tx.send(if ok { Ok(parsed) } else { Err(()) });
        true
    }
}

/// Wrap `expr` so its completion value posts back via the tab-scoped
/// result command. Runs in the page main world.
pub fn wrap_eval(tab_id: &str, nonce: &str, expr: &str) -> String {
    let tab = serde_json::to_string(tab_id).unwrap_or_else(|_| "\"\"".into());
    let n = serde_json::to_string(nonce).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"(function(){{
  var __done=function(ok,val){{try{{
    var inv=null;
    if(window.__TAURI_INTERNALS__&&window.__TAURI_INTERNALS__.invoke)inv=window.__TAURI_INTERNALS__.invoke;
    else if(window.__TAURI__&&window.__TAURI__.invoke)inv=window.__TAURI__.invoke;
    else if(window.__TAURI__&&window.__TAURI__.core&&window.__TAURI__.core.invoke)inv=window.__TAURI__.core.invoke;
    if(inv){{inv('browser_agent_eval_result',{{tabId:{tab},nonce:{n},ok:ok,value:JSON.stringify(val===undefined?null:val)}}).catch(function(){{}});return;}}
  }}catch(e){{}}}};
  try{{
    var __r=(function(){{ return ({expr}); }})();
    Promise.resolve(__r).then(function(v){{__done(true,v)}},function(e){{__done(false,String(e&&e.message||e))}});
  }}catch(e){{__done(false,String(e&&e.message||e))}}
}})();"#
    )
}

/// What the bridge adapter implements: page-script eval and DOM events,
/// all forgeable by the page, so every tier is synthetic.
#[cfg(any(test, not(target_os = "windows")))]
pub fn capability_table() -> CapabilityTable {
    CapabilityTable::unavailable("not implemented on this engine yet")
        .with(CapabilityId::IsolatedEval, Support::Synthetic)
        .with(CapabilityId::PointerClick, Support::Synthetic)
        .with(CapabilityId::TrustedKey, Support::Synthetic)
        .with(CapabilityId::PointerMove, Support::Synthetic)
        .with(CapabilityId::Wheel, Support::Synthetic)
        .with(
            CapabilityId::Screenshot,
            Support::Unavailable("screenshot requires the WebView2 engine (Windows)"),
        )
}

/// Bridge-only engine (macOS, Linux).
#[cfg(not(target_os = "windows"))]
pub struct JsBridgeEngine {
    bridge: JsBridge,
    facts: EngineFacts,
}

#[cfg(not(target_os = "windows"))]
impl JsBridgeEngine {
    pub fn new(tabs: Arc<BrowserTabManager>) -> Self {
        Self {
            bridge: JsBridge::new(tabs),
            facts: EngineFacts {
                capabilities: capability_table(),
                traits: MAIN_WORLD_TRAITS,
            },
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl BrowserEngine for JsBridgeEngine {
    fn facts(&self) -> &EngineFacts {
        &self.facts
    }

    fn evaluate<'a>(&'a self, tab: &'a str, script: &'a str) -> EngineFuture<'a, Value> {
        Box::pin(self.bridge.evaluate(tab, script))
    }

    fn pointer_click<'a>(
        &'a self,
        _tab: &'a str,
        _at: Point,
        _mode: InputMode,
    ) -> EngineFuture<'a, ()> {
        Box::pin(async { Err(EngineError::new(EngineErrorKind::Unsupported, true)) })
    }

    fn screenshot<'a>(&'a self, _tab: &'a str) -> EngineFuture<'a, Vec<u8>> {
        Box::pin(async { Err(EngineError::new(EngineErrorKind::Unsupported, true)) })
    }

    fn deliver_script_reply(&self, nonce: &str, ok: bool, value: Option<String>) -> bool {
        self.bridge.resolve(nonce, ok, value)
    }
}

#[cfg(test)]
mod tests;
