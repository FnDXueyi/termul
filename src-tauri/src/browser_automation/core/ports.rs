//! Host-service ports the core consumes: host-originated consent tickets
//! and the browser policy store. Implementations live in `acp/` and
//! `web/`; the core only sees these traits.

#![allow(
    dead_code,
    reason = "consumed by the consent-ticket and policy-store implementations"
)]

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde::Serialize;
use uuid::Uuid;

pub type PortFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Canonical origin `scheme://host[:port]`: http(s) only, lowercase,
/// punycode host, default port elided.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CanonicalOrigin(String);

impl CanonicalOrigin {
    /// Parse a URL or a bare origin. `None` for non-http(s) schemes,
    /// missing hosts, and anything carrying userinfo.
    pub fn parse(input: &str) -> Option<Self> {
        let url = input.parse::<tauri::Url>().ok()?;
        if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() {
            return None;
        }
        let host = url.host_str().filter(|h| !h.contains('*'))?;
        Some(Self(match url.port() {
            Some(port) => format!("{}://{host}:{port}", url.scheme()),
            None => format!("{}://{host}", url.scheme()),
        }))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An allow-list entry: one exact origin, or every subdomain of a host
/// (`https://*.example.com`, which does not match `example.com` itself).
/// Never a bare `*`, a scheme wildcard or a port wildcard.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OriginPattern {
    Exact(CanonicalOrigin),
    Subdomains {
        /// Canonical origin of the parent host, e.g. `https://example.com`.
        parent: CanonicalOrigin,
    },
}

impl OriginPattern {
    pub fn parse(input: &str) -> Option<Self> {
        match input.split_once("://*.") {
            Some((scheme, rest)) => {
                let parent = CanonicalOrigin::parse(&format!("{scheme}://{rest}"))?;
                // `*.` must be followed by a real host, never another wildcard.
                (!rest.contains('*')).then_some(Self::Subdomains { parent })
            }
            None if input.contains('*') => None,
            None => CanonicalOrigin::parse(input).map(Self::Exact),
        }
    }

    pub fn matches(&self, origin: &CanonicalOrigin) -> bool {
        match self {
            Self::Exact(exact) => exact == origin,
            Self::Subdomains { parent } => {
                let (scheme, host_port) = parent.as_str().split_once("://").unwrap_or_default();
                origin
                    .as_str()
                    .strip_prefix(scheme)
                    .and_then(|rest| rest.strip_prefix("://"))
                    .and_then(|rest| rest.strip_suffix(host_port))
                    .is_some_and(|sub| sub.len() > 1 && sub.ends_with('.'))
            }
        }
    }
}

/// Requested agent identity for a project's sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileMode {
    Global,
    Ephemeral,
}

/// The identity a tab was actually built with (after engine fallbacks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TabProfile {
    Global,
    Ephemeral,
    /// The app's own webview profile, shared with the user's tabs: what
    /// every agent tab runs in until per-OS agent profiles land.
    Shared,
}

/// Closed set of actions a consent ticket or a pre-approval names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionClass {
    /// First navigation to an unlisted origin in a session.
    Navigate,
    /// Navigation to a private or local-network address.
    LocalNetwork,
    PasswordSubmit,
    PaymentSubmit,
    Upload,
    Download,
    /// Data-carrying navigation or submit after untrusted content.
    TaintedExfil,
}

/// Host ticket id, `host:<uuid>`. Never reaches the ACP manager.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct TicketId(String);

impl TicketId {
    pub const PREFIX: &'static str = "host:";

    pub fn mint() -> Self {
        Self(format!("{}{}", Self::PREFIX, Uuid::new_v4()))
    }

    pub fn parse(input: &str) -> Option<Self> {
        let uuid = input.strip_prefix(Self::PREFIX)?;
        Uuid::parse_str(uuid).ok()?;
        Some(Self(input.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The fixed consent options and their ACP permission-kind mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentOption {
    AllowSession,
    AllowProject,
    Deny,
}

impl ConsentOption {
    pub const ALL: [ConsentOption; 3] = [
        ConsentOption::AllowSession,
        ConsentOption::AllowProject,
        ConsentOption::Deny,
    ];

    pub fn acp_kind(self) -> &'static str {
        match self {
            ConsentOption::AllowSession => "allow_once",
            ConsentOption::AllowProject => "allow_always",
            ConsentOption::Deny => "reject_once",
        }
    }

    /// `_meta.termul.scope`; clients label options from this, never from
    /// the ACP kind alone.
    pub fn scope(self) -> Option<&'static str> {
        match self {
            ConsentOption::AllowSession => Some("session"),
            ConsentOption::AllowProject => Some("project"),
            ConsentOption::Deny => None,
        }
    }
}

/// What a ticket shows the approver. Closed: never page title or text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketRequest {
    pub id: TicketId,
    pub session_id: String,
    pub project_id: String,
    pub origin: CanonicalOrigin,
    pub target_url: String,
    pub action: ActionClass,
    /// Agent-supplied intent, displayed as such and never classified.
    pub element: Option<String>,
    pub profile: TabProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketOutcome {
    Answered(ConsentOption),
    /// No qualified approver is attached (or the session is flagged
    /// unattended): resolved immediately, never waited on.
    Unattended,
    TimedOut,
    /// Withdrawn by Stop or session teardown.
    Cancelled,
}

/// Host-originated consent tickets on the ACP permission contract.
pub trait PermissionRequest: Send + Sync {
    /// Raise a ticket and wait up to `wait` for the first qualified answer.
    fn request(&self, ticket: TicketRequest, wait: Duration) -> PortFuture<'_, TicketOutcome>;
    /// Resolve every pending ticket of the session as `Cancelled`.
    fn cancel_session(&self, session_id: &str);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Preapproval {
    pub action: ActionClass,
    pub origin: CanonicalOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPolicy {
    pub revision: u64,
    /// Written by a newer schema version: reads work, writes fail.
    pub read_only: bool,
    pub allow_list: Vec<OriginPattern>,
    pub preapprovals: Vec<Preapproval>,
    pub profile_mode: ProfileMode,
}

impl Default for ProjectPolicy {
    fn default() -> Self {
        Self {
            revision: 0,
            read_only: false,
            allow_list: Vec::new(),
            preapprovals: Vec::new(),
            profile_mode: ProfileMode::Global,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPolicy {
    pub revision: u64,
    pub read_only: bool,
    pub global_profile_enabled: bool,
    /// Pre-approvals honoured on global-profile tabs ("all projects").
    pub preapprovals: Vec<Preapproval>,
}

impl Default for AppPolicy {
    fn default() -> Self {
        Self {
            revision: 0,
            read_only: false,
            global_profile_enabled: true,
            preapprovals: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPolicyOp {
    AddOrigin(OriginPattern),
    RemoveOrigin(OriginPattern),
    SetPreapproval { preapproval: Preapproval, allowed: bool },
    SetProfileMode(ProfileMode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppPolicyOp {
    SetGlobalProfileEnabled(bool),
    SetPreapproval { preapproval: Preapproval, allowed: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyError {
    /// `expected_revision` is stale; `current` is the stored revision.
    Conflict { current: u64 },
    ReadOnly,
    Storage,
}

fn set_membership<T: PartialEq>(items: &mut Vec<T>, item: T, present: bool) {
    let index = items.iter().position(|x| *x == item);
    match (index, present) {
        (None, true) => items.push(item),
        (Some(i), false) => {
            items.remove(i);
        }
        _ => {}
    }
}

impl ProjectPolicy {
    /// Apply one operation. Idempotent: re-applying an op converges on the
    /// same content. The store owns the revision bump.
    #[must_use]
    pub fn apply(mut self, op: ProjectPolicyOp) -> Self {
        match op {
            ProjectPolicyOp::AddOrigin(p) => set_membership(&mut self.allow_list, p, true),
            ProjectPolicyOp::RemoveOrigin(p) => set_membership(&mut self.allow_list, p, false),
            ProjectPolicyOp::SetPreapproval { preapproval, allowed } => {
                set_membership(&mut self.preapprovals, preapproval, allowed);
            }
            ProjectPolicyOp::SetProfileMode(mode) => self.profile_mode = mode,
        }
        self
    }
}

impl AppPolicy {
    #[must_use]
    pub fn apply(mut self, op: AppPolicyOp) -> Self {
        match op {
            AppPolicyOp::SetGlobalProfileEnabled(on) => self.global_profile_enabled = on,
            AppPolicyOp::SetPreapproval { preapproval, allowed } => {
                set_membership(&mut self.preapprovals, preapproval, allowed);
            }
        }
        self
    }
}

/// Host-owned browser policy: operations only, serialized per scope by a
/// revision compare-and-swap.
pub trait PolicyStore: Send + Sync {
    fn project<'a>(&'a self, project_id: &'a str) -> PortFuture<'a, Result<ProjectPolicy, PolicyError>>;
    fn app(&self) -> PortFuture<'_, Result<AppPolicy, PolicyError>>;
    fn apply_project<'a>(
        &'a self,
        project_id: &'a str,
        expected_revision: u64,
        op: ProjectPolicyOp,
    ) -> PortFuture<'a, Result<ProjectPolicy, PolicyError>>;
    fn apply_app(
        &self,
        expected_revision: u64,
        op: AppPolicyOp,
    ) -> PortFuture<'_, Result<AppPolicy, PolicyError>>;
}

#[cfg(test)]
#[path = "ports_tests.rs"]
mod tests;
