// Agents tab switcher: with two or more agents at work the right column lists them (above plan usage); a click puts
// that agent on the main card, and it stays there while sessions update. One agent: no switcher. An approval still
// comes first. Screenshot: tests/artifacts/agent-switch.png.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404).end();return}res.writeHead(200,{'content-type':type(name)}).end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18781,'127.0.0.1',r));
  const browser=await chromium.launch({channel:process.env.PW_CHANNEL||'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:760,height:430},deviceScaleFactor:2});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(now=>{
    const L={};window.__emit=(n,p)=>L[n]?.({payload:p});window.__calls=[];
    const s=(id,agent,project,state,prompt,at)=>({id,agent,project,state,steps:[],count:0,at,cwd:'C:\\w\\'+project,prompt,plan:[],action:'',started:at-60000,last:'',error:'',changes:[]});
    window.__sessions=[s('a','claude-code','korus','working','Fix the billing chart',now),s('b','codex','sora','working','Add the agent switcher',now-1000),s('c','gemini','notes','thinking','Sum up the notes',now-2000)];
    window.__pending=[];
    window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,a)=>{
      window.__calls.push([name,a]);
      if(name==='labs_state')return{agents:true,meeting:false,local_ai:false};
      if(name==='agents_state')return window.__sessions;
      if(name==='agent_pending')return window.__pending;
      if(name==='agent_hooks')return{claude:true};
      if(name==='agent_usage')return window.__agy?{claude:null,codex:null,antigravity:window.__agy}:{claude:null,codex:{primary:{used:42,resets:now+3600000,mins:300},secondary:{used:12,resets:now+86400000*3,mins:10080}}};
      if(name==='monitors'||name==='shelf_list'||name==='board_list'||name==='history_list'||name==='app_marks')return[];
      if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}};
  },Date.now());
  await page.goto('http://127.0.0.1:18781/index.html?panel=1&scale=1&tab=home');await page.addStyleTag({content:'html,body{background:#000!important}'});
  await page.waitForSelector('#home.active');await page.evaluate(()=>window.__emit('panel-show'));await page.waitForTimeout(500);
  await page.click('[data-tab=agents]');await page.waitForSelector('#agents.active');await page.waitForTimeout(800);

  const picks=page.locator('#ai-side .ai-pick');
  assert.equal(await picks.count(),3,'one row per agent at work');
  assert.match(await page.locator('#ai-side').textContent(),/Agents · 3.*Plan usage/s,'switcher above plan usage');
  await page.screenshot({path:path.join(artifacts,'agent-switch.png')});
  const side=await page.locator('#ai-side').boundingBox(),last=await page.locator('#ai-side .ai-limit').last().boundingBox();
  assert.ok(last.y+last.height<=side.y+side.height+1,'plan usage still fits under the switcher');
  assert.match(await page.locator('#ai-main .ai-prompt').textContent(),/billing chart/,'the newest busy one first');
  assert.equal(await picks.nth(0).getAttribute('aria-pressed'),'true');

  await page.waitForTimeout(700);await picks.nth(1).click();await page.waitForTimeout(150);
  assert.match(await page.locator('#ai-main .ai-prompt').textContent(),/agent switcher/,'a click shows that agent');
  assert.match(await page.locator('#ai-side .ai-pick.on').getAttribute('aria-label'),/^sora/);
  // An update from Rust (the first agent is newer again) keeps the picked one on the card.
  await page.evaluate(()=>{window.__sessions[0].at=Date.now()+5000;window.__emit('agents-updated',window.__sessions)});await page.waitForTimeout(300);
  assert.match(await page.locator('#ai-main .ai-prompt').textContent(),/agent switcher/,'the pick stays after an update');

  // An approval still takes the card; the switcher then marks none as shown.
  await page.evaluate(()=>{window.__pending=[{id:'p1',session:'a',agent:'claude-code',project:'korus',tool:'Bash',verb:'run a command',detail:'npm test',questions:[],always:null,at:Date.now()}];window.__emit('agents-pending',window.__pending)});
  await page.waitForTimeout(300);
  assert.match(await page.locator('#ai-main').textContent(),/wants to run a command/,'an approval comes first');

  // One agent left: no switcher.
  await page.evaluate(()=>{window.__pending=[];window.__sessions=window.__sessions.slice(0,1);window.__emit('agents-pending',[]);window.__emit('agents-updated',window.__sessions)});
  await page.waitForTimeout(300);
  assert.equal(await picks.count(),0,'one agent: no switcher');

  // Antigravity: one row, its most used model.
  await page.evaluate(()=>{window.__agy={models:[{label:'Gemini 3.8 Flash',used:10,resets:Date.now()+3600000},{label:'Claude Opus 4.6',used:80,resets:Date.now()+3600000},{label:'GPT-OSS 120B',used:0,resets:null},{label:'Claude Sonnet 4.6',used:40,resets:null}],plan:'Pro'};window.__emit('agents-usage')});
  await page.waitForTimeout(300);
  assert.deepEqual(await page.locator('#ai-side .ai-limit span').evaluateAll(s=>s.map(x=>x.firstChild.textContent)),['Antigravity · Opus 4.6'],'one row: the most used model');
  await page.screenshot({path:path.join(artifacts,'agent-usage-antigravity.png')});
  assert.equal(await page.locator('#agents select, #agents input[type=checkbox], #agents input[type=range]').count(),0,'no classic controls');
  assert.deepEqual(errors,[]);
  console.log('agent-switch PASS');
  await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
