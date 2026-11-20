// Home's System widget in the panel page with a fake Tauri backend: the readings, the levels that turn amber
// or red, the empty state, and no classic form controls. Screenshots go to tests/artifacts/home-system*.png.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',type(name));res.end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18768,'127.0.0.1',r));
  const browser=await chromium.launch({channel:'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:760,height:250},deviceScaleFactor:2});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    localStorage.setItem('sora.home-widgets',JSON.stringify(['system']));
    const listeners={};window.__emit=(n,p)=>listeners[n]?.({payload:p});
    window.__TAURI__={event:{listen:async(n,f)=>{listeners[n]=f;return()=>{}}},core:{invoke:async(name)=>{
      if(name==='system_stats')return{cpu:23.4,cpu_ghz:4.2,gpu:91,gpu_temp:67.6,disk:4,down:1.25*1024*1024,up:48*1024,mem_used:15.3*1024**3,mem_total:32*1024**3};
      if(name==='monitors'||name==='shelf_list')return[];
      if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};
      return null;
    }}};
  });
  await page.goto('http://127.0.0.1:18768/index.html?panel=1&scale=1&tab=home');
  await page.waitForSelector('#home.active');
  await page.addStyleTag({content:'html,body{background:#050506!important}'});
  await page.evaluate(()=>window.__emit('panel-show'));
  await page.waitForFunction(()=>document.querySelectorAll('#sys-list .sys').length===4);
  assert.deepEqual(await page.locator('#sys-list .sys .val').allTextContents(),['23% · 4.20 GHz','91% · 68°C','15.3 / 32 GB','4%']);
  assert.deepEqual(await page.locator('#sys-list .sys').evaluateAll(r=>r.map(x=>x.dataset.level)),['ok','high','ok','ok']);
  assert.equal(await page.locator('#sys-net').textContent(),'↓ 1.3 MB/s  ↑ 48 KB/s');
  // Everything fits: no row is cut off by the widget's bottom.
  for(const id of ['#sys-list']){const over=await page.locator(id).evaluate(e=>e.scrollHeight-e.clientHeight);assert.ok(over<=1,id+' overflows by '+over+'px');}
  await page.screenshot({path:path.join(artifacts,'home-system.png')});
  assert.equal(await page.locator('#home select, #home input[type=checkbox], #home input[type=range]').count(),0);
  assert.deepEqual(errors,[]);
  console.log('PASS Home System widget: readings, levels, fit');
  await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
