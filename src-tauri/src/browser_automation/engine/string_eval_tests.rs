use super::*;
use serde_json::json;

#[test]
fn a_completed_call_yields_the_helpers_json_value() {
    assert_eq!(
        parse(Ok(r#"{"rect":{"cx":1.5},"role":"button"}"#.into())),
        Ok(json!({ "rect": { "cx": 1.5 }, "role": "button" }))
    );
    assert_eq!(parse(Ok("null".into())), Ok(Value::Null));
}

#[test]
fn engine_failures_and_unusable_text_are_terminal() {
    assert_eq!(
        parse(Err(EngineErrorKind::PageException)),
        Err(EngineError::new(EngineErrorKind::PageException, false))
    );
    assert_eq!(
        parse(Ok("undefined".into())),
        Err(EngineError::new(EngineErrorKind::Malformed, false))
    );
}

#[test]
fn webkit_engines_declare_isolated_eval_and_synthetic_input_only() {
    let facts = isolated_facts();
    assert!(facts.traits.isolated_world);
    assert_eq!(facts.capabilities.get(CapabilityId::IsolatedEval), Support::Trusted);
    for id in [
        CapabilityId::PointerClick,
        CapabilityId::TrustedKey,
        CapabilityId::PointerMove,
        CapabilityId::Wheel,
    ] {
        assert_eq!(facts.capabilities.get(id), Support::Synthetic, "{id:?}");
    }
    for id in [CapabilityId::Screenshot, CapabilityId::ScriptMessage, CapabilityId::Drag] {
        assert!(matches!(facts.capabilities.get(id), Support::Unavailable(_)), "{id:?}");
    }

    let old = no_world_facts("needs a newer OS");
    assert!(!old.traits.isolated_world);
    assert_eq!(
        old.capabilities.get(CapabilityId::IsolatedEval),
        Support::Unavailable("needs a newer OS")
    );
}

#[test]
fn the_webkit_call_body_is_static_and_the_parameters_ride_one_string() {
    let hostile = "\"); window.leak = 1; (\"";
    let script = super::super::assets::fill("html>body:nth-of-type(1)", hostile);
    let body = call_body(&script);
    assert!(!body.contains(hostile));
    assert!(body.contains(&format!("JSON.parse({PARAMS_ARG})")));
    assert!(body.contains(script.helper.body));
    assert_eq!(
        serde_json::from_str::<Value>(&params_json(&script)).unwrap(),
        json!({ "path": "html>body:nth-of-type(1)", "value": hostile })
    );
}
