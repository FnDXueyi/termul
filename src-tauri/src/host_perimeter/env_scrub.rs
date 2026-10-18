//! Engine debug and automation switches a process can inherit from its
//! environment. Each one makes a webview engine listen on a local port or
//! pipe that any same-user process could attach to and drive the app UI or
//! a tab through.

use std::ffi::OsString;

/// Variables removed outright.
const REMOVED: &[&str] = &[
    // WebView2 script-debugger pipe.
    "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
    // WebKitGTK remote inspector servers.
    "WEBKIT_INSPECTOR_SERVER",
    "WEBKIT_INSPECTOR_HTTP_SERVER",
    // Makes Tauri enable WebKitGTK automation mode.
    "TAURI_WEBVIEW_AUTOMATION",
];

/// Chromium switches WebView2 reads from this variable. Only the debugging
/// switches are removed; other switches the user set stay.
const WEBVIEW2_ARGS: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";

const DEBUG_SWITCHES: &[&str] = &["remote-debugging", "remote-allow-origins"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnvScrub {
    Remove(&'static str),
    Replace(&'static str, String),
}

impl EnvScrub {
    pub(crate) fn key(&self) -> &'static str {
        match self {
            Self::Remove(key) | Self::Replace(key, _) => key,
        }
    }
}

/// What to change, given a reader for the current environment.
pub(crate) fn plan(read: impl Fn(&str) -> Option<OsString>) -> Vec<EnvScrub> {
    let mut actions: Vec<EnvScrub> = REMOVED
        .iter()
        .filter(|key| read(key).is_some())
        .map(|key| EnvScrub::Remove(key))
        .collect();
    if let Some(value) = read(WEBVIEW2_ARGS) {
        // A value that is not UTF-8 cannot be split safely, so it goes whole.
        match value.into_string() {
            Err(_) => actions.push(EnvScrub::Remove(WEBVIEW2_ARGS)),
            Ok(text) => {
                let kept: Vec<&str> = text
                    .split_whitespace()
                    .filter(|arg| !is_debug_switch(arg))
                    .collect();
                if kept.len() != text.split_whitespace().count() {
                    actions.push(if kept.is_empty() {
                        EnvScrub::Remove(WEBVIEW2_ARGS)
                    } else {
                        EnvScrub::Replace(WEBVIEW2_ARGS, kept.join(" "))
                    });
                }
            }
        }
    }
    actions
}

/// Chromium accepts `--`, `-` and, on Windows, `/` as switch prefixes, and
/// matches switch names case-insensitively on Windows.
fn is_debug_switch(arg: &str) -> bool {
    let name = arg
        .strip_prefix("--")
        .or_else(|| arg.strip_prefix('-'))
        .or_else(|| arg.strip_prefix('/'));
    name.is_some_and(|name| {
        let name = name.to_ascii_lowercase();
        DEBUG_SWITCHES.iter().any(|switch| name.starts_with(switch))
    })
}
