//! The runner's only view of the automation host.
//!
//! It stands in for the renderer (opens the webview when the host asks for an
//! agent tab) and exposes one `call` that goes through the same entry point
//! the host MCP parent uses, with the deadline the parent stamps on a frame.
//! Fixtures are loopback pages, which never raise a consent ticket; a ticket
//! means a case left loopback, and is answered as unattended. Switching the
//! runner to another host API means editing this file.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::AppHandle;
use termul_manager_lib::conformance_support as host;
use termul_manager_lib::web::sink::{AcpEvent, EventSink};
use tokio::sync::mpsc;

const AGENT_ID: &str = "conformance-agent";
const EVENT_AGENT_TAB: &str = "acp:browser_agent_tab";

/// Right-hand pane geometry in logical pixels, like the app's browser pane.
const TAB_BOUNDS: host::BrowserBounds = host::BrowserBounds {
    x: 400.0,
    y: 0.0,
    width: 800.0,
    height: 800.0,
};

#[derive(Debug, Clone)]
pub struct StepError {
    pub code: String,
    pub reason: Option<String>,
    pub message: String,
}

/// The budget the host documents for one run of `action`.
fn action_budget(action: &str) -> Duration {
    use host::budgets::*;
    match action {
        "navigate" => ACTION_NAVIGATE,
        "new_tab" => ACTION_NEW_TAB,
        "back" | "forward" | "reload" => ACTION_HISTORY,
        "click" => ACTION_CLICK,
        "wait" => ACTION_WAIT,
        "select_option" => ACTION_SELECT,
        "check" | "uncheck" => ACTION_TOGGLE,
        "list_tabs" | "close_tab" | "switch_tab" => ACTION_INSTANT,
        _ => ACTION_EVAL,
    }
}

struct RendererSink(mpsc::UnboundedSender<AcpEvent>);

impl EventSink for RendererSink {
    fn emit(&self, event: &AcpEvent) {
        let _ = self.0.send(event.clone());
    }
}

/// No one answers consent tickets.
struct Unattended;

impl host::PermissionRequest for Unattended {
    fn request(
        &self,
        _ticket: host::TicketRequest,
        _wait: Duration,
    ) -> host::PortFuture<'_, host::TicketOutcome> {
        Box::pin(async { host::TicketOutcome::Unattended })
    }

    fn cancel_session(&self, _session_id: &str) {}
}

pub struct Engine {
    _tabs: Arc<host::BrowserTabManager>,
    _policy_root: tempfile::TempDir,
}

impl Engine {
    pub fn install(app: &AppHandle) -> Self {
        let tabs = Arc::new(host::BrowserTabManager::new(app.clone()));
        tabs.set_agentation_enabled(false);
        let (tx, rx) = mpsc::unbounded_channel();
        let sink: Arc<dyn EventSink> = Arc::new(RendererSink(tx));
        let policy_root = tempfile::tempdir().expect("policy dir");
        let policy = Arc::new(host::BrowserPolicyService::new(
            policy_root.path().to_path_buf(),
            Arc::new(Vec::new),
            Vec::new(),
            Arc::new(host::UnwiredProfileControl),
        ));
        host::set_browser_host(host::DesktopBrowserHost::new(
            app.clone(),
            tabs.clone(),
            vec![sink],
            host::HostServices {
                permissions: Arc::new(Unattended),
                policy,
                project_of: Arc::new(|_| "conformance".to_string()),
            },
        ));
        tauri::async_runtime::spawn(renderer_stand_in(rx));
        Self {
            _tabs: tabs,
            _policy_root: policy_root,
        }
    }

    /// One tool call, as the wire reply an agent would read. A reply that
    /// arrives after the action's documented budget is a `timeout`.
    pub async fn call(&self, session: &str, action: &str, args: Value) -> Result<Value, StepError> {
        let call = host::BrowserCall {
            action: action.to_string(),
            args,
            element: None,
        };
        let frame = host::budgets::FRAME_BUDGET;
        let deadline = host::budgets::Deadline::after(frame);
        let started = Instant::now();
        let reply =
            tokio::time::timeout(frame, host::dispatch(session, AGENT_ID, call, deadline)).await;
        let elapsed = started.elapsed();
        match reply {
            Ok(Ok(success)) if elapsed <= action_budget(action) => Ok(success.to_json()),
            Ok(Ok(_)) => Err(over_budget(action, elapsed)),
            Ok(Err(e)) => Err(StepError {
                code: e.code.to_string(),
                reason: e.reason.map(str::to_string),
                message: e.message,
            }),
            Err(_) => Err(over_budget(action, elapsed)),
        }
    }

    pub fn end_session(&self, session: &str) {
        host::session_ended(session);
    }
}

fn over_budget(action: &str, elapsed: Duration) -> StepError {
    StepError {
        code: "timeout".into(),
        reason: Some("over_budget".into()),
        message: format!(
            "{action} answered after {}s, budget {}s",
            elapsed.as_secs(),
            action_budget(action).as_secs()
        ),
    }
}

/// Mounts every agent tab the host asks for through the registry, as the
/// renderer's `browser_tab_create` does.
async fn renderer_stand_in(mut events: mpsc::UnboundedReceiver<AcpEvent>) {
    while let Some(event) = events.recv().await {
        if event.type_ != EVENT_AGENT_TAB || event.payload["action"] != "open" {
            continue;
        }
        tauri::async_runtime::spawn(async move {
            let Some(tab_id) = event.payload["tabId"].as_str() else {
                return;
            };
            let url = event.payload["url"].as_str().unwrap_or("about:blank");
            if let Err(e) = host::create_tab(tab_id, url, TAB_BOUNDS).await {
                eprintln!("conformance: tab create failed: {} {}", e.code, e.message);
            }
        });
    }
}
