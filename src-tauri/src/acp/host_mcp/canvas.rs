//! The canvas call router: the parent-side half of the agent gateway (spine
//! AD-21, AD-29, AD-30).
//!
//! A canvas tool call arrives from the `host_mcp` child's `Bridge` as a
//! `Canvas` frame. The parent resolves the session's project and hands the
//! call to [`CanvasRouter::call`], which forwards it to the renderer that holds
//! that project's mount and waits on a oneshot for the reply. The router is the
//! only place a canvas call times out. It keeps no document or selection
//! state: params and results pass through as JSON text.
//!
//! Renderers report their mounts (`canvas_mount` / `canvas_unmount` on `/ws`,
//! the same protocol on the desktop Channel and the web socket). Until the
//! E2-4 lease exists, the newest report for a project wins. A mount replaced
//! by another `(holder, mountSeq)`, an unmount, or the holder's connection
//! closing resolves every call in flight against it with `CANVAS_CLOSED`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

/// `canvas.call_timeout_ms` default (AD-29).
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_millis(10_000);

/// Top-level type of the parent → renderer frame on `/ws`.
pub const CANVAS_CALL_TYPE: &str = "canvas_call";

/// Typed canvas error codes (integration contracts, "Typed errors").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CanvasErrorCode {
    CanvasClosed,
    CanvasClosing,
    CanvasHidden,
    CanvasOpenElsewhere,
    CanvasTimeout,
    UnsupportedInKiosk,
    UnknownMethod,
    EngineError,
}

impl CanvasErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanvasClosed => "CANVAS_CLOSED",
            Self::CanvasClosing => "CANVAS_CLOSING",
            Self::CanvasHidden => "CANVAS_HIDDEN",
            Self::CanvasOpenElsewhere => "CANVAS_OPEN_ELSEWHERE",
            Self::CanvasTimeout => "CANVAS_TIMEOUT",
            Self::UnsupportedInKiosk => "UNSUPPORTED_IN_KIOSK",
            Self::UnknownMethod => "UNKNOWN_METHOD",
            Self::EngineError => "ENGINE_ERROR",
        }
    }

    /// A wire code; anything outside the contract set is an `ENGINE_ERROR`.
    #[must_use]
    pub fn parse(code: &str) -> Self {
        serde_json::from_value(serde_json::Value::String(code.to_string()))
            .unwrap_or(Self::EngineError)
    }
}

/// `{code, message}`, the error half of every canvas reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasError {
    pub code: CanvasErrorCode,
    pub message: String,
}

impl CanvasError {
    #[must_use]
    pub fn new(code: CanvasErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The `Err(String)` figcraft's `Backend` carries: the JSON encoding of
    /// `{code, message}`.
    #[must_use]
    pub fn to_backend_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                r#"{{"code":"{}","message":"unencodable"}}"#,
                self.code.as_str()
            )
        })
    }
}

/// A forwarded call's outcome: the result's JSON text, or a typed error.
pub type CanvasOutcome = Result<String, CanvasError>;

/// Parent → renderer: `{"type":"canvas_call", callId, projectId, mountSeq,
/// method, params}`. `params` is JSON text the renderer forwards unparsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasCallFrame {
    #[serde(rename = "type")]
    type_: &'static str,
    pub call_id: u64,
    pub project_id: String,
    pub mount_seq: u64,
    pub method: String,
    pub params: String,
}

/// Renderer → parent `canvas_reply` payload: `{callId, mountSeq, ok, result}`
/// or `{callId, mountSeq, ok: false, error: {code, message}}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasReplyPayload {
    pub call_id: u64,
    pub mount_seq: u64,
    pub ok: bool,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub error: Option<CanvasReplyError>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CanvasReplyError {
    pub code: String,
    pub message: String,
}

impl CanvasReplyPayload {
    /// Parse the wire reply into an outcome. An `ok` reply without a result is
    /// malformed; an unknown error code is an `ENGINE_ERROR`.
    pub fn into_outcome(self) -> CanvasOutcome {
        match (self.ok, self.result, self.error) {
            (true, Some(result), _) => Ok(result),
            (false, _, Some(error)) => Err(CanvasError::new(
                CanvasErrorCode::parse(&error.code),
                error.message,
            )),
            _ => Err(CanvasError::new(
                CanvasErrorCode::EngineError,
                "malformed reply from the canvas renderer",
            )),
        }
    }
}

/// `canvas_mount` / `canvas_unmount` payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasMountPayload {
    pub project_id: String,
    pub mount_seq: u64,
}

/// Hands a frame to the renderer connection. `false` means it was not queued
/// (the connection is gone or its queue is full).
pub type Deliver = Arc<dyn Fn(CanvasCallFrame) -> bool + Send + Sync>;

/// A renderer connection that can hold mounts.
#[derive(Clone)]
pub struct CanvasHolder {
    id: String,
    deliver: Deliver,
}

impl CanvasHolder {
    #[must_use]
    pub fn new(id: impl Into<String>, deliver: Deliver) -> Self {
        Self {
            id: id.into(),
            deliver,
        }
    }
}

struct Mount {
    mount_seq: u64,
    holder: CanvasHolder,
}

struct Pending {
    project_id: String,
    holder_id: String,
    mount_seq: u64,
    responder: oneshot::Sender<CanvasOutcome>,
}

#[derive(Default)]
struct RouterState {
    /// projectId → the mount calls for that project go to.
    mounts: HashMap<String, Mount>,
    /// callId → the call waiting for its reply.
    pending: HashMap<u64, Pending>,
}

impl RouterState {
    /// Remove the pending calls `doomed` selects.
    fn take_pending(&mut self, doomed: impl Fn(&Pending) -> bool) -> Vec<Pending> {
        let ids: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, pending)| doomed(pending))
            .map(|(id, _)| *id)
            .collect();
        ids.iter()
            .filter_map(|id| self.pending.remove(id))
            .collect()
    }
}

fn close_all(pending: Vec<Pending>, message: &str) {
    for call in pending {
        let _ = call.responder.send(Err(CanvasError::new(
            CanvasErrorCode::CanvasClosed,
            message,
        )));
    }
}

/// Routes canvas calls to the renderer holding each project's mount.
pub struct CanvasRouter {
    state: Mutex<RouterState>,
    next_call_id: AtomicU64,
    call_timeout_ms: AtomicU64,
}

impl Default for CanvasRouter {
    fn default() -> Self {
        Self::new(DEFAULT_CALL_TIMEOUT)
    }
}

impl CanvasRouter {
    #[must_use]
    pub fn new(call_timeout: Duration) -> Self {
        Self {
            state: Mutex::new(RouterState::default()),
            next_call_id: AtomicU64::new(1),
            call_timeout_ms: AtomicU64::new(duration_ms(call_timeout)),
        }
    }

    /// `canvas.call_timeout_ms`: how long a forwarded call waits for its reply.
    pub fn set_call_timeout(&self, timeout: Duration) {
        self.call_timeout_ms
            .store(duration_ms(timeout), Ordering::Relaxed);
    }

    /// `holder` now holds `project_id`'s canvas at `mount_seq`. Calls in
    /// flight against any other `(holder, mountSeq)` of the project resolve
    /// `CANVAS_CLOSED`.
    pub fn mount(&self, project_id: &str, mount_seq: u64, holder: CanvasHolder) {
        let holder_id = holder.id.clone();
        let superseded = {
            let mut state = self.state.lock();
            state
                .mounts
                .insert(project_id.to_string(), Mount { mount_seq, holder });
            state.take_pending(|call| {
                call.project_id == project_id
                    && (call.holder_id != holder_id || call.mount_seq != mount_seq)
            })
        };
        log::info!(
            "[canvas-gateway] project {project_id} mounted at mountSeq={mount_seq} ({} in-flight calls closed)",
            superseded.len()
        );
        close_all(superseded, "the canvas was remounted");
    }

    /// Drop `project_id`'s mount when `holder_id` still holds it at
    /// `mount_seq`. Returns whether it did.
    pub fn unmount(&self, project_id: &str, holder_id: &str, mount_seq: u64) -> bool {
        let closed = {
            let mut state = self.state.lock();
            let current = state
                .mounts
                .get(project_id)
                .is_some_and(|mount| mount.holder.id == holder_id && mount.mount_seq == mount_seq);
            if !current {
                return false;
            }
            state.mounts.remove(project_id);
            state.take_pending(|call| call.project_id == project_id)
        };
        log::info!("[canvas-gateway] project {project_id} unmounted (mountSeq={mount_seq})");
        close_all(closed, "the canvas was unmounted");
        true
    }

    /// The holder's connection ended: drop its mounts and close its calls.
    pub fn release_holder(&self, holder_id: &str) {
        let (projects, closed) = {
            let mut state = self.state.lock();
            let projects: Vec<String> = state
                .mounts
                .iter()
                .filter(|(_, mount)| mount.holder.id == holder_id)
                .map(|(project, _)| project.clone())
                .collect();
            for project in &projects {
                state.mounts.remove(project);
            }
            (
                projects,
                state.take_pending(|call| call.holder_id == holder_id),
            )
        };
        for project in &projects {
            log::info!("[canvas-gateway] project {project} unmounted: its renderer disconnected");
        }
        close_all(closed, "the canvas renderer disconnected");
    }

    /// A renderer answered `call_id`. Only the connection the call went to may
    /// answer it; a reply stamped with another `mountSeq` means the mount
    /// changed under the call, which resolves `CANVAS_CLOSED`. Returns whether
    /// a waiting call was resolved.
    pub fn reply(
        &self,
        holder_id: &str,
        call_id: u64,
        mount_seq: u64,
        outcome: CanvasOutcome,
    ) -> bool {
        let pending = {
            let mut state = self.state.lock();
            match state.pending.get(&call_id) {
                Some(call) if call.holder_id == holder_id => state.pending.remove(&call_id),
                _ => None,
            }
        };
        let Some(call) = pending else {
            return false;
        };
        let outcome = if call.mount_seq == mount_seq {
            outcome
        } else {
            log::warn!(
                "[canvas-gateway] reply for project {} came from mountSeq={mount_seq}, call was sent to mountSeq={}",
                call.project_id,
                call.mount_seq
            );
            Err(CanvasError::new(
                CanvasErrorCode::CanvasClosed,
                "the canvas was remounted",
            ))
        };
        let _ = call.responder.send(outcome);
        true
    }

    /// Forward `method` with `params` (JSON text) to the renderer holding
    /// `project_id`'s mount and wait for its reply, up to the call timeout.
    pub async fn call(&self, project_id: &str, method: &str, params: String) -> CanvasOutcome {
        let call_id = self.next_call_id.fetch_add(1, Ordering::Relaxed);
        let (responder, waiting) = oneshot::channel();
        let routed = {
            let mut state = self.state.lock();
            match state.mounts.get(project_id) {
                Some(mount) => {
                    let route = (mount.holder.clone(), mount.mount_seq);
                    state.pending.insert(
                        call_id,
                        Pending {
                            project_id: project_id.to_string(),
                            holder_id: route.0.id.clone(),
                            mount_seq: route.1,
                            responder,
                        },
                    );
                    Some(route)
                }
                None => None,
            }
        };
        let Some((holder, mount_seq)) = routed else {
            log::warn!(
                "[canvas-gateway] {method} for project {project_id} refused: no canvas is mounted"
            );
            return Err(CanvasError::new(
                CanvasErrorCode::CanvasClosed,
                "no canvas is open for this project",
            ));
        };

        let frame = CanvasCallFrame {
            type_: CANVAS_CALL_TYPE,
            call_id,
            project_id: project_id.to_string(),
            mount_seq,
            method: method.to_string(),
            params,
        };
        if !(holder.deliver)(frame) {
            self.state.lock().pending.remove(&call_id);
            log::warn!(
                "[canvas-gateway] {method} for project {project_id} not delivered: the renderer connection is closed or full"
            );
            return Err(CanvasError::new(
                CanvasErrorCode::CanvasClosed,
                "the canvas renderer is not reachable",
            ));
        }

        let timeout_ms = self.call_timeout_ms.load(Ordering::Relaxed);
        match tokio::time::timeout(Duration::from_millis(timeout_ms), waiting).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => Err(CanvasError::new(
                CanvasErrorCode::CanvasClosed,
                "the canvas closed before it answered",
            )),
            Err(_) => {
                self.state.lock().pending.remove(&call_id);
                log::warn!(
                    "[canvas-gateway] {method} for project {project_id} timed out after {timeout_ms} ms (mountSeq={mount_seq})"
                );
                Err(CanvasError::new(
                    CanvasErrorCode::CanvasTimeout,
                    format!("the canvas did not answer within {timeout_ms} ms"),
                ))
            }
        }
    }
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
