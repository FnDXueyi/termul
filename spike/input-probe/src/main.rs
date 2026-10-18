mod fixtures;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Window, WindowBuilder};
use wry::{PermissionKind, PermissionResponse, WebView, WebViewBuilder};

pub trait Synth {
  fn setup_focus(&self) -> Value;
  fn native_state(&self) -> Value;
  fn motion(&self, x: f64, y: f64, button1: bool) -> Value;
  fn button(&self, x: f64, y: f64, press: bool) -> Value;
  fn key(&self, ch: char) -> Value;
  fn wheel(&self, x: f64, y: f64) -> Value;
}

enum Step {
  Wait,
  Focus,
  EvalB(&'static str),
  Read(&'static str),
  Move(f64, f64, bool),
  Down(f64, f64),
  Up(f64, f64),
  Key(char),
  Wheel(f64, f64),
  MockMedia,
  Load(&'static str),
  Exit,
}

pub fn emit(label: &str, who: &str, data: Value) {
  println!("{}", json!({ "label": label, "who": who, "data": data }));
  let _ = std::io::stdout().flush();
}

fn decode(s: &str) -> Value {
  match serde_json::from_str::<Value>(s) {
    Ok(Value::String(inner)) => serde_json::from_str(&inner).unwrap_or(Value::String(inner)),
    Ok(v) => v,
    Err(_) => Value::String(s.to_string()),
  }
}

fn serve() -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let port = listener.local_addr().unwrap().port();
  std::thread::spawn(move || {
    for stream in listener.incoming().flatten() {
      std::thread::spawn(move || handle(stream));
    }
  });
  port
}

fn handle(mut s: TcpStream) {
  let mut buf = [0u8; 8192];
  let n = s.read(&mut buf).unwrap_or(0);
  let req = String::from_utf8_lossy(&buf[..n]);
  let path = req.split_whitespace().nth(1).unwrap_or("/");
  let body = match path.split('?').next().unwrap_or("/") {
    "/target" => fixtures::TARGET,
    "/chat" => fixtures::CHAT,
    "/perm" => fixtures::PERM,
    _ => fixtures::BLANK,
  };
  let _ = write!(
    s,
    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
    body.len(),
    body
  );
}

type PermHandler = Box<dyn Fn(PermissionKind) -> PermissionResponse + Send + Sync + 'static>;

fn child(window: &Window, url: &str, top: f64, height: f64, perm: Option<PermHandler>) -> WebView {
  let mut b = WebViewBuilder::new().with_url(url).with_bounds(wry::Rect {
    position: wry::dpi::LogicalPosition::new(0.0, top).into(),
    size: wry::dpi::LogicalSize::new(900.0, height).into(),
  });
  if let Some(h) = perm {
    b = b.with_permission_handler(h);
  }
  #[cfg(target_os = "linux")]
  {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;
    b.build_gtk(window.default_vbox().unwrap()).unwrap()
  }
  #[cfg(not(target_os = "linux"))]
  {
    b.build_as_child(window).unwrap()
  }
}

const RESET_SCROLL: &str = "window.scrollTo(0,0);document.getElementById('scroller').scrollTop=0";

fn input_steps() -> Vec<Step> {
  use Step::*;
  let mut v: Vec<Step> = (0..8).map(|_| Wait).collect();
  v.extend([
    Focus,
    Wait,
    Read("initial"),
    Move(90.0, 40.0, false),
    Move(100.0, 50.0, false),
    Move(110.0, 55.0, false),
    Wait,
    Read("pointer_move"),
    Down(280.0, 50.0),
    Up(280.0, 50.0),
    Wait,
    Read("pointer_click"),
    EvalB("document.getElementById('inp').focus()"),
    Wait,
    Read("b_inp_script_focus"),
    Key('a'),
    Wait,
    Read("trusted_key_b_focused"),
    Focus,
    Wait,
    Read("refocus_a"),
    Key('b'),
    Wait,
    Read("trusted_key_a_focused"),
    EvalB(RESET_SCROLL),
    Wait,
    Wheel(120.0, 195.0),
    Wait,
    Wait,
    Read("wheel"),
    EvalB(RESET_SCROLL),
    Wait,
    Down(300.0, 150.0),
    Move(320.0, 160.0, true),
    Move(360.0, 180.0, true),
    Up(360.0, 180.0),
    Wait,
    Read("drag_mouse"),
    EvalB(RESET_SCROLL),
    Wait,
    Down(640.0, 150.0),
    Move(645.0, 155.0, true),
    Move(645.0, 200.0, true),
    Move(640.0, 250.0, true),
    Move(642.0, 252.0, true),
    Up(642.0, 252.0),
    Wait,
    Wait,
    Read("drag_html5"),
    Exit,
  ]);
  v
}

fn perm_steps(mock_first: bool) -> Vec<Step> {
  use Step::*;
  if mock_first {
    let mut v = vec![Wait, Wait, MockMedia, Wait, Load("/perm?phase=mock_first")];
    v.extend((0..44).map(|_| Wait));
    v.extend([Read("perm_mock_first"), Exit]);
    return v;
  }
  let mut v: Vec<Step> = (0..44).map(|_| Wait).collect();
  v.extend([Read("perm_default"), MockMedia, Wait, Load("/perm?phase=mock")]);
  v.extend((0..44).map(|_| Wait));
  v.extend([Read("perm_mock"), Exit]);
  v
}

fn main() {
  let args: Vec<String> = std::env::args().collect();
  let mode = args.get(1).cloned().unwrap_or_else(|| "input".into());
  let variant = args.get(2).cloned().unwrap_or_default();
  let port = serve();
  let base = format!("http://127.0.0.1:{port}");
  emit("start", "host", json!({ "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "mode": mode, "variant": variant }));

  std::thread::spawn(|| {
    std::thread::sleep(Duration::from_secs(150));
    emit("watchdog", "host", json!("timeout"));
    std::process::exit(3);
  });

  let event_loop = EventLoopBuilder::<()>::with_user_event().build();
  let window = WindowBuilder::new()
    .with_title("input-probe")
    .with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0))
    .build(&event_loop)
    .unwrap();

  let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
  let (a, b, steps) = if mode == "perm" {
    let rec = calls.clone();
    let response = match variant.as_str() {
      "allow" => Some(PermissionResponse::Allow),
      "deny" => Some(PermissionResponse::Deny),
      _ => None,
    };
    let handler: Option<PermHandler> = response.map(|r| {
      Box::new(move |kind: PermissionKind| {
        rec.lock().unwrap().push(format!("{kind}->{r}"));
        r
      }) as PermHandler
    });
    let mock_first = args.get(3).map(String::as_str) == Some("mock_first");
    let first = if mock_first { "/blank" } else { "/perm?phase=default" };
    let b = child(&window, &format!("{base}{first}"), 0.0, 800.0, handler);
    (None, b, perm_steps(mock_first))
  } else {
    let a = child(&window, &format!("{base}/chat"), 0.0, 250.0, None);
    let b = child(&window, &format!("{base}/target"), 250.0, 550.0, None);
    (Some(a), b, input_steps())
  };

  #[cfg(target_os = "linux")]
  let synth: Option<Box<dyn Synth>> = a.as_ref().map(|a| Box::new(linux::GtkSynth::new(&window, a, &b, &variant)) as Box<dyn Synth>);
  #[cfg(target_os = "macos")]
  let synth: Option<Box<dyn Synth>> = a.as_ref().map(|a| Box::new(macos::MacSynth::new(a, &b, &variant)) as Box<dyn Synth>);
  #[cfg(not(any(target_os = "linux", target_os = "macos")))]
  let synth: Option<Box<dyn Synth>> = None;

  let proxy = event_loop.create_proxy();
  std::thread::spawn(move || loop {
    std::thread::sleep(Duration::from_millis(500));
    if proxy.send_event(()).is_err() {
      break;
    }
  });

  let mut i = 0usize;
  event_loop.run(move |event, _, control_flow| {
    *control_flow = ControlFlow::Wait;
    let Event::UserEvent(()) = event else { return };
    let Some(step) = steps.get(i) else { return };
    i += 1;
    let need_synth = |name: &str| -> bool {
      if synth.is_none() {
        emit(name, "synth", json!("not_exercised: no synthesizer on this OS"));
        false
      } else {
        true
      }
    };
    match step {
      Step::Wait => {}
      Step::Focus => {
        if let Some(a) = &a {
          let _ = a.evaluate_script("document.getElementById('chat').focus()");
        }
        if need_synth("focus") {
          emit("focus", "synth", synth.as_ref().unwrap().setup_focus());
        }
      }
      Step::EvalB(js) => {
        let _ = b.evaluate_script(js);
      }
      Step::Read(label) => {
        let l = label.to_string();
        if let Some(s) = &synth {
          emit(label, "native", s.native_state());
        }
        if mode == "perm" {
          emit(label, "handler_calls", json!(*calls.lock().unwrap()));
        }
        let lb = l.clone();
        let _ = b.evaluate_script_with_callback("window.__probe ? window.__probe() : 'no_probe'", move |r| emit(&lb, "page_b", decode(&r)));
        if let Some(a) = &a {
          let la = l.clone();
          let _ = a.evaluate_script_with_callback("window.__probe ? window.__probe() : 'no_probe'", move |r| emit(&la, "page_a", decode(&r)));
        }
      }
      Step::Move(x, y, b1) => {
        if need_synth("move") {
          emit("move", "synth", synth.as_ref().unwrap().motion(*x, *y, *b1));
        }
      }
      Step::Down(x, y) => {
        if need_synth("down") {
          emit("down", "synth", synth.as_ref().unwrap().button(*x, *y, true));
        }
      }
      Step::Up(x, y) => {
        if need_synth("up") {
          emit("up", "synth", synth.as_ref().unwrap().button(*x, *y, false));
        }
      }
      Step::Key(c) => {
        if need_synth("key") {
          emit("key", "synth", synth.as_ref().unwrap().key(*c));
        }
      }
      Step::Wheel(x, y) => {
        if need_synth("wheel") {
          emit("wheel", "synth", synth.as_ref().unwrap().wheel(*x, *y));
        }
      }
      Step::MockMedia => {
        #[cfg(target_os = "linux")]
        emit("mock_media", "host", linux::enable_mock_media(&b));
        #[cfg(target_os = "macos")]
        emit("mock_media", "host", macos::enable_mock_media(&b));
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        emit("mock_media", "host", json!("fake devices via WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"));
      }
      Step::Load(path) => {
        let _ = b.load_url(&format!("{base}{path}"));
      }
      Step::Exit => {
        emit("exit", "host", json!("done"));
        *control_flow = ControlFlow::Exit;
      }
    }
  });
}
