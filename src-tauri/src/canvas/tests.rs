//! Web canvas routes (the `canvas_router` sub-router via tower oneshot):
//! open/close, the `ct` bundle gate, the write predicate and the doc-path
//! boundary. The bundle, token store and persistence sequences have their own
//! test modules.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::Router;
use tower::ServiceExt;

use super::canvas_id_for_project;
use super::persistence::CloseMode;
use super::static_server::close_current_canvas;

/// A temp `.figcraft` document (the web boundary canonicalizes real paths).
fn temp_doc() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("design.figcraft");
    std::fs::write(&path, b"{}").expect("write doc");
    (dir, path.to_string_lossy().to_string())
}

#[test]
fn canvas_id_is_deterministic_and_url_safe() {
    assert_eq!(
        canvas_id_for_project("proj-1"),
        canvas_id_for_project("proj-1")
    );
    assert_ne!(
        canvas_id_for_project("proj-1"),
        canvas_id_for_project("proj-2")
    );
    assert!(canvas_id_for_project("proj-1").starts_with("cv"));
    assert!(canvas_id_for_project("proj-1")
        .chars()
        .all(|c| c.is_ascii_alphanumeric()));
}

/// Deployment flags for the sub-router tests: a plain loopback deployment.
fn loopback_write_flags() -> crate::web::capabilities::WriteFlags {
    crate::web::capabilities::WriteFlags {
        allow_remote_writes: false,
        shared_live_writes_denied: false,
    }
}

/// The loopback peer `ConnectInfo` the canvas write routes require.
fn web_test_peer() -> std::net::SocketAddr {
    std::net::SocketAddr::from(([127, 0, 0, 1], 54321))
}

fn web_canvas_app() -> Router {
    crate::web::canvas_api::canvas_router(crate::web::canvas_api::CanvasState {
        boundary: None,
        write_flags: loopback_write_flags(),
    })
}

/// Sub-router app whose doc-path validation boundary is a registry holding
/// each `(project id, root)`, like the registry-backed production boundary.
fn web_canvas_app_with_boundary(roots: &[(&str, &std::path::Path)]) -> Router {
    crate::web::canvas_api::canvas_router(crate::web::canvas_api::CanvasState {
        boundary: Some(crate::web::canvas_api::CanvasProjectBoundary {
            registry: crate::web::project_registry::ProjectRegistry::with_roots_for_test(roots),
        }),
        write_flags: loopback_write_flags(),
    })
}

/// POST /canvas/open helper returning the parsed IpcBody JSON.
async fn web_open(app: Router, doc: &str, project_id: &str) -> serde_json::Value {
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/canvas/open")
                .extension(axum::extract::ConnectInfo(web_test_peer()))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"docPath": doc, "projectId": project_id, "clientInstanceId": "web-tab", "maxMounted": 16})
                        .to_string(),
                ))
                .expect("build open request"),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    serde_json::from_slice(&body).expect("IpcBody json")
}

async fn get_status(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .expect("build request"),
        )
        .await
        .expect("router responds");
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (
        status,
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn web_open_mints_a_same_origin_bundle_mount() {
    let (dir, doc) = temp_doc();
    let app = web_canvas_app();
    let parsed = web_open(app, &doc, "proj-web-open").await;
    assert_eq!(parsed["success"], true, "{parsed}");
    let data = &parsed["data"];
    assert_eq!(data["canvasId"], canvas_id_for_project("proj-web-open"));
    assert_eq!(data["baseUrl"], "", "the web bundle is same-origin");
    assert_eq!(data["pinTag"], crate::canvas::pin::pinned_tag());
    let token = data["canvasToken"].as_str().expect("canvasToken");
    assert_eq!(token.len(), 32);
    assert!(data["docKey"]
        .as_str()
        .is_some_and(|key| key.ends_with("design.figcraft")));
    close_current_canvas("proj-web-open", CloseMode::Save)
        .await
        .unwrap();
    drop(dir);
}

#[tokio::test]
async fn web_open_reuses_the_token_until_close_revokes_it() {
    let (dir, doc) = temp_doc();
    let app = web_canvas_app();
    let project = "proj-web-reopen";
    let token_of =
        |parsed: &serde_json::Value| parsed["data"]["canvasToken"].as_str().unwrap().to_string();

    let first = token_of(&web_open(app.clone(), &doc, project).await);
    let reopened = web_open(app.clone(), &doc, project).await;
    let second = token_of(&reopened);
    assert_eq!(
        first, second,
        "a repeat open keeps the mounted iframe URL valid"
    );

    let embed = format!(
        "/canvas/{}/{first}/?embed=kiosk",
        canvas_id_for_project(project)
    );
    assert_ne!(
        get_status(app.clone(), &embed).await.0,
        StatusCode::UNAUTHORIZED
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/canvas/close")
                .extension(axum::extract::ConnectInfo(web_test_peer()))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "projectId": project,
                        "mountSeq": reopened["data"]["mountSeq"],
                    })
                    .to_string(),
                ))
                .expect("build close request"),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        get_status(app.clone(), &embed).await.0,
        StatusCode::UNAUTHORIZED,
        "close revokes the canvas token"
    );

    let third = token_of(&web_open(app, &doc, project).await);
    assert_ne!(first, third, "the next open mints a fresh token");
    close_current_canvas(project, CloseMode::Save)
        .await
        .unwrap();
    drop(dir);
}

#[tokio::test]
async fn web_canvas_bundle_requires_a_valid_canvas_token() {
    let (dir, doc) = temp_doc();
    let app = web_canvas_app();
    let project = "proj-web-gate";
    let canvas_id = canvas_id_for_project(project);
    let token = web_open(app.clone(), &doc, project).await["data"]["canvasToken"]
        .as_str()
        .unwrap()
        .to_string();

    for authorized in [
        format!("/canvas/{canvas_id}/{token}/"),
        format!("/canvas/{canvas_id}/{token}/assets/x.js"),
    ] {
        let (status, _) = get_status(app.clone(), &authorized).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{authorized}");
    }

    for uri in [
        format!("/canvas/{canvas_id}"),
        format!("/canvas/{canvas_id}/"),
        format!("/canvas/{canvas_id}/assets/x.js"),
        format!("/canvas/{canvas_id}/{:0<32}/assets/x.js", "0"),
        format!("/canvas/{canvas_id}/assets/x.js?ct={token}"),
    ] {
        let (status, body) = get_status(app.clone(), &uri).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "uri {uri}");
        assert_eq!(body["code"], "UNAUTHORIZED", "uri {uri}");
    }
    close_current_canvas(project, CloseMode::Save)
        .await
        .unwrap();
    drop(dir);
}

#[tokio::test]
async fn web_canvas_routes_trailing_slash_and_catch_all_reach_the_token_gate() {
    // The iframe URL shape is `/canvas/<id>/<token>/` — it must route (via
    // the catch-all) into the token-gated bundle, which rejects a wrong token
    // rather than 404ing.
    let app = web_canvas_app();
    for uri in [
        "/canvas/cv0123456789abcdef/",
        "/canvas/cv0123456789abcdef/0123/",
        "/canvas/cv0123456789abcdef/0123/assets/x.js",
    ] {
        let (status, _) = get_status(app.clone(), uri).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "uri {uri} must reach the canvas token gate, not 404"
        );
    }
}

#[tokio::test]
async fn web_canvas_write_routes_refuse_a_view_only_peer() {
    let (dir, doc) = temp_doc();
    let lan = std::net::SocketAddr::from(([192, 168, 1, 50], 40000));
    let shared_live = crate::web::capabilities::WriteFlags {
        allow_remote_writes: false,
        shared_live_writes_denied: true,
    };
    let cases = [
        (lan, loopback_write_flags()),
        (web_test_peer(), shared_live),
    ];
    for (peer, flags) in cases {
        for uri in ["/canvas/open", "/canvas/close"] {
            let app: Router =
                crate::web::canvas_api::canvas_router(crate::web::canvas_api::CanvasState {
                    boundary: None,
                    write_flags: flags,
                });
            let resp = app
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .extension(axum::extract::ConnectInfo(peer))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({"docPath": doc, "projectId": "p", "mountSeq": 1, "clientInstanceId": "tab"})
                                .to_string(),
                        ))
                        .expect("build request"),
                )
                .await
                .expect("router responds");
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .expect("read body");
            let parsed: serde_json::Value = serde_json::from_slice(&body).expect("IpcBody json");
            assert_eq!(parsed["success"], false, "{uri} from {peer}");
            assert_eq!(parsed["code"], "FORBIDDEN", "{uri} from {peer}");
        }
    }
    drop(dir);
}

#[tokio::test]
async fn web_open_rejects_doc_path_outside_project_boundary() {
    // A web client may only open a doc inside the project boundary (same
    // containment the fs/git routes enforce).
    let inside = tempfile::tempdir().expect("tempdir (boundary root)");
    let outside = tempfile::tempdir().expect("tempdir (outside)");
    let (doc_dir, doc) = temp_doc(); // lives in its own temp dir → outside

    let app = web_canvas_app_with_boundary(&[("proj-7", inside.path())]);
    let parsed = web_open(app, &doc, "proj-7").await;
    assert_eq!(parsed["success"], false);
    assert_eq!(parsed["code"], "PATH_VALIDATION_FAILED");
    assert!(
        parsed["error"]
            .as_str()
            .expect("error message")
            .contains("outside the project root"),
        "message names the containment failure"
    );
    drop(doc_dir);
    drop(outside);
    drop(inside);
}

#[tokio::test]
async fn web_open_accepts_doc_path_inside_project_boundary() {
    let root = tempfile::tempdir().expect("tempdir (boundary root)");
    let doc = root.path().join("design.figcraft");
    std::fs::write(&doc, b"{}").expect("write doc");
    let app = web_canvas_app_with_boundary(&[("proj-7", root.path())]);
    let parsed = web_open(app, &doc.to_string_lossy(), "proj-7").await;
    assert_eq!(parsed["success"], true, "doc inside the boundary opens");
    close_current_canvas("proj-7", CloseMode::Discard)
        .await
        .unwrap();
}

#[tokio::test]
async fn web_open_refuses_another_projects_doc_under_this_projects_id() {
    let a = tempfile::tempdir().expect("tempdir (project a)");
    let b = tempfile::tempdir().expect("tempdir (project b)");
    let b_doc = b.path().join("design.figcraft");
    std::fs::write(&b_doc, b"{}").expect("write doc");
    let app = web_canvas_app_with_boundary(&[("proj-a", a.path()), ("proj-b", b.path())]);

    let parsed = web_open(app.clone(), &b_doc.to_string_lossy(), "proj-a").await;
    assert_eq!(parsed["code"], "PATH_VALIDATION_FAILED", "{parsed}");
    let parsed = web_open(app, &b_doc.to_string_lossy(), "proj-b").await;
    assert_eq!(parsed["success"], true, "{parsed}");
    close_current_canvas("proj-b", CloseMode::Discard)
        .await
        .unwrap();
}

/// The Windows project registry stores `\\?\D:\…` roots, and the renderer
/// echoes them back joined with `/` or with every `\` turned into `/`.
#[cfg(windows)]
#[tokio::test]
async fn web_open_keys_a_verbatim_root_doc_by_its_plain_path() {
    let root = tempfile::tempdir().expect("tempdir (boundary root)");
    std::fs::write(root.path().join("design.figcraft"), b"{}").expect("write doc");
    let verbatim = root
        .path()
        .canonicalize()
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned();
    let plain = verbatim.trim_start_matches(r"\\?\").to_string();
    let forward = verbatim.replace('\\', "/");
    let projects: Vec<String> = (0..6).map(|n| format!("proj-verbatim-{n}")).collect();
    let roots: Vec<(&str, &std::path::Path)> = projects
        .iter()
        .map(|id| (id.as_str(), std::path::Path::new(&verbatim)))
        .collect();
    let app = web_canvas_app_with_boundary(&roots);
    for (index, name) in ["design.figcraft", "new.figcraft"].into_iter().enumerate() {
        let expected = format!(r"{plain}\{name}");
        for (form, doc) in [
            format!(r"{verbatim}\{name}"),
            format!("{verbatim}/{name}"),
            format!("{forward}/{name}"),
        ]
        .into_iter()
        .enumerate()
        {
            let project = &projects[index * 3 + form];
            let parsed = web_open(app.clone(), &doc, project).await;
            assert_eq!(
                parsed["data"]["docKey"],
                expected.as_str(),
                "{doc}: {parsed}"
            );
            close_current_canvas(project, CloseMode::Discard)
                .await
                .unwrap();
        }
    }
}
