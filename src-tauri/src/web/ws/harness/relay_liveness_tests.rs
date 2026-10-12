//! AD-19 relay-execution audit repros (AD-16 "Relay execution" cases).
//!
//! Each case asserts the behavior the relay execution rework delivers
//! (per-request tasks, independent watchdog, coordinated shutdown) and is
//! `#[ignore]`d until that slice lands. Run them against today's code with
//! `cargo test --lib -- --ignored audit_` to watch each fail for the stated
//! reason.

use tokio::sync::oneshot;

use super::raw_client::RawWsClient;
use super::*;
use crate::acp::events;
use crate::acp::manager::{FakeAuth, FakeTurn};
use crate::remote::host::{RemoteBindMode, RemoteServerState};

/// One agent text chunk fanned out like the real driver does.
fn emit_chunk(host: &Host) {
    let event = events::MessageChunkEvent {
        agent_id: AgentId(AGENT.to_string()),
        session_id: SessionId(SESSION.to_string()),
        role: events::ChunkRole::Agent,
        content: agent_client_protocol::schema::v1::ContentBlock::Text(
            agent_client_protocol::schema::v1::TextContent::new("chunk"),
        ),
        message_id: None,
    };
    let sinks: Vec<Arc<dyn crate::web::EventSink>> = vec![host.relay.clone()];
    events::fan_out(&sinks, Some(SESSION), events::EVENT_MESSAGE_CHUNK, &event);
}

/// A streaming turn: chunks keep flowing for `total` of (virtual) time.
async fn stream_for(host: &Host, total: Duration) {
    let mut left = total;
    while !left.is_zero() {
        emit_chunk(host);
        let step = left.min(Duration::from_secs(1));
        tokio::time::sleep(step).await;
        left -= step;
    }
}

/// A handler slower than the pong deadline must not get the connection
/// killed. Today the read loop awaits the handler inline, so the peer's
/// pongs (queued behind the pending request) never refresh the watchdog and
/// the relay closes a perfectly healthy connection.
#[tokio::test(start_paused = true)]
#[ignore = "relay execution rework (AD-19): the read loop awaits handlers inline, so a handler slower than PONG_TIMEOUT starves the watchdog"]
async fn audit_long_handler_vs_ping() {
    let host = Host::new(|acp, sessions| {
        acp.install_test_agent_with_scripts(
            AgentId(AGENT.to_string()),
            sessions,
            Vec::new(),
            false,
            vec![FakeAuth::ok_after(Duration::from_secs(120))],
        );
    });
    let mut conn = host.connect();
    conn.authenticate().await;

    // The peer is healthy throughout: its auto-pong answers every ping.
    conn.send_request(
        "slow",
        "authenticate_agent",
        json!({"agentId": AGENT, "methodId": "oauth"}),
    );
    conn.peer.read_for(Duration::from_secs(100)).await;
    assert!(
        !conn.relay_finished().await,
        "a handler slower than the pong deadline (75 s) got a healthy connection killed"
    );

    let reply = conn.peer.wait_reply("slow").await;
    assert_eq!(reply["ok"], true, "{reply}");
}

/// A half-open peer during a streaming turn is reaped by the watchdog within
/// the pong deadline, and a backgrounded one only after the 300 s hold. Today
/// the watchdog lives in the write arm that awaits the (stalled) send, so a
/// peer whose socket stopped draining is never reaped.
#[tokio::test(start_paused = true)]
#[ignore = "relay execution rework (AD-19): the watchdog runs inside the write arm that awaits a send, so a stalled peer blocks its own reaping"]
async fn audit_stale_peer_during_stream_is_reaped_by_the_watchdog() {
    let (release_tx, release_rx) = oneshot::channel();
    let (entered_tx, entered_rx) = oneshot::channel();
    let host = Host::new(|acp, sessions| {
        acp.install_test_agent_with_turns(
            AgentId(AGENT.to_string()),
            sessions,
            vec![FakeTurn::Hold {
                release: release_rx,
                entered: entered_tx,
            }],
            false,
        );
    });

    let mut foreground = host.connect();
    foreground.authenticate().await;
    foreground.subscribe("sub").await;
    let mut backgrounded = host.connect();
    backgrounded.authenticate().await;
    backgrounded.subscribe("sub").await;

    foreground.send_prompt_request("p1", "turn-1");
    entered_rx.await.expect("turn entered the fake agent");
    foreground.peer.wait_event("user_prompt").await;

    // Both peers go half-open mid-stream: no more pongs, and the socket stops
    // draining (a full send buffer towards a dead flow).
    backgrounded.peer.send_text(r#"{"type":"background"}"#);
    Conn::settle().await;
    for conn in [&foreground, &backgrounded] {
        conn.peer.set_auto_pong(false);
        conn.peer.stall_writes();
    }

    // Foreground: reaped within PONG_TIMEOUT (75 s) plus one ping interval.
    stream_for(&host, Duration::from_secs(100)).await;
    assert!(
        foreground.relay_finished().await,
        "a stale foreground peer was not reaped within the pong deadline (100 s)"
    );
    assert!(
        !backgrounded.relay_finished().await,
        "the 300 s background hold must be honored"
    );

    // Background: still held at 200 s, reaped by 300 s + one ping interval.
    stream_for(&host, Duration::from_secs(100)).await;
    assert!(
        !backgrounded.relay_finished().await,
        "the 300 s background hold must be honored"
    );
    stream_for(&host, Duration::from_secs(130)).await;
    assert!(
        backgrounded.relay_finished().await,
        "a stale backgrounded peer was not reaped after the 300 s hold"
    );
    drop(release_tx);
}

/// Stopping the host sends `Close(1001)` to an open socket and the upgraded
/// task is drained by the time `stop()` returns (so `kill_all` / process exit
/// never races a live relay). Today `serve_router` only stops accepting; the
/// upgraded socket keeps being served after the UI says "stopped".
#[tokio::test]
#[ignore = "relay execution rework (AD-19): serve_router threads no shutdown token to upgraded sockets, so stop() leaves them open"]
async fn audit_stop_with_open_socket_sends_close_1001_and_drains() {
    let acp = Arc::new(AcpManager::new(vec![]));
    let pty = crate::web::test_pty_manager();
    let relay = Arc::new(WsRelaySink::new());
    let registry = Arc::new(ProjectRegistry::new());
    let host = RemoteServerState::new();
    let status = host
        .start(
            acp,
            pty,
            relay,
            registry,
            RemoteBindMode::Localhost,
            None,
            None,
            None,
        )
        .await
        .expect("host starts on an OS-assigned port");
    let addr: std::net::SocketAddr = ([127, 0, 0, 1], status.port.expect("bound port")).into();

    let mut client = RawWsClient::connect(addr, "/ws").await;
    let first = client.read_json().await.expect("auth_required event");
    assert_eq!(first["type"], "auth_required", "{first}");

    tokio::time::timeout(Duration::from_secs(10), host.stop())
        .await
        .expect("stop must not hang on an open socket")
        .expect("stop succeeds");

    // The tracker drained before `stop` returned: the close is already on the
    // wire, no further waiting for the host to get around to it.
    let close = client
        .close_code_within(Duration::from_millis(500))
        .await
        .expect("the host stopped but the open socket was never closed");
    assert_eq!(close, Some(1001), "Close(1001 host stopping) expected");
}
