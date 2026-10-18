mod cdp;
mod fixtures;
mod oq;

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;

use serde_json::{json, Value};

pub static CALLS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static OUT: Mutex<Option<File>> = Mutex::new(None);

pub fn rec(oq: &str, key: &str, v: Value) {
    let line = json!({ "oq": oq, "key": key, "v": v }).to_string();
    println!("{line}");
    if let Some(f) = OUT.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }
}

fn note(cmd: &str, webview: &tauri::Webview, detail: String) {
    let url = webview.url().map(|u| u.to_string()).unwrap_or_default();
    CALLS
        .lock()
        .unwrap()
        .push(format!("{cmd} executed host-side from webview={} url={url} {detail}", webview.label()));
}

#[tauri::command]
fn browser_agent_eval_result(
    webview: tauri::Webview,
    tab_id: String,
    nonce: String,
    ok: bool,
    value: String,
) {
    note("browser_agent_eval_result", &webview, format!("tab={tab_id} nonce={nonce} ok={ok} len={}", value.len()));
}

#[tauri::command]
fn browser_tab_report_url(webview: tauri::Webview, tab_id: String, url: String) {
    note("browser_tab_report_url", &webview, format!("tab={tab_id} len={}", url.len()));
}

#[tauri::command]
fn browser_tab_report_title(webview: tauri::Webview, tab_id: String, title: String) {
    note("browser_tab_report_title", &webview, format!("tab={tab_id} len={}", title.len()));
}

#[tauri::command]
fn browser_tab_report_loaded(webview: tauri::Webview, tab_id: String) {
    note("browser_tab_report_loaded", &webview, format!("tab={tab_id}"));
}

fn serve(port: u16) {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind fixture port");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(&stream);
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    return;
                }
                loop {
                    let mut l = String::new();
                    if reader.read_line(&mut l).unwrap_or(0) == 0 || l == "\r\n" {
                        break;
                    }
                }
                let path = first.split_whitespace().nth(1).unwrap_or("/");
                let path = path.split('?').next().unwrap_or("/");
                let (status, body) = match fixtures::page(path) {
                    Some(b) => ("200 OK", b),
                    None => ("404 Not Found", "not found"),
                };
                let mut s = &stream;
                let _ = write!(
                    s,
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
}

fn main() {
    let out = std::env::var("SPIKE_OUT").expect("SPIKE_OUT");
    *OUT.lock().unwrap() = Some(File::create(out).expect("create out"));
    for port in [5181, 5191, 5192] {
        serve(port);
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            browser_agent_eval_result,
            browser_tab_report_url,
            browser_tab_report_title,
            browser_tab_report_loaded
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                oq::run_all(handle.clone()).await;
                rec("meta", "done", json!(true));
                handle.exit(0);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run");
}
