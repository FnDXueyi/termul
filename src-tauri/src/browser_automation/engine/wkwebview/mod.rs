//! WKWebView adapter (macOS): isolated-world eval in a named
//! `WKContentWorld` (macOS 11 and later), and tab event hooks.

mod eval;
pub mod events;

use std::sync::Arc;

use serde_json::Value;

use super::assets::Script;
use super::{
    string_eval, BrowserEngine, EngineError, EngineErrorKind, EngineFacts, EngineFuture,
    InputMode, Point,
};
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

    fn screenshot<'a>(&'a self, _tab: &'a str) -> EngineFuture<'a, Vec<u8>> {
        Box::pin(async { Err(EngineError::new(EngineErrorKind::Unsupported, true)) })
    }
}
