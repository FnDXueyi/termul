//! CDP console, network and frame-session events, parsed into the
//! engine's observation entries. Engine-neutral: the transport that
//! delivers the events lives in the adapter.
//!
//! Every session (the page and each out-of-process frame it auto-attaches)
//! gets the same domains enabled, and auto-attach on a frame session
//! reaches the frames nested in it.

use serde_json::{json, Value};

use super::CdpMethod;
use crate::browser_automation::engine::observe::{ConsoleEntry, ConsoleLevel, NetworkEntry, NetworkTracker, TEXT_MAX};

/// The events an observing adapter subscribes to.
pub const EVENTS: [&str; 9] = [
    "Runtime.consoleAPICalled",
    "Runtime.exceptionThrown",
    "Log.entryAdded",
    "Network.requestWillBeSent",
    "Network.responseReceived",
    "Network.loadingFinished",
    "Network.loadingFailed",
    "Target.attachedToTarget",
    "Target.detachedFromTarget",
];

/// What to enable on a new session, in order. `Runtime.enable` also
/// replays console messages logged before it, and the network domain
/// keeps no bodies.
pub fn enable_calls() -> [(CdpMethod, Value); 4] {
    [
        (CdpMethod::RuntimeEnable, json!({})),
        (CdpMethod::LogEnable, json!({})),
        (
            CdpMethod::NetworkEnable,
            json!({ "maxTotalBufferSize": 0, "maxResourceBufferSize": 0, "maxPostDataSize": 0 }),
        ),
        (
            CdpMethod::TargetSetAutoAttach,
            json!({ "autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true }),
        ),
    ]
}

/// One event, as an adapter routes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    Console(ConsoleEntry),
    Network(NetworkEntry),
    /// An out-of-process frame got its own session.
    FrameAttached { session: String, frame: String },
    Detached { session: String },
    None,
}

/// Parse `params` of event `method`, received on `session` (empty for the
/// page). Network request ids are scoped to their session in `tracker`.
pub fn parse(tracker: &mut NetworkTracker, session: &str, method: &str, params: &Value) -> Observed {
    let str_at = |pointer: &str| params.pointer(pointer).and_then(Value::as_str);
    let id = || format!("{session}:{}", str_at("/requestId").unwrap_or_default());
    let at = params.get("timestamp").and_then(Value::as_f64).unwrap_or(0.0);
    let network = |entry: Option<NetworkEntry>| entry.map_or(Observed::None, Observed::Network);
    match method {
        "Runtime.consoleAPICalled" => {
            let level = ConsoleLevel::parse(str_at("/type").unwrap_or("log"));
            let args = params.get("args").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
            let text: Vec<String> = args.iter().map(remote_text).collect();
            Observed::Console(ConsoleEntry::new(level, &text.join(" "), str_at("/stackTrace/callFrames/0/url")))
        }
        "Runtime.exceptionThrown" => {
            let text = str_at("/exceptionDetails/exception/description")
                .or_else(|| str_at("/exceptionDetails/text"))
                .unwrap_or("Uncaught exception");
            Observed::Console(ConsoleEntry::new(ConsoleLevel::Error, text, str_at("/exceptionDetails/url")))
        }
        "Log.entryAdded" => Observed::Console(ConsoleEntry::new(
            ConsoleLevel::parse(str_at("/entry/level").unwrap_or("info")),
            str_at("/entry/text").unwrap_or_default(),
            str_at("/entry/url"),
        )),
        "Network.requestWillBeSent" => {
            let redirect = params
                .pointer("/redirectResponse/status")
                .and_then(Value::as_u64)
                .and_then(|s| u16::try_from(s).ok());
            network(tracker.request(
                &id(),
                str_at("/request/method").unwrap_or("GET"),
                str_at("/request/url").unwrap_or_default(),
                str_at("/type"),
                redirect,
                at,
            ))
        }
        "Network.responseReceived" => {
            if let Some(status) = params.pointer("/response/status").and_then(Value::as_u64) {
                tracker.response(&id(), u16::try_from(status).unwrap_or(0), str_at("/type"));
            }
            Observed::None
        }
        "Network.loadingFinished" => network(tracker.finished(&id(), at)),
        "Network.loadingFailed" => network(tracker.failed(&id(), at)),
        "Target.attachedToTarget" => match (str_at("/sessionId"), str_at("/targetInfo/targetId"), str_at("/targetInfo/type")) {
            (Some(session), Some(frame), Some("iframe")) => Observed::FrameAttached {
                session: session.to_string(),
                frame: frame.to_string(),
            },
            _ => Observed::None,
        },
        "Target.detachedFromTarget" => match str_at("/sessionId") {
            Some(gone) => {
                tracker.forget_prefix(&format!("{gone}:"));
                Observed::Detached { session: gone.to_string() }
            }
            None => Observed::None,
        },
        _ => Observed::None,
    }
}

/// A console argument as text: a primitive's value, else the engine's
/// own preview of the object.
fn remote_text(arg: &Value) -> String {
    match arg.get("value") {
        Some(Value::String(s)) => return s.chars().take(TEXT_MAX).collect(),
        Some(v) if !v.is_null() || arg.get("subtype").and_then(Value::as_str) == Some("null") => {
            return v.to_string();
        }
        _ => {}
    }
    ["unserializableValue", "description", "type"]
        .iter()
        .find_map(|key| arg.get(*key).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
#[path = "observe_tests.rs"]
mod tests;
