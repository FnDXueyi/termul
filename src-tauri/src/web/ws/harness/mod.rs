//! Host-level fault harness for the `/ws` relay (AD-16, test-only).
//!
//! Drives the unmodified production `run_relay` over an in-memory
//! fault-injecting carrier ([`carrier`]) with a fake agent whose command
//! handling reproduces the real loop (`AcpManager::install_test_agent_with_scripts`:
//! real `DriverState` single-flight, the real turn terminators, a slow /
//! failing `Authenticate`). [`raw_client`] covers the cases that need a real
//! upgraded socket (terminal teardown, host stop).
//!
//! Cases that assert behavior the relay execution rework (AD-19) delivers are
//! `#[ignore]`d with a reason and named `audit_*`; run them with
//! `cargo test --lib -- --ignored audit` to see them fail on pre-rework code.

pub(crate) mod carrier;
pub(crate) mod raw_client;

#[cfg(test)]
mod relay_fault_tests;
#[cfg(test)]
mod relay_liveness_tests;
#[cfg(test)]
mod relay_prompt_tests;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::task::JoinHandle;

use super::*;
use carrier::{pair, Peer};

pub(crate) const AGENT: &str = "agent-h";
pub(crate) const SESSION: &str = "sess-h";

/// A host: the relay sink (also a sink of the ACP manager, as on the server)
/// + the shared `AppState`.
pub(crate) struct Host {
    pub(crate) relay: Arc<WsRelaySink>,
    pub(crate) state: AppState,
}

impl Host {
    /// A host whose single agent (`AGENT`, owning `SESSION`) is installed by
    /// `install`. The session is known to the relay so `subscribe` succeeds.
    pub(crate) fn new(install: impl FnOnce(&Arc<AcpManager>, HashSet<String>)) -> Self {
        let relay = Arc::new(WsRelaySink::new());
        relay.seed_session_for_test(SESSION);
        let acp = Arc::new(AcpManager::new(vec![relay.clone()]));
        install(&acp, HashSet::from([SESSION.to_string()]));
        let pty = crate::web::test_pty_manager();
        let state = AppState {
            acp: Arc::clone(&acp),
            terminal_events: pty.terminal_events(),
            cwd_tracker: pty.cwd_tracker(),
            git_tracker: pty.git_tracker(),
            exit_code_tracker: pty.exit_code_tracker(),
            pty,
            relay: Arc::clone(&relay),
            registry: Arc::new(ProjectRegistry::new()),
            registry_persistence: None,
            projects_file: None,
            history_mode: HistoryMode::LiveOnly,
            workspace_manifest: None,
            acp_catalog: None,
            acp_install: None,
            store: None,
            web_auth: None,
            allow_remote_writes: false,
            shared_live_writes_denied: false,
            project_root: Arc::new(parking_lot::RwLock::new(std::env::temp_dir())),
            pending_oauth_flows: Arc::new(parking_lot::RwLock::new(
                std::collections::HashMap::new(),
            )),
            oauth_base_url: "http://127.0.0.1".to_string(),
        };
        Self { relay, state }
    }

    /// Open a connection: the production relay loop on a fault carrier.
    pub(crate) fn connect(&self) -> Conn {
        let (carrier, peer) = pair();
        let task = tokio::spawn(run_relay(carrier, self.state.clone()));
        Conn { peer, task }
    }
}

/// One relay connection: the peer-side handle plus the relay task.
pub(crate) struct Conn {
    pub(crate) peer: Peer,
    pub(crate) task: JoinHandle<()>,
}

impl Conn {
    /// Run the versioned `authenticate` handshake and require `ok`.
    pub(crate) async fn authenticate(&mut self) {
        let reply = self
            .request(
                "auth",
                "authenticate",
                json!({
                    "protocolVersion": WS_PROTOCOL_VERSION,
                    "deviceId": "device-harness",
                    "clientInstanceId": "tab-harness",
                }),
            )
            .await;
        assert_eq!(reply["ok"], true, "handshake refused: {reply}");
    }

    /// Send a request frame and wait for its reply.
    pub(crate) async fn request(&mut self, id: &str, type_: &str, payload: Value) -> Value {
        self.send_request(id, type_, payload);
        self.peer.wait_reply(id).await
    }

    pub(crate) fn send_request(&self, id: &str, type_: &str, payload: Value) {
        self.peer
            .send_json(&json!({"id": id, "type": type_, "payload": payload}));
    }

    pub(crate) async fn subscribe(&mut self, id: &str) -> Value {
        let reply = self
            .request(id, "subscribe", json!({"sessionId": SESSION}))
            .await;
        assert_eq!(reply["ok"], true, "subscribe refused: {reply}");
        reply
    }

    pub(crate) fn send_prompt_request(&self, id: &str, turn_id: &str) {
        self.send_request(
            id,
            "send_prompt",
            json!({
                "agentId": AGENT,
                "sessionId": SESSION,
                "text": "hello",
                "turnId": turn_id,
            }),
        );
    }

    /// Let every ready task run at the current (virtual) instant.
    pub(crate) async fn settle() {
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
    }

    /// Whether the relay loop (including its cleanup) has returned.
    pub(crate) async fn relay_finished(&self) -> bool {
        Self::settle().await;
        self.task.is_finished()
    }
}
