// Home's hidden menus (Event, Focus, Markets, To-do, Alarm) in the panel page with a fake Tauri backend: the Note replacing
// Quick, the Focus menu (length and today's total), code search in Markets, tasks and alarms with a Repeat every day
// switch. Screenshots go to tests/artifacts/menus-*.png.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',type(name));res.end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18773,'127.0.0.1',r));
  const browser=await chromium.launch({channel:'chrome',headless:true});
  const open=async(widgets,seed={})=>{
    const page=await browser.newPage({viewport:{width:760,height:400},deviceScaleFactor:2});const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.addInitScript(([w,seed])=>{localStorage.setItem('sora.home-widgets',JSON.stringify(w));for(const [k,v] of Object.entries(seed))if(localStorage.getItem(k)===null)localStorage.setItem(k,JSON.stringify(v));const L={};window.__emit=(n,p)=>L[n]?.({payload:p});
      window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,a)=>{
        if(name==='quotes')return(a.symbols||[]).map(s=>({symbol:s,price:100,previous:98,points:[1,2,3]}));
        if(name==='market_search')return[{symbol:'DOT-USD',name:'Polkadot USD',kind:'Cryptocurrency',exchange:'CCC'}];
        if(name==='monitors'||name==='shelf_list')return[];if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}}},[widgets,seed]);
    await page.goto('http://127.0.0.1:18773/index.html?panel=1&scale=1&tab=home');await page.addStyleTag({content:'html,body{background:#000!important}'});
    await page.waitForSelector('#home.active');await page.evaluate(()=>window.__emit('panel-show'));await page.waitForTimeout(400);return {page,errors};
  };
  const turn=(page,id,n)=>page.evaluate(([id,n])=>{const w=document.getElementById(id);w.scrollTop=n*(w.children[1].offsetTop-w.children[0].offsetTop)},[id,n]);
  const noClassic=async(page,sel)=>assert.equal(await page.locator(sel+' select, '+sel+' input[type=checkbox], '+sel+' input[type=range], '+sel+' input[type=date]:not(.native-store)').count(),0,sel+' has no classic controls');
  const on=(page,id)=>page.locator('#'+id).getAttribute('aria-checked');

  // ---- Event: a Note instead of Quick
  {const {page,errors}=await open(['calendar','todo','alarm']);
   await page.click('#add-event');await page.waitForTimeout(500);
   assert.equal(await page.locator('#event-editor [data-quick-day], #event-editor [data-quick-time]').count(),0,'no Quick picks');
   assert.equal((await page.locator('#event-editor .t-label').allTextContents()).includes('Quick'),false);
   assert.equal(await page.locator('#event-editor .t-memo .t-label').textContent(),'Note');
   await page.fill('#event-title','Team sync');await page.fill('#event-note','Bring the report');
   assert.equal(await page.locator('#note-count').textContent(),'16/120');
   await page.screenshot({path:path.join(artifacts,'menus-event.png')});
   await page.evaluate(()=>{const d=new Date();d.setDate(d.getDate()+2);document.getElementById('event-date').value=d.getFullYear()+'-'+String(d.getMonth()+1).padStart(2,'0')+'-'+String(d.getDate()).padStart(2,'0')});
   await page.click('#event-form .primary-action');await page.waitForTimeout(300);
   const saved=await page.evaluate(()=>JSON.parse(localStorage.getItem('nook.agenda')));
   assert.equal(saved.at(-1).note,'Bring the report','the note is saved with the event');
   await noClassic(page,'#event-editor');assert.deepEqual(errors,[]);await page.close()}

  // ---- Focus: length and today
  {const today=await (async()=>{const d=new Date();return d.getFullYear()+'-'+String(d.getMonth()+1).padStart(2,'0')+'-'+String(d.getDate()).padStart(2,'0')})();
   const {page,errors}=await open(['timer','markets','todo'],{'sora.focus':{log:{[today]:{sec:5400,n:3}}}});
   await page.click('#timer-edit');await page.waitForTimeout(500);
   assert.equal(await page.locator('#focus-total').textContent(),'1 h 30 min');assert.equal(await page.locator('#focus-count').textContent(),'3 sessions');
   assert.equal(await page.locator('#focus-week .fw-day').count(),7);assert.equal(await page.locator('#focus-week .fw-day.today').count(),1);
   assert.equal(await page.locator('.timer-presets button').count(),6);
   await page.click('.timer-presets button[data-min="45"]');assert.equal(await page.inputValue('#timer-min-input'),'45');
   assert.equal(await page.locator('.timer-presets button.on').getAttribute('data-min'),'45');
   await page.screenshot({path:path.join(artifacts,'menus-focus.png')});
   await page.click('#timer-form .primary-action');await page.waitForTimeout(300);
   assert.equal(await page.locator('#timer-edit').isVisible(),true,'the menu closes on Set');
   assert.equal(await page.locator('#focus-label, #focus-chime, #focus-windows').count(),0,'no Session tile');
   await noClassic(page,'#timer-editor');assert.deepEqual(errors,[]);await page.close()}

  // ---- Markets: codes
  {const {page,errors}=await open(['timer','markets','todo']);
   await page.click('#markets-edit');await page.waitForTimeout(500);
   assert.equal(await page.locator('#mk-count').textContent(),'3/8');
   assert.deepEqual(await page.locator('.mk-tabs button').allTextContents(),['All','Stocks','Crypto','Goods']);
   assert.equal(await page.locator('.mk-result b').first().textContent(),'GOLD','All starts with what people look for most');
   await page.click('.mk-tabs button[data-kind="crypto"]');
   const coins=await page.locator('.mk-result b').allTextContents();assert.ok(coins.length>=80&&coins.includes('PEPE')&&coins.includes('SUI'),'many coins: '+coins.length);
   await page.click('.mk-tabs button[data-kind="stock"]');
   const shares=await page.locator('.mk-result b').allTextContents();assert.ok(shares.includes('BBCA')&&shares.includes('AAPL')&&shares.includes('TSLA'),'Indonesian and US shares');
   assert.deepEqual(await page.locator('.mk-result .mk-tag').evaluateAll(e=>[...new Set(e.map(x=>x.textContent))].sort()),['IDX','Index','US']);
   await page.click('.mk-tabs button[data-kind="all"]');
   await page.fill('#markets-input','bbca');assert.equal(await page.locator('.mk-result b').first().textContent(),'BBCA');
   assert.equal(await page.locator('.mk-result .mk-tag').first().textContent(),'IDX');
   assert.equal(await page.locator('.mk-result .mk-act').first().textContent(),'Add');
   await page.locator('.mk-result').first().click();
   assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('sora.markets')).includes('BBCA.JK')),true,'BBCA is watched as BBCA.JK');
   assert.equal(await page.locator('#mk-count').textContent(),'4/8');assert.equal(await page.locator('.mk-result .mk-act').first().textContent(),'Added');
   assert.ok((await page.locator('#mk-chosen .mk-row b').allTextContents()).includes('BBCA'));
   await page.fill('#markets-input','zzzz');assert.equal(await page.locator('.mk-result b').first().textContent(),'ZZZZ');
   assert.match(await page.locator('.mk-result span').first().textContent(),/guess/,'a guessed code says it is a guess');
   await page.fill('#markets-input','polka');await page.waitForFunction(()=>[...document.querySelectorAll('.mk-result b')].some(b=>b.textContent==='DOT'),null,{timeout:3000});
   await page.fill('#markets-input','');await page.click('.mk-tabs button[data-kind="commodity"]');
   const com=await page.locator('.mk-result b').allTextContents();assert.ok(['GOLD','SILVER','OIL','COPPER'].every(c=>com.includes(c)),'metals and other commodities: '+com);
   await page.screenshot({path:path.join(artifacts,'menus-markets.png')});
   await noClassic(page,'#markets-editor');assert.deepEqual(errors,[]);await page.close()}

  // ---- To-do: a switch makes it repeat; old tasks are tidied
  {const {page,errors}=await open(['calendar','todo','alarm'],{'sora.todos':[{id:'a',title:'Old once',daily:false,time:'',doneOn:'2020-01-01'},{id:'b',title:'Old daily',daily:true,time:'',doneOn:'2020-01-01'},{id:'c',title:'Saved before the switch',time:'',doneOn:''}]});
   await page.click('#todo-add');await page.waitForTimeout(600);
   const rows=await page.locator('#todo-all .mk-row b').allTextContents();
   assert.deepEqual(rows.sort(),['Old daily','Saved before the switch'],'a one-time task ticked on an earlier day is gone, the others stay');
   assert.equal(await page.locator('#todo-all .mk-row small').first().textContent().then(t=>/^Every day/.test(t)),true,'a task saved before the switch counts as every day');
   assert.equal(/any time/i.test(await page.locator('#todo-editor').innerText()),false,'no "Any time" wording');
   assert.equal(await page.locator('#todo-time-title').textContent(),'No time');assert.equal(await on(page,'todo-daily'),'false','not repeating by default');
   await page.locator('.todo-wheels').dispatchEvent('wheel');await turn(page,'wheel-th',9);await turn(page,'wheel-tm',30);await page.waitForTimeout(300);
   assert.match(await page.locator('#todo-time-title').textContent(),/^09:(29|30|31)$/,'turning a wheel sets a time');
   await page.click('#todo-daily');assert.equal(await on(page,'todo-daily'),'true');
   await page.fill('#todo-title','Stand-up meeting');await page.screenshot({path:path.join(artifacts,'menus-todo.png')});
   await page.click('#todo-save');await page.waitForTimeout(300);
   let todos=await page.evaluate(()=>JSON.parse(localStorage.getItem('sora.todos')));
   const meet=todos.find(t=>t.title==='Stand-up meeting');assert.equal(meet.daily,true);assert.match(meet.time,/^09:(29|30|31)$/);
   await page.click('#todo-add');await page.waitForTimeout(500);
   assert.equal(await on(page,'todo-daily'),'false','the switch starts off again');
   await page.fill('#todo-title','Water plants');await page.click('#todo-save');await page.waitForTimeout(200);
   todos=await page.evaluate(()=>JSON.parse(localStorage.getItem('sora.todos')));
   const water=todos.find(t=>t.title==='Water plants');assert.deepEqual([water.daily,water.time],[false,''],'one time, no time');
   await page.click('#todo-add');await page.waitForTimeout(500);
   const small=await page.locator('#todo-all .mk-row').evaluateAll(r=>r.map(x=>x.querySelector('b').textContent+'|'+x.querySelector('small').textContent));
   assert.ok(small.some(t=>/^Stand-up meeting\|Every day · 09:/.test(t))&&small.includes('Water plants|Today'),'the list says which repeat: '+small);
   await noClassic(page,'#todo-editor');assert.deepEqual(errors,[]);await page.close()}

  // ---- Alarm: once, or every day
  {const {page,errors}=await open(['calendar','todo','alarm'],{'sora.alarms':[{id:'old',label:'Saved before',time:'06:00',on:true}]});
   assert.match(await page.locator('#alarm-list .al small').first().textContent(),/Every day$/,'an alarm saved before the switch repeats');
   await page.click('#alarm-add');await page.waitForTimeout(500);
   assert.equal(await page.locator('.al-quick button').count(),8);assert.equal(await on(page,'alarm-daily'),'false','once by default');
   await page.click('#alarm-daily');assert.equal(await on(page,'alarm-daily'),'true');assert.equal(await page.locator('#alarm-next').count(),0,'no "Once" line');
   await page.click('.al-quick [data-at="21:00"]');await page.waitForFunction(()=>document.getElementById('alarm-title').textContent==='21:00',null,{timeout:3000});
   await page.screenshot({path:path.join(artifacts,'menus-alarm.png')});
   await page.fill('#alarm-label','Evening walk');await page.click('#alarm-save');await page.waitForTimeout(200);
   let al=await page.evaluate(()=>JSON.parse(localStorage.getItem('sora.alarms')));const walk=al.find(a=>a.label==='Evening walk');
   assert.deepEqual([walk.time,walk.repeat,walk.on],['21:00',true,true]);
   await page.click('#alarm-add');await page.waitForTimeout(500);assert.equal(await on(page,'alarm-daily'),'false');
   await page.click('.al-quick [data-at="05:00"]');await page.waitForFunction(()=>document.getElementById('alarm-title').textContent==='05:00',null,{timeout:3000});
   await page.fill('#alarm-label','Once only');await page.click('#alarm-save');await page.waitForTimeout(200);
   al=await page.evaluate(()=>JSON.parse(localStorage.getItem('sora.alarms')));const once=al.find(a=>a.label==='Once only');
   assert.equal(once.repeat,false);assert.ok(once.ringAt>Date.now(),'a one-time alarm keeps the moment it rings');
   assert.ok((await page.locator('#alarm-list .al small').allTextContents()).some(t=>t==='Once only · Once'));
   // once its time is past it turns itself off
   await page.evaluate(()=>{const a=JSON.parse(localStorage.getItem('sora.alarms'));a.find(x=>x.label==='Once only').ringAt=Date.now()-3600e3;localStorage.setItem('sora.alarms',JSON.stringify(a))});
   await page.reload();await page.addStyleTag({content:'html,body{background:#000!important}'});await page.waitForSelector('#home.active');await page.waitForTimeout(500);
   al=await page.evaluate(()=>JSON.parse(localStorage.getItem('sora.alarms')));assert.equal(al.find(a=>a.label==='Once only').on,false,'a one-time alarm is off once its time has passed');
   assert.equal(al.find(a=>a.label==='Evening walk').on,true,'a repeating alarm stays on');
   await noClassic(page,'#alarm-editor');assert.deepEqual(errors,[]);await page.close()}

  console.log('PASS Home menus: Event note, Focus, Markets codes, To-do and Alarm with a Repeat every day switch.');
  await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
