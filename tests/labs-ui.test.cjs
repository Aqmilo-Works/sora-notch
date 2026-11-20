// Labs tabs (Agents, Meeting) in the panel page with a fake Tauri backend: tabs stay hidden until their module is
// on, an approval needs a click on its card (never a timeout), a browser meeting and "All PC audio" ask first,
// and no classic control is used. Screenshots: tests/artifacts/labs-*.png.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const type=n=>n.endsWith('.js')?'text/javascript':n.endsWith('.css')?'text/css':n.endsWith('.svg')?'image/svg+xml':n.endsWith('.woff2')?'font/woff2':'text/html';
const server=http.createServer((req,res)=>{const name=req.url.split('?')[0]==='/'?'index.html':req.url.split('?')[0].slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404).end();return}res.writeHead(200,{'content-type':type(name)}).end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18779,'127.0.0.1',r));
  const browser=await chromium.launch({channel:process.env.PW_CHANNEL||'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:760,height:430},deviceScaleFactor:2});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  const now=Date.now();
  await page.addInitScript(now=>{
    const L={};window.__emit=(n,p)=>L[n]?.({payload:p});window.__calls=[];
    window.__labs={agents:false,meeting:false,local_ai:false};
    const sessions=[{id:'s1',agent:'claude-code',project:'korus',state:'working',steps:['Edits · billing.ts +12 −3','Runs · npm test'],count:2,at:now,cwd:'C:\\w\\korus',prompt:'Fix the billing chart',plan:[{id:'1',text:'Fix chart',status:'completed'},{id:'2',text:'Run tests',status:'in_progress'}],action:'Runs · npm test',started:now-80000,last:'',error:'',
      changes:[{path:'C:\\w\\korus\\billing.ts',file:'billing.ts',added:12,removed:3,preview:['-const x = 1;','+const x = 2;'],hidden:false,at:now},{path:'C:\\w\\korus\\.env',file:'.env',added:1,removed:0,preview:[],hidden:true,at:now}]}];
    const pending=[{id:'p1',session:'s1',agent:'claude-code',project:'korus',tool:'Bash',verb:'run a command',detail:'rm -rf build && npm run build',questions:[],always:null,at:now}];
    const cands=[{pid:100,provider:'Zoom',title:'Zoom Meeting',exe:'zoom.exe',confidence:'high',score:5,reasons:['App is running','Call window: Zoom Meeting','Playing sound now'],isolated:true,browser:false},
      {pid:200,provider:'Chrome',title:'Meet - Weekly sync',exe:'chrome.exe',confidence:'medium',score:3,reasons:['App is running','Call window: Meet - Weekly sync'],isolated:false,browser:true}];
    const items=[{meta:{id:'2026-11-12 10-00 Zoom',title:'Zoom Meeting',provider:'Zoom',started:now-3600000,duration_s:1830,mode:'app',isolated:true,mic:false,recovered:false,note:''},size:350e6,mic:false,transcript:true,summary:true}];
    const transcript={id:'2026-11-12 10-00 Zoom',model:'whisper-base',language:'id',created:now,audio_s:1830,processing_s:400,edited:false,segments:[{id:'seg-1',start:0,end:4,text:'Selamat pagi semua.',speaker:'p1'},{id:'seg-2',start:5,end:9,text:'Budi kirim laporan hari Jumat.',speaker:'p2'},{id:'seg-3',start:10,end:14,text:'Kita sepakat rilis versi dua.',speaker:'me'},{id:'seg-4',start:15,end:18,text:'Oke.'}]};
    const summary={summary:{overview:'Rapat mingguan.',topics:[{title:'Rilis',supportingSegmentIds:['seg-3']}],decisions:[{text:'Rilis versi dua',supportingSegmentIds:['seg-3'],confidence:'medium'}],actionItems:[{task:'Kirim laporan',owner:'Budi',deadline:'Jumat',supportingSegmentIds:['seg-2']}],openQuestions:[],warnings:['AI summary may contain mistakes; verify with the transcript.']}};
    window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,a)=>{
      window.__calls.push([name,a]);
      if(name==='labs_state')return window.__labs;
      if(name==='agents_state')return sessions;
      if(name==='agent_pending')return pending;
      if(name==='meeting_scan')return{candidates:cands,pick:100,build:26300,isolation:true,recording:false};
      if(name==='meeting_list')return{items,folder:'C:\\Users\\x\\Documents\\Sora Recordings',consent_seen:false};
      if(name==='meeting_state')return window.__rec?{...window.__rec,seconds:(window.__rec.seconds+=0.15),level:Math.random()*0.5}:{state:'idle'};
      if(name==='meeting_start'){window.__rec={state:'recording',meta:{provider:a.req.provider,isolated:a.req.mode==='app',mic:a.req.mic},seconds:3,level:0.3,mic_level:0,warning:''};return window.__rec}
      if(name==='ai_transcript')return{transcript,summary};
      if(name==='ai_speaker_name')return null;
      if(name==='ai_export_summary')return 'C:\\x\\m summary.md';
      if(name==='ai_copy_summary')return null;
      // 20 s of silence as a 16 kHz WAV; this recording has no microphone track.
      if(name==='meeting_audio'){if(a.mic)throw'No microphone track.';const n=16000*20,b=new ArrayBuffer(44+n*2),v=new DataView(b),w=(o,t)=>[...t].forEach((c,i)=>v.setUint8(o+i,c.charCodeAt(0)));w(0,'RIFF');v.setUint32(4,36+n*2,true);w(8,'WAVEfmt ');v.setUint32(16,16,true);v.setUint16(20,1,true);v.setUint16(22,1,true);v.setUint32(24,16000,true);v.setUint32(28,32000,true);v.setUint16(32,2,true);v.setUint16(34,16,true);w(36,'data');v.setUint32(40,n*2,true);return b}
      if(name==='ai_status')return{on:true,parts:[{part:{id:'whisper-engine',group:'transcript',kind:'engine'},installed:true},{part:{id:'whisper-base',group:'transcript',kind:'model'},installed:true}],recommend:{transcript:'whisper-base',summary:null},job:null,cloud:{on:true,ready:true,label:'Claude',provider:'claude'}};
      if(name==='cloud_ai_summarize')return null;
      if(name==='monitors'||name==='shelf_list'||name==='board_list'||name==='history_list'||name==='app_marks')return[];
      if(name==='desktop_preferences')return{preferences:{},shortcut_active:true};return null}}};
  },now);
  await page.goto('http://127.0.0.1:18779/index.html?panel=1&scale=1&tab=home');await page.addStyleTag({content:'html,body{background:#000!important}'});
  await page.waitForSelector('#home.active');await page.evaluate(()=>window.__emit('panel-show'));await page.waitForTimeout(500);
  const classic=sel=>page.locator(sel+' select, '+sel+' input[type=checkbox], '+sel+' input[type=range], '+sel+' input[type=date]').count();

  // ---- off by default: no tab
  for(const t of ['agents','meeting'])assert.equal(await page.locator(`nav [data-tab=${t}]`).isVisible(),false,t+' hidden while off');
  await page.evaluate(()=>window.__emit('labs-changed',{agents:true,meeting:true,local_ai:true}));await page.waitForTimeout(200);
  for(const t of ['agents','meeting'])assert.equal(await page.locator(`nav [data-tab=${t}]`).isVisible(),true,t+' shown once on');

  // ---- Agents: the approval card; Allow is a click on it, nothing goes out on its own
  await page.click('[data-tab=agents]');await page.waitForSelector('#agents.active');await page.waitForTimeout(900);
  assert.match(await page.locator('#ai-main').textContent(),/wants to run a command/);
  assert.match(await page.locator('#ai-main .ai-code').textContent(),/rm -rf build/);
  assert.ok(await page.evaluate(()=>window.__calls.some(c=>c[0]==='agent_ack'&&c[1].id==='p1')),'the card says it is on screen');
  await page.waitForTimeout(1500);
  assert.ok(!(await page.evaluate(()=>window.__calls.some(c=>c[0]==='agent_decide'))),'no decision without a click');
  assert.doesNotMatch(await page.locator('#ai-side').textContent(),/files? changed/,'the right column shows usage, not changed files');
  assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='panel_view').pop()[1].name),'agents','normal panel height');
  await page.screenshot({path:path.join(artifacts,'labs-agents.png')});
  await page.click('#ai-main button:has-text("Allow")');
  const d=await page.evaluate(()=>window.__calls.find(c=>c[0]==='agent_decide'));
  assert.deepEqual(d[1],{id:'p1',decision:'allow'},'the exact request is answered');
  assert.equal(await classic('#agents'),0);

  // ---- Meeting: one sure meeting is picked; a browser meeting asks first; normal height until a recording is opened
  await page.click('[data-tab=meeting]');await page.waitForSelector('#meeting.active');await page.waitForTimeout(700);
  assert.equal(await page.locator('.lb-chip.on').textContent(),'Zoom','the sure meeting is picked by itself');
  assert.equal(await page.locator('#meeting [aria-label^="Microphone"]').getAttribute('aria-pressed'),'false','mic off by default');
  assert.match(await page.locator('#meeting').textContent(),/Let everyone know you record/,'consent reminder before the first recording');
  assert.equal(await page.evaluate(()=>document.getElementById('notch').dataset.view),'meeting');
  await page.screenshot({path:path.join(artifacts,'labs-meeting.png')});
  await page.click('.lb-chip:has-text("Chrome")');await page.click('#meeting .lb-rec');
  assert.match(await page.locator('#meeting').textContent(),/All of Chrome's sound/,'a browser is never called isolated');
  assert.ok(!(await page.evaluate(()=>window.__calls.some(c=>c[0]==='meeting_start'))),'nothing starts before the confirmation');
  await page.click('#meeting button:has-text("Cancel")');
  await page.click('.lb-chip:has-text("Zoom")');await page.click('#meeting .lb-rec');await page.waitForTimeout(300);
  const st=await page.evaluate(()=>window.__calls.find(c=>c[0]==='meeting_start'));
  assert.deepEqual([st[1].req.mode,st[1].req.mic,st[1].req.broad_ok,st[1].req.pid],['app',false,false,100]);
  assert.equal(await page.locator('#meeting .lb-wave i').count()>20,true,'a live waveform while recording');assert.equal(await page.locator('#meeting [aria-label="Stop recording"]').count(),1);await page.waitForTimeout(700);
  await page.screenshot({path:path.join(artifacts,'labs-meeting-recording.png')});
  await page.click('.lb-item');await page.waitForTimeout(500);
  assert.equal(await page.evaluate(()=>document.getElementById('notch').dataset.view),'meeting-detail','reading opens the tall panel');
  assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='panel_view').pop()[1].name),'meeting-detail');
  assert.match(await page.locator('.lb-sum').textContent(),/Made by AI on this PC/);
  assert.deepEqual(await page.locator('.lb-sec.todo .lb-pill').allTextContents(),['Budi','Jumat'],'who and when show as their own pills');
  assert.deepEqual(await page.locator('.lb-sec-head > span:not(.lb-glyph):not(.lb-count)').allTextContents(),['Decisions','To do','Topics'],'one section per kind of point, in one column');
  await page.screenshot({path:path.join(artifacts,'labs-meeting-detail.png')});
  await page.click('.lb-switch .fx-seg:has-text("Transcript")');
  assert.deepEqual(await page.locator('.lb-spk').allTextContents(),['Person 1','Person 2','You'],'the speaker shows when it changes; the call side by voice');
  // A speaker's name renames everywhere in the transcript; You stays You.
  await page.click('.lb-spk:has-text("Person 1")');await page.keyboard.type('Budi');await page.keyboard.press('Enter');await page.waitForTimeout(200);
  assert.deepEqual(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='ai_speaker_name').map(c=>c[1])),[{id:'2026-11-12 10-00 Zoom',speaker:'p1',name:'Budi'}],'the new name is saved');
  assert.deepEqual(await page.locator('.lb-spk').allTextContents(),['Budi','Person 2','You'],'the name shows on its lines');
  assert.equal(await page.locator('.lb-turn[data-who="me"] .lb-spk[role=button]').count(),0,'You is not renamed');
  assert.equal(await page.locator('.lb-turn').count(),4,'the lines are grouped by turn');
  await page.screenshot({path:path.join(artifacts,'labs-meeting-speakers.png')});
  // A line's time plays the recording from there; the lit line follows the sound; a second click pauses.
  await page.click('.lb-seg[data-id="seg-2"] .lb-ts');await page.waitForTimeout(600);
  assert.deepEqual(await page.locator('.lb-seg.playing').evaluateAll(r=>r.map(x=>x.dataset.id)),['seg-2'],'the line being heard is lit');
  await page.screenshot({path:path.join(artifacts,'labs-meeting-playing.png')});
  assert.deepEqual(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='meeting_audio').map(c=>c[1].mic)).then(m=>m.sort()),[false,true],'both tracks asked for once');
  await page.waitForTimeout(5200);
  assert.deepEqual(await page.locator('.lb-seg.playing').evaluateAll(r=>r.map(x=>x.dataset.id)),['seg-3'],'the lit line follows the sound');
  await page.click('.lb-seg[data-id="seg-3"] .lb-ts');await page.waitForTimeout(200);
  assert.equal(await page.locator('.lb-seg.playing').count(),0,'a second click pauses');
  await page.fill('.lb-search','you');await page.waitForTimeout(100);
  assert.equal(await page.locator('.lb-seg:visible').count(),0,'search reads what was said, not the names');
  await page.fill('.lb-search','laporan');await page.waitForTimeout(100);
  assert.equal(await page.locator('.lb-seg:visible').count(),1,'search finds the line');
  await page.click('#meeting [aria-label="More"]');
  assert.deepEqual(await page.locator('.fx-menu button').allTextContents(),['Copy transcript','Copy for an AI chat','Save as Markdown','Transcribe again','Show in folder','Delete'],'the menu follows the Transcript tab');
  await page.keyboard.press('Escape');
  await page.click('.lb-switch .fx-seg:has-text("Summary")');await page.waitForTimeout(200);
  await page.click('#meeting [aria-label="More"]');
  assert.deepEqual(await page.locator('.fx-menu button').allTextContents(),['Copy summary','Copy for an AI chat','Save as Markdown','Summarize again with Claude','Show in folder','Delete'],'the menu follows the Summary tab');
  await page.click('.fx-menu button:has-text("Copy summary")');await page.waitForTimeout(150);
  assert.ok(await page.evaluate(()=>window.__calls.some(c=>c[0]==='ai_copy_summary')),'the summary copies');
  await page.click('#meeting [aria-label="More"]');
  await page.click('.fx-menu button:has-text("Save as Markdown")');await page.waitForTimeout(200);
  assert.deepEqual(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='ai_export_summary').map(c=>c[1])),[{id:'2026-11-12 10-00 Zoom',format:'md'}],'the summary saves as a file');
  await page.click('#meeting [aria-label="More"]');
  await page.click('.fx-menu button:has-text("Summarize again with Claude")');await page.waitForTimeout(300);
  assert.deepEqual(await page.evaluate(()=>window.__calls.filter(c=>c[0]==='cloud_ai_summarize').map(c=>c[1])),[{id:'2026-11-12 10-00 Zoom'}],'the cloud summary is one click, named by its service');
  await page.click('#meeting [aria-label="More"]');
  await page.keyboard.press('Escape');
  await page.click('#meeting [aria-label="Back"]');await page.waitForTimeout(200);
  assert.equal(await page.evaluate(()=>document.getElementById('notch').dataset.view),'meeting','back to the normal height');
  assert.equal(await classic('#meeting'),0);

  assert.deepEqual(errors,[],'no page errors');
  console.log('labs-ui PASS');
  await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
