//! Page helpers for input: the actionability facts probe and the small
//! steps around trusted input. None of them calls `element.focus()`: on
//! WKWebView that makes the agent tab first responder and blurs the chat.
//!
//! They resolve a ref through the registry the snapshot helper installs
//! (`window.__termulBrowser.refs[path].sel`). Agent strings enter only as
//! JSON literals.

/// Resolves `P` to `el`, or returns a stale-ref error from the helper.
const RESOLVE: &str = r#"
  var B = window.__termulBrowser;
  var fp = B && B.refs[P];
  var el = null;
  if (fp) { try { el = document.querySelector(fp.sel); } catch (e) { el = null; } }
"#;

fn literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn with_ref(path: &str, body: &str) -> String {
    format!(
        "(function(){{ var P={p};{resolve}{body} }})()",
        p = literal(path),
        resolve = RESOLVE
    )
}

/// Actionability facts for one ref, measured over two animation frames.
/// Scrolls the element into view first when it is outside the viewport;
/// that round reports it unstable. The reply is parsed by
/// [`super::input::parse_facts`].
pub fn facts(path: &str) -> String {
    with_ref(
        path,
        r#"
  if (!el || !el.isConnected) return { state: 'detached' };
  function box() { var r = el.getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; }
  function same(a, b) { return a[0] === b[0] && a[1] === b[1] && a[2] === b[2] && a[3] === b[3]; }
  function frame() {
    return new Promise(function (done) {
      var fired = false;
      function go() { if (!fired) { fired = true; done(); } }
      requestAnimationFrame(go);
      setTimeout(go, 50);
    });
  }
  var b0 = box();
  var sized = b0[2] > 0 && b0[3] > 0;
  var inView = sized && b0[0] + b0[2] > 0 && b0[1] + b0[3] > 0 &&
    b0[0] < innerWidth && b0[1] < innerHeight;
  if (sized && !inView) el.scrollIntoView({ block: 'center', inline: 'center' });
  return frame().then(function () {
    var b1 = box();
    return frame().then(function () {
      var b2 = box();
      var s = getComputedStyle(el);
      var shown = typeof el.checkVisibility === 'function'
        ? el.checkVisibility({ visibilityProperty: true, opacityProperty: false })
        : (s.visibility !== 'hidden' && s.display !== 'none');
      var x = b2[0] + b2[2] / 2, y = b2[1] + b2[3] / 2;
      var hit = document.elementFromPoint(x, y);
      while (hit && hit.shadowRoot) {
        var inner = hit.shadowRoot.elementFromPoint(x, y);
        if (!inner || inner === hit) break;
        hit = inner;
      }
      var onTarget = false;
      for (var n = hit; n; n = n.parentNode || n.host) { if (n === el) { onTarget = true; break; } }
      var disabled = el.matches(':disabled') || el.getAttribute('aria-disabled') === 'true' ||
        !!el.closest('[inert]');
      var kind = el.matches(':read-write') ? 'text' : (el.tagName === 'SELECT' ? 'select' : 'other');
      return {
        state: 'present', role: B.roleOf(el), name: B.nameOf(el), x: x, y: y,
        visible: b2[2] > 0 && b2[3] > 0 && shown,
        stable: inView && same(b0, b1) && same(b1, b2),
        hit: onTarget, enabled: !disabled, kind: kind
      };
    });
  });
"#,
    )
}

/// After a trusted click focused a text control: select its whole content
/// (`replace`) or put the caret at its end, so trusted text replaces or
/// appends. Fails with `not_focused` when the click did not focus it.
pub fn prepare_text(path: &str, replace: bool) -> String {
    let body = format!(
        r#"
  if (!el) return {{ error: 'stale_ref' }};
  var active = document.activeElement;
  if (active !== el && !(el.isContentEditable && el.contains(active))) return {{ error: 'not_focused' }};
  var replace = {replace};
  if (el.isContentEditable) {{
    var sel = getSelection(), range = document.createRange();
    range.selectNodeContents(el);
    if (!replace) range.collapse(false);
    sel.removeAllRanges(); sel.addRange(range);
  }} else {{
    var end = (el.value || '').length;
    try {{ el.setSelectionRange(replace ? 0 : end, end); }} catch (e) {{ if (replace) el.select(); }}
  }}
  return {{ ok: true }};
"#
    );
    with_ref(path, &body)
}

/// Whether the focused element takes typed text.
pub const FOCUSED_EDITABLE: &str = r#"(function(){
  var a = document.activeElement;
  return { editable: !!a && a !== document.body && (a.matches(':read-write') || a.isContentEditable) };
})()"#;

/// The viewport size, for page-level wheel input.
pub const VIEWPORT: &str = "({ w: innerWidth, h: innerHeight })";

/// Synthetic key press on the ref (or the focused element), without
/// moving focus. Enter on a button also activates it.
pub fn synthetic_press(path: Option<&str>, key: &str) -> String {
    let body = format!(
        r#"
  var target = document.activeElement && document.activeElement !== document.body ? document.activeElement : document.body;
  if (P !== null) {{ if (!el) return {{ error: 'stale_ref' }}; target = el; }}
  var key = {key};
  ['keydown', 'keypress', 'keyup'].forEach(function (t) {{
    target.dispatchEvent(new KeyboardEvent(t, {{ key: key, bubbles: true, cancelable: true }}));
  }});
  if (key === 'Enter' && target.tagName === 'BUTTON') target.click();
  return {{ ok: true }};
"#,
        key = literal(key)
    );
    match path {
        Some(path) => with_ref(path, &body),
        None => format!(
            "(function(){{ var P=null, el=null;{body} }})()"
        ),
    }
}

/// Synthetic value write with input and change events, without moving
/// focus: `<select>` choices and clearing a field.
pub fn synthetic_set_value(path: &str, value: &str) -> String {
    let body = format!(
        r#"
  if (!el) return {{ error: 'stale_ref' }};
  var v = {v};
  if (el.isContentEditable) {{ el.textContent = v; }}
  else {{
    var proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype
      : el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    var d = Object.getOwnPropertyDescriptor(proto, 'value');
    if (d && d.set) d.set.call(el, v); else el.value = v;
  }}
  el.dispatchEvent(new Event('input', {{ bubbles: true }}));
  el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  return {{ ok: true }};
"#,
        v = literal(value)
    );
    with_ref(path, &body)
}
