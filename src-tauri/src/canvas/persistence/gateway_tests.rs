//! The mount registry's connection half: which renderer runs a mount.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::{CanvasPersistence, ReplyMatch, SliceMatch};
use crate::acp::host_mcp::canvas::{CanvasErrorCode, CanvasHolder};

fn holder(id: &'static str) -> CanvasHolder {
    CanvasHolder::new(
        id,
        crate::canvas::persistence::test_renderer(),
        Arc::new(|_| true),
    )
}

#[tokio::test]
async fn only_the_live_mount_seq_can_be_attached() {
    let mounts = CanvasPersistence::default();
    assert!(!mounts.attach("p", 1, holder("a")), "no mount");
    let seq = mounts.mount("p", PathBuf::from("p.figcraft"), Duration::from_secs(1));
    assert!(
        !mounts.attach("p", seq + 1, holder("a")),
        "unknown mountSeq"
    );
    assert!(mounts.attach("p", seq, holder("a")));
    assert_eq!(mounts.holder_of("p"), Some((seq, "a".to_string())));
}

#[tokio::test]
async fn detaching_keeps_the_mount_but_routes_nothing() {
    let mounts = CanvasPersistence::default();
    let seq = mounts.mount("p", PathBuf::from("p.figcraft"), Duration::from_secs(1));
    assert!(mounts.attach("p", seq, holder("a")));
    assert!(!mounts.detach("p", "b", seq), "another connection");
    assert!(mounts.detach("p", "a", seq));
    assert_eq!(mounts.mounted_projects(), vec!["p".to_string()]);
    assert_eq!(mounts.holder_of("p"), None);
    let outcome = mounts
        .call("p", "node.get", "{}".into(), Duration::from_secs(1))
        .await;
    assert_eq!(outcome.unwrap_err().code, CanvasErrorCode::CanvasClosed);
    assert!(
        mounts.attach("p", seq, holder("b")),
        "a reconnect re-attaches"
    );
}

#[tokio::test]
async fn a_new_mount_seq_drops_the_old_connection() {
    let mounts = CanvasPersistence::default();
    let first = mounts.mount("p", PathBuf::from("p.figcraft"), Duration::from_secs(1));
    assert!(mounts.attach("p", first, holder("a")));
    let second = mounts.mount("p", PathBuf::from("p.figcraft"), Duration::from_secs(1));
    assert_eq!(mounts.holder_of("p"), None);
    assert!(
        !mounts.attach("p", first, holder("a")),
        "the old mountSeq is gone"
    );
    assert!(mounts.attach("p", second, holder("a")));
}

struct Call {
    mounts: Arc<CanvasPersistence>,
    call_id: u64,
    outcome: tokio::task::JoinHandle<crate::acp::host_mcp::canvas::CanvasOutcome>,
}

/// A call to the mount `p` held by connection `a`, waiting for its reply.
async fn waiting_call(mounts: Arc<CanvasPersistence>) -> Call {
    let (frames, mut delivered) = tokio::sync::mpsc::unbounded_channel();
    let seq = mounts.mount("p", PathBuf::from("p.figcraft"), Duration::from_secs(1));
    let holder = CanvasHolder::new(
        "a",
        crate::canvas::persistence::test_renderer(),
        Arc::new(move |frame| frames.send(frame).is_ok()),
    );
    assert!(mounts.attach("p", seq, holder));
    let outcome = tokio::spawn({
        let mounts = Arc::clone(&mounts);
        async move {
            mounts
                .call("p", "node.get", "{}".into(), Duration::from_secs(30))
                .await
        }
    });
    let call_id = delivered
        .recv()
        .await
        .expect("the call was delivered")
        .call_id;
    Call {
        mounts,
        call_id,
        outcome,
    }
}

#[tokio::test]
async fn slices_of_a_reply_are_joined_when_the_last_one_arrives() {
    let call = waiting_call(Arc::new(CanvasPersistence::default())).await;
    let mounts = &call.mounts;
    let seq = mounts.holder_of("p").expect("held").0;

    assert_eq!(
        mounts.push_reply_slice("a", call.call_id, "{\"pad\":\""),
        SliceMatch::Buffered
    );
    assert_eq!(
        mounts.push_reply_slice("a", call.call_id, "héllo wörld"),
        SliceMatch::Buffered
    );
    let last = mounts.reply("a", call.call_id, seq, Ok("\"}".to_string()));

    assert_eq!(last, ReplyMatch::Resolved);
    assert_eq!(
        call.outcome.await.unwrap(),
        Ok("{\"pad\":\"héllo wörld\"}".to_string())
    );
}

#[tokio::test]
async fn a_slice_from_another_connection_is_dropped() {
    let call = waiting_call(Arc::new(CanvasPersistence::default())).await;
    let mounts = &call.mounts;
    let seq = mounts.holder_of("p").expect("held").0;

    assert_eq!(
        mounts.push_reply_slice("b", call.call_id, "forged"),
        SliceMatch::NotWaiting
    );
    mounts.reply("a", call.call_id, seq, Ok("real".to_string()));

    assert_eq!(call.outcome.await.unwrap(), Ok("real".to_string()));
}

#[tokio::test]
async fn a_reply_past_the_limit_is_an_engine_error_and_ends_the_call() {
    let call = waiting_call(Arc::new(CanvasPersistence::default())).await;
    let mounts = &call.mounts;
    let seq = mounts.holder_of("p").expect("held").0;
    mounts.limit_replies_to(10);

    assert_eq!(
        mounts.push_reply_slice("a", call.call_id, "123456"),
        SliceMatch::Buffered
    );
    assert_eq!(
        mounts.push_reply_slice("a", call.call_id, "789012"),
        SliceMatch::TooLarge
    );

    assert_eq!(
        call.outcome.await.unwrap().unwrap_err().code,
        CanvasErrorCode::EngineError
    );
    assert_eq!(
        mounts.reply("a", call.call_id, seq, Ok("x".into())),
        ReplyMatch::NotWaiting
    );
    assert_eq!(
        mounts.push_reply_slice("a", call.call_id, "x"),
        SliceMatch::NotWaiting
    );
}

#[tokio::test]
async fn a_last_slice_that_crosses_the_limit_is_an_engine_error() {
    let call = waiting_call(Arc::new(CanvasPersistence::default())).await;
    let mounts = &call.mounts;
    let seq = mounts.holder_of("p").expect("held").0;
    mounts.limit_replies_to(10);

    assert_eq!(
        mounts.push_reply_slice("a", call.call_id, "123456"),
        SliceMatch::Buffered
    );
    mounts.reply("a", call.call_id, seq, Ok("789012".to_string()));

    assert_eq!(
        call.outcome.await.unwrap().unwrap_err().code,
        CanvasErrorCode::EngineError
    );
}
