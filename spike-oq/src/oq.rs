use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl};

use crate::cdp::{self, cdp, drain, ev, ev_in, identity, subscribe};
use crate::{rec, CALLS};

const LOCAL: &str = "http://localhost:5191";
const OTHER: &str = "http://127.0.0.1:5192";

fn sleep(ms: u64) -> tokio::time::Sleep {
    tokio::time::sleep(Duration::from_millis(ms))
}

fn profiles_root() -> PathBuf {
    PathBuf::from(std::env::var("SPIKE_PROFILES").expect("SPIKE_PROFILES"))
}

async fn child(
    app: &AppHandle,
    label: &str,
    url: &str,
    rect: (f64, f64, f64, f64),
    cfg: impl FnOnce(WebviewBuilder<tauri::Wry>) -> WebviewBuilder<tauri::Wry>,
) -> Result<tauri::Webview, String> {
    let window = app.get_window("main").ok_or("no main window")?;
    let builder = cfg(WebviewBuilder::new(label, WebviewUrl::External(url.parse().unwrap())));
    let (x, y, w, h) = rect;
    let wv = window
        .add_child(builder, LogicalPosition::new(x, y), LogicalSize::new(w, h))
        .map_err(|e| format!("add_child: {e}"))?;
    cdp::register(&wv).await?;
    Ok(wv)
}

async fn wait_loaded(app: &AppHandle, label: &str, prefix: &str) -> bool {
    for _ in 0..100 {
        if let Ok(v) = ev(app, label, "document.readyState + ' ' + location.href").await {
            let s = v.as_str().unwrap_or("");
            if s.starts_with("complete ") && s[9..].starts_with(prefix) {
                return true;
            }
        }
        sleep(200).await;
    }
    false
}

async fn close(app: &AppHandle, wv: tauri::Webview) {
    let label = wv.label().to_string();
    let _ = wv.close();
    cdp::forget(app, &label).await;
}

pub async fn run_all(app: AppHandle) {
    let main = app.get_webview("main").expect("main webview");
    cdp::register(&main).await.expect("register main");
    let main_loaded = wait_loaded(&app, "main", "http://localhost:5181").await;
    rec("meta", "main_loaded", json!(main_loaded));
    rec("meta", "main_identity", identity(&app, "main").await);
    let mode = std::env::var("SPIKE_MODE").unwrap_or_default();
    if mode == "persist" {
        oq6_persist_check(&app).await;
        return;
    }
    if mode.is_empty() || mode.contains("oq1") {
        oq1(&app).await;
    }
    if mode.is_empty() || mode.contains("oq2") {
        oq2(&app).await;
    }
    if mode.is_empty() || mode.contains("oq3") {
        oq3(&app).await;
    }
    if mode.is_empty() || mode.contains("oq6") {
        oq6(&app).await;
    }
}

const PROBE: &str = r#"(function(){
 var o=window.__oq1={hasInternals:!!(window.__TAURI_INTERNALS__&&window.__TAURI_INTERNALS__.invoke),hasGlobalTauri:!!window.__TAURI__,origin:location.origin,results:{}};
 var inv=o.hasInternals&&window.__TAURI_INTERNALS__.invoke;
 if(!inv){o.done=true;return;}
 var calls=[['browser_agent_eval_result',{tabId:'TAB',nonce:'n1',ok:true,value:'"x"'}],['browser_tab_report_url',{tabId:'TAB',url:location.href}],['browser_tab_report_title',{tabId:'TAB',title:document.title}],['browser_tab_report_loaded',{tabId:'TAB'}]];
 var pending=calls.length;
 calls.forEach(function(c){inv(c[0],c[1]).then(function(v){o.results[c[0]]='resolved:'+JSON.stringify(v)},function(e){o.results[c[0]]='rejected:'+String(e);console.error('[oq1] '+c[0]+' rejected: '+String(e))}).finally(function(){if(--pending===0)o.done=true})});
})();"#;

async fn probe_invoke(app: &AppHandle, wv: &tauri::Webview, case: &str) {
    let label = wv.label().to_string();
    let _ = cdp(app, &label, None, "Runtime.enable", json!({})).await;
    let mut console = subscribe(app, &label, "Runtime.consoleAPICalled").await.ok();
    CALLS.lock().unwrap().clear();
    // Same delivery as the shipped bridge: Webview::eval into the page main world.
    let eval = wv.eval(PROBE.replace("TAB", &label));
    rec("oq1", &format!("{case}.webview_eval"), json!(eval.map_err(|e| e.to_string())));
    let mut state = Value::Null;
    for _ in 0..50 {
        state = ev(app, &label, "JSON.stringify(window.__oq1||null)").await.unwrap_or(Value::Null);
        if state.as_str().is_some_and(|s| s.contains("\"done\":true")) {
            break;
        }
        sleep(100).await;
    }
    sleep(300).await;
    rec("oq1", &format!("{case}.page_state"), state);
    rec("oq1", &format!("{case}.host_side_calls"), json!(CALLS.lock().unwrap().clone()));
    if let Some(rx) = console.as_mut() {
        let lines: Vec<Value> = drain(rx, 300)
            .await
            .into_iter()
            .map(|(_, v)| json!({"type": v["type"], "args": v["args"].as_array().map(|a| a.iter().map(|x| x["value"].clone()).collect::<Vec<_>>())}))
            .collect();
        rec("oq1", &format!("{case}.console"), json!(lines));
    }
}

async fn oq1(app: &AppHandle) {
    let main = app.get_webview("main").unwrap();
    probe_invoke(app, &main, "control_main_devurl").await;
    let cases = [
        ("oq1_https", "https://example.com/"),
        ("oq1_localhost", "http://localhost:5191/storage.html"),
        ("oq1_127", "http://127.0.0.1:5191/storage.html"),
        ("oq1_devorigin", "http://localhost:5181/storage.html"),
    ];
    for (i, (label, url)) in cases.iter().enumerate() {
        let wv = match child(app, label, url, (10.0 + 260.0 * i as f64, 10.0, 250.0, 150.0), |b| b).await {
            Ok(w) => w,
            Err(e) => {
                rec("oq1", &format!("{label}.create"), json!(e));
                continue;
            }
        };
        let prefix = url.trim_end_matches('/');
        let loaded = wait_loaded(app, label, &prefix[..prefix.len().min(20)]).await;
        rec("oq1", &format!("{label}.loaded"), json!({"url": url, "loaded": loaded}));
        probe_invoke(app, &wv, label).await;
        close(app, wv).await;
    }
}

async fn oq2_case(app: &AppHandle, label: &str) {
    let r = |k: &str, v: Value| rec("oq2", &format!("{label}.{k}"), v);
    let mut bind = subscribe(app, label, "Runtime.bindingCalled").await.expect("sub binding");
    let mut ctx_created = subscribe(app, label, "Runtime.executionContextCreated").await.expect("sub ctx");
    let mut attached = subscribe(app, label, "Target.attachedToTarget").await.expect("sub attach");

    let world = |name: &str, frame: &str| json!({"frameId": frame, "worldName": name, "grantUniveralAccess": false});
    let tree = cdp(app, label, None, "Page.getFrameTree", json!({})).await;
    let main_frame = tree.as_ref().ok().and_then(|t| t.pointer("/frameTree/frame/id")).and_then(Value::as_str).unwrap_or("").to_string();
    r("frame_tree", json!(tree.as_ref().map(|t| {
        let kids: Vec<Value> = t.pointer("/frameTree/childFrames").and_then(Value::as_array).cloned().unwrap_or_default()
            .iter().map(|c| json!({"id": c.pointer("/frame/id"), "url": c.pointer("/frame/url")})).collect();
        json!({"main": t.pointer("/frameTree/frame/url"), "children": kids})
    }).map_err(Clone::clone)));

    // Binding added before the world exists, scoped by world name.
    let b1 = cdp(app, label, None, "Runtime.addBinding", json!({"name": "__termulReply", "executionContextName": "termul_iso"})).await;
    r("addBinding_scoped", json!(b1));
    let w1 = cdp(app, label, None, "Page.createIsolatedWorld", world("termul_iso", &main_frame)).await;
    r("createIsolatedWorld", json!(w1));
    let ctx = w1.ok().and_then(|v| v["executionContextId"].as_i64());

    let tamper = "window.__secret='main';var __js=JSON.stringify;JSON.stringify=function(){return 'FORGED'};Element.prototype.getBoundingClientRect=function(){return {width:-1}};document.querySelector=function(){return null};'tampered'";
    r("main_world_tamper", json!(ev(app, label, tamper).await));
    let probe = "JSON.stringify({secret: typeof window.__secret, rectWidth: document.documentElement.getBoundingClientRect().width, qs: !!document.querySelector('body'), title: document.title, binding: typeof window.__termulReply})";
    r("iso_eval_after_tamper", json!(ev_in(app, label, None, ctx, probe).await));
    r("main_eval_after_tamper", json!(ev(app, label, probe).await));

    // Binding call from the isolated world, before Runtime.enable.
    r("iso_call_binding_pre_enable", json!(ev_in(app, label, None, ctx, "__termulReply('iso-payload-1'); 'called'").await));
    let ev1 = drain(&mut bind, 800).await;
    r("bindingCalled_pre_enable", json!(ev1.iter().map(|(s, v)| json!({"session": s, "name": v["name"], "payload": v["payload"], "ctx": v["executionContextId"]})).collect::<Vec<_>>()));
    r("Runtime.enable", json!(cdp(app, label, None, "Runtime.enable", json!({})).await.map(|_| "ok")));
    let created: Vec<Value> = drain(&mut ctx_created, 500).await.into_iter().map(|(_, v)| json!({"id": v.pointer("/context/id"), "name": v.pointer("/context/name"), "origin": v.pointer("/context/origin"), "aux": v.pointer("/context/auxData")})).collect();
    r("contexts_reported_on_enable", json!(created));
    r("iso_call_binding_post_enable", json!(ev_in(app, label, None, ctx, "__termulReply('iso-payload-2'); 'called'").await));
    let ev2 = drain(&mut bind, 800).await;
    r("bindingCalled_post_enable", json!(ev2.iter().map(|(s, v)| json!({"session": s, "name": v["name"], "payload": v["payload"], "ctx": v["executionContextId"]})).collect::<Vec<_>>()));
    r("main_world_binding_typeof", json!(ev(app, label, "typeof window.__termulReply").await));
    r("main_world_binding_call", json!(ev(app, label, "window.__termulReply('forged-from-main')").await));
    let ev3 = drain(&mut bind, 600).await;
    r("bindingCalled_after_main_call", json!(ev3.len()));

    // World created before the binding is added.
    let w0 = cdp(app, label, None, "Page.createIsolatedWorld", world("termul_iso_pre", &main_frame)).await;
    let ctx0 = w0.ok().and_then(|v| v["executionContextId"].as_i64());
    let _ = cdp(app, label, None, "Runtime.addBinding", json!({"name": "__preReply", "executionContextName": "termul_iso_pre"})).await;
    r("binding_into_existing_world_typeof", json!(ev_in(app, label, None, ctx0, "typeof __preReply").await));

    // Same-process child frames.
    let frames: Vec<(String, String)> = cdp(app, label, None, "Page.getFrameTree", json!({})).await.ok()
        .and_then(|t| t.pointer("/frameTree/childFrames").and_then(Value::as_array).cloned())
        .unwrap_or_default().iter()
        .map(|c| (c.pointer("/frame/id").and_then(Value::as_str).unwrap_or("").to_string(), c.pointer("/frame/url").and_then(Value::as_str).unwrap_or("").to_string()))
        .collect();
    for (fid, furl) in &frames {
        let w = cdp(app, label, None, "Page.createIsolatedWorld", world("termul_iso", fid)).await;
        let fctx = w.as_ref().ok().and_then(|v| v["executionContextId"].as_i64());
        let read = match fctx {
            Some(c) => json!(ev_in(app, label, None, Some(c), "location.href + ' secret=' + typeof window.__frameSecret").await),
            None => Value::Null,
        };
        r("child_frame_world_from_page_session", json!({"url": furl, "createIsolatedWorld": w, "iso_read": read}));
    }

    // Out-of-process iframes through flattened auto-attach sessions.
    let aa = cdp(app, label, None, "Target.setAutoAttach", json!({"autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true})).await;
    r("setAutoAttach", json!(aa));
    let att = drain(&mut attached, 2000).await;
    r("attachedToTarget", json!(att.iter().map(|(s, v)| json!({"event_session": s, "sessionId": v["sessionId"], "type": v.pointer("/targetInfo/type"), "url": v.pointer("/targetInfo/url")})).collect::<Vec<_>>()));
    for (_, v) in &att {
        let Some(sid) = v["sessionId"].as_str() else { continue };
        let s = Some(sid);
        let ft = cdp(app, label, s, "Page.getFrameTree", json!({})).await;
        let fid = ft.as_ref().ok().and_then(|t| t.pointer("/frameTree/frame/id")).and_then(Value::as_str).unwrap_or("").to_string();
        let b = cdp(app, label, s, "Runtime.addBinding", json!({"name": "__termulReplyF", "executionContextName": "termul_iso_f"})).await;
        let _ = cdp(app, label, s, "Runtime.enable", json!({})).await;
        let w = cdp(app, label, s, "Page.createIsolatedWorld", world("termul_iso_f", &fid)).await;
        let fctx = w.as_ref().ok().and_then(|v| v["executionContextId"].as_i64());
        let iso = ev_in(app, label, s, fctx, "__termulReplyF('oopif:' + location.href + ' secret=' + typeof window.__frameSecret); typeof window.__frameSecret").await;
        let mainw = ev_in(app, label, s, None, "typeof window.__frameSecret + ' binding=' + typeof window.__termulReplyF").await;
        let got: Vec<Value> = drain(&mut bind, 1000).await.into_iter().map(|(es, v)| json!({"event_session": es, "name": v["name"], "payload": v["payload"]})).collect();
        r("oopif_session", json!({"url": v.pointer("/targetInfo/url"), "frame_tree_ok": ft.is_ok(), "addBinding": b, "createIsolatedWorld": w, "iso_eval": iso, "oopif_main_world": mainw, "bindingCalled": got}));
        // Same OOPIF frame id addressed from the page session (no sessionId).
        let cross = cdp(app, label, None, "Page.createIsolatedWorld", world("termul_iso", &fid)).await;
        r("oopif_world_via_page_session", json!({"url": v.pointer("/targetInfo/url"), "result": cross}));
    }

    // Ordering probes: call into the pre-existing world, and binding-then-world with Runtime enabled.
    r("pre_world_call", json!(ev_in(app, label, None, ctx0, "__preReply('pre-world-call'); 'called'").await));
    let got: Vec<Value> = drain(&mut bind, 800).await.into_iter().map(|(es, v)| json!({"event_session": es, "name": v["name"], "payload": v["payload"], "ctx": v["executionContextId"]})).collect();
    r("pre_world_bindingCalled", json!(got));
    let _ = cdp(app, label, None, "Runtime.addBinding", json!({"name": "__termulReplyE", "executionContextName": "termul_iso_e"})).await;
    let we = cdp(app, label, None, "Page.createIsolatedWorld", world("termul_iso_e", &main_frame)).await;
    let ctxe = we.ok().and_then(|v| v["executionContextId"].as_i64());
    r("enabled_binding_then_world_call", json!(ev_in(app, label, None, ctxe, "__termulReplyE('enabled-then-world'); 'called'").await));
    let got: Vec<Value> = drain(&mut bind, 800).await.into_iter().map(|(es, v)| json!({"event_session": es, "name": v["name"], "payload": v["payload"], "ctx": v["executionContextId"]})).collect();
    r("enabled_binding_then_world_bindingCalled", json!(got));
    r("main_world_typeof_after_all", json!(ev(app, label, "[typeof window.__termulReply, typeof window.__preReply, typeof window.__termulReplyE].join(',')").await));

    // World + script installed for every new document (construction-time helpers).
    r("Page.enable", json!(cdp(app, label, None, "Page.enable", json!({})).await.map(|_| "ok")));
    let _ = cdp(app, label, None, "Runtime.addBinding", json!({"name": "__termulReply2", "executionContextName": "termul_iso2"})).await;
    let add = cdp(app, label, None, "Page.addScriptToEvaluateOnNewDocument", json!({"source": "try{__termulReply2('newdoc:'+location.href+' secret='+typeof window.__frameSecret)}catch(e){}", "worldName": "termul_iso2"})).await;
    r("addScriptToEvaluateOnNewDocument", json!(add));
    let _ = cdp(app, label, None, "Page.reload", json!({})).await;
    sleep(2500).await;
    let got: Vec<Value> = drain(&mut bind, 1000).await.into_iter().map(|(es, v)| json!({"event_session": es, "name": v["name"], "payload": v["payload"]})).collect();
    r("newdoc_bindingCalled", json!(got));
    let created: Vec<Value> = drain(&mut ctx_created, 300).await.into_iter().filter(|(_, v)| v.pointer("/context/name").and_then(Value::as_str).is_some_and(|n| !n.is_empty())).map(|(es, v)| json!({"event_session": es, "name": v.pointer("/context/name"), "origin": v.pointer("/context/origin")})).collect();
    r("named_contexts_after_reload", json!(created));
    let att2 = drain(&mut attached, 1000).await;
    r("attachedToTarget_after_reload", json!(att2.iter().map(|(_, v)| json!({"type": v.pointer("/targetInfo/type"), "url": v.pointer("/targetInfo/url")})).collect::<Vec<_>>()));
}

fn enc(s: &str) -> String {
    s.replace(':', "%3A").replace('/', "%2F")
}

async fn oq2(app: &AppHandle) {
    let url = format!("{LOCAL}/fixture.html?frames={},{}", enc(&format!("{OTHER}/frame.html")), enc("/frame.html"));
    match child(app, "oq2_local", &url, (10.0, 180.0, 700.0, 300.0), |b| b).await {
        Ok(wv) => {
            rec("oq2", "oq2_local.loaded", json!(wait_loaded(app, "oq2_local", LOCAL).await));
            sleep(800).await;
            oq2_case(app, "oq2_local").await;
            close(app, wv).await;
        }
        Err(e) => rec("oq2", "oq2_local.create", json!(e)),
    }
    match child(app, "oq2_https", "https://example.com/", (10.0, 180.0, 700.0, 300.0), |b| b).await {
        Ok(wv) => {
            rec("oq2", "oq2_https.loaded", json!(wait_loaded(app, "oq2_https", "https://example.com").await));
            let inj = ev(app, "oq2_https", "var f=document.createElement('iframe');f.src='https://example.org/';document.body.appendChild(f);var g=document.createElement('iframe');g.src='/';document.body.appendChild(g);'injected'").await;
            rec("oq2", "oq2_https.inject_iframes", json!(inj));
            sleep(3000).await;
            oq2_case(app, "oq2_https").await;
            close(app, wv).await;
        }
        Err(e) => rec("oq2", "oq2_https.create", json!(e)),
    }
}

async fn focus_snapshot(app: &AppHandle, top: isize, label: &str) -> Value {
    json!({
        "win32": cdp::win32_focus(app, top).await,
        "win32_focus_inside": cdp::focus_owner(app, vec!["main".to_string(), label.to_string()]).await,
        "main_hasFocus": ev(app, "main", "document.hasFocus()").await,
        "main_active": ev(app, "main", "document.activeElement && document.activeElement.id").await,
        "child_hasFocus": ev(app, label, "document.hasFocus()").await,
        "controller_focus_events": cdp::FOCUS_EVENTS.lock().unwrap().clone(),
    })
}

async fn mouse(app: &AppHandle, label: &str, kind: &str, x: f64, y: f64, extra: Value) -> Result<Value, String> {
    let mut p = json!({"type": kind, "x": x, "y": y});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    cdp(app, label, None, "Input.dispatchMouseEvent", p).await
}

async fn events(app: &AppHandle, label: &str) -> Value {
    let v = ev(app, label, "var e=window.__ev;window.__ev=[];JSON.stringify({ev:e,lastMove:window.__lastMove||null,moves:window.__moves||0})").await;
    v.ok().and_then(|s| s.as_str().and_then(|s| serde_json::from_str(s).ok())).unwrap_or(Value::Null)
}

async fn oq3(app: &AppHandle) {
    let label = "oq3";
    let wv = match child(app, label, &format!("{LOCAL}/fixture.html"), (10.0, 180.0, 700.0, 500.0), |b| b).await {
        Ok(w) => w,
        Err(e) => return rec("oq3", "create", json!(e)),
    };
    rec("oq3", "loaded", json!(wait_loaded(app, label, LOCAL).await));
    let top = app.get_window("main").unwrap().hwnd().map(|h| h.0 as isize).unwrap_or(0);
    let _ = app.get_window("main").unwrap().set_focus();
    cdp::watch_focus(app, "main").await;
    cdp::watch_focus(app, label).await;
    rec("oq3", "main_MoveFocus", json!(cdp::move_focus_programmatic(app, "main").await));
    let _ = ev(app, "main", "document.getElementById('focusme').focus(); true").await;
    sleep(300).await;
    cdp::FOCUS_EVENTS.lock().unwrap().clear();
    rec("oq3", "focus.before", focus_snapshot(app, top, label).await);
    let _ = events(app, label).await;

    rec("oq3", "hover.dispatch", json!(mouse(app, label, "mouseMoved", 100.0, 40.0, json!({})).await));
    sleep(250).await;
    rec("oq3", "hover.state", json!(ev(app, label, "JSON.stringify({hover: document.getElementById('hov').matches(':hover'), bg: getComputedStyle(document.getElementById('hov')).backgroundColor})").await));
    rec("oq3", "hover.events", events(app, label).await);
    if let Ok(shot) = cdp(app, label, None, "Page.captureScreenshot", json!({"format": "png"})).await {
        use base64::Engine;
        if let Some(b) = shot["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()) {
            let p = format!("{}/oq3-hover.png", std::env::var("SPIKE_EVIDENCE").unwrap());
            let _ = std::fs::write(&p, b);
            rec("oq3", "hover.screenshot", json!(p));
        }
    }
    rec("oq3", "focus.after_hover", focus_snapshot(app, top, label).await);

    rec("oq3", "wheel.dispatch", json!(mouse(app, label, "mouseWheel", 300.0, 300.0, json!({"deltaX": 0, "deltaY": 400})).await));
    sleep(600).await;
    rec("oq3", "wheel.state", json!(ev(app, label, "window.scrollY").await));
    rec("oq3", "wheel.events", events(app, label).await);
    rec("oq3", "focus.after_wheel", focus_snapshot(app, top, label).await);
    let _ = ev(app, label, "window.scrollTo(0,0); 0").await;
    sleep(200).await;
    let _ = events(app, label).await;

    // Press-move-release pointer drag on a mouse-event slider.
    let mut steps = vec![mouse(app, label, "mousePressed", 30.0, 90.0, json!({"button": "left", "buttons": 1, "clickCount": 1})).await];
    for i in 1..=5 {
        steps.push(mouse(app, label, "mouseMoved", 30.0 + 20.0 * i as f64, 90.0, json!({"button": "left", "buttons": 1})).await);
    }
    steps.push(mouse(app, label, "mouseReleased", 130.0, 90.0, json!({"button": "left", "buttons": 0, "clickCount": 1})).await);
    rec("oq3", "pointer_drag.dispatch_ok", json!(steps.iter().all(Result::is_ok)));
    sleep(200).await;
    rec("oq3", "pointer_drag.handle_left", json!(ev(app, label, "document.getElementById('handle').style.left").await));
    rec("oq3", "pointer_drag.events", events(app, label).await);
    rec("oq3", "focus.after_pointer_drag", focus_snapshot(app, top, label).await);

    // HTML5 drag via drag interception + Input.dispatchDragEvent.
    let mut intercepted = subscribe(app, label, "Input.dragIntercepted").await.expect("sub drag");
    rec("oq3", "dnd_intercept.setInterceptDrags", json!(cdp(app, label, None, "Input.setInterceptDrags", json!({"enabled": true})).await));
    let _ = mouse(app, label, "mousePressed", 50.0, 140.0, json!({"button": "left", "buttons": 1, "clickCount": 1})).await;
    for i in 1..=6 {
        let _ = mouse(app, label, "mouseMoved", 50.0 + 30.0 * i as f64, 145.0, json!({"button": "left", "buttons": 1})).await;
    }
    let got = drain(&mut intercepted, 1000).await;
    rec("oq3", "dnd_intercept.dragIntercepted", json!(got.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>()));
    if let Some((_, v)) = got.first() {
        let data = v["data"].clone();
        for t in ["dragEnter", "dragOver", "drop"] {
            let r = cdp(app, label, None, "Input.dispatchDragEvent", json!({"type": t, "x": 250.0, "y": 150.0, "data": data})).await;
            rec("oq3", &format!("dnd_intercept.{t}"), json!(r));
        }
    }
    let _ = mouse(app, label, "mouseReleased", 250.0, 150.0, json!({"button": "left", "buttons": 0, "clickCount": 1})).await;
    sleep(300).await;
    rec("oq3", "dnd_intercept.dropped", json!(ev(app, label, "window.__dropped || null").await));
    rec("oq3", "dnd_intercept.events", events(app, label).await);
    let _ = cdp(app, label, None, "Input.setInterceptDrags", json!({"enabled": false})).await;
    let _ = ev(app, label, "window.__dropped=null; 0").await;
    rec("oq3", "focus.after_dnd_intercept", focus_snapshot(app, top, label).await);

    // Trusted click focus check (the shipped Windows pointer_click path).
    let _ = mouse(app, label, "mousePressed", 100.0, 40.0, json!({"button": "left", "buttons": 1, "clickCount": 1})).await;
    let _ = mouse(app, label, "mouseReleased", 100.0, 40.0, json!({"button": "left", "buttons": 0, "clickCount": 1})).await;
    sleep(300).await;
    rec("oq3", "click.events", events(app, label).await);
    rec("oq3", "focus.after_click", focus_snapshot(app, top, label).await);

    // HTML5 drag with a plain mouse sequence and no interception (last: may enter an OS drag loop).
    let mut seq = vec![mouse(app, label, "mousePressed", 50.0, 140.0, json!({"button": "left", "buttons": 1, "clickCount": 1})).await];
    for i in 1..=6 {
        seq.push(mouse(app, label, "mouseMoved", 50.0 + 30.0 * i as f64, 145.0, json!({"button": "left", "buttons": 1})).await);
        sleep(50).await;
    }
    seq.push(mouse(app, label, "mouseReleased", 250.0, 150.0, json!({"button": "left", "buttons": 0, "clickCount": 1})).await);
    rec("oq3", "dnd_plain.dispatch", json!(seq.iter().map(|r| r.as_ref().map(|_| "ok").map_err(Clone::clone)).collect::<Vec<_>>()));
    sleep(1500).await;
    rec("oq3", "dnd_plain.dropped", json!(ev(app, label, "window.__dropped || null").await));
    rec("oq3", "dnd_plain.events", events(app, label).await);
    rec("oq3", "focus.after_dnd_plain", focus_snapshot(app, top, label).await);
    close(app, wv).await;
}

async fn storage_set(app: &AppHandle, label: &str, tag: &str) -> Result<Value, String> {
    ev(app, label, &format!("document.cookie='c_{tag}=1; path=/; max-age=86400'; localStorage.setItem('l_{tag}','1'); 'set'")).await
}

async fn storage_read(app: &AppHandle, label: &str) -> Value {
    let v = ev(app, label, "JSON.stringify({cookies: document.cookie.split('; ').filter(Boolean).map(function(c){return c.split('=')[0]}).sort(), local: Object.keys(localStorage).sort(), origin: location.origin})").await;
    v.ok().and_then(|s| s.as_str().and_then(|s| serde_json::from_str(s).ok())).unwrap_or(Value::Null)
}

async fn oq6(app: &AppHandle) {
    let root = profiles_root();
    let agent = root.join("agent-global");
    let sess1 = root.join("sess-1");
    let sess2 = root.join("sess-2");
    let url = format!("{LOCAL}/storage.html");
    let mut slot: f64 = 0.0;
    let mut next = || {
        slot += 1.0;
        (10.0 + (slot % 6.0) * 240.0, 500.0 + (slot / 6.0).floor() * 130.0, 230.0, 120.0)
    };
    let mut made: Vec<(String, tauri::Webview)> = Vec::new();
    type Cfg = Box<dyn FnOnce(WebviewBuilder<tauri::Wry>) -> WebviewBuilder<tauri::Wry> + Send>;
    let plan: Vec<(&str, Cfg)> = vec![
        ("n_c", Box::new(|b| b)),
        ("p_a", Box::new(|b| b.incognito(true))),
        ("p_b", Box::new(|b| b.incognito(true))),
        ("g_1", { let d = agent.clone(); Box::new(move |b| b.data_directory(d)) }),
        ("g_2", { let d = agent.clone(); Box::new(move |b| b.data_directory(d)) }),
        ("g_i", { let d = agent.clone(); Box::new(move |b| b.data_directory(d).incognito(true)) }),
        ("e_1", { let d = sess1.clone(); Box::new(move |b| b.data_directory(d).incognito(true)) }),
        ("e_2", { let d = sess2.clone(); Box::new(move |b| b.data_directory(d).incognito(true)) }),
    ];
    for (label, cfg) in plan {
        rec("oq6", "creating", json!(label));
        match child(app, label, &url, next(), cfg).await {
            Ok(w) => made.push((label.to_string(), w)),
            Err(e) => rec("oq6", &format!("{label}.create"), json!(e)),
        }
    }
    // One shared environment object reused across controllers.
    if let Some(env) = cdp::env_of(app, "g_1").await {
        let env2 = cdp::env_of(app, "g_1").await.unwrap();
        for (label, e) in [("s_1", env), ("s_2i", env2)] {
            rec("oq6", "creating", json!(label));
            let inc = label.ends_with('i');
            match child(app, label, &url, next(), move |b| { let e = e; b.with_environment(e.0).incognito(inc) }).await {
                Ok(w) => made.push((label.to_string(), w)),
                Err(e) => rec("oq6", &format!("{label}.create"), json!(e)),
            }
        }
    }
    // Environment option conflicts, created directly to capture the HRESULT.
    let wry_args = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection".to_string();
    let main_udf = identity(app, "main").await["user_data_folder"].as_str().unwrap_or("").to_string();
    let agent_udf = agent.to_string_lossy().to_string();
    let env_cases = [
        ("env_main_same_options", main_udf.clone(), wry_args.clone(), false),
        ("env_main_other_args", main_udf.clone(), "--disable-features=msWebOOUI".to_string(), false),
        ("env_agent_same_options", agent_udf.clone(), wry_args.clone(), false),
        ("env_agent_other_args", agent_udf.clone(), "--disable-features=msWebOOUI".to_string(), false),
        ("env_agent_extensions_on", agent_udf.clone(), wry_args.clone(), true),
        ("env_fresh_folder_other_args", root.join("fresh").to_string_lossy().to_string(), "--disable-features=msWebOOUI".to_string(), false),
    ];
    for (case, udf, args, ext) in env_cases {
        match cdp::create_env(app, udf, args, ext).await {
            Ok(env) => {
                let hwnd = app.get_window("main").unwrap().hwnd().map(|h| h.0 as isize).unwrap_or(0);
                let c = cdp::raw_controller(app, case, env, hwnd, None, false, (0, 0, 50, 50), &url).await;
                rec("oq6", &format!("{case}.result"), json!({"environment": "created", "controller": c}));
            }
            Err(e) => rec("oq6", &format!("{case}.result"), json!({"environment": e})),
        }
    }
    // Raw controllers with ProfileName (not exposed by tauri 2.11.5 / wry 0.55).
    let hwnd = app.get_window("main").unwrap().hwnd().map(|h| h.0 as isize).unwrap_or(0);
    let raws = [
        ("r_1", "main", Some("agent-s1"), true),
        ("r_2", "main", Some("agent-s2"), true),
        ("r_3", "main", Some("termul-agent"), false),
        ("r_4", "g_1", Some("agent-s1"), true),
    ];
    for (label, env_from, profile, private) in raws {
        rec("oq6", "creating", json!(label));
        let Some(env) = cdp::env_of(app, env_from).await else { continue };
        let (x, y, w, h) = next();
        let r = cdp::raw_controller(app, label, env, hwnd, profile, private, (x as i32, y as i32, w as i32, h as i32), &url).await;
        rec("oq6", &format!("{label}.create"), json!({"env_from": env_from, "profile": profile, "in_private": private, "result": r}));
    }
    let mut labels: Vec<String> = made.iter().map(|(l, _)| l.clone()).collect();
    labels.extend(["r_1", "r_2", "r_3", "r_4"].map(String::from));
    let mut live = Vec::new();
    for l in &labels {
        if wait_loaded(app, l, LOCAL).await {
            live.push(l.clone());
        } else {
            rec("oq6", &format!("{l}.loaded"), json!(false));
        }
    }
    for l in &live {
        rec("oq6", &format!("{l}.identity"), identity(app, l).await);
    }
    rec("oq6", "main.set", json!(ev(app, "main", "document.cookie='c_main=1; path=/; max-age=86400'; 'set'").await));
    for l in &live {
        let _ = storage_set(app, l, l).await;
    }
    sleep(500).await;
    for l in &live {
        rec("oq6", &format!("{l}.visible"), storage_read(app, l).await);
    }
    for l in ["g_1", "p_a", "n_c", "r_3", "e_1"] {
        if live.iter().any(|x| x == l) {
            let _ = storage_set(app, l, &format!("persist_{l}")).await;
        }
    }
    // Teardown order: delete a per-session folder while and after its controller is open.
    if let Some(pos) = made.iter().position(|(l, _)| l == "e_2") {
        rec("oq6", "teardown.remove_while_open", json!(std::fs::remove_dir_all(&sess2).map_err(|e| e.to_string())));
        let (l, w) = made.remove(pos);
        close(app, w).await;
        for ms in [0u64, 2000, 6000] {
            sleep(ms).await;
            let r = std::fs::remove_dir_all(&sess2).map_err(|e| e.to_string());
            rec("oq6", &format!("teardown.remove_after_close_{ms}ms"), json!({"label": l, "result": r, "exists": sess2.exists()}));
            if !sess2.exists() {
                break;
            }
        }
    }
    sleep(1000).await;
    // Environment option conflicts on an already-open user-data folder.
    let conflicts: Vec<(&str, Cfg)> = vec![
        ("x_agent_args", { let d = agent.clone(); Box::new(move |b| b.data_directory(d).additional_browser_args("--disable-features=msWebOOUI")) }),
        ("x_main_args", Box::new(|b| b.additional_browser_args("--disable-features=msWebOOUI"))),
        ("x_agent_ext", { let d = agent.clone(); Box::new(move |b| b.data_directory(d).browser_extensions_enabled(true)) }),
    ];
    for (label, cfg) in conflicts {
        rec("oq6", "creating", json!(label));
        match child(app, label, &url, next(), cfg).await {
            Ok(w) => {
                rec("oq6", &format!("{label}.create"), json!("ok (no conflict)"));
                made.push((label.to_string(), w));
            }
            Err(e) => rec("oq6", &format!("{label}.create"), json!(e)),
        }
    }
}

async fn oq6_persist_check(app: &AppHandle) {
    let root = profiles_root();
    let agent = root.join("agent-global");
    let sess1 = root.join("sess-1");
    let url = format!("{LOCAL}/storage.html");
    let specs: Vec<(&str, Box<dyn FnOnce(WebviewBuilder<tauri::Wry>) -> WebviewBuilder<tauri::Wry> + Send>)> = vec![
        ("n_c", Box::new(|b| b)),
        ("p_a", Box::new(|b| b.incognito(true))),
        ("g_1", { let d = agent.clone(); Box::new(move |b| b.data_directory(d)) }),
        ("e_1", { let d = sess1.clone(); Box::new(move |b| b.data_directory(d).incognito(true)) }),
    ];
    let mut i = 0.0;
    for (label, cfg) in specs {
        i += 1.0;
        if child(app, label, &url, (10.0 + i * 240.0, 500.0, 230.0, 120.0), cfg).await.is_ok() && wait_loaded(app, label, LOCAL).await {
            rec("persist", &format!("{label}.visible_after_restart"), storage_read(app, label).await);
        }
    }
    let hwnd = app.get_window("main").unwrap().hwnd().map(|h| h.0 as isize).unwrap_or(0);
    if let Some(env) = cdp::env_of(app, "main").await {
        let r = cdp::raw_controller(app, "r_3", env, hwnd, Some("termul-agent"), false, (10, 700, 230, 120), &url).await;
        if r.is_ok() && wait_loaded(app, "r_3", LOCAL).await {
            rec("persist", "r_3.visible_after_restart", storage_read(app, "r_3").await);
        }
    }
}
