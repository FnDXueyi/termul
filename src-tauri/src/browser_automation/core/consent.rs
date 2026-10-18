//! One-time per-session consent prompt (`acp:browser_consent_request` →
//! `browser_consent_respond`).

use serde::Serialize;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::{BrowserError, DesktopBrowserHost, ERR_CONFIRMATION};
use crate::browser_automation::BrowserCall;

/// How long the consent prompt may sit unanswered before auto-deny.
const CONSENT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

const EVENT_CONSENT_REQUEST: &str = "acp:browser_consent_request";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConsentRequestEvent {
    pub request_id: String,
    pub session_id: String,
    pub agent_id: String,
    /// First action the agent wants to run — surfaced in the dialog.
    pub action: String,
    /// Agent-stated intent for the first mutating action, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
}

pub(super) struct PendingConsent {
    request_id: String,
    waiters: Vec<oneshot::Sender<bool>>,
}

impl DesktopBrowserHost {
    pub(super) async fn ensure_consent(
        &self,
        session_id: &str,
        agent_id: &str,
        call: &BrowserCall,
    ) -> Result<(), BrowserError> {
        if self.consent.lock().contains_key(session_id) {
            return Ok(());
        }
        let (tx, rx) = oneshot::channel::<bool>();
        {
            let mut pending = self.pending_consent.lock();
            match pending.get_mut(session_id) {
                // A prompt is already on screen for this session — queue
                // behind it rather than stacking a second dialog.
                Some(pc) => pc.waiters.push(tx),
                None => {
                    let request_id = Uuid::new_v4().to_string();
                    pending.insert(
                        session_id.to_string(),
                        PendingConsent {
                            request_id: request_id.clone(),
                            waiters: vec![tx],
                        },
                    );
                    drop(pending);
                    self.emit(
                        Some(session_id),
                        EVENT_CONSENT_REQUEST,
                        &BrowserConsentRequestEvent {
                            request_id,
                            session_id: session_id.to_string(),
                            agent_id: agent_id.to_string(),
                            action: call.action.clone(),
                            element: call.element.clone(),
                        },
                    );
                }
            }
        }
        // Timeout or dropped sender (renderer gone) → fail closed.
        let allowed = matches!(tokio::time::timeout(CONSENT_TIMEOUT, rx).await, Ok(Ok(true)));
        if allowed {
            self.consent.lock().insert(session_id.to_string(), ());
            log::info!(
                "[browser-agent] consent granted for session {}",
                crate::logging::redact_session_id(session_id)
            );
            Ok(())
        } else {
            log::info!(
                "[browser-agent] consent denied/timed out for session {}",
                crate::logging::redact_session_id(session_id)
            );
            Err(BrowserError::new(
                ERR_CONFIRMATION,
                "user did not grant browser automation for this session",
            ))
        }
    }

    /// Answer the pending prompt carrying `request_id`; false when none does.
    pub(super) fn answer_consent(&self, request_id: &str, allowed: bool) -> bool {
        let mut pending = self.pending_consent.lock();
        let Some(session) = pending
            .iter()
            .find(|(_, pc)| pc.request_id == request_id)
            .map(|(sid, _)| sid.clone())
        else {
            return false;
        };
        let Some(pc) = pending.remove(&session) else {
            return false;
        };
        drop(pending);
        for tx in pc.waiters {
            let _ = tx.send(allowed);
        }
        true
    }

    /// Drop the session's grant and deny its in-flight prompt immediately
    /// (waiters would otherwise sit until the consent timeout).
    pub(super) fn forget_consent(&self, session_id: &str) {
        self.consent.lock().remove(session_id);
        if let Some(pc) = self.pending_consent.lock().remove(session_id) {
            for tx in pc.waiters {
                let _ = tx.send(false);
            }
        }
    }
}
