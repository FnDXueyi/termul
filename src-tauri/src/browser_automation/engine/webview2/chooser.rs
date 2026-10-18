//! WebView2 file choosers on agent tabs: CDP intercepts every one and
//! cancels it, so the native dialog never opens and the page sees a
//! cancel. Uploads set the input's files through `DOM.setFileInputFiles`
//! instead.

use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use windows_core::HSTRING;

/// False when the command could not be issued. A runtime that refuses it
/// is logged: the interception is then missing on that tab.
pub fn attach(core: &ICoreWebView2) -> bool {
    let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|hr, _| {
        if hr.is_err() {
            log::warn!("[browser-tab] WebView2 refused to intercept file choosers on an agent tab");
        }
        Ok(())
    }));
    // SAFETY: runs on the UI thread that owns `core`, inside `with_webview`;
    // WebView2 addrefs the completion handler for the round trip.
    unsafe {
        core.CallDevToolsProtocolMethod(
            &HSTRING::from("Page.setInterceptFileChooserDialog"),
            &HSTRING::from(r#"{"enabled":true,"cancel":true}"#),
            &handler,
        )
    }
    .is_ok()
}
