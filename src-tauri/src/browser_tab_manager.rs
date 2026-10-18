//! Native webviews for browser tabs: building them with the engine's
//! hooks, confirming the engine really created them, placing, showing and
//! closing them. Tab state (owner, epoch, URL, title) lives in the
//! automation core's tab registry; this module only drives webviews.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

use crate::browser_automation::engine::{events::TabEvents, hooks};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    /// True while an ACP session is driving this tab via the `browser` tool
    /// (`browser_automation`). The renderer shows an "Agent" badge and a
    /// revoke control; user-close revokes control.
    #[serde(default)]
    pub agent_controlled: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct BrowserTabManager {
    app_handle: AppHandle,
    /// Event channel of every webview this manager built, by label.
    tabs: Arc<Mutex<HashMap<String, TabEvents>>>,
    agentation_endpoint: Arc<Mutex<Option<String>>>,
    agentation_enabled: Arc<Mutex<bool>>,
}

impl BrowserTabManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            tabs: Arc::new(Mutex::new(HashMap::new())),

            agentation_endpoint: Arc::new(Mutex::new(None)),
            agentation_enabled: Arc::new(Mutex::new(true)),
        }
    }

    fn get_window(&self) -> Result<tauri::Window, String> {
        self.app_handle
            .get_window("main")
            .ok_or_else(|| "Main window not found".to_string())
    }

    fn get_webview(&self, tab_id: &str) -> Result<tauri::Webview, String> {
        self.app_handle
            .get_webview(tab_id)
            .ok_or_else(|| format!("Webview '{}' not found", tab_id))
    }

    /// Webview handle for in-crate callers (browser_automation eval/CDP).
    pub(crate) fn webview(&self, tab_id: &str) -> Result<tauri::Webview, String> {
        self.get_webview(tab_id)
    }

    /// Build the tab's webview with the engine hooks installed before its
    /// first navigation, then confirm the engine created it. A webview the
    /// engine failed to build is closed and the tab refused.
    pub async fn create(
        &self,
        tab_id: String,
        url: String,
        bounds: BrowserBounds,
        events: TabEvents,
    ) -> Result<(), String> {
        let window = self.get_window()?;
        let parsed_url: tauri::Url = url.parse().map_err(|e| format!("Invalid URL: {}", e))?;

        let mut builder = tauri::webview::WebviewBuilder::new(
            tab_id.clone(),
            tauri::WebviewUrl::External(parsed_url),
        )
        .on_permission_request(crate::browser_tab_permissions::on_permission_request);
        builder = hooks::install(builder, &events);

        // Inject agentation globals via initialization_script (runs before page JS).
        // The toolbar bundle itself is injected via eval AFTER page load,
        // because the agentation library accesses document.head at module
        // top-level — initialization_script runs before DOM exists.
        let (endpoint, enabled) = {
            let ep = self
                .agentation_endpoint
                .lock()
                .map_err(|_| "Lock poisoned")?
                .clone();
            let en = *self
                .agentation_enabled
                .lock()
                .map_err(|_| "Lock poisoned")?;
            (ep, en)
        };
        if enabled {
            if let Some(endpoint_url) = &endpoint {
                let init_script = format!(
                    r#"
                    window.__TERMUL_AGENTATION_ENDPOINT__ = "{endpoint_url}";
                    window.__TERMUL_ANNOTATION_MODE__ = true;
                    "#
                );
                builder = builder.initialization_script(&init_script);
                log::info!(
                    "[BrowserTab] Agentation init script set for tab={} endpoint={}",
                    tab_id,
                    endpoint_url
                );
            }
        }

        let _webview = window
            .add_child(
                builder,
                tauri::LogicalPosition::new(bounds.x, bounds.y),
                tauri::LogicalSize::new(bounds.width, bounds.height),
            )
            .map_err(|e| format!("Failed to create webview: {}", e))?;

        #[cfg(target_os = "linux")]
        {
            // Confirm the main webview still exists before changing the GTK hierarchy.
            if window.get_webview("main").is_none() {
                let _ = _webview.close();
                return Err("Main webview not found".to_string());
            }

            let (reparent_tx, reparent_rx) = tokio::sync::oneshot::channel();
            let reparent_bounds = bounds.clone();
            let dispatch_result = _webview.with_webview(move |child_platform| {
                let result = (|| -> Result<(), String> {
                    use gtk::prelude::*;

                    let child_widget = child_platform.inner();
                    let parent = child_widget
                        .parent()
                        .ok_or_else(|| "Child webview has no parent".to_string())?;
                    let vbox = parent
                        .dynamic_cast::<gtk::Box>()
                        .map_err(|_| "Child webview parent is not GtkBox".to_string())?;

                    // On first creation the main and child webviews are siblings in the
                    // window's default GtkBox. Later tabs reuse the overlay already placed
                    // beside the newly-created child, so no GTK handle crosses callbacks.
                    vbox.remove(&child_widget);
                    let overlay = if let Some(existing) = vbox
                        .children()
                        .into_iter()
                        .find_map(|widget| widget.dynamic_cast::<gtk::Overlay>().ok())
                    {
                        existing
                    } else {
                        let main_widget = vbox
                            .children()
                            .into_iter()
                            .find(|widget| widget.type_().name() == "WebKitWebView")
                            .ok_or_else(|| "Main GTK webview widget not found".to_string())?;
                        let overlay = gtk::Overlay::new();
                        overlay.set_hexpand(true);
                        overlay.set_vexpand(true);
                        main_widget.set_hexpand(true);
                        main_widget.set_vexpand(true);
                        vbox.remove(&main_widget);
                        vbox.pack_start(&overlay, true, true, 0);
                        overlay.add(&main_widget);
                        main_widget.show();
                        overlay.show();
                        overlay
                    };

                    child_widget.set_halign(gtk::Align::Start);
                    child_widget.set_valign(gtk::Align::Start);
                    child_widget.set_margin_start(reparent_bounds.x.round() as i32);
                    child_widget.set_margin_top(reparent_bounds.y.round() as i32);
                    child_widget.set_size_request(
                        reparent_bounds.width.round() as i32,
                        reparent_bounds.height.round() as i32,
                    );
                    overlay.add_overlay(&child_widget);
                    child_widget.show();
                    overlay.show();
                    Ok(())
                })();

                let _ = reparent_tx.send(result);
            });

            if let Err(error) = dispatch_result {
                let _ = _webview.close();
                return Err(format!("Failed to dispatch Linux GTK reparent: {error}"));
            }

            match reparent_rx.await {
                Ok(Ok(())) => {
                    log::info!("[BrowserTab] Linux GTK reparent succeeded");
                }
                Ok(Err(error)) => {
                    let _ = _webview.close();
                    return Err(format!("Linux GTK reparent failed: {error}"));
                }
                Err(error) => {
                    let _ = _webview.close();
                    return Err(format!("Linux GTK reparent callback canceled: {error}"));
                }
            }
        }

        if let Err(error) = hooks::attach(&_webview, events.clone()).await {
            let _ = _webview.close();
            log::warn!("[BrowserTab] tab {tab_id} refused: engine did not confirm the webview ({error})");
            return Err(format!("the browser engine did not create the tab: {error}"));
        }

        self.tabs
            .lock()
            .map_err(|_| "Lock poisoned")?
            .insert(tab_id, events);
        Ok(())
    }

    pub fn navigate(&self, tab_id: &str, url: String) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;
        let parsed_url: tauri::Url = url.parse().map_err(|e| format!("Invalid URL: {}", e))?;
        webview
            .navigate(parsed_url)
            .map_err(|e| format!("Navigation failed: {}", e))?;
        Ok(())
    }

    pub fn resize(&self, tab_id: &str, bounds: BrowserBounds) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;

        #[cfg(target_os = "linux")]
        {
            webview
                .with_webview(move |child_platform| {
                    use gtk::prelude::*;
                    let child_widget = child_platform.inner();
                    child_widget.set_margin_start(bounds.x.round() as i32);
                    child_widget.set_margin_top(bounds.y.round() as i32);
                    child_widget.set_size_request(
                        bounds.width.round() as i32,
                        bounds.height.round() as i32,
                    );
                    child_widget.queue_resize();
                })
                .map_err(|e| format!("Resize dispatch failed: {e}"))?;
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            webview
                .set_bounds(tauri::Rect {
                    position: tauri::LogicalPosition::new(bounds.x, bounds.y).into(),
                    size: tauri::LogicalSize::new(bounds.width, bounds.height).into(),
                })
                .map_err(|e| format!("Resize failed: {}", e))?;
            Ok(())
        }
    }

    pub fn show(&self, tab_id: &str) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;

        #[cfg(target_os = "linux")]
        {
            webview
                .with_webview(move |child_platform| {
                    use gtk::prelude::*;
                    let child_widget = child_platform.inner();
                    child_widget.show();
                })
                .map_err(|e| format!("Show dispatch failed: {e}"))?;
        }

        webview.show().map_err(|e| format!("Show failed: {}", e))?;
        Ok(())
    }

    pub fn hide(&self, tab_id: &str) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;

        #[cfg(target_os = "linux")]
        {
            webview
                .with_webview(move |child_platform| {
                    use gtk::prelude::*;
                    let child_widget = child_platform.inner();
                    child_widget.hide();
                })
                .map_err(|e| format!("Hide dispatch failed: {e}"))?;
        }

        webview.hide().map_err(|e| format!("Hide failed: {}", e))?;
        Ok(())
    }

    /// Set the agentation HTTP endpoint (called when the Rust server starts).
    /// This makes the endpoint available for injection into new browser tabs.
    pub fn set_agentation_endpoint(&self, endpoint: String) {
        let mut ep = self
            .agentation_endpoint
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *ep = Some(endpoint);
    }

    /// Enable/disable agentation annotation mode (feature flag from AppPreferences).
    pub fn set_agentation_enabled(&self, enabled: bool) {
        let mut en = self
            .agentation_enabled
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *en = enabled;
    }

    /// Check if agentation mode is enabled.
    pub fn is_agentation_enabled(&self) -> bool {
        *self
            .agentation_enabled
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Inject the agentation toolbar bundle into a browser tab via eval.
    /// Runs after a page load because the agentation library accesses
    /// document.head at module top-level.
    pub fn inject_agentation_toolbar(&self, tab_id: &str) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;

        // Inject the agentation toolbar bundle (bundled IIFE).
        // The bundle self-checks __TERMUL_AGENTATION_HOST__ to avoid double-mount.
        let toolbar_script = include_str!("../resources/agentation-toolbar.js");
        webview
            .eval(toolbar_script)
            .map_err(|e| format!("Failed to inject agentation toolbar: {}", e))?;

        log::info!(
            "[BrowserTab] Agentation toolbar injected (post-load) for tab={}",
            tab_id
        );
        Ok(())
    }

    /// The toolbar's post-load injection on user tabs, when enabled.
    pub(crate) fn inject_agentation_after_load(&self, tab_id: &str) {
        if !self.is_agentation_enabled() {
            return;
        }
        if let Err(e) = self.inject_agentation_toolbar(tab_id) {
            log::warn!("[BrowserTab] Agentation toolbar injection failed for tab={tab_id}: {e}");
        }
    }

    /// Close the tab's webview and wait until the engine has destroyed it,
    /// then publish the close on the tab's event stream. A no-op for
    /// labels this manager never built.
    pub async fn destroy(&self, tab_id: &str) {
        let events = self
            .tabs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(tab_id);
        let Some(events) = events else {
            return;
        };
        if let Ok(webview) = self.get_webview(tab_id) {
            #[cfg(target_os = "linux")]
            {
                let _ = webview.with_webview(move |child_platform| {
                    use gtk::prelude::*;
                    let child_widget = child_platform.inner();
                    if let Some(parent) = child_widget.parent() {
                        if let Ok(overlay) = parent.dynamic_cast::<gtk::Overlay>() {
                            overlay.remove(&child_widget);
                        }
                    }
                });
            }
            hooks::close(&webview).await;
        }
        events.publish(crate::browser_automation::engine::events::TabEvent::Closed);
    }

    pub fn go_back(&self, tab_id: &str) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;
        webview
            .eval("window.history.back()")
            .map_err(|e| format!("Go back failed: {}", e))?;
        Ok(())
    }

    pub fn go_forward(&self, tab_id: &str) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;
        webview
            .eval("window.history.forward()")
            .map_err(|e| format!("Go forward failed: {}", e))?;
        Ok(())
    }

    pub fn reload(&self, tab_id: &str) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;
        webview
            .reload()
            .map_err(|e| format!("Reload failed: {}", e))?;
        Ok(())
    }

    /// Open DevTools for the webview backing this browser tab.
    ///
    /// Debug-gated: only exists in debug builds. In release builds, the
    /// `browser_tab_open_devtools` Tauri command is a cfg-gated stub that
    /// returns `Ok(IpcResult::error("DevTools disabled in production", ...))`
    /// directly — it never calls this method, so there's no release stub
    /// (avoids dead_code).
    /// The desktop "Toggle DevTools" menu item is already debug-gated
    /// separately in `lib.rs`.
    #[cfg(debug_assertions)]
    pub fn open_devtools(&self, tab_id: &str) -> Result<(), String> {
        let webview = self.get_webview(tab_id)?;
        webview.open_devtools();
        Ok(())
    }

    pub fn destroy_all(&self) {
        let mut tabs = self.tabs.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<String> = tabs.keys().cloned().collect();

        #[cfg(target_os = "linux")]
        {
            for id in &ids {
                if let Ok(webview) = self.get_webview(id) {
                    let _ = webview.with_webview(move |child_platform| {
                        use gtk::prelude::*;
                        let child_widget = child_platform.inner();
                        if let Some(parent) = child_widget.parent() {
                            if let Ok(overlay) = parent.dynamic_cast::<gtk::Overlay>() {
                                overlay.remove(&child_widget);
                            }
                        }
                    });
                }
            }
        }

        for id in ids {
            if let Ok(webview) = self.get_webview(&id) {
                let _ = webview.close();
            }
        }
        tabs.clear();
    }
}

impl Drop for BrowserTabManager {
    fn drop(&mut self) {
        self.destroy_all();
    }
}
