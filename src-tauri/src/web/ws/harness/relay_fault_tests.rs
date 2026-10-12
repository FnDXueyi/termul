//! Wire-fault cases the relay already handles today: delayed and stalled
//! writes, a silent or late-ponging peer, clean and abrupt peer closes, and
//! the binary-frame protocol error. They run in plain `cargo test` so every
//! carrier fault stays exercised in CI.

use super::*;

fn plain_host() -> Host {
    Host::new(|acp, sessions| {
        acp.install_test_agent_with_turns(AgentId(AGENT.to_string()), sessions, Vec::new(), false);
    })
}

/// A slow network delays every reply but loses none: the handshake still
/// completes and each reply arrives no earlier than the injected delay.
#[tokio::test(start_paused = true)]
async fn delayed_wire_delivers_replies_late_but_intact() {
    let host = plain_host();
    let mut conn = host.connect();
    conn.peer.delay_outbound(Duration::from_secs(10));
    let started = tokio::time::Instant::now();
    conn.authenticate().await;
    assert!(
        started.elapsed() >= Duration::from_secs(10),
        "the reply was delivered ahead of the injected delay"
    );
    conn.subscribe("sub").await;
}

/// A peer whose socket stops draining blocks the relay's writer; once it
/// drains again the queued reply is delivered (nothing was dropped).
#[tokio::test(start_paused = true)]
async fn stalled_writes_resume_and_deliver_the_queued_reply() {
    let host = plain_host();
    let mut conn = host.connect();
    conn.authenticate().await;

    conn.peer.stall_writes();
    conn.send_request("sub", "subscribe", json!({"sessionId": SESSION}));
    let blocked = tokio::time::timeout(Duration::from_secs(5), conn.peer.wait_reply("sub")).await;
    assert!(blocked.is_err(), "a stalled sink must not deliver");

    conn.peer.resume_writes();
    let reply = conn.peer.wait_reply("sub").await;
    assert_eq!(reply["ok"], true, "{reply}");
}

/// Today's watchdog (idle write arm): a silent half-open peer is reaped within
/// the pong deadline plus one ping interval, and a pong arriving late but
/// inside the deadline extends the connection's life by exactly that much.
#[tokio::test(start_paused = true)]
async fn silent_peer_is_reaped_and_a_late_pong_extends_the_deadline() {
    let host = plain_host();
    let mut silent = host.connect();
    silent.authenticate().await;
    silent.peer.set_auto_pong(false);
    let mut late = host.connect();
    late.authenticate().await;
    late.peer.set_auto_pong(false);
    late.peer
        .send_message_after(Duration::from_secs(60), Message::Pong(Vec::new().into()));

    silent.peer.read_for(Duration::from_secs(100)).await;
    assert!(
        silent.relay_finished().await,
        "silent peer not reaped in 100 s"
    );
    assert!(!late.relay_finished().await, "the late pong was ignored");

    late.peer.read_for(Duration::from_secs(70)).await;
    assert!(
        late.relay_finished().await,
        "late-pong peer not reaped by 170 s"
    );
}

/// A clean `Close` and an abrupt reset both end the relay task (and release
/// its subscription) with no panic.
#[tokio::test(start_paused = true)]
async fn clean_close_and_reset_both_end_the_relay() {
    let host = plain_host();
    let mut closing = host.connect();
    closing.authenticate().await;
    closing.subscribe("sub").await;
    closing.peer.send_close(1000, "bye");
    tokio::time::timeout(Duration::from_secs(5), closing.task)
        .await
        .expect("relay ends on a clean close")
        .expect("relay task did not panic");

    let mut reset = host.connect();
    reset.authenticate().await;
    reset.subscribe("sub").await;
    reset.peer.reset();
    tokio::time::timeout(Duration::from_secs(5), reset.task)
        .await
        .expect("relay ends on a reset")
        .expect("relay task did not panic");
}

/// A binary frame is a protocol error: the structured reply is flushed, then
/// the relay sends `Close(1003)`.
#[tokio::test(start_paused = true)]
async fn binary_frame_is_answered_then_closed_with_1003() {
    let host = plain_host();
    let mut conn = host.connect();
    conn.authenticate().await;
    conn.peer
        .send_message(Message::Binary(vec![1, 2, 3].into()));
    let (code, _reason) = conn.peer.wait_close().await.expect("close frame");
    assert_eq!(code, 1003);
}
