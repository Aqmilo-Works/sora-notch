import {glyph} from './tool-icons.js';
export function createTrayMenu({invoke, refreshShelf, remove, hold, report, sendTo}) {
  const layer = document.getElementById('tray-menu-layer');
  const menu = document.getElementById('tray-menu');
  // The conversion formats open in a second menu beside the first one (like a desktop submenu).
  const options = document.getElementById('tray-submenu');
  const tools = menu.querySelector('.tm-tools');
  const status = document.getElementById('convert-status');
  let hoverTimer=0,convertingPath=null;
  const text=document.createElement('span'),meter=document.createElement('progress');meter.id='convert-meter';meter.max=100;meter.hidden=true;meter.setAttribute('aria-label','Conversion progress');status.replaceChildren(text,meter);
  window.__TAURI__?.event.listen('convert-progress',({payload:p})=>{if(!busy||p.path!==convertingPath)return;meter.value=p.percent;text.textContent=p.stage+' · '+p.percent+'%';status.title=text.textContent});
  let backToActions=null;
  let selected = null, anchor = null, version = 0, busy = false;

  function closeSub(){clearTimeout(hoverTimer);options.hidden=true;options.replaceChildren();backToActions=null;menu.querySelector('.to-formats')?.setAttribute('aria-expanded','false')}
  function close(restore = false) {
    if (layer.hidden) return;
    clearTimeout(hoverTimer);version++;
    closeSub();
    layer.hidden = true;
    anchor?.setAttribute('aria-expanded', 'false');
    if (restore && anchor?.isConnected) anchor.focus({preventScroll:true});
    selected = anchor = null;
    hold(false);
  }

  function button(container, text, handler, disabled = false) {
    const el = document.createElement('button');
    el.type = 'button';
    el.textContent = text;
    el.disabled = disabled;
    el.addEventListener('click', handler);
    container.append(el);
    return el;
  }

  // A finished or failed conversion says so for a moment, then the header is clear again.
  let statusTimer=0;
  function showStatus(text, state) {
    clearTimeout(statusTimer);
    if (state !== 'busy') statusTimer = setTimeout(() => { status.hidden = true; }, state === 'error' ? 6000 : 3500);
    status.hidden = false;
    status.dataset.state = state;
    status.firstElementChild.textContent = text;meter.hidden=state!=='busy';
    status.title = text;
  }

  async function convert(file, format) {
    if (busy) return;
    close(true);
    busy = true;convertingPath=file.path;meter.removeAttribute('value');
    showStatus('Converting to ' + format.toUpperCase() + '…', 'busy');
    try {
      const result = await invoke('tray_convert', {path:file.path, format});
      showStatus((result.added ? 'Added to Tray · ' : 'Saved beside original · ') + result.name, 'done');
      await refreshShelf();
    } catch (error) {
      showStatus('Conversion failed · ' + String(error), 'error');
      report(error);
    } finally {
      busy = false;convertingPath=null;
      for (const el of options.querySelectorAll('[data-format]')) el.disabled = false;
    }
  }

  function position(x, y) {
    const rect = layer.getBoundingClientRect();
    if (!rect.width || !rect.height) return;
    const localX = (x - rect.left) * layer.clientWidth / rect.width;
    const localY = (y - rect.top) * layer.clientHeight / rect.height;
    const w = menu.offsetWidth, h = menu.offsetHeight;
    const left = localX + w + 14 <= layer.clientWidth ? localX + 8 : localX - w - 8;
    menu.style.left = Math.max(10, Math.min(left, layer.clientWidth - w - 10)) + 'px';
    menu.style.top = Math.max(8, Math.min(localY - 26, layer.clientHeight - h - 8)) + 'px';
  }

  async function open(file, target, x, y, keyboard) {
    close();
    const request = ++version;
    selected = file.path;
    anchor = target;
    anchor.setAttribute('aria-expanded','true');
    layer.hidden = false;
    hold(true);
    const head=menu.querySelector('.tm-head');let groups=[];
    function heading(){
      head.replaceChildren(glyph(file.directory?'folder':'file'));
      const h=document.createElement('strong');h.textContent=h.title=file.name;head.append(h)
    }
    const act=fn=>async()=>{close(true);try{await fn()}catch(e){report(e)}};
    function action(text,icon,fn,disabled=false){
      const b=button(tools,text,fn,disabled);b.prepend(glyph(icon));b.onpointerenter=closeSub;return b
    }
    function actions(focus=false){
      const reopen=!options.hidden;closeSub();heading();tools.replaceChildren();
      action('Open','open',act(()=>invoke('shelf_open',{path:file.path,reveal:false})),!file.available);
      action('Open folder','folder',act(()=>invoke('shelf_open',{path:file.path,reveal:true})),!file.available);
      if(groups.length){const b=action('Convert to','convert',()=>formats(0,true));b.className='to-formats';b.setAttribute('aria-haspopup','menu');b.setAttribute('aria-expanded','false');b.onpointerenter=e=>{if(e.pointerType==='touch'||!options.hidden)return;clearTimeout(hoverTimer);hoverTimer=setTimeout(()=>{if(request===version&&b.isConnected)formats(0,false)},135)};b.onpointerleave=()=>clearTimeout(hoverTimer);const g=glyph('right');g.classList.add('tail');b.append(g)}
      action('Send to…','phone',act(()=>sendTo(file.path)),!file.available||file.directory);
      action('Remove from Tray','trash',act(()=>remove(file.path))).classList.add('delete');
      position(x,y);if(reopen&&groups.length)formats(0,false);if(focus)(tools.querySelector('.to-formats')||tools.querySelector('button:enabled'))?.focus()
    }
    function formats(index,focus){
      const opener=tools.querySelector('.to-formats');if(!opener)return;
      options.replaceChildren();options.hidden=false;opener.setAttribute('aria-expanded','true');
      backToActions=()=>{closeSub();opener.focus()};
      const group=groups[index],short={'Convert video':'Video','Extract audio':'Audio','Single-page document':'PDF','Save image as':'Image','Still image · first frame':'Still image'};
      if(groups.length>1){const tabs=document.createElement('div');tabs.className='tm-tabs';tabs.setAttribute('role','group');tabs.setAttribute('aria-label','Conversion type');
        groups.forEach((g,i)=>{const b=button(tabs,short[g.label]||g.label,()=>formats(i,true));b.title=g.label;b.setAttribute('aria-pressed',String(i===index))});options.append(tabs)}
      const caption=document.createElement('span');caption.className='caption';caption.textContent=group.label;options.append(caption);
      const grid=document.createElement('div');grid.className='tm-grid';for(const f of group.formats){const b=button(grid,f.toUpperCase(),()=>convert(file,f),busy);b.dataset.format=f;b.setAttribute('aria-label',group.label+' · '+f.toUpperCase())}
      options.append(grid);placeSub(opener);if(focus)grid.querySelector('button:enabled')?.focus()
    }
    // Beside the menu, level with "Convert to"; on the left when there is no room on the right.
    function placeSub(opener){
      const w=options.offsetWidth,h=options.offsetHeight,W=layer.clientWidth,H=layer.clientHeight;
      let left=menu.offsetLeft+menu.offsetWidth+6;if(left+w>W-8)left=menu.offsetLeft-w-6;
      options.style.left=Math.max(8,left)+'px';
      options.style.top=Math.max(8,Math.min(menu.offsetTop+opener.offsetTop-menu.scrollTop-6,H-h-8))+'px';
    }
    actions();menu.focus({preventScroll:true});
    if(file.available&&!file.directory){try{groups=await invoke('tray_conversion_options',{path:file.path})||[]}catch{groups=[]}}
    if(request!==version)return;
    const oldFocus=document.activeElement,wasMenu=oldFocus===menu;
    const oldText=tools.contains(oldFocus)?oldFocus.textContent:null;
    actions();
    if(oldText)[...tools.children].find(b=>b.textContent===oldText)?.focus();
    else if(keyboard&&wasMenu)menu.querySelector('button:enabled')?.focus();

  }

  function attach(item, target, file) {
    target.title=file.name;
    target.setAttribute('aria-haspopup','dialog');
    target.setAttribute('aria-expanded','false');
    item.oncontextmenu = event => {
      event.preventDefault();
      open(file, target, event.clientX, event.clientY, false);
    };
    target.addEventListener('keydown', event => {
      if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
      event.preventDefault();
      const rect = target.getBoundingClientRect();
      open(file, target, rect.right, rect.top, true);
    });
  }

  layer.addEventListener('pointerdown', e => { if (e.target === layer) {e.preventDefault();close(true);} });
  layer.addEventListener('contextmenu', e => { e.preventDefault(); if(e.target === layer) close(true); });
  window.addEventListener('blur', ()=>close());
  new MutationObserver(()=>{
    if (document.getElementById('expanded').inert || document.body.classList.contains('edge')) close();
  }).observe(document.getElementById('expanded'), {attributes:true,attributeFilter:['inert']});
  document.addEventListener('keydown', e => {
    if (layer.hidden) return;
    if ((e.key==='Escape'||e.key==='ArrowLeft') && backToActions && options.contains(document.activeElement)) {e.preventDefault();e.stopImmediatePropagation();backToActions();return;}
    if (e.key==='ArrowRight' && document.activeElement?.classList.contains('to-formats')) {e.preventDefault();e.stopImmediatePropagation();document.activeElement.click();return;}
    if (e.key === 'Escape') { e.preventDefault();e.stopImmediatePropagation();close(true);return; }
    if (!['ArrowRight','ArrowLeft','ArrowDown','ArrowUp','Tab','Home','End'].includes(e.key)) return;
    const buttons = [...(options.contains(document.activeElement)?options:menu).querySelectorAll('button:not(:disabled)')];
    if (!buttons.length) return;
    e.preventDefault();
    const forward = e.key === 'ArrowRight' || e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey);
    const i = buttons.indexOf(document.activeElement);
    const next = e.key === 'Home' ? 0 : e.key === 'End' ? buttons.length - 1 : (i + (forward ? 1 : -1) + buttons.length) % buttons.length;
    buttons[next].focus();
  }, true);
  return {attach, close, reconcile(files) {
    if (selected && !files.some(file=>file.path===selected)) close();

  }};
}
