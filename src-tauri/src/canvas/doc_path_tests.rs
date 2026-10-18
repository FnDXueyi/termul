#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The doc-path check both canvas transports run before an open.

use super::doc_path::resolve_doc_path;
use super::CODE_PATH_VALIDATION_FAILED;
use crate::path_validation::strip_verbatim_prefix;
use crate::web::project_registry::ProjectRegistry;

/// A project folder holding `design.figcraft`, plus its canonical root in
/// plain form.
fn project() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("design.figcraft"), b"{}").unwrap();
    let canonical = dir.path().canonicalize().unwrap();
    let plain = strip_verbatim_prefix(&canonical.to_string_lossy()).into_owned();
    (dir, plain)
}

fn native(path: &str) -> String {
    if cfg!(windows) {
        path.replace('/', r"\")
    } else {
        path.to_string()
    }
}

#[test]
fn a_doc_inside_a_project_root_opens_under_its_canonical_path() {
    let (dir, plain) = project();
    let registry = ProjectRegistry::seeded_for_test(dir.path());
    for name in ["design.figcraft", "not-yet-saved.figcraft"] {
        let doc = dir.path().join(name);
        assert_eq!(
            resolve_doc_path(&doc.to_string_lossy(), Some(&registry)),
            Ok(native(&format!("{plain}/{name}"))),
            "{name}"
        );
    }
}

#[test]
fn a_doc_outside_every_project_root_is_refused() {
    let (dir, _) = project();
    let (elsewhere, _) = project();
    let registry = ProjectRegistry::seeded_for_test(dir.path());
    for name in ["design.figcraft", "not-yet-saved.figcraft"] {
        let doc = elsewhere.path().join(name);
        let refused = resolve_doc_path(&doc.to_string_lossy(), Some(&registry)).unwrap_err();
        assert_eq!(refused.code, CODE_PATH_VALIDATION_FAILED, "{name}");
        assert!(
            refused.message.contains("outside the project boundary"),
            "{refused}"
        );
    }
}

#[test]
fn a_traversal_out_of_the_root_is_refused() {
    let (dir, _) = project();
    let registry = ProjectRegistry::seeded_for_test(dir.path());
    let doc = dir.path().join("..").join("design.figcraft");
    let refused = resolve_doc_path(&doc.to_string_lossy(), Some(&registry)).unwrap_err();
    assert_eq!(refused.code, CODE_PATH_VALIDATION_FAILED);
}

/// The Windows project registry stores `\\?\D:\…` roots, and the renderer
/// echoes them back joined with `/` or with every `\` turned into `/`.
#[cfg(windows)]
#[test]
fn a_verbatim_root_in_any_slash_style_opens_the_plain_doc_path() {
    let (dir, plain) = project();
    let verbatim = dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(verbatim.starts_with(r"\\?\"), "{verbatim}");
    let registry = ProjectRegistry::seeded_for_test(std::path::Path::new(&verbatim));
    let forward = verbatim.replace('\\', "/");
    for name in ["design.figcraft", "new.figcraft"] {
        let expected = format!(r"{plain}\{name}");
        for doc in [
            format!(r"{verbatim}\{name}"),
            format!("{verbatim}/{name}"),
            format!("{forward}/{name}"),
        ] {
            assert_eq!(
                resolve_doc_path(&doc, Some(&registry)).as_ref(),
                Ok(&expected),
                "{doc}"
            );
        }
    }

    let (elsewhere, _) = project();
    let outside = elsewhere
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let refused = resolve_doc_path(&format!("{outside}/design.figcraft"), Some(&registry));
    assert_eq!(
        refused.map_err(|err| err.code),
        Err(CODE_PATH_VALIDATION_FAILED.to_string()),
        "a verbatim doc outside the verbatim root"
    );
}
