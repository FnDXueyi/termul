//! The two policy documents and the operations that edit them.
//!
//! Operations are the only mutation path. Each is idempotent: applying it
//! twice converges on the same content, and the store only bumps the
//! revision when content actually changed.

use serde::{Deserialize, Serialize};

use super::origin::{CanonicalOrigin, OriginError, OriginPattern};
use super::refusal::PolicyRefusal;

/// Entries a policy list may hold. Bounds file size and UI length.
pub const MAX_ALLOW_LIST: usize = 256;
pub const MAX_PREAPPROVALS: usize = 512;

/// Requested agent identity for a project's sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileMode {
    Global,
    Ephemeral,
}

/// Closed set of actions a consent ticket or a pre-approval names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

impl ActionClass {
    /// Navigation classes are governed by the allow-list, not pre-approvals.
    pub const fn is_preapprovable(self) -> bool {
        !matches!(self, Self::Navigate | Self::LocalNetwork)
    }

    /// Password and payment submits: never pre-approvable on global-profile
    /// tabs, because that profile's logins are shared by every project.
    pub const fn is_credential_submit(self) -> bool {
        matches!(self, Self::PasswordSubmit | Self::PaymentSubmit)
    }
}

/// A pre-approval key: one action class on one exact origin.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Preapproval {
    pub action: ActionClass,
    pub origin: CanonicalOrigin,
}

/// A pre-approval that ephemeral-profile tabs of one project honour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPreapproval(Preapproval);

/// A pre-approval that global-profile tabs honour, in every project. The
/// constructor refuses password and payment submits, so such an entry
/// cannot exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPreapproval(Preapproval);

impl ProjectPreapproval {
    pub fn into_inner(self) -> Preapproval {
        self.0
    }

    pub fn new(action: ActionClass, origin: CanonicalOrigin) -> Result<Self, PolicyRefusal> {
        if !action.is_preapprovable() {
            return Err(PolicyRefusal::PreapprovalNotAllowed);
        }
        Ok(Self(Preapproval { action, origin }))
    }
}

impl AppPreapproval {
    pub fn into_inner(self) -> Preapproval {
        self.0
    }

    pub fn new(action: ActionClass, origin: CanonicalOrigin) -> Result<Self, PolicyRefusal> {
        if !action.is_preapprovable() || action.is_credential_submit() {
            return Err(PolicyRefusal::PreapprovalNotAllowed);
        }
        Ok(Self(Preapproval { action, origin }))
    }
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
    AddPreapproval(ProjectPreapproval),
    RemovePreapproval(Preapproval),
    SetEphemeral(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppPolicyOp {
    SetGlobalProfile(bool),
    AddPreapproval(AppPreapproval),
    RemovePreapproval(Preapproval),
}

/// Insert or remove `item`. `Ok(true)` when the list changed.
fn set_membership<T: PartialEq>(
    items: &mut Vec<T>,
    item: T,
    present: bool,
    cap: usize,
) -> Result<bool, PolicyRefusal> {
    match (items.iter().position(|x| *x == item), present) {
        (None, true) if items.len() >= cap => Err(PolicyRefusal::LimitExceeded),
        (None, true) => {
            items.push(item);
            Ok(true)
        }
        (Some(index), false) => {
            items.remove(index);
            Ok(true)
        }
        _ => Ok(false),
    }
}

impl ProjectPolicy {
    /// Apply one operation. `Ok(true)` when content changed; the store owns
    /// the revision bump.
    pub fn apply(&mut self, op: ProjectPolicyOp) -> Result<bool, PolicyRefusal> {
        match op {
            ProjectPolicyOp::AddOrigin(pattern) => {
                set_membership(&mut self.allow_list, pattern, true, MAX_ALLOW_LIST)
            }
            ProjectPolicyOp::RemoveOrigin(pattern) => {
                set_membership(&mut self.allow_list, pattern, false, MAX_ALLOW_LIST)
            }
            ProjectPolicyOp::AddPreapproval(ProjectPreapproval(item)) => {
                set_membership(&mut self.preapprovals, item, true, MAX_PREAPPROVALS)
            }
            ProjectPolicyOp::RemovePreapproval(item) => {
                set_membership(&mut self.preapprovals, item, false, MAX_PREAPPROVALS)
            }
            ProjectPolicyOp::SetEphemeral(ephemeral) => {
                let mode = if ephemeral {
                    ProfileMode::Ephemeral
                } else {
                    ProfileMode::Global
                };
                let changed = self.profile_mode != mode;
                self.profile_mode = mode;
                Ok(changed)
            }
        }
    }

    /// Whether `origin` is allow-listed (auto-granted without a prompt).
    pub fn allows(&self, origin: &CanonicalOrigin) -> bool {
        self.allow_list
            .iter()
            .any(|pattern| pattern.matches(origin))
    }

    /// Whether `action` on `origin` is pre-approved for ephemeral tabs.
    pub fn preapproves(&self, action: ActionClass, origin: &CanonicalOrigin) -> bool {
        self.preapprovals
            .iter()
            .any(|p| p.action == action && &p.origin == origin)
    }
}

impl AppPolicy {
    pub fn apply(&mut self, op: AppPolicyOp) -> Result<bool, PolicyRefusal> {
        match op {
            AppPolicyOp::SetGlobalProfile(enabled) => {
                let changed = self.global_profile_enabled != enabled;
                self.global_profile_enabled = enabled;
                Ok(changed)
            }
            AppPolicyOp::AddPreapproval(AppPreapproval(item)) => {
                set_membership(&mut self.preapprovals, item, true, MAX_PREAPPROVALS)
            }
            AppPolicyOp::RemovePreapproval(item) => {
                set_membership(&mut self.preapprovals, item, false, MAX_PREAPPROVALS)
            }
        }
    }

    /// Whether `action` on `origin` is pre-approved for global-profile tabs.
    pub fn preapproves(&self, action: ActionClass, origin: &CanonicalOrigin) -> bool {
        !action.is_credential_submit()
            && self
                .preapprovals
                .iter()
                .any(|p| p.action == action && &p.origin == origin)
    }
}

/// A pre-approval's origin: exact, canonical and not reserved.
pub fn preapproval_origin(
    input: &str,
    owned_ports: &[u16],
) -> Result<CanonicalOrigin, OriginError> {
    let origin = CanonicalOrigin::parse(input)?;
    match origin.reserved_kind(owned_ports) {
        Some(kind) => Err(OriginError::Reserved(kind)),
        None => Ok(origin),
    }
}

/// Project ids name a file, so they are restricted to a safe charset.
pub fn validate_project_id(id: &str) -> Result<&str, PolicyRefusal> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
    if valid {
        Ok(id)
    } else {
        Err(PolicyRefusal::InvalidProjectId)
    }
}
