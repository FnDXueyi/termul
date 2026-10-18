//! Engine-agnostic core: tab registry, consent, verb dispatch, reply
//! shaping and boundary logging. Drives engines only through the
//! [`BrowserEngine`] port; never touches platform crates.

pub mod authz;
pub mod boundary;
mod consent;
mod actions;
pub(crate) mod dispatch;
pub mod ports;
mod reply;
mod stop;
mod surface;
mod tabs;
#[cfg(test)]
pub mod test_support;

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::json;
use tauri::AppHandle;
use tokio::sync::oneshot;

pub use boundary::BrowserError;
pub use dispatch::{expected_args_text, schema, ToolSchema};
pub use reply::{
    Success, ERR_CAPABILITY, ERR_CONFIRMATION, ERR_INTERNAL, ERR_INVALID_PARAMS, ERR_STALE_REF,
    ERR_TAB_NOT_FOUND, ERR_TIMEOUT,
};

use super::budgets::Deadline;
use super::engine::{self, BrowserEngine, EngineFacts};
use super::{BrowserCall, BrowserHost};
use crate::acp::events;
use crate::browser_tab_manager::BrowserTabManager;
use crate::web::EventSink;
use authz::HopVerdict;
use consent::ConsentGate;
use ports::{PermissionRequest, PolicyStore};
use surface::{PaneSurface, TabSurface};
use tabs::{bump_epoch_if_agent_tab, AgentTabs};

const EVENT_STOP_CHANGED: &str = "acp:browser_stop_changed";

/// The session's host-stamped project id.
pub type ProjectResolver = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// Host services the desktop browser host consumes.
pub struct HostServices {
    pub permissions: Arc<dyn PermissionRequest>,
    /// The project allow-list the gate reads, and where "always allow for
    /// this project" is written.
    pub policy: Arc<dyn PolicyStore>,
    pub project_of: ProjectResolver,
}

/// Desktop host: drives real pane webviews through the platform engine
/// adapter and `BrowserTabManager` for navigation and tab lifecycle.
pub struct DesktopBrowserHost {
    surface: Arc<dyn TabSurface>,
    engine: Arc<dyn BrowserEngine>,
    sinks: Vec<Arc<dyn EventSink>>,
    gate: ConsentGate,
    project_of: ProjectResolver,
    pending_tab_open: parking_lot::Mutex<HashMap<String, oneshot::Sender<Result<(), String>>>>,
    agent_tabs: parking_lot::Mutex<AgentTabs>,
}

impl DesktopBrowserHost {
    pub fn new(
        app: AppHandle,
        tabs: Arc<BrowserTabManager>,
        sinks: Vec<Arc<dyn EventSink>>,
        services: HostServices,
    ) -> Arc<Self> {
        let engine = engine::desktop_engine(tabs.clone());
        let traits = engine.facts().traits;
        log::info!(
            "[browser-agent] engine registered (isolated_world={}, background_render={})",
            traits.isolated_world,
            traits.background_render
        );
        Self::with_parts(Arc::new(PaneSurface { app, tabs }), engine, sinks, services)
    }

    fn with_parts(
        surface: Arc<dyn TabSurface>,
        engine: Arc<dyn BrowserEngine>,
        sinks: Vec<Arc<dyn EventSink>>,
        services: HostServices,
    ) -> Arc<Self> {
        Arc::new(Self {
            surface,
            engine,
            sinks,
            gate: ConsentGate::new(services.permissions, services.policy),
            project_of: services.project_of,
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
        _agent_id: &'a str,
        call: BrowserCall,
        deadline: Deadline,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Success, BrowserError>> + Send + 'a>>
    {
        Box::pin(async move {
            let token = self.gate.begin(session_id)?;
            let spec = dispatch::admit(&call.action, &call.args, self.engine.facts())?;
            token
                .guard(async {
                    self.authorize_call(&token, session_id, spec.verb, &call, deadline)
                        .await?;
                    // After a consent wait the call may no longer have time to
                    // run the verb: refuse it now rather than act after the
                    // agent gave up.
                    if !deadline.admits(spec.budget) {
                        return Err(BrowserError::new(
                            ERR_TIMEOUT,
                            format!(
                                "not enough time left in this call to run {}; call it again",
                                spec.name
                            ),
                        )
                        .with_reason("deadline"));
                    }
                    self.act(session_id, spec.verb, call, deadline).await
                })
                .await
        })
    }

    fn engine_facts(&self) -> EngineFacts {
        self.engine.facts().clone()
    }

    fn end_session(&self, session_id: &str) {
        self.gate.end_session(session_id);
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

    fn stop(&self, session_id: &str) -> bool {
        let first = self.gate.stop(session_id);
        log::info!(
            "[browser-agent] stopped by user session={}",
            crate::logging::redact_session_id(session_id)
        );
        self.emit(
            Some(session_id),
            EVENT_STOP_CHANGED,
            &json!({ "sessionId": session_id, "stopped": true }),
        );
        first
    }

    fn user_turn(&self, session_id: &str) {
        if self.gate.resume(session_id) {
            self.emit(
                Some(session_id),
                EVENT_STOP_CHANGED,
                &json!({ "sessionId": session_id, "stopped": false }),
            );
        }
    }

    fn navigation_allowed(&self, tab_id: &str, url: &tauri::Url) -> bool {
        let Some(session_id) = self.agent_tab(tab_id).map(|tab| tab.session_id.clone()) else {
            return true;
        };
        if self.gate.is_stopped(&session_id) {
            log::info!("[browser-agent] navigation hop cancelled tab={tab_id} reason=stopped_by_user");
            return false;
        }
        match self.gate.check_hop(&session_id, url) {
            HopVerdict::Allow => true,
            HopVerdict::Deny(reason) => {
                log::info!("[browser-agent] navigation hop cancelled tab={tab_id} reason={}", reason.as_str());
                false
            }
            HopVerdict::Ungranted(..) => {
                log::info!("[browser-agent] navigation hop cancelled tab={tab_id} reason=ungranted_origin");
                false
            }
        }
    }

    fn resolve_eval(&self, nonce: &str, ok: bool, value: Option<String>) -> bool {
        self.engine.deliver_script_reply(nonce, ok, value)
    }
}

#[cfg(test)]
mod tests;
