//! The tab surface the core drives for navigation, history, metadata and
//! teardown: the browser pane's tab manager on the desktop, a fake in
//! tests.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::browser_tab_manager::BrowserTabManager;

/// Host-known page metadata. Page-controlled values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabInfo {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryStep {
    Back,
    Forward,
    Reload,
}

pub trait TabSurface: Send + Sync {
    fn info(&self, tab: &str) -> Result<TabInfo, String>;
    fn navigate(&self, tab: &str, url: String) -> Result<(), String>;
    fn history(&self, tab: &str, step: HistoryStep) -> Result<(), String>;
    /// Destroy the native webview; a no-op for unknown tabs.
    fn destroy(&self, tab: &str);
    /// Where screenshots are written.
    fn artifacts_root(&self) -> Result<PathBuf, String>;
}

/// The desktop browser pane.
pub struct PaneSurface {
    pub app: AppHandle,
    pub tabs: Arc<BrowserTabManager>,
}

impl TabSurface for PaneSurface {
    fn info(&self, tab: &str) -> Result<TabInfo, String> {
        self.tabs.info(tab).map(|i| TabInfo {
            url: i.url,
            title: i.title,
        })
    }

    fn navigate(&self, tab: &str, url: String) -> Result<(), String> {
        self.tabs.navigate(tab, url)
    }

    fn history(&self, tab: &str, step: HistoryStep) -> Result<(), String> {
        match step {
            HistoryStep::Back => self.tabs.go_back(tab),
            HistoryStep::Forward => self.tabs.go_forward(tab),
            HistoryStep::Reload => self.tabs.reload(tab),
        }
    }

    fn destroy(&self, tab: &str) {
        let _ = self.tabs.destroy(tab);
    }

    fn artifacts_root(&self) -> Result<PathBuf, String> {
        self.app
            .path()
            .app_data_dir()
            .map(|dir| dir.join("browser-artifacts"))
            .map_err(|e| format!("app_data_dir: {e}"))
    }
}
