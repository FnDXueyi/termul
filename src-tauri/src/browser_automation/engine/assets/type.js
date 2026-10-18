var el = p.path ? byPath(p.path) : document.activeElement;
if (p.path && !el) return STALE;
if (!el || el === document.body) return { error: 'no focused element' };
var current = el.isContentEditable ? (el.textContent || '') : (el.value || '');
setValue(el, current + p.text);
changed(el);
return { ok: true };
