var a = document.activeElement;
return { editable: !!a && a !== document.body && (a.matches(':read-write') || a.isContentEditable) };
