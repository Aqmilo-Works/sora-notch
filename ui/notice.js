const pill=document.querySelector('#capsule');
const body=document.querySelector('.notice-body');
const neck=document.querySelector('#neck');
const notice=window.soraNotice;
const invoke=window.__TAURI__.core.invoke;
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const reduced=matchMedia('(prefers-reduced-motion: reduce)').matches;
// LocalSend (lan_receive.rs): "ask:<id>" asks with Deny / Accept, "busy" follows the transfer with a bar, "done".
const receive=notice.receive||'',asking=receive.startsWith('ask:'),busy=receive==='busy';
const acts=document.querySelector('.acts'),meter=document.querySelector('.meter');

// Where Sora puts things, in this window's css px: `det` the pill's top under the notch, `nb` the notch's bottom,
// `cap` the floating capsule while the notch is it (`attached`). The window itself only moves with the notch.
let place={native:false,attached:false,det:2,nb:0,cap:null},capW=52,capH=8,phase='start';
const W=()=>innerWidth-6;
// The pill's height (notice.css --h), with room under the words for LocalSend's buttons or bar.
const extra=()=>asking?(document.body.classList.contains('large')?46:40):busy?(document.body.classList.contains('large')?12:10):0;
const H=()=>(parseFloat(getComputedStyle(pill).getPropertyValue('--h'))||64)+extra();
// Its corner when full size (notice.css --r): a box with soft corners, not a pill.
const R=()=>parseFloat(getComputedStyle(pill).getPropertyValue('--r'))||17;

// Springs: everything moves by these, so a change in the middle of a move (the notch opening or closing) just bends it.
const springs={};
function spring(name,x,k=170,c=24){springs[name]={x,v:0,to:x,k,c}}
spring('top',2);spring('w',64);spring('h',10);spring('fill',0,260,32);spring('neck',0,260,30);spring('text',0,220,30);

function targets(){
 const a=place.attached&&place.cap,s=springs;
 if(phase==='shown'||phase==='leaving'){
  s.w.to=W();s.h.to=H();s.fill.to=1;
  s.top.to=a?place.nb:place.det;
  s.neck.to=a?1:0;
  // Leaving: the words go first, the shape follows (shrink).
  if(phase==='leaving')s.text.to=0;
 }else if(phase==='shrink'){
  s.text.to=0;s.neck.to=0;
  // Back into the floating capsule (exactly it, over the real one), or a small fading drop under the notch.
  if(a){s.top.to=place.cap.top;s.w.to=place.cap.w;s.h.to=place.cap.h;s.fill.to=1}
  else{s.top.to=place.det;s.w.to=64;s.h.to=10;s.fill.to=0}
 }
}
function setPlace(p){
 if(!p?.native)return;
 place=p;if(p.cap){capW=p.cap.w;capH=p.cap.h}
 targets();wake();
}
window.__TAURI__.event.listen('notice-place',({payload})=>setPlace(payload));

function draw(){
 const s=springs,w=Math.max(0,s.w.x),h=Math.max(0,s.h.x),r=Math.min(h/2,R()),top=s.top.x;
 Object.assign(pill.style,{top:top+'px',width:w+'px',height:h+'px',borderRadius:r+'px',opacity:Math.min(1,Math.max(0,s.fill.x))});
 body.style.opacity=acts.style.opacity=meter.style.opacity=Math.min(1,Math.max(0,s.text.x));
 // The neck: from under the notch down into the pill's top, with soft inner corners where it meets the pill.
 // It widens as the two join and thins out as they part.
 const n=Math.min(1,Math.max(0,s.neck.x)),nb=place.nb,cx=innerWidth/2;
 // It starts up inside the notch and keeps to the capsule's straight part, so only its soft corners show.
 const nw=Math.max(0,capW-capH)*n,y0=nb-8,flat=w/2-r-nw/2,f=Math.max(0,Math.min(8*n,top-y0,flat));
 // Parting, it pinches off at about a third of its width instead of thinning into a line.
 if(n<.3||top<y0||nw<1){neck.setAttribute('d','');return}
 const l=cx-nw/2,rt=cx+nw/2;
 neck.setAttribute('d',`M${l} ${y0}H${rt}V${top-f}Q${rt} ${top} ${rt+f} ${top}H${l-f}Q${l} ${top} ${l} ${top-f}Z`);
}
let raf=0,last=0;
function wake(){if(!raf){last=performance.now();raf=requestAnimationFrame(tick)}}
function tick(now){
 const dt=Math.min(.034,(now-last)/1000);last=now;let busy=false;
 for(const s of Object.values(springs)){
  if(reduced){s.x=s.to;s.v=0;continue}
  s.v+=(s.k*(s.to-s.x)-s.c*s.v)*dt;s.x+=s.v*dt;
  if(Math.abs(s.to-s.x)>.05||Math.abs(s.v)>.05)busy=true;else{s.x=s.to;s.v=0}
 }
 // The words come in once the pill is nearly full width, and leave first.
 if(phase==='shown')springs.text.to=springs.w.x>W()*.82?1:0;
 draw();
 raf=busy||phase==='shown'&&springs.text.x<1?requestAnimationFrame(tick):0;
}
const settled=()=>new Promise(resolve=>{const check=()=>raf?setTimeout(check,30):resolve();setTimeout(check,60)});

// How long the notice stays: the usual hold, or for LocalSend until it is answered (a minute at most) or the transfer ends.
function stay(){
 const hold=parseFloat(getComputedStyle(pill).getPropertyValue('--hold'));
 if(asking)return new Promise(resolve=>{
  const id=receive.slice(4),answer=accept=>{acts.querySelectorAll('button').forEach(b=>b.disabled=true);invoke('receive_decide',{id,accept}).catch(()=>{});resolve()};
  acts.querySelector('.accept').onclick=()=>answer(true);acts.querySelector('.deny').onclick=()=>answer(false);
  setTimeout(resolve,62000);
 });
 if(busy)return new Promise(resolve=>{
  window.__TAURI__.event.listen('receive-progress',({payload:p})=>{
   if(p.done)return resolve();
   if(p.line)document.querySelector('#detail').textContent=p.line;
   const pct=Math.round(Math.min(1,Math.max(0,p.frac))*100);meter.firstElementChild.style.width=pct+'%';document.querySelector('.pct').textContent=pct+'%';
  });
 });
 return delay(hold);
}

async function show(){
 pill.dataset.kind=['event','water','alarm','chat','agent','meeting','receive'].includes(notice.kind)?notice.kind:'timer';
 if(notice.large)document.body.classList.add('large');
 pill.style.setProperty('--row',(parseFloat(getComputedStyle(pill).getPropertyValue('--h'))||52)+'px');
 if(asking)acts.hidden=false;
 if(busy){meter.hidden=false;body.classList.add('busy');document.querySelector('.pct').textContent='0%'}
 // A messenger's message, as Windows shows it: the sender (a person or a group, on grey) with the app's mark as a small
 // badge in the corner, ringed in the card's black.
 const m=notice.mark;
 if(notice.kind==='chat'){
  const hex=n=>'#'+n.toString(16).padStart(6,'0');
  const who=notice.group?'M9.2 11a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4ZM3 19.6c.6-3 3-4.7 6.2-4.7s5.6 1.7 6.2 4.7M16 4.9a3 3 0 0 1 0 5.9M18 14.9c1.7.5 2.8 2 3.2 4.7':'M12 11.5a3.8 3.8 0 1 0 0-7.6 3.8 3.8 0 0 0 0 7.6ZM4.5 20.3c.8-3.5 3.9-5.3 7.5-5.3s6.7 1.8 7.5 5.3';
  const badge=m?`<rect x="24" y="24" width="17" height="17" rx="4.6" fill="${hex(m.color)}" stroke="#050506" stroke-width="2.2"/><g transform="translate(26.8 26.8) scale(.48)"><path class="app" fill="${hex(m.fg)}"/></g>`:'';
  const b=document.querySelector('.badge');
  b.innerHTML=`<svg class="avatar" viewBox="0 0 40 40"><circle cx="19" cy="19" r="19" fill="#3a3a3c" stroke="none"/><g transform="translate(7 6.5)" fill="none" stroke="#e5e5ea" stroke-width="1.9" stroke-linecap="round"><path class="who"/></g>${badge}</svg>`;
  b.querySelector('.who').setAttribute('d',who);if(m)b.querySelector('.app').setAttribute('d',m.path);
 }
 document.querySelector('#title').textContent=notice.title;
 document.querySelector('#detail').textContent=notice.detail;
 await document.fonts.ready;
 // LocalSend's question and transfer are a little wider, for the sender's name.
 const p=await invoke('notice_ready',{extra:extra(),wider:asking||busy?50:0});
 // Its buttons take clicks; every other notice lets them through to what is under it.
 if(asking)await invoke('notice_clickable').catch(()=>{});
 if(p?.native){
  place=p;if(p.cap){capW=p.cap.w;capH=p.cap.h}
  // Out of the floating capsule: the pill starts as exactly that capsule; otherwise as a small drop under the notch.
  if(p.attached&&p.cap){spring('top',p.cap.top);spring('w',p.cap.w);spring('h',p.cap.h);spring('fill',1,260,32)}
  else{spring('top',p.det);spring('w',64);spring('h',10)}
 }
 draw();
 await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
 phase='shown';targets();wake();if(!busy)pill.classList.add('counting');
 if(asking)pill.style.setProperty('--hold','62000ms');
 await stay();
 phase='leaving';spring('text',springs.text.x,520,46);targets();wake();
 await delay(200);
 // Whatever is left of the words is gone before the shape folds.
 phase='shrink';spring('text',0);targets();wake();
 await Promise.race([settled(),delay(1600)]);
}
show().finally(()=>invoke('notice_finished'));
