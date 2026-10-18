//! WebView2 file choosers on agent tabs: CDP intercepts every one and
//! cancels it, so the native dialog never opens and the page sees a
//! cancel. Uploads set the input's files through `DOM.setFileInputFiles`
//! instead.

use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use windows_core::HSTRING;

/// False when a command could not be issued. A runtime that refuses one
/// is logged: the interception is then missing on that tab.
pub fn attach(core: &ICoreWebView2) -> bool {
    // The renderer consults the interception only while the Page domain is
    // enabled on the session; without it the native chooser opens.
    call(core, "Page.enable", "{}")
        && call(
            core,
            "Page.setInterceptFileChooserDialog",
            r#"{"enabled":true,"cancel":true}"#,
        )
}

fn call(core: &ICoreWebView2, method: &'static str, params: &str) -> bool {
    let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |hr, _| {
        if hr.is_err() {
            log::warn!("[browser-tab] WebView2 refused {method}; page file choosers are not intercepted on an agent tab");
        }
        Ok(())
    }));
    // SAFETY: runs on the UI thread that owns `core`, inside `with_webview`;
    // WebView2 addrefs the completion handler for the round trip.
    unsafe {
        core.CallDevToolsProtocolMethod(&HSTRING::from(method), &HSTRING::from(params), &handler)
    }
    .is_ok()
}
