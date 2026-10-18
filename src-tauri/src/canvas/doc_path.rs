//! The document path a canvas open names, checked the same way on both
//! transports (`canvas_open` and `POST /canvas/open`): it must lie inside a
//! registered, non-archived project root, with the containment the fs and
//! git routes enforce.

use std::path::{Path, PathBuf};

use super::{CanvasError, CODE_PATH_VALIDATION_FAILED};
use crate::path_validation::{client_path, strip_verbatim_prefix};
use crate::web::project_registry::ProjectRegistry;

/// The doc key a canvas opens under: the canonical doc path in its plain
/// (verbatim-stripped) form. A document that does not exist yet is resolved
/// through its nearest existing ancestor; the first save creates it.
///
/// `registry` is `None` only in test compositions without a project
/// boundary, which skip the check.
///
/// # Errors
/// `PATH_VALIDATION_FAILED` for a traversal, an unresolvable path, or a path
/// outside every registered project root.
pub fn resolve_doc_path(
    doc_path: &str,
    registry: Option<&ProjectRegistry>,
) -> Result<String, CanvasError> {
    let doc_path = client_path(doc_path);
    let Some(registry) = registry else {
        return Ok(doc_path);
    };
    let failed = |message: String| CanvasError::new(CODE_PATH_VALIDATION_FAILED, message);
    let resolved: PathBuf = crate::web::fs_api::resolve_request_path(Path::new(&doc_path))
        .map_err(|(message, code)| failed(format!("{message} (code {code})")))?;
    if !registry.is_within_any_registered_root(&resolved) {
        return Err(failed(format!(
            "doc path '{}' is outside the project boundary",
            resolved.display()
        )));
    }
    Ok(strip_verbatim_prefix(&resolved.to_string_lossy()).into_owned())
}
