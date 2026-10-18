//! WebKitGTK tab events tauri does not expose: same-document URL changes,
//! and the downloads of this tab.
//!
//! `notify::uri` fires for every URL change; while a load is in progress
//! the change belongs to that load (its commit carries the URL), so only
//! changes outside a load are same-document commits.
//!
//! Downloads start on the web context every tab shares, so each tab
//! listens there and keeps only the downloads its own webview started.
//! Those are started again by the host, which then holds the download
//! before WebKitGTK picks a destination for it.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use gtk::glib::prelude::*;
use gtk::prelude::WidgetExt;
use tauri::webview::PlatformWebview;
use tauri::Url;
use webkit2gtk::{Download, DownloadExt, URIRequestExt, WebContextExt, WebViewExt};

use crate::browser_automation::engine::events::TabEvents;

pub fn attach(platform: &PlatformWebview, events: TabEvents) -> bool {
    let webview = platform.inner();
    if events.holds_downloads() {
        attach_downloads(&webview, events.clone());
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

fn download_url(download: &Download) -> Option<Url> {
    download.request()?.uri()?.parse().ok()
}

fn attach_downloads(webview: &webkit2gtk::WebView, events: TabEvents) {
    let Some(context) = webview.context() else {
        return;
    };
    let own = webview.downgrade();
    let reissued: Rc<RefCell<Vec<Download>>> = Rc::default();
    let reissuing = Rc::new(Cell::new(false));
    let handler = context.connect_download_started(move |_, download| {
        let Some(own) = own.upgrade() else {
            return;
        };
        if reissuing.get()
            || download.web_view().as_ref() != Some(&own)
            || reissued.borrow().contains(download)
        {
            return;
        }
        // WebKitGTK chooses where a page-started download goes before this
        // handler can, so the host cancels it and starts it again itself,
        // on an object it holds before any byte arrives.
        download.cancel();
        let Some(url) = download_url(download).filter(|u| matches!(u.scheme(), "http" | "https"))
        else {
            log::info!("[browser-agent] download cancelled reason=unsupported_source");
            return;
        };
        reissuing.set(true);
        let again = own.download_uri(url.as_str());
        reissuing.set(false);
        let Some(again) = again else {
            return;
        };
        reissued.borrow_mut().push(again.clone());
        take_download(&again, &url, &events, reissued.clone());
    });
    let handler = RefCell::new(Some(handler));
    webview.connect_destroy(move |_| {
        if let Some(handler) = handler.take() {
            context.disconnect(handler);
        }
    });
}

/// Write `download` only where the host says, and report how it ended.
fn take_download(
    download: &Download,
    url: &Url,
    events: &TabEvents,
    reissued: Rc<RefCell<Vec<Download>>>,
) {
    let Some(path) = events.download_starting(url, Path::new("")) else {
        reissued.borrow_mut().retain(|d| d != download);
        download.cancel();
        return;
    };
    let destination = path.to_string_lossy().into_owned();
    download.connect_decide_destination(move |download, _| {
        download.set_destination(&destination);
        true
    });
    let failed = Rc::new(Cell::new(false));
    let failed_flag = failed.clone();
    download.connect_failed(move |_, _| failed_flag.set(true));
    let finished = events.clone();
    let url = url.clone();
    download.connect_finished(move |download| {
        reissued.borrow_mut().retain(|d| d != download);
        finished.download_finished(&url, !failed.get());
    });
}
