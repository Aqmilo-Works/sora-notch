// A spring-driven face rig. Playback selects a choreographed performance,
// rather than claiming to measure the audio beat. Pointer attention takes priority.
const face=document.getElementById('face'),notch=document.getElementById('notch');
const rig=document.getElementById('face-rig'),left=document.getElementById('eye-left'),right=document.getElementById('eye-right'),mouth=document.getElementById('mascot-mouth');
let wasNotifying=false,completionStart=0;
let awakeUntil=0,notes=[],lastNote=-10,noteAlpha=0;
let hover=false,px=0,py=0,last=performance.now(),clock=0,blinkAt=2.8,blinkStart=-10;
const reduced=matchMedia('(prefers-reduced-motion: reduce)');
const state={x:0,y:0,roll:0,sx:1,sy:1,open:1,smile:9,hum:0,asym:0,wink:0};
const velocity=Object.fromEntries(Object.keys(state).map(k=>[k,0]));
const busy=()=>notch.classList.contains('mascot-wink')||notch.classList.contains('mascot-calm');
face.addEventListener('pointerenter',()=>{if(busy())return;hover=true;blinkAt=clock+3;blinkStart=-10});
face.addEventListener('pointerleave',()=>{if(busy())return;hover=false;awakeUntil=performance.now()+5000;px=py=0});
face.addEventListener('pointermove',e=>{if(busy())return;const r=face.getBoundingClientRect();px=Math.max(-1,Math.min(1,(e.clientX-r.left-r.width/2)/(r.width/2)));py=Math.max(-1,Math.min(1,(e.clientY-r.top-r.height/2)/(r.height/2)))});
const pulse=(t,center,width)=>Math.exp(-Math.pow((t-center)/width,2));
function eye(el,cx,cy,open,tilt,wink=0){
 const amount=Math.max(0,Math.min(1,open));const smooth=v=>v*v*(3-2*v);
 // Close the smile symmetrically into a dot before opening vertically.
 const curve=1-smooth(Math.min(1,amount/.4));const rise=smooth(Math.max(0,(amount-.4)/.6));
 const x=10*curve,y=10*rise,lean=tilt*curve;
 const p=[[cx-x,cy-y+lean],[cx-x*.5,cy+8*curve],[cx+x*.5,cy+8*curve],[cx+x,cy+y-lean]];
 const mid=(a,b)=>a.map((v,i)=>(v+b[i])/2),a=mid(p[0],p[1]),b=mid(p[1],p[2]),c=mid(p[2],p[3]),d=mid(a,b),e=mid(b,c),f=mid(d,e);
 const normal=[p[0],a,d,f,e,c,p[3]],chevron=[[cx-9,cy-12.5],[cx-3,cy-8.33],[cx+3,cy-4.17],[cx+9,cy],[cx+3,cy+4.17],[cx-3,cy+8.33],[cx-9,cy+12.5]];
 const w=Math.max(0,Math.min(1,wink)),points=normal.map((p,i)=>p.map((v,k)=>v+(chevron[i][k]-v)*w));
 el.setAttribute('d','M '+points[0].join(' ')+' C '+points.slice(1,4).map(p=>p.join(' ')).join(' ')+' C '+points.slice(4).map(p=>p.join(' ')).join(' '));
 el.setAttribute('stroke-width',String((9+5*amount)*(1-w)+9*w));

}
function frame(now){
 // Only the slow idle sway is playing: about 30 frames a second is enough (lighter on slow PCs). Blinks, hover, music and winks run at full rate.
 const lively=hover||now<awakeUntil||clock>blinkAt-.1||clock-blinkStart<.3||notch.classList.contains('playing')||notch.classList.contains('mascot-wink')||notch.classList.contains('mascot-calm');
 if(!lively&&now-last<30){requestAnimationFrame(frame);return}
 const elapsed=now-last;const dt=Math.min(elapsed/1000,.034);last=now;
 const notifying=notch.classList.contains('mascot-wink');
 if(notifying&&!wasNotifying)completionStart=now;
 if(!notifying&&wasNotifying)hover=face.matches(':hover');
 wasNotifying=notifying;
 const celebration=(now-completionStart)/1000;
 clock+=dt;
 // Nothing to draw while the panel is open or only the edge capsule shows: skip the rig instead of repainting a hidden face 100 times a second.
 if(!notch.classList.contains('compact')||document.body.classList.contains('edge')||document.body.classList.contains('panel-mode')){state.wink=velocity.wink=0;requestAnimationFrame(frame);return}
 const music=notch.classList.contains('playing');const calm=notifying||notch.classList.contains('mascot-calm');const attentive=!calm&&(hover||now<awakeUntil);const listening=music&&!attentive&&!calm;const t=clock%14;drawNotes(dt,listening&&!reduced.matches);
 if(clock>blinkAt){blinkStart=clock;blinkAt=clock+3.2+Math.random()*3.8}
 const b=clock-blinkStart;const blink=b<0||b>.26?0:b<.08?Math.sin(b/.08*Math.PI/2):b<.115?1:Math.cos((b-.115)/.145*Math.PI/2);
 let target={x:0,y:calm?0:Math.sin(clock*1.1)*.7,roll:0,sx:1,sy:1,open:1,smile:8,hum:0,asym:0};
 if(listening){
   // Anticipation, two accented nods, a relaxed sway, humming, then a relaxed hum.
   const nod=pulse(t,1.2,.22)+.8*pulse(t,1.85,.19)+pulse(t,8.1,.23)+.65*pulse(t,8.7,.18);
   const anticipate=pulse(t,.85,.22)+pulse(t,7.75,.22);
   const sway=Math.sin(clock*2.25)*Math.sin(Math.PI*t/14)**2;
   const hum=pulse(t,5.1,.8);
   target={x:sway*4.5,y:nod*4.3-anticipate*2.4+Math.sin(clock*4.5)*.7,roll:sway*7+Math.sin(clock*4.5)*1.5,sx:1+nod*.07-anticipate*.025,sy:1-nod*.08+anticipate*.06,open:0,smile:10+nod*3-hum*7,hum,asym:0};
 }else if(attentive){target.x=px*7;target.y=py*3-1;target.roll=px*3;target.open=1.05;target.smile=11;target.sy=1.025;target.asym=px*.05}
 else if(!calm){const glance=Math.sin(clock*.46);target.x=glance*2;target.roll=glance*.7;target.asym=pulse(t,9,.6)*.12}

 target.wink=0;
 if(notifying){
  const smooth=v=>{v=Math.max(0,Math.min(1,v));return v*v*(3-2*v)};
  const blend=smooth(celebration/.22)*smooth((3.2-celebration)/.55);
  const bounce=pulse(celebration,.35,.18),settle=pulse(celebration,.85,.3);
  const greeting={x:2+px*2,y:-1.5-2*bounce+settle,roll:-4-3*bounce+px,sx:1+.025*bounce,sy:1-.035*bounce,open:1,smile:11+2*bounce,hum:0,asym:0};
  for(const key of Object.keys(greeting))target[key]+=(greeting[key]-target[key])*blend;
  target.wink=smooth((celebration-.12)/.16)*smooth((2.25-celebration)/.3);
 }
 if(reduced.matches){target={x:0,y:0,roll:0,sx:1,sy:1,open:listening?0:1,smile:9,hum:0,asym:0,wink:notifying?1:0}}
 for(const key of Object.keys(state)){const stiffness=key==='wink'?420:key==='open'?260:170;const damping=key==='wink'?36:key==='open'?32:23;velocity[key]+=(stiffness*(target[key]-state[key])-damping*velocity[key])*dt;state[key]+=velocity[key]*dt;if(reduced.matches)state[key]=target[key]}
 rig.setAttribute('transform',`translate(${80+state.x} ${34+state.y}) rotate(${state.roll}) scale(${state.sx} ${state.sy}) translate(-80 -34)`);
 const eyelid=!notifying&&!listening&&!reduced.matches?1-blink:1;eye(left,42,31,state.open+state.asym,state.roll*.07,state.wink);eye(right,118,31,state.open-state.asym,-state.roll*.07);for(const lid of [left,right])lid.setAttribute("transform",`translate(0 31) scale(1 ${.24+.76*eyelid}) translate(0 -31)`);
 const half=16-7*state.hum;mouth.setAttribute('d',`M ${80-half} 39 C ${80-half*.6} ${39+state.smile} ${80+half*.6} ${39+state.smile} ${80+half} 39`);mouth.setAttribute('stroke-width',String(3.8+state.hum*1.2));
 requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

const noteLayer=document.createElementNS('http://www.w3.org/2000/svg','g');noteLayer.id='music-notes';document.getElementById('mascot').prepend(noteLayer);
function drawNotes(dt,show){
 noteAlpha+=(Number(show)-noteAlpha)*(1-Math.exp(-dt*10));
 if(!show){lastNote=clock-3.4;if(noteAlpha<.02)notes=[];}
 if(show&&clock-lastNote>=3.4){lastNote=clock;notes.push({born:clock,side:Math.random()>.5?1:-1});}
 notes=notes.filter(n=>clock-n.born<4.6).slice(-2);noteLayer.replaceChildren();noteLayer.setAttribute('opacity',noteAlpha.toFixed(3));
 for(const n of notes){const age=clock-n.born,u=age/4.6;const g=document.createElementNS(noteLayer.namespaceURI,'g');g.setAttribute('transform',`translate(${80+n.side*(86+Math.sin(u*4)*3)} ${62-u*53}) rotate(${n.side*(12-24*u)}) scale(.65)`);g.setAttribute('opacity',String(Math.min(1,age/.5,(4.6-age)/1.2)*.7));const path=document.createElementNS(noteLayer.namespaceURI,'path');path.setAttribute('d','M5 14V0L14-2V11M5 3L14 1M5 14c0 4-7 4-7 0s7-4 7 0M14 11c0 4-7 4-7 0s7-4 7 0');path.setAttribute('stroke-width','2.6');g.append(path);noteLayer.append(g);}
}

