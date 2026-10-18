//! Origin consent for agent browser calls: classify the call's target,
//! honour the session's grants and the project allow-list, and raise a
//! host-originated consent ticket for everything else. Owns the Stop
//! latch, because Stop revokes exactly what consent grants.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;
use tauri::Url;

use super::authz::{DenyReason, HopVerdict, PromptKind, SessionGrants};
use super::dispatch::Verb;
use super::ports::{
    CanonicalOrigin, ConsentOption, PermissionRequest, PolicyRefusal, PolicyStore,
    ProjectPolicyOp, TabProfile, TicketId, TicketOutcome, TicketRequest,
};
use super::stop::{stopped_error, StopLatch, StopToken};
use super::{BrowserError, DesktopBrowserHost, ERR_CONFIRMATION};
use crate::acp::browser_policy::OriginPattern;
use crate::browser_automation::budgets::{self, Deadline};
use crate::browser_automation::BrowserCall;

/// CAS retries when "always allow for this project" races another edit.
const PERSIST_ATTEMPTS: usize = 3;

pub struct OriginRequest<'a> {
    pub session_id: &'a str,
    pub project_id: &'a str,
    pub url: &'a str,
    pub element: Option<&'a str>,
    pub profile: TabProfile,
    /// The call's deadline: the consent wait never outlasts it.
    pub deadline: Deadline,
}

fn confirmation(reason: &'static str, message: String) -> BrowserError {
    BrowserError::new(ERR_CONFIRMATION, message).with_reason(reason)
}

fn denied(reason: DenyReason) -> BrowserError {
    BrowserError::invalid("this origin is never reachable from the agent browser")
        .with_reason(reason.as_str())
        .with_arg_keys(&["url"])
}

fn outcome_label(outcome: &TicketOutcome) -> &'static str {
    match outcome {
        TicketOutcome::Answered(ConsentOption::AllowSession) => "allow_session",
        TicketOutcome::Answered(ConsentOption::AllowProject) => "allow_project",
        TicketOutcome::Answered(ConsentOption::Deny) => "deny",
        TicketOutcome::Unattended => "unattended",
        TicketOutcome::TimedOut => "timed_out",
        TicketOutcome::Cancelled => "cancelled",
    }
}

pub struct ConsentGate {
    grants: SessionGrants,
    stop: StopLatch,
    permissions: Arc<dyn PermissionRequest>,
    policy: Arc<dyn PolicyStore>,
    /// One consent ticket per session at a time: a concurrent call for the
    /// same origin waits, then finds the grant instead of prompting again.
    prompts: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl ConsentGate {
    pub fn new(
        permissions: Arc<dyn PermissionRequest>,
        policy: Arc<dyn PolicyStore>,
    ) -> Self {
        Self {
            grants: SessionGrants::default(),
            stop: StopLatch::default(),
            permissions,
            policy,
            prompts: Mutex::new(HashMap::new()),
        }
    }

    pub fn begin(&self, session_id: &str) -> Result<StopToken, BrowserError> {
        self.stop.begin(session_id)
    }

    pub async fn authorize(
        &self,
        token: &StopToken,
        req: OriginRequest<'_>,
    ) -> Result<(), BrowserError> {
        let url: Url = req
            .url
            .parse()
            .map_err(|_| BrowserError::invalid("invalid url").with_arg_keys(&["url"]))?;
        let allow_list = self.allow_list(req.project_id).await;
        self.grants.set_allow_list(req.session_id, allow_list);
        let lock = self
            .prompts
            .lock()
            .entry(req.session_id.to_string())
            .or_default()
            .clone();
        let _one_prompt = lock.lock().await;
        token.check()?;
        match self.grants.check_hop(req.session_id, &url) {
            HopVerdict::Allow => Ok(()),
            HopVerdict::Deny(reason) => {
                log::info!(
                    "[browser-agent] origin denied code={} reason={} keys=[url]",
                    super::ERR_INVALID_PARAMS,
                    reason.as_str()
                );
                Err(denied(reason))
            }
            HopVerdict::Ungranted(kind, origin) => self.prompt(token, &req, kind, origin).await,
        }
    }

    async fn allow_list(&self, project_id: &str) -> Vec<OriginPattern> {
        match self.policy.project(project_id).await {
            Ok(policy) => policy.allow_list,
            Err(error) => {
                log::warn!(
                    "[browser-agent] policy read failed reason={}; using an empty allow-list",
                    error.reason()
                );
                Vec::new()
            }
        }
    }

    async fn prompt(
        &self,
        token: &StopToken,
        req: &OriginRequest<'_>,
        kind: PromptKind,
        origin: CanonicalOrigin,
    ) -> Result<(), BrowserError> {
        let ticket = TicketRequest {
            id: TicketId::mint(),
            session_id: req.session_id.to_string(),
            project_id: req.project_id.to_string(),
            origin: origin.clone(),
            target_url: req.url.to_string(),
            action: kind.action(),
            element: req.element.map(str::to_string),
            profile: req.profile,
        };
        let outcome = self
            .permissions
            .request(ticket, req.deadline.clamp(budgets::CONSENT_WAIT))
            .await;
        log::info!(
            "[browser-agent] consent outcome={} session={}",
            outcome_label(&outcome),
            crate::logging::redact_session_id(req.session_id)
        );
        let origin_text = origin.as_str().to_string();
        match outcome {
            TicketOutcome::Answered(ConsentOption::AllowSession) => self.keep_grant(token, req.session_id, origin),
            TicketOutcome::Answered(ConsentOption::AllowProject) => {
                token.check()?;
                self.persist_origin(req.project_id, &origin).await;
                self.keep_grant(token, req.session_id, origin)
            }
            TicketOutcome::Answered(ConsentOption::Deny) => Err(confirmation(
                "denied_by_user",
                format!("the user denied the agent browser access to {origin_text}"),
            )),
            TicketOutcome::Unattended => Err(confirmation(
                "unattended",
                format!(
                    "no one can approve {origin_text} right now; ask the user to allow it or add it to the project's browser allow-list"
                ),
            )),
            TicketOutcome::TimedOut => Err(confirmation(
                "consent_timeout",
                format!("the user did not answer the request to open {origin_text}"),
            )),
            TicketOutcome::Cancelled if token.is_stopped() => Err(stopped_error()),
            TicketOutcome::Cancelled => Err(confirmation(
                "consent_cancelled",
                format!("the request to open {origin_text} was withdrawn"),
            )),
        }
    }

    fn keep_grant(
        &self,
        token: &StopToken,
        session_id: &str,
        origin: CanonicalOrigin,
    ) -> Result<(), BrowserError> {
        self.stop
            .commit(session_id, token, || self.grants.grant(session_id, origin))
    }

    async fn persist_origin(&self, project_id: &str, origin: &CanonicalOrigin) {
        let op = || ProjectPolicyOp::AddOrigin(OriginPattern::Exact(origin.clone()));
        let mut expected = match self.policy.project(project_id).await {
            Ok(policy) => policy.revision,
            Err(error) => {
                log::warn!("[browser-agent] consent persist failed reason={}", error.reason());
                return;
            }
        };
        for _ in 0..PERSIST_ATTEMPTS {
            match self.policy.apply_project(project_id, expected, op()).await {
                Ok(_) => return,
                Err(PolicyRefusal::RevisionConflict { current }) => expected = current,
                Err(error) => {
                    log::warn!("[browser-agent] consent persist failed reason={}", error.reason());
                    return;
                }
            }
        }
        log::warn!("[browser-agent] consent persist failed reason=revision_conflict");
    }

    /// Stop: latch the session, revoke its grants and withdraw its pending
    /// tickets. Returns false when it was already stopped.
    pub fn stop(&self, session_id: &str) -> bool {
        let first = self
            .stop
            .stop(session_id, || self.grants.revoke_session(session_id));
        self.permissions.cancel_session(session_id);
        first
    }

    /// The user's next prompt turn. Returns whether the session was stopped.
    pub fn resume(&self, session_id: &str) -> bool {
        self.stop.resume(session_id)
    }

    pub fn is_stopped(&self, session_id: &str) -> bool {
        self.stop.is_stopped(session_id)
    }

    pub fn check_hop(&self, session_id: &str, url: &Url) -> HopVerdict {
        self.grants.check_hop(session_id, url)
    }

    pub fn end_session(&self, session_id: &str) {
        self.stop.forget(session_id);
        self.grants.forget_session(session_id);
        self.prompts.lock().remove(session_id);
        self.permissions.cancel_session(session_id);
    }
}

impl DesktopBrowserHost {
    /// The URL a call needs consent for: the target of `navigate` and
    /// `new_tab`, otherwise the page the targeted tab shows now. `None`
    /// when the call reaches no page or its tab does not resolve (the verb
    /// itself then reports the error).
    fn consent_target(&self, session_id: &str, verb: Verb, call: &BrowserCall) -> Option<String> {
        match verb {
            Verb::Navigate | Verb::NewTab => call
                .args
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_string),
            Verb::ListTabs | Verb::CloseTab => None,
            _ => {
                let tab = call.args.get("tabId").and_then(Value::as_str);
                let (tab_id, _) = self.resolve_tab(session_id, tab).ok()?;
                self.surface.info(&tab_id).ok().map(|info| info.url)
            }
        }
    }

    pub(super) async fn authorize_call(
        &self,
        token: &StopToken,
        session_id: &str,
        verb: Verb,
        call: &BrowserCall,
        deadline: Deadline,
    ) -> Result<(), BrowserError> {
        let Some(url) = self.consent_target(session_id, verb, call) else {
            return Ok(());
        };
        let project_id = (self.project_of)(session_id);
        self.gate
            .authorize(
                token,
                OriginRequest {
                    session_id,
                    project_id: &project_id,
                    url: &url,
                    element: call.element.as_deref(),
                    // Every agent tab runs in the app's shared webview profile today.
                    profile: TabProfile::Shared,
                    deadline,
                },
            )
            .await
    }
}

#[cfg(test)]
#[path = "consent_tests.rs"]
mod tests;
