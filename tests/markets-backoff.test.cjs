// Markets: prices stay when Yahoo starts failing, the widget says "delayed", and requests are not repeated within the hold.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',type(name));res.end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18770,'127.0.0.1',r));
  const browser=await chromium.launch({channel:'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:760,height:250}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    localStorage.setItem('sora.home-widgets',JSON.stringify(['markets']));localStorage.setItem('sora.markets',JSON.stringify(['AAPL']));
    window.__q=0;window.__fail=false;const L={};window.__emit=(n,p)=>L[n]?.({payload:p});
    window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async name=>{
      if(name==='quotes'){window.__q++;return window.__fail?[{symbol:'AAPL',error:true}]:[{symbol:'AAPL',name:'Apple',price:200,previous:190,points:[1,2,3]}]}
      if(name==='monitors'||name==='shelf_list')return[];if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}};
  });
  await page.goto('http://127.0.0.1:18770/index.html?panel=1&scale=1&tab=home');
  await page.waitForSelector('#home.active');await page.evaluate(()=>window.__emit('panel-show'));
  await page.waitForFunction(()=>document.querySelector('.mk-page strong')?.textContent==='200.00');
  const first=await page.evaluate(()=>window.__q);
  for(let i=0;i<3;i++)await page.evaluate(()=>window.__emit('panel-show'));
  assert.equal(await page.evaluate(()=>window.__q),first,'no new request within 5 s');
  await page.evaluate(()=>{window.__fail=true});await page.waitForTimeout(5300);
  await page.evaluate(()=>window.__emit('panel-show'));
  await page.waitForFunction(()=>document.querySelector('[data-widget=markets]').hasAttribute('data-delayed'));
  assert.equal(await page.locator('.mk-page strong').textContent(),'200.00','last price stays');
  const n=await page.evaluate(()=>window.__q);await page.waitForTimeout(500);
  await page.evaluate(()=>window.__emit('panel-show'));
  assert.equal(await page.evaluate(()=>window.__q),n,'held after a failure');
  assert.deepEqual(errors,[]);console.log('PASS markets backoff');await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
