//! MCP OAuth 2.1 authorization flow (RFC 9728 + PKCE + dynamic registration).
//!
//! When an HTTP/SSE MCP server returns 401 with WWW-Authenticate: Bearer
//! resource_metadata="...", the rmcp client surfaces AuthRequiredError.
//! This module detects that, runs the full OAuth flow, and persists tokens
//! as JSON files in the app data directory (the Windows Credential Manager
//! has a 2560-char limit that OAuth JWTs routinely exceed).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;

use parking_lot::Mutex;
use rmcp::transport::auth::{
    AuthorizationCallback, AuthorizationManager, AuthorizationRequest, AuthorizationSession,
    CredentialStore, InMemoryCredentialStore, OAuthState, OAuthTokenResponse, StoredCredentials,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const OAUTH_REDIRECT_PATH: &str = "/callback";
pub const OAUTH_FLOW_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone)]
pub struct PendingOAuthFlow {
    pub server_url: String,
    pub redirect_uri: String,
    pub csrf_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub client_id: String,
    pub issuer: String,
    pub server_url: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum McpOAuthError {
    NotRequired,
    NoAuthorizationServers,
    DiscoveryFailed(String),
    RegistrationFailed(String),
    AuthUrlFailed(String),
    TokenExchangeFailed(String),
    /// The authorization server definitively rejected the refresh token
    /// (`invalid_grant`): re-authentication is required.
    TokenRefreshRejected(String),
    /// The server or stored credentials require a fresh authorization.
    AuthorizationRequired(String),
    Timeout,
    Keychain(String),
    Http(String),
    InvalidConfig(String),
}

impl std::fmt::Display for McpOAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRequired => write!(f, "OAuth not required"),
            Self::NoAuthorizationServers => write!(f, "No authorization servers"),
            Self::DiscoveryFailed(m) => write!(f, "Discovery failed: {m}"),
            Self::RegistrationFailed(m) => write!(f, "Registration failed: {m}"),
            Self::AuthUrlFailed(m) => write!(f, "Auth URL failed: {m}"),
            Self::TokenRefreshRejected(m) => write!(f, "Refresh token rejected: {m}"),
            Self::AuthorizationRequired(m) => write!(f, "Authorization required: {m}"),
            Self::TokenExchangeFailed(m) => write!(f, "Token exchange failed: {m}"),
            Self::Timeout => write!(f, "OAuth timed out"),
            Self::Keychain(m) => write!(f, "Keychain: {m}"),
            Self::Http(m) => write!(f, "HTTP: {m}"),
            Self::InvalidConfig(m) => write!(f, "Invalid config: {m}"),
        }
    }
}

impl std::error::Error for McpOAuthError {}

#[allow(dead_code)]
fn keychain_key(server_url: &str) -> String {
    format!("mcp:{}", server_url.trim_end_matches('/'))
}

/// Per-server count of stored-token writes and deletes, keyed like the token
/// file. The MCP probe dedupe reads it so a probe that read the old token is
/// never joined after a sign-in or sign-out.
static TOKEN_GENERATIONS: LazyLock<Mutex<HashMap<String, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn token_generation(server_url: &str) -> u64 {
    TOKEN_GENERATIONS
        .lock()
        .get(server_url.trim_end_matches('/'))
        .copied()
        .unwrap_or(0)
}

fn bump_token_generation(server_url: &str) {
    *TOKEN_GENERATIONS
        .lock()
        .entry(server_url.trim_end_matches('/').to_string())
        .or_default() += 1;
}

pub fn load_stored_token(server_url: &str) -> Result<Option<StoredToken>, McpOAuthError> {
    let path = token_file_path(server_url)?;
    match std::fs::read_to_string(&path) {
        Ok(json) => {
            let token: StoredToken = serde_json::from_str(&json)
                .map_err(|e| McpOAuthError::Keychain(format!("parse: {e}")))?;
            Ok(Some(token))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(McpOAuthError::Keychain(format!("read: {e}"))),
    }
}

pub fn store_token(server_url: &str, token: &StoredToken) -> Result<(), McpOAuthError> {
    let path = token_file_path(server_url)?;
    let json = serde_json::to_string(token)
        .map_err(|e| McpOAuthError::Keychain(format!("serialize: {e}")))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| McpOAuthError::Keychain(format!("create dir: {e}")))?;
    }
    // Create the token file with owner-only permissions (0600) on Unix so
    // no other local account can read the bearer token. On non-Unix the
    // default ACL still restricts to the creating user under a typical
    // profile dir.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| McpOAuthError::Keychain(format!("open: {e}")))?;
        file.write_all(json.as_bytes())
            .map_err(|e| McpOAuthError::Keychain(format!("write: {e}")))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, json).map_err(|e| McpOAuthError::Keychain(format!("write: {e}")))?;
    }
    bump_token_generation(server_url);
    Ok(())
}

pub fn delete_stored_token(server_url: &str) -> Result<(), McpOAuthError> {
    let path = token_file_path(server_url)?;
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(McpOAuthError::Keychain(format!("delete: {e}"))),
    }
    bump_token_generation(server_url);
    Ok(())
}

/// Resolve the file path for storing an OAuth token. Tokens are stored as
/// JSON files in the app's data directory under `mcp-oauth/`, keyed by a
/// hash of the server URL. This avoids the Windows Credential Manager's
/// 2560-character limit (OAuth JWT access tokens routinely exceed it).
fn token_file_path(server_url: &str) -> Result<std::path::PathBuf, McpOAuthError> {
    // Store tokens as JSON files under the app data dir (avoids the Windows
    // Credential Manager's 2560-char limit — OAuth JWTs routinely exceed it).
    // Uses the same env-var resolution as `web/config.rs` (LOCALAPPDATA on
    // Windows, XDG_STATE_HOME/HOME on Linux, HOME on macOS).
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_STATE_HOME"))
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .ok_or_else(|| McpOAuthError::Keychain("no data dir env var".to_string()))?;
    let dir = base.join("com.termul-manager.app").join("mcp-oauth");
    // Hash the normalized URL with a version-stable digest so filenames do
    // not change across Rust releases (DefaultHasher has no stability
    // guarantee) and trailing-slash variants (`https://x/mcp` vs
    // `https://x/mcp/`) map to one file.
    use sha2::{Digest, Sha256};
    let normalized = server_url.trim_end_matches('/');
    let hash = format!("{:x}", Sha256::digest(normalized.as_bytes()));
    Ok(dir.join(format!("{hash}.json")))
}

pub fn is_token_expired(token: &StoredToken) -> bool {
    match token.expires_at {
        Some(exp) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            exp <= now + 60
        }
        None => false,
    }
}

pub async fn get_valid_token(server_url: &str) -> Result<Option<String>, McpOAuthError> {
    let stored = load_stored_token(server_url)?;
    match stored {
        Some(t) if !is_token_expired(&t) => Ok(Some(t.access_token)),
        Some(t) if t.refresh_token.is_some() => refresh_single_flight(server_url, t).await,
        Some(_) => {
            let _ = delete_stored_token(server_url);
            Ok(None)
        }
        None => Ok(None),
    }
}

/// One refresh runs at a time per normalized server URL; callers that arrive
/// while it runs join it instead of racing the token endpoint with a stale
/// refresh token (single-use refresh tokens make the loser delete the
/// winner's stored token).
static REFRESH_INFLIGHT: LazyLock<Mutex<HashMap<String, Arc<RefreshInFlight>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct RefreshInFlight {
    /// Signal set by the spawned refresh task when it finishes. A
    /// `JoinHandle` can only be polled once, so joiners wait on this and
    /// read the shared result slot instead.
    done: tokio::sync::watch::Sender<bool>,
    result: parking_lot::Mutex<Option<Result<StoredToken, McpOAuthError>>>,
}

/// Upper bound on how long a chat start waits on one OAuth refresh. rmcp
/// times each HTTP request out at 30 s and discovery alone can take several
/// requests, so an unresponsive auth server could otherwise stall a chat
/// for minutes.
const REFRESH_BUDGET: std::time::Duration = std::time::Duration::from_secs(45);

async fn refresh_single_flight(
    server_url: &str,
    stored: StoredToken,
) -> Result<Option<String>, McpOAuthError> {
    let key = stored.server_url.trim_end_matches('/').to_string();
    let (inflight, join_rx) = {
        let mut map = REFRESH_INFLIGHT.lock();
        match map.get(&key) {
            Some(existing) => (existing.clone(), existing.done.subscribe()),
            None => {
                let (done_tx, done_rx) = tokio::sync::watch::channel(false);
                let entry = Arc::new(RefreshInFlight {
                    done: done_tx,
                    result: parking_lot::Mutex::new(None),
                });
                map.insert(key.clone(), entry.clone());
                let owner = entry.clone();
                let key_for_task = key.clone();
                tokio::spawn(async move {
                    let outcome = refresh_and_store(stored).await;
                    owner.result.lock().clone_from(&Some(outcome));
                    let _ = owner.done.send(true);
                    REFRESH_INFLIGHT.lock().remove(&key_for_task);
                });
                (entry, done_rx)
            }
        }
    };
    let joined = async {
        let mut rx = join_rx;
        while !*rx.borrow() {
            if rx.changed().await.is_err() {
                // The refresh task dropped without signalling: treat as
                // a transient failure and keep the stored token.
                return None;
            }
        }
        inflight.result.lock().clone()
    };
    match tokio::time::timeout(REFRESH_BUDGET, joined).await {
        Ok(Some(Ok(new))) => Ok(Some(new.access_token)),
        Ok(Some(Err(e))) => {
            log::warn!("[mcp-oauth] token refresh failed (url redacted): {e}");
            if refresh_is_definitive_rejection(&e) {
                log::warn!(
                    "[mcp-oauth] refresh token rejected, stored token removed (url redacted)"
                );
                let _ = delete_stored_token(server_url);
            }
            Ok(None)
        }
        Ok(None) => Ok(None),
        Err(_elapsed) => {
            log::warn!(
                "[mcp-oauth] token refresh exceeded the {REFRESH_BUDGET:?} budget; \
                 continuing without a token (url redacted)"
            );
            Ok(None)
        }
    }
}

async fn refresh_and_store(stored: StoredToken) -> Result<StoredToken, McpOAuthError> {
    let new = refresh_token(&stored).await?;
    store_token(&stored.server_url, &new)?;
    log::info!("[mcp-oauth] expired token refreshed (url redacted)");
    Ok(new)
}

/// Only a definitive authorization-server rejection means the stored token
/// is unusable. Transient failures (network, timeout, provider 5xx) keep the
/// stored token so a later chat can retry the refresh.
fn refresh_is_definitive_rejection(e: &McpOAuthError) -> bool {
    matches!(
        e,
        McpOAuthError::TokenRefreshRejected(_) | McpOAuthError::AuthorizationRequired(_)
    )
}

async fn refresh_token(stored: &StoredToken) -> Result<StoredToken, McpOAuthError> {
    let mut manager = AuthorizationManager::new(&stored.issuer)
        .await
        .map_err(|e| McpOAuthError::DiscoveryFailed(e.to_string()))?;
    let resolution = manager
        .resolve_metadata()
        .await
        .map_err(|e| McpOAuthError::DiscoveryFailed(e.to_string()))?;
    manager.set_metadata(resolution.metadata);
    let config = rmcp::transport::auth::OAuthClientConfig::new(
        stored.client_id.clone(),
        "http://127.0.0.1/callback",
    );
    manager.configure_client(config).ok();
    // The manager refreshes from its credential store, which starts empty.
    let credentials: OAuthTokenResponse = serde_json::from_value(serde_json::json!({
        "access_token": stored.access_token,
        "token_type": "bearer",
        "refresh_token": stored.refresh_token,
    }))
    .map_err(|e| McpOAuthError::TokenExchangeFailed(format!("stored token: {e}")))?;
    let store = InMemoryCredentialStore::new();
    store
        .save(StoredCredentials::new(
            stored.client_id.clone(),
            Some(credentials),
            Vec::new(),
            None,
        ))
        .await
        .map_err(|e| McpOAuthError::TokenExchangeFailed(e.to_string()))?;
    manager.set_credential_store(store);
    let tr = manager
        .refresh_token()
        .await
        .map_err(map_refresh_error)?;
    use oauth2::TokenResponse;
    Ok(StoredToken {
        access_token: tr.access_token().secret().to_string(),
        refresh_token: tr
            .refresh_token()
            .map(|t| t.secret().to_string())
            .or_else(|| stored.refresh_token.clone()),
        expires_at: tr
            .expires_in()
            .map(|d| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|n| n.as_secs() + d.as_secs())
                    .unwrap_or(0)
            })
            .or(stored.expires_at),
        client_id: stored.client_id.clone(),
        issuer: stored.issuer.clone(),
        server_url: stored.server_url.clone(),
    })
}

/// Map an rmcp `AuthError` from a refresh attempt to our error class.
/// `TokenRefreshRejected` (definitive `invalid_grant`) and
/// `AuthorizationRequired` (no/foreign refresh token) mean the stored token
/// can never work again; everything else — network errors, timeouts,
/// `TokenRefreshFailed` — is transient and must keep the stored token.
fn map_refresh_error(e: rmcp::transport::auth::AuthError) -> McpOAuthError {
    match e {
        rmcp::transport::auth::AuthError::AuthorizationRequired => {
            McpOAuthError::AuthorizationRequired("stored credentials need re-authorization".into())
        }
        rmcp::transport::auth::AuthError::TokenRefreshRejected(detail) => {
            McpOAuthError::TokenRefreshRejected(detail)
        }
        other => McpOAuthError::TokenExchangeFailed(other.to_string()),
    }
}

pub async fn run_full_flow(
    server_url: &str,
    redirect_uri: &str,
    callback_url: String,
) -> Result<StoredToken, McpOAuthError> {
    let mut manager = AuthorizationManager::new(server_url)
        .await
        .map_err(|e| McpOAuthError::DiscoveryFailed(e.to_string()))?;
    let resolution = manager
        .resolve_metadata()
        .await
        .map_err(|e| McpOAuthError::DiscoveryFailed(e.to_string()))?;
    manager.set_metadata(resolution.metadata);
    let request = AuthorizationRequest::new(redirect_uri).with_client_name("Termul");
    let session = AuthorizationSession::new(manager, request)
        .await
        .map_err(|(_manager, e)| McpOAuthError::RegistrationFailed(e.to_string()))?;
    let callback = AuthorizationCallback::from_redirect_url(&callback_url)
        .map_err(|e| McpOAuthError::TokenExchangeFailed(format!("callback: {e}")))?;
    let mut state = OAuthState::Session(session);
    state
        .handle_callback(&callback.code, &callback.csrf_token)
        .await
        .map_err(|e| McpOAuthError::TokenExchangeFailed(e.to_string()))?;
    let access_token = state
        .get_access_token()
        .await
        .map_err(|e| McpOAuthError::TokenExchangeFailed(e.to_string()))?;
    let (client_id, refresh_token, expires_at) = match &state {
        OAuthState::Session(s) => {
            let c = s
                .auth_manager
                .get_credentials()
                .await
                .map_err(|e| McpOAuthError::TokenExchangeFailed(e.to_string()))?;
            let refresh = c.1.as_ref().and_then(|tr| {
                use oauth2::TokenResponse;
                tr.refresh_token().map(|t| t.secret().to_string())
            });
            let exp = c.1.as_ref().and_then(|tr| {
                use oauth2::TokenResponse;
                tr.expires_in().map(|d| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|n| n.as_secs() + d.as_secs())
                        .unwrap_or(0)
                })
            });
            (c.0, refresh, exp)
        }
        OAuthState::Unauthorized(m) | OAuthState::Authorized(m) => {
            let c = m
                .get_credentials()
                .await
                .map_err(|e| McpOAuthError::TokenExchangeFailed(e.to_string()))?;
            let refresh = c.1.as_ref().and_then(|tr| {
                use oauth2::TokenResponse;
                tr.refresh_token().map(|t| t.secret().to_string())
            });
            let exp = c.1.as_ref().and_then(|tr| {
                use oauth2::TokenResponse;
                tr.expires_in().map(|d| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|n| n.as_secs() + d.as_secs())
                        .unwrap_or(0)
                })
            });
            (c.0, refresh, exp)
        }
        _ => (String::new(), None, None),
    };
    let stored = StoredToken {
        access_token,
        refresh_token,
        expires_at,
        client_id,
        issuer: server_url.to_string(),
        server_url: server_url.to_string(),
    };
    store_token(server_url, &stored)?;
    log::info!(
        "[mcp-oauth] token stored (url redacted), expires: {:?}",
        stored.expires_at
    );
    Ok(stored)
}
#[allow(dead_code)]
struct KeychainCredentialStore {
    server_url: String,
}

#[allow(dead_code)]
impl KeychainCredentialStore {
    fn new(server_url: &str) -> Self {
        Self {
            server_url: server_url.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl CredentialStore for KeychainCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, rmcp::transport::auth::AuthError> {
        match load_stored_token(&self.server_url) {
            Ok(Some(t)) => Ok(Some(StoredCredentials::new(
                t.client_id,
                None,
                vec![],
                None,
            ))),
            Ok(None) => Ok(None),
            Err(e) => Err(rmcp::transport::auth::AuthError::InternalError(
                e.to_string(),
            )),
        }
    }
    async fn save(&self, _: StoredCredentials) -> Result<(), rmcp::transport::auth::AuthError> {
        Ok(())
    }
    async fn clear(&self) -> Result<(), rmcp::transport::auth::AuthError> {
        let _ = delete_stored_token(&self.server_url);
        Ok(())
    }
}

#[allow(dead_code)]
pub async fn fetch_resource_metadata(header: &str) -> Result<Option<Value>, McpOAuthError> {
    match extract_resource_metadata_url(header) {
        Some(url) => {
            let resp = reqwest::Client::new()
                .get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| McpOAuthError::Http(e.to_string()))?;
            let json: Value = resp
                .json()
                .await
                .map_err(|e| McpOAuthError::Http(e.to_string()))?;
            Ok(Some(json))
        }
        None => Ok(None),
    }
}

#[allow(dead_code)]
pub fn extract_resource_metadata_url(header: &str) -> Option<String> {
    let marker = "resource_metadata=\"";
    if let Some(start) = header.find(marker) {
        let rest = &header[start + marker.len()..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    let marker = "resource_metadata=";
    if let Some(start) = header.find(marker) {
        let rest = &header[start + marker.len()..];
        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let url = &rest[..end];
        if !url.is_empty() && url != "Bearer" {
            return Some(url.to_string());
        }
    }
    None
}

pub fn is_auth_required(header: &str) -> bool {
    let l = header.to_ascii_lowercase();
    l.contains("bearer")
        && (l.contains("resource_metadata") || l.contains("realm") || l.contains("error"))
}

pub fn bearer_header(token: &str) -> String {
    token.to_string()
}

#[cfg(test)]
pub(crate) mod tests;
