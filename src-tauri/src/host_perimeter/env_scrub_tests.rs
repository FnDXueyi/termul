use std::collections::HashMap;
use std::ffi::OsString;

use super::env_scrub::{plan, EnvScrub};

fn env(pairs: &[(&str, &str)]) -> HashMap<String, OsString> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), OsString::from(value)))
        .collect()
}

fn plan_for(pairs: &[(&str, &str)]) -> Vec<EnvScrub> {
    let vars = env(pairs);
    plan(|key| vars.get(key).cloned())
}

#[test]
fn clean_environment_needs_no_change() {
    assert_eq!(
        plan_for(&[("PATH", "/usr/bin"), ("RUST_LOG", "debug")]),
        vec![]
    );
}

#[test]
fn webview2_debug_port_is_stripped_and_other_switches_kept() {
    assert_eq!(
        plan_for(&[(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--disable-gpu --remote-debugging-port=9222 --lang=en"
        )]),
        vec![EnvScrub::Replace(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--disable-gpu --lang=en".to_string()
        )]
    );
}

#[test]
fn webview2_variable_with_only_debug_switches_is_removed() {
    assert_eq!(
        plan_for(&[(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--remote-debugging-pipe /REMOTE-DEBUGGING-PORT=1 -remote-debugging-address=0.0.0.0 --remote-allow-origins=*"
        )]),
        vec![EnvScrub::Remove("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS")]
    );
}

#[test]
fn webview2_variable_without_debug_switches_is_untouched() {
    assert_eq!(
        plan_for(&[(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--disable-gpu  --lang=en"
        )]),
        vec![]
    );
}

#[test]
fn inspector_and_automation_variables_are_removed() {
    assert_eq!(
        plan_for(&[
            ("WEBKIT_INSPECTOR_SERVER", "127.0.0.1:9999"),
            ("WEBKIT_INSPECTOR_HTTP_SERVER", "127.0.0.1:9998"),
            ("WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER", "pipe"),
            ("TAURI_WEBVIEW_AUTOMATION", "true"),
        ]),
        vec![
            EnvScrub::Remove("WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER"),
            EnvScrub::Remove("WEBKIT_INSPECTOR_SERVER"),
            EnvScrub::Remove("WEBKIT_INSPECTOR_HTTP_SERVER"),
            EnvScrub::Remove("TAURI_WEBVIEW_AUTOMATION"),
        ]
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_webview2_variable_is_removed_whole() {
    use std::os::unix::ffi::OsStringExt;
    let value = OsString::from_vec(vec![0x2d, 0x2d, 0xff]);
    assert_eq!(
        plan(|key| (key == "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").then(|| value.clone())),
        vec![EnvScrub::Remove("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS")]
    );
}
