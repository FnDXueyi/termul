//! The document path a canvas open names, checked the same way on both
//! transports (`canvas_open` and `POST /canvas/open`): it must lie inside the
//! root of the project that opens it, a registered, non-archived project.

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
/// `PATH_VALIDATION_FAILED` for a traversal, an unresolvable path, a project
/// with no registered root, or a path outside `project_id`'s root (another
/// project's root included).
pub fn resolve_doc_path(
    doc_path: &str,
    project_id: &str,
    registry: Option<&ProjectRegistry>,
) -> Result<String, CanvasError> {
    let doc_path = client_path(doc_path);
    let Some(registry) = registry else {
        return Ok(doc_path);
    };
    let failed = |message: String| CanvasError::new(CODE_PATH_VALIDATION_FAILED, message);
    let resolved: PathBuf = crate::web::fs_api::resolve_request_path(Path::new(&doc_path))
        .map_err(|(message, code)| failed(format!("{message} (code {code})")))?;
    let Some(root) = registry.canonical_root(project_id) else {
        return Err(failed(format!(
            "project '{project_id}' has no registered root"
        )));
    };
    if !resolved.starts_with(&root) {
        return Err(failed(format!(
            "doc path '{}' is outside the project root",
            resolved.display()
        )));
    }
    Ok(strip_verbatim_prefix(&resolved.to_string_lossy()).into_owned())
}
