use super::*;
#[test]
fn extract_quoted() {
    let h = r#"Bearer resource_metadata="https://x.test/.well-known/oauth-protected-resource""#;
    assert_eq!(
        extract_resource_metadata_url(h),
        Some("https://x.test/.well-known/oauth-protected-resource".into())
    );
}
#[test]
fn extract_missing() {
    assert!(extract_resource_metadata_url("Bearer realm=\"t\"").is_none());
}
#[test]
fn auth_required() {
    assert!(is_auth_required(r#"Bearer resource_metadata="https://x""#));
    assert!(is_auth_required(r#"Bearer error="invalid_token""#));
    assert!(!is_auth_required("Basic realm=\"t\""));
    assert!(!is_auth_required(""));
}
#[test]
fn key_normalizes() {
    assert_eq!(
        keychain_key("https://x/mcp"),
        keychain_key("https://x/mcp/")
    );
}
#[test]
fn expiry() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(is_token_expired(&StoredToken {
        access_token: "x".into(),
        refresh_token: None,
        expires_at: Some(now - 1),
        client_id: "c".into(),
        issuer: "i".into(),
        server_url: "u".into()
    }));
    assert!(!is_token_expired(&StoredToken {
        access_token: "x".into(),
        refresh_token: None,
        expires_at: Some(now + 3600),
        client_id: "c".into(),
        issuer: "i".into(),
        server_url: "u".into()
    }));
    assert!(!is_token_expired(&StoredToken {
        access_token: "x".into(),
        refresh_token: None,
        expires_at: None,
        client_id: "c".into(),
        issuer: "i".into(),
        server_url: "u".into()
    }));
}

#[test]
fn token_file_path_normalizes_trailing_slash() {
    // Equivalent URLs (differing only by a trailing slash) MUST produce
    // the same token file path so a probe and a connect that differ only
    // by the trailing slash reuse the same stored token.
    let a = token_file_path("https://x/mcp").unwrap();
    let b = token_file_path("https://x/mcp/").unwrap();
    assert_eq!(a, b, "trailing slash must normalize to the same file");
}

#[test]
fn expired_token_with_refresh_is_treatable_as_expired() {
    // A token that is already expired and has a refresh_token must be
    // reported as expired so `get_valid_token` attempts a refresh. This
    // is the precondition for the refresh path being reachable at all.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expired = StoredToken {
        access_token: "x".into(),
        refresh_token: Some("r".into()),
        expires_at: Some(now - 1),
        client_id: "c".into(),
        issuer: "i".into(),
        server_url: "u".into(),
    };
    assert!(is_token_expired(&expired));
}

/// Issue #1364: two concurrent `get_valid_token` calls must coalesce into
/// one POST at the token endpoint, and both callers must receive the
/// refreshed token. Without the single-flight lock, the second caller
/// refreshes with the already-rotated (single-use) refresh token, gets
/// `invalid_grant`, and deletes the first caller's stored token.
#[tokio::test]
async fn concurrent_refreshes_join_into_one_token_request() {
    let (url, requests) = serve_token_endpoint(serde_json::json!({
        "access_token": "fresh-access",
        "token_type": "Bearer",
        "expires_in": 3600,
    }))
    .await;
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();

    let a = get_valid_token(&url);
    let b = get_valid_token(&url);
    let (a, b) = tokio::join!(a, b);

    let stored = load_stored_token(&url).unwrap();
    delete_stored_token(&url).unwrap();

    assert_eq!(a.unwrap().as_deref(), Some("fresh-access"));
    assert_eq!(b.unwrap().as_deref(), Some("fresh-access"));
    let stored = stored.expect("the refreshed token is stored");
    assert_eq!(stored.access_token, "fresh-access");
    assert_eq!(requests.lock().len(), 1, "exactly one POST /token");
}

/// Issue #1364: a transient failure (connection error — server unreachable)
/// during refresh must keep the stored token so a later chat can retry,
/// instead of deleting it and forcing a new sign-in.
#[tokio::test]
async fn connection_error_during_refresh_keeps_the_stored_token() {
    // Port 1 is reserved and closed on every platform: connecting fails fast.
    let url = "http://127.0.0.1:1/mcp".to_string();
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();
    let generation = token_generation(&url);

    let token = get_valid_token(&url).await;

    // Assert before the cleanup delete: delete_stored_token itself bumps
    // the generation counter.
    assert_eq!(token.unwrap(), None);
    assert_eq!(token_generation(&url), generation, "no delete happened");
    let stored = load_stored_token(&url).unwrap();
    delete_stored_token(&url).unwrap();

    // No token is delivered (the chat continues without a bearer), but the
    // stored refresh token survives the failure.
    let stored = stored.expect("the stored token survives a network failure");
    assert_eq!(stored.refresh_token.as_deref(), Some("stored-refresh"));
}

/// Issue #1364: when the auth server definitively rejects the refresh token
/// (HTTP 400 `invalid_grant`), the stored token is deleted so the user is
/// re-prompted instead of retrying a dead refresh token forever.
#[tokio::test]
async fn rejected_refresh_token_deletes_the_stored_token() {
    let app = axum::Router::new().route(
        "/token",
        axum::routing::post(|| async {
            axum::response::Response::builder()
                .status(axum::http::StatusCode::BAD_REQUEST)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": "refresh token invalid",
                    })
                    .to_string(),
                ))
                .unwrap()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();
    let generation = token_generation(&url);

    let token = get_valid_token(&url).await;

    assert_eq!(token.unwrap(), None);
    assert_eq!(token_generation(&url), generation + 1, "the token was deleted");
    assert!(load_stored_token(&url).unwrap().is_none());
}

/// An MCP server that publishes no OAuth metadata, so a client falls back to
/// its legacy `/token` endpoint. Answers every token request with `body` and
/// records the form bodies it received.
pub(crate) async fn serve_token_endpoint(
    body: serde_json::Value,
) -> (String, std::sync::Arc<Mutex<Vec<String>>>) {
    let requests = std::sync::Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let app = axum::Router::new().route(
        "/token",
        axum::routing::post(move |form: String| {
            let seen = seen.clone();
            let body = body.clone();
            async move {
                seen.lock().push(form);
                axum::Json(body)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (url, requests)
}

pub(crate) fn expired_token_with_refresh(url: &str) -> StoredToken {
    StoredToken {
        access_token: "stale-access".into(),
        refresh_token: Some("stored-refresh".into()),
        expires_at: Some(1),
        client_id: "termul-client".into(),
        issuer: url.into(),
        server_url: url.into(),
    }
}

/// Issue #1345: an expired token that has a refresh token is refreshed at the
/// token endpoint and stored, not deleted.
#[tokio::test]
async fn expired_token_with_refresh_token_is_refreshed() {
    let (url, requests) = serve_token_endpoint(serde_json::json!({
        "access_token": "fresh-access",
        "token_type": "Bearer",
        "expires_in": 3600,
    }))
    .await;
    store_token(&url, &expired_token_with_refresh(&url)).unwrap();
    let generation = token_generation(&url);

    let token = get_valid_token(&url).await;
    let stored = load_stored_token(&url).unwrap();
    let refreshed_generation = token_generation(&url);
    delete_stored_token(&url).unwrap();

    assert_eq!(token.unwrap().as_deref(), Some("fresh-access"));
    assert_eq!(refreshed_generation, generation + 1);
    let stored = stored.expect("the refreshed token is stored");
    assert_eq!(stored.access_token, "fresh-access");
    assert_eq!(stored.refresh_token.as_deref(), Some("stored-refresh"));
    let requests = requests.lock();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].contains("grant_type=refresh_token")
            && requests[0].contains("refresh_token=stored-refresh"),
        "{}",
        requests[0]
    );
}
