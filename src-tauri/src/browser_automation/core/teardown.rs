//! Session teardown in a fixed order, and the startup sweep of per-session
//! directories a crashed or killed app left behind.
//!
//! Order on session end: refuse admissions, deny pending prompts and
//! revoke the session's grant, close every tab the session owns and wait
//! for the engine to destroy each webview, then delete the session's
//! directory (where its ephemeral profile, staging and downloads live).
//! Deleting before the engine let go of a profile fails on Windows while
//! the browser process still holds its files.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::events::label_prefix;
use super::surface::TabSurface;
use super::tabs::{BrowserAgentTabEvent, EVENT_AGENT_TAB};
use super::DesktopBrowserHost;

/// Attempts at deleting a session directory whose files the engine may
/// still hold for a moment after its webviews closed.
const DELETE_ATTEMPTS: u32 = 5;
const DELETE_RETRY: Duration = Duration::from_millis(500);

/// The session's directory under `root`. Session ids are agent-generated:
/// only `[A-Za-z0-9_-]` survive, so an id can never name another path.
pub(super) fn session_dir(root: &Path, session_id: &str) -> PathBuf {
    let safe: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    root.join(if safe.is_empty() {
        "_".to_string()
    } else {
        safe
    })
}

/// Delete `dir`, retrying while files are still held. Missing is done.
pub(super) fn delete_dir(dir: &Path) -> bool {
    for attempt in 1..=DELETE_ATTEMPTS {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
            Err(_) if attempt < DELETE_ATTEMPTS => std::thread::sleep(DELETE_RETRY),
            Err(_) => {}
        }
    }
    false
}

/// Delete every per-session directory under `root`. Returns how many
/// were removed.
pub(super) fn sweep(root: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|entry| delete_dir(&entry.path()))
        .count()
}

pub(super) fn spawn_startup_sweep(surface: Arc<dyn TabSurface>) {
    let Ok(root) = surface.session_data_root() else {
        return;
    };
    std::thread::spawn(move || {
        let removed = sweep(&root);
        if removed > 0 {
            log::info!(
                "[browser-agent] startup sweep removed {removed} leftover session directories"
            );
        }
    });
}

impl DesktopBrowserHost {
    pub(super) async fn teardown_session(&self, session_id: &str) {
        let redacted = crate::logging::redact_session_id(session_id);
        self.registry.lock().begin_session_teardown(session_id);
        self.forget_consent(session_id);
        let tabs = self.registry.lock().session_tabs(session_id);
        for label in &tabs {
            self.close_agent_tab(label, session_id).await;
        }
        if let Ok(root) = self.surface.session_data_root() {
            let dir = session_dir(&root, session_id);
            let deleted = tokio::task::spawn_blocking(move || delete_dir(&dir))
                .await
                .unwrap_or(false);
            if !deleted {
                log::warn!("[browser-agent] session {redacted} directory not deleted; the startup sweep retries");
            }
        }
        self.registry.lock().end_session_teardown(session_id);
        log::info!(
            "[browser-agent] session {redacted} torn down ({} tab(s) closed)",
            tabs.len()
        );
    }

    /// Ask the renderer to drop the agent tab's pane tab, then close the
    /// webview and wait for the engine to destroy it.
    pub(super) async fn close_agent_tab(&self, label: &str, session_id: &str) {
        self.emit(
            Some(session_id),
            EVENT_AGENT_TAB,
            &BrowserAgentTabEvent {
                action: "close",
                tab_id: label.to_string(),
                url: None,
                session_id: session_id.to_string(),
            },
        );
        self.destroy_tab_inner(label).await;
        log::info!("[browser-agent] closed agent tab {}", label_prefix(label));
    }
}

#[cfg(test)]
#[path = "teardown_tests.rs"]
mod tests;
