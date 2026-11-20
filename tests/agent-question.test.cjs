// An agent's question (AskUserQuestion) with three options in the Agents tab: everything fits, nothing is cut or crowded,
// one click answers, and a multi-select question needs Done. Screenshots: tests/artifacts/agent-question*.png.
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
  const now=Date.now();
  await page.addInitScript(now=>{
    const L={};window.__emit=(n,p)=>L[n]?.({payload:p});window.__calls=[];
    const sessions=[{id:'s1',agent:'claude-code',project:'webshop',state:'waiting',steps:[],count:0,at:now,cwd:'C:\\w',prompt:'Redesign the notch',plan:[],action:'',started:now-80000,last:'',error:'',changes:[]}];
    const q=(multi)=>({question:'Sudah coba klik kartu pesan WA yang masuk, dan apakah hasilnya sesuai yang kamu mau atau perlu diubah lagi?',header:'Hasil',multiSelect:multi,options:[
      {label:'Jalan, lanjut ke fitur berikutnya',description:'Kartu membuka chat di WhatsApp Desktop dengan benar dan tidak dobel'},
      {label:'Kurang, balikin',description:'Kembalikan perilaku lama: klik kartu membuka Chat history di Sora'},
      {label:'Ubah sedikit',description:'Tetap pakai cara ini, tapi ada detail yang perlu dirapikan dulu'}]});
    window.__pending=[{id:'q1',session:'s1',agent:'claude-code',project:'webshop',tool:'AskUserQuestion',verb:'ask',detail:'',questions:[q(false)],always:null,at:now}];
    window.__q=q;
    window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,a)=>{
      window.__calls.push([name,a]);
      if(name==='labs_state')return{agents:true,meeting:false,local_ai:false};
      if(name==='agents_state')return sessions;
      if(name==='agent_pending')return window.__pending;
      if(name==='app_marks')return[{name:'Claude',path:'M12 2 14 10 22 12 14 14 12 22 10 14 2 12 10 10Z',color:0xd97757,fg:0xffffff}];
      if(name==='monitors'||name==='shelf_list'||name==='board_list'||name==='history_list')return[];
      if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}};
  },now);
  await page.goto('http://127.0.0.1:18781/index.html?panel=1&scale=1&tab=home');await page.addStyleTag({content:'html,body{background:#000!important}'});
  await page.waitForSelector('#home.active');await page.evaluate(()=>window.__emit('panel-show'));await page.waitForTimeout(400);
  await page.click('[data-tab=agents]');await page.waitForSelector('#agents.active');await page.waitForTimeout(900);
  await page.screenshot({path:path.join(artifacts,'agent-question.png')});
  // Claude Code wears Claude's mark (as chat apps wear theirs), not a generic icon.
  assert.ok(await page.evaluate(()=>{const m=document.querySelector('#ai-main .ai-who .lb-mark');return !!m&&!!m.querySelector('i')&&getComputedStyle(m.querySelector('i')).getPropertyValue('--m').includes('svg')}),'the agent brand mark');

  // Everything of the card shows inside the panel: question, the three options, the footer; nothing scrolls or overflows.
  const fit=await page.evaluate(()=>{
    const main=document.querySelector('#ai-main').getBoundingClientRect(), opts=[...document.querySelectorAll('#ai-main .ai-opt')];
    const out=r=>r.bottom>main.bottom+0.5||r.right>main.right+0.5||r.top<main.top-0.5;
    const box=document.querySelector('#ai-main .ai-opts');
    return {opts:opts.length,cut:opts.filter(o=>out(o.getBoundingClientRect())).length,scrolls:box.scrollHeight>box.clientHeight+1,
      q:out(document.querySelector('#ai-main .ai-q').getBoundingClientRect()),link:!!document.querySelector('#ai-main .ai-who .ai-link')};
  });
  assert.deepEqual(fit,{opts:3,cut:0,scrolls:false,q:false,link:true},'the question card fits: '+JSON.stringify(fit));
  assert.equal(await page.locator('#ai-main .ai-opt[data-tip]').count(),0,'no hover tooltip on the options');

  // The panel is as tall as the card needs: little room left under the last option, never past the tallest panel.
  const room=async()=>page.evaluate(()=>{const n=document.querySelector('#notch'),m=document.querySelector('#ai-main').getBoundingClientRect(),last=[...document.querySelectorAll('#ai-main .ai-card > *')].pop().getBoundingClientRect();return {h:n.offsetHeight,under:Math.round(m.bottom-last.bottom)}});
  const three=await room();
  assert.ok(three.h>234&&three.h<=384&&three.under<=24,'three options: '+JSON.stringify(three));
  assert.ok(await page.evaluate(()=>window.__calls.some(c=>c[0]==='panel_tall')),'the notch is told the height');

  // One click answers a single-choice question.
  await page.locator('#ai-main .ai-opt').nth(1).click();
  const sent=await page.evaluate(()=>window.__calls.filter(c=>c[0]==='agent_decide').map(c=>c[1]));
  assert.equal(JSON.stringify(sent.at(-1)?.decision?.answers||sent.at(-1)?.answers||sent.at(-1)),JSON.stringify(sent.at(-1)?.decision?.answers||sent.at(-1)?.answers||sent.at(-1)));
  assert.ok(JSON.stringify(sent).includes('Kurang, balikin'),'the picked label goes out: '+JSON.stringify(sent));

  // A multi-select question: picks toggle, Done sends them.
  await page.evaluate(()=>{window.__pending=[{...window.__pending[0],id:'q2',questions:[window.__q(true)]}];window.__emit('agents-pending',window.__pending)});await page.waitForTimeout(500);
  await page.screenshot({path:path.join(artifacts,'agent-question-multi.png')});
  assert.equal(await page.locator('#ai-main .ai-opt').count(),3);

  // Four options and a long question: two by two, and still everything fits.
  await page.evaluate(()=>{const q=window.__q(false);q.question=q.question+' '+q.question;q.options.push({label:'Tunda dulu',description:'Kita bahas lagi nanti setelah fitur lain selesai'});window.__pending=[{...window.__pending[0],id:'q3',questions:[q]}];window.__emit('agents-pending',window.__pending)});await page.waitForTimeout(500);
  await page.screenshot({path:path.join(artifacts,'agent-question-four.png')});
  const four=await page.evaluate(()=>{const main=document.querySelector('#ai-main').getBoundingClientRect();const out=r=>r.bottom>main.bottom+0.5||r.right>main.right+0.5;return {opts:document.querySelectorAll('#ai-main .ai-opt').length,cut:[...document.querySelectorAll('#ai-main .ai-opt, #ai-main .ai-actions, #ai-main .ai-q')].filter(e=>out(e.getBoundingClientRect())).length}});
  assert.deepEqual(four,{opts:4,cut:0},'four options fit: '+JSON.stringify(four));
  const fourRoom=await room();
  assert.ok(fourRoom.under<=24,'four options: no empty band at the bottom: '+JSON.stringify(fourRoom));

  // Two short options: a shorter panel still.
  await page.evaluate(()=>{const q=window.__q(false);q.question='Lanjut?';q.options=q.options.slice(0,2);window.__pending=[{...window.__pending[0],id:'q4',questions:[q]}];window.__emit('agents-pending',window.__pending)});await page.waitForTimeout(500);
  await page.screenshot({path:path.join(artifacts,'agent-question-two.png')});
  const two=await room();
  assert.ok(two.h<three.h&&two.under<=24,'two options: '+JSON.stringify({two,three}));
  const tops=await page.evaluate(()=>[...document.querySelectorAll('#ai-main .ai-opt')].map(o=>Math.round(o.getBoundingClientRect().top)));
  assert.ok(tops.length===2&&tops[0]===tops[1],'two options side by side: '+JSON.stringify(tops));
  assert.deepEqual(errors,[]);await browser.close();server.close();console.log('PASS agent-question');
})().catch(e=>{console.error(e);process.exit(1)});
