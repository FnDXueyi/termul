use crate::Synth;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, Sel};
use objc2::{class, msg_send, sel};
use objc2_app_kit::NSWindow;
use objc2_foundation::{NSPoint, NSRect, NSString};
use serde_json::{json, Value};
use std::ffi::c_void;
use wry::{WebViewExtMacOS, WryWebView};

#[repr(C)]
#[derive(Clone, Copy)]
struct CgPoint {
  x: f64,
  y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
  fn CGEventCreateScrollWheelEvent2(source: *const c_void, units: u32, wheel_count: u32, wheel1: i32, wheel2: i32, wheel3: i32) -> *mut c_void;
  fn CGEventSetLocation(event: *mut c_void, location: CgPoint);
  fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
  fn CFRelease(cf: *const c_void);
}

const LEFT_DOWN: usize = 1;
const LEFT_UP: usize = 2;
const MOUSE_MOVED: usize = 5;
const LEFT_DRAGGED: usize = 6;
const KEY_DOWN: usize = 10;
const KEY_UP: usize = 11;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Route {
  Direct,
  Window,
  Spi,
  Tracking,
}

pub struct MacSynth {
  window: Retained<NSWindow>,
  a: Retained<WryWebView>,
  b: Retained<WryWebView>,
  route: Route,
}

fn uptime() -> f64 {
  unsafe {
    let pi: Retained<AnyObject> = msg_send![class!(NSProcessInfo), processInfo];
    msg_send![&*pi, systemUptime]
  }
}

impl MacSynth {
  pub fn new(a: &wry::WebView, b: &wry::WebView, variant: &str) -> Self {
    let route = match variant {
      "window" => Route::Window,
      "spi" => Route::Spi,
      "tracking" => Route::Tracking,
      _ => Route::Direct,
    };
    Self { window: b.ns_window(), a: a.webview(), b: b.webview(), route }
  }

  fn window_number(&self) -> isize {
    unsafe { msg_send![&*self.window, windowNumber] }
  }

  fn window_point(&self, x: f64, y: f64) -> NSPoint {
    unsafe {
      let flipped: bool = msg_send![&*self.b, isFlipped];
      let bounds: NSRect = msg_send![&*self.b, bounds];
      let p = NSPoint::new(x, if flipped { y } else { bounds.size.height - y });
      msg_send![&*self.b, convertPoint: p, toView: std::ptr::null_mut::<AnyObject>()]
    }
  }

  fn responds(&self, obj: &AnyObject, s: Sel) -> bool {
    unsafe { msg_send![obj, respondsToSelector: s] }
  }

  fn mouse_event(&self, ty: usize, x: f64, y: f64, pressure: f32) -> Option<Retained<AnyObject>> {
    let p = self.window_point(x, y);
    unsafe {
      msg_send![class!(NSEvent), mouseEventWithType: ty, location: p, modifierFlags: 0usize, timestamp: uptime(), windowNumber: self.window_number(), context: std::ptr::null_mut::<AnyObject>(), eventNumber: 0isize, clickCount: 1isize, pressure: pressure]
    }
  }

  fn deliver(&self, method: Sel, ev: &AnyObject) -> String {
    unsafe {
      match self.route {
        Route::Window => {
          let _: () = msg_send![&*self.window, sendEvent: ev];
          "window.sendEvent".into()
        }
        Route::Spi if method == sel!(mouseMoved:) && self.responds(&self.b, sel!(_simulateMouseMove:)) => {
          let _: () = msg_send![&*self.b, _simulateMouseMove: ev];
          "_simulateMouseMove".into()
        }
        Route::Tracking if method == sel!(mouseMoved:) => {
          let areas: Retained<AnyObject> = msg_send![&*self.b, trackingAreas];
          let n: usize = msg_send![&*areas, count];
          let mut owners = Vec::new();
          for i in 0..n {
            let area: *mut AnyObject = msg_send![&*areas, objectAtIndex: i];
            let owner: *mut AnyObject = msg_send![area, owner];
            if owner.is_null() {
              continue;
            }
            let name = format!("{:?}", (*owner).class());
            if name.contains("WKMouseTrackingObserver") {
              let _: () = msg_send![owner, mouseMoved: ev];
              return format!("tracking_owner {name}");
            }
            owners.push(name);
          }
          format!("no_tracking_owner {owners:?}")
        }
        _ => {
          let _: *mut AnyObject = msg_send![&*self.b, performSelector: method, withObject: ev];
          format!("direct {method:?}")
        }
      }
    }
  }

  fn describe(&self, ev: &AnyObject) -> Value {
    unsafe {
      let loc: NSPoint = msg_send![ev, locationInWindow];
      let wn: isize = msg_send![ev, windowNumber];
      json!({ "loc": [loc.x, loc.y], "window_number": wn })
    }
  }

  fn who(&self, fr: *mut AnyObject) -> String {
    if fr.is_null() {
      return "none".into();
    }
    let a = Retained::as_ptr(&self.a) as *mut AnyObject;
    let b = Retained::as_ptr(&self.b) as *mut AnyObject;
    if fr == a {
      return "A".into();
    }
    if fr == b {
      return "B".into();
    }
    unsafe {
      let is_view: bool = msg_send![fr, isKindOfClass: class!(NSView)];
      if is_view {
        let in_a: bool = msg_send![fr, isDescendantOf: &*self.a];
        let in_b: bool = msg_send![fr, isDescendantOf: &*self.b];
        if in_a {
          return "A-descendant".into();
        }
        if in_b {
          return "B-descendant".into();
        }
      }
      format!("{:?}", (*fr).class())
    }
  }
}

impl Synth for MacSynth {
  fn setup_focus(&self) -> Value {
    unsafe {
      let app: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
      let _: () = msg_send![&*app, activateIgnoringOtherApps: Bool::YES];
      let _: () = msg_send![&*self.window, makeKeyAndOrderFront: std::ptr::null_mut::<AnyObject>()];
      let ok: bool = msg_send![&*self.window, makeFirstResponder: &*self.a];
      json!({ "make_first_responder_a": ok, "state": self.native_state() })
    }
  }

  fn native_state(&self) -> Value {
    unsafe {
      let fr: *mut AnyObject = msg_send![&*self.window, firstResponder];
      let key: bool = msg_send![&*self.window, isKeyWindow];
      let app: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
      let active: bool = msg_send![&*app, isActive];
      json!({
        "route": format!("{:?}", self.route),
        "first_responder": self.who(fr),
        "key_window": key,
        "app_active": active,
        "spi_simulateMouseMove": self.responds(&self.b, sel!(_simulateMouseMove:)),
      })
    }
  }

  fn motion(&self, x: f64, y: f64, button1: bool) -> Value {
    let (ty, method) = if button1 { (LEFT_DRAGGED, sel!(mouseDragged:)) } else { (MOUSE_MOVED, sel!(mouseMoved:)) };
    match self.mouse_event(ty, x, y, if button1 { 1.0 } else { 0.0 }) {
      Some(ev) => json!({ "via": self.deliver(method, &ev), "event": self.describe(&ev) }),
      None => json!("event_construction_failed"),
    }
  }

  fn button(&self, x: f64, y: f64, press: bool) -> Value {
    let (ty, method) = if press { (LEFT_DOWN, sel!(mouseDown:)) } else { (LEFT_UP, sel!(mouseUp:)) };
    match self.mouse_event(ty, x, y, if press { 1.0 } else { 0.0 }) {
      Some(ev) => json!({ "press": press, "via": self.deliver(method, &ev), "event": self.describe(&ev) }),
      None => json!("event_construction_failed"),
    }
  }

  fn key(&self, ch: char) -> Value {
    let s = NSString::from_str(&ch.to_string());
    let mut out = Vec::new();
    for (ty, method) in [(KEY_DOWN, sel!(keyDown:)), (KEY_UP, sel!(keyUp:))] {
      let ev: Option<Retained<AnyObject>> = unsafe {
        msg_send![class!(NSEvent), keyEventWithType: ty, location: NSPoint::new(0.0, 0.0), modifierFlags: 0usize, timestamp: uptime(), windowNumber: self.window_number(), context: std::ptr::null_mut::<AnyObject>(), characters: &*s, charactersIgnoringModifiers: &*s, isARepeat: Bool::NO, keyCode: 0u16]
      };
      out.push(match ev {
        Some(ev) => json!(self.deliver(method, &ev)),
        None => json!("event_construction_failed"),
      });
    }
    json!({ "via": out })
  }

  fn wheel(&self, x: f64, y: f64) -> Value {
    unsafe {
      let wp = self.window_point(x, y);
      let sp: NSPoint = msg_send![&*self.window, convertPointToScreen: wp];
      let screens: Retained<AnyObject> = msg_send![class!(NSScreen), screens];
      let s0: Retained<AnyObject> = msg_send![&*screens, objectAtIndex: 0usize];
      let frame: NSRect = msg_send![&*s0, frame];
      let wn = self.window_number() as i64;
      let mut out = Vec::new();
      for _ in 0..3 {
        let e = CGEventCreateScrollWheelEvent2(std::ptr::null(), 0, 1, -40, 0, 0);
        if e.is_null() {
          out.push(json!("cg_event_null"));
          continue;
        }
        CGEventSetLocation(e, CgPoint { x: sp.x, y: frame.size.height - sp.y });
        CGEventSetIntegerValueField(e, 91, wn);
        CGEventSetIntegerValueField(e, 92, wn);
        let ev: Option<Retained<AnyObject>> = msg_send![class!(NSEvent), eventWithCGEvent: e];
        CFRelease(e);
        out.push(match ev {
          Some(ev) => json!({ "via": self.deliver(sel!(scrollWheel:), &ev), "event": self.describe(&ev) }),
          None => json!("ns_event_null"),
        });
      }
      json!(out)
    }
  }
}

pub fn enable_mock_media(b: &wry::WebView) -> Value {
  let wv = b.webview();
  unsafe {
    let cfg: Retained<AnyObject> = msg_send![&*wv, configuration];
    let prefs: Retained<AnyObject> = msg_send![&*cfg, preferences];
    let has = |s: Sel| -> bool { msg_send![&*prefs, respondsToSelector: s] };
    let mut out = serde_json::Map::new();
    if has(sel!(_setMockCaptureDevicesEnabled:)) {
      let _: () = msg_send![&*prefs, _setMockCaptureDevicesEnabled: Bool::YES];
      out.insert("mock".into(), json!(true));
    }
    if has(sel!(_setMediaCaptureRequiresSecureConnection:)) {
      let _: () = msg_send![&*prefs, _setMediaCaptureRequiresSecureConnection: Bool::NO];
      out.insert("insecure_ok".into(), json!(true));
    }
    if has(sel!(_setMediaDevicesEnabled:)) {
      let _: () = msg_send![&*prefs, _setMediaDevicesEnabled: Bool::YES];
      out.insert("media_devices".into(), json!(true));
    }
    if has(sel!(_setGetUserMediaRequiresFocus:)) {
      let _: () = msg_send![&*prefs, _setGetUserMediaRequiresFocus: Bool::NO];
      out.insert("no_focus_req".into(), json!(true));
    }
    let cfg2: Retained<AnyObject> = msg_send![&*wv, configuration];
    let prefs2: Retained<AnyObject> = msg_send![&*cfg2, preferences];
    let same = Retained::as_ptr(&prefs) == Retained::as_ptr(&prefs2);
    out.insert("prefs_shared_across_copies".into(), json!(same));
    let has2: bool = msg_send![&*prefs2, respondsToSelector: sel!(_mockCaptureDevicesEnabled)];
    if has2 {
      let v: bool = msg_send![&*prefs2, _mockCaptureDevicesEnabled];
      out.insert("mock_readback".into(), json!(v));
    }
    Value::Object(out)
  }
}
