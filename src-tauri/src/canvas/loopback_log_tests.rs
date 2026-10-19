//! The desktop loopback listener logs through `log`: neither a rejected nor
//! a served bundle request may put the canvas token in a log line.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use super::static_server::{bundle_router, bundle_tokens};
use crate::web::log_capture::logged_lines;

async fn status_of(path: &str) -> StatusCode {
    bundle_router()
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response")
        .status()
}

#[tokio::test]
async fn no_loopback_request_logs_the_canvas_token() {
    let _ = logged_lines();
    let id = "cv_loopback_log";
    let token = bundle_tokens().issue(id).unwrap();
    let wrong = "ffffffffffffffffffffffffffffffff";

    let rejected = status_of(&format!("/canvas/{id}/{wrong}/app.js")).await;
    let accepted = status_of(&format!("/canvas/{id}/{token}")).await;

    assert_eq!(rejected, StatusCode::UNAUTHORIZED);
    assert_eq!(accepted, StatusCode::PERMANENT_REDIRECT);
    let lines = logged_lines();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("bundle request rejected")),
        "the rejected request was not logged: {lines:?}"
    );
    for line in &lines {
        assert!(!line.contains(&token), "{line}");
        assert!(!line.contains(wrong), "{line}");
    }
}
