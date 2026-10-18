//! The agent gateway's half of the mount registry (spine AD-19, AD-21,
//! AD-29): which renderer connection holds each mount, and the calls waiting
//! on one. `host_mcp::canvas::CanvasRouter` is the thin front the transports
//! and the `host_mcp` parent use.
//!
//! Only the mount's lease holder (AD-30) may attach a connection to it. When
//! that connection ends the holder keeps the lease for the mount's grace
//! ([`CanvasPersistence::release_holder`]); the gateway front then calls
//! [`CanvasPersistence::expire_lease`], which drops a mount nobody took up
//! again.
//!
//! A call waits on three things at once: the reply, its timeout, and the
//! connection it went to. A [`Connection`] owns a `watch` sender that is never
//! sent on; every waiting call holds a receiver, so dropping the connection
//! (a newer connection or mountSeq replaced it, the renderer reported the
//! unmount or disconnected, or the mount closed) resolves those calls
//! `CANVAS_CLOSED` without a list to sweep. An agent call that arrives while
//! the holder's connection is lost but inside its grace waits for the holder
//! to attach again, within the same call timeout.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::{oneshot, watch};
use tokio::time::Instant;

use super::{CanvasPersistence, ExpiredLease, Lifecycle, Link, Mount, RendererId};
use crate::acp::host_mcp::canvas::{
    CanvasCallFrame, CanvasError, CanvasErrorCode, CanvasHolder, CanvasOutcome,
};

/// The renderer connection a mount's editor runs in.
pub(super) struct Connection {
    holder: CanvasHolder,
    gone: watch::Sender<()>,
}

impl Connection {
    pub(super) fn holder(&self) -> &CanvasHolder {
        &self.holder
    }

    fn new(holder: CanvasHolder) -> Self {
        let (gone, _) = watch::channel(());
        Self { holder, gone }
    }
}

struct Pending {
    holder_id: String,
    mount_seq: u64,
    responder: oneshot::Sender<CanvasOutcome>,
}

#[derive(Default)]
pub(super) struct Calls {
    waiting: Mutex<HashMap<u64, Pending>>,
    next_id: AtomicU64,
}

/// Who is asking. Agent calls are refused while the mount quiesces and are
/// counted so the quiesce can wait them out; the host's own calls belong to
/// the close sequence and name the mount they expect. Presence lookups
/// follow an agent call already answered: they reach only a connected
/// editor, and never count as activity, as a call the quiesce waits for, or
/// toward the crash timeouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Caller {
    Agent,
    Host { mount_seq: u64 },
    Presence,
}

/// Keeps an agent call counted in its mount's `in_flight` until it settles.
struct InFlight(Arc<Mount>);

impl InFlight {
    /// Call with the mount's state lock held, so a quiesce that flips the
    /// lifecycle under the same lock sees every call it lets through.
    fn enter(mount: &Arc<Mount>) -> Self {
        mount.in_flight.send_modify(|count| *count += 1);
        Self(Arc::clone(mount))
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0
            .in_flight
            .send_modify(|count| *count = count.saturating_sub(1));
    }
}

struct Route {
    mount_seq: u64,
    holder: CanvasHolder,
    gone: watch::Receiver<()>,
    _in_flight: Option<InFlight>,
    /// Whether the answer or timeout feeds the crash detector.
    watched: bool,
}

/// Where a call can go right now.
enum Routed {
    Ready(Route),
    /// The holder's connection dropped and its lease grace runs until
    /// `grace_ends`; `ended` wakes when the holder attaches again or the
    /// mount goes.
    Reconnecting {
        ended: watch::Receiver<()>,
        grace_ends: Instant,
    },
}

fn refused(code: CanvasErrorCode, message: &str) -> CanvasError {
    CanvasError::new(code, message)
}

fn not_running() -> CanvasError {
    refused(
        CanvasErrorCode::CanvasClosed,
        "the canvas is not running in any window",
    )
}

fn timed_out(timeout: Duration) -> CanvasError {
    CanvasError::new(
        CanvasErrorCode::CanvasTimeout,
        format!(
            "the canvas did not answer within {} ms",
            timeout.as_millis()
        ),
    )
}

/// A mount whose holder's connection ended: the holder keeps the lease for
/// `grace`, then [`CanvasPersistence::expire_lease`] drops the mount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LostLease {
    pub project_id: String,
    pub mount_seq: u64,
    pub grace: Duration,
}

fn held_by(link: &Link, connection_id: &str) -> bool {
    link.connection()
        .is_some_and(|connection| connection.holder.id() == connection_id)
}

impl CanvasPersistence {
    /// `holder` now runs the editor of `project_id`'s mount at `mount_seq`,
    /// which also takes up a lease kept through a reconnect. A previous
    /// connection's calls resolve `CANVAS_CLOSED`. Refused (false) when the
    /// host holds no such mount or another renderer holds its lease: a stale
    /// report cannot claim a project.
    pub fn attach(&self, project_id: &str, mount_seq: u64, holder: CanvasHolder) -> bool {
        let Some(mount) = self.mounts.lock().get(project_id).cloned() else {
            return false;
        };
        if mount.mount_seq != mount_seq || mount.holder != *holder.renderer() {
            return false;
        }
        let previous = {
            let mut state = mount.state.lock();
            if state.lifecycle == Lifecycle::Closed {
                return false;
            }
            std::mem::replace(&mut state.link, Link::Attached(Connection::new(holder)))
        };
        drop(previous);
        true
    }

    /// The renderer reports it no longer runs the mount. Only the connection
    /// holding `(project_id, mount_seq)` can detach it; the mount itself
    /// stays (it is unmounted only through [`Self::close`]).
    pub fn detach(&self, project_id: &str, holder_id: &str, mount_seq: u64) -> bool {
        let Some(mount) = self.mounts.lock().get(project_id).cloned() else {
            return false;
        };
        if mount.mount_seq != mount_seq {
            return false;
        }
        let taken = {
            let mut state = mount.state.lock();
            if !held_by(&state.link, holder_id) {
                return false;
            }
            state.activity.visible = false;
            std::mem::replace(&mut state.link, Link::Unattached)
        };
        drop(taken);
        true
    }

    /// The connection `holder_id` ended. Calls it was handed resolve
    /// `CANVAS_CLOSED` now, but each mount it ran keeps its lease for the
    /// mount's grace; the caller schedules [`Self::expire_lease`] for each
    /// returned [`LostLease`].
    pub fn release_holder(&self, holder_id: &str) -> Vec<LostLease> {
        let mounts: Vec<(String, Arc<Mount>)> = self
            .mounts
            .lock()
            .iter()
            .map(|(project, mount)| (project.clone(), Arc::clone(mount)))
            .collect();
        let mut lost = Vec::new();
        for (project_id, mount) in mounts {
            let taken = {
                let mut state = mount.state.lock();
                if !held_by(&state.link, holder_id) {
                    continue;
                }
                state.activity.visible = false;
                std::mem::replace(&mut state.link, Link::lost())
            };
            drop(taken);
            lost.push(LostLease {
                project_id,
                mount_seq: mount.mount_seq,
                grace: mount.lease_grace,
            });
        }
        lost
    }

    /// The grace of a [`LostLease`] ran out: if the holder has not taken the
    /// mount up again, unmount it without a save (no renderer is left to
    /// snapshot it; its last autosave stands). `None` when the mount was
    /// reattached, replaced or closed meanwhile, or lost again since.
    pub fn expire_lease(&self, project_id: &str, mount_seq: u64) -> Option<ExpiredLease> {
        let mut mounts = self.mounts.lock();
        let mount = mounts
            .get(project_id)
            .filter(|mount| mount.mount_seq == mount_seq)
            .cloned()?;
        {
            let state = mount.state.lock();
            let Link::Lost { since, .. } = state.link else {
                return None;
            };
            if state.lifecycle != Lifecycle::Open || since.elapsed() < mount.lease_grace {
                return None;
            }
        }
        mounts.remove(project_id);
        drop(mounts);
        let expired = ExpiredLease::of(&mount.view());
        Self::retire(&mount);
        Some(expired)
    }

    /// The renderer holding `project_id`'s lease, whether or not it is
    /// connected.
    pub fn lease_holder(&self, project_id: &str) -> Option<RendererId> {
        let mount = self.mounts.lock().get(project_id).cloned()?;
        let open = mount.state.lock().lifecycle != Lifecycle::Closed;
        open.then(|| mount.holder.clone())
    }

    /// The renderer connection running `project_id`'s editor, if any.
    pub fn holder_of(&self, project_id: &str) -> Option<(u64, String)> {
        let mount = self.mounts.lock().get(project_id).cloned()?;
        let state = mount.state.lock();
        let holder_id = state.link.connection()?.holder.id().to_string();
        Some((mount.mount_seq, holder_id))
    }

    fn route(&self, project_id: &str, caller: Caller) -> Result<Routed, CanvasError> {
        let no_canvas = || {
            refused(
                CanvasErrorCode::CanvasClosed,
                "no canvas is open for this project",
            )
        };
        let mount = self
            .mounts
            .lock()
            .get(project_id)
            .cloned()
            .ok_or_else(no_canvas)?;
        if let Caller::Host { mount_seq } = caller {
            if mount.mount_seq != mount_seq {
                return Err(refused(
                    CanvasErrorCode::CanvasClosed,
                    "the canvas was remounted",
                ));
            }
        }
        let mut state = mount.state.lock();
        match (state.lifecycle, caller) {
            (Lifecycle::Closed, _) => return Err(no_canvas()),
            (Lifecycle::Closing, Caller::Agent | Caller::Presence) => {
                return Err(refused(
                    CanvasErrorCode::CanvasClosing,
                    "the canvas is closing",
                ))
            }
            _ => {}
        }
        let connection = match &state.link {
            Link::Attached(connection) => connection,
            Link::Lost { since, ended }
                if caller != Caller::Presence && since.elapsed() < mount.lease_grace =>
            {
                return Ok(Routed::Reconnecting {
                    ended: ended.subscribe(),
                    grace_ends: *since + mount.lease_grace,
                });
            }
            Link::Lost { .. } | Link::Unattached => return Err(not_running()),
        };
        let holder = connection.holder.clone();
        let gone = connection.gone.subscribe();
        if caller == Caller::Agent {
            state.activity.tick = self.tick();
        }
        Ok(Routed::Ready(Route {
            mount_seq: mount.mount_seq,
            holder,
            gone,
            _in_flight: (caller == Caller::Agent).then(|| InFlight::enter(&mount)),
            watched: caller != Caller::Presence,
        }))
    }

    /// An agent call: forwarded to the renderer holding the project's mount
    /// and answered from its editor, or a typed refusal. A call that arrives
    /// while the holder reconnects inside its lease grace waits for it to
    /// attach again. Waits up to `timeout` in all; nothing else times a
    /// canvas call out (AD-29).
    pub async fn call(
        &self,
        project_id: &str,
        method: &str,
        params: String,
        timeout: Duration,
    ) -> CanvasOutcome {
        let deadline = Instant::now() + timeout;
        loop {
            match self.route(project_id, Caller::Agent)? {
                Routed::Ready(route) => {
                    return self
                        .forward(project_id, route, method, params, (deadline, timeout))
                        .await;
                }
                Routed::Reconnecting {
                    mut ended,
                    grace_ends,
                } => {
                    tokio::select! {
                        _ = ended.changed() => {}
                        () = tokio::time::sleep_until(grace_ends.min(deadline)) => {}
                    }
                    if Instant::now() >= deadline {
                        return Err(timed_out(timeout));
                    }
                }
            }
        }
    }

    /// The host's own editor call on the mount at `mount_seq`. It passes the
    /// quiesce: the close sequence uses it to snapshot and verify.
    pub(super) async fn host_call(
        &self,
        project_id: &str,
        mount_seq: u64,
        method: &str,
        params: &str,
    ) -> CanvasOutcome {
        let Routed::Ready(route) = self.route(project_id, Caller::Host { mount_seq })? else {
            return Err(not_running());
        };
        self.forward(
            project_id,
            route,
            method,
            params.to_string(),
            (
                Instant::now() + self.host_call_timeout,
                self.host_call_timeout,
            ),
        )
        .await
    }

    /// A presence lookup (`node.get`, `document.inspect`) for an agent call
    /// already answered. It goes only to a connected editor, never waits out
    /// a reconnect, and leaves activity, the quiesce and the crash count
    /// alone (see [`Caller::Presence`]).
    pub async fn presence_call(
        &self,
        project_id: &str,
        method: &str,
        params: String,
        timeout: Duration,
    ) -> CanvasOutcome {
        let Routed::Ready(route) = self.route(project_id, Caller::Presence)? else {
            return Err(not_running());
        };
        self.forward(
            project_id,
            route,
            method,
            params,
            (Instant::now() + timeout, timeout),
        )
        .await
    }

    /// A presence notice (`host.guest*`) to the connected editor of
    /// `project_id`'s open mount. False when none is connected or the frame
    /// was not queued; nothing waits for the answer.
    pub fn presence_notice(&self, project_id: &str, method: &str, params: String) -> bool {
        let Some(mount) = self.mounts.lock().get(project_id).cloned() else {
            return false;
        };
        let holder = {
            let state = mount.state.lock();
            if state.lifecycle != Lifecycle::Open {
                return false;
            }
            let Some(connection) = state.link.connection() else {
                return false;
            };
            connection.holder.clone()
        };
        self.notify(&holder, project_id, mount.mount_seq, method, params)
    }

    async fn forward(
        &self,
        project_id: &str,
        route: Route,
        method: &str,
        params: String,
        (deadline, timeout): (Instant, Duration),
    ) -> CanvasOutcome {
        let Route {
            mount_seq,
            holder,
            mut gone,
            _in_flight,
            watched,
        } = route;
        let call_id = self.calls.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (responder, waiting) = oneshot::channel();
        self.calls.waiting.lock().insert(
            call_id,
            Pending {
                holder_id: holder.id().to_string(),
                mount_seq,
                responder,
            },
        );
        let frame = CanvasCallFrame::new(call_id, project_id, mount_seq, method, params);
        if !holder.deliver(frame) {
            self.calls.waiting.lock().remove(&call_id);
            return Err(refused(
                CanvasErrorCode::CanvasClosed,
                "the canvas renderer is not reachable",
            ));
        }
        let gone_before_answer = || {
            Err(refused(
                CanvasErrorCode::CanvasClosed,
                "the canvas closed before it answered",
            ))
        };
        let outcome = tokio::select! {
            answered = waiting => match answered {
                Ok(outcome) => {
                    if watched {
                        self.call_answered(project_id, mount_seq);
                    }
                    outcome
                }
                Err(_) => gone_before_answer(),
            },
            _ = gone.changed() => gone_before_answer(),
            () = tokio::time::sleep_until(deadline) => {
                self.calls.waiting.lock().remove(&call_id);
                if watched {
                    self.call_timed_out(project_id, mount_seq);
                }
                Err(timed_out(timeout))
            }
        };
        self.calls.waiting.lock().remove(&call_id);
        outcome
    }

    /// A host notice to the renderer on `holder`, on the call channel so it
    /// reaches both transports alike. Nothing waits for the answer: a reply
    /// to it finds no waiting call and is dropped. False when it was not
    /// queued.
    pub(super) fn notify(
        &self,
        holder: &CanvasHolder,
        project_id: &str,
        mount_seq: u64,
        method: &str,
        params: String,
    ) -> bool {
        let call_id = self.calls.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        holder.deliver(CanvasCallFrame::new(
            call_id, project_id, mount_seq, method, params,
        ))
    }

    /// A renderer answered `call_id`. Only the connection the call went to
    /// may answer it; a reply stamped with another `mountSeq` means the mount
    /// changed under the call, which resolves `CANVAS_CLOSED`. Returns
    /// whether a waiting call was resolved.
    pub fn reply(
        &self,
        holder_id: &str,
        call_id: u64,
        mount_seq: u64,
        outcome: CanvasOutcome,
    ) -> ReplyMatch {
        let pending = {
            let mut waiting = self.calls.waiting.lock();
            match waiting.get(&call_id) {
                Some(call) if call.holder_id == holder_id => waiting.remove(&call_id),
                _ => None,
            }
        };
        let Some(call) = pending else {
            return ReplyMatch::NotWaiting;
        };
        if call.mount_seq == mount_seq {
            let _ = call.responder.send(outcome);
            ReplyMatch::Resolved
        } else {
            let _ = call.responder.send(Err(refused(
                CanvasErrorCode::CanvasClosed,
                "the canvas was remounted",
            )));
            ReplyMatch::StaleMount {
                sent_to: call.mount_seq,
            }
        }
    }
}

/// What a renderer reply matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyMatch {
    Resolved,
    /// The call went to `sent_to`; the reply came from another mount.
    StaleMount {
        sent_to: u64,
    },
    NotWaiting,
}
