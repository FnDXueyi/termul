pub const TARGET: &str = r#"<!doctype html><html><head><meta charset=utf-8><style>
body{margin:0;font:14px sans-serif;height:3000px}
#hov{position:absolute;left:20px;top:20px;width:160px;height:60px;background:#ccc}
#hov:hover{background:#f00}
#btn{position:absolute;left:220px;top:20px;width:120px;height:60px}
#inp{position:absolute;left:380px;top:20px;width:200px;height:30px}
#scroller{position:absolute;left:20px;top:120px;width:200px;height:150px;overflow:auto;background:#eef}
#drag{position:absolute;left:260px;top:120px;width:300px;height:150px;background:#efe}
#dnd{position:absolute;left:600px;top:120px;width:80px;height:60px;background:#fee}
#drop{position:absolute;left:600px;top:220px;width:80px;height:60px;background:#eee}
</style></head><body>
<div id=hov>hover</div><button id=btn>click</button><input id=inp>
<div id=scroller><div style="height:2000px">scroll</div></div>
<div id=drag>drag area</div>
<div id=dnd draggable=true>dnd</div><div id=drop>drop</div>
<script>
const log=[];
const rec=(e)=>{const t=e.target;const o={t:e.type,tr:e.isTrusted,id:(t&&(t.id||t.nodeName))||'?'};
 if(e.clientX!==undefined){o.x=Math.round(e.clientX);o.y=Math.round(e.clientY);o.b=e.buttons}
 if(e.type==='wheel'){o.dy=e.deltaY;o.dm=e.deltaMode}
 if(e.key!==undefined)o.k=e.key;
 log.push(o);if(log.length>300)log.shift();};
['mousemove','pointermove','mouseover','mousedown','mouseup','click','pointerdown','pointerup','keydown','keyup','keypress','input','beforeinput','wheel','scroll','dragstart','dragenter','dragover','drop','dragend','focus','blur'].forEach(t=>window.addEventListener(t,rec,true));
document.getElementById('drop').addEventListener('dragover',e=>e.preventDefault());
document.getElementById('drop').addEventListener('drop',e=>e.preventDefault());
window.__probe=()=>JSON.stringify({hover:document.getElementById('hov').matches(':hover'),inp:document.getElementById('inp').value,scrollTop:document.getElementById('scroller').scrollTop,winScrollY:window.scrollY,hasFocus:document.hasFocus(),active:document.activeElement?(document.activeElement.id||document.activeElement.nodeName):null,ua:navigator.userAgent,log:log.splice(0)});
</script></body></html>"#;

pub const CHAT: &str = r#"<!doctype html><html><head><meta charset=utf-8></head><body style="margin:0;background:#ddf">
<textarea id=chat style="width:400px;height:60px"></textarea>
<script>
const log=[];
['keydown','input','focus','blur','mousedown'].forEach(t=>window.addEventListener(t,e=>{log.push({t:e.type,tr:e.isTrusted,k:e.key});if(log.length>100)log.shift();},true));
window.__probe=()=>JSON.stringify({hasFocus:document.hasFocus(),active:document.activeElement?(document.activeElement.id||document.activeElement.nodeName):null,value:document.getElementById('chat').value,log:log.splice(0)});
</script></body></html>"#;

pub const PERM: &str = r#"<!doctype html><html><head><meta charset=utf-8></head><body>perm
<script>
window.__perm={secure:window.isSecureContext,hasMediaDevices:!!(navigator.mediaDevices&&navigator.mediaDevices.getUserMedia),hasGeo:!!navigator.geolocation,ua:navigator.userAgent,done:false};
const race=(p,ms)=>Promise.race([p,new Promise(r=>setTimeout(()=>r('timeout_pending'),ms))]);
async function gum(c){try{const s=await navigator.mediaDevices.getUserMedia(c);const n=s.getTracks().map(t=>t.kind+':'+t.label);s.getTracks().forEach(t=>t.stop());return 'granted '+n.join(',');}catch(e){return 'error '+e.name+': '+e.message;}}
function geo(){return new Promise(r=>{try{navigator.geolocation.getCurrentPosition(p=>r('granted'),e=>r('error code='+e.code+' '+e.message),{timeout:4000});}catch(e){r('throw '+e.name)}});}
(async()=>{
 try{const d=await navigator.mediaDevices.enumerateDevices();__perm.devices=d.map(x=>x.kind);}catch(e){__perm.devices='error '+e.name}
 __perm.camera=__perm.hasMediaDevices?await race(gum({video:true}),5000):'no_api';
 __perm.microphone=__perm.hasMediaDevices?await race(gum({audio:true}),5000):'no_api';
 __perm.geolocation=__perm.hasGeo?await race(geo(),6000):'no_api';
 try{__perm.notification=typeof Notification==='undefined'?'no_api':await race(Notification.requestPermission(),3000);}catch(e){__perm.notification='error '+e.name}
 __perm.done=true;
})();
window.__probe=()=>JSON.stringify(window.__perm);
</script></body></html>"#;

pub const BLANK: &str = "<!doctype html><html><body>blank</body></html>";
