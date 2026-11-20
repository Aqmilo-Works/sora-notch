// Markets: holding the left button and dragging sideways turns the pages.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',type(name));res.end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18771,'127.0.0.1',r));
  const browser=await chromium.launch({channel:'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:760,height:250}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    localStorage.setItem('sora.home-widgets',JSON.stringify(['markets']));localStorage.setItem('sora.markets',JSON.stringify(['AAPL','MSFT','TSLA']));
    const L={};window.__emit=(n,p)=>L[n]?.({payload:p});
    window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async name=>{
      if(name==='quotes')return['AAPL','MSFT','TSLA'].map(s=>({symbol:s,name:s,price:200,previous:190,points:[1,2,3]}));
      if(name==='monitors'||name==='shelf_list')return[];if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}};
  });
  await page.goto('http://127.0.0.1:18771/index.html?panel=1&scale=1&tab=home');
  await page.waitForSelector('#home.active');await page.evaluate(()=>window.__emit('panel-show'));
  await page.waitForFunction(()=>document.querySelector('.mk-page strong')?.textContent==='200.00');
  const pg=()=>page.evaluate(()=>getComputedStyle(document.getElementById('markets-list')).getPropertyValue('--page').trim());
  const b=await page.locator('#markets-list').boundingBox(),cx=b.x+b.width/2,cy=b.y+b.height/2;
  console.log('hit',await page.evaluate(([x,y])=>document.elementFromPoint(x,y)?.outerHTML.slice(0,100),[cx,cy]));
  await page.mouse.move(cx+60,cy);await page.mouse.down();await page.mouse.move(cx-20,cy,{steps:5});
  console.log('mid',await pg());
  await page.mouse.move(cx-100,cy,{steps:5});await page.mouse.up();await page.waitForTimeout(500);
  assert.equal(await pg(),'1','drag left turns to page 1');
  await page.mouse.move(cx-60,cy);await page.mouse.down();await page.mouse.move(cx+60,cy,{steps:5});await page.mouse.up();await page.waitForTimeout(500);
  assert.equal(await pg(),'0','drag right turns back');
  assert.deepEqual(errors,[]);console.log('PASS markets drag');await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
