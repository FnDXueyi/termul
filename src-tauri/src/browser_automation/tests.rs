use super::*;

/// Alias for the shared test lock (see `TEST_HOST_LOCK`).
use super::TEST_HOST_LOCK as HOST_LOCK;
use serde_json::json;
use uuid::Uuid;

fn call(action: &str, args: Value) -> BrowserCall {
    BrowserCall {
        action: action.to_string(),
        args,
        element: None,
    }
}

#[test]
fn dispatch_without_host_fails_closed() {
    let _g = HOST_LOCK.lock().unwrap();
    clear_browser_host();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt
        .block_on(dispatch(
            "sess",
            "agent",
            call("navigate", json!({"url": "https://x"})),
        ))
        .expect_err("no host must fail closed");
    assert_eq!(err.code, ERR_CAPABILITY);
    // No pre-stamp: the parent boundary's `boundary_arg_keys` fallback is
    // the single place the action table is applied.
    assert!(
        err.arg_keys.is_empty(),
        "dispatch must not pre-stamp arg_keys"
    );
}

#[test]
fn hooks_without_host_are_noops() {
    let _g = HOST_LOCK.lock().unwrap();
    clear_browser_host();
    assert!(!tab_created("t1"));
    // Real nonces/request ids are `Uuid`s — keep the test values in the
    // same shape (and avoid hard-coded literal nonces).
    assert!(!eval_resolved(&Uuid::new_v4().to_string(), true, None));
    assert!(!consent_responded(&Uuid::new_v4().to_string(), true));
    tab_closed("t1");
    tab_navigated("t1");
    session_ended("s1");
}

/// A registered host routes dispatch end-to-end; `session_ended` reaches
/// `end_session` so tabs/consent are reclaimed.
#[test]
fn registered_host_routes_dispatch_and_session_end() {
    let _g = HOST_LOCK.lock().unwrap();
    clear_browser_host();
    struct Stub {
        ended: parking_lot::Mutex<Vec<String>>,
    }
    impl BrowserHost for Stub {
        fn execute<'a>(
            &'a self,
            session_id: &'a str,
            _agent_id: &'a str,
            call: BrowserCall,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Success, BrowserError>> + Send + 'a>,
        > {
            Box::pin(async move {
                Ok(Success {
                    data: Some(json!({ "sid": session_id, "action": call.action })),
                    ..Success::default()
                })
            })
        }
        fn end_session(&self, session_id: &str) {
            self.ended.lock().push(session_id.to_string());
        }
        fn notify_tab_created(&self, _tab_id: &str) -> bool {
            false
        }
        fn notify_tab_closed(&self, _tab_id: &str) {}
        fn notify_tab_navigated(&self, _tab_id: &str) {}
        fn resolve_consent(&self, _request_id: &str, _allowed: bool) -> bool {
            false
        }
        fn resolve_eval(&self, _nonce: &str, _ok: bool, _value: Option<String>) -> bool {
            false
        }
        fn engine_facts(&self) -> engine::EngineFacts {
            engine::EngineFacts::all_trusted()
        }
    }
    let stub = Arc::new(Stub {
        ended: parking_lot::Mutex::new(Vec::new()),
    });
    set_browser_host(stub.clone());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let out = rt
        .block_on(dispatch("sess-1", "agent", call("list_tabs", Value::Null)))
        .expect("stub host resolves");
    let data = out.data.expect("stub data");
    assert_eq!(data["sid"], "sess-1");
    assert_eq!(data["action"], "list_tabs");
    session_ended("sess-1");
    assert_eq!(stub.ended.lock().as_slice(), &["sess-1".to_string()]);
    clear_browser_host();
}

#[test]
fn browser_call_deserializes_action_args_element() {
    let c: BrowserCall = serde_json::from_value(json!({
        "action": "click",
        "args": {"ref": "@e2"},
        "element": "the login button"
    }))
    .unwrap();
    assert_eq!(c.action, "click");
    assert_eq!(c.args["ref"], "@e2");
    assert_eq!(c.element.as_deref(), Some("the login button"));

    let bare: BrowserCall = serde_json::from_value(json!({"action": "snapshot"})).unwrap();
    assert!(bare.args.is_null());
    assert!(bare.element.is_none());
}

/// Dependency direction: the core and the shared CDP module never touch
/// platform crates or per-OS engine code, and adapters never reach into
/// the core.
#[test]
fn layering_keeps_platform_code_inside_engine_adapters() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/browser_automation");
    let platform_markers = [
        "cfg(target_os",
        "cfg(windows",
        "cfg(unix",
        "cfg(not(target_os",
        "webview2_com",
        "windows_core",
        "windows::",
        "webkit2gtk",
        "gtk::",
        "objc2",
        "PlatformWebview",
        "with_webview",
    ];
    let adapter_markers = ["browser_automation::core", "super::super::core"];
    let mut violations = Vec::new();
    let mut scanned = 0;
    for (dir, markers) in [
        ("core", &platform_markers[..]),
        ("engine/cdp", &platform_markers[..]),
        ("engine/webview2", &adapter_markers[..]),
        ("engine/jsbridge", &adapter_markers[..]),
    ] {
        for entry in std::fs::read_dir(root.join(dir)).expect("layer dir exists") {
            let path = entry.expect("dir entry").path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            scanned += 1;
            let text = std::fs::read_to_string(&path).expect("read source");
            for marker in markers {
                if text.contains(marker) {
                    violations.push(format!("{} mentions {marker}", path.display()));
                }
            }
        }
    }
    assert!(scanned >= 8, "scanned only {scanned} files");
    assert_eq!(violations, Vec::<String>::new());
}
