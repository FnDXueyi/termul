//! The hook through which clearing the global agent profile reaches the
//! browser engine. The engine owns the live global tabs and the profile
//! data, so the policy layer only forwards the request and reports the
//! impact; until an engine installs itself the request is refused honestly.

use std::future::Future;
use std::pin::Pin;

use serde::Serialize;

/// What clearing the global profile closes: global-bound agent tabs, counted
/// as the sessions and projects that lose them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ClearImpact {
    pub sessions: u32,
    pub projects: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileClearError {
    /// No engine is attached that can clear the profile.
    Unavailable,
    Failed,
}

pub type ProfileFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Engine side of "clear global agent profile": take the binding lock,
/// refuse new global binds, tear down every global-bound agent tab, wait for
/// the engine to confirm close, wipe the profile, release.
pub trait AgentProfileControl: Send + Sync {
    fn clear_impact(&self) -> ClearImpact;
    fn clear_global_profile(&self) -> ProfileFuture<'_, Result<(), ProfileClearError>>;
}

/// The control used until a browser engine is attached.
pub struct UnwiredProfileControl;

impl AgentProfileControl for UnwiredProfileControl {
    fn clear_impact(&self) -> ClearImpact {
        ClearImpact::default()
    }

    fn clear_global_profile(&self) -> ProfileFuture<'_, Result<(), ProfileClearError>> {
        Box::pin(async { Err(ProfileClearError::Unavailable) })
    }
}
