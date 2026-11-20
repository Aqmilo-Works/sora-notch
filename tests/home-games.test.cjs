// Home's Steam widget with a fake backend: the fanned covers, the play click, the scroll wheel.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',type(name));res.end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18769,'127.0.0.1',r));
  const browser=await chromium.launch({channel:process.env.PW_CHANNEL||'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:760,height:250},deviceScaleFactor:2});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  const px='data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';
  await page.addInitScript(px=>{
    localStorage.setItem('sora.home-widgets',JSON.stringify(['steam']));
    const listeners={};window.__emit=(n,p)=>listeners[n]?.({payload:p});window.__launched=[];
    window.__TAURI__={event:{listen:async(n,f)=>{listeners[n]=f;return()=>{}}},core:{invoke:async(name,a)=>{
      if(name==='steam_games')return[{id:730,name:'Counter-Strike 2',played:Math.floor(Date.now()/1000)-100,minutes:90,size:1},{id:2767030,name:'Marvel Rivals',played:Math.floor(Date.now()/1000)-86400*3,minutes:4063,size:1},{id:1422450,name:'Deadlock',played:0,minutes:0,size:1}];
      if(name==='steam_running')return 730;
      if(name==='steam_cover'){window.__asked=(window.__asked||[]).concat([[a.id,a.wide]]);return px}
      if(name==='steam_launch'){window.__launched.push(a.id);return null}
      if(name==='monitors'||name==='shelf_list')return[];
      if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};
      return null;
    }}};
  },px);
  await page.goto('http://127.0.0.1:18769/index.html?panel=1&scale=1&tab=home');
  await page.waitForSelector('#home.active');
  await page.addStyleTag({content:'html,body{background:#050506!important}'});
  await page.evaluate(()=>window.__emit('panel-show'));
  await page.waitForFunction(()=>document.querySelectorAll('#steam-stack .steam-card').length===3);
  await page.waitForFunction(()=>document.querySelectorAll('#steam-stack img').length>=3);
  assert.equal(await page.locator('#home .w-steam .st-kicker').textContent(),'Steam Games');
  assert.equal(await page.locator('#steam-name').textContent(),'Counter-Strike 2');
  assert.equal(await page.locator('#steam-when').textContent(),'Playing now');
  await page.screenshot({path:path.join(artifacts,'home-steam.png')});
  await page.click('#steam-stack .steam-card[data-d="0"]');
  { const b=await page.locator("#steam-stack .steam-card[data-d=\"1\"]").boundingBox(); await page.mouse.click(b.x+25,b.y+b.height*0.6); }
  assert.equal(await page.locator('#steam-name').textContent(),'Marvel Rivals');
  assert.equal(await page.locator('#steam-when').textContent(),'3 days ago · 68 h');
  await page.click('#steam-stack .steam-card[data-d="0"]');
  assert.deepEqual(await page.evaluate(()=>window.__launched),[730,2767030]);
  // The wheel turns the games, one per flick.
  await page.mouse.move(120,150); await page.mouse.wheel(0,120);
  await page.waitForFunction(()=>document.querySelector('#steam-name').textContent==='Deadlock');
  await page.waitForTimeout(300); await page.mouse.wheel(0,-120);
  await page.waitForFunction(()=>document.querySelector('#steam-name').textContent==='Marvel Rivals');
  assert.equal(await page.locator('#home select, #home input[type=checkbox], #home input[type=range]').count(),0);
  assert.deepEqual(errors,[]);
  console.log('PASS Home Steam widget');
  await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
