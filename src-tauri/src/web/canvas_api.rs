//! Canvas web routes: `POST /canvas/open` / `/canvas/close` (`IpcBody`
//! envelope, HTTP 200 for app-level failures like their Tauri command twins),
//! the persistence routes of `canvas_persistence_api`, and the same-origin
//! `/canvas/<id>/*` figcraft bundle.
//!
//! Registered AHEAD of the static fallback, so the SPA mount cannot shadow
//! any of it. Doc paths on `/canvas/open` are validated against the
//! project-root boundary exactly like the fs/git routes (`validate_doc_path`).
//! Auth split (browser iframes cannot send `Authorization: Bearer` headers):
//! - the API routes stay behind the outer bearer `web_auth_gate`
//!   (`"/canvas/"` in `GATED_PREFIXES` — the renderer facades already hold
//!   the web auth token);
//! - `/canvas/<id>/*` (the bundle) is EXEMPT from the bearer gate
//!   (`router::is_canvas_proxy_path`) and authenticated by
//!   `static_server::serve_bundle`: a `ct` query param or the per-canvas
//!   cookie it sets must constant-time-match the token minted by the
//!   project's open.
//!
//! `POST /canvas/open` mounts the figcraft editor: it mints the project's
//! bundle token (`static_server::open_canvas`) and `/canvas/<id>/*` then
//! serves the embedded bundle. `POST /canvas/close` revokes the token, so
//! stale `ct` params and cookies stop authenticating.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::response::{Json, Response};
use axum::routing::{any, get, post};
use axum::Router;
use serde::Deserialize;

use crate::canvas::persistence::CloseOutcome;
use crate::canvas::static_server::{
    close_canvas, open_canvas, same_origin_base, serve_bundle, CanvasOpenInfo, CookiePolicy,
    OpenRequest,
};
use crate::web::capabilities::WriteFlags;
use crate::web::fs_api::{check_local_only, IpcBody};
use crate::web::project_registry::ProjectRegistry;

/// Router state: the project-root boundary for doc-path validation (`None`
/// in test compositions skips validation) and the deployment's write flags.
#[derive(Clone)]
pub struct CanvasState {
    pub boundary: Option<CanvasProjectBoundary>,
    /// Deployment flags for the shared write predicate: `/canvas/open` and
    /// `/canvas/close` write the document, so they are write routes like the
    /// fs/git ones.
    pub write_flags: WriteFlags,
}

/// Project-root boundary for the web doc-path validation on open — the SAME
/// containment semantics the fs/git routes enforce (`git_api::
/// ensure_within_project_boundary`): the path is accepted when it is within
/// any registered, non-archived project root.
#[derive(Clone)]
pub struct CanvasProjectBoundary {
    pub registry: Arc<ProjectRegistry>,
}

/// The shared write predicate for the canvas open/close routes.
pub(crate) fn check_canvas_write<T>(
    state: &CanvasState,
    peer: SocketAddr,
    route: &str,
) -> Option<IpcBody<T>> {
    check_local_only::<T>(
        peer,
        state.write_flags.allow_remote_writes,
        state.write_flags.shared_live_writes_denied,
        route,
    )
}

/// Validate a canvas doc path against the project boundary — mirrors the
/// fs/git route validation (`fs_api::resolve_request_path` canonicalization +
/// `git_api::ensure_within_project_boundary` containment): the doc a web
/// client opens must live inside a registered, non-archived project root,
/// exactly like every other web fs operation. Returns the
/// canonicalized, verbatim-stripped doc path on success; a typed
/// `PATH_VALIDATION_FAILED` pair on rejection. `None` boundary (test
/// compositions) skips validation and returns the verbatim-stripped path.
fn validate_doc_path(
    boundary: Option<&CanvasProjectBoundary>,
    doc_path: &str,
) -> Result<String, (String, String)> {
    let failed = |message: String| {
        (
            message,
            crate::canvas::CODE_PATH_VALIDATION_FAILED.to_string(),
        )
    };
    let Some(boundary) = boundary else {
        return Ok(crate::path_validation::strip_verbatim_prefix(doc_path).into_owned());
    };
    // Traversal rejection + canonicalization (same helper the fs routes use;
    // ancestor-walks for not-yet-existing tails).
    let resolved: PathBuf = crate::web::fs_api::resolve_request_path(Path::new(doc_path))
        .map_err(|(message, code)| failed(format!("{message} (code {code})")))?;
    // Containment vs any registered, non-archived project root.
    let outside =
        crate::web::git_api::ensure_within_project_boundary::<()>(&resolved, &boundary.registry)
            .is_some();
    if outside {
        return Err(failed(format!(
            "doc path '{}' is outside the project boundary",
            resolved.display()
        )));
    }
    // Tool-friendly string form (verbatim-stripped), like `git_api::cwd_string`.
    Ok(crate::path_validation::strip_verbatim_prefix(&resolved.to_string_lossy()).into_owned())
}

/// `POST /canvas/open` body: `{ "docPath": "...", "projectId": "...",
/// "clientInstanceId": "...", "autosaveMs"?: n, "leaseGraceMs"?: n,
/// "maxMounted"?: n, "crashTimeouts"?: n }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CanvasOpenRequest {
    doc_path: String,
    project_id: String,
    #[serde(flatten)]
    request: OpenRequest,
}

/// `POST /canvas/close` body: `{ "projectId": "...", "discard"?: bool }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CanvasCloseRequest {
    project_id: String,
    #[serde(default)]
    discard: Option<bool>,
}

/// Build the canvas sub-router with its state APPLIED — generic over the
/// caller's outer router state so it merges into both `router()` and
/// `router_with_static()` like any stateless router.
pub fn canvas_router<S>(state: CanvasState) -> Router<S> {
    Router::new()
        .route("/canvas/open", post(open))
        .route("/canvas/close", post(close))
        .merge(crate::web::canvas_persistence_api::persistence_routes())
        // The iframe src is `/canvas/<id>/` (the trailing slash keeps the
        // bundle's relative asset URLs inside the prefix). axum does not
        // collapse the bare and trailing-slash forms, so both are registered.
        .route("/canvas/{id}", get(bundle))
        .route("/canvas/{id}/", get(bundle))
        .route("/canvas/{id}/{*rest}", any(bundle))
        .with_state(state)
}

/// `/canvas/<id>[/...]` — the figcraft bundle, gated by the canvas's `ct`
/// token or cookie.
async fn bundle(request: Request) -> Response {
    serve_bundle(
        CookiePolicy::SameOrigin,
        request.method(),
        request.uri(),
        request.headers(),
    )
}

/// `POST /canvas/open` — validate the doc path against the project boundary,
/// then return the same-origin bundle base plus the project's minted `ct`
/// token (the twin of the `canvas_open` Tauri command).
async fn open(
    State(state): State<CanvasState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(req): Json<CanvasOpenRequest>,
) -> Json<IpcBody<CanvasOpenInfo>> {
    if let Some(forbidden) = check_canvas_write(&state, peer, "/canvas/open") {
        return Json(forbidden);
    }
    let log_project_id = crate::commands::sanitize_log_field(&req.project_id);
    let doc_key = match validate_doc_path(state.boundary.as_ref(), &req.doc_path) {
        Ok(path) => path,
        Err((message, code)) => {
            tracing::warn!(
                target: "termul::web::canvas",
                "canvas open rejected project_id={log_project_id}: {message}"
            );
            return Json(IpcBody::err(message, code));
        }
    };
    match open_canvas(same_origin_base(), &req.project_id, doc_key, req.request).await {
        Ok(info) => {
            tracing::info!(
                target: "termul::web::canvas",
                "canvas open (web) project_id={log_project_id} canvas_id={}",
                info.canvas_id
            );
            Json(IpcBody::ok(info))
        }
        Err(err) => {
            tracing::warn!(
                target: "termul::web::canvas",
                "canvas open failed project_id={log_project_id} code={}",
                err.code
            );
            Json(IpcBody::err(err.message, err.code))
        }
    }
}

/// `POST /canvas/close` — unmount the project's canvas through the AD-31
/// sequence (`static_server::close_canvas`, shared with `canvas_close`). A
/// failed save answers `SAVE_FAILED` and keeps the mount; `discard: true` is
/// the user's explicit choice to drop unsaved changes. Idempotent.
async fn close(
    State(state): State<CanvasState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(req): Json<CanvasCloseRequest>,
) -> Json<IpcBody<CloseOutcome>> {
    if let Some(forbidden) = check_canvas_write(&state, peer, "/canvas/close") {
        return Json(forbidden);
    }
    let mode = crate::commands::close_mode(req.discard);
    let project_id = crate::commands::sanitize_log_field(&req.project_id);
    tracing::info!(
        target: "termul::web::canvas",
        "canvas close (web) project_id={project_id} mode={mode:?}"
    );
    match close_canvas(&req.project_id, mode).await {
        Ok(outcome) => {
            if outcome.discarded {
                tracing::warn!(
                    target: "termul::web::canvas",
                    "canvas closed project_id={project_id}: unsaved changes discarded by the user"
                );
            } else {
                tracing::info!(
                    target: "termul::web::canvas",
                    "canvas closed project_id={project_id} unmounted={} saved={}",
                    outcome.unmounted,
                    outcome.saved_version.as_deref().unwrap_or("none")
                );
            }
            Json(IpcBody::ok(outcome))
        }
        Err(err) => {
            tracing::warn!(
                target: "termul::web::canvas",
                "canvas close refused project_id={project_id} code={}: {}",
                err.code,
                err.message
            );
            Json(IpcBody::err(err.message, err.code))
        }
    }
}
