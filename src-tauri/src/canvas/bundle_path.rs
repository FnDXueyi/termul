//! The shape of a `/canvas/...` request path, shared by the router's auth
//! gate and every log line that names a request path.
//!
//! The canvas token is the path segment after the canvas id, so a logged
//! bundle path is a logged credential: log it through [`redact_canvas_path`].

use std::borrow::Cow;

/// What replaces the token segment in a logged path.
pub const REDACTED_SEGMENT: &str = "[redacted]";

/// First segments under `/canvas/` that name an API route, not a canvas id.
const API_ROUTES: &[&str] = &[
    "open",
    "close",
    "read-document",
    "document-loaded",
    "dirty-changed",
    "set-visible",
    "write-snapshot",
    "disk-changed",
    "resolve-conflict",
    "crashed",
];

/// Whether `path` is a `/canvas/<id>/<token>/...` bundle path, which the
/// canvas token authorizes instead of the outer bearer gate.
#[must_use]
pub fn is_bundle_path(path: &str) -> bool {
    path.strip_prefix("/canvas/")
        .is_some_and(|rest| !API_ROUTES.contains(&rest.split('/').next().unwrap_or_default()))
}

/// `path` with the token segment of a bundle path replaced, so it can be
/// logged: `/canvas/<id>/<token>/app.js` becomes `/canvas/<id>/[redacted]/app.js`.
/// Every other path comes back unchanged.
#[must_use]
pub fn redact_canvas_path(path: &str) -> Cow<'_, str> {
    if !is_bundle_path(path) {
        return Cow::Borrowed(path);
    }
    let Some((canvas_id, after_id)) = path
        .strip_prefix("/canvas/")
        .and_then(|rest| rest.split_once('/'))
    else {
        return Cow::Borrowed(path);
    };
    let (token, tail) = after_id.split_at(after_id.find('/').unwrap_or(after_id.len()));
    if token.is_empty() {
        return Cow::Borrowed(path);
    }
    Cow::Owned(format!("/canvas/{canvas_id}/{REDACTED_SEGMENT}{tail}"))
}
