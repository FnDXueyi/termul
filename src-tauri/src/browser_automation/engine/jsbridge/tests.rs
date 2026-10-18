use super::*;

#[test]
fn wrap_eval_posts_the_completion_back_with_json_escaped_tab_and_nonce() {
    let script = wrap_eval("agent-\"x\"", "n-1", "document.title");
    assert!(script.contains("inv('browser_agent_eval_result'"), "{script}");
    assert!(script.contains(r#"tabId:"agent-\"x\"""#), "tab id must be a JSON string: {script}");
    assert!(script.contains(r#"nonce:"n-1""#), "{script}");
    assert!(script.contains("return (document.title);"), "{script}");
}

#[test]
fn bridge_capabilities_are_all_synthetic_and_screenshot_is_unavailable() {
    let table = capability_table();
    for id in [
        CapabilityId::IsolatedEval,
        CapabilityId::PointerClick,
        CapabilityId::TrustedKey,
        CapabilityId::PointerMove,
        CapabilityId::Wheel,
    ] {
        assert_eq!(table.get(id), Support::Synthetic, "{id:?}");
    }
    assert_eq!(
        table.get(CapabilityId::Screenshot),
        Support::Unavailable("screenshot requires the WebView2 engine (Windows)")
    );
}
