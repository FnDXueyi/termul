var INTERACTIVE =
  'a[href],button,input,select,textarea,summary,' +
  '[role=button],[role=link],[role=textbox],[role=checkbox],[role=switch],[role=combobox],' +
  '[role=listbox],[role=menuitem],[role=tab],[role=slider],[role=searchbox],' +
  '[contenteditable=true],[contenteditable=""],[onclick],[tabindex]';
var TAG_ROLE = {
  A: 'link', BUTTON: 'button', INPUT: 'textbox', SELECT: 'combobox',
  TEXTAREA: 'textbox', SUMMARY: 'button', H1: 'heading', H2: 'heading',
  H3: 'heading', H4: 'heading', H5: 'heading', H6: 'heading',
  NAV: 'navigation', MAIN: 'main', ASIDE: 'complementary',
  HEADER: 'banner', FOOTER: 'contentinfo', FORM: 'form', IMG: 'img',
  UL: 'list', OL: 'list', LI: 'listitem', TABLE: 'table'
};
function oneLine(value, max) {
  return String(value).replace(/\s+/g, ' ').trim().slice(0, max);
}
function inputType(el) {
  return oneLine(el.getAttribute('type') || 'text', 32).toLowerCase();
}
function roleOf(el) {
  var r = oneLine(el.getAttribute('role') || '', 64);
  if (r) return r;
  if (el.tagName === 'INPUT') {
    var t = inputType(el);
    if (t === 'checkbox') return 'checkbox';
    if (t === 'radio') return 'radio';
    if (t === 'submit' || t === 'button') return 'button';
    if (t === 'search') return 'searchbox';
    return 'textbox';
  }
  return TAG_ROLE[el.tagName] || '';
}
function checkState(el) {
  var t = el.tagName === 'INPUT' ? inputType(el) : '';
  if (t === 'checkbox' || t === 'radio') return { checked: !!el.checked, radio: t === 'radio' };
  var role = el.getAttribute('role');
  if (role === 'checkbox' || role === 'switch' || role === 'radio' || role === 'menuitemcheckbox') {
    return { checked: el.getAttribute('aria-checked') === 'true', radio: role === 'radio' };
  }
  return null;
}
function labelledText(el) {
  var ids = (el.getAttribute('aria-labelledby') || '').split(/\s+/);
  var out = '';
  for (var i = 0; i < ids.length; i++) {
    var t = document.getElementById(ids[i]);
    if (t) out += (t.innerText || '') + ' ';
  }
  return out.trim();
}
function nameOf(el) {
  // A password value never leaves the page: snapshots reach the agent's
  // model provider, and autofill would put credentials in the transcript.
  var secret = el.tagName === 'INPUT' && inputType(el) === 'password';
  var raw = el.getAttribute('aria-label') ||
    (el.getAttribute('aria-labelledby') ? labelledText(el) : '') ||
    el.getAttribute('title') || el.getAttribute('placeholder') ||
    el.getAttribute('alt') || el.innerText ||
    (secret ? (el.value ? '[redacted]' : '') : el.value) || '';
  return oneLine(raw, 100);
}
function visible(el) {
  if (!el.isConnected) return false;
  var r = el.getBoundingClientRect();
  if (r.width === 0 && r.height === 0) return false;
  var s = getComputedStyle(el);
  return s.visibility !== 'hidden' && s.display !== 'none';
}
function pathOf(el) {
  var segs = [];
  while (el && el.nodeType === 1 && el.tagName !== 'HTML') {
    var parent = el.parentElement;
    if (!parent) break;
    var tag = el.tagName;
    var same = Array.prototype.filter.call(parent.children, function (c) {
      return c.tagName === tag;
    });
    segs.unshift(tag.toLowerCase() + ':nth-of-type(' + (same.indexOf(el) + 1) + ')');
    el = parent;
  }
  return 'html>' + segs.join('>');
}
function isInteractive(el) {
  try { return el.matches(INTERACTIVE); } catch (e) { return false; }
}
function nodeRole(el) {
  return roleOf(el) || el.tagName.toLowerCase();
}
function locate(path) {
  try { return document.querySelector(path); } catch (e) { return null; }
}
function byPath(path) {
  var el = locate(path);
  return el && visible(el) ? el : null;
}
// A background tab may never paint, so a frame also ends on a timer.
function frame() {
  return new Promise(function (done) {
    var fired = false;
    function go() { if (!fired) { fired = true; done(); } }
    requestAnimationFrame(go);
    setTimeout(go, 50);
  });
}
function center(el) {
  var rc = el.getBoundingClientRect();
  return { cx: rc.left + rc.width / 2, cy: rc.top + rc.height / 2, w: rc.width, h: rc.height };
}
function setValue(el, value) {
  if (el.isContentEditable) {
    el.textContent = value;
    return;
  }
  var proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype
    : el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  var desc = Object.getOwnPropertyDescriptor(proto, 'value');
  if (desc && desc.set) desc.set.call(el, value); else el.value = value;
}
function changed(el) {
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
}
var STALE = { error: 'stale_ref' };
