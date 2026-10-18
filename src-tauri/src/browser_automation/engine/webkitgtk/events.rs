//! WebKitGTK tab events tauri does not expose: same-document URL changes,
//! and script dialogs on agent tabs. `notify::uri` fires for every URL
//! change; while a load is in progress the change belongs to that load
//! (its commit carries the URL), so only changes outside a load are
//! same-document commits.

use gtk::glib::prelude::*;
use tauri::webview::PlatformWebview;
use webkit2gtk::{SettingsExt, WebViewExt};

use super::dialogs;
use crate::browser_automation::engine::events::{TabEvents, TabRole};

pub fn attach(platform: &PlatformWebview, events: TabEvents) -> bool {
    let webview = platform.inner();
    if events.role() == TabRole::Agent {
        dialogs::attach(&webview, events.clone());
        // Agent input on this engine is page-script events, which the
        // popup blocker refuses; every popup still meets the host's popup
        // policy.
        if let Some(settings) = WebViewExt::settings(&webview) {
            settings.set_javascript_can_open_windows_automatically(true);
        }
    }
    let _ = webview.connect_notify_local(Some("uri"), move |webview, _| {
        if webview.property::<bool>("is-loading") {
            return;
        }
        if let Some(uri) = webview.property::<Option<String>>("uri") {
            events.same_document(&uri);
        }
    });
    true
}
