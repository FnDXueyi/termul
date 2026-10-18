use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::browser_automation::core::reply::eval_gate;
use crate::browser_automation::{ERR_STALE_REF, ERR_TAB_NOT_FOUND};

fn agent_tab(session_id: &str) -> Arc<AgentTab> {
    Arc::new(AgentTab::new(session_id))
}

#[test]
fn n_call_session_reuses_exactly_one_agent_tab() {
    // N-call reuse (navigate → snapshot → navigate → …): the first
    // navigate auto-opens, every later navigate retargets the SAME tab —
    // exactly 1 agent tab exists for the session.
    let mut tabs = HashMap::new();
    match navigate_resolution(&tabs, "sess-1", None) {
        NavigateResolution::AutoOpen => {}
        _ => panic!("first navigate (session owns no tab) must auto-open"),
    }
    // The open landed exactly one tab (the model: single reused tab).
    tabs.insert("agent-a".to_string(), agent_tab("sess-1"));
    for _ in 0..4 {
        match navigate_resolution(&tabs, "sess-1", None) {
            NavigateResolution::Reuse(id, _) => assert_eq!(id, "agent-a"),
            _ => panic!("later navigates must reuse the session's tab"),
        }
    }
    assert_eq!(tabs.len(), 1, "exactly one agent tab for the session");
}

#[test]
fn navigate_resolution_reuses_session_tab_even_with_foreign_tabs_present() {
    // Other sessions' tabs never leak in as the default target.
    let mut tabs = HashMap::new();
    tabs.insert("agent-b".to_string(), agent_tab("sess-other"));
    match navigate_resolution(&tabs, "sess-1", None) {
        NavigateResolution::AutoOpen => {}
        _ => panic!("session with no owned tab must auto-open, not reuse a foreign tab"),
    }
    tabs.insert("agent-a".to_string(), agent_tab("sess-1"));
    match navigate_resolution(&tabs, "sess-1", None) {
        NavigateResolution::Reuse(id, tab) => {
            assert_eq!(id, "agent-a");
            assert_eq!(tab.session_id, "sess-1");
        }
        _ => panic!("no tabId + owned tab must resolve to Reuse"),
    }
}

#[test]
fn explicit_foreign_or_unknown_tabid_is_tab_not_found_never_auto_open() {
    // An explicit `tabId` that is unknown or owned by another session is
    // a caller error: `tab_not_found`, no fallback to the session's own
    // tab, and never an auto-open (the tab-proliferation bug).
    let mut tabs = HashMap::new();
    tabs.insert("agent-own".to_string(), agent_tab("sess-1"));
    tabs.insert("agent-foreign".to_string(), agent_tab("sess-2"));
    for bad in ["agent-foreign", "agent-unknown"] {
        match navigate_resolution(&tabs, "sess-1", Some(bad)) {
            NavigateResolution::TabNotFound(e) => {
                assert_eq!(e.code, ERR_TAB_NOT_FOUND);
                assert!(
                    e.message.contains(bad),
                    "error must name the tab: {}",
                    e.message
                );
            }
            _ => panic!("explicit non-owned tabId must be TabNotFound, got {bad}"),
        }
    }
    // The session's own tab is still there — resolution opened nothing.
    assert_eq!(tabs.len(), 2);
}

#[test]
fn explicit_owned_tabid_reuses_that_tab() {
    let mut tabs = HashMap::new();
    tabs.insert("agent-own".to_string(), agent_tab("sess-1"));
    match navigate_resolution(&tabs, "sess-1", Some("agent-own")) {
        NavigateResolution::Reuse(id, tab) => {
            assert_eq!(id, "agent-own");
            assert_eq!(tab.session_id, "sess-1");
        }
        _ => panic!("explicit session-owned tabId must resolve to Reuse"),
    }
}

#[test]
fn user_closed_tab_means_next_navigate_auto_opens_fresh() {
    // Closing the agent tab revokes control (notify_tab_closed removes
    // the mapping); the session owns nothing again, so the next navigate
    // (no tabId) mints a fresh tab.
    let mut tabs = HashMap::new();
    tabs.insert("agent-a".to_string(), agent_tab("sess-1"));
    tabs.remove("agent-a");
    match navigate_resolution(&tabs, "sess-1", None) {
        NavigateResolution::AutoOpen => {}
        _ => panic!("after the user closed the tab the next navigate must auto-open"),
    }
}

#[test]
fn manual_redirect_retargets_same_tab_and_kills_pre_navigation_refs() {
    // The user manually navigated the agent tab elsewhere: the next agent
    // navigate retargets the SAME tab silently (Reuse — no error, no new
    // tab), the tab epoch advances, and a ref minted before the redirect
    // reports `stale_ref` through the eval gate.
    let tab = agent_tab("sess-1");
    let mut tabs = HashMap::new();
    tabs.insert("agent-a".to_string(), tab.clone());
    let pre_nav_epoch = tab.epoch();
    // browser_tab_report_url → notify_tab_navigated →
    // bump_epoch_if_agent_tab (the notify path's core, driven directly
    // here — the host wrapper needs an AppHandle).
    bump_epoch_if_agent_tab(&tabs, "agent-a");
    assert_eq!(
        tab.epoch(),
        pre_nav_epoch + 1,
        "manual navigation must advance the tab epoch"
    );
    // A tab id that is not an agent tab is a no-op (the notify hook also
    // fires for plain user tabs).
    bump_epoch_if_agent_tab(&tabs, "user-tab");
    assert_eq!(tab.epoch(), pre_nav_epoch + 1);
    // Silent retarget: still the same tab, no error surfaced.
    match navigate_resolution(&tabs, "sess-1", None) {
        NavigateResolution::Reuse(id, t) => {
            assert_eq!(id, "agent-a");
            assert!(Arc::ptr_eq(&t, &tab), "the same agent tab is retargeted");
        }
        _ => panic!("manual redirect must not detach or replace the session tab"),
    }
    // A pre-navigation ref dies: the navigated document no longer has the
    // registry entry, the page reports `stale_ref:`, and the eval gate
    // maps it to the typed wire code.
    let err = eval_gate(&json!({"error": "stale_ref: @e1 not in this document"}), Some("@e1"))
        .expect_err("a pre-navigation ref must be rejected");
    assert_eq!(err.code, ERR_STALE_REF);
}

#[test]
fn default_tab_pick_is_deterministic_across_multiple_owned_tabs() {
    // `new_tab` still mints several tabs per session by design; the
    // default (no tabId) pick must be stable, not HashMap-iteration
    // order � and the auto-open race re-check reuses the same pick.
    let mut tabs = HashMap::new();
    tabs.insert("agent-z".to_string(), agent_tab("sess-1"));
    tabs.insert("agent-a".to_string(), agent_tab("sess-1"));
    tabs.insert("agent-m".to_string(), agent_tab("sess-1"));
    for _ in 0..8 {
        match navigate_resolution(&tabs, "sess-1", None) {
            NavigateResolution::Reuse(id, _) => {
                assert_eq!(id, "agent-a", "the lexicographically smallest owned id")
            }
            _ => panic!("owned tabs must resolve to Reuse"),
        }
    }
    // The same core drives resolve_tab for every other action.
    let (id, tab) = owned_tab_or_default(&tabs, "sess-1", None)
        .expect("core resolves without an error")
        .expect("session owns tabs");
    assert_eq!(id, "agent-a");
    assert_eq!(tab.session_id, "sess-1");
}

#[test]
fn owned_tab_or_default_core_contract() {
    let mut tabs = HashMap::new();
    // No owned tab, no tabId ? Ok(None): the caller decides (navigate
    // auto-opens; other actions error with the no-agent-tab message).
    assert!(owned_tab_or_default(&tabs, "sess-1", None)
        .expect("missing default is not an error")
        .is_none());
    // Foreign tab, no tabId ? still Ok(None) for this session.
    tabs.insert("agent-f".to_string(), agent_tab("sess-2"));
    assert!(owned_tab_or_default(&tabs, "sess-1", None)
        .expect("missing default is not an error")
        .is_none());
    // Explicit foreign tabId ? typed tab_not_found (never a default
    // fallback).
    let err = owned_tab_or_default(&tabs, "sess-1", Some("agent-f"))
        .expect_err("explicit foreign tabId must error");
    assert_eq!(err.code, ERR_TAB_NOT_FOUND);
    // Site attribution pinned from origin: the failing key is `tabId`.
    assert_eq!(err.arg_keys, vec!["tabId"]);
    assert!(
        err.message.contains("agent-f"),
        "error must name the tab: {}",
        err.message
    );
}
