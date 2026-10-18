use super::canvas::validate_canvas_doc_path;

/// A project folder whose root is spelled the way the Windows project registry
/// stores it (`\\?\D:\…`), plus that root in plain form.
#[cfg(windows)]
fn verbatim_project() -> (tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("design.figcraft"), b"{}").expect("write doc");
    let verbatim = dir
        .path()
        .canonicalize()
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned();
    assert!(verbatim.starts_with(r"\\?\"), "{verbatim}");
    let plain = verbatim.trim_start_matches(r"\\?\").to_string();
    (dir, verbatim, plain)
}

#[cfg(windows)]
#[test]
fn a_verbatim_root_in_any_slash_style_opens_the_plain_doc_path() {
    let (_dir, verbatim, plain) = verbatim_project();
    let forward = verbatim.replace('\\', "/");
    for name in ["design.figcraft", "new.figcraft"] {
        let expected = format!(r"{plain}\{name}");
        for doc in [
            format!(r"{verbatim}\{name}"),
            format!("{verbatim}/{name}"),
            format!("{forward}/{name}"),
        ] {
            assert_eq!(
                validate_canvas_doc_path(&doc).as_deref(),
                Ok(expected.as_str()),
                "{doc}"
            );
        }
    }
}
