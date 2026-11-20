// An isolated notch-size fixture. No OS cursor, clipboard or network peers are used.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const {chromium}=require('playwright'),ui=path.resolve(__dirname,'../ui');
async function run(){
 const section=fs.readFileSync(path.join(ui,'index.html'),'utf8').split('<section id="send"')[1].split('<section id="location-editor"')[0];
 const server=http.createServer((req,res)=>{if(req.url==='/'){res.setHeader('Content-Type','text/html');res.end('<body style="margin:0;background:#090b0a;color:white;font:13px Segoe UI"><style>button{background:none;border:0;color:inherit;cursor:pointer}</style><link rel="stylesheet" href="send.css"><main id="notch"><header><button id="share-disconnect" class="pill-button hidden">Disconnect</button></header><section id="send"'+section+'</main>');return}
 const p=path.join(ui,req.url);if(!p.startsWith(ui+path.sep)||!fs.existsSync(p)){res.writeHead(404).end();return}res.setHeader('Content-Type',p.endsWith('.css')?'text/css':'application/javascript');res.end(fs.readFileSync(p))});
 await new Promise(r=>server.listen(0,'127.0.0.1',r));let b;
 try{
   b=await chromium.launch({channel:'chrome',headless:true});const page=await b.newPage({viewport:{width:720,height:234}});await page.goto('http://127.0.0.1:'+server.address().port);
   await page.evaluate(async()=>{
     const {createSend}=await import('./send.js');const listeners={};window.__TAURI__={event:{listen:(name,cb)=>listeners[name]=cb}};window.__fire=(n,p)=>listeners[n]({payload:p});
     window.queue=Array.from({length:80},(_,i)=>({path:'item-'+i,name:'Field recording — '+i+'.wav',size:1024+i,available:true,directory:false}));
     document.querySelector('#send').classList.add('active');const send=createSend(async(n,args)=>{
       if(n==='file_thumbnail')return null;if(n==='send_list')return window.queue;if(n==='send_remove'){window.queue=window.queue.filter(f=>!args.paths.includes(f.path));return window.queue}
       if(n==='send_add'){for(const p of args.paths)if(!window.queue.some(f=>f.path===p))window.queue.push({path:p,name:p,size:5,available:true,directory:false});return window.queue}if(n==='send_scan'){window.scans=(window.scans||0)+1}if(n==='send_scan')return [{id:'test',alias:'Studio phone',address:'192.168.1.8:53317',protocol:'https'}];
       if(n==='web_share_start'){window.shared=args;window.starts=(window.starts||0)+1;return {url:'https://192.168.1.5:50000/s/x',code:'123456',width:21,modules:Array.from({length:441},(_,i)=>(i*7%3===0)?'1':'0').join(''),seconds:900,trusted:false}}
       if(n==='web_share_state')return window.shareState??null;
       if(n==='web_share_decide'){window.decided=args.accept;return null}if(n==='web_share_reset'){window.reset=true;return null}if(n==='web_share_stop'){window.stopped=true;return null}
       if(n==='send_files'){window.selectedPaths=args.paths;listeners['send-progress']({payload:{index:1,files:80,name:'Field recording',sent:100,total:1000}});await new Promise(r=>window.finish=r);return {completed:args.paths.slice(0,2),skipped:78,error:null}}
     },async()=>[]);window.send=send;send.enter();
   });
   await page.waitForFunction(()=>document.querySelector('#send-name').textContent!=='Choose files');await page.click('#send-source');assert.equal(await page.locator('.batch-row').count(),80);await page.click('#send-all');
   assert.equal(await page.locator('#send-count').textContent(),'80 selected');
   assert(await page.locator('#send-list').evaluate(e=>e.scrollWidth>e.clientWidth&&e.scrollHeight<=e.clientHeight+1),'file cards scroll sideways, never down');
   await page.keyboard.press('Escape');await page.click('#send-peer');await page.waitForFunction(()=>window.scans>=1);await page.waitForSelector('.bubble');assert.equal(await page.evaluate(()=>window.scans),1,'the devices look around by themselves on opening');assert.match(await page.locator('#send-scan').textContent(),/Scan/);await page.click('#send-scan');await page.click('.bubble');await page.waitForFunction(()=>document.querySelector('#send-sheet').hidden);
   await page.click('#send-go');await page.waitForFunction(()=>window.selectedPaths?.length===80&&document.querySelector('#send-meter').value===100);
   await page.evaluate(()=>window.finish());await page.waitForFunction(()=>document.querySelector('#send-note').textContent.includes('78 declined'));
   await page.click('#send-source');assert.equal(await page.locator('.batch-row').count(),78,'sent files leave Send');assert.equal(await page.locator('.batch-row input:checked').count(),78);
   await page.keyboard.press('Escape');await page.evaluate(()=>window.send.add(['from-tray.pdf']));await page.click('#send-source');
   assert.equal(await page.locator('.batch-row').count(),79);assert(await page.locator('.batch-row input[value="from-tray.pdf"]').isChecked(),'Send to… selects the file');
   // QR Code: chosen from the method menu; then scan, devices, an offer from the phone, progress, a new QR after use, instant disconnect.
   const fire=(kind,data)=>page.evaluate(([k,d])=>window.__fire('web-share',{kind:k,data:d}),[kind,data]);
   await page.keyboard.press('Escape');await page.click('#send-peer');
   // Enter IP is a view of its own (the radar goes away, nothing moves); its error stays inside it and is gone with the sheet.
   assert.equal(await page.locator('.radar-info').count(),0,'no (i) on the radar');assert.equal(await page.locator('.radar-hint').count(),0,'no hint text on the radar');
   const noteBefore=await page.locator('#send-note').textContent();await page.click('#send-manual');assert.equal(await page.locator('#send-list').isHidden(),true);assert.equal(await page.locator('#send-ip-form').isVisible(),true);await page.fill('#send-ip','10.0.0.9');await page.press('#send-ip','Enter');
   await page.waitForFunction(()=>document.querySelector('#send-ip-error').textContent!=='');assert.equal(await page.locator('#send-note').textContent(),noteBefore,'the IP error is not on the tile');
   await page.click('#send-manual');assert.equal(await page.locator('#send-list').isVisible(),true,'Devices brings the radar back');
   assert.equal(await page.locator('#method-menu').isHidden(),true,'the menu opens on a click only');await page.click('#method-button');
   assert.deepEqual(await page.locator('#method-menu button').allTextContents(),['LocalSend','Quick Share','QR Code']);
   await page.click('#method-menu button[data-method=browser]');assert.equal(await page.locator('#method-name').textContent(),'QR Code');assert.equal(await page.locator('#method-menu').isHidden(),true,'picking closes the menu');
   assert.equal(await page.locator('#send-scan').isHidden(),true);assert.equal(await page.locator('#send-manual').isHidden(),true);await page.click('#send-done');
   assert.equal(await page.locator('#send-via').textContent(),'QR Code');assert.equal(await page.locator('#send-go').isDisabled(),false,'no device needed');
   // Nothing picked: the QR still starts, as a page that only receives (phone to PC).
   await page.click('#send-source');await page.click('#send-all');if(await page.locator('#send-count').textContent()!=='0 selected')await page.click('#send-all');await page.click('#send-done');
   assert.equal(await page.locator('#send-go').isDisabled(),false,'QR needs no files');await page.click('#send-peer');assert.equal(await page.locator('.role-pill').count(),0,'no Sender / Receiver choice');await page.click('#send-done');
   assert.equal(await page.locator('#send-name').textContent(),'Receive from phone');assert.equal(await page.locator('#send-go').getAttribute('aria-label'),'Receive files');
   await page.click('#send-source');await page.click('#send-all');await page.click('#send-done');assert.equal(await page.locator('#send-go').isDisabled(),false,'Sender with files can start');
   await page.click('#send-go');await page.waitForFunction(()=>!document.querySelector('#send-share').hidden);
   assert.equal(await page.locator('#send-code').textContent(),'123 456');assert.match(await page.locator('#send-share-note').textContent(),/router blocks/,'says why the phone will warn');assert.deepEqual(await page.evaluate(()=>Object.keys(window.shared)),['paths'],'starting needs only the files: the PC decides about anything coming back');
   {const px=await page.evaluate(()=>{const c=document.querySelector('#send-qr'),g=c.getContext('2d'),m=+c.dataset.px,at=(x,y)=>[...g.getImageData(x*m+(m>>1),y*m+(m>>1),1,1).data.slice(0,3)].join();return {quiet:at(0,0),corner:at(4,4),px:m,w:c.width}});assert.equal(px.quiet,'255,255,255');assert.equal(px.corner,'0,0,0','the QR is black on white whatever the page CSS');assert.equal(px.w,px.px*29)}
   assert.equal(await page.locator('#send-sheet .sheet-head').isHidden(),true);assert.equal(await page.locator('#share-disconnect').isVisible(),true,'Disconnect is in the header');assert.equal(await page.evaluate(()=>getComputedStyle(document.querySelector('#send-sheet')).backgroundColor),'rgba(0, 0, 0, 0)','no box behind the QR view');assert.equal(await page.locator('#send-allow').count(),0,'receiving needs no switch');
   assert.equal(await page.locator('#share-devices .share-none').textContent(),'Nobody yet');
   await fire('connected',{devices:['iPhone','Pixel 8']});assert.deepEqual(await page.locator('.share-device b').allTextContents(),['iPhone','Pixel 8']);assert.equal(await page.locator('#share-reset').isVisible(),true);
   await fire('offer',{offer:{count:32,total:4400000,names:['a.jpg'],device:'iPhone'}});assert.equal(await page.locator('#send-share').getAttribute('data-state'),'offer');
   assert.equal(await page.locator('#offer-title').textContent(),'iPhone');assert.equal(await page.locator('#send-code').textContent(),'123 456','the code stays as the eye left it while a phone asks');assert.equal(await page.locator('#offer-detail').textContent(),'32 files · 4.2 MB');
   await page.locator('#send-share').screenshot({path:require('path').join(__dirname,'artifacts','send-share-offer.png')});
   await page.click('#offer-decline');await page.waitForFunction(()=>window.decided===false);assert.equal(await page.locator('#send-code').textContent(),'123 456','and back after the answer');assert.equal(await page.locator('#send-share').getAttribute('data-state'),'idle','Reject goes back to the code');
   await fire('offer',{offer:{count:32,total:4400000,names:['a.jpg'],device:'iPhone'}});await page.click('#offer-accept');await page.waitForFunction(()=>window.decided===true);
   assert.equal(await page.locator('#send-share').getAttribute('data-state'),'receive');await fire('progress',{done:23,total:100,name:'a.jpg'});
   assert.equal(await page.locator('#recv-pct').textContent(),'23%');assert.match(await page.locator('#recv-label').textContent(),/Receiving 32 files/);
   await page.locator('#send-share').screenshot({path:require('path').join(__dirname,'artifacts','send-share-receiving.png')});
   await fire('progress',{done:100,total:100,name:'a.jpg'});assert.equal(await page.locator('#recv-label').textContent(),'Received');
   assert.equal(await page.locator('#code-eye').count(),0,'no hide-code control');
   await fire('code',{info:{url:'https://x',code:'111222',width:21,modules:'1'.repeat(441),seconds:60}});assert.equal(await page.locator('#send-code').textContent(),'111 222','a new code replaces the old one at once');
   await fire('rotated',{info:{url:'https://x',code:'654321',width:21,modules:'1'.repeat(441),seconds:900}});await page.waitForFunction(()=>document.querySelector('#send-code').textContent==='654 321');
   assert.equal(await page.locator('#send-share').getAttribute('data-state'),'idle');assert.equal(await page.locator('#share-devices .share-none').count(),1,'everybody is out after the code is replaced');assert.match(await page.locator('#send-share-note').textContent(),/Received 32 files/);
   await page.locator('#send-share').screenshot({path:require('path').join(__dirname,'artifacts','send-share.png')});
   await fire('connected',{devices:['iPhone']});await page.click('#share-reset');await page.waitForFunction(()=>window.reset===true);
   await page.keyboard.press('Escape');await page.waitForFunction(()=>document.querySelector('#send-sheet').hidden);
   // The panel slept while the link changed: showing it again draws the QR and code that are valid now, and the arrow reopens that link.
   await page.evaluate(()=>{window.shareState={info:{url:'https://y',code:'999888',width:21,modules:'0'.repeat(441),seconds:60,trusted:false},devices:[],offer:null};window.send.shown()});
   await page.waitForFunction(()=>document.querySelector('#send-code').textContent==='999 888');
   const before=await page.evaluate(()=>window.starts);await page.click('#send-go');await page.waitForFunction(()=>!document.querySelector('#send-share').hidden);
   assert.equal(await page.evaluate(()=>window.starts),before,'a live link is reopened, not replaced');assert.equal(await page.locator('#send-code').textContent(),'999 888');
   // It ended while the panel slept: the arrow makes a new one instead of showing the dead QR.
   await page.keyboard.press('Escape');await page.evaluate(()=>{window.shareState=null;window.send.shown()});await page.waitForTimeout(50);
   await page.click('#send-go');await page.waitForFunction(n=>window.starts===n&&!document.querySelector('#send-share').hidden,before+1);assert.equal(await page.locator('#send-code').textContent(),'123 456');
   await page.click('#share-disconnect');await page.waitForFunction(()=>window.stopped===true&&document.querySelector('#send-sheet').hidden);
   console.log('PASS 80-file layout, select all, aggregate progress, partial approval, retry selection, sent files removed and Send to… from the Tray');
 }finally{if(b)await b.close();server.close()}
}
run().catch(e=>{console.error(e);process.exitCode=1});
