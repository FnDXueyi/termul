//! Browser policy web routes: the HTTP twins of the `browser_policy_*` Tauri
//! commands. Both call [`BrowserPolicyService`], so a given operation
//! sequence returns the same body on either transport (`IpcBody`, HTTP 200
//! for app-level refusals).
//!
//! - `GET  /host/browser-policy/app`
//! - `POST /host/browser-policy/app` `{expectedRevision, op}`
//! - `GET  /host/browser-policy/project?projectId=`
//! - `POST /host/browser-policy/project` `{projectId, expectedRevision, op}`
//!
//! The `/host/` prefix is bearer-gated whenever the host runs a token gate.
//! A request that passed the gate holds app scope. A client bound to one
//! project carries a [`BoundProject`] request extension (set by whatever
//! authenticates project-bound clients) and is refused app-level operations
//! with `app_scope_required`. Without a gate only loopback peers are served.
//! A host with no policy service (`termul-server`) answers
//! `capability_unavailable`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::request::Parts;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use crate::acp::browser_policy::{
    AppPolicyView, BrowserPolicyService, CallerScope, PolicyRefusal, ProjectPolicyView,
};
use crate::web::auth::BearerVerified;
use crate::web::fs_api::IpcBody;

/// Request extension marking a client bound to a single project.
#[derive(Debug, Clone)]
pub struct BoundProject(pub String);

#[derive(Clone)]
pub struct PolicyRouteState {
    /// `None` on a host without browser policy.
    pub service: Option<Arc<BrowserPolicyService>>,
    /// Whether the host runs a bearer-token gate. Without one only loopback
    /// peers are served.
    pub token_gated: bool,
}

pub fn browser_policy_router<S>(state: PolicyRouteState) -> Router<S> {
    Router::new()
        .route("/host/browser-policy/app", get(get_app).post(apply_app))
        .route(
            "/host/browser-policy/project",
            get(get_project).post(apply_project),
        )
        .with_state(state)
}

fn body<T>(result: Result<T, PolicyRefusal>) -> Json<IpcBody<T>> {
    Json(match result {
        Ok(data) => IpcBody::ok(data),
        Err(refusal) => IpcBody::err(refusal.envelope_error(), refusal.code().as_str()),
    })
}

fn refuse<T>(refusal: PolicyRefusal) -> Json<IpcBody<T>> {
    body(Err(refusal))
}

/// Who is calling: the service plus the scope this request holds.
fn caller<T>(
    state: &PolicyRouteState,
    parts: &Parts,
    peer: SocketAddr,
) -> Result<(Arc<BrowserPolicyService>, CallerScope), Json<IpcBody<T>>> {
    let Some(service) = state.service.clone() else {
        return Err(refuse(PolicyRefusal::StoreUnavailable));
    };
    let admitted = parts.extensions.get::<BearerVerified>().is_some()
        || (!state.token_gated && peer.ip().is_loopback());
    if !admitted {
        tracing::warn!(
            target: "termul::web::browser_policy",
            peer = %peer,
            "browser policy request refused: no verified bearer token"
        );
        return Err(Json(IpcBody::err(
            "browser policy needs the host token",
            "FORBIDDEN",
        )));
    }
    let scope = match parts.extensions.get::<BoundProject>() {
        Some(BoundProject(project_id)) => CallerScope::Project(project_id.clone()),
        None => CallerScope::App,
    };
    Ok((service, scope))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectQuery {
    #[serde(default)]
    project_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProjectBody {
    project_id: String,
    expected_revision: u64,
    op: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyAppBody {
    expected_revision: u64,
    op: Value,
}

async fn get_app(
    State(state): State<PolicyRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
) -> Json<IpcBody<AppPolicyView>> {
    match caller(&state, &parts, peer) {
        Ok((service, scope)) => body(service.app_view(&scope).await),
        Err(refused) => refused,
    }
}

async fn apply_app(
    State(state): State<PolicyRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
    request: Result<Json<ApplyAppBody>, JsonRejection>,
) -> Json<IpcBody<AppPolicyView>> {
    let (service, scope) = match caller(&state, &parts, peer) {
        Ok(caller) => caller,
        Err(refused) => return refused,
    };
    match request {
        Ok(Json(request)) => body(
            service
                .apply_app(&scope, request.expected_revision, &request.op)
                .await,
        ),
        Err(_) => refuse(PolicyRefusal::MalformedOperation),
    }
}

async fn get_project(
    State(state): State<PolicyRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
    Query(query): Query<ProjectQuery>,
) -> Json<IpcBody<ProjectPolicyView>> {
    match caller(&state, &parts, peer) {
        Ok((service, scope)) => body(service.project_view(&scope, &query.project_id).await),
        Err(refused) => refused,
    }
}

async fn apply_project(
    State(state): State<PolicyRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    parts: Parts,
    request: Result<Json<ApplyProjectBody>, JsonRejection>,
) -> Json<IpcBody<ProjectPolicyView>> {
    let (service, scope) = match caller(&state, &parts, peer) {
        Ok(caller) => caller,
        Err(refused) => return refused,
    };
    match request {
        Ok(Json(request)) => body(
            service
                .apply_project(
                    &scope,
                    &request.project_id,
                    request.expected_revision,
                    &request.op,
                )
                .await,
        ),
        Err(_) => refuse(PolicyRefusal::MalformedOperation),
    }
}

#[cfg(test)]
#[path = "browser_policy_api_tests.rs"]
mod tests;
