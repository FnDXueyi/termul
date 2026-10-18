//! Agent tab registry: ownership, default-tab resolution, epochs, and the
//! renderer-mediated open/close lifecycle.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::{BrowserError, DesktopBrowserHost};

/// Renderer round-trip to open a workspace tab + native webview.
const TAB_OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub(super) const EVENT_AGENT_TAB: &str = "acp:browser_agent_tab";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentTabEvent {
    pub action: &'static str, // "open" | "close"
    pub tab_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub session_id: String,
}

#[derive(Debug)]
pub(super) struct AgentTab {
    pub(super) session_id: String,
    /// Advanced on every navigation (agent- or user-driven); snapshot
    /// replies carry it so an agent can detect document changes. Ref
    /// staleness itself is decided page-side (fingerprint re-resolution
    /// → `stale_ref`), not by this counter.
    pub(super) epoch: AtomicU64,
}

impl AgentTab {
    pub(super) fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            epoch: AtomicU64::new(1),
        }
    }

    pub(super) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub(super) fn bump_epoch(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }
}

pub(super) type AgentTabs = HashMap<String, Arc<AgentTab>>;

/// How `navigate` targets a tab.
#[derive(Debug)]
pub(super) enum NavigateResolution {
    /// Retarget this session-owned tab (explicit or default match).
    Reuse(String, Arc<AgentTab>),
    /// No `tabId` given and the session owns no agent tab — open one.
    AutoOpen,
    /// An explicit non-resolving `tabId` never auto-opens.
    TabNotFound(BrowserError),
}

/// Decide which tab `navigate` targets. `tab_id: None` reuses the
/// session's default tab or auto-opens when the session owns none; an
/// explicit `tab_id` must resolve to a tab owned by this session or the
/// call fails with `tab_not_found` and opens nothing.
pub(super) fn navigate_resolution(
    tabs: &AgentTabs,
    session_id: &str,
    tab_id: Option<&str>,
) -> NavigateResolution {
    match owned_tab_or_default(tabs, session_id, tab_id) {
        Ok(Some((id, tab))) => NavigateResolution::Reuse(id, tab),
        Ok(None) => NavigateResolution::AutoOpen,
        Err(e) => NavigateResolution::TabNotFound(e),
    }
}

/// The session's default tab: the lexicographically smallest owned tab id
/// — deterministic across calls and processes (HashMap iteration order
/// is not). Several owned tabs can exist (`new_tab` mints by design).
pub(super) fn default_session_tab(
    tabs: &AgentTabs,
    session_id: &str,
) -> Option<(String, Arc<AgentTab>)> {
    tabs.iter()
        .filter(|(_, t)| t.session_id == session_id)
        .min_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(id, tab)| (id.clone(), tab.clone()))
}

/// Tab-resolution rule shared by every action: an explicit `tab_id` must
/// name a tab owned by `session_id`, otherwise `tab_not_found`; `None`
/// resolves the session's default tab, or `None` when it owns no tab (the
/// caller decides whether that means auto-open or an error).
pub(super) fn owned_tab_or_default(
    tabs: &AgentTabs,
    session_id: &str,
    tab_id: Option<&str>,
) -> Result<Option<(String, Arc<AgentTab>)>, BrowserError> {
    if let Some(id) = tab_id {
        return match tabs.get(id) {
            Some(tab) if tab.session_id == session_id => Ok(Some((id.to_string(), tab.clone()))),
            _ => Err(BrowserError::tab_not_found(id).with_arg_keys(&["tabId"])),
        };
    }
    Ok(default_session_tab(tabs, session_id))
}

/// User/script-driven navigation inside an agent tab advances that tab's
/// epoch. A tab id that is not an agent tab is a no-op.
pub(super) fn bump_epoch_if_agent_tab(tabs: &AgentTabs, tab_id: &str) {
    if let Some(tab) = tabs.get(tab_id) {
        tab.bump_epoch();
    }
}

impl DesktopBrowserHost {
    pub(super) fn agent_tab(&self, tab_id: &str) -> Option<Arc<AgentTab>> {
        self.agent_tabs.lock().get(tab_id).cloned()
    }

    /// Resolve the target tab for an action. `None` → the session's
    /// default agent tab; creating one is the caller's job (needs a URL).
    pub(super) fn resolve_tab(
        &self,
        session_id: &str,
        tab_id: Option<&str>,
    ) -> Result<(String, Arc<AgentTab>), BrowserError> {
        let tabs = self.agent_tabs.lock();
        owned_tab_or_default(&tabs, session_id, tab_id)?.ok_or_else(|| {
            BrowserError::tab_not_found("(no agent tab — call navigate or new_tab first)")
                .with_arg_keys(&["tabId"])
        })
    }

    /// Open a renderer-mediated agent tab: emit the open event, wait for the
    /// renderer to mount the pane and call `browser_tab_create`.
    pub(super) async fn open_agent_tab(
        &self,
        session_id: &str,
        url: Option<&str>,
    ) -> Result<String, BrowserError> {
        let tab_id = format!("agent-{}", Uuid::new_v4());
        let (tx, rx) = oneshot::channel();
        self.pending_tab_open.lock().insert(tab_id.clone(), tx);
        self.emit(
            Some(session_id),
            EVENT_AGENT_TAB,
            &BrowserAgentTabEvent {
                action: "open",
                tab_id: tab_id.clone(),
                url: url.map(str::to_string),
                session_id: session_id.to_string(),
            },
        );
        match tokio::time::timeout(TAB_OPEN_TIMEOUT, rx).await {
            Ok(Ok(Ok(()))) => {
                self.agent_tabs
                    .lock()
                    .insert(tab_id.clone(), Arc::new(AgentTab::new(session_id)));
                log::info!(
                    "[browser-agent] opened agent tab {} for session {}",
                    tab_id,
                    crate::logging::redact_session_id(session_id)
                );
                Ok(tab_id)
            }
            Ok(Ok(Err(e))) => {
                self.pending_tab_open.lock().remove(&tab_id);
                Err(BrowserError::internal(e))
            }
            _ => {
                self.pending_tab_open.lock().remove(&tab_id);
                Err(BrowserError::capability(
                    "renderer did not open the agent tab (no desktop UI attached?)",
                ))
            }
        }
    }

    pub(super) fn close_agent_tab(&self, tab_id: &str) {
        let removed = self.agent_tabs.lock().remove(tab_id);
        if let Some(tab) = removed {
            self.emit(
                Some(&tab.session_id),
                EVENT_AGENT_TAB,
                &BrowserAgentTabEvent {
                    action: "close",
                    tab_id: tab_id.to_string(),
                    url: None,
                    session_id: tab.session_id.clone(),
                },
            );
            // The renderer removes the workspace tab on the event; also
            // destroy the native webview directly so a wedged renderer can't
            // leak it.
            self.surface.destroy(tab_id);
            log::info!("[browser-agent] closed agent tab {tab_id}");
        }
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;
