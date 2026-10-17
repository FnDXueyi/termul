#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The canvas gateway's renderer leg through the real `/ws` dispatch: a
//! `canvas_mount` makes the connection the holder, the router's call lands
//! on the connection's outbound queue as a `canvas_call` frame, and the
//! connection's `canvas_reply` resolves it.

use super::tests::{route, test_env, test_outbound};
use super::*;
use crate::acp::host_mcp::canvas::CanvasErrorCode;
use crate::web::project_registry::ProjectRegistry;

fn request(id: &str, type_: &str, payload: Value) -> String {
    json!({ "id": id, "type": type_, "payload": payload }).to_string()
}

#[tokio::test]
async fn a_mounted_connection_serves_canvas_calls_over_its_queue() {
    let acp = Arc::new(AcpManager::new(vec![]));
    let relay = Arc::new(WsRelaySink::new());
    let registry = Arc::new(ProjectRegistry::new());
    let (tx, mut rx) = test_outbound();
    let env = test_env(&acp, &relay, &registry, &tx, &Arc::default());

    let mounted = route(
        &request(
            "m1",
            "canvas_mount",
            json!({ "projectId": "proj-1", "mountSeq": 2 }),
        ),
        &env,
    )
    .await;
    assert!(mounted.ok);

    let router = acp.canvas_router();
    let call = tokio::spawn(async move {
        router
            .call(
                "proj-1",
                "node.get",
                r#"{"id":18446744073709551615}"#.into(),
            )
            .await
    });
    let frame = match rx.recv().await.expect("a canvas_call frame") {
        Outbound::Canvas(frame) => frame,
        _ => panic!("expected a canvas_call frame"),
    };
    let wire = serde_json::to_value(&frame).unwrap();
    assert_eq!(wire["type"], "canvas_call");
    assert_eq!(wire["projectId"], "proj-1");
    assert_eq!(wire["mountSeq"], 2);
    assert_eq!(wire["params"], r#"{"id":18446744073709551615}"#);

    let replied = route(
        &request(
            "r1",
            "canvas_reply",
            json!({
                "callId": frame.call_id,
                "mountSeq": 2,
                "ok": true,
                "result": r#"{"id":18446744073709551615}"#,
            }),
        ),
        &env,
    )
    .await;
    assert_eq!(replied.payload, Some(json!({ "resolved": true })));
    assert_eq!(
        call.await.unwrap(),
        Ok(r#"{"id":18446744073709551615}"#.to_string())
    );

    let unmounted = route(
        &request(
            "u1",
            "canvas_unmount",
            json!({ "projectId": "proj-1", "mountSeq": 2 }),
        ),
        &env,
    )
    .await;
    assert_eq!(unmounted.payload, Some(json!({ "unmounted": true })));
    let after = acp
        .canvas_router()
        .call("proj-1", "node.get", "{}".into())
        .await;
    assert_eq!(after.unwrap_err().code, CanvasErrorCode::CanvasClosed);
}

#[tokio::test]
async fn another_connection_cannot_answer_or_unmount_a_call_it_does_not_hold() {
    let acp = Arc::new(AcpManager::new(vec![]));
    let relay = Arc::new(WsRelaySink::new());
    let registry = Arc::new(ProjectRegistry::new());
    let (holder_tx, mut holder_rx) = test_outbound();
    let (other_tx, _other_rx) = test_outbound();
    let holder = test_env(&acp, &relay, &registry, &holder_tx, &Arc::default());
    let other = test_env(&acp, &relay, &registry, &other_tx, &Arc::default());

    route(
        &request(
            "m1",
            "canvas_mount",
            json!({ "projectId": "proj-1", "mountSeq": 1 }),
        ),
        &holder,
    )
    .await;
    let router = acp.canvas_router();
    router.set_call_timeout(std::time::Duration::from_millis(200));
    let call =
        tokio::spawn(async move { router.call("proj-1", "document.inspect", "{}".into()).await });
    let Some(Outbound::Canvas(frame)) = holder_rx.recv().await else {
        panic!("expected a canvas_call frame");
    };

    let forged = route(
        &request(
            "r1",
            "canvas_reply",
            json!({ "callId": frame.call_id, "mountSeq": 1, "ok": true, "result": "{}" }),
        ),
        &other,
    )
    .await;
    assert_eq!(forged.payload, Some(json!({ "resolved": false })));
    let unmounted = route(
        &request(
            "u1",
            "canvas_unmount",
            json!({ "projectId": "proj-1", "mountSeq": 1 }),
        ),
        &other,
    )
    .await;
    assert_eq!(unmounted.payload, Some(json!({ "unmounted": false })));
    assert_eq!(
        call.await.unwrap().unwrap_err().code,
        CanvasErrorCode::CanvasTimeout
    );
}

#[tokio::test]
async fn a_malformed_canvas_payload_is_refused() {
    let acp = Arc::new(AcpManager::new(vec![]));
    let relay = Arc::new(WsRelaySink::new());
    let registry = Arc::new(ProjectRegistry::new());
    let (tx, _rx) = test_outbound();
    let env = test_env(&acp, &relay, &registry, &tx, &Arc::default());
    for (type_, payload) in [
        ("canvas_mount", json!({ "projectId": "proj-1" })),
        ("canvas_unmount", json!({ "mountSeq": 1 })),
        ("canvas_reply", json!({ "ok": true })),
    ] {
        let reply = route(&request("x", type_, payload), &env).await;
        assert!(!reply.ok, "{type_} must refuse a malformed payload");
    }
}
