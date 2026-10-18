use crate::Synth;
use gdk::ffi as gs;
use gdk::prelude::SeatExt;
use glib::prelude::*;
use glib::translate::{from_glib_none, ToGlibPtr};
use gtk::ffi as gts;
use serde_json::{json, Value};
use std::cell::Cell;
use tao::platform::unix::WindowExtUnix;
use webkit2gtk::WebViewExt;
use wry::WebViewExtUnix;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Route {
  MainDoEvent,
  WidgetEvent,
}

pub struct GtkSynth {
  window: gtk::Widget,
  a: gtk::Widget,
  b: gtk::Widget,
  route: Route,
  time: Cell<u32>,
}

impl GtkSynth {
  pub fn new(window: &tao::window::Window, a: &wry::WebView, b: &wry::WebView, variant: &str) -> Self {
    let route = if variant == "widget" { Route::WidgetEvent } else { Route::MainDoEvent };
    Self {
      window: window.gtk_window().clone().upcast::<gtk::Widget>(),
      a: a.webview().upcast::<gtk::Widget>(),
      b: b.webview().upcast::<gtk::Widget>(),
      route,
      time: Cell::new((glib::monotonic_time() / 1000) as u32),
    }
  }

  fn gtk_window(&self) -> *mut gts::GtkWindow {
    let p: *mut gts::GtkWidget = self.window.to_glib_none().0;
    p as *mut gts::GtkWindow
  }

  fn tick(&self) -> u32 {
    let t = self.time.get().wrapping_add(30);
    self.time.set(t);
    t
  }

  fn display(&self) -> gdk::Display {
    gdk::Display::default().unwrap()
  }

  fn pointer(&self) -> gdk::Device {
    self.display().default_seat().unwrap().pointer().unwrap()
  }

  fn keyboard(&self) -> gdk::Device {
    self.display().default_seat().unwrap().keyboard().unwrap()
  }

  /// Event window plus window-relative and root coordinates for a point in B's CSS px.
  unsafe fn coords(&self, x: f64, y: f64) -> (*mut gs::GdkWindow, f64, f64, f64, f64, bool) {
    let bw: *mut gts::GtkWidget = self.b.to_glib_none().0;
    let win = gts::gtk_widget_get_window(bw);
    let own = gts::gtk_widget_get_has_window(bw) != 0;
    let (mut wx, mut wy) = (x, y);
    if !own {
      let mut alloc: gts::GtkAllocation = std::mem::zeroed();
      gts::gtk_widget_get_allocation(bw, &mut alloc);
      wx += alloc.x as f64;
      wy += alloc.y as f64;
    }
    let (mut ox, mut oy) = (0i32, 0i32);
    gs::gdk_window_get_origin(win, &mut ox, &mut oy);
    gobject_ref(win);
    (win, wx, wy, wx + ox as f64, wy + oy as f64, own)
  }

  unsafe fn dispatch(&self, ev: *mut gs::GdkEvent) -> bool {
    let handled = match self.route {
      Route::MainDoEvent => {
        gts::gtk_main_do_event(ev);
        true
      }
      Route::WidgetEvent => gts::gtk_widget_event(self.b.to_glib_none().0, ev) != 0,
    };
    gs::gdk_event_free(ev);
    handled
  }

  unsafe fn set_device(&self, ev: *mut gs::GdkEvent, d: &gdk::Device) {
    gs::gdk_event_set_device(ev, d.to_glib_none().0);
    gs::gdk_event_set_source_device(ev, d.to_glib_none().0);
  }

  fn who(&self, w: *mut gts::GtkWidget) -> String {
    if w.is_null() {
      return "none".into();
    }
    let a: *mut gts::GtkWidget = self.a.to_glib_none().0;
    let b: *mut gts::GtkWidget = self.b.to_glib_none().0;
    if w == a {
      "A".into()
    } else if w == b {
      "B".into()
    } else {
      let wd: gtk::Widget = unsafe { from_glib_none(w) };
      wd.type_().name().to_string()
    }
  }
}

unsafe fn gobject_ref(win: *mut gs::GdkWindow) {
  glib::gobject_ffi::g_object_ref(win as *mut _);
}

impl Synth for GtkSynth {
  fn setup_focus(&self) -> Value {
    unsafe {
      let w = self.gtk_window();
      gts::gtk_window_present(w);
      gts::gtk_widget_grab_focus(self.a.to_glib_none().0);
    }
    self.native_state()
  }

  fn native_state(&self) -> Value {
    unsafe {
      let w = self.gtk_window();
      let focus = gts::gtk_window_get_focus(w);
      json!({
        "backend": self.display().type_().name(),
        "route": format!("{:?}", self.route),
        "window_active": gts::gtk_window_is_active(w) != 0,
        "toplevel_focus": gts::gtk_window_has_toplevel_focus(w) != 0,
        "focus_widget": self.who(focus),
        "a_has_focus": gts::gtk_widget_has_focus(self.a.to_glib_none().0) != 0,
        "b_has_focus": gts::gtk_widget_has_focus(self.b.to_glib_none().0) != 0,
        "b_has_window": gts::gtk_widget_get_has_window(self.b.to_glib_none().0) != 0,
      })
    }
  }

  fn motion(&self, x: f64, y: f64, button1: bool) -> Value {
    unsafe {
      let (win, wx, wy, rx, ry, own) = self.coords(x, y);
      let ev = gs::gdk_event_new(gs::GDK_MOTION_NOTIFY);
      let m = &mut (*ev).motion;
      m.window = win;
      m.send_event = 0;
      m.time = self.tick();
      m.x = wx;
      m.y = wy;
      m.x_root = rx;
      m.y_root = ry;
      m.state = if button1 { gs::GDK_BUTTON1_MASK } else { 0 };
      m.is_hint = 0;
      self.set_device(ev, &self.pointer());
      let handled = self.dispatch(ev);
      json!({ "x": wx, "y": wy, "own_window": own, "handled": handled })
    }
  }

  fn button(&self, x: f64, y: f64, press: bool) -> Value {
    unsafe {
      let (win, wx, wy, rx, ry, own) = self.coords(x, y);
      let ev = gs::gdk_event_new(if press { gs::GDK_BUTTON_PRESS } else { gs::GDK_BUTTON_RELEASE });
      let bt = &mut (*ev).button;
      bt.window = win;
      bt.send_event = 0;
      bt.time = self.tick();
      bt.x = wx;
      bt.y = wy;
      bt.x_root = rx;
      bt.y_root = ry;
      bt.button = 1;
      bt.state = if press { 0 } else { gs::GDK_BUTTON1_MASK };
      self.set_device(ev, &self.pointer());
      let handled = self.dispatch(ev);
      json!({ "press": press, "x": wx, "y": wy, "own_window": own, "handled": handled })
    }
  }

  fn key(&self, ch: char) -> Value {
    unsafe {
      let keyval = gs::gdk_unicode_to_keyval(ch as u32);
      let keymap = gs::gdk_keymap_get_for_display(self.display().to_glib_none().0);
      let mut keys: *mut gs::GdkKeymapKey = std::ptr::null_mut();
      let mut n: i32 = 0;
      let mut code: u32 = 0;
      if gs::gdk_keymap_get_entries_for_keyval(keymap, keyval, &mut keys, &mut n) != 0 && n > 0 {
        code = (*keys).keycode;
        glib::ffi::g_free(keys as *mut _);
      }
      let text = std::ffi::CString::new(ch.to_string()).unwrap();
      let mut out = Vec::new();
      for press in [true, false] {
        let (win, ..) = self.coords(1.0, 1.0);
        let ev = gs::gdk_event_new(if press { gs::GDK_KEY_PRESS } else { gs::GDK_KEY_RELEASE });
        let k = &mut (*ev).key;
        k.window = win;
        k.send_event = 0;
        k.time = self.tick();
        k.state = 0;
        k.keyval = keyval;
        k.length = 1;
        k.string = glib::ffi::g_strdup(text.as_ptr());
        k.hardware_keycode = code as u16;
        k.group = 0;
        self.set_device(ev, &self.keyboard());
        out.push(self.dispatch(ev));
      }
      json!({ "keyval": keyval, "keycode": code, "handled": out })
    }
  }

  fn wheel(&self, x: f64, y: f64) -> Value {
    unsafe {
      let mut out = Vec::new();
      for smooth in [false, false, false, true] {
        let (win, wx, wy, rx, ry, _) = self.coords(x, y);
        let ev = gs::gdk_event_new(gs::GDK_SCROLL);
        let s = &mut (*ev).scroll;
        s.window = win;
        s.send_event = 0;
        s.time = self.tick();
        s.x = wx;
        s.y = wy;
        s.x_root = rx;
        s.y_root = ry;
        s.state = 0;
        if smooth {
          s.direction = gs::GDK_SCROLL_SMOOTH;
          s.delta_x = 0.0;
          s.delta_y = 2.0;
        } else {
          s.direction = gs::GDK_SCROLL_DOWN;
        }
        self.set_device(ev, &self.pointer());
        out.push(self.dispatch(ev));
      }
      json!({ "handled": out })
    }
  }
}

pub fn enable_mock_media(b: &wry::WebView) -> Value {
  let wv = b.webview();
  match wv.settings() {
    Some(s) => {
      let read = |s: &webkit2gtk::Settings| {
        json!({
          "media_stream": s.property::<bool>("enable-media-stream"),
          "mock_capture": s.property::<bool>("enable-mock-capture-devices"),
        })
      };
      let before = read(&s);
      s.set_property("enable-media-stream", true);
      s.set_property("enable-mock-capture-devices", true);
      json!({ "before": before, "after": read(&s) })
    }
    None => json!("no settings"),
  }
}
