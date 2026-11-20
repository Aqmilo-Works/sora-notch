import {glyph,paintIcons} from './tool-icons.js';
const $=id=>document.getElementById(id);
export function bytes(n){
  if(n<1024)return n+' B';
  for(const [unit,scale] of [['GB',1073741824],['MB',1048576],['KB',1024]])if(n>=scale)return +(n/scale).toFixed(1)+' '+unit;
}
// Send keeps its own file list (separate from the Tray): files come from Add files, a drop on this view, or the Tray's "Send to…".
// hold(true) keeps the panel open (outside clicks ignored) while Send is in use; hold(false) hands it back like the Tray does.
export function createSend(invoke,pick,hold=()=>{}){
  const root=$('send'),sheet=$('send-sheet'),chosen=new Set(),peers=new Map(),art=new Map();
  // LocalSend (any phone with the app) or Quick Share (Android, built in); remembered between sessions.
  let method=(()=>{try{return ['quickshare','browser'].find(m=>m===localStorage.getItem('send-method'))||'localsend'}catch{return 'localsend'}})();
  // QR Code: this PC sends (needs files first) or receives from a phone (the arrow works at once); remembered.
  const role='receiver';
  const quick=p=>p?.protocol==='quickshare';
  let sharing=false;
  let files=[],device='',view='',sending=false,starting=false,searching=false,initialized=false,picking=false,lastArt='',returnFocus=null;
  paintIcons(root);
  let doneTimer=0;
  // An error raised while a sheet is open belongs to that sheet: it goes away when the sheet closes (or Send is entered again).
  let sheetNote=false;
  // An error stays readable for a while, then the header shows the address again (no note left behind).
  let noteTimer=0;
  const note=(s,error=false)=>{const el=$('send-note');el.textContent=s;el.title=s;el.classList.toggle('error',error);root.classList.toggle('noted',error||sending);sheetNote=error&&!!view&&!sending;
    clearTimeout(noteTimer);if(error)noteTimer=setTimeout(()=>{if(!sending&&el.classList.contains('error')){el.classList.remove('error');root.classList.remove('noted');sheetNote=false}},8000)};
  function summary(){
    const list=files.filter(f=>chosen.has(f.path)),peer=peers.get(device),n=list.length;
    const browser=method==='browser',rx=browser&&role==='receiver'&&n===0;
    $('send-name').textContent=n===0?(rx?'Receive from phone':'Choose files'):n===1?list[0].name:n+' files';
    $('send-name').title=list.map(f=>f.name).join('\n');
    $('send-size').textContent=n?bytes(list.reduce((v,f)=>v+f.size,0))+' total':rx?'Or add files to share too':'Add files or drop them here';
    $('send-alias').textContent=browser?'Any phone or computer':peer?peer.alias:'Choose device';
    $('send-address').textContent=browser?(sharing?'Sharing now':'Scan to connect'):peer?(peer.model||peer.address.split(':')[0]):'Same Wi-Fi';
    $('send-via').textContent=browser?'QR Code':(peer?quick(peer):method==='quickshare')?'Quick Share':'LocalSend';
    const kind=peer?.kind==='tablet'?'tablet':peer?.kind==='laptop'?'laptop':'phone',icon=root.querySelector('#send-peer .peer-icon');
    if(icon.dataset.kind!==kind){icon.dataset.kind=kind;icon.replaceChildren(glyph(kind))}
    $('send-go').setAttribute('aria-label',rx?'Receive files':n>1?'Send '+n+' files':'Send file');
    root.classList.toggle('ready',(browser?(role==='receiver'||!!n||sharing):(!!n&&!!peer))&&!sending);root.classList.toggle('sending',sending);
    $('send-go').disabled=sending||starting||fetching()||(browser?!(role==='receiver'||n||sharing):(!n||!peer));
    $('send-source').disabled=$('send-peer').disabled=sending;$('send-pick').disabled=sending||picking;{const c=document.getElementById('send-pick-center');if(c)c.disabled=sending||picking}
    $('send-count').textContent=n+' selected';$('send-remove').disabled=sending||!n;$('send-all').textContent=n===files.length&&n?'Clear selection':'Select all';
    const cover=list.slice(0,3);const key=JSON.stringify(cover.map(f=>[f.path,f.size]));
    if(key!==lastArt && root.classList.contains('active')){
      lastArt=key;const stack=$('send-art');stack.replaceChildren();stack.dataset.count=cover.length;
      if(!cover.length)stack.append(glyph('file'));
      // The first file sits on top of the stack (the last child is drawn last).
      for(const f of [...cover].reverse()){const i=document.createElement('i');i.append(glyph('file'));stack.append(i);thumb(f.path,i)}
    }
  }
  const fetching=()=>searching||picking;
  function thumb(path,into){
    if(!art.has(path))art.set(path,invoke('file_thumbnail',{path}).catch(()=>null));
    art.get(path).then(src=>{if(src&&into.isConnected){const img=new Image();img.alt='';img.src=src;into.replaceChildren(img)}});
  }
  const empty=text=>{const p=document.createElement('p');p.className='send-empty';p.textContent=text;return p};
  // Files as a strip of thumbnail cards (scrolls sideways, the mouse wheel too); the check badge on each card selects it.
  function fileRows(){
    const list=$('send-list');list.replaceChildren();
    // Nothing yet: the Add files button moves from the header to the middle of the sheet.
    $('send-pick').hidden=!files.length;
    if(!files.length){
      // The whole empty sheet is one drop area that also opens the file picker on a click.
      const box=document.createElement('button');box.type='button';box.id='send-pick-center';box.className='send-empty big drop-area';
      box.innerHTML='<strong>Drop files here or <u>browse</u></strong><small>Any type of file</small>';box.disabled=sending||picking;box.onclick=()=>$('send-pick').onclick();
      list.append(box);return
    }
    files.forEach((f,i)=>{const row=document.createElement('label'),check=document.createElement('input'),name=document.createElement('span'),size=document.createElement('small');
      row.className='batch-row file-card';row.style.setProperty('--i',Math.min(i,14));check.type='checkbox';check.checked=chosen.has(f.path);check.value=f.path;
      check.setAttribute('aria-label','Send '+f.name);
      const pic=document.createElement('i');pic.className='row-thumb';pic.append(glyph('file'));thumb(f.path,pic);
      name.textContent=f.name;name.title=f.path;size.textContent=bytes(f.size);
      check.onchange=()=>{initialized=true;if(check.checked)chosen.add(f.path);else chosen.delete(f.path);summary()};row.append(check,pic,name,size);list.append(row)});
  }
  $('send-list').addEventListener('wheel',e=>{const l=$('send-list');if(view!=='files'||!e.deltaY||l.scrollWidth<=l.clientWidth)return;e.preventDefault();l.scrollLeft+=e.deltaY},{passive:false});
  // Nearby devices as a radar: Sora's face in the middle, rings that pulse while scanning, each device a bubble that keeps its
  // place (slot from a hash of its id) and pops in when first seen. The eyes follow the bubble under the pointer.
  // Slots spread sideways (the radar is wide and short): x as a share of the half width, y in px. The first six never overlap;
  // more devices go in between, partly overlapping, and the one under the pointer comes to the front with its full name.
  const SLOTS=[[-.92,-8],[.92,-8],[-.63,3],[.63,3],[-.36,-12],[.36,-12]];
  const EXTRA=[[-.77,12],[.77,12],[-.47,-16],[.47,-16],[-1,10],[1,10]];
  const placed=new Set();
  const slotOf=(id,taken,extra)=>{let h=0;for(const c of id)h=(h*31+c.charCodeAt(0))>>>0;for(let k=0;k<SLOTS.length;k++){const i=(h+k)%SLOTS.length;if(!taken.has(i)){taken.add(i);return [...SLOTS[i],false]}}
    const n=extra.n++,[x,y]=EXTRA[n%EXTRA.length];return [x,y+(Math.floor(n/EXTRA.length)%2?-6:0),true]};
  // QR Code: nothing is searched for here, so no radar: a phone scanning the code on this screen. Only drawn while this view is open (the
  // animation lives in the picture and stops with it), small, and nothing else moves in it.
  const QR_ART='<div class="qr-box" role="img" aria-label="Scan this code with your phone"><svg viewBox="96 54 490 212" fill="none" aria-hidden="true"><line x1="100" y1="259" x2="582" y2="259" stroke="#6e6e75" stroke-width="2" stroke-opacity="0.55" stroke-linecap="round"/><circle cx="493" cy="133" r="76" fill="#4f9dff" opacity="0.06"/><path d="M220 228 H263 L269 246 H214 Z" fill="#18181b" stroke="#6e6e75" stroke-width="1.5" stroke-linejoin="round"/><rect x="193" y="246" width="97" height="12" rx="6" fill="#1c1c1f" stroke="#8e8e93" stroke-width="1.5"/><rect x="113" y="68" width="258" height="161" rx="17" fill="#1c1c1f" stroke="#8e8e93" stroke-width="2.2"/><rect x="124" y="80" width="236" height="134" rx="9" fill="#060607"/><g transform="translate(207 114)"><rect width="70" height="70" rx="9" fill="#f2f2f4"/><g fill="#18181b"><rect x="9.7" y="10.4" width="21.7" height="21.7" rx="3"/><rect x="38.8" y="10.4" width="21.7" height="21.7" rx="3"/><rect x="9.7" y="39.7" width="21.7" height="21.7" rx="3"/></g><g fill="#f2f2f4"><rect x="14.9" y="15.6" width="11.3" height="11.3"/><rect x="44" y="15.6" width="11.3" height="11.3"/><rect x="14.9" y="44.9" width="11.3" height="11.3"/></g><g fill="#18181b"><rect x="38.8" y="39.7" width="6.6" height="6.6" rx="1.2"/><rect x="53.9" y="39.7" width="6.6" height="6.6" rx="1.2"/><rect x="46.5" y="47.4" width="6.6" height="6.6" rx="1.2"/><rect x="38.8" y="54.6" width="6.6" height="6.6" rx="1.2"/></g></g></svg><svg viewBox="96 54 490 212" fill="none" aria-hidden="true" class="qr-cone"><defs><linearGradient id="qrFade" gradientUnits="userSpaceOnUse" x1="271" y1="0" x2="452" y2="0"><stop offset="0" stop-color="#4f9dff" stop-opacity="0"/><stop offset="0.35" stop-color="#4f9dff" stop-opacity="0.12"/><stop offset="1" stop-color="#4f9dff" stop-opacity="0.5"/></linearGradient></defs><polygon points="271,106 452,92 452,108 271,194" fill="url(#qrFade)"/></svg><svg viewBox="96 54 490 212" fill="none" aria-hidden="true"><g transform="rotate(-7 500 153)"><rect x="455" y="69" width="90" height="166" rx="17" fill="#1c1c1f" stroke="#8e8e93" stroke-width="2.1"/><line x1="489" y1="85" x2="511" y2="85" stroke="#8e8e93" stroke-width="3" stroke-linecap="round"/><path d="M545 117 V145" stroke="#6e6e75" stroke-width="2.5" stroke-linecap="round"/><rect x="462" y="93" width="76" height="125" rx="8" fill="#18181b"/><g transform="translate(477 119) scale(0.6857)"><rect width="70" height="70" rx="9" fill="#f2f2f4"/><g fill="#18181b"><rect x="9.7" y="10.4" width="21.7" height="21.7" rx="3"/><rect x="38.8" y="10.4" width="21.7" height="21.7" rx="3"/><rect x="9.7" y="39.7" width="21.7" height="21.7" rx="3"/></g><g fill="#f2f2f4"><rect x="14.9" y="15.6" width="11.3" height="11.3"/><rect x="44" y="15.6" width="11.3" height="11.3"/><rect x="14.9" y="44.9" width="11.3" height="11.3"/></g><g fill="#18181b"><rect x="38.8" y="39.7" width="6.6" height="6.6" rx="1.2"/><rect x="53.9" y="39.7" width="6.6" height="6.6" rx="1.2"/><rect x="46.5" y="47.4" width="6.6" height="6.6" rx="1.2"/><rect x="38.8" y="54.6" width="6.6" height="6.6" rx="1.2"/></g></g><g stroke="#4f9dff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M479 111 H474 Q469 111 469 116 V124"/><path d="M523 111 H528 Q533 111 533 116 V124"/><path d="M469 162 V169 Q469 174 474 174 H479"/><path d="M523 174 H528 Q533 174 533 169 V162"/></g></g></svg><svg viewBox="96 54 490 212" fill="none" aria-hidden="true" class="qr-scan"><line x1="469" y1="144" x2="532" y2="144" stroke="#4f9dff" stroke-width="3.5" stroke-linecap="round"/></svg></div>';
  function deviceRows(){
    const list=$('send-list');
    const browser=method==='browser';
    if(browser){list.replaceChildren();const art=document.createElement('div');art.className='qr-art';art.innerHTML=QR_ART;list.append(art);return}
    const shown=[...peers.values()].filter(p=>quick(p)===(method==='quickshare'));
    // The radar (its rings and Sora's face) is made once and kept; a redraw (Scan, a result) only redraws the bubbles,
    // so the rings never restart or blink. Their delays come from a shared clock, in case the radar is made again.
    let radar=list.querySelector(':scope>.radar');
    if(radar)radar.querySelectorAll('.bubble').forEach(b=>b.remove());
    else{
      list.replaceChildren();radar=document.createElement('div');
      const t=(performance.now()/1000)%2.4;
      [0,.8,1.6].forEach((d,k)=>{const e=document.createElement('span');e.className='ring p'+(k+1);e.style.animationDelay=(-((t-d+2.4)%2.4)).toFixed(3)+'s';radar.append(e)});
      const face=document.createElement('div');face.className='radar-me';face.innerHTML='<i></i><i></i>';radar.append(face);
      list.append(radar);
    }
    radar.className='radar'+(searching?' scanning':'');
    const me=radar.querySelector('.radar-me');me.classList.remove('blink');
    if(browser)return;
    const w=radar.clientWidth||560,rx=w/2-34;
    const taken=new Set(),extra={n:0};let fresh=0;
    for(const p of shown){
      const [fx,y,squeezed]=slotOf(p.id,taken,extra),x=fx*rx;
      const b=document.createElement('button');b.className='device-row bubble'+(p.id===device?' picked':'')+(squeezed?' extra':'');b.style.cssText='--x:'+x.toFixed(1)+'px;--y:'+y.toFixed(1)+'px';
      if(!placed.has(p.id)){placed.add(p.id);b.classList.add('new');b.style.setProperty('--d',(fresh++*110)+'ms')}
      const ico=document.createElement('i');ico.className='dev-ico';ico.append(glyph(p.kind==='tablet'?'tablet':p.kind==='laptop'?'laptop':'phone'));
      const name=document.createElement('span');name.className='bubble-name';name.textContent=p.alias;
      const via=(p.model?p.model+' · ':'')+(quick(p)?'Quick Share':'LocalSend')+' · '+p.address.split(':')[0];b.setAttribute('aria-label',p.alias+' · '+via);
      // Hover card beside the bubble (towards the middle, so it stays inside the radar).
      const card=document.createElement('span'),title=document.createElement('strong'),sub=document.createElement('small');
      card.className='bubble-card '+(x<0?'right':'left');title.textContent=p.alias;sub.textContent=via;card.append(title,sub);
      b.append(ico,name,card);
      b.onpointerenter=()=>{const d=Math.hypot(x,y)||1;me.style.setProperty('--gx',(x/d*3.6).toFixed(1)+'px');me.style.setProperty('--gy',(y/d*3).toFixed(1)+'px')};
      b.onpointerleave=()=>{me.style.removeProperty('--gx');me.style.removeProperty('--gy')};
      // Picked: it lights up, then the sheet closes.
      b.onclick=()=>{device=p.id;$('send-pin').hidden=true;$('send-pin').value='';note('Ready to send');radar.querySelectorAll('.bubble').forEach(x=>x.classList.toggle('picked',x===b));setTimeout(()=>{if(view==='devices'&&device===p.id)closeSheet()},420)};
      radar.append(b);
    }
    if(fresh){void me.offsetWidth;me.classList.add('blink')}
  }
  // Enter IP takes the whole sheet (the radar is hidden), so Sora's face never moves under it.
  function setIp(on){sheet.classList.toggle('ip',on);$('send-ip-form').hidden=!on;$('send-manual').textContent=on?'Devices':'Enter IP';$('send-ip-error').textContent='';if(on)$('send-ip').focus()}
  // The Files and Nearby devices sheets give the panel more height (the picture and the file list need room): the notch grows by SHEET_EXTRA px while
  // one is open (the panel_view / panel_tall pair the Board preview uses) and goes back when it closes.
  const SHEET_NORMAL=234,SHEET_EXTRA=60;let isTall=false;
  function tall(on){
    const n=$('notch');if(!n||on===isTall)return;isTall=on;
    if(on){n.style.setProperty('--sh-h',(SHEET_NORMAL+SHEET_EXTRA)+'px');n.dataset.view='send-sheet';invoke('panel_view',{name:'send-sheet'}).then(()=>invoke('panel_tall',{px:SHEET_EXTRA})).catch(()=>{})}
    else{n.style.removeProperty('--sh-h');n.dataset.view='send';invoke('panel_view',{name:'send'}).catch(()=>{})}
  }
  function closeSheet(){tall(false);$('send-list').querySelector('.qr-art')?.remove();view='';setIp(false);sheet.hidden=true;if(sheetNote&&!sending)note('');$('share-disconnect').classList.add('hidden');root.classList.remove('sharing-open');sheet.classList.remove('sharing');if(!sending)hold(false);summary();returnFocus?.focus({preventScroll:true})}
  function openSheet(which){
    if(sending)return;if(!view)returnFocus=document.activeElement;view=which;setIp(false);sheet.hidden=false;hold(true);
    $('send-sheet-title').textContent=which==='files'?'Files to send':which==='share'?'Scan with a phone':'Nearby devices';
    $('send-share').hidden=which!=='share';$('share-disconnect').classList.toggle('hidden',which!=='share');root.classList.toggle('sharing-open',which==='share');sheet.classList.toggle('sharing',which==='share');$('send-list').hidden=which==='share';
    $('send-file-actions').hidden=which!=='files';$('send-device-actions').hidden=which==='files'||which==='share';
    $('send-list').classList.toggle('devices',which==='devices');$('send-list').classList.toggle('files',which==='files');which==='files'?fileRows():which==='share'?0:deviceRows();summary();$('send-done').focus();tall(which!=='share');
    // Nearby devices look around by themselves (LocalSend and Quick Share); the Scan button is for looking again.
    if(which==='devices')autoScan();
  }
  let lastScan={method:'',at:0};
  // The look that happens by itself: skipped when the same way was searched a moment ago.
  const autoScan=()=>{if(method==='browser'||(lastScan.method===method&&Date.now()-lastScan.at<15000))return;discover('')};
  async function discover(ip){
    if(fetching())return;const asked0=method;if(!ip)lastScan={method:asked0,at:Date.now()};searching=true;$('send-scan').disabled=$('send-connect').disabled=true;if(!ip)$('send-scan').textContent='Scanning…';$('send-ip-error').textContent='';if(!ip&&view==='devices')deviceRows();
    try{
      if(ip){const p=await invoke('send_lookup',{address:ip});peers.set(p.id,p);device=p.id;$('send-pin').value='';$('send-pin').hidden=true;if(view==='devices')closeSheet();note('Ready to send')}
      else{const asked=method,found=await invoke('send_scan',{method:asked});for(const [id,p] of peers)if(quick(p)===(asked==='quickshare'))peers.delete(id);found.forEach(p=>peers.set(p.id,p));if(!peers.has(device))device=found.length===1?found[0].id:'';note(found.length?'Choose a nearby device':asked==='quickshare'?'No Android found':'No devices found')}
    }catch(e){if(ip)$('send-ip-error').textContent=String(e);else{note(String(e),true);if(view==='devices')$('send-list').replaceChildren(empty(String(e)))}return}
    finally{searching=false;$('send-scan').disabled=$('send-connect').disabled=false;$('send-scan').textContent='Scan';summary()}
    if(view==='devices'&&!ip)deviceRows();
    // The way was switched while it searched: search again for the new one.
    if(!ip&&view==='devices'&&method!==asked0)autoScan()
  }
  function setMethod(m){
    method=m;try{localStorage.setItem('send-method',m)}catch{}
    $('method-name').textContent={localsend:'LocalSend',quickshare:'Quick Share',browser:'QR Code'}[m];
    for(const b of $('method-menu').children)b.setAttribute('aria-selected',String(b.dataset.method===m));closeMethodMenu();
    $('send-manual').hidden=m!=='localsend';$('send-scan').hidden=m==='browser';setIp(false);
    if(device&&quick(peers.get(device))!==(m==='quickshare'))device='';
    $('send-pin').hidden=true;if(view==='devices'){deviceRows();autoScan()}summary();prepare()
  }
  // QR Code picked: look up the trusted name now, so the arrow shows the QR at once (see share_cert.rs).
  function prepare(){if(method==='browser'&&!sharing)invoke('web_share_prepare').catch(()=>{})}
  function closeMethodMenu(){$('method-menu').hidden=true;$('method-button').setAttribute('aria-expanded','false')}
  $('method-button').onclick=()=>{const open=$('method-menu').hidden&&!sending;$('method-menu').hidden=!open;$('method-button').setAttribute('aria-expanded',String(open))};
  $('method-menu').onclick=e=>{const b=e.target.closest('button[data-method]');if(b&&!sending&&b.dataset.method!==method)setMethod(b.dataset.method);closeMethodMenu()};
  document.addEventListener('pointerdown',e=>{if(!$('method-menu').hidden&&!e.target.closest('#send-methods'))closeMethodMenu()});
  setMethod(method);
  // Quick Share: the phone shows a 4-digit code on its accept screen; Sora shows the same one.
  window.__TAURI__?.event.listen('send-code',({payload})=>{if(sending)note('Code '+payload)});
  $('send-source').onclick=()=>openSheet('files');$('send-peer').onclick=()=>openSheet('devices');
  $('send-done').onclick=closeSheet;$('send-scan').onclick=()=>{deviceRows();discover('')};
  $('send-manual').onclick=()=>{const on=!sheet.classList.contains('ip');setIp(on);if(!on&&view==='devices')deviceRows()};
  $('send-ip-form').onsubmit=e=>{e.preventDefault();if($('send-ip').value.trim())discover($('send-ip').value.trim())};
  $('send-pick').onclick=async()=>{if(fetching())return;picking=true;summary();try{const paths=await pick();await refresh();if(paths?.length){initialized=true;for(const p of paths)chosen.add(p);}if(view==='files')fileRows()}catch(e){note(String(e),true)}finally{picking=false;summary()}};
  // Takes the selected files out of Send (the files themselves are untouched).
  $('send-remove').onclick=async()=>{const paths=[...chosen];if(!paths.length||sending)return;try{setFiles(await invoke('send_remove',{paths}))}catch(e){note(String(e),true)}if(view==='files')fileRows()};
  $('send-all').onclick=()=>{initialized=true;if(files.length===chosen.size)chosen.clear();else files.forEach(f=>chosen.add(f.path));fileRows();summary()};
  document.addEventListener('keydown',e=>{if(view&&root.classList.contains('active')&&e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();closeSheet()}},true);
  // The ring around the arrow follows the transfer; without a total yet (the phone has not accepted) it spins.
  function progress(sent,total){const el=$('send-meter');el.hidden=false;if(total){el.max=total;el.value=Math.min(sent,total);root.style.setProperty('--pct',Math.min(100,sent/total*100).toFixed(1))}else{el.removeAttribute('value');root.style.removeProperty('--pct')}}
  window.__TAURI__?.event.listen('send-progress',({payload:p})=>{if(sending){note((p.files>1?p.index+'/'+p.files+' · ':'')+(p.total?Math.floor(p.sent/p.total*100):0)+'%');progress(p.sent,p.total)}});
  // QR Code: a page on this PC (HTTPS, QR + 6-digit code), see src/web_share.rs. A phone gets in with the code and may offer files
  // back (the PC accepts or rejects). Every use of the link or the code replaces both with new ones.
  const QUIET=4;
  function drawQr(width,modules){
    const c=$('send-qr'),total=width+QUIET*2,px=Math.max(1,Math.floor(140*(window.devicePixelRatio||1)/total));
    c.width=c.height=px*total;const g=c.getContext('2d');g.fillStyle='#fff';g.fillRect(0,0,c.width,c.height);g.fillStyle='#000';
    for(let y=0;y<width;y++)for(let x=0;x<width;x++)if(modules[y*width+x]==='1')g.fillRect((x+QUIET)*px,(y+QUIET)*px,px,px);
    c.dataset.px=px;
  }
  const IDLE='Waiting for a phone…';
  // No trusted name (router blocks it, or the service is down): the phone's browser warns once.
  let idleText=IDLE;
  const UNTRUSTED='Waiting for a phone… This router blocks the secure link, so the phone may warn once.';
  let lastOffer=null,receiving=false,flashTimer=0;
  let shownCode='';
  const paintCode=()=>{$('send-code').textContent=shownCode||'------'};
  const setState=name=>{$('send-share').dataset.state=name};
  const flash=text=>{const el=$('send-share-note');el.textContent=text;clearTimeout(flashTimer);flashTimer=setTimeout(()=>{el.textContent=idleText},6000)};
  const plural=(n,w)=>n+' '+w+(n===1?'':'s');
  function renderDevices(list){
    const box=$('share-devices');
    box.replaceChildren(...list.map(n=>{const p=document.createElement('span'),dot=document.createElement('i'),t=document.createElement('b');p.className='share-device';p.title=n;t.textContent=n;p.innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3.4 3 14.6 0 18M12 3c-3 3.4-3 14.6 0 18"/></svg>';p.append(t);return p}));
    if(!list.length){const e=document.createElement('small');e.className='share-none';e.textContent='Nobody yet';box.append(e)}
    $('share-reset').hidden=!list.length;
  }
  // Shows a link: the QR, the code and the countdown bar; a replaced link fades the old QR out first.
  function showShare(r,swap){
    const paint=()=>{
      drawQr(r.width,r.modules);shownCode=r.code.slice(0,3)+' '+r.code.slice(3);paintCode();
      const c=$('send-clock');c.style.setProperty('--life',r.seconds+'s');c.style.animation='none';void c.offsetWidth;c.style.animation='';
      $('send-qr').classList.remove('swap');
    };
    if(!swap)return paint();
    $('send-qr').classList.add('swap');setTimeout(paint,260);
  }
  // A phone offered files: the question sits under the code until it is answered.
  function showOffer(o){
    lastOffer=o;receiving=false;
    $('offer-title').textContent=o.device||'Phone';
    $('offer-detail').textContent=plural(o.count,'file')+' · '+bytes(o.total);
    setState('offer');if(view!=='share')openSheet('share');
  }
  function showReceiving(done,total){
    receiving=true;setState('receive');
    const pct=total?Math.min(100,Math.floor(done/total*100)):0,n=lastOffer?.count||1;
    $('recv-label').textContent=pct>=100?'Received':'Receiving '+(n===1?'file':plural(n,'file'))+'…';
    $('recv-pct').textContent=pct+'%';$('recv-bar').style.width=pct+'%';
  }
  // The panel page may have been closed while the phone did something: ask what is being shared now.
  async function syncShare(){
    let st=null;try{st=await invoke('web_share_state')}catch{}
    if(!st){sharing=false;if(view==='share')closeSheet();summary();return}
    // Always the link that is valid now: events may have been missed while the panel slept.
    sharing=true;showShare(st.info,false);
    renderDevices(st.devices||[]);
    if(st.offer)showOffer(st.offer);else if(!receiving&&$('send-share').dataset.state==='offer')setState('idle');summary();
    return st;
  }
  async function startShare(){
    const paths=files.filter(f=>chosen.has(f.path)).map(f=>f.path);if(starting||(sharing&&view==='share'))return;
    // Reopen only a link that is still alive; one that ended while the panel slept is replaced.
    if(sharing&&await syncShare()){openSheet('share');return}
    starting=true;summary();
    try{
      note('Preparing a secure link…');
      const r=await invoke('web_share_start',{paths});
      showShare(r,false);renderDevices([]);setState('idle');idleText=r.trusted===false?UNTRUSTED:IDLE;$('send-share-note').textContent=idleText;sharing=true;openSheet('share');note('Sharing on Wi-Fi')
    }catch(e){note(String(e),true)}
    finally{starting=false}
    summary()
  }
  // Disconnect is immediate: the server stops and the link no longer opens.
  $('share-disconnect').onclick=async()=>{await invoke('web_share_stop').catch(()=>{});sharing=false;receiving=false;setState('idle');closeSheet();summary()};
  $('share-reset').onclick=()=>invoke('web_share_reset').catch(()=>{});
  $('offer-accept').onclick=()=>{invoke('web_share_decide',{accept:true}).catch(()=>{});showReceiving(0,lastOffer?.total||0)};
  $('offer-decline').onclick=()=>{invoke('web_share_decide',{accept:false}).catch(()=>{});setState('idle')};
  window.__TAURI__?.event.listen('web-share',({payload:p})=>{
    const d=p.data||{};
    if(p.kind==='rotated'&&d.info){
      const got=receiving&&lastOffer;showShare(d.info,true);renderDevices([]);setState('idle');
      flash(got?'Received '+plural(lastOffer.count,'file')+'. New QR and code ready.':'New QR and code ready.');lastOffer=null;receiving=false;summary();return;
    }
    if(p.kind==='code'&&d.info){showShare(d.info,false);return}
    if(p.kind==='connected'){renderDevices(d.devices||[]);flash((d.devices||[]).slice(-1)[0]+' connected');return}
    if(p.kind==='offer'&&d.offer){showOffer(d.offer);return}
    if(p.kind==='offer-closed'){if(d.accepted===false)setState('idle');return}
    if(p.kind==='progress'){if(!receiving)lastOffer=lastOffer||{count:1,total:d.total};showReceiving(d.done,d.total);return}
    const say={downloaded:'Downloaded '+d.name,uploaded:'Received '+d.name,locked:'Stopped: too many wrong codes',ended:'Sharing ended'}[p.kind];
    if(p.kind==='ended'||p.kind==='locked'){sharing=false;receiving=false;$('send-qr').width=0;shownCode='';paintCode();setState('idle');if(view==='share')closeSheet()}
    if(say){flash(say);note(say,p.kind==='locked')}summary();
  });
  $('send-go').onclick=async()=>{
    if(method==='browser')return startShare();
    const paths=files.filter(f=>chosen.has(f.path)).map(f=>f.path);if(sending||!paths.length||!peers.has(device))return;
    clearTimeout(doneTimer);root.classList.remove('done');sending=true;hold(true);summary();note('Waiting…');root.classList.add('noted');progress(0,0);
    try{const r=await invoke('send_files',{device,paths,pin:$('send-pin').value});r.completed.forEach(p=>chosen.delete(p));
      // Sent files leave Send; anything declined or failed stays for a retry.
      if(r.completed.length)setFiles(await invoke('send_remove',{paths:r.completed}).catch(()=>files));
      if(r.error||r.skipped){note(r.completed.length+'/'+paths.length+' sent · '+(r.error||r.skipped+' declined'),true)}
      else{note('Sent '+r.completed.length+(r.completed.length===1?' file':' files'))}
      $('send-pin').hidden=true;$('send-pin').value='';
    }catch(e){note(String(e),true);if(String(e).includes('PIN')){$('send-pin').hidden=false;$('send-pin').focus()}}
    finally{sending=false;$('send-meter').hidden=true;root.style.removeProperty('--pct');hold(false);
      // Sent: the arrow turns into a check for a moment (the status line stays with it); an error keeps its line.
      const failed=$('send-note').classList.contains('error');root.classList.toggle('done',!failed);root.classList.toggle('noted',true);
      doneTimer=setTimeout(()=>{root.classList.remove('done');if(!$('send-note').classList.contains('error'))root.classList.remove('noted')},2600);
      summary()}
  };
  function setFiles(list){
    files=(list||[]).filter(f=>!f.directory&&f.available);const live=new Set(files.map(f=>f.path));
    for(const p of chosen)if(!live.has(p))chosen.delete(p);for(const p of art.keys())if(!live.has(p))art.delete(p);
    if(!initialized&&files.length){initialized=true;chosen.add(files[0].path)}
    if(view==='files')fileRows();summary()
  }
  const refresh=async()=>{try{setFiles(await invoke('send_list')||[])}catch(e){note(String(e),true)}};
  window.__TAURI__?.event.listen('send-updated',({payload})=>{initialized=true;const before=new Set(files.map(f=>f.path));setFiles(payload||[]);for(const f of files)if(!before.has(f.path))chosen.add(f.path);summary()});
  return {
    // What the (i) in the header says about the way picked now.
    help(){return method==='browser'?'Scan the QR code with a phone to send the files you picked, or to send files from the phone to this PC.':method==='quickshare'?'Open Quick Share on Android and set it to Everyone.':'Open LocalSend on your phone.'},
    // Files handed over from the Tray: added to Send and selected.
    async add(paths){initialized=true;setFiles(await invoke('send_add',{paths})||[]);for(const p of paths)if(files.some(f=>f.path===p))chosen.add(p);summary()},
    enter(){refresh();if(view)closeSheet();if(!sending)note('');summary();syncShare();prepare()},
    // The panel opened again on Send: show the QR and code that are valid now.
    // A reopened panel asks for the height of its tab: a sheet that is still open asks again for its taller one.
    shown(){syncShare();if(isTall){isTall=false;setTimeout(()=>tall(true),60)}}};
}
