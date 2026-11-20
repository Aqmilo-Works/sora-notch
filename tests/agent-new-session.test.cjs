// New session sheet in the Agents tab: the idle card offers "New session"; the sheet lets you pick the agent (only those
// installed) and starts it with that agent, folder and prompt. One agent installed: no picker. No classic controls.
// Screenshots: tests/artifacts/agent-new-session.png, agent-idle.png.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404).end();return}res.writeHead(200,{'content-type':type(name)}).end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18782,'127.0.0.1',r));
  const browser=await chromium.launch({channel:process.env.PW_CHANNEL||'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:640,height:220},deviceScaleFactor:2});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    const L={};window.__emit=(n,p)=>L[n]?.({payload:p});window.__calls=[];window.__launchers=['claude-code','codex'];
    window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,a)=>{
      window.__calls.push([name,a]);
      if(name==='labs_state')return{agents:true,meeting:false,local_ai:false};
      if(name==='agents_state'||name==='agent_pending')return[];
      if(name==='agent_hooks')return{claude_code:true,codex:true};
      if(name==='agent_folders')return['C:\\w\\korus','C:\\w\\sora'];
      if(name==='agent_launchers')return window.__launchers;
      if(name==='agent_new_session')return null;
      if(name==='monitors'||name==='shelf_list'||name==='board_list'||name==='history_list'||name==='app_marks')return[];
      if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}};
  });
  await page.goto('http://127.0.0.1:18782/index.html?panel=1&scale=1&tab=home');await page.addStyleTag({content:'html,body{background:#000!important}'});
  await page.waitForSelector('#home.active');await page.evaluate(()=>window.__emit('panel-show'));await page.waitForTimeout(400);
  await page.click('[data-tab=agents]');await page.waitForSelector('#agents.active');await page.waitForTimeout(900);
  await page.screenshot({path:path.join(artifacts,'agent-idle.png')});
  const main=await page.locator('#ai-main').boundingBox(),orbit=await page.locator('.ai-orbit').boundingBox();
  assert.ok(orbit.x-14>=main.x-1,'the orbit and its swinging marks stay inside the column');
  assert.equal(await page.locator('#ai-main button:has-text("New Claude Code session")').count(),0,'no Claude-only button');

  await page.click('#ai-main button:has-text("New session")');await page.waitForTimeout(300);
  assert.match(await page.locator('.ai-sheet-title').textContent(),/^New session$/);
  const picks=page.locator('.ai-agent');
  assert.deepEqual(await page.locator('.ai-agent .ai-agent-name').allTextContents(),['Claude Code','Codex'],'one chip per installed agent');
  assert.equal(await picks.nth(0).getAttribute('aria-pressed'),'true','Claude Code first');
  await picks.nth(1).click();await page.waitForTimeout(150);
  assert.equal(await page.locator('.ai-agent.on .ai-agent-name').textContent(),'Codex');
  await page.fill('.ai-input','Fix the login test');
  await page.screenshot({path:path.join(artifacts,'agent-new-session.png')});
  await page.click('#ai-main button:has-text("Start")');await page.waitForTimeout(150);
  const call=await page.evaluate(()=>window.__calls.find(c=>c[0]==='agent_new_session'));
  assert.deepEqual(call[1],{folder:'C:\\w\\korus',prompt:'Fix the login test',agent:'codex'},'starts the picked agent');

  // Only Claude Code installed: nothing to pick, it starts Claude Code.
  await page.evaluate(()=>{window.__launchers=['claude-code']});
  await page.click('#ai-main button:has-text("New session")');await page.waitForTimeout(300);
  assert.equal(await page.locator('.ai-agent').count(),0,'one agent: no picker');
  await page.fill('.ai-input','x');await page.click('#ai-main button:has-text("Start")');await page.waitForTimeout(150);
  assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='agent_new_session').pop()[1].agent),'claude-code');

  assert.equal(await page.locator('#agents select, #agents input[type=checkbox], #agents input[type=range]').count(),0,'no classic controls');
  assert.deepEqual(errors,[]);
  console.log('agent-new-session PASS');
  await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
