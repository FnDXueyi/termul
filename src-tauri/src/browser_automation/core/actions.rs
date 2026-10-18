//! Per-verb handlers: argument checks and the engine calls they make.

use serde_json::{json, Value};
use tauri::Manager;
use uuid::Uuid;

use super::dispatch::Verb;
use super::reply::{engine_error, eval_gate, shape_snapshot};
use super::tabs::{default_session_tab, navigate_resolution, NavigateResolution};
use super::{BrowserError, DesktopBrowserHost, ERR_TIMEOUT};
use crate::browser_automation::engine::{assets, CapabilityId, InputMode, Point, Support};
use crate::browser_automation::BrowserCall;

/// Post-navigate settle poll budget.
const NAV_SETTLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Validate + normalize a navigation target. http(s) only.
pub(super) fn check_url(url: &str) -> Result<String, BrowserError> {
    let parsed = url
        .parse::<tauri::Url>()
        .map_err(|_| BrowserError::invalid(format!("invalid url: {url}")).with_arg_keys(&["url"]))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        other => Err(BrowserError::invalid(format!(
            "scheme '{other}' is not allowed (http/https only)"
        ))
        .with_arg_keys(&["url"])),
    }
}

/// Extract the required `url` arg for navigate/new_tab — a missing or
/// non-string value fails with `invalid_params` naming the key.
pub(super) fn require_url_arg(args: &Value, action: &str) -> Result<String, BrowserError> {
    args.get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| BrowserError::invalid(format!("{action} needs 'url'")).with_arg_keys(&["url"]))
}

/// Extract the optional `tabId` arg — absent (or JSON null) → `None`
/// (default-tab resolution); a non-string or empty value fails with
/// `invalid_params` naming the key. Silently treating a malformed
/// `tabId` as absent would retarget a tab the caller never named.
pub(super) fn tab_id_arg(args: &Value, action: &str) -> Result<Option<String>, BrowserError> {
    match args.get("tabId") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
        Some(_) => Err(BrowserError::invalid(format!(
            "{action}: 'tabId' must be a non-empty string when provided"
        ))
        .with_arg_keys(&["tabId"])),
    }
}

/// Refs are host-minted `@eN` tokens. Validating before script build
/// keeps an agent-supplied value out of the page entirely (defense in
/// depth on top of the script builders' JSON encoding).
pub(super) fn check_ref(ref_id: &str) -> Result<(), BrowserError> {
    let ok = ref_id.starts_with("@e")
        && ref_id.len() > 2
        && ref_id[2..].chars().all(|c| c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(BrowserError::invalid(format!("invalid ref '{ref_id}' (expected @eN)")).with_arg_keys(&["ref"]))
    }
}

impl DesktopBrowserHost {
    /// Run a helper script through the engine. A failure is the eval's
    /// final outcome (the engine already applied its one safe fallback).
    async fn eval(&self, tab_id: &str, expr: &str) -> Result<Value, BrowserError> {
        self.engine.evaluate(tab_id, expr).await.map_err(|e| {
            // Tab id + kind only: eval failures can quote page content.
            log::warn!("[browser-agent] eval failed on tab {tab_id} ({:?})", e.kind);
            engine_error(e, tab_id)
        })
    }

    /// Wait until `document.readyState` reaches interactive/complete.
    async fn settle(&self, tab_id: &str) {
        let deadline = std::time::Instant::now() + NAV_SETTLE_TIMEOUT;
        loop {
            let state = self.eval(tab_id, assets::READY_STATE).await.ok();
            if matches!(state.as_ref().and_then(Value::as_str), Some("complete" | "interactive")) {
                return;
            }
            // A slow SPA still settles Ok; the agent retries reads on demand.
            if std::time::Instant::now() > deadline {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    pub(super) async fn act(
        &self,
        session_id: &str,
        verb: Verb,
        call: BrowserCall,
    ) -> Result<Value, BrowserError> {
        let args = &call.args;
        match verb {
            Verb::ListTabs => {
                let tabs = self.agent_tabs.lock();
                let list: Vec<Value> = tabs
                    .iter()
                    .filter(|(_, t)| t.session_id == session_id)
                    .filter_map(|(id, _)| {
                        self.tabs
                            .info(id)
                            .ok()
                            .map(|info| json!({ "tabId": id, "url": info.url, "title": info.title }))
                    })
                    .collect();
                Ok(json!({ "tabs": list }))
            }
            Verb::NewTab => {
                let url = check_url(&require_url_arg(args, "new_tab")?)?;
                let tab_id = self.open_agent_tab(session_id, Some(&url)).await?;
                Ok(json!({ "tabId": tab_id }))
            }
            Verb::CloseTab => {
                let (tab_id, _) = self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))?;
                self.close_agent_tab(&tab_id);
                Ok(json!({ "closed": tab_id }))
            }
            Verb::Navigate => self.navigate(session_id, args).await,
            Verb::Back | Verb::Forward | Verb::Reload => {
                let (tab_id, tab) = self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))?;
                let r = match verb {
                    Verb::Back => self.tabs.go_back(&tab_id),
                    Verb::Forward => self.tabs.go_forward(&tab_id),
                    _ => self.tabs.reload(&tab_id),
                };
                r.map_err(BrowserError::internal)?;
                tab.bump_epoch();
                self.settle(&tab_id).await;
                Ok(json!({ "ok": true }))
            }
            Verb::Snapshot => {
                let (tab_id, tab) = self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))?;
                let epoch = tab.epoch();
                let out = self.eval(&tab_id, &assets::snapshot_script()).await?;
                shape_snapshot(&out, &tab_id, epoch)
            }
            Verb::Screenshot => {
                let (tab_id, _) = self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))?;
                self.screenshot(session_id, &tab_id).await
            }
            Verb::Click => {
                let (tab_id, tab) = self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))?;
                let el = args
                    .get("ref")
                    .and_then(Value::as_str)
                    .ok_or_else(|| BrowserError::invalid("missing 'ref'").with_arg_keys(&["ref"]))?;
                check_ref(el)?;
                self.click_ref(&tab_id, el, tab.epoch()).await?;
                Ok(json!({ "ok": true }))
            }
            Verb::Fill => {
                let (tab_id, tab) = self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))?;
                let ref_id = args
                    .get("ref")
                    .and_then(Value::as_str)
                    .ok_or_else(|| BrowserError::invalid("missing 'ref'").with_arg_keys(&["ref"]))?;
                check_ref(ref_id)?;
                let value = args
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(|| BrowserError::invalid("missing 'value'").with_arg_keys(&["value"]))?;
                let out = self
                    .eval(&tab_id, &assets::fill_ref(ref_id, value, tab.epoch()))
                    .await?;
                eval_gate(out)?;
                Ok(json!({ "ok": true }))
            }
            Verb::Type | Verb::Press | Verb::Scroll | Verb::Hover => {
                let (tab_id, tab) = self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))?;
                // `ref` is optional for these (type/press fall back to the
                // focused element); validate when present.
                if let Some(r) = args.get("ref").and_then(Value::as_str) {
                    check_ref(r)?;
                }
                let out = self
                    .eval(&tab_id, &assets::interaction(&call.action, args, tab.epoch()))
                    .await?;
                eval_gate(out)?;
                Ok(json!({ "ok": true }))
            }
            Verb::Wait => self.wait(session_id, args).await,
        }
    }

    async fn navigate(&self, session_id: &str, args: &Value) -> Result<Value, BrowserError> {
        let url = check_url(&require_url_arg(args, "navigate")?)?;
        let tab_id = tab_id_arg(args, "navigate")?;
        let resolution = {
            let tabs = self.agent_tabs.lock();
            navigate_resolution(&tabs, session_id, tab_id.as_deref())
        };
        let (tab_id, tab, fresh) = match resolution {
            NavigateResolution::Reuse(id, tab) => (id, tab, false),
            // Auto-open ONLY when no `tabId` was given: the first navigate
            // creates the visible tab already pointing at the target URL
            // (the renderer loads it on mount — no double nav).
            NavigateResolution::AutoOpen => {
                // The resolution and the open are not atomic: re-check under
                // the lock immediately before minting and reuse a concurrent
                // winner's tab so a race can't produce two tabs.
                let winner = default_session_tab(&self.agent_tabs.lock(), session_id);
                match winner {
                    Some((id, tab)) => (id, tab, false),
                    None => {
                        let id = self.open_agent_tab(session_id, Some(&url)).await?;
                        let tab = self
                            .agent_tab(&id)
                            .ok_or_else(|| BrowserError::tab_not_found(&id).with_arg_keys(&["tabId"]))?;
                        (id, tab, true)
                    }
                }
            }
            // An explicit non-resolving `tabId` is a caller error: never
            // mint another tab (that let a bad target proliferate tabs).
            NavigateResolution::TabNotFound(e) => {
                log::warn!(
                    "[browser-agent] navigate: explicit tabId not owned by session {} ({}), no auto-open",
                    crate::logging::redact_session_id(session_id),
                    e.code
                );
                return Err(e);
            }
        };
        if !fresh {
            self.tabs
                .navigate(&tab_id, url.clone())
                .map_err(BrowserError::internal)?;
        }
        tab.bump_epoch();
        self.settle(&tab_id).await;
        // Report the tab's real URL — never fabricate the requested one.
        let url_now = self
            .tabs
            .info(&tab_id)
            .map(|i| i.url)
            .map_err(|_| BrowserError::tab_not_found(&tab_id).with_arg_keys(&["tabId"]))?;
        Ok(json!({ "tabId": tab_id, "url": url_now }))
    }

    async fn wait(&self, session_id: &str, args: &Value) -> Result<Value, BrowserError> {
        let (tab_id, _) = self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))?;
        if let Some(ms) = args.get("ms").and_then(Value::as_u64) {
            tokio::time::sleep(std::time::Duration::from_millis(ms.min(30_000))).await;
            return Ok(json!({ "ok": true }));
        }
        let text = args.get("text").and_then(Value::as_str).ok_or_else(|| {
            BrowserError::invalid("wait needs 'ms' or 'text'").with_arg_keys(&["ms", "text"])
        })?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let found = self
                .eval(&tab_id, &assets::contains_text(text))
                .await
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if found {
                return Ok(json!({ "ok": true }));
            }
            if std::time::Instant::now() > deadline {
                return Err(BrowserError::new(ERR_TIMEOUT, "wait: text did not appear within 30s")
                    .with_arg_keys(&["text"]));
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    }

    /// Click an `@eN` ref: resolve to a rect (the page re-verifies the
    /// fingerprint and reports `stale_ref` if the DOM changed), then a
    /// trusted click where the engine has one, else a synthetic DOM click.
    async fn click_ref(&self, tab_id: &str, ref_id: &str, epoch: u64) -> Result<(), BrowserError> {
        let mode = match self.engine.facts().capabilities.get(CapabilityId::PointerClick) {
            Support::Trusted => InputMode::Trusted,
            Support::Synthetic => InputMode::Synthetic,
            Support::Unavailable(reason) => return Err(BrowserError::capability(reason)),
        };
        if mode == InputMode::Trusted {
            let out = self.eval(tab_id, &assets::resolve_ref(ref_id, epoch)).await?;
            eval_gate(out.clone())?;
            let rect = out.get("rect").ok_or_else(|| {
                BrowserError::new(super::ERR_STALE_REF, format!("{ref_id} no longer resolves"))
                    .with_arg_keys(&["ref"])
            })?;
            let at = Point {
                x: rect.get("cx").and_then(Value::as_f64).unwrap_or(0.0),
                y: rect.get("cy").and_then(Value::as_f64).unwrap_or(0.0),
            };
            match self.engine.pointer_click(tab_id, at, InputMode::Trusted).await {
                Ok(()) => return Ok(()),
                Err(e) => log::warn!(
                    "[browser-agent] trusted click failed on tab {tab_id} ({:?}), synthetic fallback",
                    e.kind
                ),
            }
        }
        let out = self.eval(tab_id, &assets::dom_click(ref_id, epoch)).await?;
        eval_gate(out)
    }

    async fn screenshot(&self, session_id: &str, tab_id: &str) -> Result<Value, BrowserError> {
        let png = self
            .engine
            .screenshot(tab_id)
            .await
            .map_err(|e| engine_error(e, tab_id))?;
        // The session id is agent-generated — sanitize before it ever
        // touches a path (only [A-Za-z0-9_-] survive).
        let safe_sid: String = session_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|e| BrowserError::internal(format!("app_data_dir: {e}")))?
            .join("browser-artifacts")
            .join(if safe_sid.is_empty() { "_".to_string() } else { safe_sid });
        std::fs::create_dir_all(&dir).map_err(|e| BrowserError::internal(format!("artifact dir: {e}")))?;
        let path = dir.join(format!("shot-{}.png", Uuid::new_v4()));
        std::fs::write(&path, png).map_err(|e| BrowserError::internal(format!("write artifact: {e}")))?;
        Ok(json!({ "path": path.to_string_lossy() }))
    }
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
