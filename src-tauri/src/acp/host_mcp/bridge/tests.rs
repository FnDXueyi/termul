//! The whole gateway path: `Bridge::call` in the child → the parent's TCP
//! listener → session → project resolution → the canvas router → a fake
//! renderer → back.

use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use figcraft_mcp::Backend;
use serde_json::{json, Value};

use super::*;
use crate::acp::host_mcp::canvas::{CanvasCallFrame, CanvasHolder, CanvasRouter};
use crate::acp::host_mcp::parent::HostPlanServer;
use crate::acp::session_persistence::SessionPersistence;

struct Harness {
    runtime: tokio::runtime::Runtime,
    root: std::path::PathBuf,
    persistence: Arc<SessionPersistence>,
    server: Arc<HostPlanServer>,
}

impl Harness {
    /// A parent with `sessions` (`(session id, project id)`) registered,
    /// bound and in an active turn.
    fn new(sessions: &[(&str, Option<&str>)]) -> (Self, Vec<ChildConfig>) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let root =
            std::env::temp_dir().join(format!("termul-canvas-bridge-{}", uuid::Uuid::new_v4()));
        let cwd = root.join("cwd");
        std::fs::create_dir_all(&cwd).unwrap();
        let persistence = runtime.block_on(async {
            let persistence = SessionPersistence::open(root.join("store")).await.unwrap();
            for (session_id, project_id) in sessions {
                persistence
                    .register_session(crate::acp::SessionRegistration {
                        session_id: (*session_id).into(),
                        stable_agent_namespace: Some("config:test".into()),
                        runtime_agent_id: Some("agent-1".into()),
                        project_id: project_id.map(str::to_string),
                        cwd: cwd.clone(),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
            }
            persistence
        });
        let server = HostPlanServer::start(vec![], Some(Arc::clone(&persistence)));
        let configs = sessions
            .iter()
            .enumerate()
            .map(|(index, (session_id, _))| {
                let agent_id = format!("agent-{index}");
                let (port, token, provisional) = server.register_session(&agent_id);
                server.bind_session(&token, session_id);
                server.begin_turn(&agent_id, session_id);
                ChildConfig {
                    port,
                    token,
                    session_id: provisional,
                    agent_id,
                }
            })
            .collect();
        (
            Self {
                runtime,
                root,
                persistence,
                server,
            },
            configs,
        )
    }

    fn router(&self) -> Arc<CanvasRouter> {
        self.server.canvas_router()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let persistence = Arc::clone(&self.persistence);
        let _ = self.runtime.block_on(persistence.shutdown());
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A renderer that answers each call with `answer(frame)` (`None` = never).
fn mount_renderer(
    router: &Arc<CanvasRouter>,
    holder_id: &'static str,
    project_id: &str,
    mount_seq: u64,
    answer: impl Fn(&CanvasCallFrame) -> Option<Result<String, CanvasError>> + Send + 'static,
) -> mpsc::Receiver<CanvasCallFrame> {
    let (frames_tx, frames_rx) = mpsc::channel::<CanvasCallFrame>();
    let (seen_tx, seen_rx) = mpsc::channel::<CanvasCallFrame>();
    router.mount(
        project_id,
        mount_seq,
        CanvasHolder::new(
            holder_id,
            Arc::new(move |frame| frames_tx.send(frame).is_ok()),
        ),
    );
    let router = Arc::downgrade(router);
    std::thread::spawn(move || {
        for frame in frames_rx {
            let reply = answer(&frame);
            let _ = seen_tx.send(frame.clone());
            if let (Some(outcome), Some(router)) = (reply, router.upgrade()) {
                router.reply(holder_id, frame.call_id, frame.mount_seq, outcome);
            }
        }
    });
    seen_rx
}

fn error_of(result: Result<Value, String>) -> Value {
    serde_json::from_str(&result.expect_err("the call must fail")).expect("Err(String) is JSON")
}

#[test]
fn a_bridge_call_round_trips_through_the_router_with_u64_ids_intact() {
    let (harness, configs) = Harness::new(&[("sess-1", Some("proj-1"))]);
    let seen = mount_renderer(&harness.router(), "conn-a", "proj-1", 1, |frame| {
        assert!(
            frame.params.contains("18446744073709551615"),
            "params forwarded as text"
        );
        Some(Ok(
            r#"{"id":18446744073709551615,"type":"FRAME"}"#.to_string()
        ))
    });

    let mut bridge = Bridge::new(configs[0].clone());
    let result = bridge
        .call("node.get", json!({ "id": u64::MAX, "depth": 1 }))
        .unwrap();
    assert_eq!(result["id"].as_u64(), Some(u64::MAX));
    assert_eq!(result["type"], "FRAME");
    let frame = seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(frame.project_id, "proj-1");
    assert_eq!(frame.method, "node.get");
}

#[test]
fn figcraft_tools_drive_the_bridge_as_their_backend() {
    let (harness, configs) = Harness::new(&[("sess-1", Some("proj-1"))]);
    let _seen = mount_renderer(&harness.router(), "conn-a", "proj-1", 1, |frame| {
        assert_eq!(frame.method, "node.get");
        Some(Ok(r#"{"id":4294967297,"name":"Card"}"#.to_string()))
    });
    let mut bridge = Bridge::new(configs[0].clone());
    let result =
        figcraft_mcp::call_tool(&mut bridge, "get_node", &json!({ "id": 4_294_967_297u64 }));
    assert!(!result.is_error, "{:?}", result.content);
    assert!(result.content[0]["text"].as_str().unwrap().contains("Card"));
}

#[test]
fn each_session_reaches_only_its_own_projects_canvas() {
    let (harness, configs) =
        Harness::new(&[("sess-1", Some("proj-1")), ("sess-2", Some("proj-2"))]);
    let router = harness.router();
    let _one = mount_renderer(&router, "conn-a", "proj-1", 1, |_| {
        Some(Ok(r#""one""#.into()))
    });
    let _two = mount_renderer(&router, "conn-b", "proj-2", 7, |_| {
        Some(Ok(r#""two""#.into()))
    });

    let mut first = Bridge::new(configs[0].clone());
    let mut second = Bridge::new(configs[1].clone());
    assert_eq!(
        first.call("document.inspect", json!({})).unwrap(),
        json!("one")
    );
    assert_eq!(
        second.call("document.inspect", json!({})).unwrap(),
        json!("two")
    );
}

#[test]
fn no_mount_is_a_typed_canvas_closed() {
    let (_harness, configs) = Harness::new(&[("sess-1", Some("proj-1"))]);
    let mut bridge = Bridge::new(configs[0].clone());
    let error = error_of(bridge.call("document.inspect", json!({})));
    assert_eq!(error["code"], "CANVAS_CLOSED");
    assert!(error["message"].is_string());
}

#[test]
fn a_session_without_a_project_is_canvas_closed() {
    let (_harness, configs) = Harness::new(&[("sess-1", None)]);
    let mut bridge = Bridge::new(configs[0].clone());
    assert_eq!(
        error_of(bridge.call("document.inspect", json!({})))["code"],
        "CANVAS_CLOSED"
    );
}

#[test]
fn a_silent_renderer_times_out_in_the_gateway() {
    let (harness, configs) = Harness::new(&[("sess-1", Some("proj-1"))]);
    let router = harness.router();
    router.set_call_timeout(Duration::from_millis(150));
    let _seen = mount_renderer(&router, "conn-a", "proj-1", 1, |_| None);
    let mut bridge = Bridge::new(configs[0].clone());
    assert_eq!(
        error_of(bridge.call("document.inspect", json!({})))["code"],
        "CANVAS_TIMEOUT"
    );
}

#[test]
fn a_remount_while_a_call_waits_is_canvas_closed() {
    let (harness, configs) = Harness::new(&[("sess-1", Some("proj-1"))]);
    let router = harness.router();
    let seen = mount_renderer(&router, "conn-a", "proj-1", 1, |_| None);
    let config = configs[0].clone();
    let call = std::thread::spawn(move || Bridge::new(config).call("document.inspect", json!({})));
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    let _next = mount_renderer(&router, "conn-a", "proj-1", 2, |_| None);
    assert_eq!(error_of(call.join().unwrap())["code"], "CANVAS_CLOSED");
}

#[test]
fn renderer_errors_reach_the_backend_as_typed_json() {
    let (harness, configs) = Harness::new(&[("sess-1", Some("proj-1"))]);
    let _seen = mount_renderer(&harness.router(), "conn-a", "proj-1", 1, |_| {
        Some(Err(CanvasError::new(
            CanvasErrorCode::CanvasHidden,
            "the canvas tab is hidden",
        )))
    });
    let mut bridge = Bridge::new(configs[0].clone());
    let error = error_of(bridge.call("ui.screenshot", json!({})));
    assert_eq!(
        error,
        json!({"code": "CANVAS_HIDDEN", "message": "the canvas tab is hidden"})
    );
}

#[test]
fn render_png_asks_host_render_png_and_decodes_its_base64() {
    let png = b"\x89PNG\r\n\x1a\nrest";
    let encoded = base64::engine::general_purpose::STANDARD.encode(png);
    let (harness, configs) = Harness::new(&[("sess-1", Some("proj-1"))]);
    let seen = mount_renderer(&harness.router(), "conn-a", "proj-1", 1, move |_| {
        Some(Ok(Value::String(encoded.clone()).to_string()))
    });
    let mut bridge = Bridge::new(configs[0].clone());
    assert_eq!(bridge.render_png(&[7], 2.0).unwrap(), png.to_vec());
    let frame = seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(frame.method, "host.renderPng");
    let params: Value = serde_json::from_str(&frame.params).unwrap();
    assert_eq!(params, json!({"scale": 2.0, "ids": [7]}));
}
