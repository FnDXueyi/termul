use super::*;
use tower::ServiceExt;

fn test_state() -> AppState {
    AppState {
        store: Arc::new(SqliteStore::open_in_memory().unwrap()),
    }
}

#[tokio::test]
async fn test_create_session_route() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/sessions")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"url":"https://example.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_list_sessions_route() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/sessions")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_annotation_lifecycle() {
    let state = test_state();
    let app = router(state.clone());

    // Create session
    let resp = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/sessions")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"url":"https://test.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let session: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let session_id = session["id"].as_str().unwrap();

    // Add annotation
    let app2 = router(state.clone());
    let resp = app2
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/sessions/{session_id}/annotations"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"x":10,"y":20,"comment":"Fix this","element":"button","elementPath":"body > button","timestamp":12345}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Get pending
    let app3 = router(state.clone());
    let resp = app3
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/pending"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pending["count"], 1);
}

/// Raw HTTP/1.1 GET over a real socket so the `Host` header is exactly the
/// one a rebinding page would send.
async fn raw_status(addr: SocketAddr, host: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!("GET /sessions HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response.lines().next().unwrap_or_default().to_string()
}

#[tokio::test]
async fn listener_rejects_foreign_hosts_and_owns_its_port_while_serving() {
    let (addr, shutdown) = start_server(Arc::new(SqliteStore::open_in_memory().unwrap()))
        .await
        .unwrap();
    assert!(crate::host_perimeter::is_owned_port(addr.port()));

    assert_eq!(raw_status(addr, "attacker.example").await, "HTTP/1.1 403 Forbidden");
    assert_eq!(
        raw_status(addr, &format!("attacker.example:{}", addr.port())).await,
        "HTTP/1.1 403 Forbidden"
    );
    assert_eq!(raw_status(addr, &addr.to_string()).await, "HTTP/1.1 200 OK");
    assert_eq!(
        raw_status(addr, &format!("localhost:{}", addr.port())).await,
        "HTTP/1.1 200 OK"
    );

    shutdown.cancel();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while crate::host_perimeter::is_owned_port(addr.port()) {
        assert!(std::time::Instant::now() < deadline, "port released after shutdown");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
