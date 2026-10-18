//! Engine-agnostic core: tab registry, consent, verb dispatch, reply
//! shaping and boundary logging. Drives engines only through the
//! [`BrowserEngine`] port; never touches platform crates.

pub mod boundary;
mod consent;
mod dispatch;
mod reply;
mod tabs;

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;
use tokio::sync::oneshot;

pub use boundary::BrowserError;
pub use reply::{
    ERR_CAPABILITY, ERR_CONFIRMATION, ERR_INTERNAL, ERR_INVALID_PARAMS, ERR_STALE_REF,
    ERR_TAB_NOT_FOUND, ERR_TIMEOUT,
};

use super::engine::{self, BrowserEngine};
use super::{BrowserCall, BrowserHost};
use crate::acp::events;
use crate::browser_tab_manager::BrowserTabManager;
use crate::web::EventSink;
use consent::PendingConsent;
use tabs::{bump_epoch_if_agent_tab, AgentTabs};

/// Desktop host: drives real pane webviews through the platform engine
/// adapter and `BrowserTabManager` for navigation and tab lifecycle.
pub struct DesktopBrowserHost {
    app: AppHandle,
    tabs: Arc<BrowserTabManager>,
    engine: Arc<dyn BrowserEngine>,
    sinks: Vec<Arc<dyn EventSink>>,
    /// session_id -> consent granted. Denials are not cached — a denied agent
    /// re-prompts on the next call so the user can change their mind.
    consent: parking_lot::Mutex<HashMap<String, ()>>,
    /// session_id -> in-flight consent prompt (request id + every caller
    /// waiting on it). Concurrent tool calls share ONE prompt instead of
    /// stacking dialogs.
    pending_consent: parking_lot::Mutex<HashMap<String, PendingConsent>>,
    pending_tab_open: parking_lot::Mutex<HashMap<String, oneshot::Sender<Result<(), String>>>>,
    agent_tabs: parking_lot::Mutex<AgentTabs>,
}

impl DesktopBrowserHost {
    pub fn new(
        app: AppHandle,
        tabs: Arc<BrowserTabManager>,
        sinks: Vec<Arc<dyn EventSink>>,
    ) -> Arc<Self> {
        let engine = engine::desktop_engine(tabs.clone());
        let traits = engine.traits();
        log::info!(
            "[browser-agent] engine registered (isolated_world={}, background_render={})",
            traits.isolated_world,
            traits.background_render
        );
        Arc::new(Self {
            app,
            engine,
            tabs,
            sinks,
            consent: parking_lot::Mutex::new(HashMap::new()),
            pending_consent: parking_lot::Mutex::new(HashMap::new()),
            pending_tab_open: parking_lot::Mutex::new(HashMap::new()),
            agent_tabs: parking_lot::Mutex::new(HashMap::new()),
        })
    }

    fn emit<T: Serialize>(&self, session_id: Option<&str>, name: &'static str, payload: &T) {
        events::fan_out(&self.sinks, session_id, name, payload);
    }
}

impl BrowserHost for DesktopBrowserHost {
    fn execute<'a>(
        &'a self,
        session_id: &'a str,
        agent_id: &'a str,
        call: BrowserCall,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, BrowserError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.ensure_consent(session_id, agent_id, &call).await?;
            self.act(session_id, call).await
        })
    }

    fn end_session(&self, session_id: &str) {
        self.forget_consent(session_id);
        let ids: Vec<String> = {
            let tabs = self.agent_tabs.lock();
            tabs.iter()
                .filter(|(_, t)| t.session_id == session_id)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in ids {
            self.close_agent_tab(&id);
        }
    }

    fn notify_tab_created(&self, tab_id: &str) -> bool {
        match self.pending_tab_open.lock().remove(tab_id) {
            Some(tx) => {
                let _ = tx.send(Ok(()));
                true
            }
            None => false,
        }
    }

    fn notify_tab_closed(&self, tab_id: &str) {
        if self.agent_tabs.lock().remove(tab_id).is_some() {
            log::info!("[browser-agent] agent tab closed externally: {tab_id}");
        }
    }

    fn notify_tab_navigated(&self, tab_id: &str) {
        bump_epoch_if_agent_tab(&self.agent_tabs.lock(), tab_id);
    }

    fn resolve_consent(&self, request_id: &str, allowed: bool) -> bool {
        self.answer_consent(request_id, allowed)
    }

    fn resolve_eval(&self, nonce: &str, ok: bool, value: Option<String>) -> bool {
        self.engine.deliver_script_reply(nonce, ok, value)
    }
}
