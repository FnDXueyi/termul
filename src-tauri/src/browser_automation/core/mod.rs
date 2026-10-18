//! Engine-agnostic core: tab registry, consent, verb dispatch, reply
//! shaping and boundary logging. Drives engines only through the
//! [`BrowserEngine`] port; never touches platform crates.

pub mod boundary;
mod consent;
mod actions;
pub(crate) mod dispatch;
mod events;
pub mod ports;
mod reply;
mod surface;
mod tabs;
mod teardown;
#[cfg(test)]
pub mod test_support;

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::AppHandle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

pub use boundary::BrowserError;
pub use dispatch::{expected_args_text, schema, ToolSchema};
pub use reply::{
    Success, ERR_CAPABILITY, ERR_CONFIRMATION, ERR_INTERNAL, ERR_INVALID_PARAMS, ERR_STALE_REF,
    ERR_TAB_NOT_FOUND, ERR_TIMEOUT,
};
pub use tabs::TabView;

use super::budgets::Deadline;
use super::engine::events::{AllowAll, NavPolicy, TabEvent, TabEvents};
use super::engine::{self, BrowserEngine, EngineFacts};
use super::{BrowserCall, BrowserHost, HostFuture, TabCreateError};
use crate::acp::events as acp_events;
use crate::browser_tab_manager::{BrowserBounds, BrowserTabManager};
use crate::web::EventSink;
use consent::PendingConsent;
use events::TabProjection;
use surface::{PaneSurface, TabSurface};
use tabs::TabRegistry;

/// Desktop host: owns the tab registry and drives the pane's webviews
/// through the platform engine adapter and the tab surface.
pub struct DesktopBrowserHost {
    me: Weak<DesktopBrowserHost>,
    surface: Arc<dyn TabSurface>,
    engine: Arc<dyn BrowserEngine>,
    sinks: Vec<Arc<dyn EventSink>>,
    /// session_id -> consent granted. Denials are not cached — a denied agent
    /// re-prompts on the next call so the user can change their mind.
    consent: parking_lot::Mutex<HashMap<String, ()>>,
    /// session_id -> in-flight consent prompt (request id + every caller
    /// waiting on it). Concurrent tool calls share ONE prompt instead of
    /// stacking dialogs.
    pending_consent: parking_lot::Mutex<HashMap<String, PendingConsent>>,
    registry: Arc<Mutex<TabRegistry>>,
    events: UnboundedSender<(String, TabEvent)>,
    nav_policy: Arc<dyn NavPolicy>,
}

impl DesktopBrowserHost {
    pub fn new(
        app: AppHandle,
        tabs: Arc<BrowserTabManager>,
        sinks: Vec<Arc<dyn EventSink>>,
    ) -> Arc<Self> {
        let engine = engine::desktop_engine(tabs.clone());
        let traits = engine.facts().traits;
        log::info!(
            "[browser-agent] engine registered (isolated_world={}, background_render={})",
            traits.isolated_world,
            traits.background_render
        );
        let surface: Arc<dyn TabSurface> = Arc::new(PaneSurface { app, tabs });
        teardown::spawn_startup_sweep(surface.clone());
        Self::with_parts(surface, engine, sinks)
    }

    fn with_parts(
        surface: Arc<dyn TabSurface>,
        engine: Arc<dyn BrowserEngine>,
        sinks: Vec<Arc<dyn EventSink>>,
    ) -> Arc<Self> {
        let registry = Arc::new(Mutex::new(TabRegistry::default()));
        let (tx, rx) = unbounded_channel();
        spawn(events::consume(rx, registry.clone(), surface.clone(), sinks.clone()));
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            surface,
            engine,
            sinks,
            consent: parking_lot::Mutex::new(HashMap::new()),
            pending_consent: parking_lot::Mutex::new(HashMap::new()),
            registry,
            events: tx,
            nav_policy: Arc::new(AllowAll),
        })
    }

    fn emit<T: Serialize>(&self, session_id: Option<&str>, name: &'static str, payload: &T) {
        acp_events::fan_out(&self.sinks, session_id, name, payload);
    }

    fn tab_events(&self, label: &str) -> TabEvents {
        TabEvents::new(label, self.events.clone(), self.nav_policy.clone())
    }

    /// Build the webview for a renderer-requested tab and admit it to the
    /// registry once the engine confirmed it.
    async fn create_tab_inner(
        &self,
        label: &str,
        url: &str,
        bounds: BrowserBounds,
    ) -> Result<TabView, TabCreateError> {
        let url = self
            .registry
            .lock()
            .begin_create(label, url)
            .map_err(|refusal| TabCreateError {
                code: refusal.code(),
                message: refusal.message().to_string(),
            })?;
        if let Err(message) = self
            .surface
            .create(label, &url, bounds, self.tab_events(label))
            .await
        {
            self.registry
                .lock()
                .fail_create(label, "the browser engine did not create the tab");
            return Err(TabCreateError {
                code: "BROWSER_TAB_CREATE_FAILED",
                message,
            });
        }
        let live = self.registry.lock().finish_create(label);
        if !live {
            // Closed while it was being built.
            self.surface.close(label).await;
            return Err(TabCreateError {
                code: "BROWSER_TAB_CLOSED",
                message: "the tab was closed while it was being created".to_string(),
            });
        }
        let (view, session) = {
            let registry = self.registry.lock();
            let session = registry
                .get(label)
                .and_then(|record| record.session_id().map(str::to_string));
            let view = registry.view(label).ok_or_else(|| TabCreateError {
                code: "BROWSER_TAB_CLOSED",
                message: "the tab closed during creation".to_string(),
            })?;
            (view, session)
        };
        let projection = TabProjection::Updated(view.clone());
        events::publish(self.surface.as_ref(), &self.sinks, session.as_deref(), &projection);
        log::info!("[browser-tab] tab {} live", events::label_prefix(label));
        Ok(view)
    }

    async fn destroy_tab_inner(&self, label: &str) {
        let closing = self.registry.lock().begin_close(label);
        if closing {
            self.surface.close(label).await;
        }
    }
}

/// Run `fut` on the current tokio runtime when there is one (tests), else
/// on tauri's.
fn spawn<F>(fut: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(fut);
        }
        Err(_) => {
            tauri::async_runtime::spawn(fut);
        }
    }
}

impl BrowserHost for DesktopBrowserHost {
    fn execute<'a>(
        &'a self,
        session_id: &'a str,
        agent_id: &'a str,
        call: BrowserCall,
        deadline: Deadline,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Success, BrowserError>> + Send + 'a>>
    {
        Box::pin(async move {
            let spec = dispatch::admit(&call.action, &call.args, self.engine.facts())?;
            if self.registry.lock().is_closing(session_id) {
                return Err(BrowserError::new(ERR_TAB_NOT_FOUND, "this session has ended")
                    .with_reason("session_ended"));
            }
            self.ensure_consent(session_id, agent_id, &call, deadline).await?;
            // After a consent wait the call may no longer have time to run
            // the verb: refuse it now rather than act after the agent gave up.
            if !deadline.admits(spec.budget) {
                return Err(BrowserError::new(
                    ERR_TIMEOUT,
                    format!("not enough time left in this call to run {}; call it again", spec.name),
                )
                .with_reason("deadline"));
            }
            self.act(session_id, spec.verb, call, deadline).await
        })
    }

    fn engine_facts(&self) -> EngineFacts {
        self.engine.facts().clone()
    }

    fn end_session(&self, session_id: &str) {
        if let Some(me) = self.me.upgrade() {
            let session_id = session_id.to_string();
            spawn(async move {
                me.teardown_session(&session_id).await;
            });
        }
    }

    fn create_tab<'a>(
        &'a self,
        label: &'a str,
        url: &'a str,
        bounds: BrowserBounds,
    ) -> HostFuture<'a, Result<TabView, TabCreateError>> {
        Box::pin(self.create_tab_inner(label, url, bounds))
    }

    fn destroy_tab<'a>(&'a self, label: &'a str) -> HostFuture<'a, ()> {
        Box::pin(self.destroy_tab_inner(label))
    }

    fn resolve_consent(&self, request_id: &str, allowed: bool) -> bool {
        self.answer_consent(request_id, allowed)
    }

    fn resolve_eval(&self, nonce: &str, ok: bool, value: Option<String>) -> bool {
        self.engine.deliver_script_reply(nonce, ok, value)
    }
}

#[cfg(test)]
mod tests;
