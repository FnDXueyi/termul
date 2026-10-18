//! Agent browser automation for the embedded Termul browser pane.
//!
//! The host-injected `termul` MCP tool `browser` (see `acp::host_mcp`) forwards
//! `FrameKind::Browser` calls here. A `BrowserHost` trait keeps the parent
//! runtime-neutral: the desktop registers a [`DesktopBrowserHost`] at setup;
//! `termul-server` and tests that never register one get a fail-closed
//! `capability_unavailable`.
//!
//! Layout: `core` owns the tab registry, consent, verb dispatch and reply
//! shaping and drives engines only through the `engine::BrowserEngine`
//! port. Per-OS adapters live under `engine/`: WebView2 (Windows, in-process
//! CDP) and the JS-eval bridge (macOS/Linux, and the Windows fallback).
//!
//! Logging: boundary/failure lines only — never page contents, cookies, or
//! typed values (CWE-532).

pub mod core;
pub mod engine;

#[cfg(test)]
pub use self::core::boundary::BROWSER_CALL_FAILED_PREFIX;
pub use self::core::boundary::{boundary_arg_keys, boundary_failure_line};
pub use self::core::{expected_args_text, BrowserError, DesktopBrowserHost, Success, ToolSchema};
#[cfg(test)]
pub use self::core::{
    ERR_CAPABILITY, ERR_CONFIRMATION, ERR_INTERNAL, ERR_INVALID_PARAMS, ERR_STALE_REF,
    ERR_TAB_NOT_FOUND, ERR_TIMEOUT,
};

use std::sync::{Arc, RwLock};

use engine::EngineFacts;

use serde::Deserialize;
use serde_json::Value;

/// One `browser` tool call decoded at the frame boundary.
#[derive(Debug, Clone, Deserialize)]
pub struct BrowserCall {
    pub action: String,
    #[serde(default)]
    pub args: Value,
    /// Agent-stated intent for mutating actions (shown in consent/audit UI).
    #[serde(default)]
    pub element: Option<String>,
}

/// Host-side capability. The desktop registers the real impl; standalone
/// servers and tests leave it unset — `dispatch` then fails closed.
pub trait BrowserHost: Send + Sync {
    fn execute<'a>(
        &'a self,
        session_id: &'a str,
        agent_id: &'a str,
        call: BrowserCall,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Success, BrowserError>> + Send + 'a>>;
    fn end_session(&self, session_id: &str);
    /// Called by `browser_tab_create` — returns true when the tab was a
    /// pending agent tab (so the caller can flag it agent-controlled).
    fn notify_tab_created(&self, tab_id: &str) -> bool;
    fn notify_tab_closed(&self, tab_id: &str);
    /// Advance the per-tab epoch (rides snapshot replies so an agent can
    /// detect document changes; ref staleness is decided page-side).
    fn notify_tab_navigated(&self, tab_id: &str);
    fn resolve_consent(&self, request_id: &str, allowed: bool) -> bool;
    /// Resolve a pending eval bridge reply from `browser_agent_eval_result`.
    fn resolve_eval(&self, nonce: &str, ok: bool, value: Option<String>) -> bool;
    /// The registered engine's frozen capability table and traits.
    fn engine_facts(&self) -> EngineFacts;
}

static BROWSER_HOST: RwLock<Option<Arc<dyn BrowserHost>>> = RwLock::new(None);

/// Register the desktop host (called once from `lib.rs` setup).
pub fn set_browser_host(host: Arc<dyn BrowserHost>) {
    *BROWSER_HOST.write().unwrap_or_else(|e| e.into_inner()) = Some(host);
}

pub fn browser_host() -> Option<Arc<dyn BrowserHost>> {
    BROWSER_HOST
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Entry point used by `host_mcp::parent` for `FrameKind::Browser`.
///
/// No pre-stamping of `arg_keys` here or in `DesktopBrowserHost::execute`:
/// the parent log site's `boundary_arg_keys` fallback is the SINGLE place
/// the action table is applied (pre-stamps are unobservable — they
/// produce the same line — and erase the site-keys-vs-table distinction).
pub async fn dispatch(
    session_id: &str,
    agent_id: &str,
    call: BrowserCall,
) -> Result<Success, BrowserError> {
    match browser_host() {
        Some(host) => host.execute(session_id, agent_id, call).await,
        None => Err(BrowserError::capability(
            "browser automation is only available on the desktop app",
        )
        .with_reason("no_automation_host")),
    }
}

/// The `browser` tool schema served to host_mcp children: built from the
/// registered host's engine, or the fail-closed schema when none is.
pub fn describe() -> ToolSchema {
    let facts = browser_host().map(|host| host.engine_facts());
    self::core::schema(facts.as_ref())
}

/// Called by `HostPlanServer::unregister_session` — closes the session's
/// agent tabs and drops its consent grant.
pub fn session_ended(session_id: &str) {
    if let Some(host) = browser_host() {
        host.end_session(session_id);
    }
}

/// `browser_consent_respond` command → resolve the pending consent prompt.
pub fn consent_responded(request_id: &str, allowed: bool) -> bool {
    match browser_host() {
        Some(host) => host.resolve_consent(request_id, allowed),
        None => false,
    }
}

/// `browser_agent_eval_result` command → resolve the pending eval waiter.
/// Returns false when the nonce is unknown (stale/unrelated page report).
pub fn eval_resolved(nonce: &str, ok: bool, value: Option<String>) -> bool {
    match browser_host() {
        Some(host) => host.resolve_eval(nonce, ok, value),
        None => false,
    }
}

/// `browser_tab_create` hook — returns true when `tab_id` is a pending agent
/// tab (the caller marks it agent-controlled).
pub fn tab_created(tab_id: &str) -> bool {
    match browser_host() {
        Some(host) => host.notify_tab_created(tab_id),
        None => false,
    }
}

/// `browser_tab_destroy` / user-close hook.
pub fn tab_closed(tab_id: &str) {
    if let Some(host) = browser_host() {
        host.notify_tab_closed(tab_id);
    }
}

/// `browser_tab_report_url` hook — user/script-driven navigation inside an
/// agent tab invalidates stored refs too.
pub fn tab_navigated(tab_id: &str) {
    if let Some(host) = browser_host() {
        host.notify_tab_navigated(tab_id);
    }
}

/// Test hook: drop the registered host so `dispatch`-level tests can assert
/// the no-host fail-closed path deterministically.
#[cfg(test)]
pub fn clear_browser_host() {
    *BROWSER_HOST.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Serializes every test that touches the process-global `BROWSER_HOST`
/// (this module's `mod tests` plus `acp::host_mcp::parent::tests`).
#[cfg(test)]
pub(crate) static TEST_HOST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests;
