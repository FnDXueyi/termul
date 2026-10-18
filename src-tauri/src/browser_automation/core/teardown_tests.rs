use super::*;

#[test]
fn a_session_id_never_names_a_path_outside_its_root() {
    let root = Path::new("root");
    assert_eq!(session_dir(root, "sess-1_a"), root.join("sess-1_a"));
    assert_eq!(session_dir(root, "../../etc"), root.join("etc"));
    assert_eq!(session_dir(root, "C:\\Windows"), root.join("CWindows"));
    assert_eq!(session_dir(root, "../"), root.join("_"));
}

#[test]
fn the_startup_sweep_deletes_every_session_directory_and_nothing_else() {
    let root = tempfile::tempdir().unwrap();
    for session in ["s1", "s2"] {
        let profile = root.path().join(session).join("profile");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("Cookies"), b"x").unwrap();
    }
    std::fs::write(root.path().join("not-a-session"), b"x").unwrap();

    assert_eq!(sweep(root.path()), 2);
    let left: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, ["not-a-session"]);
    assert_eq!(sweep(&root.path().join("missing")), 0);
}

#[test]
fn deleting_a_missing_directory_is_done() {
    let root = tempfile::tempdir().unwrap();
    assert!(delete_dir(&root.path().join("never-created")));
}
