// Settings: AI Agent and Meeting sections, the one-click Meeting package, folds, search. Screenshots: tests/artifacts/settings-*.png
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const server=http.createServer((req,res)=>{const u=req.url.split('?')[0];const name=u==='/'?'index.html':u.slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',name.endsWith('.js')?'text/javascript':name.endsWith('.css')?'text/css':name.endsWith('.svg')?'image/svg+xml':name.endsWith('.woff2')?'font/woff2':'text/html');res.end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18767,'127.0.0.1',r));
  const browser=await chromium.launch({channel:process.env.PW_CHANNEL||'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:900,height:700}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    const L={};window.__calls=[];let desk={shortcut:'Ctrl+Alt+Space',hide_maximized:false,game_mode:'off',edge_handle:true};
    const P=(id,name,group,kind,size)=>({id,name,group,kind,size,detail:'Fast, 4 GB RAM',license:'MIT',url:'',sha256:'',min_ram_gb:1});
    const parts=[P('whisper-engine','Transcript engine','transcript','engine',8928640),P('whisper-vad','Voice detection','transcript','helper',885098),P('whisper-base-q5','Whisper Base','transcript','model',59707625),P('whisper-small','Whisper Small','transcript','model',487601967),P('whisper-turbo','Whisper Turbo','transcript','model',574041195),P('llama-engine','Summary engine','summary','engine',25084375),P('qwen35-0.8b','Qwen 0.8B','summary','model',532517120),P('qwen35-2b','Qwen3.5 2B','summary','model',1280835840),P('qwen35-4b','Qwen 4B','summary','model',2740937888),P('speaker-engine','Voice engine','speakers','engine',25567228),P('speaker-seg','Speech turns','speakers','helper',1540506),P('speaker-voice','Voice separation','speakers','model',26485263)];
    const have=new Set();
    window.__L=L;window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,args={})=>{window.__calls.push({name,args:JSON.parse(JSON.stringify(args))});
      if(name==='desktop_preferences')return{preferences:desk,shortcut_active:true};
      if(name==='set_desktop_preferences'){desk=args.preferences;return{preferences:desk,shortcut_active:true}}
      if(name==='ai_status')return{on:!!desk.feature_local_ai,parts:parts.map(part=>({part,installed:have.has(part.id),progress:(window.__prog||{})[part.id]||null,rtf:null,warn:false})),hardware:{ram_gb:8,free_ram_gb:4,cores:8,avx2:true,disk_free_gb:100,on_battery:false},recommend:{tier:'Sora AI Balanced',transcript:'whisper-small',summary:'qwen35-2b',notes:[]},job:null};
      if(name==='ai_download'){have.add(args.id);return null}
      const cl=window.__cloud||(window.__cloud={on:false,provider:'claude',model:'claude-opus-5-5',url:'https://api.openai.com/v1',has_key:false,ready:false,label:'Claude'});
      const ready=()=>{cl.label={claude:'Claude',openai:'OpenAI'}[cl.provider]||'localhost';cl.ready=cl.on&&!!cl.model&&(cl.provider==='other'?!!cl.url:cl.has_key);return{...cl}};
      if(name==='cloud_ai_get')return ready();
      if(name==='cloud_ai_set'){Object.assign(cl,args.config);return ready()}
      if(name==='cloud_ai_key'){cl.has_key=!!args.key;cl.key=args.key;return ready()}
      if(name==='cloud_ai_models'){if(cl.key==='sk-wrong')throw (cl.provider==='other'?'localhost':'Claude')+' did not accept the API key.';return cl.provider==='other'?['llama3.1:8b','qwen3:4b']:[]}
      if(name==='ai_remove'){have.delete(args.id);return null}
      if(name==='meeting_list')return{folder:'Documents/Sora Recordings',items:[],consent_seen:true};
      if(name==='meeting_scan')return{isolation:true,build:26100,candidates:[]};
      if(name==='agent_hooks')return{claude_code:true};
      if(name==='agent_capabilities')return[];
      if(name==='wa_state')return window.__wa||{state:'unlinked',qr:null,me:'',error:''};
      if(name==='wa_link'||name==='wa_unlink'||name==='notification_access_settings'||name==='tg_submit'||name==='tg_sign_out')return null;
      if(name==='tg_state')return window.__tg||{state:'phone',hint:'',me:'',error:''};
      if(name==='app_marks')return[{name:'WhatsApp',path:'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z',color:0x25d366,fg:0xffffff},{name:'Telegram',path:'M3 11l18-7-3 16-6-5-3 3v-5Z',color:0x26a5e4,fg:0xffffff}];
      if(name==='app_version')return'1.0.0';if(name==='startup_enabled')return false;if(name==='notch_displays')return[];
      return null}}};
  });
  await page.goto('http://127.0.0.1:18767/settings.html');await page.evaluate(()=>document.fonts.ready);
  await page.waitForFunction(()=>!document.querySelector('#maximize-toggle').disabled);
  assert.deepEqual((await page.locator('.tab').allTextContents()).map(t=>t.trim()),['General','Notch','AI Agent','Meeting','Chat']);
  // Chat > Hide message text (all chat apps): off by default, saved as hide_chat_text.
  await page.locator('.tab',{hasText:'Chat'}).click();
  assert.equal(await page.locator('#hide-chat-toggle').isChecked(),false);
  await page.getByRole('switch',{name:'Hide message text'}).check();
  await page.waitForFunction(()=>window.__calls.some(c=>c.name==='set_desktop_preferences'&&c.args.preferences.hide_chat_text===true),null,{timeout:3000});
  await page.locator('#hide-chat-toggle').scrollIntoViewIfNeeded();await page.screenshot({path:path.join(artifacts,'settings-chat-hide-text.png')});
  await page.getByRole('switch',{name:'Hide message text'}).uncheck();
  await page.waitForFunction(()=>window.__calls.filter(c=>c.name==='set_desktop_preferences').pop().args.preferences.hide_chat_text===false,null,{timeout:3000});
  await page.locator('.tab',{hasText:'General'}).click();
  // Meeting and Chat sit under the Beta label and carry the Beta badge; no What's New, the version is in General.
  assert.deepEqual(await page.evaluate(()=>[...document.querySelectorAll('#advanced-label ~ .tab')].map(b=>b.dataset.section)),['meeting','chats']);
  assert.equal(await page.locator('#advanced-label').textContent(),'Beta');assert.equal(await page.locator('#pane-badge').isHidden(),true,'General is not beta');
  assert.equal(await page.locator('#news-open,#panel-whatsnew,#agent-caps').count(),0,'no What is New, no agent table');
  assert.equal(await page.locator('.brand span').textContent(),'Settings');assert.equal(await page.locator('.brand svg').count(),0,'no logo');{const f=await page.locator('.tab-set').evaluateAll(s=>s.map(x=>{const r=x.getBoundingClientRect();return [r.top,r.height].join()}));assert.equal(new Set(f).size,1,'both tab frames are the same size: '+f)}assert.equal(await page.locator('#panel-agent .row').first().locator('small').count(),0,'no Off by default');assert.equal(await page.locator('#labs-telegram .note').count(),0,'Telegram has no note: only the switch and the account');assert.equal(await page.locator('#panel-general #brand-version').textContent(),'v1.0.0');
  assert.equal(await page.locator('#pane-sub').textContent(),'Language, startup and updates.');
  assert.equal(await page.locator('.tab', {hasText:'Labs'}).count(),0,'no Labs left');
  assert.equal(await page.locator('select,input[type=checkbox]:not(.switch),input[type=date]').count(),0,'no classic controls');
  // AI Agent
  await page.locator('.tab[data-section=agent]').click();assert.equal(await page.locator('#pane-title').textContent(),'AI Agent');
  assert.equal(await page.locator('#labs-agents').isHidden(),true,'off until switched on');
  await page.getByRole('switch',{name:'AI Agent'}).check();await page.waitForFunction(()=>window.__calls.some(c=>c.name==='set_desktop_preferences'&&c.args.preferences.feature_agents===true));
  await page.waitForSelector('#labs-agents:visible');assert.equal(await page.locator('#agent-more').isHidden(),true,'extras are folded');
  await page.getByRole('button',{name:'More options'}).click();assert.equal(await page.locator('#agent-more').isVisible(),true);
  await page.screenshot({path:path.join(artifacts,'settings-agent.png')});
  // Meeting + package
  await page.locator('.tab[data-section=meeting]').click();assert.equal(await page.locator('#pane-title').textContent(),'Meeting');
  assert.equal(await page.locator('#labs-meeting').isHidden(),true);
  await page.getByRole('switch',{name:'Meeting recorder'}).check();await page.waitForSelector('#pkgs .pkg-card');
  assert.deepEqual(await page.locator('#pkgs .pkg-card strong').allTextContents(),['Lite','Balanced','Powerful']);
  assert.deepEqual(await page.locator('#pkgs-sum .pkg-card strong').allTextContents(),['Short','Standard','Deep']);
  // Switching the recorder on brings the recommended transcript (Balanced here), not the summary.
  await page.waitForFunction(()=>document.querySelectorAll('#pkgs .pkg-card button')[1].textContent==='Remove');
  assert.deepEqual(await page.evaluate(()=>window.__calls.filter(c=>c.name==='ai_download').map(c=>c.args.id)),['whisper-engine','whisper-vad','speaker-engine','speaker-seg','speaker-voice','whisper-small'],'the voice separation comes with the transcript');
  assert.deepEqual(await page.locator('#pkgs-sum .pkg-card button').allTextContents(),['Install','Install','Install'],'the summary stays optional');
  assert.equal(await page.locator('#pkgs-spk, #pkgs-gpu').count(),0,'only Transcript and Summary to choose');
  assert.equal(await page.getByText('Choose models').count(),0,'no single downloads');assert.equal(await page.locator('#ai-parts').count(),0);
  // The size picked for this PC is labelled; no warnings or download messages inside the cards.
  assert.deepEqual(await page.locator('#pkgs .pkg-card').evaluateAll(c=>c.map(x=>!!x.querySelector('.pkg-rec'))),[false,true,false]);
  assert.deepEqual(await page.locator('#pkgs-sum .pkg-card').evaluateAll(c=>c.map(x=>!!x.querySelector('.pkg-rec'))),[false,true,false]);
  assert.equal(await page.locator('.pkg-warn').count(),0);
  await page.screenshot({path:path.join(artifacts,'settings-meeting.png')});
  await page.locator('#meeting-lang-seg button[data-v=id]').click();await page.waitForTimeout(150);
  assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.name==='set_desktop_preferences').pop().args.preferences.meeting_language),'id','the transcript language is saved');
  assert.equal(await page.locator('#meeting-words').count(),0,'no words-to-know field');
  await page.locator('#pkgs-sum .pkg-card').nth(1).locator('button').click();await page.waitForFunction(()=>document.querySelectorAll('#pkgs-sum .pkg-card button')[1].textContent==='Remove');
  const dl=await page.evaluate(()=>window.__calls.filter(c=>c.name==='ai_download').map(c=>c.args.id));assert.deepEqual(dl,['whisper-engine','whisper-vad','speaker-engine','speaker-seg','speaker-voice','whisper-small','llama-engine','qwen35-2b']);
  await page.waitForFunction(()=>window.__calls.some(c=>c.name==='set_desktop_preferences'&&c.args.preferences.feature_local_ai===true),null,{timeout:3000});
  assert.deepEqual(await page.locator('#pkgs .pkg-card button').allTextContents(),['Install','Remove','Install']);
  await page.locator('#pkgs .pkg-card').nth(0).locator('button').click();await page.waitForFunction(()=>document.querySelectorAll('#pkgs .pkg-card button')[0].textContent==='Remove');
  await page.locator('#pkgs .pkg-card').nth(1).locator('button').click();await page.waitForFunction(()=>document.querySelectorAll('#pkgs .pkg-card button')[1].textContent==='Install');
  const rm=await page.evaluate(()=>window.__calls.filter(c=>c.name==='ai_remove').map(c=>c.args.id));assert.deepEqual(rm,['whisper-small'],'the engine and voice detection stay while Lite is installed');
  await page.screenshot({path:path.join(artifacts,'settings-meeting-ready.png')});
  // Cloud summaries: off until switched on; the key is saved and never shown again.
  assert.equal(await page.locator('#cloud-more').isVisible(),false,'cloud AI is off by default');
  await page.locator('label:has(#cloud-on)').click();await page.waitForSelector('#cloud-more:not([hidden])');
  assert.equal(await page.locator('#cloud-more .note').count(),0,'no warning box');
  assert.equal(await page.locator('#cloud-url-field').isVisible(),false,'Claude needs no address');
  await page.locator('#cloud-provider-seg button[data-v=openai]').click();await page.waitForTimeout(150);
  assert.equal(await page.locator('#cloud-url-field').isVisible(),false,'OpenAI needs only a key, like Claude');
  await page.locator('#cloud-provider-seg button[data-v=other]').click();await page.waitForSelector('#cloud-url-field:not([hidden])');
  await page.fill('#cloud-url','http://localhost:11434/v1');
  // A key the service turns down is marked on the field itself, with no note under the form.
  await page.fill('#cloud-key','sk-wrong');await page.click('#cloud-form button[type=submit]');await page.waitForSelector('#cloud-key.bad');
  assert.equal(await page.getAttribute('#cloud-key','placeholder'),'Key not accepted');
  assert.equal(await page.getAttribute('#cloud-key','aria-invalid'),'true');
  assert.equal(await page.locator('#cloud-status').textContent(),'','still no note under the form');
  await page.locator('#cloud-more').scrollIntoViewIfNeeded();await page.screenshot({path:path.join(artifacts,'settings-cloud-bad-key.png')});
  await page.fill('#cloud-key','sk-test-123');
  await page.click('#cloud-form button[type=submit]');await page.waitForTimeout(300);
  const keyCalls=await page.evaluate(()=>window.__calls.filter(c=>c.name==='cloud_ai_key').map(c=>c.args));
  assert.deepEqual(keyCalls,[{provider:'other',key:'sk-wrong'},{provider:'other',key:'sk-test-123'}],'each key goes to Rust once');
  assert.equal(await page.locator('#cloud-key.bad').count(),0,'a good key clears the mark');
  assert.equal(await page.getAttribute('#cloud-key','placeholder'),'Saved. Type to replace');
  assert.equal(await page.locator('#cloud-model').isVisible(),false,'the model is picked from a menu, not typed');
  // The Model menu lists what the server has (Ollama here); picking one saves it.
  await page.click('#cloud-model-btn');
  assert.deepEqual(await page.locator('#cloud-menu button').allTextContents(),['llama3.1:8b','qwen3:4b','Other…']);
  await page.screenshot({path:path.join(artifacts,'settings-cloud-models.png')});
  await page.click('#cloud-menu button:has-text("qwen3:4b")');await page.waitForFunction(()=>document.getElementById('cloud-model-name').textContent==='qwen3:4b');
  assert.deepEqual((await page.evaluate(()=>window.__calls.filter(c=>c.name==='cloud_ai_set').pop().args)).config,{on:true,provider:'other',model:'qwen3:4b',url:'http://localhost:11434/v1'});
  assert.equal(await page.locator('#cloud-model-name').textContent(),'qwen3:4b');
  assert.equal(await page.locator('#cloud-menu').isVisible(),false,'the menu closes after a pick');
  // Other… types a name the list does not have.
  await page.click('#cloud-model-btn');await page.click('#cloud-menu button:has-text("Other…")');
  assert.equal(await page.locator('#cloud-model').isVisible(),true,'Other… switches to typing');
  assert.equal(await page.inputValue('#cloud-key'),'','the key field is cleared');
  assert.equal(await page.locator('#cloud-status').textContent(),'','no note under the form');
  await page.locator('#cloud-more').scrollIntoViewIfNeeded();await page.screenshot({path:path.join(artifacts,'settings-cloud-ai.png')});
  // A shared engine downloading (Lite reinstalling it here) marks only that package busy, not its neighbours.
  await page.evaluate(()=>{window.__prog={'whisper-small':{state:'downloading',got:120000000,total:487601967},'whisper-engine':{state:'downloading',got:1,total:2}};window.__L['ai-progress']()});
  await page.waitForFunction(()=>document.querySelectorAll('#pkgs .pkg-card button.progress').length===1);
  assert.deepEqual(await page.locator('#pkgs .pkg-card button').evaluateAll(b=>b.map(x=>x.classList.contains('progress'))),[false,true,false],'only Balanced shows progress');
  assert.match(await page.locator('#pkgs .pkg-card button.progress').textContent(),/^Cancel\d+%$/);
  const heights=await page.locator('#pkgs .pkg-card').evaluateAll(c=>c.map(x=>x.offsetHeight));assert.equal(new Set(heights).size,1,'cards keep one height while downloading');
  const spots=await page.locator('#pkgs .pkg-card').evaluateAll(c=>c.map(x=>['.pkg-size','button'].map(s=>x.querySelector(s).offsetTop).join()));assert.equal(new Set(spots).size,1,'size and button sit in the same place on every card');
  await page.evaluate(()=>{window.__prog={};window.__L['ai-progress']()});await page.waitForFunction(()=>!document.querySelector('#pkgs .pkg-card button.progress'));
  // WhatsApp: hidden until switched on; the unofficial-link note shows before Link; the QR draws; linked shows Unlink.
  await page.locator('.tab[data-section=chats]').click();assert.equal(await page.locator('#pane-title').textContent(),'Chat');assert.equal(await page.locator('#pane-badge').isVisible(),true,'Chat is advanced');
  assert.deepEqual((await page.locator('.chat-app b').allTextContents()),['WhatsApp','Telegram'],'no Other apps');
  assert.equal(await page.locator('.chat-app[data-app=whatsapp] .chat-logo svg path').count(),1,'the real WhatsApp mark');
  assert.equal(await page.locator('#chat-telegram').isVisible(),false,'one app at a time');
  assert.equal(await page.locator('#labs-whatsapp').isHidden(),true);
  await page.getByRole('switch',{name:'WhatsApp replies'}).check();await page.waitForSelector('#labs-whatsapp:not([hidden])');
  assert.match(await page.locator('#labs-whatsapp .note.warn').textContent(),/Unofficial WhatsApp/);
  assert.doesNotMatch(await page.locator('#labs-whatsapp .note.warn').textContent(),/Test with a number/);
  assert.equal(await page.locator('#labs-whatsapp .group:has(#wa-link) + .note.warn').count(),1,'the WhatsApp note sits under the Account row');
  await page.waitForFunction(()=>window.__calls.some(c=>c.name==='set_desktop_preferences'&&c.args.preferences.feature_whatsapp===true),null,{timeout:3000});
  assert.equal(await page.locator('#wa-status').textContent(),'Not linked');assert.equal(await page.locator('#wa-qr').isHidden(),true);
  await page.locator('#wa-link').click();await page.waitForFunction(()=>window.__calls.some(c=>c.name==='wa_link'));
  await page.evaluate(()=>{const w=29,m=Array.from({length:w*w},(_,i)=>((i*7)%3===0||i%w<7&&Math.floor(i/w)<7)?'1':'0').join('');window.__wa={state:'qr',qr:{width:w,modules:m},me:'',error:''};window.__L['wa-state']({payload:window.__wa})});
  await page.waitForSelector('#wa-qr:not([hidden])');assert.equal(await page.locator('#wa-link').textContent(),'New code');
  await page.screenshot({path:path.join(artifacts,'settings-whatsapp-qr.png')});
  await page.evaluate(()=>{window.__wa={state:'linked',qr:null,me:'+15551234',error:''};window.__L['wa-state']({payload:window.__wa})});
  assert.equal(await page.locator('#wa-status').textContent(),'Linked as +15551234');assert.equal(await page.locator('#wa-link').textContent(),'Unlink');assert.equal(await page.locator('#wa-qr').isHidden(),true);
  await page.locator('#wa-link').click();await page.waitForFunction(()=>window.__calls.some(c=>c.name==='wa_unlink'));
  await page.screenshot({path:path.join(artifacts,'settings-whatsapp.png')});
  // Telegram: phone, then code (the hint names where it went), then signed in with Sign out.
  await page.locator('.chat-app[data-app=telegram]').click();assert.equal(await page.locator('#chat-telegram').isVisible(),true);assert.equal(await page.locator('#chat-whatsapp').isVisible(),false);
  await page.getByRole('switch',{name:'Telegram replies'}).check();await page.waitForSelector('#tg-login:not([hidden])');
  assert.equal(await page.locator('#tg-label').textContent(),'Phone number');
  await page.locator('#tg-input').fill('+62 812 0000 0000');await page.locator('#tg-next').click();
  await page.waitForFunction(()=>window.__calls.some(c=>c.name==='tg_submit'&&c.args.value==='+62 812 0000 0000'));
  await page.evaluate(()=>{window.__tg={state:'code',hint:'Code sent to your Telegram app',me:'',error:''};window.__L['tg-state']({payload:window.__tg})});
  assert.equal(await page.locator('#tg-label').textContent(),'Code sent to your Telegram app');assert.equal(await page.locator('#tg-input').inputValue(),'','a new step starts empty');
  await page.screenshot({path:path.join(artifacts,'settings-telegram-code.png')});
  await page.evaluate(()=>{window.__tg={state:'password',hint:'Hint: kucing',me:'',error:''};window.__L['tg-state']({payload:window.__tg})});
  assert.equal(await page.locator('#tg-input').getAttribute('type'),'password');
  await page.evaluate(()=>{window.__tg={state:'ready',hint:'',me:'Alex (+1 555)',error:''};window.__L['tg-state']({payload:window.__tg})});
  assert.equal(await page.locator('#tg-status').textContent(),'Signed in as Alex (+1 555)');assert.equal(await page.locator('#tg-login').isHidden(),true);
  assert.equal(await page.locator('#ca-telegram').textContent(),'Signed in');
  await page.locator('#tg-out').click();await page.waitForFunction(()=>window.__calls.some(c=>c.name==='tg_sign_out'));
  await page.screenshot({path:path.join(artifacts,'settings-telegram.png')});
  await page.locator('.chat-app[data-app=whatsapp]').click();
  // Search finds folded rows
  await page.locator('#filter').fill('sounds folder');await page.waitForTimeout(250);assert.equal(await page.locator('.row:visible').count(),1);
  await page.locator('#filter').fill('');
  for(const s of ['general','notch']){await page.locator(`.tab[data-section=${s}]`).click();await page.waitForTimeout(400);await page.screenshot({path:path.join(artifacts,`settings-${s}-new.png`)});}
  // Every row carries a control: no information-only rows in any section.
  for(const s of ['general','notch','agent','meeting','chats']){await page.locator(`.tab[data-section=${s}]`).click();await page.waitForTimeout(300);const bare=await page.locator(`#panel-${s} .row:visible`).evaluateAll(r=>r.filter(x=>!x.querySelector('button,input')).map(x=>x.textContent.trim().slice(0,30)));assert.deepEqual(bare,[],s+' has rows without a control');}
  // Language: Indonesian swaps every label, text set later by the code too, and English comes back exactly.
  await page.locator('.tab[data-section=general]').click();
  await page.locator('#lang-seg button[data-v=id]').click();
  assert.deepEqual((await page.locator('.tab').allTextContents()).map(t=>t.trim()),['Umum','Notch','AI Agent','Meeting','Chat']);
  assert.equal(await page.locator('#advanced-label').textContent(),'Beta');assert.equal(await page.locator('.brand span').textContent(),'Pengaturan');
  assert.equal(await page.locator('#pane-title').textContent(),'Umum');assert.equal(await page.locator('#pane-sub').textContent(),'Bahasa, startup, dan pembaruan.');
  assert.equal(await page.locator('#filter').getAttribute('placeholder'),'Cari');
  assert.deepEqual(await page.locator('#lang-seg button').allTextContents(),['English','Indonesia'],'language names stay native');
  assert.equal(await page.evaluate(()=>localStorage.getItem('sora.lang')),'id');
  await page.locator('.tab[data-section=notch]').click();await page.waitForTimeout(250);
  assert.equal(await page.locator('#position-seg button.on').textContent(),'Tengah');
  await page.getByRole('button',{name:'Rekam pintasan'}).click();await page.keyboard.press('Alt+k');await page.waitForTimeout(50);assert.equal(await page.locator('#desktop-status').innerText(),'Tekan dan tahan Ctrl dulu.','text set later is translated');await page.keyboard.press('Escape');
  await page.locator('.tab[data-section=meeting]').click();await page.waitForTimeout(250);
  assert.deepEqual(await page.locator('#pkgs .pkg-card strong').allTextContents(),['Ringan','Seimbang','Kuat']);
  await page.screenshot({path:path.join(artifacts,'settings-meeting-id.png')});
  await page.locator('.tab[data-section=general]').click();await page.waitForTimeout(300);
  await page.screenshot({path:path.join(artifacts,'settings-general-id.png')});
  // Reload keeps Indonesian.
  await page.reload();await page.waitForFunction(()=>!document.querySelector('#maximize-toggle').disabled);
  assert.equal(await page.locator('#pane-title').textContent(),'Umum','the language is remembered');
  await page.locator('#lang-seg button[data-v=en]').click();
  assert.deepEqual((await page.locator('.tab').allTextContents()).map(t=>t.trim()),['General','Notch','AI Agent','Meeting','Chat']);
  assert.equal(await page.locator('#pane-sub').textContent(),'Language, startup and updates.');assert.equal(await page.locator('#filter').getAttribute('placeholder'),'Search');
  assert.equal(await page.locator('#startup-toggle').getAttribute('aria-label'),'Start with Windows');
  assert.deepEqual(errors,[]);await browser.close();server.close();console.log('settings-ui PASS');
})().catch(e=>{console.error(e);process.exit(1)});
