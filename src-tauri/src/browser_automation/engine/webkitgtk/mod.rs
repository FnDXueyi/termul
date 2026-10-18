//! WebKitGTK adapter (Linux): isolated-world eval in a named script
//! world, and tab event hooks.

mod eval;
pub mod events;
pub mod input;

use std::sync::Arc;

use serde_json::Value;

use super::assets::Script;
use super::{
    string_eval, BrowserEngine, EngineError, EngineErrorKind, EngineFacts, EngineFuture,
    InputMode, Point,
};
use crate::browser_tab_manager::BrowserTabManager;

pub struct WebKitGtkEngine {
    tabs: Arc<BrowserTabManager>,
    facts: EngineFacts,
}

impl WebKitGtkEngine {
    pub fn new(tabs: Arc<BrowserTabManager>) -> Self {
        Self {
            tabs,
            facts: string_eval::isolated_facts(),
        }
    }
}

impl BrowserEngine for WebKitGtkEngine {
    fn facts(&self) -> &EngineFacts {
        &self.facts
    }

    fn evaluate<'a>(&'a self, tab: &'a str, script: Script) -> EngineFuture<'a, Value> {
        Box::pin(async move {
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
