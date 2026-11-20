// The Microsoft Store edition (labs_state.store): Settings has no AI Agent or Chat and no Beta frame, search finds nothing of them,
// and the panel hides the chat history button. The full edition is covered by settings-ui and ui tests.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../ui');
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.png':'image/png','.woff2':'font/woff2'};
const server=http.createServer((req,res)=>{const u=req.url.split('?')[0];const file=path.join(root,u==='/'?'index.html':u.slice(1));if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',types[path.extname(file)]||'application/octet-stream');res.end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18768,'127.0.0.1',r));
  const browser=await chromium.launch({channel:process.env.PW_CHANNEL||'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:900,height:700}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    const desk={shortcut:'Ctrl+Alt+Space',hide_maximized:false,game_mode:'off',edge_handle:true,notch_position:'center',notch_display:'primary',notch_size:100,feature_agents:false,feature_meeting:false,feature_local_ai:false,feature_whatsapp:false,feature_telegram:false,hide_chat_text:false,meeting_language:'auto',meeting_words:''};
    window.__TAURI__={event:{listen:async()=>()=>{}},core:{invoke:async(name)=>{
      if(name==='labs_state')return{agents:false,meeting:false,local_ai:false,whatsapp:false,telegram:false,store:true};
      if(name==='desktop_preferences')return{preferences:desk,maximized:false,fullscreen:false};
      if(name==='app_version')return'2.0.0';if(name==='startup_enabled')return false;if(name==='notch_displays')return[];
      if(name==='check_update')return{configured:false,available:false,current:'2.0.0',latest:'',size:0};
      if(name==='meeting_list')return{folder:'Documents/Sora Recordings',items:[],consent_seen:true};
      if(name==='meeting_scan')return{isolation:true,build:26100,candidates:[]};
      return null}}};
  });
  await page.goto('http://127.0.0.1:18768/settings.html');await page.evaluate(()=>document.fonts.ready);
  await page.waitForFunction(()=>!document.querySelector('.tab[data-section=agent]'),null,{timeout:3000});
  assert.deepEqual((await page.locator('.tab').allTextContents()).map(t=>t.trim()),['General','Notch','Meeting'],'Store tabs');
  assert.equal(await page.locator('#advanced-label').count(),0,'no Beta label');
  await page.locator('.tab[data-section=meeting]').click();
  assert.equal(await page.locator('#pane-title').textContent(),'Meeting');assert.equal(await page.locator('#pane-badge').isHidden(),true,'Meeting is not marked Beta');
  await page.locator('#filter').fill('whatsapp');
  assert.equal(await page.locator('.tab:not(.hidden)').count(),0,'search finds no chat settings');
  await page.locator('#filter').fill('');
  await page.goto('http://127.0.0.1:18768/index.html');
  await page.waitForFunction(()=>document.getElementById('history-button')?.style.display==='none',null,{timeout:5000});
  assert.deepEqual(errors,[]);await browser.close();server.close();console.log('store-ui PASS');
})().catch(e=>{console.error(e);process.exit(1)});
