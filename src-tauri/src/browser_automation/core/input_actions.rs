//! Input verbs. Each checks its target's actionability, then uses the
//! engine's trusted input where the capability table declares it and the
//! page-script fallback only where it declares synthetic, and replies with
//! the fidelity of the input that ran.
//!
//! Trusted text goes into an element focused by a trusted click. Where
//! focusing an element moves native focus to the tab (WKWebView makes it
//! first responder and blurs the chat), nothing focuses one: a click that
//! would is synthetic, and so is text entry, which writes the value
//! without `element.focus()`.

use std::future::Future;

use serde_json::{json, Value};

use super::actionability::{wait_until_actionable, Need, Target};
use super::actions::{ref_arg, ResolvedRef};
use super::reply::{engine_error, eval_gate, Fidelity};
use super::{BrowserError, DesktopBrowserHost, ERR_TIMEOUT};
use crate::browser_automation::budgets::{self, Deadline};
use crate::browser_automation::engine::{
    assets, input_assets, CapabilityId, ControlKind, EngineFuture, InputMode, Key, KeyInput,
    KeyKind, Point, Support,
};

fn mode_of(support: Support) -> Result<InputMode, BrowserError> {
    match support {
        Support::Trusted => Ok(InputMode::Trusted),
        Support::Synthetic => Ok(InputMode::Synthetic),
        Support::Unavailable(reason) => Err(BrowserError::capability(reason)),
    }
}

fn fidelity(mode: InputMode) -> Fidelity {
    match mode {
        InputMode::Trusted => Fidelity::Trusted,
        InputMode::Synthetic => Fidelity::Synthetic,
    }
}

/// Run `step`, cut at `bound`.
async fn within<T>(
    bound: Deadline,
    step: impl Future<Output = Result<T, BrowserError>>,
) -> Result<T, BrowserError> {
    tokio::time::timeout(bound.remaining(), step)
        .await
        .unwrap_or_else(|_| {
            Err(BrowserError::new(ERR_TIMEOUT, "the input did not finish in time")
                .with_reason("input_deadline"))
        })
}

fn not_focused() -> BrowserError {
    BrowserError::new(ERR_TIMEOUT, "clicking the element did not focus it")
        .with_reason("not_focused")
        .with_arg_keys(&["ref"])
}

impl DesktopBrowserHost {
    fn input_mode(&self, id: CapabilityId) -> Result<InputMode, BrowserError> {
        mode_of(self.engine.facts().capabilities.get(id))
    }

    /// The key tier for `kind`, and whether trusted keys can reach an
    /// element: that needs a trusted click to focus it first, one that
    /// leaves native focus with the chat.
    fn key_modes(&self, kind: KeyKind) -> Result<(InputMode, bool), BrowserError> {
        let mode = mode_of(self.engine.key_support(kind))?;
        let click = self.engine.facts().capabilities.get(CapabilityId::PointerClick);
        let by_click = mode == InputMode::Trusted
            && click == Support::Trusted
            && !self.engine.element_focus_moves_tab_focus();
        Ok((mode, by_click))
    }

    async fn input(&self, tab: &str, step: EngineFuture<'_, ()>) -> Result<(), BrowserError> {
        step.await.map_err(|e| {
            // Tab id + kind only (CWE-532).
            log::warn!(
                "[browser-agent] trusted input failed on tab {} ({:?})",
                super::events::label_prefix(tab),
                e.kind
            );
            engine_error(e, tab)
        })
    }

    async fn helper(&self, tab: &str, token: Option<&str>, script: &str) -> Result<Value, BrowserError> {
        let out = self.eval(tab, script).await?;
        eval_gate(&out, token)?;
        Ok(out)
    }

    async fn actionable(
        &self,
        tab: &str,
        token: &str,
        resolved: &ResolvedRef,
        need: Need,
        deadline: Deadline,
    ) -> Result<Target, BrowserError> {
        wait_until_actionable(&*self.engine, tab, token, &resolved.node, need, deadline).await
    }

    /// Click `target` to focus it, then select its content (`replace`) or
    /// put the caret at its end.
    async fn focus_text(
        &self,
        tab: &str,
        token: &str,
        resolved: &ResolvedRef,
        target: Target,
        replace: bool,
        bound: Deadline,
    ) -> Result<(), BrowserError> {
        within(bound, self.input(tab, self.engine.pointer_click(tab, target.point, InputMode::Trusted))).await?;
        let script = input_assets::prepare_text(&resolved.node.path, replace);
        let out = within(bound, self.eval(tab, &script)).await?;
        if out.get("error").and_then(Value::as_str) == Some("not_focused") {
            return Err(not_focused());
        }
        eval_gate(&out, Some(token))
    }

    pub(super) async fn click(
        &self,
        tab: &str,
        token: &str,
        resolved: &ResolvedRef,
        deadline: Deadline,
    ) -> Result<Fidelity, BrowserError> {
        let mut mode = self.input_mode(CapabilityId::PointerClick)?;
        let target = self.actionable(tab, token, resolved, Need::Activate, deadline).await?;
        if target.focus_on_click && self.engine.element_focus_moves_tab_focus() {
            mode = InputMode::Synthetic;
        }
        match mode {
            InputMode::Trusted => {
                self.input(tab, self.engine.pointer_click(tab, target.point, mode)).await?;
            }
            InputMode::Synthetic => {
                let script = assets::dom_click(&resolved.node.path, resolved.epoch);
                self.helper(tab, Some(token), &script).await?;
            }
        }
        Ok(fidelity(mode))
    }

    pub(super) async fn hover(
        &self,
        tab: &str,
        token: &str,
        resolved: &ResolvedRef,
        deadline: Deadline,
    ) -> Result<Fidelity, BrowserError> {
        let mode = self.input_mode(CapabilityId::PointerMove)?;
        let target = self.actionable(tab, token, resolved, Need::Pointer, deadline).await?;
        match mode {
            InputMode::Trusted => {
                self.input(tab, self.engine.pointer_move(tab, target.point, mode)).await?;
            }
            InputMode::Synthetic => {
                let args = json!({ "ref": resolved.node.path });
                let script = assets::interaction("hover", &args, resolved.epoch);
                self.helper(tab, Some(token), &script).await?;
            }
        }
        Ok(fidelity(mode))
    }

    /// Scroll the page by `dy` with the wheel, or a ref into view (a
    /// programmatic scroll, so synthetic on every engine).
    pub(super) async fn scroll(&self, tab: &str, args: &Value, epoch: u64) -> Result<Fidelity, BrowserError> {
        if let Some(token) = ref_arg(args)? {
            let resolved = self.resolve_ref(tab, token)?;
            self.revalidate(tab, token, &resolved).await?;
            return Ok(Fidelity::Synthetic);
        }
        let mode = self.input_mode(CapabilityId::Wheel)?;
        match mode {
            InputMode::Trusted => {
                let dy = args.get("dy").and_then(Value::as_f64).unwrap_or(600.0);
                let viewport = self.helper(tab, None, input_assets::VIEWPORT).await?;
                let half = |key: &str| viewport.get(key).and_then(Value::as_f64).unwrap_or(0.0) / 2.0;
                let at = Point {
                    x: half("w"),
                    y: half("h"),
                };
                self.input(tab, self.engine.wheel(tab, at, (0.0, dy), mode)).await?;
            }
            InputMode::Synthetic => {
                self.helper(tab, None, &assets::interaction("scroll", args, epoch)).await?;
            }
        }
        Ok(fidelity(mode))
    }

    /// Script text entry into the ref: replace its value, or `append`.
    /// It focuses the element first only where that leaves native focus
    /// with the chat.
    fn synthetic_text(&self, resolved: &ResolvedRef, text: &str, append: bool) -> String {
        let path = &resolved.node.path;
        if self.engine.element_focus_moves_tab_focus() {
            return input_assets::synthetic_set_value(path, text, append);
        }
        if append {
            let args = json!({ "ref": path, "text": text });
            assets::interaction("type", &args, resolved.epoch)
        } else {
            assets::fill_ref(path, text, resolved.epoch)
        }
    }

    pub(super) async fn fill(
        &self,
        tab: &str,
        token: &str,
        resolved: &ResolvedRef,
        value: &str,
        deadline: Deadline,
    ) -> Result<Fidelity, BrowserError> {
        let (_, by_click) = self.key_modes(KeyKind::Text)?;
        let bound = Deadline::after(deadline.clamp(budgets::ACTION_TEXT));
        let target = self.actionable(tab, token, resolved, Need::Fill, bound).await?;
        if target.kind == ControlKind::Select || value.is_empty() {
            let script = input_assets::synthetic_set_value(&resolved.node.path, value, false);
            within(bound, self.helper(tab, Some(token), &script)).await?;
            return Ok(Fidelity::Synthetic);
        }
        if !by_click {
            let script = self.synthetic_text(resolved, value, false);
            within(bound, self.helper(tab, Some(token), &script)).await?;
            return Ok(Fidelity::Synthetic);
        }
        self.focus_text(tab, token, resolved, target, true, bound).await?;
        let text = self.engine.key(tab, KeyInput::Text(value), InputMode::Trusted);
        within(bound, self.input(tab, text)).await?;
        Ok(Fidelity::Trusted)
    }

    pub(super) async fn type_text(&self, tab: &str, args: &Value, deadline: Deadline) -> Result<Fidelity, BrowserError> {
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| BrowserError::invalid("missing 'text'").with_arg_keys(&["text"]))?;
        let (mode, by_click) = self.key_modes(KeyKind::Text)?;
        let bound = Deadline::after(deadline.clamp(budgets::ACTION_TEXT));
        match ref_arg(args)? {
            Some(token) => {
                let resolved = self.resolve_ref(tab, token)?;
                let target = self.actionable(tab, token, &resolved, Need::Type, bound).await?;
                if !by_click {
                    let script = self.synthetic_text(&resolved, text, true);
                    within(bound, self.helper(tab, Some(token), &script)).await?;
                    return Ok(Fidelity::Synthetic);
                }
                self.focus_text(tab, token, &resolved, target, false, bound).await?;
            }
            None if mode == InputMode::Synthetic => {
                let script = assets::interaction("type", args, self.tab_facts(tab)?.epoch);
                within(bound, self.helper(tab, None, &script)).await?;
                return Ok(Fidelity::Synthetic);
            }
            None => self.require_focused_editable(tab, bound).await?,
        }
        let input = self.engine.key(tab, KeyInput::Text(text), InputMode::Trusted);
        within(bound, self.input(tab, input)).await?;
        Ok(Fidelity::Trusted)
    }

    async fn require_focused_editable(&self, tab: &str, bound: Deadline) -> Result<(), BrowserError> {
        let out = within(bound, self.helper(tab, None, input_assets::FOCUSED_EDITABLE)).await?;
        if out.get("editable").and_then(Value::as_bool) == Some(true) {
            Ok(())
        } else {
            Err(BrowserError::invalid("no element has focus; pass a ref").with_reason("no_focused_element"))
        }
    }

    pub(super) async fn press(&self, tab: &str, args: &Value, deadline: Deadline) -> Result<Fidelity, BrowserError> {
        let name = args.get("key").and_then(Value::as_str).unwrap_or("Enter");
        let (mode, by_click) = self.key_modes(KeyKind::Press)?;
        let key = match (mode, Key::parse(name)) {
            (InputMode::Trusted, None) => {
                return Err(BrowserError::invalid(
                    "unknown key; pass one character or a key name such as Enter, Tab, Escape, Backspace, Delete, ArrowDown, Home, End, PageUp or Space",
                )
                .with_arg_keys(&["key"]));
            }
            (_, key) => key,
        };
        let bound = Deadline::after(deadline.clamp(budgets::ACTION_TEXT));
        let token = ref_arg(args)?;
        let mut trusted = mode == InputMode::Trusted;
        let mut path = None;
        if let Some(token) = token {
            let resolved = self.resolve_ref(tab, token)?;
            let target = self.actionable(tab, token, &resolved, Need::Activate, bound).await?;
            // Only a text control can be focused by a click without side
            // effects; anything else gets the key on the element itself.
            trusted = by_click && target.kind == ControlKind::Text;
            if trusted {
                self.focus_text(tab, token, &resolved, target, false, bound).await?;
            }
            path = Some(resolved.node.path);
        }
        match key {
            Some(key) if trusted => {
                let input = self.engine.key(tab, KeyInput::Press(key), InputMode::Trusted);
                within(bound, self.input(tab, input)).await?;
                Ok(Fidelity::Trusted)
            }
            _ => {
                let script = input_assets::synthetic_press(path.as_deref(), name);
                within(bound, self.helper(tab, token, &script)).await?;
                Ok(Fidelity::Synthetic)
            }
        }
    }
}
