var el = byPath(p.path);
if (!el) return STALE;
el.scrollIntoView({ block: 'center', inline: 'center' });
return { rect: center(el), role: roleOf(el), name: nameOf(el) };
