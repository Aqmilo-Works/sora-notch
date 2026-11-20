import {createSend} from './send.js';
import {segments,litCount} from './rings.js';

import {createTrayMenu} from './tray-menu.js';
import {initHomeWidgets} from './home-widgets.js';
import {initHomeCards} from './home-cards.js';
import {drawSky} from './weather-art.js';
import {mediaSourceInfo} from './media-sources.js';
import {initFeatures} from './features.js';
import {initLabs} from './labs.js';
import {PRAYER_METHODS,DEFAULT_METHOD,PRAYERS,PRAYER_NAMES,timesFor} from './prayer.js';
import {hasCjk,romanizeLines} from './vendor/cjk.js';
import {monthDays,alarmTimes,dueLabel,parseLrc,timerRemaining,dateKey,formatTime,fileSize,weatherLabel,skyOf,isFutureEvent,lyricChunks,vocalParts,lyricSections,reconcileMedia,expireDone} from './logic.js';
const $ = id => document.getElementById(id);
const paths = {
 board:'<path d="M9 5H7.5A2.5 2.5 0 0 0 5 7.5v11A2.5 2.5 0 0 0 7.5 21h9a2.5 2.5 0 0 0 2.5-2.5v-11A2.5 2.5 0 0 0 16.5 5H15"/><rect x="9" y="3" width="6" height="4" rx="1.5"/>',
agents:'<path d="M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8ZM18 15l.8 2.2L21 18l-2.2.8L18 21l-.8-2.2L15 18l2.2-.8Z"/>',
meeting:'<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"/>',
 history:'<path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1M3.5 4v4.5H8M12 7.5V12l3 2"/>',
 pin:'<path d="M9 4h6l-1 5 3 3H7l3-3ZM12 12v8"/>',
 home:'<path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1Z"/>',
 music:'<path d="M9 18V5l12-2v13M9 8l12-2"/><ellipse cx="6" cy="18" rx="3" ry="3"/><ellipse cx="18" cy="16" rx="3" ry="3"/>',
 shelf:'<path d="m3 10 3-7h12l3 7v10H3Z"/><path d="M3 11h5l2 4h4l2-4h5"/>',
 settings:'<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>',
 info:'<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5M12 7.6v.1"/>', down:'<path d="m6 9 6 6 6-6"/>', left:'<path d="m15 6-6 6 6 6"/>', right:'<path d="m9 6 6 6-6 6"/>',
 play:'<path d="m7 5 10 7-10 7Z" fill="currentColor" stroke="none"/>', pause:'<path d="M8 5v14M16 5v14" stroke-width="4"/>',
 previous:'<path d="M5 6v12m14-12L8 12l11 6Z" fill="currentColor"/>', next:'<path d="M19 6v12M5 6l11 6-11 6Z" fill="currentColor"/>',
 minus:'<path d="M5 7a8 8 0 1 1-1 8M2 3v5h5"/><text x="12" y="12.5" text-anchor="middle" dominant-baseline="central" font-size="8" stroke="none" fill="currentColor">10</text>',
 plus:'<path d="M19 7a8 8 0 1 0 1 8m2-12v5h-5"/><text x="12" y="12.5" text-anchor="middle" dominant-baseline="central" font-size="8" stroke="none" fill="currentColor">10</text>',
 hourglass:'<path d="M6 3h12M6 21h12M7 3v4c0 2 5 5 5 5s5-3 5-5V3M7 21v-4c0-2 5-5 5-5s5 3 5 5v4"/>',

 sun:'<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1"/>',
 moon:'<path d="M20 15A9 9 0 0 1 9 4a9 9 0 1 0 11 11Z"/>',
 cloud:'<path d="M6 18a5 5 0 1 1 1-10 6 6 0 0 1 11 1 4.5 4.5 0 0 1 0 9Z"/>',
 rain:'<path d="M6 15a4 4 0 1 1 1-8 6 6 0 0 1 11 1 4 4 0 0 1 1 7M8 18l-1 3m6-3-1 3m6-3-1 3"/>',
 trash:'<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7m4-7v7"/>',
 file:'<path d="M14 2H5v20h14V7Zm0 0v5h5M8 12h8m-8 4h5"/>',
 image:'<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8" cy="8" r="1"/><path d="m3 17 6-6 5 5 3-3 4 4"/>',
 code:'<path d="m8 6-6 6 6 6m8-12 6 6-6 6m-3-14-2 16"/>',
 folder:'<path d="M3 6V4h7l2 3h9v13H3Z"/>',
 lyrics:'<path d="M5 5h14M5 10h9M5 15h14M5 20h9"/>',
 send:'<path d="m3 11 17-8-6 18-3-7-8-3Z M11 14l9-11"/>',
 calendar:'<rect x="4" y="5" width="16" height="15" rx="2"/><path d="M4 10h16M8 3v4m8-4v4"/>',
 clock:'<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
 check:'<path d="m5 12.5 4.5 4.5L19 7.5"/>',
 close:'<path d="m6.5 6.5 11 11m0-11-11 11"/>',
 edit:'<path d="M5 19h3.5L19 8.5 15.5 5 5 15.5Z"/><path d="m13.5 7 3.5 3.5"/>',
 external:'<path d="M14 3h7v7m0-7-11 11M9 3H3v18h18v-6"/>',
 chart:'<path d="M5 20V11m7 9V4m7 16v-6"/>',
 gauge:'<path d="M4 17a8 8 0 1 1 16 0"/><path d="m12 17 4-5"/>',
 drop:'<path d="M12 3.5c3 3.6 6 7 6 10.5a6 6 0 0 1-12 0c0-3.5 3-6.9 6-10.5Z"/>',
 compass:'<circle cx="12" cy="12" r="8.5"/><path d="m15 9-4.2 1.8L9 15l4.2-1.8Z"/>',
 bell:'<path d="M6 15.25v-5a6 6 0 0 1 12 0v5l1.5 2h-15ZM10 19.75h4"/>',
 add:'<path d="M12 5v14M5 12h14"/>',
 less:'<path d="M5 12h14"/>',
 gamepad:'<rect x="3" y="7" width="18" height="11" rx="5"/><path d="M8 10.5v4M6 12.5h4M15.5 11.5h.01M17.5 13.5h.01"/>',
 steam:'<g transform="translate(12 12) scale(0.86) translate(-12 -12)"><path fill="currentColor" stroke="none" d="M11.979 0C5.678 0 .511 4.86.022 11.037l6.432 2.658c.545-.371 1.203-.59 1.912-.59.063 0 .125.004.187.006l2.861-4.142V8.91c0-2.495 2.028-4.524 4.524-4.524 2.494 0 4.524 2.031 4.524 4.527s-2.03 4.525-4.524 4.525h-.105l-4.076 2.911c0 .052.004.105.004.159 0 1.875-1.515 3.396-3.39 3.396-1.635 0-3.016-1.173-3.331-2.727L.436 15.27C1.862 20.307 6.486 24 11.979 24c6.627 0 11.999-5.373 11.999-12S18.605 0 11.979 0zM7.54 18.21l-1.473-.61c.262.543.714.999 1.314 1.25 1.297.539 2.793-.076 3.332-1.375.263-.63.264-1.319.005-1.949s-.75-1.121-1.377-1.383c-.624-.26-1.29-.249-1.878-.03l1.523.63c.956.4 1.409 1.5 1.009 2.455-.397.957-1.497 1.41-2.454 1.012H7.54zm11.415-9.303c0-1.662-1.353-3.015-3.015-3.015-1.665 0-3.015 1.353-3.015 3.015 0 1.665 1.35 3.015 3.015 3.015 1.663 0 3.015-1.35 3.015-3.015zm-5.273-.005c0-1.252 1.013-2.266 2.265-2.266 1.249 0 2.266 1.014 2.266 2.266 0 1.251-1.017 2.265-2.266 2.265-1.253 0-2.265-1.014-2.265-2.265z"/></g>',
 chevron:'<path d="m9 6 6 6-6 6"/>',
 'bell-off':'<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15ZM10 20.5h4M4 4l16 16"/>'
};
const icon = name => `<svg viewBox="0 0 24 24" aria-hidden="true">${paths[name] || paths.file}</svg>`;
for(const [id,name] of Object.entries({'info-button':'info','history-button':'history','settings-button':'settings','shelf-clear':'trash','board-clear':'trash','history-clear':'trash','calendar-prev':'left','calendar-next':'right','timer-play':'play',previous:'previous',next:'next','play-pause':'play','tray-symbol':'shelf','home-edit':'edit','todo-add':'add','add-event':'add','water-drank':'add','water-undo':'less','alarm-add':'add','markets-edit':'edit','pick-prev':'left','pick-next':'right'})) $(id).innerHTML = icon(name);
document.querySelectorAll('[data-tab]').forEach(b => b.innerHTML = icon(b.dataset.tab));
const native = !!window.__TAURI__;
if(/Macintosh|Mac OS X/.test(navigator.userAgent))document.documentElement.classList.add('mac');
// Panel mode: the native notch draws the notch and the mascot and opens this page only while the panel is open (?panel&scale=&tab=).
const PANEL = new URLSearchParams(location.search).has('panel');
if (PANEL) document.body.classList.add('panel-mode');
const invoke = (command,args={}) => native ? window.__TAURI__.core.invoke(command,args) : Promise.reject(new Error('Run Sora.exe to use Windows features'));
function read(key,fallback){try{return JSON.parse(localStorage.getItem(key)) ?? fallback}catch{return fallback}}
function persist(key,value){localStorage.setItem(key,JSON.stringify(value))}
let settings={scale:1,top:0,monitor:0,autoCollapse:false,city:'Jakarta Timur',latitude:-6.225,longitude:106.9,...read('nook.settings',{})};
settings.scale=0.85;
let expanded=false, currentTab='home', transition=0, autoClose, activeDialog=false;
let romanWanted=read('nook.romanize',false),romanLines=[],romanGeneration=0,romanBusy=false;
let files=[],media=null,lyrics=[],lyricsTrack='',mediaBusy=false,mediaPoll=0,weatherBusy=false;
let events=read('nook.agenda',[]); if(!Array.isArray(events))events=[];
let selectedDate=new Date(),weekAnchor=new Date(),lastDate=dateKey(new Date());
let timer=read('nook.timer',{total:1500,remaining:1500,end:null});
if(!Number.isFinite(timer.total)||timer.total<=0||!Number.isFinite(timer.remaining))timer={total:1500,remaining:1500,end:null};
let completed=false,toastTimeout,focusNote='';
// Focus extras: minutes focused per day (30 days), shown in the Focus menu.
let focus=read('sora.focus',{});if(!focus||typeof focus!=='object'||Array.isArray(focus))focus={};if(!focus.log||typeof focus.log!=='object')focus.log={};
function saveFocus(){persist('sora.focus',focus)}
function logFocus(sec){const k=dateKey(new Date()),d=focus.log[k]||{sec:0,n:0};d.sec+=sec;d.n+=1;focus.log[k]=d;const keys=Object.keys(focus.log).sort();while(keys.length>30)delete focus.log[keys.shift()];saveFocus()}
function toast(message){$('toast').textContent=String(message);$('toast').classList.add('show');clearTimeout(toastTimeout);toastTimeout=setTimeout(()=>$('toast').classList.remove('show'),3500)}
function showError(error){toast(error.message || error)}
let prayerSync=0;
function saveSettings(){persist('nook.settings',settings);clearTimeout(prayerSync);prayerSync=setTimeout(()=>syncCalendarNotices(),2000)}
let edgeHandleEnabled=false,edgePreferenceVersion=0;
function setEdgePreference(enabled){
 edgeHandleEnabled=enabled;
 if(enabled&&visibilityAnimation){
  visibilitySequence++;visibilityAnimation.cancel();visibilityAnimation=null;
  $('notch').style.translate='0 0';
 }
}
let visibilityAnimation=null,visibilitySequence=0,holdEdgeMode=false,pendingEdgeMode=false;
async function slidePanel(visible){const el=$('notch'),sequence=++visibilitySequence,style=getComputedStyle(el),from={translate:edgeHandleEnabled||style.translate==='none'?'0 0':style.translate,opacity:style.opacity};visibilityAnimation?.cancel();const to={translate:visible||edgeHandleEnabled?'0 0':'0 -110%',opacity:visible?'1':'0'};Object.assign(el.style,to);const animation=el.animate([from,to],{duration:matchMedia('(prefers-reduced-motion: reduce)').matches?0:visible?420:280,easing:visible?'cubic-bezier(.16,1,.3,1)':'cubic-bezier(.4,0,.8,.3)'});visibilityAnimation=animation;try{await animation.finished;return sequence===visibilitySequence}catch{return false}finally{if(sequence===visibilitySequence)visibilityAnimation=null}}
let dragOffered=false,dragHold=0,trayHoldTimer=0,trayHeldTab=null,edgeTrayFast=false;
const TRAY_HOLD_MS=10000;
// After a drop the tab that took the files (Tray or Send) stays open for the next file, never closing on a timer:
// outside clicks (picking the next file) are ignored until you switch to another tab or app, or close the panel.
function armTrayHold(){
 clearTimeout(trayHoldTimer);trayHeldTab=currentTab;
 const renew=()=>{if(!expanded||currentTab!==trayHeldTab){trayHeldTab=null;return}invoke('tray_hold',{ms:30000}).catch(()=>{});trayHoldTimer=setTimeout(renew,25000)};
 renew();
}
// Another tab: the hold ends, so a click outside closes the panel again.
function releaseTrayHold(name){if(!trayHeldTab||name===trayHeldTab)return;clearTimeout(trayHoldTimer);trayHeldTab=null;if(!activeDialog)invoke('tray_hold',{ms:0}).catch(()=>{})}
// ---- Edge handle: while the pointer rests on the capsule it grows, in place, into the mini notch ----
// The CSS derives the capsule's whole shape from two animated numbers: --p (capsule -> mini notch) and --q (mini notch -> tray).
const edgeEl=$('edge'),edgePill=edgeEl.querySelector('.edge-pill'),edgeEyes=[...edgePill.querySelectorAll('b')],edgeMouth=edgePill.querySelector('.edge-face');
let edgeAnims=[],edgeDoneAt=-1e9,edgeGrowDone=null;
const reducedMotion=()=>matchMedia('(prefers-reduced-motion: reduce)').matches;
const wait=ms=>new Promise(r=>setTimeout(r,ms));
// One continuous curve per size change (no per-segment easing, so no stop-and-go in the middle).
const SHRINK='cubic-bezier(.45,0,.25,1)',SPRING='cubic-bezier(.3,1.35,.5,1)';
const edgeNum=(prop,from,to,opts)=>edgePill.animate([{[prop]:String(from)},{[prop]:String(to)}],opts);
// The mascot's eyes are 14-unit round-capped strokes, 20 units tall, 76 units apart, on a 160x70 face drawn at 0.9625 x notch scale.
function faceGeom(){const s=Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1,k=s*.9625;return{w:14*k,h:34*k,r:7*k,gap:62*k,lift:-4*k}}
const eyeSmall=()=>({width:'7px',height:'10px',borderRadius:'4px',translate:'0px 0px'});
const eyeBig=()=>{const g=faceGeom();return{width:g.w.toFixed(2)+'px',height:g.h.toFixed(2)+'px',borderRadius:g.r.toFixed(2)+'px',translate:'0px '+g.lift.toFixed(2)+'px'}};
function wakeMascot(){const f=$('face');f.dispatchEvent(new PointerEvent('pointerenter'));f.dispatchEvent(new PointerEvent('pointerleave'))}
function edgeStop(){edgeAnims.forEach(a=>a.cancel());edgeAnims=[]}
function edgeReset(){edgeStop();edgeGrowDone=null;edgeEl.classList.remove('hot','handoff','plain')}
// scale0: the capsule's current "breathing" scale, eased to 1 so starting the morph never snaps.
function easeBreathing(scale0){if(scale0&&scale0!=='none'&&scale0!=='1')edgeAnims.push(edgePill.animate([{scale:scale0},{scale:'1'}],{duration:260,easing:'ease-out'}))}
function edgeStart(ms,scale0='none',eyes=true){
 const progress=parseFloat(getComputedStyle(edgePill).getPropertyValue('--p'))||0;
 const opacity=edgeEyes.map(e=>getComputedStyle(e).opacity);
 edgeStop();edgeGrowDone=null;if(reducedMotion())return;
 edgeAnims=[
  edgeNum('--p',progress,1,{duration:ms,easing:'linear',fill:'forwards'}),
  ...(eyes?edgeEyes.map((e,i)=>e.animate([{opacity:progress?opacity[i]:0},{opacity:1,offset:.18},{opacity:1}],{duration:ms,fill:'forwards'})):[])
 ];
 easeBreathing(scale0);
}
function edgeRetreat(){
 if(!edgeAnims.length)return;
 const progress=parseFloat(getComputedStyle(edgePill).getPropertyValue('--p'))||0;
 const opacity=edgeEyes.map(e=>getComputedStyle(e).opacity);
 edgeStop();
 const duration=reducedMotion()?0:180+210*progress;
 const opts={duration,easing:'cubic-bezier(.27,.05,.22,1)',fill:'forwards'};
 const running=[edgeNum('--p',progress,0,opts),...edgeEyes.map((e,i)=>e.animate([{opacity:opacity[i]},{opacity:0}],opts))];
 edgeAnims=running;
 Promise.all(running.map(a=>a.finished)).then(()=>{if(edgeAnims===running)edgeStop()}).catch(()=>{});
}
// The capsule is now exactly the compact notch. It blinks, and the blink opens into the mascot's eyes and smile. speed < 1 is faster.
function edgeFinish(speed=1,plain=false){
 edgeDoneAt=performance.now();edgeAnims.forEach(a=>a.finish());edgeEl.classList.add('handoff');
 if(reducedMotion()||plain)return;
 wakeMascot();
 const g=faceGeom(),d=n=>Math.round(n*speed),blink=edgeEyes.map(e=>e.animate([{scale:'1 1'},{scale:'1 .12',offset:.5},{scale:'1 1'}],{duration:d(190),easing:'ease-in-out'}));
 const grow=edgeEyes.map(e=>e.animate([eyeSmall(),eyeBig()],{duration:d(380),delay:d(190),easing:SPRING,fill:'both'}));
 const gap=edgePill.animate([{gap:'18px'},{gap:g.gap.toFixed(2)+'px'}],{duration:d(380),delay:d(190),easing:SPRING,fill:'both'});
 const smile=edgeMouth.animate([{opacity:0},{opacity:1}],{duration:d(240),delay:d(300),fill:'both'});
 edgeAnims.push(...blink,...grow,gap,smile);
 edgeGrowDone=Promise.all([...blink,...grow,gap,smile].map(a=>a.finished)).catch(()=>{});
}
// The real notch takes over from the identical capsule in one frame: no fade.
function swapToNotch(){const el=$('notch');visibilitySequence++;visibilityAnimation=null;Object.assign(el.style,{translate:'0 0',opacity:'1'});edgeReset();setTimeout(()=>maybeIntro(true),250)}
// Revealed (shortcut, tray, timer) while only the capsule is showing: it grows into the mini notch right where it floats.
async function peekFromEdge(){
 preparePeek(true);clearTimeout(autoClose);const turn=++transition;expanded=false;
 $('notch').classList.remove('from-edge');$('notch').classList.add('compact');$('expanded').inert=true;$('face').tabIndex=0;
 const breathing=getComputedStyle(edgePill).scale;
 edgeEl.classList.add('handoff','hot');
 $('notch').classList.add('peek-sizing');void $('notch').offsetWidth;await applyPlacement();$('notch').classList.remove('peek-sizing');
 if(turn!==transition)return;
 if(reducedMotion()){swapToNotch();return}
 edgeStart(500,breathing);
 try{await edgeAnims[0].finished}catch{return}
 edgeFinish(.65);await Promise.race([edgeGrowDone||Promise.resolve(),wait(900)]);if(turn===transition)swapToNotch();
}
// Use the real, populated Tray for the entire morph, starting at the capsule's current bounds.
function openTray() {
 const notch=$('notch');
 if(currentTab!=='shelf')tab('shelf');
 if(expanded&&!document.body.classList.contains('edge'))return;
 if(!notch.classList.contains('from-edge')&&(document.body.classList.contains('edge')||edgeEl.classList.contains('handoff'))) {
   const bounds=edgePill.getBoundingClientRect(), scale=Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1;
   notch.style.setProperty('--start-width',bounds.width/scale+'px');
   notch.style.setProperty('--start-height',bounds.height/scale+'px');
   notch.style.setProperty('--start-top',bounds.top+'px');
   notch.style.setProperty('--start-radius',parseFloat(getComputedStyle(edgePill).borderBottomLeftRadius)/scale+'px');
   notch.classList.add('from-edge','compact','peek-sizing');
   void notch.offsetWidth;
   notch.classList.remove('peek-sizing');
 }
 visibilitySequence++;visibilityAnimation?.cancel();visibilityAnimation=null;
 edgeReset();edgeTrayFast=false;
 document.body.classList.remove('edge');
 Object.assign(notch.style,{translate:'0 0',opacity:'1'});
 return panel(true);
}
const peekAuto=()=>document.body.classList.contains('edge')?peekFromEdge():peekIn(edgeHandleEnabled);
// Hiding with the handle on: the notch swaps for the identical capsule, its eyes shrink back through a blink, and it shrinks into the floating capsule.
async function morphToCapsule(){
 const sequence=++visibilitySequence;visibilityAnimation?.cancel();visibilityAnimation=null;
 edgeStop();edgeEl.classList.remove('hot');
 wakeMascot();await wait(170);
 if(sequence!==visibilitySequence)return false;
 const g=faceGeom(),el=$('notch');
 edgeEl.classList.add('handoff');
 const shrink=edgeEyes.map(e=>e.animate([eyeBig(),eyeSmall()],{duration:260,easing:SHRINK,fill:'forwards'}));
 const blink=edgeEyes.map(e=>e.animate([{scale:'1 1'},{scale:'1 1',offset:.55},{scale:'1 .12',offset:.78},{scale:'1 1'}],{duration:260}));
 const gap=edgePill.animate([{gap:g.gap.toFixed(2)+'px'},{gap:'18px'}],{duration:260,easing:SHRINK,fill:'forwards'});
 const smile=edgeMouth.animate([{opacity:1},{opacity:0}],{duration:200,fill:'forwards'});
 const hideEyes=edgeEyes.map(e=>e.animate([{opacity:1},{opacity:0}],{duration:200,delay:260+260,fill:'both'}));
 const pill=edgeNum('--p',1,0,{duration:480,delay:260,easing:SHRINK,fill:'both'});
 edgeAnims=[...shrink,...blink,gap,smile,...hideEyes,pill];
 el.style.opacity='0';
 try{await pill.finished}catch{return false}
 Object.assign(el.style,{translate:'0 0',opacity:'0'});
 if(sequence!==visibilitySequence)return false;
 edgeStop();edgeEl.classList.remove('handoff');return true;
}
async function peekIn(instant){
 preparePeek(instant);clearTimeout(autoClose);
 const turn=++transition;expanded=false;
 const notch=$('notch');notch.classList.remove('from-edge');notch.classList.add('compact','peek-sizing');
 $('expanded').inert=true;$('face').tabIndex=0;
 void notch.offsetWidth;await applyPlacement();notch.classList.remove('peek-sizing');
 if(turn!==transition)return;
 if(instant){
  await Promise.race([edgeGrowDone||Promise.resolve(),wait(1100)]);
  if(turn!==transition)return;
  if(edgeTrayFast)await openTray();else swapToNotch();
 }else{slidePanel(true);setTimeout(maybeIntro,500)}
}
function preparePeek(instant){visibilitySequence++;visibilityAnimation?.cancel();visibilityAnimation=null;if(!instant)edgeReset();$('notch').style.translate=instant||edgeHandleEnabled?'0 0':'0 -110%';$('notch').style.opacity='0';}
// The native window keeps its size (it is only clipped to the mini notch when closed), so the page never lays out in a stale frame.
const resizeWindow=args=>invoke('set_panel',args);
async function panel(open){
  if(!open)trayMenu.close();
  if(PANEL){if(!open&&native)invoke('panel_close').catch(()=>{});return}
  if(open===expanded)return;
  // Clicked while the capsule is still turning into the mini notch: finish that hand-over first, then open.
  if(open&&edgeEl.classList.contains('handoff'))swapToNotch();
  clearTimeout(autoClose);if(!open){clearTimeout(trayHoldTimer);trayHeldTab=null}expanded=open;if(open)cancelIntro();const turn=++transition;
  if(open){
    if(native)await resizeWindow({expanded:true,scale:settings.scale,top:settings.top,monitor:settings.monitor,home:currentTab==='home'}).catch(showError);
    if(turn!==transition)return;
    $('notch').classList.remove('compact');$('expanded').inert=false;$('face').tabIndex=-1;scheduleClose();
  }else{
    const notch=$('notch');
    notch.classList.remove('from-edge');
    notch.classList.add('compact');$('expanded').inert=true;$('face').tabIndex=0;
    // Shrink the HWND only when the current CSS morph has finished (also handles reversals and reduced motion).
    await Promise.allSettled(notch.getAnimations().filter(a=>a instanceof CSSTransition).map(a=>a.finished));
    if(turn!==transition)return;
    if(native)await resizeWindow({expanded:false,scale:settings.scale,top:settings.top,monitor:settings.monitor,home:currentTab==='home'}).catch(showError);
  }
}
const EDITOR_TABS=['location-editor','event-editor','todo-editor','timer-editor','alarm-editor','markets-editor'];
function tab(name){releaseTrayHold(name);if(name!=='shelf')trayMenu.close();if(name!=='home')homeWidgets?.leave();if(PANEL&&native)invoke('panel_view',{name}).catch(()=>{});hideFieldNote();if(EDITOR_TABS.includes(currentTab))activeDialog=false;currentTab=name;if(EDITOR_TABS.includes(name))activeDialog=true;$('notch').dataset.view=name;document.querySelectorAll('.view').forEach(v=>v.classList.toggle('active',v.id===name));document.querySelectorAll('[data-tab]').forEach(b=>{const on=b.dataset.tab===name||(b.dataset.tab==='home'&&name.endsWith('-editor'));b.classList.toggle('active',on);b.setAttribute('aria-pressed',String(on))});$('shelf-clear').classList.toggle('hidden',name!=='shelf'||!files.length);if(name==='music')pollMedia();if(name==='home')homeCards?.refresh();if(name==='shelf')refreshShelf();if(native)invoke('send_focus',{on:name==='send'}).catch(()=>{});if(name==='send')sendView.enter();features?.entered(name);labs?.entered(name);$('history-button').classList.toggle('active',name==='history');$('info-wrap').classList.toggle('hidden',name!=='send');}
$('face').onclick=()=>panel(true);
document.addEventListener('dblclick',e=>{e.preventDefault();e.stopPropagation()},true);
document.querySelectorAll('[data-tab]').forEach(b=>b.onclick=()=>tab(b.dataset.tab));
let startupBusy=false;
$('settings-button').onclick=()=>{if(native)invoke('open_settings').catch(showError)};
$('history-button').onclick=()=>tab(currentTab==='history'?'home':'history');
const showInfo=()=>{const t=sendView.help();$('info-tip').textContent=t;$('info-button').setAttribute('aria-label',t)};$('info-wrap').addEventListener('pointerenter',showInfo);$('info-button').addEventListener('focus',showInfo);
document.addEventListener('keydown',e=>{if(e.key==='Escape'){if(EDITOR_TABS.includes(currentTab))tab('home');else panel(false)}if(e.ctrlKey&&['1','2','3','4'].includes(e.key)){e.preventDefault();panel(true);tab(['home','music','shelf','send'][Number(e.key)-1])}});
function scheduleClose(){clearTimeout(autoClose)}
$('notch').addEventListener('mouseenter',()=>clearTimeout(autoClose));
$('notch').addEventListener('mouseleave',scheduleClose);

let calendarSync=Promise.resolve();
function syncCalendarNotices(){
 if(!native)return;
 const entries=events.filter(e=>!e.done).map(e=>({
  id:String(e.id??[e.date,e.time,e.title].join('|')),title:e.title,
  at:new Date(e.date+'T'+e.time).getTime(),time:e.time,note:e.note||''
 })).filter(e=>Number.isFinite(e.at)&&typeof e.title==='string'&&typeof e.time==='string');
 entries.push(...alarms.filter(a=>a.on).flatMap(a=>alarmRings(a).map(at=>({id:'alarm-'+a.id+'-'+at,title:a.label||'Alarm',at,time:a.time,kind:'alarm'}))));
 entries.push(...prayerRings());
 // Submit snapshots in edit order, including quick add/delete actions.
 calendarSync=calendarSync.then(()=>invoke('sync_calendar_notices',{entries})).catch(showError);
}
// The five daily prayers ring like alarms (today and tomorrow), from the weather location; no widget on Home needed.
function prayerRings(){
 const lat=Number(settings.latitude),lon=Number(settings.longitude);
 if(!Number.isFinite(lat)||!Number.isFinite(lon))return[];
 const method=PRAYER_METHODS[settings.prayerMethod]?settings.prayerMethod:DEFAULT_METHOD,asr=settings.asr==='hanafi'?'hanafi':'shafi',out=[];
 for(const add of[0,1]){
  const day=new Date();day.setDate(day.getDate()+add);const times=timesFor(day,lat,lon,method,asr);
  for(const k of PRAYERS){
   const m=times[k];if(m==null)continue;
   const at=new Date(day.getFullYear(),day.getMonth(),day.getDate(),0,m).getTime(),time=String(Math.floor(m/60)%24).padStart(2,'0')+':'+String(m%60).padStart(2,'0');
   out.push({id:'prayer-'+dateKey(day)+'-'+k,title:PRAYER_NAMES[k],at,time,kind:'alarm'});
  }
 }
 return out;
}
setInterval(()=>syncCalendarNotices(),1800000);
function saveAgenda(){persist('nook.agenda',events);syncCalendarNotices()}
// Ticked events leave a day after they were ticked: checked at start and every minute.
function pruneDone(){const r=expireDone(events);if(!r.changed)return;const gone=r.events.length!==events.length;events=r.events;saveAgenda();if(gone)renderWeek()}
setInterval(pruneDone,60000);
// The calendar shows the whole month of the selected date (Sunday first); days outside the month stay blank.
function renderWeek(){
  const y=selectedDate.getFullYear(),m=selectedDate.getMonth(),today=dateKey(new Date());
  $('month').textContent=selectedDate.toLocaleDateString('en-US',{month:'long'});$('month').dataset.year=y===new Date().getFullYear()?'':String(y);
  $('week').classList.toggle('current-week',y===new Date().getFullYear()&&m===new Date().getMonth());$('week').replaceChildren();
  for(const d of monthDays(y,m)){
    if(!d){$('week').append(document.createElement('span'));continue}
    const b=document.createElement('button'),key=dateKey(d);b.className='day';b.classList.toggle('today',key===today);b.classList.toggle('selected',key===dateKey(selectedDate));b.classList.toggle('past',key<today);b.classList.toggle('has-events',events.some(e=>e.date===key));b.setAttribute('aria-label',d.toLocaleDateString('en-US',{dateStyle:'full'}));b.setAttribute('aria-pressed',String(key===dateKey(selectedDate)));
    b.textContent=d.getDate();b.onclick=()=>{selectedDate=d;renderWeek()};$('week').append(b);
  }
  $('week').dataset.rows=Math.ceil(monthDays(y,m).length/7);const key0=dateKey(selectedDate);
  const tmr=new Date();tmr.setDate(tmr.getDate()+1);$('cal-weekday').textContent=key0===today?'Today':key0===dateKey(tmr)?'Tomorrow':selectedDate.toLocaleDateString('en-US',{weekday:'short',day:'numeric',month:'short'});$('cal-date').textContent=selectedDate.getDate();
  $('add-event').disabled=dateKey(selectedDate)<today;renderAgenda();
}
function renderAgenda(){
  $('agenda').replaceChildren();$('agenda').scrollTop=0;let dayEvents=events.filter(e=>e.date===dateKey(selectedDate)).sort((a,b)=>a.time.localeCompare(b.time));
  // Never an empty list: a free day shows what comes next, and with nothing planned at all, an invitation.
  const nowKey=dateKey(new Date())+' '+new Date().toTimeString().slice(0,5),ahead=!dayEvents.length&&events.filter(e=>!e.done&&e.date+' '+e.time>=nowKey).sort((a,b)=>(a.date+a.time).localeCompare(b.date+b.time)).slice(0,3);
  $('agenda').classList.toggle('ahead',!!(ahead&&ahead.length));if(ahead&&ahead.length)dayEvents=ahead;
  if(!dayEvents.length){const empty=document.createElement('div');empty.className='agenda-empty';empty.textContent='Nothing planned';$('agenda').append(empty)}
  for(const event of dayEvents){const row=document.createElement('div');row.className='event';const title=document.createElement('span');title.className='event-name';title.textContent=event.title;const time=document.createElement('time');time.textContent=event.date===dateKey(selectedDate)?event.time:new Date(event.date+'T'+event.time).toLocaleDateString('en-US',{weekday:'short',day:'numeric',month:'short'})+' · '+event.time;const remove=document.createElement('button');remove.className='event-remove';remove.innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17"/></svg>';remove.setAttribute('aria-label',`Delete ${event.title}`);remove.onclick=()=>{events=events.filter(e=>e.id!==event.id);saveAgenda();renderWeek()};
    const check=document.createElement('button');check.className='event-check';check.setAttribute('role','checkbox');check.setAttribute('aria-checked',String(!!event.done));check.setAttribute('aria-label',(event.done?'Mark not done: ':'Mark done: ')+event.title);
    check.onclick=()=>{event.done=!event.done;if(event.done)event.doneAt=Date.now();else delete event.doneAt;saveAgenda();renderAgenda()};row.classList.toggle('done',!!event.done);
    row.append(check,title,time,remove);$('agenda').append(row)}
  const left=dayEvents.filter(e=>!e.done).length;$('cal-count').textContent=$('agenda').classList.contains('ahead')?'Free day · next up':dayEvents.length?left+(left===1?' upcoming event':' upcoming events'):'';$('cal-more').textContent=dayEvents.length>2?(dayEvents.length-2)+' more':'';
}
// ---- To-do widget: things done every day (work, habits), apart from the calendar's events. Each one is ticked for
// today only: the next day it is open again. Kept per user as {id,title,time:'' or 'HH:MM',doneOn:'YYYY-MM-DD'}.
let todos=read('sora.todos',[]);if(!Array.isArray(todos))todos=[];
// Tasks saved before the repeat switch existed came back every day.
todos.forEach(t=>{if(typeof t.daily!=='boolean')t.daily=true});
function saveTodos(){persist('sora.todos',todos);renderTodo()}
function renderTodo(){
 const today=dateKey(new Date()),now=new Date().toTimeString().slice(0,5),done=t=>t.doneOn===today;
 // A task that does not repeat goes the day after it is ticked.
 {const kept=todos.filter(t=>t.daily||!t.doneOn||t.doneOn===today);if(kept.length!==todos.length){todos=kept;persist('sora.todos',todos)}}
 const list=[...todos].sort((a,b)=>(done(a)-done(b))||((a.time||'99')).localeCompare(b.time||'99')||a.title.localeCompare(b.title));
 $('todo-count').textContent=todos.filter(t=>!done(t)).length||'';
 if(!list.length){const p=document.createElement('p');p.className='td-empty';p.textContent='Nothing to do';$('todo-list').replaceChildren(p)}
 else $('todo-list').replaceChildren(...list.map(t=>{const r=document.createElement('div'),c=document.createElement('button'),n=document.createElement('span'),d=document.createElement('time'),x=document.createElement('button');
  r.className='td'+(done(t)?' done':'')+(!done(t)&&t.time&&t.time<now?' late':'');c.type='button';c.className='td-check';c.setAttribute('role','checkbox');c.setAttribute('aria-checked',String(done(t)));c.setAttribute('aria-label',(done(t)?'Mark not done: ':'Mark done: ')+t.title);
  c.onclick=()=>{t.doneOn=done(t)?'':today;saveTodos()};n.textContent=t.title;d.textContent=t.time||'';
  x.type='button';x.className='td-remove';x.setAttribute('aria-label','Delete '+t.title);x.innerHTML=icon('close');x.onclick=()=>{todos=todos.filter(o=>o!==t);saveTodos()};
  r.append(c,n,d,x);return r}));
 // the editor's list of every to-do, to remove one there too
 $('todo-all').replaceChildren(...(todos.length?[]:[Object.assign(document.createElement('p'),{className:'mk-none',textContent:'Nothing yet.'})]),...todos.map(t=>{const b=document.createElement('button');b.type='button';b.className='mk-row';b.innerHTML='<b></b><small></small><i aria-hidden="true">×</i>';b.querySelector('b').textContent=t.title;b.querySelector('small').textContent=(t.daily?'Every day':'Today')+(t.time?' · '+t.time:'');b.setAttribute('aria-label','Delete '+t.title);b.onclick=()=>{todos=todos.filter(o=>o!==t);saveTodos()};return b}));
}
setInterval(()=>{if(homeWidgets?.has('todo'))renderTodo()},60000);
// System (src/sysmon.rs): CPU, GPU, memory, disk and network, every 2 s while Home shows it.
const GB=1024**3;
const rate=b=>b==null?'—':b<1024?Math.round(b)+' B/s':b<1024**2?(b/1024).toFixed(b<10240?1:0)+' KB/s':(b/1024**2).toFixed(1)+' MB/s';
setInterval(()=>{if(currentTab==='home'&&document.body.classList.contains('ready')&&!document.body.classList.contains('closing'))renderSystem()},1000);
function sysRow(name,pct,value){const r=document.createElement('div'),n=document.createElement('b'),v=document.createElement('span');r.className='sys';n.textContent=name;v.className='val';v.textContent=value;r.dataset.level=pct>=90?'high':pct>=70?'mid':'ok';r.append(n,v,segments(pct??0,{count:16}));return r}
let systemBusy=false,systemShown='';
async function renderSystem(){if(!native||systemBusy||!homeWidgets?.has('system'))return;systemBusy=true;let s;try{s=await invoke('system_stats')}catch{return}finally{systemBusy=false}
 const pct=v=>v==null?'—':Math.round(v)+'%',mem=s.mem_total?s.mem_used/s.mem_total*100:null;
 const net='↓ '+rate(s.down)+'  ↑ '+rate(s.up);if($('sys-net').textContent!==net)$('sys-net').textContent=net;
 // Rebuild the rows only when a shown value or bar changes (most seconds nothing does).
 // CPU with its clock now, GPU with its temperature (when the driver reports one).
 const cpuText=pct(s.cpu)+(s.cpu_ghz?' · '+s.cpu_ghz.toFixed(2)+' GHz':''),gpuText=pct(s.gpu)+(s.gpu_temp?' · '+Math.round(s.gpu_temp)+'°C':'');
 const seg=v=>litCount(v??0,16),key=[cpuText,seg(s.cpu),gpuText,seg(s.gpu),s.mem_total?(s.mem_used/GB).toFixed(1):'',seg(mem),pct(s.disk),seg(s.disk)].join();if(key===systemShown&&$('sys-list').childElementCount)return;systemShown=key;
 $('sys-list').replaceChildren(sysRow('CPU',s.cpu,cpuText),sysRow('GPU',s.gpu,gpuText),sysRow('Memory',mem,s.mem_total?(s.mem_used/GB).toFixed(1)+' / '+Math.round(s.mem_total/GB)+' GB':'—'),sysRow('Disk',s.disk,pct(s.disk)))}
let todoAnytime=true,todoDaily=false,alarmRepeat=false;
function setTodoAnytime(on){todoAnytime=on;$('todo-editor').classList.toggle('anytime',on);markWheel($('wheel-th'))}
function setTodoDaily(on){todoDaily=on;$('todo-daily').setAttribute('aria-checked',String(on))}
$('todo-daily').onclick=()=>setTodoDaily(!todoDaily);
$('todo-add').onclick=()=>{tab('todo-editor');setTodoDaily(false);$('todo-title').value='';renderTodo();setWheels(9,0,'wheel-th','wheel-tm');requestAnimationFrame(()=>requestAnimationFrame(()=>setTodoAnytime(true)));$('todo-title').focus()};
$('todo-back').onclick=()=>tab('home');
// turning a wheel means a time is wanted
['wheel','pointerdown','keydown'].forEach(n=>document.querySelector('.todo-wheels').addEventListener(n,()=>{if(todoAnytime)setTodoAnytime(false)},{passive:true}));
$('todo-save').onclick=()=>{const title=$('todo-title').value.trim();if(!title){fieldNote($('todo-title'),'Please fill out this field');return}
 todos.push({id:crypto.randomUUID(),title:title.slice(0,60),daily:todoDaily,time:todoAnytime?'':String(wheelValue($('wheel-th'))).padStart(2,'0')+':'+String(wheelValue($('wheel-tm'))).padStart(2,'0'),doneOn:''});saveTodos();tab('home');toast('To-do added')};
const shiftMonth=n=>{const now=new Date(),d=new Date(selectedDate.getFullYear(),selectedDate.getMonth()+n,1);selectedDate=d.getFullYear()===now.getFullYear()&&d.getMonth()===now.getMonth()?now:d;weekAnchor=new Date(selectedDate);renderWeek()};
$('calendar-prev').onclick=()=>shiftMonth(-1);$('calendar-next').onclick=()=>shiftMonth(1);
function refreshEventLimits(){const now=new Date();$('event-date').min=dateKey(now);$('event-time').min=$('event-date').value===dateKey(now)?new Date(Math.ceil((Date.now()+1)/60000)*60000).toTimeString().slice(0,5):'';}
$('add-event').onclick=()=>{tab('event-editor');const next=new Date(Math.ceil((Date.now()+1)/60000)*60000);const date=dateKey(selectedDate);$('event-date').value=date<=dateKey(next)?dateKey(next):date;$('event-time').value=date<=dateKey(next)?next.toTimeString().slice(0,5):'09:00';$('event-title').value='';$('event-note').value='';$('note-count').textContent='0/120';$('event-error').textContent='';refreshEventLimits();const d0=new Date($('event-date').value+'T12:00:00');pickMonth=new Date(d0.getFullYear(),d0.getMonth(),1);syncPickers();const [h0,m0]=$('event-time').value.split(':').map(Number);setWheels(h0,m0);$('event-title').focus()};
$('event-date').onchange=()=>{refreshEventLimits();$('event-error').textContent=''};
// ---- Date and time pickers in the notch's own style (the native inputs only store the values) ----
// The New event panel: a month to pick the day from, the time wheels and a note, all on one screen.
function syncPickers(){renderDayPicker()}
function renderDayPicker(){
 const today=dateKey(new Date()),chosen=$('event-date').value||today,y=pickMonth.getFullYear(),m=pickMonth.getMonth();
 $('pick-month').textContent=pickMonth.toLocaleDateString('en-US',{month:'long'})+(y===new Date().getFullYear()?'':' '+y);
 const now=new Date();$('pick-prev').disabled=y*12+m<=now.getFullYear()*12+now.getMonth();
 $('pick-days').replaceChildren(...monthDays(y,m).map(d=>{
  if(!d)return document.createElement('span');
  const key=dateKey(d),b=document.createElement('button');b.type='button';b.className='pk-day'+(key===today?' today':'')+(key===chosen?' sel':'')+(events.some(e=>e.date===key)?' has-ev':'');
  b.dataset.day=d.getDate();b.textContent=d.getDate();b.disabled=key<today;b.setAttribute('aria-label',d.toLocaleDateString('en-GB',{dateStyle:'full'}));
  b.onclick=()=>{commit('event-date',key)};return b}));
 $('pick-days').dataset.rows=Math.ceil(monthDays(y,m).length/7);
}
// When an alarm set for h:m rings next, as words.
function nextRing(h,m){const now=new Date(),at=new Date(now);at.setHours(h,m,0,0);if(at<=now)at.setDate(at.getDate()+1);const min=Math.round((at-now)/60000),hh=Math.floor(min/60),mm=min%60;return 'in '+(hh?hh+' h ':'')+(mm||!hh?mm+' min':'').trim()}
function setWheels(h,m,hw='wheel-h',mw='wheel-m'){requestAnimationFrame(()=>{$(hw).scrollTop=h*WHEEL;$(mw).scrollTop=m*WHEEL;markWheel($(hw));markWheel($(mw))})}
$('pick-prev').onclick=()=>{pickMonth=new Date(pickMonth.getFullYear(),pickMonth.getMonth()-1,1);renderDayPicker()};
$('pick-next').onclick=()=>{pickMonth=new Date(pickMonth.getFullYear(),pickMonth.getMonth()+1,1);renderDayPicker()};
['input','change'].forEach(n=>['event-date','event-time'].forEach(id=>$(id).addEventListener(n,syncPickers)));
const commit=(id,value)=>{const e=$(id);e.value=value;e.dispatchEvent(new Event('input',{bubbles:true}));e.dispatchEvent(new Event('change',{bubbles:true}))};
let pickMonth=new Date(new Date().getFullYear(),new Date().getMonth(),1);
// Validation in the notch's own style instead of the browser's bubble.
for(const f of document.forms)f.noValidate=true;
let noteTimer=0;
function hideFieldNote(){clearTimeout(noteTimer);$('field-note').classList.remove('show');document.querySelectorAll('.field-bad').forEach(e=>e.classList.remove('field-bad'))}
function fieldNote(input,message){
 const note=$('field-note'),n=$('notch').getBoundingClientRect(),r=input.getBoundingClientRect(),s=Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1;
 note.querySelector('span').textContent=message;note.classList.remove('above');note.style.left='0px';note.style.top='0px';
 const w=note.offsetWidth,h=note.offsetHeight;let left=(r.left-n.left)/s,top=(r.bottom-n.top)/s+9;
 if(top+h>n.height/s-6){top=(r.top-n.top)/s-h-9;note.classList.add('above')}
 const x=Math.max(8,Math.min(left,n.width/s-w-8));
 note.style.left=x+'px';note.style.top=top+'px';note.style.setProperty('--ax',Math.max(16,Math.min(w-26,left+24-x))+'px');
 note.classList.add('show');input.classList.add('field-bad','field-shake');setTimeout(()=>input.classList.remove('field-shake'),420);input.focus();
 clearTimeout(noteTimer);noteTimer=setTimeout(hideFieldNote,2600);
}
document.addEventListener('input',e=>{if(e.target.classList?.contains('field-bad'))hideFieldNote()},true);
const WHEEL=30;
const wheelValue=el=>Math.max(0,Math.min(el.children.length-1,Math.round(el.scrollTop/WHEEL)));
function markWheel(el){const v=wheelValue(el);[...el.children].forEach((b,i)=>b.classList.toggle('on',i===v));const view=el.closest('.view'),[h,m]=view.querySelectorAll('.wheel');const t=String(wheelValue(h)).padStart(2,'0')+':'+String(wheelValue(m)).padStart(2,'0');view.querySelector('.wheel-title').textContent=view.id==='todo-editor'?(todoAnytime?'No time':t):t;if(view.id==='event-editor'&&$('event-time').value!==t)commit('event-time',t)}
function buildWheel(el,count){
 el.replaceChildren();
 for(let i=0;i<count;i++){const b=document.createElement('button');b.type='button';b.className='wheel-item';b.textContent=String(i).padStart(2,'0');b.tabIndex=-1;b.onclick=()=>{if(el.dataset.moved)return;el.scrollTo({top:i*WHEEL,behavior:'smooth'})};el.append(b)}
 el.addEventListener('scroll',()=>markWheel(el),{passive:true});
 el.addEventListener('keydown',e=>{if(e.key==='ArrowUp'||e.key==='ArrowDown'){e.preventDefault();el.scrollBy({top:(e.key==='ArrowUp'?-1:1)*WHEEL,behavior:'smooth'})}});
 let drag=null;
 el.addEventListener('pointerdown',e=>{if(e.button!==0)return;drag={y:e.clientY,top:el.scrollTop};delete el.dataset.moved;el.setPointerCapture(e.pointerId);el.classList.add('dragging')});
 el.addEventListener('pointermove',e=>{if(!drag)return;const dy=e.clientY-drag.y;if(Math.abs(dy)>4)el.dataset.moved='1';el.scrollTop=drag.top-dy/(Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1)});
 const end=()=>{if(!drag)return;drag=null;el.classList.remove('dragging');el.scrollTo({top:wheelValue(el)*WHEEL,behavior:'smooth'});setTimeout(()=>delete el.dataset.moved,60)};
 el.addEventListener('pointerup',end);el.addEventListener('pointercancel',end);
}
buildWheel($('wheel-h'),24);buildWheel($('wheel-m'),60);buildWheel($('wheel-ah'),24);buildWheel($('wheel-am'),60);buildWheel($('wheel-th'),24);buildWheel($('wheel-tm'),60);
syncPickers();
$('event-cancel').onclick=()=>tab('home');
$('event-note').addEventListener('input',()=>{$('note-count').textContent=$('event-note').value.length+'/120'});
$('event-form').onsubmit=e=>{e.preventDefault();const title=$('event-title').value.trim();if(!title){fieldNote($('event-title'),'Please fill out this field');return}if(!isFutureEvent($('event-date').value,$('event-time').value)){refreshEventLimits();$('event-error').textContent='Choose a date and time in the future.';return}events.push({id:crypto.randomUUID(),title,date:$('event-date').value,time:$('event-time').value,note:$('event-note').value.trim().slice(0,120)});saveAgenda();selectedDate=new Date($('event-date').value+'T12:00:00');weekAnchor=new Date(selectedDate);renderWeek();tab('home');toast('Event saved')};

let focusQueue=Promise.resolve(),lastFocusMessage='';
function syncWindowsFocus(){const deadline=timer.end;focusQueue=focusQueue.then(async()=>{try{const result=await invoke('sync_windows_focus',{endTime:deadline});if(timer.end!==deadline)return;const message=result.message||'';focusNote=result.active?message:'';document.querySelector('[data-widget=timer]').dataset.windowsFocus=String(!!result.active);lastFocusMessage=message}catch{const message='Windows Focus could not be updated. Check Focus in the Windows calendar.';focusNote='';document.querySelector('[data-widget=timer]').dataset.windowsFocus='false';lastFocusMessage=message}})}
function saveTimer(){persist('nook.timer',timer);syncWindowsFocus();if(PANEL&&native&&!completed)invoke('timer_deadline',{end:timer.end||null}).catch(()=>{})}
function renderTimerDigits(id,value){const el=$(id);if(el.dataset.digits===value)return;el.dataset.digits=value;while(el.children.length>value.length)el.lastChild.remove();if(el.firstChild?.nodeType===Node.TEXT_NODE)el.replaceChildren();while(el.children.length<value.length){const digit=document.createElement('span');digit.className='timer-digit';el.append(digit)}[...el.children].forEach((digit,i)=>digit.textContent=value[i]);}
// The Focus dial: a half circle of minutes, turned like a camera's dial. The minute under the marker at the top is the
// time left; the blue band runs from 0 to the marker. Scroll or drag it to set the length (whole minutes, 1 to 180).
const DIAL={cx:100,cy:100,r:92,deg:8};let dialKey='';
function renderDial(remaining){
 const rem=remaining/60,key=rem.toFixed(3);if(key===dialKey)return;dialKey=key;
 const {cx,cy,r,deg}=DIAL,pt=(a,rr)=>[cx+rr*Math.sin(a*Math.PI/180),cy-rr*Math.cos(a*Math.PI/180)],f=n=>n.toFixed(2);
 // A full half circle as the track, so the dial reads as one clean shape whatever the time is.
 let out='<path class="track" d="M'+(cx-(r-10))+' '+cy+'A'+(r-10)+' '+(r-10)+' 0 0 1 '+(cx+(r-10))+' '+cy+'"/>';const zero=-rem*deg;
 if(rem>0){const a0=Math.max(-90,zero),[x0,y0]=pt(a0,r-10),[x1,y1]=pt(0,r-10);out+='<path class="band" d="M'+f(x0)+' '+f(y0)+'A'+(r-10)+' '+(r-10)+' 0 0 1 '+f(x1)+' '+f(y1)+'"/>'}
 for(let i=Math.max(0,Math.ceil(rem-90/deg));i<=Math.min(180,Math.floor(rem+90/deg));i++){
  const a=(i-rem)*deg,major=i%5===0,[x0,y0]=pt(a,r-(major?3:5)),[x1,y1]=pt(a,r-(major?17:13));
  out+='<path class="'+(a<=0?'in':major?'major':'minor')+'" d="M'+f(x0)+' '+f(y0)+'L'+f(x1)+' '+f(y1)+'"/>';
  if(major&&Math.abs(a)<84){const [tx,ty]=pt(a,r-28);out+='<text x="'+f(tx)+'" y="'+f(ty)+'" transform="rotate('+f(a)+' '+f(tx)+' '+f(ty)+')">'+i+'</text>'}
 }
 out+='<path class="marker" d="M'+cx+' '+(cy-r-2)+'l-5-7h10ZM'+cx+' '+(cy-r-2)+'V'+(cy-r+20)+'"/>';
 $('timer-dial').innerHTML=out;$('timer-dial-wrap').setAttribute('aria-valuenow',String(Math.round(rem)));
}
function setFocusMinutes(m){
 m=Math.max(1,Math.min(180,Math.round(m)));timer.total=m*60;timer.remaining=m*60;if(timer.end)timer.end=Date.now()+m*60000;completed=false;saveTimer();renderTimer();
}
{
 const wrap=$('timer-dial-wrap');let drag=null;
 wrap.addEventListener('wheel',e=>{e.preventDefault();setFocusMinutes(Math.round(timerRemaining(timer)/60)+(e.deltaY>0?1:-1))},{passive:false});
 wrap.addEventListener('keydown',e=>{const d={ArrowRight:1,ArrowUp:1,ArrowLeft:-1,ArrowDown:-1}[e.key];if(d){e.preventDefault();setFocusMinutes(Math.round(timerRemaining(timer)/60)+d)}});
 wrap.addEventListener('pointerdown',e=>{if(e.button!==0||e.target.closest('#timer-play'))return;drag={x:e.clientX,m:Math.round(timerRemaining(timer)/60)};wrap.setPointerCapture(e.pointerId);wrap.classList.add('turning')});
 // One minute per tick spacing at the rim: dragging left brings larger minutes under the marker.
 wrap.addEventListener('pointermove',e=>{if(!drag)return;const ppm=wrap.getBoundingClientRect().width/200*DIAL.r*DIAL.deg*Math.PI/180;const m=Math.round(drag.m-(e.clientX-drag.x)/ppm);if(m!==Math.round(timerRemaining(timer)/60))setFocusMinutes(m)});
 const end=()=>{drag=null;wrap.classList.remove('turning')};wrap.addEventListener('pointerup',end);wrap.addEventListener('pointercancel',end);
}
function renderTimer(){const remaining=timerRemaining(timer);$('timer-edit').classList.toggle('long-time',remaining>=6000);renderTimerDigits('minutes',String(Math.floor(remaining/60)).padStart(2,'0'));renderTimerDigits('seconds',String(remaining%60).padStart(2,'0'));{const want=timer.end?'pause':'play',play=$('timer-play');if(play.dataset.icon!==want){play.innerHTML=icon(want);play.dataset.icon=want}}$('timer-play').setAttribute('aria-label',timer.end?'Pause timer':'Start timer');$('mini-timer').textContent=timer.end?formatTime(remaining):'';$('timer-label').textContent=completed?'Focus complete':timer.end?'Focusing':'Focus Timer';updateSand(remaining);renderDial(remaining);document.querySelector('[data-widget=timer]').classList.toggle('done',completed);if(timer.end&&remaining===0){timer.end=null;timer.remaining=0;completed=true;logFocus(timer.total);saveTimer();if(!PANEL){chime();notifyTimerComplete()};renderTimer()}}
let timerNoticeTimeout;
// The mascot's wink used to play here. For now it is the start-up greeting (playIntro) and the timer has no animation.
function notifyTimerComplete(){
 if(native)invoke('notify_focus_complete').catch(showError);
}
let introDone=false,introTimeout,greetTimeout;
// Opening the panel mid-greeting ends it for good: no wink, calm or hidden-mascot state may survive into the next mini notch.
function cancelIntro(){clearTimeout(introTimeout);clearTimeout(greetTimeout);$('notch').classList.remove('mascot-wink','mascot-calm');$('mascot').classList.remove('intro-hidden','intro-pop')}
function playIntro(force=false){
 if(introDone&&!force)return;introDone=true;
 if(!$('notch').classList.contains('compact')||document.body.classList.contains('edge'))return;
 clearTimeout(introTimeout);$('notch').classList.add('mascot-wink');
 introTimeout=setTimeout(()=>$('notch').classList.remove('mascot-wink'),3200);
}
// First time the mini notch is on screen after launch (not under automation, which triggers it by event).
// Start-up: the notch drops in first; only then the mascot pops in and greets. Until the greeting ends the mascot is calm
// (no idle sway, no listening pose or notes, no pointer following), so nothing else moves.
function prepareGreeting(){$('mascot').classList.add('intro-hidden');$('notch').classList.add('mascot-calm')}
function greet(){
 if(expanded){cancelIntro();return}
 const face=$('mascot');face.classList.remove('intro-hidden');face.classList.add('intro-pop');playIntro(true);
 setTimeout(()=>face.classList.remove('intro-pop'),700);clearTimeout(greetTimeout);greetTimeout=setTimeout(()=>$('notch').classList.remove('mascot-calm'),3300);
}
async function startupReveal(){
 const shown=await invoke('window_visible').catch(()=>true);
 if(!shown||document.body.classList.contains('edge'))return;
 // Let fonts and the first paint settle so none of it lands in the middle of the capsule's growth.
 await document.fonts.ready;await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));   // hidden at launch: the first reveal plays the greeting (maybeIntro)
 prepareGreeting();introDone=true;if(edgeHandleEnabled){await peekFromEdge()}else{await slidePanel(true);await wait(140)}greet();
}
// fromCapsule: the capsule's blink already opened into the mascot, so the greeting is only the wink (no hide-and-pop again).
function maybeIntro(fromCapsule=false){if(introDone||navigator.webdriver)return;introDone=true;if(expanded)return;
 if(fromCapsule===true){$('notch').classList.add('mascot-calm');playIntro(true);clearTimeout(greetTimeout);greetTimeout=setTimeout(()=>$('notch').classList.remove('mascot-calm'),3300);return}
 prepareGreeting();setTimeout(greet,160)}
let audio;
function chime(){try{audio ||= new AudioContext();[523.25,659.25,783.99].forEach((hz,i)=>{const o=audio.createOscillator(),gain=audio.createGain(),t=audio.currentTime+i*.17;o.frequency.value=hz;gain.gain.setValueAtTime(0,t);gain.gain.linearRampToValueAtTime(.08,t+.02);gain.gain.exponentialRampToValueAtTime(.001,t+.65);o.connect(gain);gain.connect(audio.destination);o.start(t);o.stop(t+.7)})}catch{}}
$('timer-play').onclick=()=>{try{audio ||= new AudioContext();audio.resume()}catch{}completed=false;if(timer.end){timer.remaining=timerRemaining(timer);timer.end=null}else{if(timer.remaining===0)timer.remaining=timer.total;timer.end=Date.now()+timer.remaining*1000}saveTimer();renderTimer()};
$('timer-restart').onclick=()=>{const running=!!timer.end;timer.remaining=timer.total;timer.end=running?Date.now()+timer.total*1000:null;completed=false;saveTimer();renderTimer()};

// Long names keep their extension: "Proposal-f….pdf" instead of "Proposal-final.…"
function shortFileName(name){if(name.length<=13)return name;const dot=name.lastIndexOf('.'),ext=dot>0&&name.length-dot<=6?name.slice(dot):'';return name.slice(0,Math.max(4,12-ext.length))+'…'+ext}
let weatherShownFor=null;
// Keyed by position, not name: the full address arriving later renames the place without fetching the weather again.
const weatherAt=()=>settings.latitude+','+settings.longitude;
async function refreshWeather(){if(weatherBusy)return;weatherBusy=true;const requestedCity=weatherAt();showPlaceName();if(weatherShownFor!==requestedCity)$('condition').textContent='Loading';try{const data=await invoke('weather',{latitude:settings.latitude,longitude:settings.longitude});if(weatherAt()!==requestedCity)return;const c=data.current;if(!c||!Number.isFinite(c.temperature_2m))throw Error('Weather data unavailable');const day=c.is_day===1,sky=skyOf(c.weather_code,day);$('condition').textContent=weatherLabel(c.weather_code,day)[0];document.querySelector('[data-widget=weather]').dataset.sky=sky;drawSky($('wx-art'),sky);$('temp').textContent=Math.round(c.temperature_2m);$('fahrenheit').textContent=`${Math.round(c.temperature_2m*9/5+32)}°F`;$('feels').textContent=`${Math.round(c.apparent_temperature)}°`;$('humidity').textContent=`${c.relative_humidity_2m}%`;$('weather-status').classList.add('online');weatherShownFor=requestedCity;homeCards?.renderPlace();}catch{ $('condition').textContent='Weather unavailable';$('temp').textContent='—';$('fahrenheit').textContent='—°F';$('feels').textContent='—°';$('humidity').textContent='—%';$('weather-status').classList.remove('online');weatherShownFor=null}finally{weatherBusy=false;if(weatherAt()!==requestedCity)refreshWeather()}}
new ResizeObserver(()=>drawSky($('wx-art'),document.querySelector('[data-widget=weather]').dataset.sky)).observe($('wx-art'));
// The full address (jalan, RT/RW, kelurahan/desa, kecamatan, kota/kabupaten, provinsi, kode pos) for the weather position.
const PLACE_LANG=[...new Set([...(navigator.languages||[]),navigator.language,'en'].filter(Boolean))].slice(0,4).join(',');
const placeLines=()=>(settings.place?.parts||[]).filter(p=>p.name);
function showPlaceName(){$('city-name').textContent=settings.city}
let placeRequest=0;
async function lookupPlace(rename){const at=weatherAt(),request=++placeRequest;try{const place=await invoke('place_lookup',{latitude:settings.latitude,longitude:settings.longitude,lang:PLACE_LANG});if(!place||request!==placeRequest||weatherAt()!==at)return;settings.place=place;if(rename&&place.name)settings.city=place.name;saveSettings();showPlaceName();if(currentTab==='location-editor')showCurrentCity(true);homeCards?.renderPlace()}catch{}}
// accuracy: metres when Windows location found the position, null for a searched place or the IP address.
function usePlace(name,latitude,longitude,accuracy,rename){Object.assign(settings,{city:name,latitude,longitude,accuracy:accuracy??null,place:null});saveSettings();return Promise.all([refreshWeather(),lookupPlace(rename)])}


async function pollMedia(){if(mediaBusy||!native||document.body.classList.contains('game'))return;mediaBusy=true;try{const withArt=!media||mediaPoll++%16===0;const requestedAt=performance.now();const fresh=await invoke('media_status',{withArt});if(fresh?.playing)fresh.position=Math.min(fresh.duration,fresh.position+(performance.now()-requestedAt)/2000*(fresh.playback_rate||1));if(fresh)reconcileMedia(media,fresh,performance.now());if(fresh&&media&&fresh.title===media.title&&!withArt)fresh.artwork=media.artwork;const changed=fresh?.title!==media?.title||fresh?.artist!==media?.artist||fresh?.source!==media?.source;if(changed&&fresh&&!withArt)mediaPoll=0;media=fresh;if(media)media.sampled=performance.now();if(changed){romanGeneration++;romanLines=[];romanBusy=false;$('roman-language').hidden=true;lyrics=[];plainLyrics='';lyricsTrack='';renderLyrics();if(fresh)fetchLyrics(fresh)}renderMedia()}catch{$('playback-label').textContent='MEDIA UNAVAILABLE';$('play-pause').disabled=true;$('next').disabled=true;$('previous').disabled=true;$('seek').disabled=true}finally{mediaBusy=false}}
const albumArt=$('album-art');
// Music: the cover, large and blurred, fills the panel behind the player.
const backdrop=$('music-backdrop');
// How bright the glow may be follows the cover: a light cover is dimmed more, so the tabs above it stay readable.
function glowDim(img){try{const c=document.createElement('canvas');c.width=c.height=16;const g=c.getContext('2d',{willReadFrequently:true});g.drawImage(img,0,0,16,16);const d=g.getImageData(0,0,16,16).data;let l=0;for(let i=0;i<d.length;i+=4)l+=(0.299*d[i]+0.587*d[i+1]+0.114*d[i+2])/255;l/=d.length/4;return l>0.3?Math.max(0.16,0.5*0.3/l):0.5}catch{return 0.5}}
albumArt.onload=()=>{albumArt.classList.remove('hidden');backdrop.style.setProperty('--dim',glowDim(albumArt).toFixed(3));backdrop.style.backgroundImage='url("'+albumArt.src+'")';backdrop.classList.add('on')};
albumArt.onerror=()=>{albumArt.classList.add('hidden');backdrop.classList.remove('on')};
function setAlbumArt(src){
 if(albumArt.getAttribute('src')===(src||null))return;
 albumArt.classList.add('hidden');
 if(src)albumArt.src=src;else{albumArt.removeAttribute('src');backdrop.classList.remove('on')}
}
function renderMedia(){const m=media;$('notch').classList.toggle('playing',!!m?.playing);$('track-title').textContent=m?.title||(m?'Untitled':'No music');$('track-artist').textContent=m?.artist|| (m?'Windows media is active':'Play a song.');setSourceLogo(m?.source||'');$('playback-label').textContent=m?(m.playing?'NOW PLAYING':'PAUSED'):'WINDOWS MEDIA';setAlbumArt(m?.artwork);$('play-pause').innerHTML=icon(m?.playing?'pause':'play');;$('play-pause').disabled=!m?.can_play;$('next').disabled=!m?.can_next;$('previous').disabled=!m?.can_previous;$('seek').disabled=!m?.can_seek||!m?.duration;if(document.activeElement!==$('seek')){$('seek').max=m?.duration||100;$('seek').value=m?.position||0}updateSeekFill();$('elapsed').textContent=formatTime(m?.position);$('duration').textContent=formatTime(m?.duration);renderLyrics()}
// ---- Alarm widget: daily alarms that ring through the notch (sync_calendar_notices with kind "alarm"), kept per user.
let alarms=read('sora.alarms',[]);if(!Array.isArray(alarms))alarms=[];
// Alarms saved before the repeat switch existed rang every day.
alarms.forEach(a=>{if(typeof a.repeat!=='boolean')a.repeat=true});
// The rings of one alarm: every day for a week ahead, or the one time it was set for.
function alarmRings(a){return a.repeat===false?(a.ringAt>Date.now()-300000?[a.ringAt]:[]):alarmTimes(a.time)}
// A one-time alarm turns itself off once its time is past (the notch can still ring it up to 5 minutes late).
function settleAlarms(){const now=Date.now();let ch=false;for(const a of alarms)if(a.on&&a.repeat===false&&!(a.ringAt>now-300000)){a.on=false;ch=true}if(ch)saveAlarms()}
function saveAlarms(){persist('sora.alarms',alarms);syncCalendarNotices();renderAlarms()}
function renderAlarms(){
 $('alarm-count').textContent=alarms.length||'';
 if(!alarms.length){const p=document.createElement('p');p.className='td-empty';p.textContent='No alarms';$('alarm-list').replaceChildren(p);return}
 $('alarm-list').replaceChildren(...alarms.map(a=>{const r=document.createElement('div'),t=document.createElement('span'),l=document.createElement('small'),b=document.createElement('b'),sw=document.createElement('button'),x=document.createElement('button');
  r.className='al'+(a.on?' on':'');l.textContent=(a.label||'Alarm')+' · '+(a.repeat===false?'Once':'Every day');b.textContent=a.time;t.append(l,b);
  sw.type='button';sw.className='sw';sw.setAttribute('role','switch');sw.setAttribute('aria-checked',String(!!a.on));sw.setAttribute('aria-label',(a.label||'Alarm')+' at '+a.time);sw.onclick=()=>{a.on=!a.on;if(a.on&&a.repeat===false)a.ringAt=alarmTimes(a.time,Date.now(),1)[0];saveAlarms()};
  x.type='button';x.className='al-remove';x.setAttribute('aria-label','Delete alarm '+a.time);x.innerHTML=icon('close');x.onclick=()=>{alarms=alarms.filter(o=>o!==a);saveAlarms()};
  r.append(t,x,sw);return r}));
}
function setAlarmRepeat(on){alarmRepeat=on;$('alarm-daily').setAttribute('aria-checked',String(on))}
$('alarm-daily').onclick=()=>setAlarmRepeat(!alarmRepeat);
$('alarm-add').onclick=()=>{if(alarms.length>=6){toast('Six alarms at most');return}tab('alarm-editor');alarmRepeat=false;$('alarm-daily').setAttribute('aria-checked','false');$('alarm-label').value='';requestAnimationFrame(()=>{$('wheel-ah').scrollTop=7*WHEEL;$('wheel-am').scrollTop=0;markWheel($('wheel-ah'))})};
$('alarm-back').onclick=()=>tab('home');
document.querySelectorAll('.al-quick [data-at]').forEach(b=>b.onclick=()=>{const [h,m]=b.dataset.at.split(':').map(Number);setWheels(h,m,'wheel-ah','wheel-am')});
$('alarm-save').onclick=()=>{const time=String(wheelValue($('wheel-ah'))).padStart(2,'0')+':'+String(wheelValue($('wheel-am'))).padStart(2,'0');alarms.push({id:crypto.randomUUID(),label:$('alarm-label').value.trim().slice(0,30),time,on:true,repeat:alarmRepeat,ringAt:alarmRepeat?0:alarmTimes(time,Date.now(),1)[0]});alarms.sort((a,b)=>a.time.localeCompare(b.time));saveAlarms();tab('home');toast('Alarm set')};
settleAlarms();setInterval(settleAlarms,30000);
// To-do and Alarm: one row per wheel notch (CSS snap holds the row for drags and keys; it would let one flick skip rows).
for(const id of ['todo-list','alarm-list']){const list=$(id);let at=0;list.addEventListener('wheel',e=>{if(Math.abs(e.deltaY)<2)return;e.preventDefault();const now=performance.now();if(now-at<220)return;at=now;list.scrollBy({top:Math.sign(e.deltaY)*(list.firstElementChild?.getBoundingClientRect().height||0),behavior:'smooth'})},{passive:false})}
// ---- Home widgets (home-widgets.js picks them, home-cards.js fills Screen Time, Water, Qibla and Prayer) ----
let homeWidgets=null,homeCards=null;
for(const[id,action]of[['play-pause','toggle'],['previous','previous'],['next','next']])$(id).onclick=async()=>{try{await invoke('media_action',{action});setTimeout(pollMedia,180)}catch(e){showError(e)}};
function updateSeekFill(){const value=Number($('seek').value),max=Number($('seek').max)||1;$('seek').style.setProperty('--seek-progress',Math.min(100,value/max*100)+'%')}
$('seek').oninput=()=>{updateSeekFill();$('elapsed').textContent=formatTime(Number($('seek').value))};$('seek').onchange=async()=>{try{await invoke('media_action',{action:'seek',position:Number($('seek').value)});$('seek').blur();pollMedia()}catch(e){showError(e)}};
// Providers sometimes mix song metadata into the lyric text ("Title - Artist", "Album: ...", "作词 : ...").
const META_KEY=new RegExp('^(?:'+['title','song','track','artist','singer','vocals?','album','lyrics?','lyricist','words','music','composer','composed','arranger','arranged','producer','produced','written','writer','chorus','keyboards?','guitars?','bass','drums','piano','strings','mix(?:ed)?','master(?:ed|ing)?','record(?:ed|ing)?','engineer(?:ed)?','publisher','label','copyright','source','provider','lrc','by','translat(?:ed|ion)','词','曲','词曲','作词','作詞','作曲','编曲','編曲','演唱','歌手','专辑','專輯','歌曲','歌名','制作人?','製作','混音','母带','录音','监制','企划','出品','发行','作사','작사','작곡','편곡','노래','가수','앨범','제목','프로듀서','믹스','마스터링','녹음'].join('|')+')(?:\\s+[\\p{L}\\p{N}.&/ ]{0,24})?\\s*[:：]','iu');
const normMeta=s=>(s||'').toLowerCase().replace(/[^\p{L}\p{N}]+/gu,'');
function isLyricMeta(text,m,leading){
 const t=text.trim();if(!t)return false;
 if(META_KEY.test(t)||/^[\p{L}\p{N} .&'/()+-]{1,28}\s+by\s*[:：]/iu.test(t))return true;
 if(!leading)return false;
 if(/^(?:written|lyrics?|music|composed|produced|arranged|mixed|mastered)\s+by\b/iu.test(t))return true;
 const n=normMeta(t),title=normMeta(m?.title),artist=normMeta(m?.artist),album=normMeta(m?.album);
 return !!n&&([artist,album,title+artist,artist+title,title+'-'+artist].includes(n)||(title&&artist&&n.includes(title)&&n.includes(artist)&&n.length<=title.length+artist.length+6));
}
function stripLyricMeta(list,m){let leading=true;return list.filter(l=>{const meta=isLyricMeta(l.text,m,leading);if(!meta&&l.text.trim())leading=false;return !meta})}
function stripPlainMeta(text,m){let leading=true;return text.split(/\r?\n/).filter(line=>{const meta=isLyricMeta(line,m,leading)||(leading&&/^(?:paroles|lyrics|letra|testo|songtext)\b.{0,80}$/iu.test(line.trim()));if(!meta&&line.trim())leading=false;return !meta||!line.trim()&&!leading}).join('\n').trim()}
const lyricCache=new Map();let lyricMessage='Automatic lyrics';let lyricFrame='';
async function fetchLyrics(m){const key=m.title+'|'+m.artist;lyricMessage='Finding lyrics…';try{let data=lyricCache.get(key);if(!data){data=await invoke('online_lyrics',{title:m.title,artist:m.artist,album:m.album,duration:m.duration});lyricCache.set(key,data)}if(media?.title!==m.title||media?.artist!==m.artist)return;lyrics=stripLyricMeta(parseLrc(data.syncedLyrics||'',true),m);plainLyrics=lyrics.length?'':stripPlainMeta(data.plainLyrics||'',m);romanLines=[];if(romanWanted)prepareRoman();lyricMessage=data.instrumental?'Instrumental':lyrics.length?'Synced lyrics':'Synced lyrics are not available';}catch{if(media?.title===m.title)lyricMessage='Lyrics are not available'}lyricFrame='';renderLyrics()}
function showInstrumental(container){if(container.classList.contains('instrumental'))return;container.className='instrumental';container.replaceChildren();const dots=document.createElement('div');dots.className='instrumental-dots';dots.setAttribute('aria-label','Instrumental');for(let i=0;i<3;i++){const dot=document.createElement('span');dot.textContent='.';dots.append(dot)}const label=document.createElement('p');label.textContent='Instrumental';container.append(dots,label);lyricNodeKey='';lyricFrame=''}
let lyricNodeKey='';let plainLyrics='';let preparedLyrics=null;
function appendLyricText(row,text){
 const body=document.createElement('span');body.className='lyric-body';
 for(const part of vocalParts(text)){const block=document.createElement('span');block.className=part.backing?'vocal-secondary':'vocal-main';block.textContent=part.text;body.append(block)}row.append(body);
}
const LYRIC_GLIDE_MS=520;
let featherUntil=0;
function renderLyrics(){
 const original=romanWanted&&romanLines.length===lyrics.length?romanLines:lyrics;
 if(!preparedLyrics||preparedLyrics.original!==original||preparedLyrics.track!==media?.title){
  const text=lyrics.map(l=>l.text).join(''),context=text+(media?.title||'')+(media?.artist||'');
  const display=lyricSections(original);if(lyrics[0]?.time>0)display.unshift({time:0,text:'',source:-1});
  display.unshift({text:'',source:-999,padding:true},{text:'',source:-999,padding:true});display.push({text:'',source:-999,padding:true},{text:'',source:-999,padding:true});
  preparedLyrics={original,track:media?.title,display,available:hasCjk(text),lang:/[\u3040-\u30ff]/u.test(context)?'ja':/[\uac00-\ud7af]/u.test(context)?'ko':'zh-CN',key:(media?.title||'')+'|'+(media?.artist||'')+'|'+display.map(l=>l.text).join('')};
 }
 const {display,available,key}=preparedLyrics;const container=$('lyrics-lines');
 $('romanize').classList.toggle('hidden',!available);$('romanize').disabled=romanBusy;$('romanize').setAttribute('aria-pressed',String(romanWanted));
 if(!lyrics.length){if(media&&lyricMessage==='Instrumental'){showInstrumental(container);return}container.classList.remove('synced','instrumental');container.classList.toggle('plain',!!plainLyrics);const message=media?(plainLyrics||lyricMessage):'Lyrics';if(container.textContent!==message){container.replaceChildren();const hint=document.createElement('p');hint.textContent=message;container.append(hint)}lyricNodeKey='';return}
 const position=(media?.position||0)+(media?.playing?Math.max(0,performance.now()-(media.sampled||performance.now()))/1000*(media.playback_rate||1):0);
 const source=lyrics.findLastIndex(l=>l.time<=position);const index=display.findIndex(l=>l.source===source);const long=false;const count=5;
 const frame=index+':'+count+':'+key+':'+media?.can_seek;if(frame===lyricFrame)return;lyricFrame=frame;
 const previous=key===lyricNodeKey?container.querySelector('.lyric-line.current'):null;
 const step=previous?index-Number(previous.dataset.line):0;
 const before=new Map([...container.children].filter(e=>!e.hidden).map(e=>{const s=getComputedStyle(e);return[e.dataset.line,{top:e.getBoundingClientRect().top,size:parseFloat(s.fontSize),opacity:s.opacity,color:s.color}]}));
 for(const row of container.children)row.getAnimations().filter(a=>a instanceof Animation&&!(a instanceof CSSTransition)).forEach(a=>a.cancel());
 container.classList.remove('plain','instrumental');container.classList.add('synced');container.classList.toggle('long-lines',long);
 container.lang=preparedLyrics.lang;
 if(key!==lyricNodeKey){container.replaceChildren();lyricNodeKey=key;display.forEach((line,i)=>{const row=document.createElement('button');row.type='button';row.className='lyric-line'+(line.backing?' backing-vocal':'');row.dataset.line=i;row.dataset.time=line.time??'';if(line.padding){row.classList.add('lyric-spacer');row.setAttribute('aria-hidden','true')}else if(!line.text){row.classList.add('instrumental-cue');for(let d=0;d<3;d++){const dot=document.createElement('span');dot.textContent='.';row.append(dot)}}else appendLyricText(row,line.text);row.setAttribute('aria-label','Seek to '+formatTime(line.time)+': '+line.text);row.onclick=()=>seekToLyric(line.source);container.append(row)})}
 const start=Math.max(0,index-2);const scale=Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1;
 for(const row of container.children){const i=Number(row.dataset.line);row.hidden=i<start||i>=start+count;row.dataset.slot=i-start;row.classList.toggle('current',i===index);row.classList.toggle('edge-line',i===start||i===start+4);row.classList.toggle('cjk',hasCjk(row.textContent));row.disabled=!media?.can_seek||display[i].source<0;}
 fitLyrics(container,index);
 const bounds=container.getBoundingClientRect();
 for(const row of container.children){if(row.hidden)continue;
 const rect=row.getBoundingClientRect(),top=rect.top,old=before.get(row.dataset.line);
 if(!matchMedia('(prefers-reduced-motion: reduce)').matches){
  const now=getComputedStyle(row),size=parseFloat(now.fontSize)||1;
  // For the next or previous cue, let the newly visible line travel through the actual faded edge.
  const entryY=old?0:step===1?Math.max(0,(bounds.bottom-top)/scale):step===-1?Math.min(0,(bounds.top-rect.bottom)/scale):16;
  const from=old?{transform:'translateY('+((old.top-top)/scale)+'px) scale('+(old.size/size).toFixed(3)+')',opacity:old.opacity,color:old.color}:{transform:'translateY('+entryY+'px) scale(.96)',opacity:'0'};
  row.animate([from,{transform:'translateY(0) scale(1)',opacity:now.opacity,color:now.color}],{duration:LYRIC_GLIDE_MS,easing:'cubic-bezier(.22,.8,.25,1)'});
 }}
 updateLyricFeather();
 featherUntil=reducedMotion()?0:performance.now()+LYRIC_GLIDE_MS;
}
// Keep readable type and all five timestamp cues. Wrap naturally; if their
// combined height exceeds the stage, scroll the active cue into the center.
function fitLyrics(container,index){
 if(!container.clientWidth)return;
 container.classList.remove('overflow-cue');
 const rows=[...container.children].filter(r=>!r.hidden);
 const used=rows.reduce((sum,r)=>sum+r.getBoundingClientRect().height,0)/(Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1)+7*Math.max(0,rows.length-1);
 container.classList.toggle('overflow-cue',used>container.clientHeight);
 const active=rows.find(r=>r.classList.contains('current'));
 if(active&&used>container.clientHeight){
  const scale=Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1;
  const top=(active.getBoundingClientRect().top-container.getBoundingClientRect().top)/scale+container.scrollTop;
  container.scrollTop=Math.max(0,top-Math.max(0,(container.clientHeight-active.offsetHeight)/2));
 }else container.scrollTop=0;
}
document.fonts.ready.then(()=>{lyricFrame='';renderLyrics()});
new ResizeObserver(()=>{lyricFrame='';renderLyrics()}).observe($('lyrics-lines'));
async function seekToLyric(index){if(!media?.can_seek||!lyrics[index])return;const session=media;const position=lyrics[index].time;try{await invoke('media_action',{action:'seek',position});if(media!==session)return;media.position=position;media.sampled=performance.now();lyricFrame='';renderMedia();}catch(e){showError(e)}}

function updateLyricFeather(){
 const box=$('lyrics-lines');
 if(!box.classList.contains('synced')||!box.clientHeight)return;
 const active=box.querySelector('.current');
 if(!active)return;
 const stage=box.parentElement,bounds=box.getBoundingClientRect(),cue=active.getBoundingClientRect();
 const scale=bounds.height/box.clientHeight;
 if(!scale)return;
 const cap=box.clientHeight*.38;
 // Leave six logical pixels of clear space around the painted line, even mid-glide or after scrolling.
 const reach=gap=>Math.floor(4*Math.min(cap,Math.max(0,gap/scale-6)))/4;
 const top=reach(cue.top-bounds.top),bottom=reach(bounds.bottom-cue.bottom);
 const changes=[];
 // Defocus the glyphs themselves as well as their backdrop. Use their painted positions,
 // not their slot numbers, so a line comes into focus continuously while it glides.
 for(const row of box.querySelectorAll('.lyric-line:not([hidden]):not(.current):not(.lyric-spacer)')){
  const r=row.getBoundingClientRect(),mid=(r.top+r.bottom)/2;
  const t=Math.min(1,Math.max(0,top?1-(mid-bounds.top)/(scale*top):0,bottom?1-(bounds.bottom-mid)/(scale*bottom):0));
  changes.push([row,'--cue-defocus',(1.9*t*t*(3-2*t)).toFixed(2)+'px']);
 }
 changes.push([stage,'--feather-top',top+'px'],[stage,'--feather-bottom',bottom+'px']);
 for(const [el,prop,value] of changes)if(el.style.getPropertyValue(prop)!==value)el.style.setProperty(prop,value);
}
$('lyrics-lines').addEventListener('scroll',updateLyricFeather,{passive:true});
function lyricAnimationFrame(){
 if(expanded&&currentTab==='music'){
  renderLyrics();
  if(featherUntil){updateLyricFeather();if(performance.now()>=featherUntil)featherUntil=0}
 }
 requestAnimationFrame(lyricAnimationFrame);
}
requestAnimationFrame(lyricAnimationFrame);
setInterval(()=>{if(expanded&&currentTab==='music'&&media?.playing&&document.activeElement!==$('seek')){const position=Math.min(media.duration,media.position+Math.max(0,performance.now()-media.sampled)/1000*(media.playback_rate||1));$('seek').value=position;$('elapsed').textContent=formatTime(position);updateSeekFill()}},80);
async function detectLocation(){try{const loc=await invoke('auto_location');if(!Number.isFinite(loc.latitude)||!Number.isFinite(loc.longitude))throw Error();await usePlace(loc.city||'Your location',loc.latitude,loc.longitude,loc.source==='device'?loc.accuracy:null,true)}catch{$('condition').textContent='Location unavailable';$('city-name').textContent='Check your connection / VPN'}}


let sendHold=0;
const sendView=createSend(invoke,async()=>{activeDialog=true;try{return await invoke("send_pick")}finally{activeDialog=false}},on=>{
 clearTimeout(sendHold);activeDialog=on;
 // Like the Tray: an outside click does not close the panel while Send is in use (re-armed for long transfers),
 // and for a few seconds after, so a click back in Explorer does not throw the panel away. It never closes by itself.
 if(on){clearTimeout(autoClose);const renew=()=>{invoke('tray_hold',{ms:30000}).catch(()=>{});sendHold=setTimeout(renew,25000)};renew()}else invoke('tray_hold',{ms:TRAY_HOLD_MS}).catch(()=>{});
});
const trayMenu=createTrayMenu({
 invoke, refreshShelf, report:showError,
 sendTo:async path=>{await sendView.add([path]);tab('send')},
 hold:on=>{activeDialog=on;if(on){clearTimeout(autoClose);invoke('tray_hold',{ms:12000}).catch(()=>{})}},
 remove:async path=>{files=await invoke('shelf_remove',{path});shelfRevision++;renderFiles()}
});
let shelfRevision=0;
async function refreshShelf(){
 if(!native)return;
 const revision=++shelfRevision;
 try{
  const list=await invoke('shelf_list');
  if(revision!==shelfRevision||JSON.stringify(list)===JSON.stringify(files))return;
  files=list;renderFiles();
 }catch(e){showError(e)}
}
// A drop on the tray: the mascot closes its hands on the file and ducks back down with it.
let caughtTimer=0;function caught(){const n=$('notch');if(!n.classList.contains('dragging'))return;n.classList.add('caught');clearTimeout(caughtTimer);caughtTimer=setTimeout(()=>n.classList.remove('caught'),700)}
function renderFiles(){trayMenu.reconcile(files);const empty=!files.length;$('shelf-empty').classList.toggle('hidden',!empty);$('files').classList.toggle('hidden',empty);$('shelf-clear').classList.toggle('hidden',empty||currentTab!=='shelf');$('file-count').textContent=empty?'A little space for your files.':`${files.length} / 80 file  /  click to open`;$('files').replaceChildren();for(const f of files){const ext=f.name.split('.').pop().toLowerCase();const symbol=f.directory?'folder':['jpg','jpeg','png','webp','gif','svg'].includes(ext)?'image':['html','js','rs','css','json','ts'].includes(ext)?'code':'file';const item=document.createElement('div');item.className='file'+(f.available?'':' unavailable');const open=document.createElement('button');open.className='file-open';const art=document.createElement('span');art.className='file-art'+(ext==='pdf'?' pdf':'');art.innerHTML=icon(symbol);const name=document.createElement('span');name.className='file-name';name.textContent=shortFileName(f.name);const size=document.createElement('span');size.className='file-size';size.textContent=!f.available?'File not found':f.directory?'Folder':fileSize(f.size);open.append(art,name,size);attachFileDrag(open,f);trayMenu.attach(item,open,f);open.onclick=()=>{if(open.dataset.dragged)return;invoke('shelf_open',{path:f.path,reveal:false}).catch(showError)};const remove=document.createElement('button');remove.className='file-remove';remove.innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17"/></svg>';remove.setAttribute('aria-label','Remove from tray');remove.setAttribute('aria-label',`Remove ${f.name}`);remove.onclick=async()=>{try{files=await invoke('shelf_remove',{path:f.path});shelfRevision++;renderFiles()}catch(e){showError(e)}};item.append(open,remove);$('files').append(item);invoke('file_thumbnail',{path:f.path}).then(src=>{if(!art.isConnected)return;const img=document.createElement('img');img.src=src;img.alt='';art.replaceChildren(img);art.classList.add('native-thumbnail')}).catch(()=>{})}}
function attachFileDrag(button,file){
 let start=null,pan=null;
 button.ondragstart=e=>e.preventDefault();
 button.onpointerdown=e=>{if(e.button!==0)return;delete button.dataset.dragged;start={x:e.clientX,y:e.clientY};pan=null;button.setPointerCapture(e.pointerId)};
 const end=()=>{start=null;pan=null;$('files').classList.remove('panning')};
 button.onpointerup=end;button.onpointercancel=end;
 button.onpointermove=async e=>{
  if(!start||!(e.buttons&1))return;
  const dx=e.clientX-start.x,dy=e.clientY-start.y,strip=$('files');
  if(!pan&&Math.hypot(dx,dy)<6)return;
  // Mostly sideways on a crowded tray: slide the tray. Anything else drags the file out, as before.
  if(pan||(Math.abs(dx)>Math.abs(dy)*1.2&&strip.scrollWidth>strip.clientWidth+1)){
   if(!pan){pan={x:start.x,left:strip.scrollLeft};button.dataset.dragged='true';strip.classList.add('panning')}
   strip.scrollLeft=pan.left-(e.clientX-pan.x)/(Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1);return;
  }
  start=null;button.dataset.dragged='true';activeDialog=true;clearTimeout(autoClose);
  try{await invoke('shelf_drag',{path:file.path});await refreshShelf()}catch(error){showError(error)}finally{activeDialog=false;if(!$('notch').matches(':hover'))scheduleClose()}
 };
}
// ---- Tray strip: mouse wheel and hold-drag on the background scroll it sideways; soft edges show there is more ----
function stripFade(){const s=$('files');s.style.setProperty('--fade-l',s.scrollLeft>2?'18px':'0px');s.style.setProperty('--fade-r',s.scrollLeft+s.clientWidth<s.scrollWidth-2?'18px':'0px')}
{
 const strip=$('files');let bg=null;
 strip.addEventListener('scroll',stripFade,{passive:true});
 strip.addEventListener('wheel',e=>{if(strip.scrollWidth<=strip.clientWidth+1||Math.abs(e.deltaX)>Math.abs(e.deltaY))return;e.preventDefault();strip.scrollBy({left:e.deltaY,behavior:'smooth'})},{passive:false});
 strip.addEventListener('pointerdown',e=>{if(e.button!==0||e.target.closest('.file'))return;bg={x:e.clientX,left:strip.scrollLeft};strip.setPointerCapture(e.pointerId);strip.classList.add('panning')});
 strip.addEventListener('pointermove',e=>{if(bg)strip.scrollLeft=bg.left-(e.clientX-bg.x)/(Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1)});
 const done=()=>{bg=null;strip.classList.remove('panning')};strip.addEventListener('pointerup',done);strip.addEventListener('pointercancel',done);
 new ResizeObserver(stripFade).observe(strip);new MutationObserver(()=>requestAnimationFrame(stripFade)).observe(strip,{childList:true});
}
async function pickFiles(){activeDialog=true;try{files=await invoke('shelf_pick');shelfRevision++;renderFiles()}catch(e){showError(e)}finally{activeDialog=false}}
// Clearing asks twice: the first press turns the button into "Confirm?" for three seconds, the second one (on it) clears. Any press elsewhere,
// or the time running out, puts it back. It sits in front of the button's own handler, so the Tray, the Board and the Chat history all use it.
for(const id of ['shelf-clear','board-clear','history-clear']){
 const b=$(id),label=b.getAttribute('aria-label');let timer=0;
 const disarm=()=>{clearTimeout(timer);b.classList.remove('confirming');b.setAttribute('aria-label',label)};
 b.addEventListener('click',e=>{if(b.classList.contains('confirming')){disarm();return}e.stopImmediatePropagation();b.classList.add('confirming');b.setAttribute('aria-label','Confirm: '+label);timer=setTimeout(disarm,3000)},true);
 document.addEventListener('pointerdown',e=>{if(!b.contains(e.target))disarm()},true);
 b.addEventListener('blur',disarm);
}
$('shelf-clear').onclick=async()=>{try{files=await invoke('shelf_remove',{path:null});shelfRevision++;renderFiles();toast('Tray cleared. Original files are untouched.')}catch(e){showError(e)}};

async function applyPlacement(){saveSettings();if(PANEL)return;if(native)await invoke('set_panel',{expanded,scale:settings.scale,top:settings.top,monitor:settings.monitor,home:currentTab==='home'}).catch(showError);else document.documentElement.style.setProperty('--scale',settings.scale)}
function showCurrentCity(keepResults){
 $('loc-city').textContent=settings.city;const lat=Number(settings.latitude),lon=Number(settings.longitude),acc=Number(settings.accuracy);
 // The title is the finest area already; "Kota Bandung" stays under a picked "Bandung" (it is not Kabupaten Bandung).
 const lines=placeLines().filter(p=>!(p.name===settings.city&&p.name===settings.place?.name)&&!['Negara','Country'].includes(p.label));
 const prefix={Kelurahan:'Kel. ',Desa:'Desa ','Kelurahan/Desa':'Kel./Desa ',Kecamatan:'Kec. ',Kota:'Kota ',Kabupaten:'Kab. ',Provinsi:'Prov. '};
 $('loc-detail').textContent=lines.map(p=>(prefix[p.label]||'')+p.name).join(', ');
 $('loc-coords').textContent=Number.isFinite(lat)&&Number.isFinite(lon)?Math.abs(lat).toFixed(4)+'° '+(lat<0?'S':'N')+'  '+Math.abs(lon).toFixed(4)+'° '+(lon<0?'W':'E')+(settings.accuracy!=null&&Number.isFinite(acc)?'  ±'+(acc<1000?Math.round(acc)+' m':(acc/1000).toFixed(1)+' km'):''):'';
 if(!keepResults)$('city-results').replaceChildren();
}
$('loc-auto').onclick=async()=>{$('loc-auto').disabled=true;try{await detectLocation()}finally{$('loc-auto').disabled=false;showCurrentCity()}};
$('city-name').onclick=()=>{cityRequest++;$('city-form').querySelector('button').disabled=false;tab('location-editor');activeDialog=true;$('city-query').value='';$('city-query').placeholder=settings.city;showCurrentCity();$('city-query').focus()};
$('location-done').onclick=()=>{activeDialog=false;tab('home');if(!$('notch').matches(':hover'))scheduleClose()};
let cityRequest=0;
$('city-form').onsubmit=async e=>{e.preventDefault();const query=$('city-query').value.trim();if(query.length<2){fieldNote($('city-query'),query?'Type at least 2 letters':'Please fill out this field');return}const request=++cityRequest;const button=$('city-form').querySelector('button');button.disabled=true;$('city-results').textContent='Finding places…';
 // Two sources at once: the bundled city list (instant, offline) and OpenStreetMap (kelurahan, villages, streets, landmarks).
 let local=[],online=[],pending=2,failed=0;
 const row=place=>{const b=document.createElement('button');b.type='button';b.className='city-result';b.setAttribute('aria-label',place.name+(place.details?', '+place.details:''));const title=document.createElement('strong');title.textContent=place.name;const region=document.createElement('span');region.textContent=place.details||'Location';b.append(title,region);b.onclick=()=>{cityRequest++;usePlace(place.name,place.latitude,place.longitude,null,false);tab('home');toast('Weather location updated')};return b};
 const render=()=>{if(request!==cityRequest)return;const near=(a,b)=>a.name.toLowerCase()===b.name.toLowerCase()&&Math.abs(a.latitude-b.latitude)<.05&&Math.abs(a.longitude-b.longitude)<.05;const extra=online.filter(o=>!local.some(l=>near(o,l)));const rows=[...local.slice(0,5),...extra,...local.slice(5)];if(rows.length)$('city-results').replaceChildren(...rows.map(row));else $('city-results').textContent=pending?'Finding places…':failed===2?'Search unavailable. Check your connection and try again.':'No places found. Try another spelling.'};
 const done=()=>{pending--;render();if(!pending&&request===cityRequest)button.disabled=false};
 const valid=p=>Number.isFinite(p.latitude)&&Number.isFinite(p.longitude);
 invoke('city_search',{query}).then(data=>{const seen=new Set();local=(data?.results||[]).filter(city=>{const key=city.id??(city.latitude+','+city.longitude);if(seen.has(key))return false;seen.add(key);return valid(city)}).map(city=>({name:city.name,latitude:city.latitude,longitude:city.longitude,details:[...new Set([city.admin1,city.country].filter(v=>v&&v!==city.name))].join(', ')||city.country_code||''}))}).catch(()=>failed++).finally(done);
 invoke('place_search',{query,lang:PLACE_LANG}).then(list=>{online=(list||[]).filter(valid).map(p=>({name:p.name,latitude:p.latitude,longitude:p.longitude,details:p.detail||''}))}).catch(()=>failed++).finally(done)};


async function initPanel(){
 const q=new URLSearchParams(location.search),listen=window.__TAURI__.event.listen;
 document.documentElement.style.setProperty('--scale',q.get('scale')||'0.85');
 expanded=true;$('notch').classList.remove('compact');$('expanded').inert=false;$('face').tabIndex=-1;tab(q.get('tab')||'home');
 await listen('shelf-hover',e=>{if(e.payload)tab('shelf');$('notch').classList.toggle('dragging',e.payload)});
 // Files received with LocalSend join the Tray without switching tabs.
 await listen('shelf-refresh',e=>{files=e.payload;renderFiles()});
 await listen('shelf-updated',e=>{dragOffered=false;files=e.payload;tab('shelf');renderFiles();caught();$('notch').classList.remove('dragging');toast('Files added to the tray');armTrayHold()});
 await listen('app-error',e=>showError(e.payload));
 await listen('panel-tab',e=>tab(e.payload));
 await listen('open-tab',e=>tab(e.payload));
 // While a file is dragged over the open panel, resting on the Send or Tray tab switches where it will be dropped.
 let dragTab=null,dragTabTimer=0;
  await listen('drag-position',e=>{
  const [x,y]=e.payload,k=devicePixelRatio||1;
  const name=document.elementFromPoint(x/k,y/k)?.closest('[data-tab]')?.dataset.tab;
  const want=name==='send'||name==='shelf'?name:null;
  if(want===dragTab)return;dragTab=want;clearTimeout(dragTabTimer);
  if(want&&want!==currentTab)dragTabTimer=setTimeout(()=>{if(dragTab===want){tab(want);$('notch').classList.add('dragging')}},220);
 });
 await listen('send-updated',()=>{dragOffered=false;clearTimeout(dragTabTimer);dragTab=null;tab('send');$('notch').classList.remove('dragging');toast('Files added to Send');armTrayHold()});
 // The native notch keeps the timer's deadline (it chimes while this page is closed).
 if(!completed)invoke('timer_deadline',{end:timer.end||null}).catch(()=>{});
 refreshShelf();pollMedia();refreshWeather();if(!settings.place||'full' in settings.place)lookupPlace(false);
 // Loaded (maybe on hover, before the click): the notch shows the window when the panel is nearly open, then the contents fade in.
 // Shown again after sleeping: bring the live data up to date at once (scripts were suspended while hidden).
 // Closing: the native notch cuts the page to its shape; the contents shrink and fade inside it so nothing shows half cut or over the face.
 await listen('panel-hide',()=>{clearTimeout(trayHoldTimer);document.body.classList.add('closing');if(EDITOR_TABS.includes(currentTab))setTimeout(()=>tab('home'),260)});
 await listen('panel-show',()=>{if(PANEL&&native)invoke('panel_view',{name:currentTab}).catch(()=>{});if(native)invoke('take_open_tab').then(n=>n&&tab(n)).catch(()=>{});document.body.classList.remove('closing');document.body.classList.add('ready');renderSystem();refreshWeather();renderTimer();pollMedia();if(currentTab==='shelf')refreshShelf();if(currentTab==='send')sendView.shown();lyricFrame='';renderLyrics()});
 requestAnimationFrame(()=>requestAnimationFrame(()=>invoke('panel_ready').catch(()=>{})));
}
let features=null,labs=null;
async function init(){if(native){features=initFeatures({invoke:(...a)=>invoke(...a),listen:window.__TAURI__.event.listen,$,icon,toast,showError,notch:$('notch'),sendTo:async path=>{await sendView.add([path]);tab('send')}});try{labs=initLabs({invoke:(...a)=>invoke(...a),listen:window.__TAURI__.event.listen,$,toast,showError})}catch(e){console.error(e)}invoke('take_open_tab').then(n=>n&&tab(n)).catch(()=>{})}homeWidgets=initHomeWidgets({icon,toast:m=>toast(m),onChange:()=>{homeCards?.refresh();renderSystem()}});renderAlarms();renderTodo();homeCards=initHomeCards({invoke:(...a)=>invoke(...a),native,settings,icon,tab,read,persist,has:id=>homeWidgets.has(id),visible:()=>currentTab==='home'&&(PANEL||expanded)&&!document.hidden,showError});homeCards.refresh();pruneDone();syncCalendarNotices();renderWeek();renderTimer();if(timer.end)syncWindowsFocus();tab('home');if(native&&PANEL){await initPanel();return}if(native){const listen=window.__TAURI__.event.listen;
 await listen('edge-preference',e=>{edgePreferenceVersion++;setEdgePreference(e.payload===true)});
 const version=edgePreferenceVersion,prefs=await invoke('desktop_preferences').catch(()=>null);
 if(version===edgePreferenceVersion)setEdgePreference(prefs?.preferences?.edge_handle===true);
 await listen('view-scale',e=>{const root=document.documentElement.style,v=String(e.payload);if(root.getPropertyValue('--scale')!==v)root.setProperty('--scale',v)});await listen('panel-request',e=>panel(e.payload));await listen('panel-visibility',async e=>{const {visible,token,edge}=e.payload;if(visible&&document.body.classList.contains('edge')){await peekFromEdge();return}
  let done;
  if(!visible&&edge&&!reducedMotion()){
   // An open panel first shrinks into the mini notch, then the mini notch shrinks into the capsule: one continuous motion, no fade.
   // The edge-mode event the native side sends meanwhile would hide the notch half way, so it is applied afterwards.
   holdEdgeMode=true;
   try{if(expanded)await panel(false);done=await morphToCapsule()}finally{holdEdgeMode=false}
   if(pendingEdgeMode){pendingEdgeMode=false;document.body.classList.add('edge')}
  }else done=await slidePanel(visible);
  if(done&&!visible)await invoke('finish_panel_hide',{token});});await listen('play-intro',()=>playIntro(true));await listen('panel-peek',()=>{if(edgeEl.classList.contains('handoff')&&performance.now()-edgeDoneAt<1500)peekIn(true);else peekAuto()});await listen('game-mode',e=>document.body.classList.toggle('game',!!e.payload));await listen('edge-mode',e=>{if(holdEdgeMode&&e.payload){pendingEdgeMode=true;return}document.body.classList.toggle('edge',!!e.payload)});await listen('edge-hover',e=>{const p=e.payload||{};if(p.active){const breathing=getComputedStyle(edgePill).scale;edgeTrayFast=!!p.tray;edgeEl.classList.add('hot');edgeEl.classList.toggle('plain',edgeTrayFast);edgeStart(p.ms,breathing,!edgeTrayFast)}else if(p.done){edgeTrayFast=!!p.tray;edgeFinish(1,edgeTrayFast)}else{edgeTrayFast=false;edgeEl.classList.remove('hot');edgeRetreat()}});
 await listen('drag-offer',()=>{
  trayMenu.close();dragOffered=true;dragHold++;clearTimeout(trayHoldTimer);
  $('notch').classList.add('dragging');openTray();
 });
 await listen('drag-ended',()=>{
  $('notch').classList.remove('dragging');
  if(!dragOffered)return;
  dragOffered=false;++dragHold;
 });
 await listen('shelf-hover',e=>{
  if(e.payload){trayMenu.close();dragHold++;clearTimeout(trayHoldTimer);openTray()}
  $('notch').classList.toggle('dragging',e.payload||dragOffered);
 });
 // While a file is dragged over the open panel, resting on the Send or Tray tab switches where it will be dropped.
 let dragTab=null,dragTabTimer=0;
 await listen('drag-position',e=>{
  if(!expanded)return;const [x,y]=e.payload,k=devicePixelRatio||1;
  const name=document.elementFromPoint(x/k,y/k)?.closest('[data-tab]')?.dataset.tab;
  const want=name==='send'||name==='shelf'?name:null;
  if(want===dragTab)return;dragTab=want;clearTimeout(dragTabTimer);
  if(want&&want!==currentTab)dragTabTimer=setTimeout(()=>{if(dragTab===want){tab(want);$('notch').classList.add('dragging')}},220);
 });
 await listen('send-updated',()=>{
  dragOffered=false;dragHold++;clearTimeout(dragTabTimer);dragTab=null;if(currentTab!=='send')tab('send');
  $('notch').classList.remove('dragging');toast('Files added to Send');armTrayHold();
 });
 await listen('shelf-updated',e=>{
  dragOffered=false;dragHold++;openTray();
  shelfRevision++;files=e.payload;renderFiles();
  caught();$('notch').classList.remove('dragging');toast('Files added to the tray');armTrayHold();
 });
 await listen('shelf-refresh',e=>{shelfRevision++;files=e.payload;renderFiles()});
 await listen('app-error',e=>showError(e.payload));const intro=!navigator.webdriver;if(intro){$('notch').style.translate=edgeHandleEnabled?'0 0':'0 -110%';$('notch').style.opacity='0'}await invoke('set_panel',{expanded:false,scale:settings.scale,top:settings.top,monitor:settings.monitor,home:currentTab==='home'}).catch(showError);if(intro)await startupReveal();
 // Data (and the DOM updates it causes) waits until the start-up animation is over.
 refreshShelf();pollMedia();refreshWeather();if(!settings.place||'full' in settings.place)lookupPlace(false)}else{await panel(true);$('condition').textContent='Preview desain';$('temp').textContent='—'}}
init().catch(showError);
setInterval(()=>{renderTimer();const today=dateKey(new Date());if(today!==lastDate){lastDate=today;selectedDate=new Date();weekAnchor=new Date();renderWeek()}},250);
let mediaTick=0;setInterval(()=>{if(expanded||PANEL||++mediaTick%3===0)pollMedia()},750);setInterval(()=>refreshWeather(),300000);

let sourceLogoKey=null;const sourceIconCache=new Map();
// A filled, slightly grown silhouette of an icon (holes closed, edge pushed out by the gap), as a PNG data URL used to cut the cover.
// Smallest convex outline around the opaque part of an icon (no holes or pockets that could leave bits of cover), as points.
function convexHull(points){
 const P=points.slice().sort((a,b)=>a[0]-b[0]||a[1]-b[1]),cross=(o,a,b)=>(a[0]-o[0])*(b[1]-o[1])-(a[1]-o[1])*(b[0]-o[0]);
 const half=list=>{const out=[];for(const p of list){while(out.length>=2&&cross(out[out.length-2],out[out.length-1],p)<=0)out.pop();out.push(p)}out.pop();return out};
 return half(P).concat(half(P.slice().reverse()));
}
// The mask the cover is cut with: that outline filled and widened by R on every side with a round join, drawn as a vector so the edge is smooth.
async function iconSilhouette(src){
 const img=new Image();img.src=src;await img.decode();
 const N=280,area=200,R=29,c=document.createElement('canvas');c.width=c.height=N;const x=c.getContext('2d',{willReadFrequently:true});
 const iw=img.naturalWidth||24,ih=img.naturalHeight||24,k=Math.min(area/iw,area/ih),w=iw*k,h=ih*k;
 x.drawImage(img,(N-w)/2,(N-h)/2,w,h);
 const px=x.getImageData(0,0,N,N).data,pts=[];
 for(let y=0;y<N;y++){let lo=-1,hi=-1;for(let i=0;i<N;i++)if(px[(y*N+i)*4+3]>64){if(lo<0)lo=i;hi=i}if(lo>=0)pts.push([lo,y],[hi+1,y],[lo,y+1],[hi+1,y+1])}
 x.clearRect(0,0,N,N);
 const hull=pts.length>=3?convexHull(pts):[[N/2-area/2,N/2-area/2],[N/2+area/2,N/2-area/2],[N/2+area/2,N/2+area/2],[N/2-area/2,N/2+area/2]];
 x.fillStyle=x.strokeStyle='#000';x.lineJoin='round';x.lineWidth=2*R;x.beginPath();hull.forEach((p,i)=>i?x.lineTo(p[0],p[1]):x.moveTo(p[0],p[1]));x.closePath();x.fill();x.stroke();
 return c.toDataURL('image/png');
}
// Prefer the icon Windows itself shows for the app (so it always matches); the bundled logo is only the fallback.
function resolveSourceIcon(source,info){
 if(!sourceIconCache.has(source)){
  if(sourceIconCache.size>=64)sourceIconCache.delete(sourceIconCache.keys().next().value);
  sourceIconCache.set(source,(async()=>{
   let src=null;
   if(native&&source){try{const icon=await Promise.race([invoke('media_source_icon',{source}),wait(700).then(()=>null)]);if(typeof icon==='string'&&icon.startsWith('data:image/png;base64,'))src=icon}catch{}}
   // Without Windows' icon: the app's own logo in colour (logos/color, same optical size as the others), else the mark.
   for(const candidate of [src,info.known?'logos/color/'+info.icon+'.png':'','logos/'+info.icon+'.svg','logos/media.svg']){if(!candidate)continue;try{return{src:candidate,mask:await iconSilhouette(candidate)}}catch{}}
   throw Error('No usable icon');
  })());
 }
 return sourceIconCache.get(source);
}
function setSourceLogo(source){
 if(source===sourceLogoKey)return;sourceLogoKey=source;
 const info=mediaSourceInfo(source),el=$('music-source'),art=el.closest('.artwork');
 el.setAttribute('aria-label',info.label);el.dataset.source=info.icon;
 art.classList.remove('cut');el.classList.remove('ready');
 if(!source){el.replaceChildren();return}
 resolveSourceIcon(source,info).then(r=>{
  if(sourceLogoKey!==source)return;
  const img=document.createElement('img');img.alt=info.label;img.src=r.src;el.replaceChildren(img);
  art.style.setProperty('--cut-url','url("'+r.mask+'")');art.classList.add('cut');el.classList.add('ready');
 }).catch(()=>{});
}

function updateSand(remaining){const fraction=Math.min(1,Math.max(0,remaining/timer.total));const top=$('sand-top');top.setAttribute('y',String(8+20*(1-fraction)));top.setAttribute('height',String(20*fraction));const rise=19*(1-fraction);$('sand-bottom').setAttribute('d',`M6 49H34L30 ${49-rise*.65}L20 ${49-rise}L10 ${49-rise*.65}Z`);$('sand-stream').style.opacity=timer.end&&remaining>0?'1':'0';$('hourglass').classList.toggle('flowing',!!timer.end&&remaining>0);}
function syncPresets(){const m=Number($('timer-min-input').value),s=Number($('timer-sec-input').value);document.querySelectorAll('.timer-presets button').forEach(b=>b.classList.toggle('on',Number(b.dataset.min)===m&&s===0))}
document.querySelectorAll('.timer-presets button').forEach(b=>b.onclick=()=>{$('timer-min-input').value=b.dataset.min;$('timer-sec-input').value=0;$('timer-error').textContent='';syncPresets()});
['timer-min-input','timer-sec-input'].forEach(id=>$(id).addEventListener('input',syncPresets));
$('timer-edit').onclick=()=>{fillFocusMenu();const value=timerRemaining(timer);$('timer-min-input').value=Math.floor(value/60);$('timer-sec-input').value=value%60;$('timer-error').textContent='';syncPresets();$('timer-form').hidden=false;tab('timer-editor');activeDialog=true;$('timer-min-input').focus()};
function closeTimerEditor(){$('timer-form').hidden=true;activeDialog=false;tab('home');if(!$('notch').matches(':hover'))scheduleClose()}
$('timer-cancel').onclick=closeTimerEditor;
// ---- Focus menu: what today and the last seven days added up to.
const fmtFocus=sec=>{const m=Math.round(sec/60);return m>=60?Math.floor(m/60)+' h'+(m%60?' '+m%60+' min':''):m+' min'};
function renderFocusStats(){
 const t=focus.log[dateKey(new Date())]||{sec:0,n:0};$('focus-total').textContent=fmtFocus(t.sec);$('focus-count').textContent=t.n?t.n+(t.n===1?' session':' sessions'):'No sessions yet';
 const days=[...Array(7)].map((_,i)=>{const d=new Date();d.setDate(d.getDate()-(6-i));return {name:d.toLocaleDateString('en-US',{weekday:'short'}),sec:(focus.log[dateKey(d)]||{}).sec||0}});
 const max=Math.max(...days.map(d=>d.sec),1);
 $('focus-week').replaceChildren(...days.map((d,i)=>{const c=document.createElement('div'),bar=document.createElement('i'),n=document.createElement('small');c.className='fw-day'+(i===6?' today':'');bar.style.height=Math.max(3,Math.round(d.sec/max*40))+'px';n.textContent=d.name[0];c.append(bar,n);return c}));
 $('focus-week').setAttribute('aria-label','Focus time, last 7 days: '+days.map(d=>d.name+' '+fmtFocus(d.sec)).join(', '));
}
function fillFocusMenu(){renderFocusStats()}
$('timer-form').onsubmit=e=>{e.preventDefault();const m=Number($('timer-min-input').value),sec=Number($('timer-sec-input').value);if(!Number.isInteger(m)||!Number.isInteger(sec)||m<0||m>999||sec<0||sec>59||m*60+sec<1){$('timer-error').textContent='Enter a duration of at least 1 second.';return}timer={total:m*60+sec,remaining:m*60+sec,end:null};completed=false;saveTimer();renderTimer();closeTimerEditor()};

async function prepareRoman(){if(!lyrics.length||!hasCjk(lyrics.map(l=>l.text).join('')))return;const generation=++romanGeneration;romanBusy=true;renderLyrics();try{const result=await romanizeLines(lyrics,(media?.title||'')+' '+(media?.artist||''),$('roman-language').value);if(generation!==romanGeneration)return;romanLines=result.lines;}catch(e){if(generation===romanGeneration){romanWanted=false;persist('nook.romanize',false);showError(e)}}finally{if(generation===romanGeneration){romanBusy=false;lyricFrame='';renderLyrics()}}}
$('romanize').onclick=()=>{romanWanted=!romanWanted;persist('nook.romanize',romanWanted);lyricFrame='';if(romanWanted&&!romanLines.length)prepareRoman();renderLyrics()};
$('romanize').oncontextmenu=e=>{e.preventDefault();$('roman-language').hidden=!$('roman-language').hidden};
$('roman-language').onchange=()=>{romanLines=[];romanWanted=true;persist('nook.romanize',true);$('roman-language').hidden=true;prepareRoman()};

document.querySelector('.music-controls').append($('romanize'));



document.addEventListener('contextmenu',e=>e.preventDefault());
// Background update check, at most once a day: it only marks the gear. The Settings window shows the details.
$('settings-button').classList.toggle('has-update',!!read('sora.updateAvailable',false));
if(native)setTimeout(()=>{if(Date.now()-Number(read('sora.lastUpdateCheck',0))>864e5)invoke('check_update').then(info=>{persist('sora.lastUpdateCheck',Date.now());persist('sora.updateAvailable',!!info?.available);$('settings-button').classList.toggle('has-update',!!info?.available)}).catch(()=>{})},12000);
