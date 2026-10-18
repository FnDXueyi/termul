//! WKWebView tab events. Starts, commits, loads and titles come from
//! tauri's builder hooks (navigation delegate and title observer). Same-
//! document URL changes need key-value observation of `URL`, which needs
//! an Objective-C class this crate cannot define yet, so `pushState` URLs
//! reach the registry only with the next commit on macOS.

use tauri::webview::PlatformWebview;

use crate::browser_automation::engine::events::TabEvents;

/// Confirm the engine created the `WKWebView`.
pub fn attach(platform: &PlatformWebview, _events: TabEvents) -> bool {
    !platform.inner().is_null()
}
