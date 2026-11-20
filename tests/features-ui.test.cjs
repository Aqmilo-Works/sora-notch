// The Board and Chat history tabs in the panel page with a fake Tauri backend: what each tab shows, that no classic
// control is used. Screenshots go to tests/artifacts/features-*.png.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404).end();return}res.writeHead(200,{'content-type':type(name)}).end(fs.readFileSync(file))});
const png='data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';
(async()=>{
  await new Promise(r=>server.listen(18774,'127.0.0.1',r));
  const browser=await chromium.launch({channel:process.env.PW_CHANNEL||'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:760,height:400},deviceScaleFactor:2});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  const now=Math.floor(Date.now()/1000);
  await page.addInitScript(([now,png])=>{
    const L={};window.__emit=(n,p)=>L[n]?.({payload:p});window.__calls=[];
    window.__iconFails=1e9;const items=window.__items=[
      {id:'a',kind:'link',tag:'link',text:'https://www.youtube.com/watch?v=jNQXAC9IVRw',site:'YouTube',media:true,at:now-30,keep:false},
      {id:'b',kind:'text',tag:'text',text:'Meeting moved to Thursday 14:00, room 3. Bring the report and the signed forms.',at:now-600,keep:true},
      {id:'c',kind:'image',tag:'image',text:'',w:1920,h:1080,at:now-4000,keep:false,thumb:png,app:'Google Chrome',src:'C:\chrome.exe',ocr:'Invoice 4471\nTotal Rp 250.000'},
      {id:'d',kind:'text',tag:'code',lang:'Rust',text:'fn main() {\n  println!("hi");\n}',at:now-9000,keep:false},
      {id:'e',kind:'text',tag:'color',text:'#4f9dff',at:now-9500,keep:false},
      {id:'f',kind:'link',tag:'link',text:'https://example.com/docs/page',site:'',media:false,at:now-90000,keep:false}];
    const hist=[
      {id:1,cat:'social',app:'WhatsApp',title:'Alex',detail:'halo, jadi ketemu jam berapa?',at:now-120,aumid:'WA!App',reply:'wa:628111@s.whatsapp.net'},
      {id:5,cat:'social',app:'WhatsApp',title:'Alex',detail:'besok kita lanjut ya, bawa laptop',at:now-400},
      {id:6,cat:'social',app:'WhatsApp',title:'Alex',detail:'oke siap',at:now-700},
      {id:2,cat:'social',app:'Discord',title:'#general',detail:'Hey team just wanted to let you know that the new version of app has officially gone live',at:now-3000},
      {id:7,cat:'social',app:'Telegram',title:'Budi',detail:'udah sampe?',at:now-5000,reply:'tg:777'},
      {id:3,cat:'ai',app:'Claude',title:'Revisi UI notifikasi floating',detail:'water reminder on by default; prayer alarms added',at:now-500},
      {id:4,cat:'ai',app:'ChatGPT',title:'Code review',detail:'Here is the summary of the changes.',at:now-7000}];
    window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,a)=>{
      window.__calls.push([name,a]);
      if(name==='board_list')return{capture:true,items};
      if(name==='board_link_preview'){const k=document.createElement('canvas');k.width=600;k.height=315;const g=k.getContext('2d');g.fillStyle='#1d4ed8';g.fillRect(0,0,600,315);g.fillStyle='#fff';g.font='700 64px sans-serif';g.fillText('Me at the zoo',40,170);return{url:a.url,site:'YouTube',title:'Me at the zoo',description:'The first video on YouTube. Maybe it is time to go back to the zoo?',image:k.toDataURL(),kind:/youtube/.test(a.url)?'video':'page'}}
      if(name==='board_image'){const k=document.createElement('canvas');k.width=1920;k.height=1080;const g=k.getContext('2d');g.fillStyle='#10233f';g.fillRect(0,0,1920,1080);g.strokeStyle='#4f9dff';g.lineWidth=2;for(let x=0;x<1920;x+=80){g.beginPath();g.moveTo(x,0);g.lineTo(x,1080);g.stroke()}for(let y=0;y<1080;y+=80){g.beginPath();g.moveTo(0,y);g.lineTo(1920,y);g.stroke()}g.fillStyle='#fff';g.font='700 120px sans-serif';g.fillText('1920 x 1080',560,560);return k.toDataURL()}
      if(name==='board_open_link'||name==='board_open_terminal'||name==='board_put_text'||name==='board_save'||name==='board_share')return name==='board_open_terminal'?'Windows Terminal':name==='board_save'?'x.txt':null;
      if(name==='board_ocr'){const L=(y,h,ws)=>{let x=560;return{text:ws.join(' '),words:ws.map(t=>{const w={text:t,x,y,w:t.length*48,h};x+=t.length*48+36;return w})}};return[L(300,110,['Invoice','4471']),L(470,110,['Total','Rp','250.000']),L(640,90,['Thank','you'])]}
      if(name==='history_list')return{enabled:true,items:hist};
      if(name==='wa_send'){if(a.text==='gagal')throw 'WhatsApp is not connected.';return null}
      if(name==='tg_send')return null;
      if(name==='chat_take_reply'){const p=window.__pending;window.__pending=null;return p||null}
      if(name==='board_app_icon'&&window.__iconFails>0){window.__iconFails--;throw 'not yet'}
      if(name==='app_icon'||name==='board_app_icon')return png;
      if(name==='app_marks')return[{name:'WhatsApp',path:'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20ZM8 8h8v8H8Z',color:0x25d366,fg:0xffffff},{name:'Claude',path:'M12 2 14 10 22 12 14 14 12 22 10 14 2 12 10 10Z',color:0xd97757,fg:0xffffff}];
      if(name==='monitors'||name==='shelf_list')return[];if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}};
  },[now,png]);
  await page.goto('http://127.0.0.1:18774/index.html?panel=1&scale=1&tab=home');await page.addStyleTag({content:'html,body{background:#000!important}'});
  await page.waitForSelector('#home.active');await page.evaluate(()=>window.__emit('panel-show'));await page.waitForTimeout(500);
  const classic=sel=>page.locator(sel+' select, '+sel+' input[type=checkbox], '+sel+' input[type=range], '+sel+' input[type=date]').count();

  // ---- Board
  await page.click('[data-tab=board]');await page.waitForSelector('#board.active');await page.waitForTimeout(400);
  assert.equal(await page.locator('.bd-card').count(),6,'six cards');
  assert.equal(await page.locator('#bd-preview:not([hidden])').count(),0,'no preview until a card is clicked');
  assert.equal(await page.locator('.bd-card.image .bd-foot .bd-dot').count(),1,'no icon yet: the card keeps its dot');
  await page.evaluate(()=>{window.__iconFails=0;window.__emit('board-changed')});await page.waitForTimeout(300);
  assert.ok(await page.evaluate(()=>window.__calls.some(c=>c[0]==='board_app_icon'&&c[1].src)),'the icon is asked by the program file');
  assert.equal(await page.locator('.bd-card.image .bd-app[src^="data:"]').count(),1,'a screenshot card shows the icon of the program it came from');
  assert.equal(await page.locator('.bd-card.link.og img').count(),2,'link cards show their page picture without a click');
  await page.evaluate(()=>{window.__items.push({id:'g',kind:'link',tag:'link',text:'budi.design',at:Math.floor(Date.now()/1000)-100,keep:false});window.__emit('board-changed')});await page.waitForTimeout(300);
  assert.equal(await page.evaluate(()=>window.__calls.some(c=>c[0]==='board_link_preview'&&c[1].url==='budi.design')),false,'a link without https:// waits for a click');
  await page.locator('.bd-card[data-id="g"]').click();await page.waitForTimeout(200);
  await page.locator('#bd-preview .pv-load').click();await page.waitForTimeout(300);
  assert.ok(await page.evaluate(()=>window.__calls.some(c=>c[0]==='board_link_preview'&&c[1].url==='budi.design')),'a click loads it');
  await page.keyboard.press('Escape');await page.evaluate(()=>{window.__items.pop();window.__emit('board-changed')});await page.waitForTimeout(300);
  const size=()=>page.locator('.bd-card.image').evaluate(e=>{const r=e.getBoundingClientRect();return [r.left,r.top,r.width,r.height].join()}),before=await size();
  await page.locator('.bd-card.image').first().click();await page.waitForTimeout(300);
  assert.equal(await size(),before,'cards keep their size and place while the preview is open');
  assert.equal(await page.locator('.bd-card.image .bd-kind').textContent(),'Google Chrome','the card names where it came from');
  assert.equal(await page.locator('.bd-card.image.sel').evaluate(e=>getComputedStyle(e,'::after').boxShadow),'rgb(79, 157, 255) 0px 0px 0px 2px inset','a picked screenshot gets a blue border');
  await page.screenshot({path:path.join(artifacts,'features-board-image-sel.png')});
  assert.ok(await page.evaluate(()=>{const c=[...document.querySelectorAll('.pv-head>.pv-chip')].pop().getBoundingClientRect(),t=document.querySelector('.pv-tools').getBoundingClientRect();return c.right<=t.left+0.5&&t.height<34}),'the preview head shortens its chips, not its buttons');
  await page.locator('.bd-card.image').click();await page.waitForTimeout(300);
  assert.equal(await page.locator('#bd-preview:not([hidden])').count(),0,'a second click on the card on show hides its preview');
  const copies=()=>page.evaluate(()=>window.__calls.filter(c=>c[0]==='board_copy').length),k0=await copies();
  await page.locator('.bd-card.tag-text').dblclick();await page.waitForTimeout(300);
  assert.equal(await copies(),k0,'a card never copies on a click, only Copy in the preview does');assert.equal(await page.locator('.bd-card.sel').count(),0,'two fast clicks open and hide it');
  // spam clicks: each one counts, and the picture is read from disk once however often it opens
  const reads=()=>page.evaluate(()=>window.__calls.filter(c=>c[0]==='board_image').length),r0=await reads();
  for(let i=0;i<7;i++)await page.locator('.bd-card.image').click({delay:0});await page.waitForTimeout(400);
  assert.equal(await page.locator('.bd-card.image.sel').count(),1,'seven fast clicks leave it open');assert.equal(await reads(),r0,'the picture is not read again');
  assert.equal(await page.evaluate(()=>document.getElementById('notch').dataset.view),'board-preview');
  for(let i=0;i<5;i++)await page.locator('.bd-card.image').click({delay:0});await page.waitForTimeout(400);
  assert.deepEqual(await page.evaluate(()=>{const c=[...window.__calls].reverse().find(c=>c[0]==='panel_view'||c[0]==='panel_tall');return[c[0],c[1].name]}),['panel_view','board'],'closed by fast clicks, the panel ends at the shelf height');
  await page.locator('.bd-card.image').click();await page.waitForTimeout(300);
  await page.keyboard.press('Escape');await page.waitForTimeout(300);
  assert.equal(await page.locator('.bd-card.kept').count(),1,'one kept');
  await page.screenshot({path:path.join(artifacts,'features-board.png')});
  await page.click('#bd-filters .fx-seg:nth-child(4)');assert.equal(await page.locator('.bd-card').count(),2,'links only');
  await page.click('#bd-filters .fx-seg:nth-child(3)');assert.equal(await page.locator('.bd-card').count(),1,'code only');assert.equal(await page.locator('.bd-card.tag-code').count(),1);
  await page.click('#bd-filters .fx-seg:nth-child(1)');
  // right-click on a link: Copy, Keep, Remove
  await page.locator('.bd-card.link').first().click({button:'right'});
  const labels=await page.locator('.fx-menu button').allTextContents();
  assert.deepEqual(labels,['Copy','Preview','Save','Keep','Remove'],'menu: '+labels+' (Share is for pictures only)');
  await page.screenshot({path:path.join(artifacts,'features-board-menu.png')});
  await page.locator('.fx-menu button:has-text("Preview")').click();await page.waitForTimeout(300);
  assert.equal(await page.evaluate(()=>document.getElementById('notch').dataset.view),'board-preview','preview asks for the tall panel');
  assert.equal(await page.locator('#bd-preview:not([hidden])').count(),1,'preview shown');
  await page.screenshot({path:path.join(artifacts,'features-board-preview-link.png')});
  for(const k of ['text','color','image','code']){const n=await page.locator('.bd-card.'+(k==='text'?'tag-text':k==='code'?'tag-code':k==='color'?'tag-color':'image')).count();if(n){await page.locator('.bd-card.'+(k==='text'?'tag-text':k==='code'?'tag-code':k==='color'?'tag-color':'image')).first().focus();await page.keyboard.press('Space');await page.waitForTimeout(250);await page.screenshot({path:path.join(artifacts,'features-board-preview-'+k+'.png')})}}
  assert.equal(await classic('#board'),0);
  // a link shows its page and has a browser button; code names its language and has a terminal button; a picture zooms with the wheel
  await page.locator('.bd-card.link').first().click();await page.waitForSelector('#bd-preview .pv-link.rich');
  assert.equal(await page.locator('#bd-preview .pv-og').count(),1,'the page picture');assert.match(await page.locator('#bd-preview .pv-ogtext').textContent(),/Me at the zoo/);
  assert.equal(await page.locator('#bd-preview [aria-label="Open in browser"]').count(),1);
  assert.equal(await page.locator('.bd-card.link.og .bd-play').count(),1,'only the video link card gets a play badge');
  const opens=()=>page.evaluate(()=>window.__calls.filter(c=>c[0]==='board_open_link').length),o0=await opens();
  await page.locator('#bd-preview .pv-play').click();assert.equal(await opens(),o0+1,'the video picture plays it in the browser');
  await page.locator('#bd-preview [aria-label="Open in browser"]').click();assert.ok(await page.evaluate(()=>window.__calls.some(c=>c[0]==='board_open_link')),'open link');
  await page.waitForTimeout(300);await page.screenshot({path:path.join(artifacts,'features-board-preview-linkpage.png')});
  await page.locator('.bd-card.tag-code').first().click();await page.waitForTimeout(250);
  assert.match(await page.locator('#bd-preview .pv-head').textContent(),/Rust/);
  assert.equal(await page.locator('#bd-preview .pv-term').count(),1,'code is shown as in a terminal');assert.equal(await page.locator('#bd-preview .pv-btn:has-text("Share")').count(),0,'no Share on code');
  await page.locator('#bd-preview [aria-label="Open a terminal"]').click();await page.waitForTimeout(200);
  assert.ok(await page.evaluate(()=>window.__calls.some(c=>c[0]==='board_open_terminal')),'open terminal');
  assert.ok(await page.evaluate(()=>window.__calls.some(c=>c[0]==='board_put_text')),'the code is copied first');
  await page.screenshot({path:path.join(artifacts,'features-board-preview-code-term.png')});
  await page.locator('.bd-card.image').first().click();await page.waitForTimeout(500);
  assert.equal((await page.locator('#bd-preview .pv-zoomchip').textContent()).trim(),'Scroll to zoom');
  const box=await page.locator('#bd-preview .pv-image').boundingBox();await page.mouse.move(box.x+box.width/2,box.y+Math.min(box.height/2,60));
  for(let i=0;i<6;i++){await page.mouse.wheel(0,-120);await page.waitForTimeout(40)}
  await page.waitForTimeout(200);assert.match(await page.locator('#bd-preview .pv-zoomchip').textContent(),/^\d+%$/,'zoomed');
  await page.screenshot({path:path.join(artifacts,'features-board-preview-zoom.png')});
  await page.locator('#bd-preview .pv-image').click({position:{x:200,y:40}});await page.locator('#bd-preview .pv-image').click({position:{x:200,y:40}});await page.locator('#bd-preview .pv-image').click({position:{x:200,y:40}});await page.waitForTimeout(150);
  assert.match(await page.locator('#bd-preview .pv-zoomchip').textContent(),/^\d+%$/,'clicking does not undo a zoom');
  assert.equal(await page.locator('#bd-preview [aria-label="Share"], #bd-preview .pv-btn:has-text("Share")').count(),1,'a picture can be shared');
  // Scan text: every word found gets a box; the user picks any amount (drag across words and lines, Ctrl+click to add one)
  await page.setViewportSize({width:760,height:900});{const z=await page.locator('#bd-preview .pv-image').boundingBox();await page.mouse.move(z.x+z.width/2,z.y+z.height/2);for(let i=0;i<20;i++){await page.mouse.wheel(0,120);await page.waitForTimeout(20)}await page.waitForTimeout(200);assert.equal((await page.locator('#bd-preview .pv-zoomchip').textContent()).trim(),'Scroll to zoom','back at fit')}
  await page.locator('#bd-preview .pv-btn:has-text("Scan text")').click();await page.waitForTimeout(250);
  assert.equal(await page.locator('#bd-preview .ocr-word').count(),7,'one box per word');
  assert.ok(await page.locator('#bd-preview .pv-ocrbar .pv-btn.copy').isDisabled(),'nothing picked, nothing to copy');
  await page.screenshot({path:path.join(artifacts,'features-board-scan.png')});
  const word=async n=>{const b=await page.locator('#bd-preview .ocr-word').nth(n).boundingBox();return[b.x+b.width/2,b.y+b.height/2]};
  const copied=()=>page.evaluate(()=>[...window.__calls].reverse().find(c=>c[0]==='board_put_text')?.[1].text);
  const [ax,ay]=await word(1),[bx,by]=await word(3);
  await page.mouse.move(ax,ay);await page.mouse.down();await page.mouse.move((ax+bx)/2,(ay+by)/2,{steps:4});await page.mouse.move(bx,by,{steps:4});await page.mouse.up();await page.waitForTimeout(100);
  assert.equal((await page.locator('#bd-preview .pv-ocrnote').textContent()).trim(),'3 of 7 words picked','drag picks a run of words across lines');
  await page.screenshot({path:path.join(artifacts,'features-board-scan-picked.png')});
  await page.locator('#bd-preview .pv-ocrbar .pv-btn.copy').click();await page.waitForTimeout(150);
  assert.equal(await copied(),'4471\nTotal Rp','only the picked words, lines kept');
  const [cx,cy]=await word(6);await page.keyboard.down('Control');await page.mouse.click(cx,cy);await page.keyboard.up('Control');
  await page.locator('#bd-preview .pv-ocrbar .pv-btn.copy').click();await page.waitForTimeout(150);
  assert.equal(await copied(),'4471\nTotal Rp\nyou','Ctrl+click adds one more word');
  await page.locator('#bd-preview .pv-ocrbar .pv-btn:has-text("Select all")').click();await page.locator('#bd-preview .pv-ocrbar .pv-btn.copy').click();await page.waitForTimeout(150);
  assert.equal(await copied(),'Invoice 4471\nTotal Rp 250.000\nThank you','select all copies every word');
  await page.locator('#bd-preview .pv-btn:has-text("Scan text")').click();await page.waitForTimeout(100);
  assert.equal(await page.locator('#bd-preview .ocr-word').count(),0,'scan text again closes it');
  await page.setViewportSize({width:760,height:400});await page.waitForTimeout(150);
  await page.fill('#bd-search','invoice');await page.waitForTimeout(150);
  assert.equal(await page.locator('.bd-card.image').count(),1,'search finds the picture by its text');
  await page.fill('#bd-search','');await page.waitForTimeout(150);
  await page.click('#bd-preview .pv-btn.icon:last-child');await page.waitForTimeout(200);
  assert.equal(await page.evaluate(()=>document.getElementById('notch').dataset.view),'board','closing the preview returns the normal height');
  assert.equal(await page.locator('#bd-preview:not([hidden])').count(),0);
  // Clear asks twice: the first press only arms it ("Confirm?"), the second clears; waiting puts it back
  await page.click('#board-clear');assert.equal(await page.locator('#board-clear.confirming').count(),1,'first press arms');assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='board_clear').length),0,'nothing cleared yet');
  assert.match(await page.locator('#board-clear').evaluate(e=>getComputedStyle(e,'::after').content),/Confirm?/);
  await page.screenshot({path:path.join(artifacts,'features-board-confirm.png')});
  await page.mouse.click(700,300);await page.waitForTimeout(100);assert.equal(await page.locator('#board-clear.confirming').count(),0,'a press elsewhere puts it back');
  await page.click('#board-clear');await page.click('#board-clear');assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='board_clear').length),1,'second press clears');
  assert.equal(await classic('#board'),0);

  // ---- Chat history
  await page.click('#history-button');await page.waitForSelector('#history.active');await page.waitForTimeout(400);
  assert.equal(await page.locator('.hs-g').count(),3,'social side first');
  assert.deepEqual(await page.locator('#hs-apps .fx-chip').allTextContents(),['All','WhatsApp','Discord','Telegram']);
  await page.screenshot({path:path.join(artifacts,'features-history.png')});
  await page.click('#hs-cat .fx-seg:nth-child(2)');
  assert.deepEqual(await page.locator('.hs-g .hs-app').allTextContents(),['Claude','ChatGPT'],'AI side apart from social');
  await page.fill('#hs-search','summary');assert.equal(await page.locator('.hs-g').count(),1,'search');
  assert.equal(await classic('#history'),0);
  await page.screenshot({path:path.join(artifacts,'features-history-ai.png')});
  // Reply to a linked WhatsApp chat: only its newest card has Reply; Enter sends to that chat; an error stays in the box.
  await page.fill('#hs-search','');await page.click('#hs-cat .fx-seg:nth-child(1)');
  assert.equal(await page.locator('.hs-replyto').count(),2,'Reply on the WhatsApp and Telegram cards, not on Discord');
  await page.locator('.hs-replyto').first().click();await page.waitForSelector('.hs-reply input:focus');
  assert.equal(await page.locator('.hs-send').isDisabled(),true,'empty: no send');
  await page.keyboard.type('gagal');await page.keyboard.press('Enter');await page.waitForSelector('.hs-reply-err');
  assert.deepEqual(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='chat_typing').map(c=>c[1])),[{key:'wa:628111@s.whatsapp.net',on:true}],'typing told once, not per key');
  assert.equal(await page.locator('.hs-reply-err').textContent(),'WhatsApp is not connected.');assert.equal(await page.locator('.hs-reply input').inputValue(),'gagal','draft kept');
  await page.screenshot({path:path.join(artifacts,'features-history-reply.png')});
  await page.fill('.hs-reply input','oke otw');await page.click('.hs-send');await page.waitForFunction(()=>!document.querySelector('.hs-reply'));
  const sent=await page.evaluate(()=>window.__calls.filter(c=>c[0]==='wa_send').map(c=>c[1]));
  assert.deepEqual(sent.at(-1),{chat:'628111@s.whatsapp.net',text:'oke otw'});
  await page.evaluate(()=>{window.__pending='wa:628111@s.whatsapp.net';window.__emit('chat-reply','wa:628111@s.whatsapp.net')});await page.waitForSelector('.hs-reply input');
  assert.equal(await page.locator('.hs-reply input').inputValue(),'','a notice opens an empty box');
  await page.keyboard.press('Escape');await page.waitForFunction(()=>!document.querySelector('.hs-reply'));
  // A Telegram chat answers through tg_send, opened the way a click on its notice opens it.
  await page.evaluate(()=>{window.__pending='tg:777';window.__emit('chat-reply','tg:777')});await page.waitForSelector('.hs-reply input:focus');
  await page.keyboard.type('udah');await page.keyboard.press('Enter');await page.waitForFunction(()=>!document.querySelector('.hs-reply'));
  assert.deepEqual(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='tg_send').map(c=>c[1]).at(-1)),{chat:'777',text:'udah'});
  assert.equal(await classic('#history'),0);
  // A clicked AI app card opens the AI side; a social card the Social side.
  await page.evaluate(()=>{window.__pending='cat:ai';window.__emit('chat-reply','cat:ai')});await page.waitForSelector('#hs-cat .fx-seg:nth-child(2).on');
  assert.ok((await page.locator('#hs-list').textContent()).includes('Code review'),'AI side shown');
  await page.evaluate(()=>{window.__pending='cat:social';window.__emit('chat-reply','cat:social')});await page.waitForFunction(()=>!document.querySelector('#hs-list').textContent.includes('Code review'));

  assert.deepEqual(errors,[]);
  await browser.close();server.close();console.log('features UI ok');
})().catch(e=>{console.error(e);process.exit(1)});
