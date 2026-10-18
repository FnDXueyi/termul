//! WKWebView adapter (macOS): isolated-world eval in a named
//! `WKContentWorld` (macOS 11 and later), tab event hooks, JavaScript
//! panels held for the core, and viewport snapshots.

mod eval;
pub mod events;
mod screenshot;
mod ui_delegate;

use std::sync::Arc;

use objc2_web_kit::WKWebView;
use serde_json::Value;
use tokio::sync::oneshot;

use super::assets::Script;
use super::events::DialogAnswer;
use super::{
    dialogs, string_eval, BrowserEngine, EngineError, EngineErrorKind, EngineFacts, EngineFuture,
    InputMode, Point,
};
use crate::browser_automation::budgets::EVAL_STEP;
use crate::browser_tab_manager::BrowserTabManager;

pub struct WkWebViewEngine {
    tabs: Arc<BrowserTabManager>,
    facts: EngineFacts,
}

impl WkWebViewEngine {
    pub fn new(tabs: Arc<BrowserTabManager>) -> Self {
        let facts = if eval::content_worlds_available() {
            string_eval::isolated_facts()
        } else {
            string_eval::no_world_facts("isolated eval needs macOS 11 or later")
        };
        Self { tabs, facts }
    }
}

impl BrowserEngine for WkWebViewEngine {
    fn facts(&self) -> &EngineFacts {
        &self.facts
    }

    fn evaluate<'a>(&'a self, tab: &'a str, script: Script) -> EngineFuture<'a, Value> {
        Box::pin(async move {
            if !self.facts.traits.isolated_world {
                return Err(EngineError::new(EngineErrorKind::Unsupported, true));
            }
            let body = string_eval::call_body(&script);
            let params = string_eval::params_json(&script);
            string_eval::call(&self.tabs, tab, move |platform, done| {
                eval::call(platform, &body, &params, done)
            })
            .await
        })
    }

    fn pointer_click<'a>(
        &'a self,
        _tab: &'a str,
        _at: Point,
        _mode: InputMode,
    ) -> EngineFuture<'a, ()> {
        Box::pin(async { Err(EngineError::new(EngineErrorKind::Unsupported, true)) })
    }

    fn screenshot<'a>(&'a self, tab: &'a str) -> EngineFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let webview = self
                .tabs
                .webview(tab)
                .map_err(|_| EngineError::new(EngineErrorKind::TabNotFound, true))?;
            let (tx, rx) = oneshot::channel();
            webview
                .with_webview(move |platform| {
                    // SAFETY: tauri hands out the tab's live WKWebView, and
                    // this closure runs on the main thread that owns it.
                    let webview = unsafe { &*platform.inner().cast::<WKWebView>() };
                    screenshot::capture(webview, tx);
                })
                .map_err(|_| EngineError::new(EngineErrorKind::Dispatch, true))?;
            match tokio::time::timeout(EVAL_STEP, rx).await {
                Ok(Ok(shot)) => shot.map_err(|kind| EngineError::new(kind, false)),
                Ok(Err(_)) => Err(EngineError::new(EngineErrorKind::NoReply, false)),
                Err(_) => Err(EngineError::new(EngineErrorKind::Timeout, false)),
            }
        })
    }

    fn answer_dialog<'a>(
        &'a self,
        tab: &'a str,
        id: u64,
        answer: DialogAnswer,
    ) -> EngineFuture<'a, bool> {
        Box::pin(dialogs::answer(&self.tabs, tab, id, answer))
    }
}
