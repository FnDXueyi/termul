//! The engine-to-core half of the port: one ordered event stream per tab,
//! and the navigation policy the core hands each tab at construction.
//!
//! Adapters publish [`TabEvent`]s into a [`TabEvents`] channel and never
//! call the core directly. Navigation hooks are synchronous on every
//! engine termul uses through tauri, so the per-hop verdict comes from the
//! [`NavPolicy`] the core provisions when the tab is built. It is the only
//! decision a tab's navigation gets, and it is reported with the
//! `NavStarting` event it applied to.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tauri::Url;
use tokio::sync::mpsc::UnboundedSender;

/// How a main-frame commit changed the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitKind {
    /// A new document replaced the old one.
    NewDocument,
    /// The URL changed inside the same document (`pushState`, fragment,
    /// same-document history step).
    #[cfg_attr(
        all(target_os = "macos", not(test)),
        allow(dead_code, reason = "WKWebView reports no same-document changes yet")
    )]
    SameDocument,
}

/// A per-hop navigation verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavVerdict {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabEvent {
    /// A main-frame navigation hop is about to start (first hop or
    /// redirect). `verdict` is what the hook applied.
    NavStarting {
        nav_id: u64,
        url: String,
        verdict: NavVerdict,
    },
    /// The main frame committed. A same-document commit carries no
    /// `nav_id`: engines raise no start hop for it.
    NavCommitted {
        nav_id: Option<u64>,
        kind: CommitKind,
        url: String,
    },
    /// The document finished loading.
    LoadFinished,
    TitleChanged {
        title: String,
    },
    /// The engine destroyed the webview.
    Closed,
}

/// The core's per-hop navigation decision, consulted synchronously on the
/// UI thread. Implementations must not block.
pub trait NavPolicy: Send + Sync {
    fn decide(&self, tab: &str, url: &Url) -> NavVerdict;
}

/// What the core gives an adapter for one tab: its label, the event
/// channel, and the policy its navigation hook applies.
#[derive(Clone)]
pub struct TabEvents {
    label: Arc<str>,
    tx: UnboundedSender<(String, TabEvent)>,
    policy: Arc<dyn NavPolicy>,
    /// Hook-assigned navigation ids. Engines reached through tauri hooks
    /// expose none, so ids are minted in hook order: a commit belongs to
    /// the latest hop that started before it.
    last_nav: Arc<AtomicU64>,
}

impl TabEvents {
    pub fn new(
        label: &str,
        tx: UnboundedSender<(String, TabEvent)>,
        policy: Arc<dyn NavPolicy>,
    ) -> Self {
        Self {
            label: Arc::from(label),
            tx,
            policy,
            last_nav: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Run the policy for a starting hop and publish it. Returns whether
    /// the hop may proceed.
    pub fn nav_starting(&self, url: &Url) -> bool {
        let verdict = self.policy.decide(&self.label, url);
        let nav_id = self.last_nav.fetch_add(1, Ordering::AcqRel) + 1;
        self.publish(TabEvent::NavStarting {
            nav_id,
            url: url.to_string(),
            verdict,
        });
        verdict == NavVerdict::Allow
    }

    /// A new-document commit, attributed to the latest started hop.
    pub fn committed(&self, url: &str) {
        let latest = self.last_nav.load(Ordering::Acquire);
        self.publish(TabEvent::NavCommitted {
            nav_id: (latest > 0).then_some(latest),
            kind: CommitKind::NewDocument,
            url: url.to_string(),
        });
    }

    #[cfg_attr(
        all(target_os = "macos", not(test)),
        allow(dead_code, reason = "WKWebView reports no same-document changes yet")
    )]
    pub fn same_document(&self, url: &str) {
        self.publish(TabEvent::NavCommitted {
            nav_id: None,
            kind: CommitKind::SameDocument,
            url: url.to_string(),
        });
    }

    pub fn publish(&self, event: TabEvent) {
        // The receiver is gone only when the host is shutting down.
        let _ = self.tx.send((self.label.to_string(), event));
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
