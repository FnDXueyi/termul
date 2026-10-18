use parking_lot::Mutex;
use serde_json::json;

use super::*;
use crate::browser_automation::engine::{CapabilityTable, EngineTraits};

#[test]
fn key_names_parse_to_named_keys_or_one_character() {
    assert_eq!(Key::parse("Enter"), Some(Key::Named(NamedKey::Enter)));
    assert_eq!(Key::parse("ArrowDown"), Some(Key::Named(NamedKey::ArrowDown)));
    assert_eq!(Key::parse("Space"), Some(Key::Named(NamedKey::Space)));
    assert_eq!(Key::parse(" "), Some(Key::Named(NamedKey::Space)));
    assert_eq!(Key::parse("é"), Some(Key::Char('é')));
    assert_eq!(Key::parse("Control+a"), None);
    assert_eq!(Key::parse(""), None);
    assert_eq!(Key::parse("\u{7}"), None);
}

#[test]
fn facts_parse_from_the_helper_reply() {
    let facts = parse_facts(json!({
        "state": "present", "role": "textbox", "name": "Email", "x": 10.5, "y": 20.0,
        "visible": true, "stable": false, "hit": true, "enabled": true, "kind": "text",
        "focusOnClick": true
    }))
    .unwrap();
    assert_eq!(
        facts,
        ActionabilityFacts::Present(ElementState {
            role: "textbox".into(),
            name: "Email".into(),
            point: Point { x: 10.5, y: 20.0 },
            visible: true,
            stable: false,
            hit: true,
            enabled: true,
            kind: ControlKind::Text,
            focus_on_click: true,
        })
    );
    assert_eq!(parse_facts(json!({ "state": "detached" })).unwrap(), ActionabilityFacts::Detached);
    let with_extra = json!({
        "state": "present", "role": "textbox", "name": "Email", "x": 10.5, "y": 20.0,
        "visible": true, "stable": true, "hit": true, "enabled": true, "kind": "text",
        "focusOnClick": true, "forged": true
    });
    let bad_kind = json!({
        "state": "present", "role": "textbox", "name": "Email", "x": 10.5, "y": 20.0,
        "visible": true, "stable": true, "hit": true, "enabled": true, "kind": "button",
        "focusOnClick": true
    });
    for bad in [
        json!({}),
        json!({ "state": "present", "role": "x" }),
        json!("stale_ref"),
        json!({ "error": "stale_ref" }),
        with_extra,
        bad_kind,
    ] {
        assert_eq!(parse_facts(bad.clone()).unwrap_err().kind, EngineErrorKind::Malformed, "{bad}");
    }
}

struct Inner(EngineFacts);

impl BrowserEngine for Inner {
    fn facts(&self) -> &EngineFacts {
        &self.0
    }
    fn evaluate<'a>(&'a self, _tab: &'a str, script: Script) -> EngineFuture<'a, Value> {
        assert_eq!(script, assets::facts("html>body>button:nth-of-type(1)"));
        Box::pin(async { Ok(json!({ "state": "detached" })) })
    }
    fn pointer_click<'a>(&'a self, _tab: &'a str, _at: Point, _mode: InputMode) -> EngineFuture<'a, ()> {
        unreachable!("the layer owns input")
    }
    fn screenshot<'a>(&'a self, _tab: &'a str) -> EngineFuture<'a, Vec<u8>> {
        unreachable!()
    }
}

#[derive(Default)]
struct Recorded(Mutex<Vec<String>>);

const MAC_LIKE: &[(CapabilityId, Support)] = &[
    (CapabilityId::PointerClick, Support::Trusted),
    (CapabilityId::TrustedKey, Support::Trusted),
    (CapabilityId::PointerMove, Support::Synthetic),
];

impl NativeInput for Recorded {
    fn tiers(&self) -> &'static [(CapabilityId, Support)] {
        MAC_LIKE
    }
    fn press_support(&self) -> Option<Support> {
        Some(Support::Synthetic)
    }
    fn click<'a>(&'a self, _tab: &'a str, at: Point) -> EngineFuture<'a, ()> {
        self.0.lock().push(format!("click {} {}", at.x, at.y));
        Box::pin(async { Ok(()) })
    }
    fn move_to<'a>(&'a self, _tab: &'a str, _at: Point) -> EngineFuture<'a, ()> {
        self.0.lock().push("move".into());
        Box::pin(async { Ok(()) })
    }
    fn key<'a>(&'a self, _tab: &'a str, input: KeyInput<'a>) -> EngineFuture<'a, ()> {
        self.0.lock().push(format!("{input:?}"));
        Box::pin(async { Ok(()) })
    }
}

fn layered() -> (WithNativeInput<Inner>, Arc<Recorded>) {
    let inner = Inner(EngineFacts {
        capabilities: CapabilityTable::unavailable("eval only")
            .with(CapabilityId::IsolatedEval, Support::Trusted)
            .with(CapabilityId::PointerClick, Support::Synthetic),
        traits: EngineTraits {
            isolated_world: true,
            background_render: false,
            network_observe: false,
        },
    });
    let input = Arc::new(Recorded::default());
    (WithNativeInput::new(inner, input.clone()), input)
}

#[tokio::test]
async fn the_layer_raises_declared_tiers_and_refuses_trusted_input_elsewhere() {
    let (engine, input) = layered();
    let caps = &engine.facts().capabilities;
    assert_eq!(caps.get(CapabilityId::PointerClick), Support::Trusted);
    assert_eq!(caps.get(CapabilityId::PointerMove), Support::Synthetic);
    assert_eq!(caps.get(CapabilityId::IsolatedEval), Support::Trusted, "eval tier is the inner engine's");
    assert!(matches!(caps.get(CapabilityId::Wheel), Support::Unavailable(_)));
    assert_eq!(engine.key_support(KeyKind::Text), Support::Trusted);
    assert_eq!(engine.key_support(KeyKind::Press), Support::Synthetic);

    let at = Point { x: 1.0, y: 2.0 };
    engine.pointer_click("agent-1", at, InputMode::Trusted).await.unwrap();
    engine.key("agent-1", KeyInput::Text("hi"), InputMode::Trusted).await.unwrap();
    let refused = [
        engine.pointer_click("agent-1", at, InputMode::Synthetic).await,
        engine.pointer_move("agent-1", at, InputMode::Trusted).await,
        engine.wheel("agent-1", at, (0.0, 10.0), InputMode::Trusted).await,
        engine.key("agent-1", KeyInput::Press(Key::Named(NamedKey::Enter)), InputMode::Trusted).await,
    ];
    for result in refused {
        assert_eq!(result.unwrap_err().kind, EngineErrorKind::Unsupported);
    }
    assert_eq!(input.0.lock().as_slice(), ["click 1 2", "Text(\"hi\")"]);
    assert_eq!(
        engine.element_facts("agent-1", "html>body>button:nth-of-type(1)").await.unwrap(),
        ActionabilityFacts::Detached,
        "facts come through the inner engine's eval"
    );
}
