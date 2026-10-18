use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, CallDevToolsProtocolMethodCompletedHandler,
    CreateCoreWebView2ControllerCompletedHandler, DevToolsProtocolEventReceivedEventHandler,
};
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use windows::Win32::Foundation::{HWND, RECT};

thread_local! {
    static CORES: RefCell<HashMap<String, (ICoreWebView2Controller, ICoreWebView2)>> =
        RefCell::new(HashMap::new());
    static RECEIVERS: RefCell<Vec<ICoreWebView2DevToolsProtocolEventReceiver>> =
        const { RefCell::new(Vec::new()) };
}

pub struct SendEnv(pub ICoreWebView2Environment);
unsafe impl Send for SendEnv {}

pub async fn on_main<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (tx, rx) = oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .expect("run_on_main_thread");
    rx.await.expect("main closure dropped")
}

fn with_core<T>(
    label: &str,
    f: impl FnOnce(&ICoreWebView2Controller, &ICoreWebView2) -> T,
) -> Option<T> {
    CORES.with(|c| c.borrow().get(label).map(|(a, b)| f(a, b)))
}

pub async fn register(wv: &tauri::Webview) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    let label = wv.label().to_string();
    wv.with_webview(move |p| {
        let c = p.controller();
        let core = unsafe { c.CoreWebView2() }.expect("CoreWebView2");
        CORES.with(|m| m.borrow_mut().insert(label, (c, core)));
        let _ = tx.send(());
    })
    .map_err(|e| format!("with_webview: {e}"))?;
    match tokio::time::timeout(Duration::from_secs(15), rx).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err("with_webview closure dropped: the webview was never created".into()),
        Err(_) => Err("with_webview timeout".into()),
    }
}

/// CreateCoreWebView2EnvironmentWithOptions on a user-data folder, returning the HRESULT.
pub async fn create_env(app: &AppHandle, udf: String, args: String, extensions: bool) -> Result<SendEnv, String> {
    let (tx, rx) = oneshot::channel::<Result<SendEnv, String>>();
    let issued: Result<(), String> = on_main(app, move || unsafe {
        use webview2_com::{CoreWebView2EnvironmentOptions, CreateCoreWebView2EnvironmentCompletedHandler};
        use windows::Win32::Globalization::{LCIDToLocaleName, LOCALE_ALLOW_NEUTRAL_NAMES, MAX_LOCALE_NAME, GetUserDefaultUILanguage};
        let options = CoreWebView2EnvironmentOptions::default();
        options.set_additional_browser_arguments(args);
        options.set_are_browser_extensions_enabled(extensions);
        let lcid = GetUserDefaultUILanguage();
        let mut lang = [0; MAX_LOCALE_NAME as usize];
        LCIDToLocaleName(lcid as u32, Some(&mut lang), LOCALE_ALLOW_NEUTRAL_NAMES);
        options.set_language(String::from_utf16_lossy(&lang));
        let h = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |hr, env| {
            let _ = tx.send(match hr {
                Ok(()) => env.map(SendEnv).ok_or_else(|| "no env".to_string()),
                Err(e) => Err(format!("hr={:#010x} {}", e.code().0, e.message())),
            });
            Ok(())
        }));
        CreateCoreWebView2EnvironmentWithOptions(
            windows::core::PCWSTR::null(),
            &HSTRING::from(udf),
            &ICoreWebView2EnvironmentOptions::from(options),
            &h,
        )
        .map_err(|e| format!("issue hr={:#010x} {}", e.code().0, e.message()))
    })
    .await;
    issued?;
    tokio::time::timeout(Duration::from_secs(20), rx)
        .await
        .map_err(|_| "env timeout".to_string())?
        .map_err(|_| "env dropped".to_string())?
}

pub async fn forget(app: &AppHandle, label: &str) {
    let l = label.to_string();
    on_main(app, move || {
        CORES.with(|m| {
            if let Some((c, _)) = m.borrow_mut().remove(&l) {
                let _ = unsafe { c.Close() };
            }
        })
    })
    .await;
}

pub async fn cdp(
    app: &AppHandle,
    label: &str,
    session: Option<&str>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let (tx, rx) = oneshot::channel::<Result<String, String>>();
    let (l, m, p, s) = (
        label.to_string(),
        method.to_string(),
        params.to_string(),
        session.map(str::to_string),
    );
    let issued: Result<(), String> = on_main(app, move || {
        with_core(&l, |_, core| unsafe {
            let h = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |hr, json| {
                let _ = tx.send(match hr {
                    Ok(()) => Ok(json),
                    Err(e) => Err(format!("hr={:#010x} body={json}", e.code().0)),
                });
                Ok(())
            }));
            match s {
                None => core
                    .CallDevToolsProtocolMethod(&HSTRING::from(m), &HSTRING::from(p), &h)
                    .map_err(|e| format!("issue hr={:#010x}", e.code().0)),
                Some(s) => match core.cast::<ICoreWebView2_11>() {
                    Ok(c11) => c11
                        .CallDevToolsProtocolMethodForSession(
                            &HSTRING::from(s),
                            &HSTRING::from(m),
                            &HSTRING::from(p),
                            &h,
                        )
                        .map_err(|e| format!("issue(session) hr={:#010x}", e.code().0)),
                    Err(e) => Err(format!("no ICoreWebView2_11 hr={:#010x}", e.code().0)),
                },
            }
        })
        .unwrap_or_else(|| Err("no core".into()))
    })
    .await;
    issued?;
    let json = tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .map_err(|_| "cdp timeout".to_string())?
        .map_err(|_| "cdp dropped".to_string())??;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

/// Runtime.evaluate returning the by-value result, or `Err(exception text)`.
pub async fn ev_in(
    app: &AppHandle,
    label: &str,
    session: Option<&str>,
    context_id: Option<i64>,
    expr: &str,
) -> Result<Value, String> {
    let mut params = json!({ "expression": expr, "returnByValue": true, "awaitPromise": true });
    if let Some(id) = context_id {
        params["contextId"] = json!(id);
    }
    let out = cdp(app, label, session, "Runtime.evaluate", params).await?;
    if let Some(ex) = out.get("exceptionDetails") {
        return Err(format!(
            "exception: {}",
            ex.pointer("/exception/description")
                .or_else(|| ex.get("text"))
                .cloned()
                .unwrap_or(Value::Null)
        ));
    }
    Ok(out.pointer("/result/value").cloned().unwrap_or(Value::Null))
}

pub async fn ev(app: &AppHandle, label: &str, expr: &str) -> Result<Value, String> {
    ev_in(app, label, None, None, expr).await
}

pub type Events = mpsc::UnboundedReceiver<(Option<String>, Value)>;

pub async fn subscribe(app: &AppHandle, label: &str, event: &str) -> Result<Events, String> {
    let (tx, rx) = mpsc::unbounded_channel();
    let (l, e) = (label.to_string(), event.to_string());
    on_main(app, move || {
        with_core(&l, |_, core| unsafe {
            let recv = core
                .GetDevToolsProtocolEventReceiver(&HSTRING::from(e))
                .map_err(|e| format!("receiver hr={:#010x}", e.code().0))?;
            let h = DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_s, args| {
                if let Some(a) = args {
                    let mut p = PWSTR::null();
                    a.ParameterObjectAsJson(&mut p)?;
                    let body = take_pwstr(p);
                    let sid = a
                        .cast::<ICoreWebView2DevToolsProtocolEventReceivedEventArgs2>()
                        .ok()
                        .and_then(|a2| {
                            let mut s = PWSTR::null();
                            a2.SessionId(&mut s).ok()?;
                            Some(take_pwstr(s))
                        });
                    let _ = tx.send((sid, serde_json::from_str(&body).unwrap_or(Value::Null)));
                }
                Ok(())
            }));
            let mut tok = 0i64;
            recv.add_DevToolsProtocolEventReceived(&h, &mut tok)
                .map_err(|e| format!("add handler hr={:#010x}", e.code().0))?;
            RECEIVERS.with(|k| k.borrow_mut().push(recv));
            Ok::<(), String>(())
        })
        .unwrap_or_else(|| Err("no core".into()))
    })
    .await?;
    Ok(rx)
}

pub async fn drain(rx: &mut Events, wait_ms: u64) -> Vec<(Option<String>, Value)> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);
    while let Ok(Some(e)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        out.push(e);
    }
    out
}

fn pw(f: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> Option<String> {
    let mut p = PWSTR::null();
    f(&mut p).ok()?;
    Some(take_pwstr(p))
}

/// Environment, browser process and profile facts for one controller.
pub async fn identity(app: &AppHandle, label: &str) -> Value {
    let l = label.to_string();
    on_main(app, move || {
        with_core(&l, |_, core| unsafe {
            let mut pid = 0u32;
            let _ = core.BrowserProcessId(&mut pid);
            let env = core.cast::<ICoreWebView2_2>().and_then(|c| c.Environment()).ok();
            let env_ptr = env
                .as_ref()
                .and_then(|e| e.cast::<windows::core::IUnknown>().ok())
                .map(|u| format!("{:p}", u.as_raw()));
            let udf = env
                .as_ref()
                .and_then(|e| e.cast::<ICoreWebView2Environment7>().ok())
                .and_then(|e7| pw(|p| e7.UserDataFolder(p)));
            let version = env.as_ref().and_then(|e| pw(|p| e.BrowserVersionString(p)));
            let prof = core.cast::<ICoreWebView2_13>().and_then(|c| c.Profile()).ok();
            let (name, private, path) = match prof {
                Some(p) => {
                    let mut b = BOOL(0);
                    let _ = p.IsInPrivateModeEnabled(&mut b);
                    (
                        pw(|x| p.ProfileName(x)),
                        Some(b.as_bool()),
                        pw(|x| p.ProfilePath(x)),
                    )
                }
                None => (None, None, None),
            };
            json!({
                "browser_pid": pid, "env_ptr": env_ptr, "user_data_folder": udf,
                "runtime_version": version, "profile_name": name,
                "in_private": private, "profile_path": path,
            })
        })
        .unwrap_or(Value::Null)
    })
    .await
}

pub async fn env_of(app: &AppHandle, label: &str) -> Option<SendEnv> {
    let l = label.to_string();
    on_main(app, move || {
        with_core(&l, |_, core| unsafe {
            core.cast::<ICoreWebView2_2>()
                .and_then(|c| c.Environment())
                .ok()
                .map(SendEnv)
        })
        .flatten()
    })
    .await
}

/// A controller created directly on an environment (bypassing wry) so the
/// controller options wry does not expose (ProfileName) can be exercised.
pub async fn raw_controller(
    app: &AppHandle,
    label: &str,
    env: SendEnv,
    hwnd: isize,
    profile: Option<&str>,
    in_private: bool,
    rect: (i32, i32, i32, i32),
    url: &str,
) -> Result<(), String> {
    let (tx, rx) = oneshot::channel::<Result<(), String>>();
    let (l, p, u) = (label.to_string(), profile.map(str::to_string), url.to_string());
    let issued: Result<(), String> = on_main(app, move || unsafe {
        let env = env;
        let env10 = env
            .0
            .cast::<ICoreWebView2Environment10>()
            .map_err(|e| format!("env10 hr={:#010x}", e.code().0))?;
        let opts = env10
            .CreateCoreWebView2ControllerOptions()
            .map_err(|e| format!("opts hr={:#010x}", e.code().0))?;
        if let Some(name) = &p {
            opts.SetProfileName(&HSTRING::from(name.as_str()))
                .map_err(|e| format!("SetProfileName hr={:#010x}", e.code().0))?;
        }
        opts.SetIsInPrivateModeEnabled(in_private)
            .map_err(|e| format!("SetIsInPrivateModeEnabled hr={:#010x}", e.code().0))?;
        let h = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |hr, ctrl| {
            let r = (|| -> Result<(), String> {
                hr.map_err(|e| format!("create controller hr={:#010x}", e.code().0))?;
                let c = ctrl.ok_or("no controller")?;
                let core = c.CoreWebView2().map_err(|e| format!("core {e}"))?;
                let (x, y, w, h) = rect;
                let _ = c.SetBounds(RECT { left: x, top: y, right: x + w, bottom: y + h });
                let _ = c.SetIsVisible(true);
                core.Navigate(&HSTRING::from(u)).map_err(|e| format!("navigate {e}"))?;
                CORES.with(|m| m.borrow_mut().insert(l, (c, core)));
                Ok(())
            })();
            let _ = tx.send(r);
            Ok(())
        }));
        env10
            .CreateCoreWebView2ControllerWithOptions(HWND(hwnd as _), &opts, &h)
            .map_err(|e| format!("CreateCoreWebView2ControllerWithOptions hr={:#010x}", e.code().0))
    })
    .await;
    issued?;
    tokio::time::timeout(Duration::from_secs(20), rx)
        .await
        .map_err(|_| "controller timeout".to_string())?
        .map_err(|_| "controller dropped".to_string())?
}

pub async fn move_focus_programmatic(app: &AppHandle, label: &str) -> Result<(), String> {
    let l = label.to_string();
    on_main(app, move || {
        with_core(&l, |c, _| unsafe {
            c.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
                .map_err(|e| format!("MoveFocus hr={:#010x}", e.code().0))
        })
        .unwrap_or_else(|| Err("no core".into()))
    })
    .await
}

pub static FOCUS_EVENTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub async fn watch_focus(app: &AppHandle, label: &str) {
    let l = label.to_string();
    on_main(app, move || {
        with_core(&l, |c, _| unsafe {
            for got in [true, false] {
                let tag = format!("{l}:{}", if got { "GotFocus" } else { "LostFocus" });
                let h = webview2_com::FocusChangedEventHandler::create(Box::new(move |_, _| {
                    FOCUS_EVENTS.lock().unwrap().push(tag.clone());
                    Ok(())
                }));
                let mut tok = 0i64;
                let _ = if got { c.add_GotFocus(&h, &mut tok) } else { c.add_LostFocus(&h, &mut tok) };
            }
        });
    })
    .await;
}

/// Win32 keyboard focus and foreground window, read on the UI thread.
pub async fn win32_focus(app: &AppHandle, top: isize) -> Value {
    on_main(app, move || unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetAncestor, GetClassNameW, GetForegroundWindow, GA_ROOT,
        };
        let class = |h: HWND| {
            let mut buf = [0u16; 128];
            let n = GetClassNameW(h, &mut buf);
            String::from_utf16_lossy(&buf[..n as usize])
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GUITHREADINFO};
        let f = GetFocus();
        let fg = GetForegroundWindow();
        let mut gti = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let _ = GetGUIThreadInfo(0, &mut gti);
        let ff = gti.hwndFocus;
        json!({
            "focus_hwnd": format!("{:?}", f.0),
            "focus_class": class(f),
            "fg_thread_focus_class": class(ff),
            "fg_thread_focus_root_is_app_window": GetAncestor(ff, GA_ROOT).0 as isize == top,
            "focus_root_is_app_window": GetAncestor(f, GA_ROOT).0 as isize == top,
            "foreground_is_app_window": fg.0 as isize == top,
        })
    })
    .await
}

/// Which controller's host window contains the Win32 keyboard focus.
pub async fn focus_owner(app: &AppHandle, labels: Vec<String>) -> Value {
    on_main(app, move || unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
        use windows::Win32::UI::WindowsAndMessaging::IsChild;
        let f = GetFocus();
        let mut out = serde_json::Map::new();
        for l in labels {
            let owns = with_core(&l, |c, _| {
                let mut p = HWND::default();
                c.ParentWindow(&mut p).ok().map(|_| p == f || IsChild(p, f).as_bool())
            })
            .flatten();
            out.insert(l, json!(owns));
        }
        Value::Object(out)
    })
    .await
}
