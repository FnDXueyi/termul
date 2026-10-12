use super::*;

/// Minimal std-only temp dir (reuses the repo's pid+nanos pattern — no
/// `tempfile` dev-dep).
fn tempdir_like(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!(
        "termul-store-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&p).expect("create tempdir");
    p
}

fn cleanup(p: &PathBuf) {
    let _ = std::fs::remove_dir_all(p);
}

#[test]
fn missing_file_opens_empty_and_round_trips() {
    let dir = tempdir_like("roundtrip");
    let file = dir.join("store.json");
    let store = WebStore::open(file.clone()).unwrap();
    assert_eq!(store.read("missing").unwrap(), None);

    store
        .write("terminals/p1", serde_json::json!({ "active": "t1" }), None)
        .unwrap();
    store
        .write("settings", serde_json::json!({ "theme": "dark" }), None)
        .unwrap();
    assert_eq!(
        store.read("settings").unwrap(),
        Some(serde_json::json!({ "theme": "dark" }))
    );
    assert_eq!(
        store.read("terminals/p1").unwrap(),
        Some(serde_json::json!({ "active": "t1" }))
    );

    // A second open (simulating a restart) reloads the persisted map.
    let reloaded = WebStore::open(file.clone()).unwrap();
    assert_eq!(
        reloaded.read("settings").unwrap(),
        Some(serde_json::json!({ "theme": "dark" }))
    );

    // delete removes + persists.
    assert!(reloaded.delete("settings").unwrap());
    assert_eq!(reloaded.read("settings").unwrap(), None);
    assert!(!reloaded.delete("settings").unwrap());
    let reloaded_again = WebStore::open(file.clone()).unwrap();
    assert_eq!(reloaded_again.read("settings").unwrap(), None);
    assert_eq!(
        reloaded_again.read("terminals/p1").unwrap(),
        Some(serde_json::json!({ "active": "t1" }))
    );
    cleanup(&dir);
}

#[test]
fn write_replaces_existing_value_and_no_temp_leaks() {
    let dir = tempdir_like("replace");
    let file = dir.join("store.json");
    let store = WebStore::open(file.clone()).unwrap();
    store.write("k", serde_json::json!(1), None).unwrap();
    store.write("k", serde_json::json!(2), None).unwrap();
    assert_eq!(store.read("k").unwrap(), Some(serde_json::json!(2)));

    let temps: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.ends_with(".tmp"))
                .unwrap_or(false)
        })
        .collect();
    assert!(temps.is_empty(), "no lingering temp files: {temps:?}");
    cleanup(&dir);
}

#[test]
fn corrupt_file_is_backed_up_and_treated_as_empty() {
    let dir = tempdir_like("corrupt");
    let file = dir.join("store.json");
    std::fs::write(&file, "{ not valid json").expect("write garbage");

    let store = WebStore::open(file.clone()).unwrap();
    assert_eq!(store.read("anything").unwrap(), None);

    let backups: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("store.json.corrupt-") && n.ends_with(".bak"))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(backups.len(), 1, "exactly one corrupt backup");

    // A write after recovery recreates a valid file.
    store.write("k", serde_json::json!("v"), None).unwrap();
    let reloaded = WebStore::open(file).unwrap();
    assert_eq!(reloaded.read("k").unwrap(), Some(serde_json::json!("v")));
    cleanup(&dir);
}

fn backups_with_prefix(dir: &PathBuf, prefix: &str) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with(prefix) && n.ends_with(".bak"))
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect()
}

#[test]
fn writes_are_stored_in_the_schema_v1_envelope() {
    let dir = tempdir_like("envelope");
    let file = dir.join("store.json");
    let store = WebStore::open(file.clone()).unwrap();
    store.write("k", serde_json::json!({ "a": 1 }), None).unwrap();

    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(
        on_disk,
        serde_json::json!({ "schemaVersion": 1, "data": { "k": { "a": 1 } } })
    );
    assert_eq!(store.path(), file.as_path());
    cleanup(&dir);
}

#[test]
fn legacy_bare_map_is_backed_up_then_migrated_to_the_envelope() {
    let dir = tempdir_like("legacy");
    let file = dir.join("store.json");
    let legacy = br#"{"settings":{"theme":"dark"},"terminals/p1":[1,2]}"#;
    std::fs::write(&file, legacy).unwrap();

    let store = WebStore::open(file.clone()).unwrap();
    assert_eq!(
        store.read("settings").unwrap(),
        Some(serde_json::json!({ "theme": "dark" }))
    );

    // Original bytes preserved verbatim in exactly one pre-v1 backup.
    let backups = backups_with_prefix(&dir, "store.json.pre-v1-");
    assert_eq!(backups.len(), 1, "exactly one pre-migration backup");
    assert_eq!(std::fs::read(&backups[0]).unwrap(), legacy);

    // The file itself is already in the envelope (rewritten on open) and a
    // restart reloads it without taking another backup.
    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(on_disk["schemaVersion"], serde_json::json!(1));
    assert_eq!(on_disk["data"]["settings"], serde_json::json!({ "theme": "dark" }));
    let reloaded = WebStore::open(file.clone()).unwrap();
    assert_eq!(
        reloaded.read("terminals/p1").unwrap(),
        Some(serde_json::json!([1, 2]))
    );
    assert_eq!(backups_with_prefix(&dir, "store.json.pre-v1-").len(), 1);
    cleanup(&dir);
}

#[test]
fn newer_schema_is_refused_and_the_file_is_left_byte_identical() {
    let dir = tempdir_like("newer");
    let file = dir.join("store.json");
    let future = br#"{"schemaVersion":2,"data":{"k":1},"newThing":{"x":[1]}}"#;
    std::fs::write(&file, future).unwrap();

    let error = match WebStore::open(file.clone()) {
        Ok(_) => panic!("a newer schema must be refused"),
        Err(error) => error,
    };
    match &error {
        WebStoreError::NewerSchema(newer) => {
            assert_eq!((newer.found, newer.supported), (2, STORE_SCHEMA_VERSION));
            assert_eq!(newer.path, file);
        }
        other => panic!("expected NewerSchema, got {other:?}"),
    }
    assert!(error.to_string().contains("store.json"));
    assert_eq!(std::fs::read(&file).unwrap(), future);
    assert!(
        backups_with_prefix(&dir, "store.json.").is_empty(),
        "refusing must not create backups"
    );
    cleanup(&dir);
}

#[test]
fn unrecognised_envelope_is_treated_as_corrupt() {
    let dir = tempdir_like("bad-envelope");
    let file = dir.join("store.json");
    std::fs::write(&file, br#"{"schemaVersion":1,"data":[1,2]}"#).unwrap();

    let store = WebStore::open(file.clone()).unwrap();
    assert_eq!(store.read("k").unwrap(), None);
    assert_eq!(backups_with_prefix(&dir, "store.json.corrupt-").len(), 1);
    cleanup(&dir);
}
