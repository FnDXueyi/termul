var MAX_ITEMS = 800;
var MAX_DEPTH = 24;
var LANDMARKS = ['heading', 'navigation', 'main', 'form', 'list', 'listitem', 'img', 'table'];
var TEXT_TAGS = ['P', 'BLOCKQUOTE', 'PRE', 'TD', 'TH', 'LABEL', 'HGROUP', 'CAPTION', 'FIGCAPTION'];
var SKIP_TAGS = ['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE'];
var items = [];
var capped = false;
function push(item) {
  if (items.length < MAX_ITEMS) items.push(item); else capped = true;
}
function walk(el, depth) {
  if (capped || depth > MAX_DEPTH) return;
  var tag = el.tagName;
  if (SKIP_TAGS.indexOf(tag) >= 0) return;
  if (tag === 'IFRAME') {
    push({ kind: 'landmark', depth: depth, role: 'iframe', name: '' });
    return;
  }
  var role = roleOf(el);
  var name = nameOf(el);
  if (isInteractive(el) && visible(el)) {
    var node = { kind: 'node', depth: depth, role: role || tag.toLowerCase(), name: name, path: pathOf(el) };
    if (tag === 'A' && el.href) node.href = String(el.href);
    if (tag === 'INPUT') {
      node.inputType = inputType(el);
      if (node.inputType === 'checkbox' || node.inputType === 'radio') node.checked = !!el.checked;
    }
    push(node);
    return;
  }
  if (LANDMARKS.indexOf(role) >= 0) {
    push({ kind: 'landmark', depth: depth, role: role, name: name });
  } else if (TEXT_TAGS.indexOf(tag) >= 0) {
    var text = name || (el.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 140);
    if (text) push({ kind: 'text', depth: depth, text: text });
  }
  for (var i = 0; i < el.children.length; i++) walk(el.children[i], depth + 1);
}
walk(document.body || document.documentElement, 0);
return { url: location.href, title: document.title || '', items: items, capped: capped };
