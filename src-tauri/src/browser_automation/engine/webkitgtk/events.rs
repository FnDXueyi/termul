//! WebKitGTK tab events tauri does not expose: same-document URL changes.
//! `notify::uri` fires for every URL change; while a load is in progress
//! the change belongs to that load (its commit carries the URL), so only
//! changes outside a load are same-document commits.

use gtk::glib::prelude::*;
use tauri::webview::PlatformWebview;

use crate::browser_automation::engine::events::TabEvents;

pub fn attach(platform: &PlatformWebview, events: TabEvents) -> bool {
    let webview = platform.inner();
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
