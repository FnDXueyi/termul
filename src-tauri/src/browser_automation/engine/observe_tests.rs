use super::*;

#[test]
fn urls_keep_origin_and_path_only() {
    for (raw, want) in [
        ("https://user:pw@example.com:8443/a/b?token=s3cret#frag", "https://example.com:8443/a/b"),
        ("http://127.0.0.1:5191/api/items?page=2", "http://127.0.0.1:5191/api/items"),
        ("wss://example.com/socket?auth=x", "wss://example.com/socket"),
        ("data:text/plain;base64,c2VjcmV0", "data:"),
        ("blob:https://example.com/7f1c", "blob:"),
        ("not a url", ""),
    ] {
        assert_eq!(strip_url(raw), want, "{raw}");
    }
}

#[test]
fn console_text_is_capped_on_a_char_boundary() {
    let long = "é".repeat(TEXT_MAX);
    let entry = ConsoleEntry::new(ConsoleLevel::Warn, &long, Some("https://example.com/app.js?v=1"));
    assert_eq!(entry.text.len(), TEXT_MAX);
    assert_eq!(entry.source.as_deref(), Some("https://example.com/app.js"));
    assert_eq!(ConsoleEntry::new(ConsoleLevel::Log, "x", Some("")).source, None);
}

#[test]
fn levels_map_console_methods_and_engine_levels() {
    for (name, want) in [
        ("log", ConsoleLevel::Log),
        ("debug", ConsoleLevel::Debug),
        ("verbose", ConsoleLevel::Debug),
        ("info", ConsoleLevel::Info),
        ("warning", ConsoleLevel::Warn),
        ("warn", ConsoleLevel::Warn),
        ("error", ConsoleLevel::Error),
        ("assert", ConsoleLevel::Error),
        ("table", ConsoleLevel::Log),
    ] {
        assert_eq!(ConsoleLevel::parse(name), want, "{name}");
    }
}

#[test]
fn a_redirect_ends_the_earlier_hop_with_its_status() {
    let mut t = NetworkTracker::default();
    assert_eq!(t.request("1", "GET", "http://h/redirect?x=1", Some("Document"), None, 1.0), None);
    let hop = t.request("1", "GET", "http://h/target", Some("Document"), Some(302), 1.25);
    assert_eq!(
        hop,
        Some(NetworkEntry {
            method: "GET".into(),
            url: "http://h/redirect".into(),
            status: Some(302),
            resource: Some("document".into()),
            duration_ms: Some(250),
            failed: false,
        })
    );
    t.response("1", 200, None);
    let done = t.finished("1", 1.5).unwrap();
    assert_eq!((done.url.as_str(), done.status, done.duration_ms), ("http://h/target", Some(200), Some(250)));
    assert_eq!(t.finished("1", 2.0), None);
}

#[test]
fn failures_and_unknown_ids() {
    let mut t = NetworkTracker::default();
    t.request("a", "POST", "https://h/api", None, None, 0.0);
    t.response("a", 500, Some("Fetch"));
    let failed = t.failed("a", 0.1).unwrap();
    assert!(failed.failed);
    assert_eq!((failed.status, failed.resource.as_deref()), (Some(500), Some("fetch")));
    assert_eq!(t.failed("zzz", 1.0), None);
    t.request("s1:9", "GET", "https://h/x", None, None, 0.0);
    t.forget_prefix("s1:");
    assert_eq!(t.finished("s1:9", 1.0), None);
}

#[test]
fn entries_serialize_without_absent_fields() {
    let entry = NetworkEntry {
        method: "GET".into(),
        url: "https://h/".into(),
        status: None,
        resource: None,
        duration_ms: None,
        failed: false,
    };
    assert_eq!(serde_json::to_value(&entry).unwrap(), serde_json::json!({ "method": "GET", "url": "https://h/" }));
}

#[test]
fn shim_batches_parse_leniently() {
    let pulled = serde_json::json!({
        "batches": [
            r#"[{"level":"warn","text":"low disk","source":"http://h/app.js?v=2"},{"level":"bogus","text":"plain"}]"#,
            "not json",
            42,
            r#"[{"level":"error"},{"text":"no level"}]"#
        ]
    });
    assert_eq!(
        parse_console_batches(&pulled),
        vec![
            ConsoleEntry::new(ConsoleLevel::Warn, "low disk", Some("http://h/app.js")),
            ConsoleEntry::new(ConsoleLevel::Log, "plain", None),
            ConsoleEntry::new(ConsoleLevel::Log, "no level", None),
        ]
    );
    assert!(parse_console_batches(&serde_json::json!({ "error": "x" })).is_empty());
}
