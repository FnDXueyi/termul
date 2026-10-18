//! `Host` header allow-list for termul's HTTP and WebSocket listeners.
//!
//! DNS rebinding lets a page on a hostname its author controls resolve that
//! name to `127.0.0.1` and reach a loopback listener as a same-origin
//! request. The browser still sends the attacker's hostname in `Host`, so a
//! listener answers only names it actually serves:
//!
//! - IP literals, because rebinding needs a hostname;
//! - `localhost`;
//! - names given at startup (the hosts of `--allowed-origins`) or added at
//!   runtime (the shared-live tunnel hostname).
//!
//! A standalone server that requires its bearer token accepts any `Host`
//! ([`HostAllowList::any`]): a rebinding page cannot present the token, and
//! reverse proxies and container networks forward names the server cannot
//! know in advance.
//!
//! A request without `Host` passes. Browsers always send one, so its absence
//! means a non-browser client, which rebinding cannot produce.

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::origin::{split_host_port, OriginPolicy};

/// Extra host names a listener serves. Clones share one list, so a name
/// added after the listener started applies to it.
#[derive(Debug, Clone, Default)]
pub struct HostAllowList {
    names: Arc<RwLock<BTreeSet<String>>>,
    any: bool,
}

impl HostAllowList {
    /// IP literals and `localhost` only.
    pub fn loopback() -> Self {
        Self::default()
    }

    /// Every `Host`. Only for a listener whose non-public routes all require
    /// the bearer token.
    pub fn any() -> Self {
        Self {
            any: true,
            ..Self::default()
        }
    }

    /// Loopback names plus the hosts of the operator's extra origins.
    pub fn from_origins(policy: &OriginPolicy) -> Self {
        let list = Self::default();
        for host in policy.hosts() {
            list.allow(&host);
        }
        list
    }

    /// Accept `host` (a bare host name, any case, no port) from now on.
    pub fn allow(&self, host: &str) {
        if let Some((name, _)) = split_host_port(host) {
            self.names
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(name);
        }
    }

    /// Whether a request with this `Host` value (host and optional port) may
    /// proceed.
    pub fn allows(&self, host_header: Option<&str>) -> bool {
        let Some(raw) = host_header else {
            return true;
        };
        if self.any {
            return true;
        }
        let Some((host, _port)) = split_host_port(raw) else {
            return false;
        };
        host == "localhost"
            || host.parse::<IpAddr>().is_ok()
            || self
                .names
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(&host)
    }

    /// For listeners that parse HTTP by hand: read the header lines that
    /// follow the request line and check their `Host`. A block that does not
    /// end cleanly (read error, timeout, EOF, more than 100 lines) fails.
    pub fn allows_header_block(
        &self,
        lines: &mut impl Iterator<Item = std::io::Result<String>>,
    ) -> bool {
        read_header_block(lines).is_some_and(|headers| self.allows(header_value(&headers, "host")))
    }
}

/// Put the `Host` check in front of everything else in `router`.
pub fn layer<S>(router: axum::Router<S>, hosts: HostAllowList) -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(axum::middleware::from_fn_with_state(hosts, enforce_host))
}

async fn enforce_host(
    State(hosts): State<HostAllowList>,
    request: Request,
    next: Next,
) -> Response {
    let host = match request.headers().get(header::HOST) {
        Some(value) => value
            .to_str()
            .ok()
            .map(str::to_owned)
            .or(Some(String::new())),
        None => request
            .uri()
            .authority()
            .map(|authority| authority.to_string()),
    };
    if hosts.allows(host.as_deref()) {
        return next.run(request).await;
    }
    // The name is caller-controlled, so only its port is logged.
    let port = host
        .as_deref()
        .and_then(split_host_port)
        .and_then(|(_, port)| port);
    tracing::warn!(
        target: "termul::web::host",
        method = %request.method(),
        port = ?port,
        "rejected request: host not served"
    );
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "success": false,
            "error": "host is not allowed",
            "code": "FORBIDDEN",
        })),
    )
        .into_response()
}

fn read_header_block(
    lines: &mut impl Iterator<Item = std::io::Result<String>>,
) -> Option<Vec<String>> {
    let mut headers = Vec::new();
    for line in lines.take(100) {
        let line = line.ok()?;
        if line.trim().is_empty() {
            return Some(headers);
        }
        headers.push(line);
    }
    None
}

fn header_value<'a>(headers: &'a [String], name: &str) -> Option<&'a str> {
    headers.iter().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

#[cfg(test)]
mod tests;
