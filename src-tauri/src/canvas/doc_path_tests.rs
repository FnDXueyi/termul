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

fn refused_code(doc: &str, project_id: &str, registry: &ProjectRegistry) -> String {
    resolve_doc_path(doc, project_id, Some(registry))
        .expect_err(doc)
        .code
}

#[test]
fn a_doc_inside_its_project_root_opens_under_its_canonical_path() {
    let (dir, plain) = project();
    let registry = ProjectRegistry::with_roots_for_test(&[("a", dir.path())]);
    for name in ["design.figcraft", "not-yet-saved.figcraft"] {
        let doc = dir.path().join(name);
        assert_eq!(
            resolve_doc_path(&doc.to_string_lossy(), "a", Some(&registry)),
            Ok(native(&format!("{plain}/{name}"))),
            "{name}"
        );
    }
}

#[test]
fn a_doc_outside_every_project_root_is_refused() {
    let (dir, _) = project();
    let (elsewhere, _) = project();
    let registry = ProjectRegistry::with_roots_for_test(&[("a", dir.path())]);
    for name in ["design.figcraft", "not-yet-saved.figcraft"] {
        let doc = elsewhere.path().join(name);
        let refused = resolve_doc_path(&doc.to_string_lossy(), "a", Some(&registry)).unwrap_err();
        assert_eq!(refused.code, CODE_PATH_VALIDATION_FAILED, "{name}");
        assert!(
            refused.message.contains("outside the project root"),
            "{refused}"
        );
    }
}

#[test]
fn a_doc_in_another_projects_root_is_refused_under_this_projects_id() {
    let (a, _) = project();
    let (b, b_plain) = project();
    let registry = ProjectRegistry::with_roots_for_test(&[("a", a.path()), ("b", b.path())]);
    for name in ["design.figcraft", "not-yet-saved.figcraft"] {
        let b_doc = b.path().join(name).to_string_lossy().into_owned();
        assert_eq!(
            refused_code(&b_doc, "a", &registry),
            CODE_PATH_VALIDATION_FAILED,
            "B's doc under A's id: {name}"
        );
        assert_eq!(
            resolve_doc_path(&b_doc, "b", Some(&registry)),
            Ok(native(&format!("{b_plain}/{name}"))),
            "B's doc under B's own id: {name}"
        );
    }
}

#[test]
fn a_project_the_registry_does_not_hold_opens_nothing() {
    let (dir, _) = project();
    let registry = ProjectRegistry::with_roots_for_test(&[("a", dir.path())]);
    let doc = dir.path().join("design.figcraft");
    let refused = resolve_doc_path(&doc.to_string_lossy(), "unknown", Some(&registry)).unwrap_err();
    assert_eq!(refused.code, CODE_PATH_VALIDATION_FAILED);
    assert!(refused.message.contains("no registered root"), "{refused}");
}

#[test]
fn a_traversal_out_of_the_root_is_refused() {
    let (dir, _) = project();
    let registry = ProjectRegistry::with_roots_for_test(&[("a", dir.path())]);
    let doc = dir.path().join("..").join("design.figcraft");
    assert_eq!(
        refused_code(&doc.to_string_lossy(), "a", &registry),
        CODE_PATH_VALIDATION_FAILED
    );
}

/// The Windows project registry stores `\\?\D:\…` roots, and the renderer
/// echoes them back joined with `/` or with every `\` turned into `/`.
#[cfg(windows)]
#[test]
fn a_verbatim_root_in_any_slash_style_opens_only_its_own_docs() {
    let verbatim_of = |dir: &tempfile::TempDir| {
        dir.path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    let (a, a_plain) = project();
    let (b, _) = project();
    let (elsewhere, _) = project();
    let a_verbatim = verbatim_of(&a);
    let b_verbatim = verbatim_of(&b);
    assert!(a_verbatim.starts_with(r"\\?\"), "{a_verbatim}");
    let registry = ProjectRegistry::with_roots_for_test(&[
        ("a", std::path::Path::new(&a_verbatim)),
        ("b", std::path::Path::new(&b_verbatim)),
    ]);
    let spellings = |verbatim: &str, name: &str| {
        let forward = verbatim.replace('\\', "/");
        [
            format!(r"{verbatim}\{name}"),
            format!("{verbatim}/{name}"),
            format!("{forward}/{name}"),
        ]
    };
    for name in ["design.figcraft", "new.figcraft"] {
        let expected = format!(r"{a_plain}\{name}");
        for doc in spellings(&a_verbatim, name) {
            assert_eq!(
                resolve_doc_path(&doc, "a", Some(&registry)).as_ref(),
                Ok(&expected),
                "{doc}"
            );
        }
        for doc in spellings(&b_verbatim, name) {
            assert_eq!(
                refused_code(&doc, "a", &registry),
                CODE_PATH_VALIDATION_FAILED,
                "B's verbatim doc under A's id: {doc}"
            );
        }
        for doc in spellings(&verbatim_of(&elsewhere), name) {
            assert_eq!(
                refused_code(&doc, "a", &registry),
                CODE_PATH_VALIDATION_FAILED,
                "a verbatim doc outside every root: {doc}"
            );
        }
    }
}
