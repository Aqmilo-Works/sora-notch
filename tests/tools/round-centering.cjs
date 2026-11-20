// Pixel audit: centre of each round background vs centre of what sits on it, per state.
const {chromium}=require('playwright');const http=require('node:http'),fs=require('node:fs'),path=require('node:path');
const root=path.resolve(process.argv[2]);
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',type(name));res.end(fs.readFileSync(file))});
const day=86400000,iso=d=>new Date(Date.now()+d*day).toISOString().slice(0,10);
const scenes=[
 {order:['calendar','water','todo'],water:false,items:[['#add-event',['none','hover']],['#calendar-prev',['hover']],['#calendar-next',['hover']],['#week .day.today',['none','hover']],['#week .day:not(.today):not(.past)',['hover','click']],['#water-bell',['none','hover']],['#water-undo',['none','hover']],['#water-drank',['none','hover']],['#todo-add',['none','hover']],['.td-remove',['hoverrow']]]},
 {order:['alarm','markets','timer'],water:true,items:[['#alarm-add',['none','hover']],['.al-remove',['hoverrow']],['#markets-edit',['none','hover']],['#timer-play',['none','hover']],['#timer-restart',['none','hover']]]},
 {order:['water','calendar','todo'],water:true,items:[['#water-bell',['none','hover']],['#settings-button',['none','hover']],['#history-button',['none','hover']],['#home-edit',['none','hover']]]},
 {order:['weather','water','prayer'],water:true,edit:true,items:[['#widget-done',['none','hover']],['#widget-cancel',['none','hover']]]},
];
(async()=>{await new Promise(r=>server.listen(18795,'127.0.0.1',r));
 const browser=await chromium.launch({channel:'msedge',headless:true});const out=[];
 for(const sc of scenes){
  const page=await browser.newPage({viewport:{width:760,height:250},deviceScaleFactor:4});
  await page.addInitScript(([order,water,todos])=>{localStorage.setItem('sora.home-widgets',JSON.stringify(order));localStorage.setItem('sora.todos',todos);localStorage.setItem('sora.alarms',JSON.stringify([{id:'a',label:'Wake',time:'06:30',on:true,repeat:true,ringAt:0}]));const L={};window.__emit=(n,p)=>L[n]?.({payload:p});
   window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name)=>{
    if(name==='health_get')return{prefs:{screen_time:true,water,water_minutes:30},today:9000,week:40000,apps:[],glasses:2,next_secs:water?1680:null};
    if(name==='quotes')return[{symbol:'BTC-USD',price:101234.5,previous:99800,points:[1,2,3]}];
    if(name==='monitors'||name==='shelf_list')return[];if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}}},[sc.order,sc.water,JSON.stringify([{id:'t0',title:'Makan Siang',date:iso(0),time:'12:00',done:false}])]);
  await page.goto('http://127.0.0.1:18795/index.html?panel=1&scale=1&tab=home');await page.waitForSelector('#home.active');
  await page.addStyleTag({content:'html,body{background:#050506!important}*{transition:none!important;animation:none!important}'});await page.evaluate(()=>window.__emit('panel-show'));await page.waitForTimeout(1200);
  if(sc.edit){await page.evaluate(()=>document.querySelector('#notch').classList.add('home-editing'));await page.waitForTimeout(200)}
  await page.addStyleTag({content:`.probe-noink,.probe-noink *{color:transparent!important;-webkit-text-fill-color:transparent!important;text-shadow:none!important}.probe-noink svg,.probe-noink svg *{stroke:transparent!important;fill:transparent!important}.probe-noink img{opacity:0!important}
   .probe-nocirc{background:transparent!important;box-shadow:none!important;outline:none!important}.probe-nocirc::before,.probe-nocirc::after{opacity:0!important}`});
  for(const [sel,states] of sc.items){
   const el=page.locator(sel).first();if(!await el.count()){out.push([sel,'-','missing']);continue}
   for(const st of states){
    await page.mouse.move(5,245);await page.waitForTimeout(60);
    if(st==='hover')await el.hover({force:true});
    if(st==='hoverrow'){await el.evaluate(e=>e.parentElement.scrollIntoView({block:'nearest'}));const b=await el.evaluate(e=>{const r=e.parentElement.getBoundingClientRect();return[r.x+20,r.y+r.height/2]});await page.mouse.move(b[0],b[1]);await page.waitForTimeout(80);await el.hover({force:true})}
    if(st==='click'){await el.click({force:true});await page.mouse.move(5,245)}
    await page.waitForTimeout(120);
    const box=await el.boundingBox();if(!box||!box.width){out.push([sel,st,'hidden']);continue}
    const clip={x:box.x-4,y:box.y-4,width:box.width+8,height:box.height+8};
    const shot=async(noInk,noCirc)=>{await el.evaluate((e,[a,b])=>{e.classList.toggle('probe-noink',a);e.classList.toggle('probe-nocirc',b)},[noInk,noCirc]);await page.waitForTimeout(40);return (await page.screenshot({clip})).toString('base64')};
    const base=await shot(true,true),circ=await shot(true,false),ink=await shot(false,false);
    const r=await page.evaluate(async([a,b,c,sx])=>{const load=async s=>{const im=await createImageBitmap(await (await fetch('data:image/png;base64,'+s)).blob());const cv=new OffscreenCanvas(im.width,im.height);const x=cv.getContext('2d');x.drawImage(im,0,0);return x.getImageData(0,0,im.width,im.height)};
     const B=await load(a),C=await load(b),I=await load(c);const bb=(D,R=B)=>{let x0=1e9,y0=1e9,x1=-1,y1=-1,n=0;for(let y=0;y<B.height;y++)for(let x=0;x<B.width;x++){const i=(y*B.width+x)*4;const d=Math.abs(D.data[i]-R.data[i])+Math.abs(D.data[i+1]-R.data[i+1])+Math.abs(D.data[i+2]-R.data[i+2]);if(d>18){n++;if(x<x0)x0=x;if(x>x1)x1=x;if(y<y0)y0=y;if(y>y1)y1=y}}return n?{cx:(x0+x1+1)/2/sx,cy:(y0+y1+1)/2/sx,w:(x1-x0+1)/sx,h:(y1-y0+1)/sx}:null};
     return{c:bb(C),i:bb(I,C)}},[base,circ,ink,4]);
    if(!r.c||!r.i){out.push([sel,st,r.c?'no ink':'no circle']);continue}
    out.push([sel,st,`dx ${(r.i.cx-r.c.cx).toFixed(2)} dy ${(r.i.cy-r.c.cy).toFixed(2)}  circle ${r.c.w.toFixed(1)}x${r.c.h.toFixed(1)} ink ${r.i.w.toFixed(1)}x${r.i.h.toFixed(1)}`]);
   }}
  await page.close()}
 for(const o of out)console.log(o.join(' | '));await browser.close();server.close()})();
