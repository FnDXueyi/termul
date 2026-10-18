//! Per-verb handlers: argument checks, the engine and surface calls they
//! make, and the success reply each one shapes.

use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use super::dispatch::Verb;
use super::ports::{CanonicalOrigin, TabProfile};
use super::reply::{engine_error, eval_gate, shape_snapshot, Fidelity, PageData, Success, TabFacts};
use super::surface::HistoryStep;
use super::tabs::{default_session_tab, navigate_resolution, AgentTab, NavigateResolution};
use super::{BrowserError, DesktopBrowserHost, ERR_TIMEOUT};
use crate::browser_automation::budgets::{self, Deadline};
use crate::browser_automation::engine::{assets, CapabilityId, InputMode, Point, Support};
use crate::browser_automation::BrowserCall;

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

fn ref_arg(args: &Value) -> Result<Option<&str>, BrowserError> {
    let r = args.get("ref").and_then(Value::as_str);
    if let Some(r) = r {
        check_ref(r)?;
    }
    Ok(r)
}

fn required_ref(args: &Value) -> Result<&str, BrowserError> {
    ref_arg(args)?.ok_or_else(|| BrowserError::invalid("missing 'ref'").with_arg_keys(&["ref"]))
}

fn mutated(fidelity: Fidelity, tab: TabFacts) -> Success {
    Success {
        fidelity: Some(fidelity),
        tab: Some(tab),
        ..Success::default()
    }
}

impl DesktopBrowserHost {
    /// Host facts for `id`, its origin from the tab's current URL.
    fn tab_facts(&self, id: &str, tab: &AgentTab) -> TabFacts {
        let origin = self
            .surface
            .info(id)
            .ok()
            .and_then(|info| CanonicalOrigin::parse(&info.url).ok());
        TabFacts {
            id: id.to_string(),
            epoch: tab.epoch(),
            origin,
            profile: TabProfile::Shared,
        }
    }

    fn page_meta(&self, id: &str) -> Option<PageData> {
        self.surface
            .info(id)
            .ok()
            .map(|info| PageData::meta(&info.url, &info.title))
    }

    fn resolve_arg_tab(
        &self,
        session_id: &str,
        args: &Value,
    ) -> Result<(String, Arc<AgentTab>), BrowserError> {
        self.resolve_tab(session_id, args.get("tabId").and_then(Value::as_str))
    }

    /// Run a helper script through the engine. A failure is the eval's
    /// final outcome (the engine already applied its one safe fallback).
    async fn eval(&self, tab_id: &str, expr: &str) -> Result<Value, BrowserError> {
        self.engine.evaluate(tab_id, expr).await.map_err(|e| {
            // Tab id + kind only: eval failures can quote page content.
            log::warn!("[browser-agent] eval failed on tab {tab_id} ({:?})", e.kind);
            engine_error(e, tab_id)
        })
    }

    /// Wait until `document.readyState` reaches interactive/complete. A
    /// slow SPA still settles Ok; the agent retries reads on demand.
    async fn settle(&self, tab_id: &str, deadline: Deadline) {
        let settle = Deadline::after(deadline.clamp(budgets::NAV_SETTLE));
        loop {
            let state = self.eval(tab_id, assets::READY_STATE).await.ok();
            if matches!(state.as_ref().and_then(Value::as_str), Some("complete" | "interactive")) {
                return;
            }
            if settle.remaining().is_zero() {
                return;
            }
            tokio::time::sleep(settle.clamp(budgets::SETTLE_POLL)).await;
        }
    }

    pub(super) async fn act(
        &self,
        session_id: &str,
        verb: Verb,
        call: BrowserCall,
        deadline: Deadline,
    ) -> Result<Success, BrowserError> {
        let args = &call.args;
        match verb {
            Verb::ListTabs => Ok(self.list_tabs(session_id)),
            Verb::NewTab => {
                let url = check_url(&require_url_arg(args, "new_tab")?)?;
                let id = self.open_agent_tab(session_id, Some(&url)).await?;
                let tab = self
                    .agent_tab(&id)
                    .ok_or_else(|| BrowserError::tab_not_found(&id).with_arg_keys(&["tabId"]))?;
                let mut facts = self.tab_facts(&id, &tab);
                // The pane may not report the URL until it has loaded.
                facts.origin = facts.origin.or_else(|| CanonicalOrigin::parse(&url).ok());
                Ok(mutated(Fidelity::Trusted, facts))
            }
            Verb::CloseTab => {
                let (id, tab) = self.resolve_arg_tab(session_id, args)?;
                let facts = self.tab_facts(&id, &tab);
                self.close_agent_tab(&id);
                Ok(mutated(Fidelity::Trusted, facts))
            }
            Verb::Navigate => self.navigate(session_id, args, deadline).await,
            Verb::Back | Verb::Forward | Verb::Reload => {
                let (id, tab) = self.resolve_arg_tab(session_id, args)?;
                let step = match verb {
                    Verb::Back => HistoryStep::Back,
                    Verb::Forward => HistoryStep::Forward,
                    _ => HistoryStep::Reload,
                };
                self.surface.history(&id, step).map_err(BrowserError::internal)?;
                tab.bump_epoch();
                self.settle(&id, deadline).await;
                Ok(Success {
                    page: self.page_meta(&id),
                    ..mutated(Fidelity::Trusted, self.tab_facts(&id, &tab))
                })
            }
            Verb::Snapshot => {
                let (id, tab) = self.resolve_arg_tab(session_id, args)?;
                let facts = self.tab_facts(&id, &tab);
                let out = self.eval(&id, &assets::snapshot_script()).await?;
                let (page, refs) = shape_snapshot(&out)?;
                Ok(Success {
                    tab: Some(facts),
                    page: Some(page),
                    data: Some(json!({ "refs": refs })),
                    ..Success::default()
                })
            }
            Verb::Screenshot => {
                let (id, tab) = self.resolve_arg_tab(session_id, args)?;
                let path = self.screenshot(session_id, &id).await?;
                Ok(Success {
                    tab: Some(self.tab_facts(&id, &tab)),
                    data: Some(json!({ "path": path })),
                    ..Success::default()
                })
            }
            Verb::Click => {
                let (id, tab) = self.resolve_arg_tab(session_id, args)?;
                let el = required_ref(args)?;
                let fidelity = self.click_ref(&id, el, tab.epoch(), deadline).await?;
                Ok(mutated(fidelity, self.tab_facts(&id, &tab)))
            }
            Verb::Fill => {
                let (id, tab) = self.resolve_arg_tab(session_id, args)?;
                let ref_id = required_ref(args)?;
                let value = args
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(|| BrowserError::invalid("missing 'value'").with_arg_keys(&["value"]))?;
                let out = self.eval(&id, &assets::fill_ref(ref_id, value, tab.epoch())).await?;
                eval_gate(&out, Some(ref_id))?;
                Ok(mutated(Fidelity::Synthetic, self.tab_facts(&id, &tab)))
            }
            Verb::Type | Verb::Press | Verb::Scroll | Verb::Hover => {
                let (id, tab) = self.resolve_arg_tab(session_id, args)?;
                // `ref` is optional for type/press/scroll (they fall back to
                // the focused element or the page).
                let ref_id = ref_arg(args)?;
                let out = self
                    .eval(&id, &assets::interaction(&call.action, args, tab.epoch()))
                    .await?;
                eval_gate(&out, ref_id)?;
                Ok(mutated(Fidelity::Synthetic, self.tab_facts(&id, &tab)))
            }
            Verb::Wait => self.wait(session_id, args, deadline).await,
        }
    }

    fn list_tabs(&self, session_id: &str) -> Success {
        let owned: Vec<(String, Arc<AgentTab>)> = {
            let tabs = self.agent_tabs.lock();
            let mut owned: Vec<_> = tabs
                .iter()
                .filter(|(_, t)| t.session_id == session_id)
                .map(|(id, t)| (id.clone(), t.clone()))
                .collect();
            owned.sort_by(|a, b| a.0.cmp(&b.0));
            owned
        };
        let list: Vec<Value> = owned
            .iter()
            .filter_map(|(id, tab)| {
                let page = self.page_meta(id)?;
                Some(json!({ "tab": self.tab_facts(id, tab), "page": page }))
            })
            .collect();
        Success {
            data: Some(json!({ "tabs": list })),
            ..Success::default()
        }
    }

    async fn navigate(&self, session_id: &str, args: &Value, deadline: Deadline) -> Result<Success, BrowserError> {
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
            self.surface
                .navigate(&tab_id, url.clone())
                .map_err(BrowserError::internal)?;
        }
        tab.bump_epoch();
        self.settle(&tab_id, deadline).await;
        // Report the tab's real URL — never fabricate the requested one.
        let page = self
            .page_meta(&tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(&tab_id).with_arg_keys(&["tabId"]))?;
        Ok(Success {
            page: Some(page),
            ..mutated(Fidelity::Trusted, self.tab_facts(&tab_id, &tab))
        })
    }

    async fn wait(&self, session_id: &str, args: &Value, deadline: Deadline) -> Result<Success, BrowserError> {
        let (tab_id, tab) = self.resolve_arg_tab(session_id, args)?;
        let done = |host: &Self| Success {
            tab: Some(host.tab_facts(&tab_id, &tab)),
            ..Success::default()
        };
        if let Some(ms) = args.get("ms").and_then(Value::as_u64) {
            let ms = std::time::Duration::from_millis(ms).min(budgets::WAIT_MAX);
            tokio::time::sleep(deadline.clamp(ms)).await;
            return Ok(done(self));
        }
        let text = args.get("text").and_then(Value::as_str).ok_or_else(|| {
            BrowserError::invalid("wait needs 'ms' or 'text'").with_arg_keys(&["ms", "text"])
        })?;
        let wait = Deadline::after(deadline.clamp(budgets::WAIT_MAX));
        loop {
            let found = self
                .eval(&tab_id, &assets::contains_text(text))
                .await
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if found {
                return Ok(done(self));
            }
            if wait.remaining().is_zero() {
                return Err(BrowserError::new(ERR_TIMEOUT, "wait: text did not appear within 30s")
                    .with_reason("text_not_found")
                    .with_arg_keys(&["text"]));
            }
            tokio::time::sleep(wait.clamp(budgets::WAIT_POLL)).await;
        }
    }

    /// Click an `@eN` ref and report the input actually used: a trusted
    /// click at the resolved point where the engine has one (falling back
    /// to a synthetic DOM click if it fails), else the synthetic click.
    async fn click_ref(
        &self,
        tab_id: &str,
        ref_id: &str,
        epoch: u64,
        deadline: Deadline,
    ) -> Result<Fidelity, BrowserError> {
        let mode = match self.engine.facts().capabilities.get(CapabilityId::PointerClick) {
            Support::Trusted => InputMode::Trusted,
            Support::Synthetic => InputMode::Synthetic,
            Support::Unavailable(reason) => return Err(BrowserError::capability(reason)),
        };
        if mode == InputMode::Trusted {
            let out = self.eval(tab_id, &assets::resolve_ref(ref_id, epoch)).await?;
            eval_gate(&out, Some(ref_id))?;
            let rect = out.get("rect").ok_or_else(|| {
                BrowserError::new(super::ERR_STALE_REF, format!("ref {ref_id} no longer resolves; take a new snapshot"))
                    .with_reason("ref_not_resolved")
                    .with_arg_keys(&["ref"])
            })?;
            let at = Point {
                x: rect.get("cx").and_then(Value::as_f64).unwrap_or(0.0),
                y: rect.get("cy").and_then(Value::as_f64).unwrap_or(0.0),
            };
            match self.engine.pointer_click(tab_id, at, InputMode::Trusted).await {
                Ok(()) => return Ok(Fidelity::Trusted),
                Err(e) if !deadline.admits(budgets::EVAL_STEP) => return Err(engine_error(e, tab_id)),
                Err(e) => log::warn!(
                    "[browser-agent] trusted click failed on tab {tab_id} ({:?}), synthetic fallback",
                    e.kind
                ),
            }
        }
        let out = self.eval(tab_id, &assets::dom_click(ref_id, epoch)).await?;
        eval_gate(&out, Some(ref_id))?;
        Ok(Fidelity::Synthetic)
    }

    /// Capture the tab and write the PNG under the session's artifact dir.
    async fn screenshot(&self, session_id: &str, tab_id: &str) -> Result<String, BrowserError> {
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
            .surface
            .artifacts_root()
            .map_err(BrowserError::internal)?
            .join(if safe_sid.is_empty() { "_".to_string() } else { safe_sid });
        std::fs::create_dir_all(&dir).map_err(|e| BrowserError::internal(format!("artifact dir: {e}")))?;
        let path = dir.join(format!("shot-{}.png", Uuid::new_v4()));
        std::fs::write(&path, png).map_err(|e| BrowserError::internal(format!("write artifact: {e}")))?;
        Ok(path.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
