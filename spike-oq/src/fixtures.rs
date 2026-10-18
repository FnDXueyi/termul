pub fn page(path: &str) -> Option<&'static str> {
    Some(match path {
        "/" | "/main.html" => MAIN,
        "/fixture.html" => FIXTURE,
        "/frame.html" => FRAME,
        "/storage.html" => STORAGE,
        _ => return None,
    })
}

const MAIN: &str = r#"<!doctype html><html><head><title>main</title></head><body>
<p>main webview (devUrl origin)</p><input id="focusme" autofocus>
<script>
window.__focusLog=[];
window.addEventListener('focus',function(){__focusLog.push('focus')});
window.addEventListener('blur',function(){__focusLog.push('blur')});
</script></body></html>"#;

const STORAGE: &str = r#"<!doctype html><html><head><title>storage</title></head><body>storage</body></html>"#;

const FRAME: &str = r#"<!doctype html><html><head><title>frame</title></head><body>frame
<script>window.__frameSecret='frame-main-world';</script></body></html>"#;

const FIXTURE: &str = r#"<!doctype html><html><head><title>fixture</title><style>
body{margin:0;font:14px sans-serif;height:4000px}
#hov{position:absolute;left:20px;top:20px;width:160px;height:40px;background:rgb(200,200,200);border:0}
#hov:hover{background:rgb(255,0,0)}
#slider{position:absolute;left:20px;top:80px;width:300px;height:20px;background:#ddd}
#handle{position:absolute;left:0;top:0;width:20px;height:20px;background:#333}
#drag{position:absolute;left:20px;top:120px;width:60px;height:40px;background:#9cf}
#drop{position:absolute;left:200px;top:120px;width:100px;height:60px;background:#fc9}
#frames{position:absolute;left:340px;top:20px}
#frames iframe{width:240px;height:60px;display:block}
</style></head><body>
<button id="hov">hover me</button>
<div id="slider"><div id="handle"></div></div>
<div id="drag" draggable="true">drag</div><div id="drop">drop</div>
<div id="frames"></div>
<script>
window.__ev=[];
function rec(e){__ev.push({t:e.type,id:(e.target&&e.target.id)||'',trusted:e.isTrusted,b:e.buttons})}
['mouseover','mouseenter','mousedown','mouseup','click','wheel','pointerover','dragstart','dragenter','dragover','drop','dragend'].forEach(function(t){document.addEventListener(t,rec,true)});
var lastMove=null;document.addEventListener('mousemove',function(e){lastMove={t:'mousemove',id:e.target.id||'',trusted:e.isTrusted,b:e.buttons};window.__moves=(window.__moves||0)+1;window.__lastMove=lastMove},true);
window.addEventListener('scroll',function(e){__ev.push({t:'scroll',trusted:e.isTrusted,y:scrollY})});
var h=document.getElementById('handle'),dragging=false,startX=0,startL=0;
h.addEventListener('mousedown',function(e){dragging=true;startX=e.clientX;startL=h.offsetLeft;e.preventDefault()});
document.addEventListener('mousemove',function(e){if(dragging){h.style.left=(startL+e.clientX-startX)+'px'}});
document.addEventListener('mouseup',function(){dragging=false});
document.getElementById('drag').addEventListener('dragstart',function(e){e.dataTransfer.setData('text/plain','payload')});
var d=document.getElementById('drop');d.addEventListener('dragover',function(e){e.preventDefault()});
d.addEventListener('drop',function(e){e.preventDefault();window.__dropped=e.dataTransfer.getData('text/plain')});
var q=new URLSearchParams(location.search);
(q.get('frames')||'').split(',').filter(Boolean).forEach(function(u){var f=document.createElement('iframe');f.src=u;document.getElementById('frames').appendChild(f)});
</script></body></html>"#;
