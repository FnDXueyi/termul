//! Engine port: the `BrowserEngine` trait the core drives, plus the
//! capability, trait and error types every adapter reports in.
//!
//! Dependency direction: the core depends on this module; adapters
//! (`webview2`, `jsbridge`) implement it and never import core types.
//! Platform crates and per-OS code live only inside the adapter
//! directories; `cdp` holds engine-neutral CDP logic over `CdpTransport`.

pub mod assets;
#[cfg(any(test, target_os = "windows"))]
pub mod cdp;
mod jsbridge;
#[cfg(target_os = "windows")]
mod webview2;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::browser_tab_manager::BrowserTabManager;

/// Closed set of engine capabilities. Verbs name the capabilities they
/// need; adapters declare a tier for each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityId {
    IsolatedEval,
    ScriptMessage,
    PersistentProfile,
    NavigationIntercept,
    NewWindowIntercept,
    DialogIntercept,
    FileChooser,
    DownloadIntercept,
    PermissionIntercept,
    ExternalProtocolIntercept,
    Screenshot,
    PointerClick,
    TrustedKey,
    PointerMove,
    Wheel,
    Drag,
    ConsoleObserve,
    NetworkObserve,
}

impl CapabilityId {
    pub const ALL: [CapabilityId; 18] = [
        CapabilityId::IsolatedEval,
        CapabilityId::ScriptMessage,
        CapabilityId::PersistentProfile,
        CapabilityId::NavigationIntercept,
        CapabilityId::NewWindowIntercept,
        CapabilityId::DialogIntercept,
        CapabilityId::FileChooser,
        CapabilityId::DownloadIntercept,
        CapabilityId::PermissionIntercept,
        CapabilityId::ExternalProtocolIntercept,
        CapabilityId::Screenshot,
        CapabilityId::PointerClick,
        CapabilityId::TrustedKey,
        CapabilityId::PointerMove,
        CapabilityId::Wheel,
        CapabilityId::Drag,
        CapabilityId::ConsoleObserve,
        CapabilityId::NetworkObserve,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CapabilityId::IsolatedEval => "isolated_eval",
            CapabilityId::ScriptMessage => "script_message",
            CapabilityId::PersistentProfile => "persistent_profile",
            CapabilityId::NavigationIntercept => "navigation_intercept",
            CapabilityId::NewWindowIntercept => "new_window_intercept",
            CapabilityId::DialogIntercept => "dialog_intercept",
            CapabilityId::FileChooser => "file_chooser",
            CapabilityId::DownloadIntercept => "download_intercept",
            CapabilityId::PermissionIntercept => "permission_intercept",
            CapabilityId::ExternalProtocolIntercept => "external_protocol_intercept",
            CapabilityId::Screenshot => "screenshot",
            CapabilityId::PointerClick => "pointer_click",
            CapabilityId::TrustedKey => "trusted_key",
            CapabilityId::PointerMove => "pointer_move",
            CapabilityId::Wheel => "wheel",
            CapabilityId::Drag => "drag",
            CapabilityId::ConsoleObserve => "console_observe",
            CapabilityId::NetworkObserve => "network_observe",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// How an engine supports one capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "windows"),
    allow(dead_code, reason = "only the WebView2 adapter declares Trusted tiers")
)]
pub enum Support {
    /// Real engine input or a native engine API.
    Trusted,
    /// Page-script events; replies report the input as synthetic.
    Synthetic,
    /// Not available; calls that need it return `capability_unavailable`.
    Unavailable(&'static str),
}

/// Per-engine capability table. Built once from app and OS facts and
/// frozen before the host registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityTable {
    tiers: [Support; CapabilityId::ALL.len()],
}

impl CapabilityTable {
    /// Every capability unavailable with the same reason; adapters raise
    /// the tiers they implement with [`CapabilityTable::with`].
    pub fn unavailable(reason: &'static str) -> Self {
        Self::uniform(Support::Unavailable(reason))
    }

    pub fn uniform(support: Support) -> Self {
        Self {
            tiers: [support; CapabilityId::ALL.len()],
        }
    }

    #[must_use]
    pub fn with(mut self, id: CapabilityId, support: Support) -> Self {
        self.tiers[id.index()] = support;
        self
    }

    pub fn get(&self, id: CapabilityId) -> Support {
        self.tiers[id.index()]
    }
}

/// Engine facts that are not input tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineTraits {
    /// Helper scripts run in a world page script cannot reach.
    pub isolated_world: bool,
    /// Background tabs keep rendering.
    pub background_render: bool,
    pub network_observe: bool,
}

/// What the core and the tool schema read about the registered engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFacts {
    pub capabilities: CapabilityTable,
    pub traits: EngineTraits,
}

#[cfg(test)]
impl EngineFacts {
    /// An engine with every capability trusted, for stub hosts.
    pub fn all_trusted() -> Self {
        Self {
            capabilities: CapabilityTable::uniform(Support::Trusted),
            traits: EngineTraits {
                isolated_world: true,
                background_render: true,
                network_observe: true,
            },
        }
    }
}

/// The input mode the core picks from the capability tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Trusted,
    Synthetic,
}

/// A point in main-frame CSS px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Why an engine step failed. Carries no engine message: messages can
/// quote page or protocol content, so only the core writes reply text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "windows"),
    allow(dead_code, reason = "Protocol and Malformed come from the CDP adapter")
)]
pub enum EngineErrorKind {
    /// The tab label resolves to no webview.
    TabNotFound,
    /// The engine does not support this capability or input mode.
    Unsupported,
    /// The engine could not be reached; the command never ran.
    Dispatch,
    /// No answer within the step budget.
    Timeout,
    /// The command was issued but its reply was lost.
    NoReply,
    /// The page helper script threw.
    PageException,
    /// The engine answered with a protocol error.
    Protocol,
    /// The engine answered with an unusable reply.
    Malformed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineError {
    pub kind: EngineErrorKind,
    /// True only when the command provably never reached the page, so a
    /// retry on another channel cannot repeat its side effects.
    pub retryable: bool,
}

impl EngineError {
    pub fn new(kind: EngineErrorKind, retryable: bool) -> Self {
        Self { kind, retryable }
    }
}

pub type EngineFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, EngineError>> + Send + 'a>>;

/// Capability-level engine commands. Tab ids are pane labels.
pub trait BrowserEngine: Send + Sync {
    /// Frozen at construction, before the host registers.
    fn facts(&self) -> &EngineFacts;
    /// Run a helper script from [`assets`] and return its JSON completion
    /// value (`isolated_eval`; [`EngineTraits::isolated_world`] says which
    /// world it runs in).
    fn evaluate<'a>(&'a self, tab: &'a str, script: &'a str) -> EngineFuture<'a, Value>;
    /// Click at a main-frame point (`pointer_click`). Adapters honour the
    /// mode or return `Unsupported`; they never substitute another mode.
    fn pointer_click<'a>(&'a self, tab: &'a str, at: Point, mode: InputMode)
        -> EngineFuture<'a, ()>;
    /// PNG bytes of the visible viewport (`screenshot`).
    fn screenshot<'a>(&'a self, tab: &'a str) -> EngineFuture<'a, Vec<u8>>;
    /// Deliver a page-invoked eval reply (`browser_agent_eval_result`).
    /// Returns false when the nonce is unknown.
    fn deliver_script_reply(&self, nonce: &str, ok: bool, value: Option<String>) -> bool;
}

/// The engine adapter for this desktop OS.
pub fn desktop_engine(tabs: Arc<BrowserTabManager>) -> Arc<dyn BrowserEngine> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(webview2::WebView2Engine::new(tabs))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Arc::new(jsbridge::JsBridgeEngine::new(tabs))
    }
}

#[cfg(test)]
mod tests;
