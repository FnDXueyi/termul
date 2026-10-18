//! In-memory fakes of the host ports for core tests.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use parking_lot::Mutex;

use super::ports::{
    AppPolicy, AppPolicyOp, PermissionRequest, PolicyError, PolicyStore, PortFuture,
    ProjectPolicy, ProjectPolicyOp, TicketOutcome, TicketRequest,
};

/// Answers tickets from a script, recording every request. An empty
/// script answers `Unattended`, the safe default.
#[derive(Default)]
pub struct ScriptedPermissions {
    outcomes: Mutex<VecDeque<TicketOutcome>>,
    pub requests: Mutex<Vec<(TicketRequest, Duration)>>,
    pub cancelled_sessions: Mutex<Vec<String>>,
}

impl ScriptedPermissions {
    pub fn answering(outcomes: impl IntoIterator<Item = TicketOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().collect()),
            ..Self::default()
        }
    }
}

impl PermissionRequest for ScriptedPermissions {
    fn request(&self, ticket: TicketRequest, wait: Duration) -> PortFuture<'_, TicketOutcome> {
        self.requests.lock().push((ticket, wait));
        let outcome = self
            .outcomes
            .lock()
            .pop_front()
            .unwrap_or(TicketOutcome::Unattended);
        Box::pin(async move { outcome })
    }

    fn cancel_session(&self, session_id: &str) {
        self.cancelled_sessions.lock().push(session_id.to_string());
    }
}

/// Policy store with the real CAS and read-only rules, held in memory.
#[derive(Default)]
pub struct InMemoryPolicyStore {
    projects: Mutex<HashMap<String, ProjectPolicy>>,
    app: Mutex<AppPolicy>,
}

impl InMemoryPolicyStore {
    /// Simulate a policy file written by a newer schema version.
    pub fn mark_project_read_only(&self, project_id: &str) {
        self.projects
            .lock()
            .entry(project_id.to_string())
            .or_default()
            .read_only = true;
    }

    pub fn mark_app_read_only(&self) {
        self.app.lock().read_only = true;
    }
}

fn check_writable(read_only: bool, revision: u64, expected: u64) -> Result<(), PolicyError> {
    if read_only {
        return Err(PolicyError::ReadOnly);
    }
    if revision != expected {
        return Err(PolicyError::Conflict { current: revision });
    }
    Ok(())
}

impl PolicyStore for InMemoryPolicyStore {
    fn project<'a>(&'a self, project_id: &'a str) -> PortFuture<'a, Result<ProjectPolicy, PolicyError>> {
        let policy = self.projects.lock().get(project_id).cloned().unwrap_or_default();
        Box::pin(async move { Ok(policy) })
    }

    fn app(&self) -> PortFuture<'_, Result<AppPolicy, PolicyError>> {
        let policy = self.app.lock().clone();
        Box::pin(async move { Ok(policy) })
    }

    fn apply_project<'a>(
        &'a self,
        project_id: &'a str,
        expected_revision: u64,
        op: ProjectPolicyOp,
    ) -> PortFuture<'a, Result<ProjectPolicy, PolicyError>> {
        let mut projects = self.projects.lock();
        let current = projects.get(project_id).cloned().unwrap_or_default();
        let result = check_writable(current.read_only, current.revision, expected_revision).map(|()| {
            let mut next = current.apply(op);
            next.revision += 1;
            projects.insert(project_id.to_string(), next.clone());
            next
        });
        Box::pin(async move { result })
    }

    fn apply_app(
        &self,
        expected_revision: u64,
        op: AppPolicyOp,
    ) -> PortFuture<'_, Result<AppPolicy, PolicyError>> {
        let mut app = self.app.lock();
        let result = check_writable(app.read_only, app.revision, expected_revision).map(|()| {
            let mut next = app.clone().apply(op);
            next.revision += 1;
            *app = next.clone();
            next
        });
        Box::pin(async move { result })
    }
}
