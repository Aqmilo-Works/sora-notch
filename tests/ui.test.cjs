const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
const root=path.resolve(__dirname,'../ui');
const server=http.createServer((req,res)=>{if(req.url==='/mascot-test'){res.setHeader('Content-Type','text/html');return res.end('<html></html>')}const name=req.url==='/'?'index.html':req.url.slice(1);const file=path.join(root,name);if(!file.startsWith(root)||!fs.existsSync(file)){res.writeHead(404);return res.end()}res.setHeader('Content-Type',name.endsWith('.js')?'text/javascript':name.endsWith('.css')?'text/css':name.endsWith('.svg')?'image/svg+xml':name.endsWith('.woff2')?'font/woff2':'text/html');res.end(fs.readFileSync(file))});
(async()=>{
  await new Promise(r=>server.listen(18766,'127.0.0.1',r));
  const browser=await chromium.launch({channel:'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:1000,height:380},deviceScaleFactor:1});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    const listeners={},calls=[];let shelf=[],startup=false,desktop={shortcut:"Ctrl+Alt+Space",hide_maximized:false,game_mode:"off"};window.__calls=calls;window.__emit=(name,payload)=>listeners[name]?.({payload});
    window.__TAURI__={event:{listen:async(name,fn)=>{listeners[name]=fn;return()=>{}}},core:{invoke:async(name,args={})=>{
      calls.push({name,args});
      if(name==='set_shortcut_capture')return null;
      if(name==='sync_windows_focus')return window.__focusDenied&&args.endTime?{active:false,message:'Windows did not allow automatic Focus. Start Focus from the Windows calendar.'}:{active:!!args.endTime,message:args.endTime?'Windows Focus is active.':''};
      if(name==='desktop_preferences')return {preferences:desktop,shortcut_active:desktop.shortcut!=='None'};if(name==='set_desktop_preferences'){if(args.preferences.shortcut==='Ctrl+Alt+X')throw Error('Shortcut in use');desktop=args.preferences;return {preferences:desktop,shortcut_active:desktop.shortcut!=='None'}}
      if(name==='check_update')return window.__update||{configured:true,available:true,current:'1.0.0',latest:'1.0.1',size:47400000};if(name==='apply_update'){window.__applied=true;return null}if(name==='app_version')return '1.0.0';if(name==='startup_enabled')return startup;if(name==='set_startup'){startup=args.enabled;return startup;}
      if(name==='set_panel'){listeners['view-scale']?.({payload:args.scale});return null}
      if(name==='auto_location')return {latitude:-6.2251,longitude:106.9001,accuracy:35,source:'device'};
      if(name==='place_lookup')return args.latitude<-6.5?{name:'Kebon Pisang',area:'Sumur Bandung, Bandung',parts:[{label:'Kelurahan/Desa',name:'Kebon Pisang'},{label:'Kecamatan',name:'Sumur Bandung'},{label:'Kota',name:'Bandung'},{label:'Negara',name:'Indonesia'}]}:{name:'Pondok Bambu',area:'Duren Sawit, Jakarta Timur',parts:[{label:'Kelurahan',name:'Pondok Bambu'},{label:'Kecamatan',name:'Duren Sawit'},{label:'Kota',name:'Jakarta Timur'},{label:'Provinsi',name:'Jakarta'}]};
      if(name==='place_search')return args.query==='Bandung'?[{name:'Bandung',detail:'Jawa Barat, Indonesia',latitude:-6.91,longitude:107.61},{name:'Kebon Pisang',detail:'Sumur Bandung, Bandung, Jawa Barat',latitude:-6.917,longitude:107.62}]:[];
      if(name==='file_thumbnail'){
        if(!args.path.endsWith('proposal.pdf'))throw Error('no preview');
        const canvas=document.createElement('canvas');canvas.width=72;canvas.height=128;
        const brush=canvas.getContext('2d');brush.fillStyle='#457487';brush.fillRect(0,0,72,128);return canvas.toDataURL();
      }
      if(name==='weather')return{current:{temperature_2m:27.2,apparent_temperature:29.4,relative_humidity_2m:72,weather_code:0,is_day:1}};
      if(name==='monitors')return['Display 1 · 1920 × 1080'];
      if(name==='city_search')return{results:[{name:'Bandung',admin1:'Jawa Barat',latitude:-6.9,longitude:107.6}]};
      if(name==='media_status')return window.__testMedia||null;
      if(name==='online_lyrics')return window.__testLyrics||{syncedLyrics:'[00:00]An original line for testing\n[00:03]A longer original sentence that should wrap into two neat lines\n[00:07]The next original line\n[00:11]One more test line\n[00:15]Final test line'};
      if(name==='shelf_list')return window.__shelf||shelf;
      if(name==='shelf_pick')return shelf=[{path:'C:\\sample\\proposal.pdf',name:'proposal.pdf',size:123000,directory:false,available:true},{path:'C:\\sample\\<img>.txt',name:'<img>.txt',size:128,directory:false,available:true}];
      if(name==='shelf_remove'){shelf=args.path?shelf.filter(f=>f.path!==args.path):[];return shelf}
      if(name==='lyrics_pick')return'[00:00.00]An original test line\n[00:03.50]Second test line';
      return null;
    }}};
  });


  await page.goto('http://127.0.0.1:18766');
  await page.getByRole('button',{name:'Open Sora',exact:true}).click();await page.waitForTimeout(400);await page.evaluate(()=>document.fonts.ready);
  await page.mouse.move(950,350);await page.waitForTimeout(4300);assert.equal(await page.locator('#notch').evaluate(e=>e.classList.contains('compact')),false);
  assert.equal(await page.locator('html').getAttribute('lang'),'en');
  assert.match(await page.locator('body').evaluate(e=>getComputedStyle(e).fontFamily),/Poppins/);
  await page.evaluate(()=>window.__emit('panel-peek'));await page.waitForSelector('#notch.compact');assert.equal(await page.locator('#expanded').evaluate(e=>e.inert),true);await page.getByRole('button',{name:'Open Sora',exact:true}).click();await page.waitForTimeout(350);
  await page.evaluate(()=>window.__emit('panel-peek'));await page.waitForTimeout(70);
  const entering=await page.locator('#notch').evaluate(e=>({y:e.getBoundingClientRect().top,h:e.getBoundingClientRect().height,opacity:Number(getComputedStyle(e).opacity)}));assert(entering.y<0&&entering.y>-entering.h*1.2);assert(entering.opacity>0&&entering.opacity<1);
  await page.waitForTimeout(450);assert(Math.abs(await page.locator('#notch').evaluate(e=>e.getBoundingClientRect().top))<1);
  await page.evaluate(()=>{window.__emit('panel-visibility',{visible:false,token:101});});await page.waitForTimeout(100);assert.equal(await page.evaluate(()=>window.__calls.some(c=>c.name==='finish_panel_hide'&&c.args.token===101)),false);
  await page.waitForTimeout(260);assert(await page.evaluate(()=>window.__calls.some(c=>c.name==='finish_panel_hide'&&c.args.token===101)));
  await page.evaluate(()=>{window.__emit('panel-visibility',{visible:true,token:102});});await page.waitForTimeout(450);
  await page.evaluate(()=>{window.__emit('panel-visibility',{visible:false,token:103});});await page.waitForTimeout(60);await page.evaluate(()=>{window.__emit('panel-visibility',{visible:true,token:104});});await page.waitForTimeout(460);assert.equal(await page.evaluate(()=>window.__calls.some(c=>c.name==='finish_panel_hide'&&c.args.token===103)),false);
  await page.emulateMedia({reducedMotion:'reduce'});await page.evaluate(()=>window.__emit('panel-peek'));await page.waitForTimeout(70);assert(Math.abs(await page.locator('#notch').evaluate(e=>e.getBoundingClientRect().top))<1);await page.emulateMedia({reducedMotion:'no-preference'});
  await page.getByRole('button',{name:'Open Sora',exact:true}).click();await page.waitForTimeout(350);
  console.log('PASS: top-edge entrance/exit, deferred native hide, reversal cancellation and reduced motion.');
  assert.equal(await page.locator('#timer-reset').count(),0);
  assert.equal(await page.locator('.timer-ring').evaluate(e=>e.tagName),'BUTTON');
  assert.equal(await page.locator('#weather-clock').count(),0);
  await page.locator('#timer-dial-wrap').dispatchEvent('wheel',{deltaY:-100});assert.equal(await page.locator('#minutes').textContent(),'24','scrolling the dial sets whole minutes');await page.locator('#timer-dial-wrap').dispatchEvent('wheel',{deltaY:100});assert.equal(await page.locator('#minutes').textContent(),'25');await page.getByRole('button',{name:'Restart timer',exact:true}).click();assert.equal(await page.locator('#minutes').textContent(),'25');assert.equal(await page.locator('#seconds').textContent(),'00');assert.equal(await page.locator('#timer-play').getAttribute('aria-label'),'Start timer');
  await page.locator('#timer-play').click();await page.locator('#timer-restart').click();assert.equal(await page.locator('#timer-play').getAttribute('aria-label'),'Pause timer');assert.equal(await page.locator('#minutes').textContent(),'25');await page.locator('#timer-play').click();
  const labelPositions=[];for(const minutes of ['9','10','99']){await page.locator('#timer-edit').click();await page.locator('#timer-min-input').fill(minutes);await page.locator('#timer-sec-input').fill('0');await page.getByRole('button',{name:'Set timer',exact:true}).click();await page.waitForTimeout(300);labelPositions.push(await page.locator('#timer-min-label').evaluate(e=>e.getBoundingClientRect().left))}assert(Math.max(...labelPositions)-Math.min(...labelPositions)<.5);
  await page.locator('#timer-edit').click();await page.locator('#timer-min-input').fill('25');await page.getByRole('button',{name:'Set timer',exact:true}).click();await page.waitForTimeout(300);
  const digitLayouts=[];for(let n=0;n<=9;n++){await page.locator('#timer-edit').click();await page.locator('#timer-min-input').fill(String(n*11));await page.locator('#timer-sec-input').fill('1');await page.getByRole('button',{name:'Set timer',exact:true}).click();await page.waitForTimeout(300);digitLayouts.push(await page.locator('#minutes .timer-digit').evaluateAll(els=>els.map(e=>{const r=e.getBoundingClientRect();return {left:r.left,width:r.width,text:e.textContent}})))}
  for(const layout of digitLayouts){assert.equal(layout.length,2);for(let i=0;i<2;i++){assert(Math.abs(layout[i].width-19*.85)<.1);assert(Math.abs(layout[i].left-digitLayouts[0][i].left)<.5)}}
  await page.locator('#timer-edit').click();await page.locator('#timer-min-input').fill('25');await page.locator('#timer-sec-input').fill('0');await page.getByRole('button',{name:'Set timer',exact:true}).click();await page.waitForTimeout(300);
  console.log('PASS: digit slots stay fixed for every numeral 0–9.');
  const cards=await page.evaluate(()=>[...document.querySelectorAll('#home>.widget:not([hidden])')].map(w=>{const r=w.getBoundingClientRect();return{id:w.dataset.widget,top:r.top,bottom:r.bottom,width:r.width,bg:getComputedStyle(w).backgroundColor}}));
  assert.deepEqual(cards.map(c=>c.id),['weather','calendar','timer'],'Home shows three widgets, no cards');assert(cards.every(c=>Math.abs(c.top-cards[0].top)<.5&&Math.abs(c.bottom-cards[0].bottom)<.5&&Math.abs(c.width-cards[0].width)<.5&&c.bg==='rgba(0, 0, 0, 0)'),JSON.stringify(cards));
  const inside=await page.evaluate(()=>[...document.querySelectorAll('#home>.widget:not([hidden]) *')].filter(e=>e.getClientRects().length&&!e.closest('.w-remove')&&getComputedStyle(e).visibility!=='hidden').every(e=>{const c=e.closest('.widget').getBoundingClientRect(),r=e.getBoundingClientRect();return r.width===0||(r.left>=c.left-.5&&r.right<=c.right+.5&&r.top>=c.top-.5&&r.bottom<=c.bottom+.5)}));assert(inside,'nothing spills out of a card');
  const homeReadability=await page.evaluate(()=>{const label=document.querySelector('#timer-label'),digits=document.querySelector('#timer-edit'),units=document.querySelector('#condition');return{label:label.textContent,visible:getComputedStyle(label).clipPath,labelBottom:label.getBoundingClientRect().bottom,digitTop:digits.getBoundingClientRect().top,align:getComputedStyle(units).textAlign,font:parseFloat(getComputedStyle(units).fontSize),lineHeight:parseFloat(getComputedStyle(units).lineHeight),}});assert.equal(homeReadability.label,'Focus Timer');assert.equal(homeReadability.visible,'none');assert(homeReadability.labelBottom<=homeReadability.digitTop);assert(homeReadability.font>=15&&homeReadability.lineHeight>=homeReadability.font*1.2,JSON.stringify(homeReadability));assert.equal(homeReadability.align,'right');
  await page.screenshot({path:path.join(artifacts,'rev1105-home.png')});
  await page.screenshot({path:path.join(artifacts,'rev1102-home.png')});
  await page.evaluate(()=>(document.querySelector('[data-widget=calendar]').dataset.size='wide',document.querySelector('[data-widget=timer]').hidden=true));await page.getByRole('button',{name:'Add event',exact:false}).click();
  const dates=await page.evaluate(()=>{const key=d=>d.getFullYear()+'-'+String(d.getMonth()+1).padStart(2,'0')+'-'+String(d.getDate()).padStart(2,'0');const yesterday=new Date(),tomorrow=new Date();yesterday.setDate(yesterday.getDate()-1);tomorrow.setDate(tomorrow.getDate()+1);return{past:key(yesterday),future:key(tomorrow)}});
  await page.screenshot({path:path.join(artifacts,'rev17-event.png')});await page.locator('#event-title').fill('Future review');assert.equal(await page.locator('#event-title').evaluate(e=>getComputedStyle(e).webkitTextFillColor),'rgb(237, 237, 237)');await page.screenshot({path:path.join(artifacts,'rev18-event.png')});await page.locator('#event-date').fill(dates.past);await page.locator('#event-time').fill('09:00');
  await page.locator('#event-form').dispatchEvent('submit');assert.match(await page.locator('#event-error').textContent(),/future/);assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('nook.agenda')||'[]').length),0);
  await page.locator('#event-date').fill(dates.future);await page.locator('#event-time').fill('12:00');await page.getByRole('button',{name:'Save event',exact:true}).click();assert.equal(await page.locator('.event-name').textContent(),'Future review');
  await page.evaluate(()=>(document.querySelector('[data-widget=calendar]').dataset.size='wide',document.querySelector('[data-widget=timer]').hidden=true));await page.getByRole('button',{name:'Next month',exact:true}).click();await page.getByRole('button',{name:'Next month',exact:true}).click();
  assert.equal(await page.locator('#week').evaluate(e=>e.classList.contains('current-week')),false);
  await page.locator('.day').first().hover();assert.equal(await page.locator('.day').first().evaluate(e=>getComputedStyle(e).backgroundColor),'rgba(0, 0, 0, 0)');await page.evaluate(()=>(document.querySelector('[data-widget=calendar]').dataset.size='square',document.querySelector('[data-widget=timer]').hidden=false));
  await page.evaluate(()=>{window.__testLyrics={syncedLyrics:'[00:00]今日はいい天気です\n[00:03]明日は新しい一日\n[00:07]一緒に歩いていこう\n[00:11]笑顔で会いましょう\n[00:15]ありがとうございます'};window.__testMedia={title:'Japanese fixture',artist:'Tester',album:'Tests',source:'Spotify.exe',playing:false,position:4,duration:100,can_play:true,can_seek:true,can_next:true,can_previous:true};});
  await page.getByRole('button',{name:'Music',exact:true}).click();await page.locator('.artwork').hover();
  const musicInset=await page.locator('.artwork').evaluate(e=>{const a=e.getBoundingClientRect(),n=document.querySelector('#notch').getBoundingClientRect();return a.left-n.left});assert(Math.abs(musicInset-28*.85)<1);
  await page.waitForSelector('.lyric-line:not(.lyric-spacer)');await page.evaluate(()=>document.fonts.ready);await page.waitForTimeout(500);
  assert.equal(await page.locator('#seek').evaluate(e=>getComputedStyle(e).height),'20px');
  const line=page.locator('.lyric-line[data-line="4"]');const before=await line.evaluate(e=>{window.__lyricNode=e;return true});await line.click();
  await page.waitForFunction(()=>document.querySelector('.lyric-line[data-line="4"]').classList.contains('current'));
  assert.equal(await page.evaluate(()=>window.__lyricNode===document.querySelector('.lyric-line[data-line="4"]')),true);
  const action=await page.evaluate(()=>window.__calls.filter(c=>c.name==='media_action').at(-1));assert.equal(action.args.action,'seek');assert.equal(action.args.position,7);
  assert.match(await line.evaluate(e=>getComputedStyle(e).fontFamily),/Noto JP/);
  await page.waitForTimeout(450);await page.screenshot({path:path.join(artifacts,'rev14-music-jp.png')});await page.waitForTimeout(600);const levels=await page.locator('#lyrics-lines .lyric-line:not([hidden])').evaluateAll(rows=>rows.map(e=>({opacity:Number(getComputedStyle(e).opacity),current:e.classList.contains('current'),edge:e.classList.contains('edge-line')})));assert(levels.filter(r=>!r.current&&!r.edge).every(r=>r.opacity===.58));assert(levels.find(r=>r.current).opacity===1);
  const bounds=await page.locator('#lyrics-lines').evaluate(e=>({client:e.clientHeight,scroll:e.scrollHeight}));assert(bounds.scroll<=bounds.client+2,JSON.stringify(bounds));
  for(const [lang,text,font] of [['ko','오늘은 함께 천천히 길을 걸으며 아름다운 하늘을 바라봅니다','Noto KR'],['zh','今天我们一起慢慢走过街道看看远方美丽的天空和白云','Noto SC']]){
    await page.evaluate(({lang,text})=>{window.__testMedia={...window.__testMedia,title:'Long '+lang,position:7};window.__testLyrics={syncedLyrics:[0,3,7,11,15].map(t=>'[00:'+String(t).padStart(2,'0')+']'+text).join('\n')}},{lang,text});
    await page.waitForFunction(lang=>document.querySelector('#track-title').textContent==='Long '+lang,lang);
    await page.evaluate(()=>document.fonts.ready);await page.waitForTimeout(500);
    assert.match(await page.locator('.lyric-line.current').first().evaluate(e=>getComputedStyle(e).fontFamily),new RegExp(font));
    assert.equal(await page.locator('.lyric-line:visible').count(),5);
    const fit=await page.locator('#lyrics-lines').evaluate(e=>{const b=e.getBoundingClientRect(),a=e.querySelector('.current').getBoundingClientRect();return{top:a.top-b.top,bottom:b.bottom-a.bottom,overflow:getComputedStyle(e).overflowY}});assert(fit.top>=-1&&fit.bottom>=-1,JSON.stringify(fit));
    assert.deepEqual(await page.locator('.lyric-line:visible').evaluateAll(rows=>rows.map(r=>Number(r.dataset.time))),[0,3,7,11,15]);assert.equal(await page.locator('.lyric-line.current').getAttribute('data-time'),'7');
    if(lang==='ko'){
      await page.locator('#romanize').click();await page.waitForFunction(()=>/[a-z]/i.test(document.querySelector('.lyric-line.current').textContent));
      const romanTimes=await page.locator('.lyric-line:visible').evaluateAll(rows=>rows.map(r=>Number(r.dataset.time)));assert(romanTimes.includes(7));assert(romanTimes.every(t=>[0,3,7,11,15].includes(t)));
      await page.locator('.lyric-line[data-time="3"]').click();assert.equal((await page.evaluate(()=>window.__calls.filter(c=>c.name==='media_action').at(-1))).args.position,3);
      await page.locator('.lyric-line[data-time="11"]').click();assert.equal((await page.evaluate(()=>window.__calls.filter(c=>c.name==='media_action').at(-1))).args.position,11);
      await page.locator('#romanize').click();
    }
    await page.screenshot({path:path.join(artifacts,'rev14-music-'+lang+'.png')});
  }

  // Mixed one/two/multi-line cues must show full text with equal physical gaps.
  const fullCue='Aku ingin semua sukacita yang kita simpan tetap bersama sampai pagi datang kembali';
  await page.evaluate(fullCue=>{window.__testMedia={...window.__testMedia,title:'Full text spacing test',position:7};window.__testLyrics={syncedLyrics:'[00:00]Awal cerita\n[00:03]Kita berjalan bersama di bawah cahaya bulan\n[00:07]'+fullCue+'\n[00:11]Besok kembali\n[00:15]Sampai jumpa di tempat yang sama'}},fullCue);
  await page.waitForFunction(()=>document.querySelector('.lyric-line.current')?.textContent.includes('Aku ingin semua'));
  await page.waitForTimeout(600);
  assert.equal(await page.locator('.lyric-line.current').textContent(),fullCue);
  const lyricFit=await page.locator('#lyrics-lines').evaluate(el=>{const scale=Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1;const rows=[...el.children].filter(r=>!r.hidden);return {font:parseFloat(getComputedStyle(rows.find(r=>r.classList.contains('current'))).fontSize),rows:rows.map(r=>{const b=r.querySelector('.lyric-body');return {height:r.clientHeight,content:b?.scrollHeight||r.clientHeight,width:b?.clientWidth,scrollWidth:b?.scrollWidth}}),gaps:rows.slice(1).map((r,i)=>(r.getBoundingClientRect().top-rows[i].getBoundingClientRect().bottom)/scale),height:el.clientHeight,scroll:el.scrollHeight}});
  assert.equal(lyricFit.font,17);assert(lyricFit.rows.every(r=>r.content<=r.height+1&&(!r.width||r.scrollWidth<=r.width+1)),JSON.stringify(lyricFit));assert(lyricFit.gaps.every(g=>Math.abs(g-7)<.3),JSON.stringify(lyricFit));assert.equal(await page.locator('.lyric-line:not([hidden])').count(),5);
  const stageCheck=await page.locator('#lyrics-lines').evaluate(el=>{const r=el.getBoundingClientRect(),a=el.querySelector('.current').getBoundingClientRect(),settings=document.querySelector('#settings-button').getBoundingClientRect();return{top:a.top-r.top,bottom:r.bottom-a.bottom,right:r.right,limit:(settings.left+settings.right)/2}});assert(stageCheck.top>=-1&&stageCheck.bottom>=-1,JSON.stringify(stageCheck));assert(Math.abs(stageCheck.right-stageCheck.limit)<1,JSON.stringify(stageCheck));
  const emphasis=await page.locator('.lyric-line:not([hidden])').evaluateAll(rows=>rows.map(r=>({slot:Number(r.dataset.slot),opacity:Number(getComputedStyle(r).opacity),filter:getComputedStyle(r).filter,mask:getComputedStyle(r).maskImage,size:parseFloat(getComputedStyle(r).fontSize)})));assert.deepEqual(emphasis.map(r=>r.slot),[0,1,2,3,4]);const feather=await page.locator('.lyric-stage').evaluate(stage=>{
   const box=stage.querySelector('#lyrics-lines'),cue=box.querySelector('.current'),b=box.getBoundingClientRect(),c=cue.getBoundingClientRect(),scale=b.height/box.clientHeight;
   const paint=getComputedStyle(stage,'::after');
   return {blur:paint.backdropFilter,mask:paint.maskImage,active:getComputedStyle(cue).filter,top:parseFloat(stage.style.getPropertyValue('--feather-top')),bottom:parseFloat(stage.style.getPropertyValue('--feather-bottom')),spaceAbove:(c.top-b.top)/scale,spaceBelow:(b.bottom-c.bottom)/scale};
  });
  assert(feather.blur==='blur(3.2px)'&&feather.mask.includes('linear-gradient')&&feather.active==='none',JSON.stringify(feather));
  assert(feather.top<=Math.max(0,feather.spaceAbove-5)+1&&feather.bottom<=Math.max(0,feather.spaceBelow-5)+1,JSON.stringify(feather));
  assert(emphasis.filter(r=>r.slot===1||r.slot===3).every(r=>r.opacity===.58));assert(emphasis[2].size>emphasis[1].size&&emphasis[2].opacity===1);
  await page.screenshot({path:path.join(artifacts,'rev1104-full-lyrics.png')});

  // A live interpolated timestamp crossing must immediately show full emphasis.
  await page.evaluate(()=>{window.__cueSample=0;const original=window.__TAURI__.core.invoke;window.__TAURI__.core.invoke=async(name,args)=>{const result=await original(name,args);if(name==='media_status'&&result?.title==='Fast cue test'&&!window.__cueSample)window.__cueSample=performance.now();return result};window.__testMedia={...window.__testMedia,title:'Fast cue test',position:.8,playing:true};window.__testLyrics={syncedLyrics:'[00:00]First cue\n[00:01]Second cue\n[00:02]Third cue\n[00:03]Fourth cue\n[00:04]Fifth cue'}});
  const latency=await page.evaluate(()=>new Promise((resolve,reject)=>{const deadline=performance.now()+3000;function tick(){const row=document.querySelector('.lyric-line.current');if(row?.textContent==='Second cue'){resolve({elapsed:performance.now()-window.__cueSample,opacity:Number(getComputedStyle(row).opacity),color:getComputedStyle(row).color,durations:row.getAnimations().map(a=>a.effect.getTiming().duration)});return}if(performance.now()>deadline){reject(Error('Timed cue never highlighted'));return}requestAnimationFrame(tick)}tick()}));assert(latency.elapsed>=150&&latency.elapsed<800,JSON.stringify(latency));assert(latency.opacity>0.2&&latency.opacity<=1);assert(latency.durations.every(d=>d<=600));console.log('PASS: timestamp highlight latency',latency);
  await page.evaluate(()=>{window.__testMedia={...window.__testMedia,title:'Instrumental test',position:7};window.__testLyrics={instrumental:true}});
  await page.waitForSelector('.instrumental-dots');assert.equal(await page.locator('.instrumental-dots span').count(),3);
  assert.equal(await page.locator('#collapse').count(),0);
  await page.evaluate(()=>{window.__testMedia={...window.__testMedia,title:'Plain fallback'};window.__testLyrics={plainLyrics:'An original unsynced test line\nA second test line',provider:'Lyrics.ovh'}});
  await page.waitForFunction(()=>document.querySelector('#lyrics-lines').classList.contains('plain'));
  assert(!/Unsynced|Lyrics.ovh/.test(await page.locator('#lyrics-lines').innerText()));assert.equal(await page.locator('.lyric-line').count(),0);
  await page.evaluate(()=>{window.__testMedia={...window.__testMedia,title:'Timed break test',position:5};window.__testLyrics={syncedLyrics:'[00:00]Before the break\n[00:04]\n[00:12]After the break\n[00:16]Another lyric'}});
  await page.waitForSelector('.instrumental-cue.current');
  assert.match(await page.locator('#lyrics-lines').innerText(),/Before the break/);assert.match(await page.locator('#lyrics-lines').innerText(),/After the break/);
  await page.evaluate(()=>{window.__testMedia={...window.__testMedia,title:'Backing vocal test',position:5};window.__testLyrics={syncedLyrics:'[00:00]An opening line\n[00:04]I stay (with you)\n[00:12]The next line'}});
  await page.waitForSelector('.current .vocal-secondary');assert.equal(await page.locator('.current .vocal-secondary').innerText(),'(with you)');
  const vocalRows=await page.locator('.lyric-line.current').allTextContents();assert.deepEqual(vocalRows,['I stay(with you)']);
  await page.screenshot({path:path.join(artifacts,'rev16-backing.png')});

  for(const [source,icon,label] of [['AppleInc.AppleMusicWin_nzyj5cx40ttqa!App','applemusic','Apple Music'],['chrome.exe','chrome','Google Chrome'],['msedge.exe','edge','Microsoft Edge'],['brave.exe','brave','Brave'],['firefox.exe','firefox','Firefox'],['opera.exe','opera','Opera'],['vivaldi.exe','vivaldi','Vivaldi'],['SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify','spotify','Spotify'],['vlc.exe','vlc','VLC'],['YouTube-Music.exe','youtubemusic','YouTube Music'],['AmazonMusic.exe','amazonmusic','Amazon Music'],['TIDAL.exe','tidal','TIDAL'],['SoundCloud.exe','soundcloud','SoundCloud'],['iTunes.exe','itunes','iTunes']]){
    await page.evaluate(source=>{window.__testMedia={...window.__testMedia,source}},source);await page.getByRole('button',{name:'Music',exact:true}).click();await page.waitForFunction(icon=>document.querySelector('#music-source').dataset.source===icon,icon);
    await page.waitForFunction(()=>{const img=document.querySelector('#music-source img'),art=document.querySelector('.artwork');return art.classList.contains('cut')&&document.querySelector('#music-source').classList.contains('ready')&&img&&img.complete&&img.naturalWidth>0});assert.equal(await page.locator('#music-source').getAttribute('aria-label'),label);
    const cut=await page.locator('#music-source').evaluate(e=>{const reach=(()=>{const a=e.parentElement.getBoundingClientRect(),l=document.querySelector('.art-bg').getBoundingClientRect();return l.right-a.right>=2.5&&l.bottom-a.bottom>=2.5&&getComputedStyle(document.querySelector('.art-bg')).clipPath.includes('inset')})();if(!reach)throw Error('cover layers must reach past the visible box so the cut covers its edge row');const b=e.getBoundingClientRect(),a=e.parentElement.getBoundingClientRect(),s=getComputedStyle(e),bg=getComputedStyle(document.querySelector('.art-bg')),cover=getComputedStyle(document.querySelector('#album-art'));return{overlaps:b.left>=a.left&&b.right>a.right&&b.bottom>a.bottom,frame:s.borderTopWidth,fill:s.backgroundColor,shadow:s.boxShadow,radius:s.borderTopLeftRadius,bgMask:bg.maskImage.includes('url('),coverMask:cover.maskImage.includes('url('),composite:bg.maskComposite}});
    assert(cut.overlaps&&cut.frame==='0px'&&cut.fill==='rgba(0, 0, 0, 0)'&&cut.shadow==='none'&&cut.radius==='0px'&&cut.bgMask&&cut.coverMask,'logo has no frame and the cover is cut around it: '+JSON.stringify(cut));
  }
  await page.locator('#music-source img').evaluate(e=>window.__savedLogo=e);await page.waitForTimeout(850);assert(await page.locator('#music-source img').evaluate(e=>e===window.__savedLogo));
  await page.evaluate(()=>{const original=window.__TAURI__.core.invoke;window.__TAURI__.core.invoke=(name,args)=>name==='media_source_icon'?new Promise(resolve=>{(window.__resolvers||=[]).push(resolve)}):original(name,args);window.__testMedia={...window.__testMedia,source:'Unknown.Player!App'}});await page.getByRole('button',{name:'Music',exact:true}).click();await page.waitForFunction(()=>window.__resolvers?.length>0);assert.equal(await page.locator('#music-source').getAttribute('data-source'),'media');
  await page.evaluate(()=>{window.__testMedia={...window.__testMedia,source:'brave.exe'}});await page.getByRole('button',{name:'Music',exact:true}).click();await page.waitForFunction(()=>document.querySelector('#music-source').dataset.source==='brave');
  await page.evaluate(()=>window.__resolvers[0]('data:image/png;base64,stale'));
  await page.waitForFunction(()=>document.querySelector('#music-source.ready img'),null,{timeout:4000});
  assert.match(await page.locator('#music-source img').getAttribute('src'),/logos\/color\/brave\.png$/,'a late answer for the previous player never replaces the current icon');
  await page.screenshot({path:path.join(artifacts,'rev1108-player-badge.png')});console.log('PASS: 14 source icons load, the cover is cut around the unframed logo, same-source image persists, unknown/stale fallback is safe.');
  await page.getByRole('button',{name:'Settings',exact:true}).click();await page.waitForFunction(()=>window.__calls.some(c=>c.name==='open_settings'));
  assert.equal(await page.locator('#settings').count(),0,'settings are no longer inside the notch');assert.equal(await page.locator('nav .nav:not([hidden])').count(),5,'the notch has Home, Music, Tray, Send and Board');assert.equal(await page.locator('nav .nav[hidden]').count(),2,'Labs tabs stay hidden while their modules are off');
  console.log('PASS: the gear opens the separate Settings window; the notch includes the Send tab.');
  // Edge handle: idle strip, hover glow with dwell fill, and its switch
  await page.evaluate(()=>window.__emit('edge-mode',true));
  assert(await page.evaluate(()=>document.body.classList.contains('edge')&&getComputedStyle(document.querySelector('#edge')).display==='block'&&getComputedStyle(document.querySelector('#notch')).visibility==='hidden'));
  const idle=await page.evaluate(()=>{const p=document.querySelector('#edge .edge-pill'),r=p.getBoundingClientRect(),s=getComputedStyle(p);return{top:r.top,width:r.width,bg:s.backgroundColor,shadow:s.boxShadow,border:s.borderTopWidth,eyeOpacity:getComputedStyle(p.querySelector('b')).opacity,earW:p.querySelector('.edge-ear').getBoundingClientRect().width}});
  assert(idle.top>=4&&idle.width<=76&&idle.bg==='rgb(5, 5, 6)'&&idle.shadow==='none'&&idle.border==='0px'&&idle.eyeOpacity==='0'&&idle.earW<.5,'floating flat black capsule, no stroke, no eyes or wings at rest, no shadow: '+JSON.stringify(idle));
  // Morph: the capsule grows into the compact notch (same size, radius and place), then hands over to the real notch
  await page.evaluate(()=>window.__emit('edge-hover',{active:true,ms:1200}));await page.waitForTimeout(450);
  const mid=await page.evaluate(()=>{const p=document.querySelector('#edge .edge-pill');return{w:p.getBoundingClientRect().width,eye:Number(getComputedStyle(p.querySelector('b')).opacity)}});
  assert(mid.w>100&&mid.w<290&&mid.eye>.5,'capsule is growing and its eyes are open: '+JSON.stringify(mid));
  const wings=await page.evaluate(()=>{const p=document.querySelector('#edge .edge-pill'),pr=p.getBoundingClientRect(),l=p.querySelector('.edge-ear.l').getBoundingClientRect();return{wing:l.width,gapToBody:pr.left-l.right,p:getComputedStyle(p).getPropertyValue('--p')}});
  assert(wings.gapToBody<=.05,'wings stay attached to the body while it grows: '+JSON.stringify(wings));
  await page.waitForTimeout(1000);
  const docked=await page.evaluate(()=>{const p=document.querySelector('#edge .edge-pill'),r=p.getBoundingClientRect(),s=getComputedStyle(p),scale=Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1,vw=innerWidth;return{w:r.width,h:r.height,top:r.top,centred:Math.abs((r.left+r.width/2)-vw/2),wantW:292*scale,wantH:64*scale,radius:s.borderBottomLeftRadius,topRadius:s.borderTopLeftRadius,shadow:s.boxShadow,ear:p.querySelector('.edge-ear').getBoundingClientRect().width/(13*scale),earJoin:[p.querySelector('.edge-ear.l').getBoundingClientRect(),p.querySelector('.edge-ear.r').getBoundingClientRect(),r].map(b=>[b.left,b.right,b.top]),eye:Number(getComputedStyle(p.querySelector('b')).opacity)}});
  assert(Math.abs(docked.w-docked.wantW)<1.5&&Math.abs(docked.h-docked.wantH)<1.5&&docked.top<=.5&&docked.centred<1.5&&docked.topRadius==='0px'&&parseFloat(docked.radius)>10&&docked.ear>.95&&docked.eye>.95&&docked.shadow==='none'&&docked.earJoin[0][1]>=docked.earJoin[2][0]&&docked.earJoin[1][0]<=docked.earJoin[2][1]&&Math.abs(docked.earJoin[0][2]-docked.earJoin[2][2])<.5,'docked capsule equals the compact notch: '+JSON.stringify(docked));
  await page.evaluate(()=>{window.__samples={op:new Set(),eyeW:0,eyeH:0,mouth:0};const loop=()=>{const n=document.querySelector('#notch'),e=document.querySelector('#edge .edge-pill b').getBoundingClientRect();window.__samples.op.add(Number(getComputedStyle(n).opacity).toFixed(2));window.__samples.eyeW=Math.max(window.__samples.eyeW,e.width);window.__samples.eyeH=Math.max(window.__samples.eyeH,e.height);window.__samples.mouth=Math.max(window.__samples.mouth,Number(getComputedStyle(document.querySelector('#edge .edge-face')).opacity));if(!window.__stopSamples)requestAnimationFrame(loop)};loop()});
  await page.evaluate(()=>window.__emit('edge-hover',{active:false,ms:0,done:true}));
  assert(await page.evaluate(()=>document.querySelector('#edge').classList.contains('handoff')),'hand-off keeps the docked capsule on screen');
  await page.evaluate(()=>{window.__emit('panel-peek');window.__emit('edge-mode',false)});
  await page.waitForTimeout(300);
  assert.equal(await page.evaluate(()=>Number(getComputedStyle(document.querySelector('#notch')).opacity)),0,'notch stays hidden while the capsule blinks and its eyes grow');
  await page.waitForTimeout(1000);
  const handed=await page.evaluate(()=>{window.__stopSamples=true;const k=(Number(getComputedStyle(document.documentElement).getPropertyValue('--scale'))||1)*.9625;return{ops:[...window.__samples.op],eyeW:window.__samples.eyeW,eyeH:window.__samples.eyeH,wantW:14*k,wantH:34*k,mouth:window.__samples.mouth,notch:Number(getComputedStyle(document.querySelector('#notch')).opacity),handoff:document.querySelector('#edge').classList.contains('handoff'),display:getComputedStyle(document.querySelector('#edge')).display}});
  assert(handed.ops.every(o=>o==='0.00'||o==='1.00')&&handed.ops.includes('1.00'),'the notch appears in one frame, never fading: '+JSON.stringify(handed));
  assert(handed.eyeW>=handed.wantW*.9&&handed.eyeH>=handed.wantH*.9&&handed.mouth>.9,'eyes grew to the mascot size and the smile appeared before the swap: '+JSON.stringify(handed));
  assert(handed.notch===1&&!handed.handoff&&handed.display==='none','capsule is gone after the swap: '+JSON.stringify(handed));
  // Leaving early: the capsule shrinks back instead of snapping
  await page.evaluate(()=>window.__emit('edge-mode',true));
  await page.evaluate(()=>window.__emit('edge-hover',{active:true,ms:3000}));await page.waitForTimeout(700);
  await page.evaluate(()=>window.__emit('edge-hover',{active:false,ms:0}));await page.waitForTimeout(900);
  const back=await page.evaluate(()=>({w:document.querySelector('#edge .edge-pill').getBoundingClientRect().width,anims:document.querySelector('#edge .edge-pill').getAnimations().filter(a=>a instanceof Animation&&!(a instanceof CSSAnimation)&&!(a instanceof CSSTransition)).length,hot:document.querySelector('#edge').classList.contains('hot')}));
  assert(back.w<=76&&back.anims===0&&!back.hot,'capsule retreats to rest: '+JSON.stringify(back));
  // Hiding with the handle on: the compact notch shrinks into the capsule, then Rust is told to finish
  await page.evaluate(()=>window.__emit('edge-mode',false));await page.waitForTimeout(100);
  await page.evaluate(()=>{window.__calls.length=0;window.__emit('panel-visibility',{visible:false,token:707,edge:true})});
  await page.waitForTimeout(450);assert(await page.evaluate(()=>document.querySelector('#edge').classList.contains('handoff')&&Number(getComputedStyle(document.querySelector('#notch')).opacity)===0),'capsule replaces the notch in one frame, then shrinks');
  await page.waitForFunction(()=>window.__calls.some(c=>c.name==='finish_panel_hide'&&c.args.token===707));
  assert(!(await page.evaluate(()=>document.querySelector('#edge').classList.contains('handoff'))));
  await page.evaluate(()=>window.__emit('panel-peek'));await page.waitForTimeout(700);
  await page.evaluate(()=>window.__emit('edge-hover',{active:false,ms:0}));
  // Revealed from the capsule (shortcut): it grows into the notch where it floats; the notch never slides in from above
  await page.evaluate(()=>{const n=document.querySelector('#notch');n.style.opacity='0';n.style.translate='0 -110%';window.__emit('edge-mode',true);window.__seen={tr:new Set(),op:new Set(),handoff:false,grew:0};const loop=()=>{const s=getComputedStyle(n);window.__seen.tr.add(s.translate);window.__seen.op.add(Number(s.opacity).toFixed(2));if(document.querySelector('#edge').classList.contains('handoff'))window.__seen.handoff=true;window.__seen.grew=Math.max(window.__seen.grew,document.querySelector('#edge .edge-pill').getBoundingClientRect().width);if(!window.__stopSeen)requestAnimationFrame(loop)};loop()});
  await page.waitForTimeout(200);await page.evaluate(()=>window.__emit('panel-peek'));await page.waitForTimeout(1900);
  const fromEdge=await page.evaluate(()=>{window.__stopSeen=true;const n=getComputedStyle(document.querySelector('#notch'));return{seen:{tr:[...window.__seen.tr],op:[...window.__seen.op],handoff:window.__seen.handoff,grew:window.__seen.grew},opacity:Number(n.opacity),translate:n.translate,handoff:document.querySelector('#edge').classList.contains('handoff')}});
  assert(fromEdge.seen.handoff&&fromEdge.seen.grew>200,'capsule grows into the notch when revealed from the edge: '+JSON.stringify(fromEdge));
  assert(fromEdge.seen.op.every(o=>o==='0.00'||o==='1.00'),'notch appears in one frame, not faded: '+JSON.stringify(fromEdge));
  assert(fromEdge.opacity===1&&(fromEdge.translate==='none'||fromEdge.translate==='0px')&&!fromEdge.handoff,'notch settled in place, capsule gone: '+JSON.stringify(fromEdge));
  assert(!fromEdge.seen.tr.some(v=>v&&v!=='none'&&v!=='0px'&&v!=='0px 0px'&&!/^0px -110%/.test(v)),'notch never slides: '+JSON.stringify(fromEdge.seen.tr));
  // A policy un-hide (panel-visibility visible) while the capsule is showing grows it too
  await page.evaluate(()=>{const n=document.querySelector('#notch');n.style.opacity='0';n.style.translate='0 -110%';window.__emit('edge-mode',true);window.__emit('panel-visibility',{visible:true,token:55})});
  await page.waitForTimeout(250);
  assert(await page.evaluate(()=>document.querySelector('#edge').classList.contains('handoff')&&document.querySelector('#edge .edge-pill').getBoundingClientRect().width>70),'policy un-hide grows from the capsule');
  await page.waitForTimeout(1700);
  assert.equal(await page.evaluate(()=>getComputedStyle(document.querySelector('#notch')).translate==='none'||getComputedStyle(document.querySelector('#notch')).translate==='0px'),true);
  await page.evaluate(()=>window.__emit('edge-mode',false));
  // Double-clicking the mascot opens once; the notch ignores double-click handling
  const dbl=await page.evaluate(()=>{let prevented=false;const h=e=>{prevented=e.defaultPrevented};const ev=new MouseEvent('dblclick',{bubbles:true,cancelable:true});document.addEventListener('dblclick',h);document.querySelector('#notch').dispatchEvent(ev);document.removeEventListener('dblclick',h);return ev.defaultPrevented});
  assert(dbl,'double-click is swallowed');
  await page.getByRole('button',{name:'Open Sora',exact:true}).click();await page.waitForTimeout(600);
  console.log('PASS: edge handle capsule morphs into the notch and back.');
  await page.getByRole('button',{name:'Home',exact:true}).click();await page.waitForTimeout(350);
  const ring=await page.evaluate(()=>{const c=document.querySelector('[data-widget=timer]').getBoundingClientRect(),r=document.querySelector('.timer-ring').getBoundingClientRect(),d=document.getElementById('timer-edit').getBoundingClientRect();return r.right<=c.right&&r.top>=c.top&&d.left>=c.left&&d.right<=c.right});assert(ring,'the hourglass and the digits stay inside the Focus card');
  await page.getByRole('button',{name:'Change weather location',exact:true}).click();await page.waitForTimeout(350);await page.screenshot({path:path.join(artifacts,'rev17-location.png')});await page.getByRole('textbox',{name:'City name'}).fill('Bandung');assert.equal(await page.locator('#city-query').evaluate(e=>getComputedStyle(e).color),'rgb(237, 237, 237)');await page.screenshot({path:path.join(artifacts,'rev18-location.png')});await page.getByRole('button',{name:'Search',exact:true}).click();await page.getByRole('button',{name:'Bandung, Jawa Barat',exact:true}).click();assert.equal(await page.locator('#city-name').innerText(),'Bandung');
  // The full address comes in behind the weather; the picked name stays. Online results add kelurahan; the duplicate city is merged.
  await page.waitForFunction(()=>JSON.parse(localStorage.getItem('nook.settings')).place?.name==='Kebon Pisang');assert.equal(await page.locator('#city-name').innerText(),'Bandung');
  await page.getByRole('button',{name:'Change weather location',exact:true}).click();await page.waitForTimeout(350);assert.match(await page.locator('#loc-detail').innerText(),/Kel\.\/Desa Kebon Pisang, Kec\. Sumur Bandung, Kota Bandung/);
  await page.getByRole('textbox',{name:'City name'}).fill('Bandung');await page.getByRole('button',{name:'Search',exact:true}).click();await page.getByRole('button',{name:'Kebon Pisang, Sumur Bandung, Bandung, Jawa Barat'}).waitFor();assert.equal(await page.locator('#city-results .city-result').count(),2,'the online Bandung is the bundled one');
  await page.getByRole('button',{name:'Use my location'}).click();await page.waitForFunction(()=>document.querySelector('#loc-city').textContent==='Pondok Bambu');assert.match(await page.locator('#loc-coords').innerText(),/6.2251° S +106.9001° E +±35 m/);assert.match(await page.locator('#loc-detail').innerText(),/^Kec. Duren Sawit, Kota Jakarta Timur, Prov. Jakarta$/);await page.screenshot({path:path.join(artifacts,'location-full-address.png')});
  await page.getByRole('button',{name:'Back'}).first().click();await page.waitForTimeout(350);assert.equal(await page.locator('#city-name').innerText(),'Pondok Bambu');
  await page.getByRole('button',{name:'Set duration',exact:true}).click();await page.waitForTimeout(300);await page.screenshot({path:path.join(artifacts,'rev17-timer.png')});
  await page.locator('#timer-min-input').fill('12');await page.locator('#timer-sec-input').fill('34');await page.getByRole('button',{name:'Set timer',exact:true}).click();assert.equal(await page.locator('#minutes').textContent(),'12');assert.equal(await page.locator('#seconds').textContent(),'34');

  await page.evaluate(()=>window.__focusDenied=true);await page.locator('#timer-play').click();await page.waitForFunction(()=>window.__calls.some(c=>c.name==='sync_windows_focus'&&c.args.endTime));await page.waitForTimeout(150);assert(!(await page.locator('#toast').textContent()).includes('Focus'));assert.equal(await page.locator('[data-widget=timer]').getAttribute('data-windows-focus'),'false');await page.locator('#timer-play').click();await page.evaluate(()=>window.__focusDenied=false);
  await page.getByRole('button',{name:'Set duration',exact:true}).click();await page.locator('#timer-min-input').fill('0');await page.locator('#timer-sec-input').fill('2');await page.getByRole('button',{name:'Set timer',exact:true}).click();await page.getByRole('button',{name:'Start timer',exact:true}).click();assert.equal(await page.locator('#timer-label').innerText(),'Focusing');
  await page.keyboard.press('Escape');await page.mouse.move(950,350);
  await page.waitForFunction(()=>document.querySelector('#timer-label').textContent!=='Focusing');await page.waitForTimeout(700);
  assert.equal(await page.locator('#notch').evaluate(e=>e.classList.contains('mascot-wink')),false,'the timer has no mascot animation for now');
  await page.evaluate(()=>window.__emit('play-intro'));await page.waitForSelector('#notch.mascot-wink');const noticeStart=Date.now();
  assert.equal(await page.locator('#notch').evaluate(e=>e.classList.contains('compact')),true);
  const pose=await page.locator('#eye-left').getAttribute('d');await page.waitForTimeout(650);assert.notEqual(await page.locator('#eye-left').getAttribute('d'),pose);assert(Math.abs(Number(await page.locator('#eye-left').getAttribute('stroke-width'))-9)<.2);assert(Number(await page.locator('#eye-right').getAttribute('stroke-width'))>12);
  assert.equal(await page.locator('#mascot').evaluate(e=>Number(getComputedStyle(e).opacity)),1);
  await page.screenshot({path:path.join(artifacts,'rev1104-timer-complete.png')});
  await page.waitForSelector('#notch:not(.mascot-wink)');const elapsed=Date.now()-noticeStart;assert(elapsed>=2900&&elapsed<=4100,elapsed);assert.equal(await page.locator('#timer-notice').isVisible(),false);
  await page.waitForTimeout(300);assert.equal(await page.locator('#mascot').evaluate(e=>Number(getComputedStyle(e).opacity)),1);assert.equal(await page.locator('#notch').evaluate(e=>e.classList.contains('compact')),true);
  await page.getByRole('button',{name:'Open Sora',exact:true}).click();
  console.log('PASS: full lyric text, equal seven-pixel gaps, brief smooth wink and restored mascot.');
  const widths=[];for(const tab of ['Home','Music','File tray']){await page.getByRole('button',{name:tab,exact:true}).click();await page.waitForTimeout(350);widths.push(await page.locator('#notch').evaluate(e=>e.getBoundingClientRect().width))}assert.equal(widths[0],widths[1]);assert.equal(widths[1],widths[2]);
  await page.getByRole('button',{name:'File tray',exact:true}).click();await page.waitForTimeout(350);await page.screenshot({path:path.join(artifacts,'rev14-tray.png')});
  await page.evaluate(()=>window.__TAURI__.core.invoke('shelf_pick'));await page.getByRole('button',{name:'Home',exact:true}).click();await page.getByRole('button',{name:'File tray',exact:true}).click();await page.waitForSelector('.file');await page.locator('.file').first().hover();await page.waitForTimeout(350);
  assert.equal(await page.locator('.file-reveal').count(),0);assert.equal(await page.locator('.editor-mark').count(),0);
  const geometry=await page.evaluate(()=>{const f=document.querySelector('.file').getBoundingClientRect(),d=document.querySelector('.dropzone').getBoundingClientRect(),n=document.querySelector('#notch').getBoundingClientRect();return {square:Math.abs(f.width-f.height),bottom:n.bottom-d.bottom,side:d.left-n.left}});assert(geometry.square<1);assert(Math.abs(geometry.bottom-geometry.side)<4);assert.equal(await page.locator('#notch').evaluate(e=>getComputedStyle(e).height),'234px');
  await page.waitForFunction(()=>document.querySelector('.native-thumbnail img')?.naturalHeight===128);
  const fit=await page.locator('.native-thumbnail').first().evaluate(slot=>{const img=slot.querySelector('img'),b=img.getBoundingClientRect(),s=slot.getBoundingClientRect(),label=slot.parentElement.querySelector('.file-name').getBoundingClientRect();return {bottom:b.bottom,bound:s.bottom,text:label.top,fit:getComputedStyle(img).objectFit}});
  assert(fit.fit==='contain'&&fit.bottom<=fit.bound+.5&&fit.bottom+2<fit.text,'Portrait preview must fit above its label: '+JSON.stringify(fit));
  await page.screenshot({path:path.join(artifacts,'rev18-tray.png')});
  assert.deepEqual(errors,[]);
  console.log('PASS: English UI, Poppins, restart hourglass, past-event rejection/future save, current-week-only hover, wider seek hit area, CJK typography, clickable lyrics seek exact timestamp, retained lyric DOM nodes, no lyric overflow or page errors.');
  await page.evaluate(()=>{const d=new Date(),date=d.getFullYear()+'-'+String(d.getMonth()+1).padStart(2,'0')+'-'+String(d.getDate()).padStart(2,'0');localStorage.setItem('nook.agenda',JSON.stringify(Array.from({length:5},(_,i)=>({id:'snap-'+i,title:'Event '+(i+1),date,time:(10+i)+':00'}))))});
  await page.reload();await page.getByRole('button',{name:'Open Sora',exact:true}).click();await page.waitForTimeout(400);await page.evaluate(()=>(document.querySelector('[data-widget=calendar]').dataset.size='wide',document.querySelector('[data-widget=timer]').hidden=true));await page.waitForTimeout(100);
  const visible=()=>page.locator('#agenda').evaluate(el=>{const box=el.getBoundingClientRect();return [...el.children].filter(e=>{const r=e.getBoundingClientRect();return r.top>=box.top-.5&&r.bottom<=box.bottom+.5}).map(e=>e.querySelector('.event-name')?.textContent)});
  assert.equal((await visible())[0],'Event 1','the list starts at the first item');
  await page.locator('#agenda').evaluate(el=>el.scrollTo({top:48,behavior:'smooth'}));await page.waitForTimeout(650);assert.equal((await visible())[0],'Event 2','one item per snap');
  await page.locator('#agenda').evaluate(el=>el.scrollTo({top:999,behavior:'smooth'}));await page.waitForTimeout(650);assert.equal((await visible()).at(-1),'Event 5');
  await page.screenshot({path:path.join(artifacts,'rev1102-events.png')});await page.evaluate(()=>(document.querySelector('[data-widget=calendar]').dataset.size='square',document.querySelector('[data-widget=timer]').hidden=false));
  await page.evaluate(()=>{window.__testMedia={title:'Pending test',artist:'Test',source:'Spotify',position:7,duration:100,playing:false,can_seek:true};const original=window.__TAURI__.core.invoke;window.__TAURI__.core.invoke=(name,args)=>name==='online_lyrics'?new Promise(()=>{}):original(name,args)});
  await page.getByRole('button',{name:'Music',exact:true}).click();await page.waitForTimeout(350);
  const loadingCentered=await page.locator('#lyrics-lines').evaluate(el=>{const a=el.getBoundingClientRect(),b=el.firstElementChild.getBoundingClientRect();return {text:el.textContent,offset:Math.abs((a.top+a.bottom-b.top-b.bottom)/2),align:getComputedStyle(el).textAlign}});assert.match(loadingCentered.text,/Finding lyrics/);assert(loadingCentered.offset<1);assert.equal(loadingCentered.align,'center');
  console.log('PASS: shared Home baseline, exactly two events, row snapping through final pair, centered lyric loading state.');
  const mp=await browser.newPage();await mp.goto('http://127.0.0.1:18766/mascot-test');await mp.setContent(`<main id="notch" class="compact playing"><button id="face"><svg id="mascot" viewBox="0 0 160 70"><g id="face-rig"><path id="eye-left"/><path id="eye-right"/><path id="mascot-mouth"/></g></svg></button></main><script type="module" src="http://127.0.0.1:18766/mascot.js"></script>`);
  await mp.waitForTimeout(4100);assert(Number(await mp.locator('#eye-left').getAttribute('stroke-width'))<9.5);
  const notes=await mp.locator('#music-notes g').count();assert(notes>0);
  await mp.locator('#face').dispatchEvent('pointerenter');await mp.waitForTimeout(450);assert(Number(await mp.locator('#eye-left').getAttribute('stroke-width'))>13.5);
  await mp.waitForTimeout(1600);assert(Number(await mp.locator('#eye-left').getAttribute('stroke-width'))>13.5);
  await mp.locator('#face').dispatchEvent('pointerleave');await mp.waitForTimeout(4000);assert(Number(await mp.locator('#eye-left').getAttribute('stroke-width'))>13.5);
  console.log('PASS: mascot opens within 450 ms, floating notes render, and five-second awake hold remains.');
  for(const playing of [false,true]){
   await mp.locator('#notch').evaluate((e,playing)=>{e.classList.toggle('playing',playing);e.classList.add('mascot-wink')},playing);
   const motion=await mp.evaluate(()=>new Promise(resolve=>{const samples=[],start=performance.now();function frame(){const eye=document.querySelector('#eye-left');samples.push([...eye.getAttribute('d').matchAll(/-?\d+(?:\.\d+)?(?:e[+-]?\d+)?/gi)].map(m=>Number(m[0])));if(performance.now()-start<650)requestAnimationFrame(frame);else resolve(samples)}frame()}));
   assert(motion.length>20);const last=motion.at(-1);assert(last[6]-last[0]>10);let maxStep=0;for(let i=1;i<motion.length;i++)for(let j=0;j<last.length;j++)maxStep=Math.max(maxStep,Math.abs(motion[i][j]-motion[i-1][j]));assert(maxStep<6,'Wink discontinuity '+maxStep);
   await mp.locator('#notch').evaluate(e=>e.classList.remove('mascot-wink'));await mp.waitForTimeout(650);assert(Number(await mp.locator('#eye-left').getAttribute('stroke-width'))>8);
  }
  console.log('PASS: smooth one-eye chevron transition from idle and music states.');
await mp.close();
  // A real drop can land between the first drag offer and the end of the morph.
  await page.evaluate(()=>window.__emit('panel-request',false));
  await page.waitForTimeout(350);
  const direct=await page.evaluate(async()=>{
    const n=document.querySelector('#notch'),contents=document.querySelector('#expanded');
    n.style.opacity='0';window.__emit('edge-mode',true);
    let releaseList;
    const invoke=window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke=(name,args)=>name==='shelf_list'?new Promise(resolve=>{releaseList=resolve}):invoke(name,args);
    window.__calls.length=0;
    // Start on Home so the pending shelf_list overlaps a newer shelf-updated event.
    document.querySelector('[data-tab=home]').click();
    window.__emit('drag-offer');window.__emit('shelf-hover',true);
    const frames=[];
    const file={name:'arrival.txt',path:'C:/sample/arrival.txt',size:42,available:true,directory:false};
    for(let i=0;i<45;i++){
      await new Promise(requestAnimationFrame);
      const shell=n.getBoundingClientRect(),opacity=Number(getComputedStyle(contents).opacity);
      frames.push({width:shell.width,opacity,view:n.dataset.view});
      if(i===3){window.__shelf=[file];window.__emit('drag-ended');window.__emit('shelf-updated',[file]);releaseList?.([])}
    }
    window.__TAURI__.core.invoke=invoke;
    return {frames,open:!n.classList.contains('compact'),dragging:n.classList.contains('dragging'),
      rows:[...document.querySelectorAll('.file-name')].map(e=>e.textContent),
      placements:window.__calls.filter(c=>c.name==='set_panel').map(c=>c.args.expanded)};
  });
  assert.deepEqual(direct.placements,[true]);assert.deepEqual(direct.rows,['arrival.txt']);
  assert(direct.open&&!direct.dragging&&direct.frames.every(f=>f.view==='shelf'));
  const lastWidth=direct.frames.at(-1).width;
  assert(direct.frames.some(f=>f.width>85&&f.width<lastWidth-10&&f.opacity>0.03&&f.opacity<1),'Tray contents and outline animate together');
  assert(direct.frames.every((f,i,a)=>!i||f.width>=a[i-1].width-1),'no collapse during drop');
  await page.evaluate(()=>{delete window.__shelf});
  await page.evaluate(()=>window.__emit('edge-mode',false));
  await page.evaluate(()=>window.__emit('panel-request',false));await page.waitForTimeout(700);
  // Date and time pickers in the notch's own style
  await page.getByRole('button',{name:'Open Sora',exact:true}).click();await page.waitForTimeout(700);
  await page.getByRole('button',{name:'Home',exact:true}).click();await page.locator('#add-event').click();await page.waitForTimeout(400);
  assert.equal(await page.locator('#event-date').evaluate(e=>getComputedStyle(e).opacity),'0','native date input only stores the value');
  await page.locator('#event-title').fill('');await page.getByRole('button',{name:'Save event',exact:true}).click();await page.waitForTimeout(350);
  assert.equal(await page.locator('#event-form').evaluate(f=>f.noValidate),true,'no browser validation bubble');
  assert.equal(await page.locator('#field-note').innerText(),'Please fill out this field');assert.equal(await page.locator('#field-note.show').count(),1,'themed note is shown');
  assert(await page.locator('#event-title').evaluate(e=>e.classList.contains('field-bad')),'the field is marked');
  await page.locator('#event-title').fill('Hi');await page.waitForTimeout(250);assert.equal(await page.locator('#field-note.show').count(),0,'the note leaves when typing');await page.locator('#event-title').fill('');
  // The New to-do panel holds the day and the time itself: the notch grows taller for it.
  assert(await page.evaluate(()=>document.getElementById('notch').getBoundingClientRect().height>300),'the panel is taller while a menu is open');
  const todayNum=await page.evaluate(()=>new Date().getDate());
  assert.equal(await page.locator('#pick-days .pk-day:disabled').count(),todayNum-1,'earlier days of this month are off');
  assert.equal(await page.locator('#pick-days .pk-day.sel').count(),1,'the chosen day is marked');
  const wantMonth=await page.evaluate(()=>{const d=new Date();d.setMonth(d.getMonth()+1,1);return d.getFullYear()+'-'+String(d.getMonth()+1).padStart(2,'0')+'-15'});
  await page.locator('#pick-next').click();await page.waitForTimeout(150);await page.locator('#pick-days .pk-day[data-day="15"]').click();await page.waitForTimeout(150);
  assert.equal(await page.locator('#event-date').inputValue(),wantMonth,'a tap on a day sets the date');
  await page.locator('#event-note').fill('Bring the report');assert.equal(await page.locator('#note-count').textContent(),'16/120','the note counts its letters');
  assert.equal(await page.locator('#event-editor [data-quick-day], #event-editor [data-quick-time]').count(),0,'the Quick picks are gone');
  await page.evaluate(()=>{for(const [id,n] of [['wheel-h',20],['wheel-m',30]]){const w=document.getElementById(id);w.scrollTop=n*(w.children[1].offsetTop-w.children[0].offsetTop)}});await page.waitForFunction(()=>document.querySelector('#time-title').textContent==='20:30',null,{timeout:4000}).catch(()=>{});
  const pickedTime=await page.locator('#time-title').innerText();assert.match(pickedTime,/^20:(29|30|31)$/,'the wheels land on the hour and (within one step of) the minute: '+pickedTime);
  assert.equal(await page.locator('#event-time').inputValue(),pickedTime,'turning the wheels sets the time');
  await page.evaluate(()=>{for(const [id,n] of [['wheel-h',9],['wheel-m',0]]){const w=document.getElementById(id);w.scrollTop=n*(w.children[1].offsetTop-w.children[0].offsetTop)}});await page.waitForFunction(()=>document.getElementById('event-time').value==='09:00',null,{timeout:4000});
  const pickerFit=await page.evaluate(()=>{const n=document.querySelector('#notch').getBoundingClientRect();return[...document.querySelectorAll('#event-editor .tile')].map(v=>v.getBoundingClientRect().bottom<=n.bottom+1)});
  assert(pickerFit.every(Boolean),'every tile fits in the taller notch');
  await page.locator('#event-title').fill('Picked');await page.getByRole('button',{name:'Save event',exact:true}).click();await page.waitForTimeout(300);
  console.log('PASS: New to-do panel: taller notch, day grid, note, wheels set the time.');
  // Timer buttons: a press that lasts across a clock tick (the display refreshes every 250 ms) still counts
  await page.locator('#event-cancel').click().catch(()=>{});await page.getByRole('button',{name:'Home',exact:true}).click();await page.waitForTimeout(400);
  const states=[];
  for(let i=0;i<6;i++){const box=await page.locator('#timer-play svg').boundingBox();await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.waitForTimeout(90+i*45);await page.mouse.up();await page.waitForTimeout(120);states.push(await page.locator('#timer-play').getAttribute('aria-label'))}
  assert.deepEqual(states,['Pause timer','Start timer','Pause timer','Start timer','Pause timer','Start timer'],'every press registered: '+states);
  {const before=Number(await page.locator('#minutes').textContent());for(let i=0;i<4;i++){await page.locator('#timer-dial-wrap').dispatchEvent('wheel',{deltaY:100});await page.waitForTimeout(60)}assert.equal(Number(await page.locator('#minutes').textContent()),Math.min(180,before+4),'four dial steps');
   const box=await page.locator('#timer-dial-wrap').boundingBox();await page.mouse.move(box.x+20,box.y+box.height/2);await page.mouse.down();await page.mouse.move(box.x+20+60,box.y+box.height/2,{steps:6});await page.mouse.up();assert(Number(await page.locator('#minutes').textContent())<before+4,'dragging right turns it back');}
  console.log('PASS: timer buttons register every press, even slow ones.');
  // Tray with many files: wheel and hold-drag move it sideways, a vertical drag still takes the file out
  await page.evaluate(()=>{window.__shelf=Array.from({length:24},(_,i)=>({path:'C:\t\f'+i+'.txt',name:'file-'+i+'.txt',size:100+i,directory:false,available:true}));window.__emit('shelf-updated',window.__shelf)});
  await page.waitForTimeout(800);
  const strip=page.locator('#files'),scrollLeft=()=>strip.evaluate(e=>e.scrollLeft);
  assert(await strip.evaluate(e=>e.scrollWidth>e.clientWidth+400),'tray overflows sideways');
  assert.equal(await strip.evaluate(e=>getComputedStyle(e).getPropertyValue('--fade-r').trim()),'18px','right edge fades while more files wait');
  const box=await strip.boundingBox();
  await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.wheel(0,500);await page.waitForTimeout(700);
  const byWheel=await scrollLeft();assert(byWheel>250,'wheel scrolls the tray sideways: '+byWheel);
  assert.equal(await strip.evaluate(e=>getComputedStyle(e).getPropertyValue('--fade-l').trim()),'18px','left edge fades after scrolling');
  await page.evaluate(()=>{window.__calls.length=0});
  const card=await page.locator('.file-open').nth(4).boundingBox();
  await page.mouse.move(card.x+card.width/2,card.y+card.height/2);await page.mouse.down();await page.mouse.move(card.x+card.width/2-120,card.y+card.height/2+6,{steps:10});await page.mouse.up();
  const byDrag=await scrollLeft();assert(byDrag>byWheel+80,'hold-drag slides the tray: '+byWheel+' -> '+byDrag);
  assert(!(await page.evaluate(()=>window.__calls.some(c=>c.name==='shelf_drag'||c.name==='shelf_open'))),'a sideways drag neither drags the file out nor opens it');
  const card2=await page.locator('.file-open').nth(5).boundingBox();
  await page.mouse.move(card2.x+card2.width/2,card2.y+card2.height/2);await page.mouse.down();await page.mouse.move(card2.x+card2.width/2+4,card2.y+card2.height/2+60,{steps:8});await page.mouse.up();await page.waitForTimeout(200);
  assert(await page.evaluate(()=>window.__calls.some(c=>c.name==='shelf_drag')),'a vertical drag still takes the file out');
  console.log('PASS: crowded tray scrolls with the wheel and by hold-drag; vertical drag still drags files out.');
  // Second drag: the open tray keeps waiting instead of collapsing and re-appearing; it never closes on a timer, only once you leave the tab
  await page.evaluate(()=>{window.__calls.length=0;window.__emit('drag-offer')});await page.waitForTimeout(500);
  const second=await page.evaluate(()=>({compact:document.querySelector('#notch').classList.contains('compact'),view:document.querySelector('#notch').dataset.view,dragging:document.querySelector('#notch').classList.contains('dragging'),collapses:window.__calls.filter(c=>c.name==='set_panel'&&c.args.expanded===false).length}));
  assert(!second.compact&&second.view==='shelf'&&second.dragging&&second.collapses===0,'an open tray is left alone by the next drag: '+JSON.stringify(second));
  await page.evaluate(()=>window.__emit('shelf-updated',window.__shelf));await page.waitForTimeout(300);
  assert(await page.evaluate(()=>window.__calls.some(c=>c.name==='tray_hold'&&c.args.ms>=10000)),'the drop asks Rust to ignore outside clicks for a while');
  await page.mouse.move(5,5);await page.waitForTimeout(13000);
  assert(!(await page.evaluate(()=>document.querySelector('#notch').classList.contains('compact'))),'tray still waits for the next file after 13 s');
  await page.evaluate(()=>{window.__calls.length=0});await page.click('[data-tab=home]');await page.waitForTimeout(200);
  assert(await page.evaluate(()=>window.__calls.some(c=>c.name==='tray_hold'&&c.args.ms===0)),'another tab ends the hold, so an outside click closes the panel again');
  await page.keyboard.press('Escape');await page.waitForTimeout(700);
  assert(await page.evaluate(()=>document.querySelector('#notch').classList.contains('compact')),'the panel still closes');
  // A fresh tray opens quickly from the capsule: short dwell, no eyes, straight into the tray view
  await page.evaluate(()=>{window.__emit('edge-mode',true);window.__emit('edge-hover',{active:true,ms:700,tray:true})});await page.waitForTimeout(950);
  const quick=await page.evaluate(()=>{const p=document.querySelector('#edge .edge-pill');return{w:p.getBoundingClientRect().width,eye:Number(getComputedStyle(p.querySelector('b')).opacity)}});
  assert(quick.w>200&&quick.eye===0,'fast hover grows the capsule without eyes: '+JSON.stringify(quick));
  await page.evaluate(()=>{window.__emit('edge-hover',{active:false,ms:0,done:true,tray:true});window.__emit('panel-peek');window.__emit('edge-mode',false)});await page.waitForTimeout(1300);
  assert(await page.evaluate(()=>{const n=document.querySelector('#notch');return !n.classList.contains('compact')&&n.dataset.view==='shelf'}),'fast hover lands directly in the tray view');
  console.log('PASS: second drag keeps the tray, it closes itself later, and a fresh tray opens fast from the capsule.');
  // Start-up: the notch drops in first, then the mascot pops in and greets; no listening pose or idle motion in between
  const boot=await browser.newPage({viewport:{width:600,height:300}});
  await boot.addInitScript(()=>{Object.defineProperty(navigator,'webdriver',{get:()=>false});const L={};window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,args={})=>{if(name==='set_panel'){await new Promise(r=>setTimeout(r,120));L['view-scale']?.({payload:.85});return null}if(name==='window_visible')return true;if(name==='shelf_list')return[];if(name==='media_status')return{title:'Song',artist:'Artist',album:'',playing:true,position:10,duration:200,playback_rate:1,can_seek:true,source:'Spotify.exe',artwork:null};if(name==='weather')return{current:{temperature_2m:27,apparent_temperature:29,relative_humidity_2m:70,weather_code:0,is_day:1}};return null}}};
   window.__boot=[];const t0=performance.now();const loop=()=>{const n=document.querySelector('#notch'),f=document.querySelector('#mascot');if(n&&f){const s=getComputedStyle(n);window.__boot.push({t:Math.round(performance.now()-t0),y:s.translate,o:Number(s.opacity),m:Number(getComputedStyle(f).opacity),w:n.classList.contains('mascot-wink'),calm:n.classList.contains('mascot-calm'),notes:Number(document.querySelector('#music-notes')?.getAttribute('opacity')||0),eye:Number(document.querySelector('#eye-right').getAttribute('stroke-width'))})}requestAnimationFrame(loop)};addEventListener('DOMContentLoaded',loop)});
  await boot.goto('http://127.0.0.1:18766');await boot.waitForTimeout(3000);
  const seq=await boot.evaluate(()=>window.__boot);
  const firstVisible=seq.find(s=>s.o>0.02),popAt=seq.find(s=>s.m>0.05&&s.o>0.02),firstWink=seq.find(s=>s.w);
  assert(firstVisible&&/-/.test(firstVisible.y),'the notch starts above the screen: '+JSON.stringify(firstVisible));
  assert(popAt&&popAt.o>=.97&&popAt.t-firstVisible.t>=350,'the mascot only appears once the notch has landed: '+JSON.stringify([firstVisible,popAt]));
  assert(firstWink&&Math.abs(firstWink.t-popAt.t)<120,'the greeting starts together with the pop: '+JSON.stringify([popAt,firstWink]));
  assert(seq.filter(s=>s.t>=firstVisible.t&&s.t<=popAt.t+3000).every(s=>s.calm),'calm for the whole greeting');
  assert(seq.filter(s=>s.t>popAt.t&&s.t<popAt.t+3000).every(s=>s.notes<.05),'no music notes during the greeting');
  assert(seq.filter(s=>s.t>popAt.t+300&&s.t<popAt.t+2000).every(s=>s.eye>12),'the right eye stays open (no closed-eye listening pose) during the greeting');
  await boot.close();
  // Start with Edge handle enabled and sample the real animation frames without moving the pointer.
  const modes=await browser.newPage({viewport:{width:680,height:260}});
  await modes.addInitScript(()=>{
    Object.defineProperty(navigator,'webdriver',{get:()=>false});
    const listeners={};window.__signal=(name,payload)=>listeners[name]?.({payload});
    window.__TAURI__={event:{listen:async(name,fn)=>{listeners[name]=fn}},core:{invoke:async name=>{
      if(name==='desktop_preferences')return {preferences:{edge_handle:true}};
      if(name==='window_visible')return true;
      if(name==='shelf_list')return [];
      if(name==='weather')return {current:{temperature_2m:20}};
      if(name==='set_panel')window.__signal('view-scale',.85);
      return null;
    }}};
    window.__frames=[];
    addEventListener('DOMContentLoaded',()=>{const record=()=>{const n=document.querySelector('#notch');
      if(n){const style=getComputedStyle(n);window.__frames.push({y:n.getBoundingClientRect().top,opacity:Number(style.opacity),edge:document.querySelector('#edge').classList.contains('handoff')})}
      requestAnimationFrame(record)};requestAnimationFrame(record)});
  });
  await modes.goto('http://127.0.0.1:18766');await modes.waitForTimeout(1500);
  const startup=await modes.evaluate(()=>window.__frames);
  assert(startup.some(f=>f.edge)&&startup.every(f=>f.opacity<.02||Math.abs(f.y)<1),'enabled startup stays at the edge');
  const measured=await modes.evaluate(async()=>{
    const n=document.querySelector('#notch'),edge=document.querySelector('#edge'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
    window.__signal('edge-mode',true);window.__frames.length=0;
    const start=performance.now();window.__signal('edge-hover',{active:true,ms:1430,tray:false});await sleep(1430);
    window.__signal('edge-hover',{active:false,done:true,tray:false});window.__signal('panel-peek');window.__signal('edge-mode',false);
    while(edge.classList.contains('handoff')&&performance.now()-start<2800)await new Promise(requestAnimationFrame);
    const elapsed=performance.now()-start,stayed=window.__frames.every(f=>f.opacity<.02||Math.abs(f.y)<1);
    window.__frames.length=0;window.__signal('edge-preference',false);window.__signal('panel-peek');await sleep(460);
    const slid=window.__frames.some(f=>f.opacity>.02&&f.opacity<.98&&f.y<-1);
    window.__frames.length=0;window.__signal('edge-preference',true);window.__signal('panel-peek');await sleep(460);
    return {elapsed,stayed,slid,again:window.__frames.every(f=>f.opacity<.02||Math.abs(f.y)<1),visible:Number(getComputedStyle(n).opacity)};
  });
  assert(measured.elapsed>=1950&&measured.elapsed<2250&&measured.stayed&&measured.slid&&measured.again&&measured.visible===1,JSON.stringify(measured));
  console.log('PASS: two-second hover and live Edge handle routing',measured);
  await modes.close();
  console.log('PASS: start-up drops the notch in and greets immediately.');
  // ---- Settings window ----
  const sw=await browser.newPage({viewport:{width:900,height:620}});const settingsErrors=[];sw.on('pageerror',e=>settingsErrors.push(e.message));
  await sw.addInitScript(()=>{const L={};window.__calls=[];window.__emit=(n,p)=>L[n]?.({payload:p});
   let startup=false,desk={shortcut:'Ctrl+Alt+Space',hide_maximized:false,game_mode:'off',edge_handle:true};
   window.__TAURI__={event:{listen:async(n,f)=>{L[n]=f;return()=>{}}},core:{invoke:async(name,args={})=>{window.__calls.push({name,args:JSON.parse(JSON.stringify(args))});
    if(name==='startup_enabled')return startup;if(name==='set_startup'){startup=args.enabled;return startup}
    if(name==='desktop_preferences')return{preferences:desk,shortcut_active:true};if(name==='set_desktop_preferences'){if(args.preferences.shortcut==='Ctrl+Alt+X')throw Error('Shortcut in use');desk=args.preferences;return{preferences:desk,shortcut_active:true}}
    if(name==='check_update')return{configured:true,available:true,current:'1.0.0',latest:'1.0.1',size:47400000};if(name==='apply_update'){window.__applied=true;return null}
    if(name==='notch_displays')return[{id:'primary',label:'Display 1 · 1920 × 1080',primary:true},{id:'\\\\.\\DISPLAY2',label:'Display 2 · 2560 × 1440',primary:false}];
    if(name==='app_version')return'1.0.0';return null}}}});
  await sw.goto('http://127.0.0.1:18766/settings.html');await sw.waitForTimeout(400);await sw.evaluate(()=>document.fonts.ready);
  assert.equal(await sw.locator('.tab').count(),5,'General, Notch, AI Agent, Meeting, Chat');assert.equal(await sw.locator('.traffic, .tl').count(),0,'no colored dots');assert.equal(await sw.locator('#w-max').count(),0,'no maximize button');{const c=await sw.locator('#w-close').boundingBox(),m=await sw.locator('#w-min').boundingBox();assert(c.x+c.width>=898&&c.y<=1&&m.x+m.width<=c.x+1&&c.width>=44,'Windows style: minimize and close in the top-right corner '+JSON.stringify([c,m]))}assert.equal(await sw.locator('.ti').first().evaluate(e=>getComputedStyle(e).backgroundColor),'rgba(0, 0, 0, 0)','plain line icons, no colored tiles');assert.equal(await sw.locator('#pane-title').textContent(),'General');assert.match(await sw.locator('#win').evaluate(e=>getComputedStyle(e).borderRadius),/14px/);
  await sw.waitForFunction(()=>!document.querySelector('#startup-toggle').disabled);
  await sw.getByRole('switch',{name:'Start with Windows'}).check();await sw.waitForTimeout(450);assert.equal(await sw.getByRole('switch',{name:'Start with Windows'}).evaluate(e=>getComputedStyle(e).backgroundColor),'rgb(255, 255, 255)','switches are white when on');assert.equal(await sw.evaluate(()=>window.__calls.filter(c=>c.name==='set_startup').at(-1).args.enabled),true);assert(await sw.getByRole('switch',{name:'Start with Windows'}).isChecked());
  await sw.screenshot({path:path.join(artifacts,'settings-general.png')});
  await sw.locator('#update-button').click();await sw.waitForFunction(()=>document.querySelector('#update-button').textContent==='Update to v1.0.1');
  await sw.locator('#update-button').click();await sw.waitForFunction(()=>window.__applied===true);await sw.evaluate(()=>window.__emit('update-progress',42));
  assert.equal(await sw.locator('#update-button').innerText(),'Downloading 42%');assert(await sw.locator('#update-button').isDisabled());
  await sw.locator('.tab[data-section=notch]').click();assert.equal(await sw.locator('#pane-title').textContent(),'Notch');await sw.waitForFunction(()=>!document.querySelector('#maximize-toggle').disabled);
  await sw.getByRole('switch',{name:'Hide when maximized'}).check();await sw.waitForFunction(()=>window.__calls.some(c=>c.name==='set_desktop_preferences'&&c.args.preferences.hide_maximized===true));
  await sw.locator('#game-mode-seg button[data-v=auto]').click();await sw.waitForFunction(()=>window.__calls.filter(c=>c.name==='set_desktop_preferences').at(-1).args.preferences.game_mode==='auto');
  assert.equal(await sw.locator('#game-mode-seg button.on').textContent(),'Auto');
  // Notch placement: defaults, one button per display, every save keeps the other fields.
  assert.equal(await sw.locator('#position-seg button.on').textContent(),'Center');assert.equal(await sw.locator('#size-input').inputValue(),'100');
  assert.deepEqual(await sw.locator('#display-seg button').allTextContents(),['Primary','Display 2']);assert.equal(await sw.locator('#display-seg button.on').textContent(),'Primary');
  await sw.locator('#position-seg button[data-v=right]').click();await sw.waitForFunction(()=>window.__calls.filter(c=>c.name==='set_desktop_preferences').at(-1).args.preferences.notch_position==='right');
  await sw.locator('#display-seg button').nth(1).click();await sw.waitForFunction(()=>window.__calls.filter(c=>c.name==='set_desktop_preferences').at(-1).args.preferences.notch_display==='\\\\.\\DISPLAY2');
  await sw.locator('#size-input').fill('40');await sw.waitForFunction(()=>{const p=window.__calls.filter(c=>c.name==='set_desktop_preferences').at(-1).args.preferences;return p.notch_size===40&&p.notch_position==='right'&&p.notch_display==='\\\\.\\DISPLAY2'&&p.game_mode==='auto'});
  assert.equal(await sw.locator('#position-seg button.on').textContent(),'Right');
  await sw.screenshot({path:path.join(artifacts,'settings-notch-placement.png')});
  await sw.locator('.tab[data-section=notch]').click();await sw.waitForFunction(()=>!document.querySelector('#maximize-toggle').disabled);
  await sw.getByRole('button',{name:'Record shortcut'}).click();await sw.keyboard.press('Alt+k');assert.match(await sw.locator('#desktop-status').innerText(),/Ctrl first/);await sw.keyboard.press('Escape');
  await sw.getByRole('button',{name:'Record shortcut'}).click();await sw.keyboard.press('Control+k');await sw.waitForFunction(()=>document.querySelector('#shortcut-record').textContent==='Ctrl+K');
  await sw.getByRole('button',{name:'Record shortcut'}).click();await sw.keyboard.press('Control+Alt+x');await sw.waitForFunction(()=>document.querySelector('#desktop-status').textContent.includes('Shortcut in use'));
  assert.equal(await sw.locator('#shortcut-record').innerText(),'Ctrl+K','a refused shortcut rolls back');
  await sw.locator('#filter').fill('hotkey');await sw.waitForTimeout(250);
  assert.equal(await sw.locator('#pane-title').textContent(),'Search results');assert.equal(await sw.locator('.row:visible').count(),1,'only the shortcut row matches');
  const found=await sw.locator('.row:visible strong').allTextContents();assert(found.includes('Shortcut'),String(found));
  await sw.screenshot({path:path.join(artifacts,'settings-filter.png')});
  await sw.locator('#filter').fill('qqqq');await sw.waitForTimeout(200);assert.equal(await sw.locator('#no-match').isVisible(),true);
  await sw.locator('#filter').fill('');await sw.waitForTimeout(200);assert.equal(await sw.locator('#pane-title').textContent(),'Notch');
  await sw.locator('#w-min').click();await sw.locator('#w-close').click();await sw.waitForFunction(()=>window.__calls.some(c=>c.name==='window_minimize')&&window.__calls.some(c=>c.name==='window_close'));
  assert.deepEqual(settingsErrors,[]);await sw.close();
  console.log('PASS: settings window (sections, search, startup, updates, notch preferences).');
  await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});





