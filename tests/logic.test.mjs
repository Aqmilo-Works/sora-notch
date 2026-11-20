import {test} from 'node:test';
import assert from 'node:assert/strict';
import {parseLrc,timerRemaining,dateKey,formatTime,isFutureEvent,expireDone,DONE_KEEP_MS} from '../ui/logic.js';
test('ticked events stay a day after the tick, then go',()=>{
  const now=1_000_000_000;
  const r=expireDone([{id:'open'},{id:'fresh',done:true,doneAt:now-1000},{id:'old',done:true,doneAt:now-DONE_KEEP_MS},{id:'legacy',done:true}],now);
  assert.deepEqual(r.events.map(e=>e.id),['open','fresh','legacy'],'only the day-old tick is removed');
  assert.equal(r.events[2].doneAt,now,'a tick without a time starts its day now');
  assert.equal(r.changed,true);
  assert.equal(expireDone([{id:'open'},{id:'fresh',done:true,doneAt:now}],now).changed,false,'nothing to save when nothing changed');
});
test('timer uses wall-clock deadlines after sleep, never negative',()=>{
  assert.equal(timerRemaining({end:10000,remaining:90},8500),2);
  assert.equal(timerRemaining({end:10000,remaining:90},11000),0);
  assert.equal(timerRemaining({end:null,remaining:90},11000),90);
});
test('LRC preserves repeated timestamps and fractional seconds',()=>{
  assert.deepEqual(parseLrc('[ar:Someone]\n[00:01.50][01:02.25]Original line'),[{time:1.5,text:'Original line'},{time:62.25,text:'Original line'}]);
});
test('LRC applies offset, clamps negative times and orders lines',()=>{
  assert.deepEqual(parseLrc('[offset:-2000]\n[00:05.00]later\n[00:01.00]first'),[{time:0,text:'first'},{time:3,text:'later'}]);
});
test('plain text is not treated as synchronized lyrics',()=>assert.deepEqual(parseLrc('no timestamps\n[ar:Name]'),[]));
test('local dates do not shift via UTC conversion',()=>assert.equal(dateKey(new Date(2026,9,29,0,1)),'2026-10-29'));
test('media time formatting clamps invalid negative positions',()=>{
  assert.equal(formatTime(-2),'0:00');assert.equal(formatTime(65.8),'1:05');assert.equal(formatTime(undefined),'0:00');
});

test('events reject past times, exact-now, malformed dates, and invalid local dates',()=>{
 const now=new Date(2026,9,29,14,30).getTime();
 assert.equal(isFutureEvent('2026-10-29','14:29',now),false);
 assert.equal(isFutureEvent('2026-10-29','14:30',now),false);
 assert.equal(isFutureEvent('2026-10-28','23:59',now),false);
 assert.equal(isFutureEvent('2026-02-30','09:00',now),false);
 assert.equal(isFutureEvent('2026-10-30','24:00',now),false);
 assert.equal(isFutureEvent('2026-10-29','14:31',now),true);
 assert.equal(isFutureEvent('2026-10-30','00:00',now),true);
});

test('lyric chunks preserve words and timed blank instrumental cues',async()=>{
 const {lyricChunks}=await import('../ui/logic.js');const text='One original sentence with many words to split neatly across several short rows';assert.equal(lyricChunks(text).join(' '),text);assert(lyricChunks(text).every(s=>s.length<=30));assert.deepEqual(parseLrc('[00:00]\n[00:10]Words',true),[{time:0,text:''},{time:10,text:'Words'}]);
});

test('backing vocals keep text, nesting, full-width parentheses and malformed input',async()=>{
 const {vocalParts}=await import('../ui/logic.js');assert.deepEqual(vocalParts('I stay (with you)'),[{text:'I stay',backing:false},{text:'(with you)',backing:true}]);assert.deepEqual(vocalParts('A (B (C)) D'),[{text:'A',backing:false},{text:'(B (C))',backing:true},{text:'D',backing:false}]);assert.deepEqual(vocalParts('A (unfinished'),[{text:'A',backing:false},{text:'(unfinished',backing:false}]);assert.equal(vocalParts('A（B）')[1].backing,true);assert.equal(vocalParts('(echo)')[0].backing,true);
});

test('word wrapping never splits Latin or Korean words',async()=>{const {lyricChunks}=await import('../ui/logic.js');assert.deepEqual(lyricChunks('Aku cinta kamu',8),['Aku','cinta','kamu']);assert.deepEqual(lyricChunks('extraordinary affection',5),['extraordinary','affection']);assert.deepEqual(lyricChunks('사랑합니다 언제나',3),['사랑합니다','언제나']);});

test('each lyric section has its own timestamp, never word fragments',async()=>{const {lyricSections}=await import('../ui/logic.js');const lines=[{time:0,text:'Previous'},{time:3,text:'A long current line with words that wrap (echo)'},{time:3,text:'A second voice'},{time:7,text:'Next'}];const rows=lyricSections(lines);assert.deepEqual(rows.map(r=>r.time),[0,3,7]);assert.equal(rows[1].source,2);assert.equal(rows[1].text,'A long current line with words that wrap (echo) A second voice');assert.equal(rows[2].text,'Next');});
import {reconcileMedia} from '../ui/logic.js';
test('media clock smooths small jitter but preserves seek, pause and player changes',()=>{
 const prior={source:'Spotify',title:'Song',artist:'Artist',position:10,sampled:1000,playing:true,playback_rate:1,duration:100};
 const next={...prior,position:11.1};reconcileMedia(prior,next,2000);assert(Math.abs(next.position-11.03)<.001);
 const seek={...prior,position:40};assert.equal(reconcileMedia(prior,seek,2000).position,40);
 const paused={...prior,position:11.1,playing:false};assert.equal(reconcileMedia(prior,paused,2000).position,11.1);
 const other={...prior,source:'Browser',position:11.1};assert.equal(reconcileMedia(prior,other,2000).position,11.1);
 const fast={...prior,playback_rate:2};assert.equal(reconcileMedia(fast,{...fast,position:12},2000).position,12);
});
test('unticked events go once their day is over',()=>{
  const now=new Date(2026,10,3,9,0).getTime();
  const r=expireDone([{id:'yesterday',date:'2026-11-02'},{id:'today',date:'2026-11-03'},{id:'tomorrow',date:'2026-11-04'},{id:'ticked-yesterday',date:'2026-11-02',done:true,doneAt:now-3600e3}],now);
  assert.deepEqual(r.events.map(e=>e.id),['today','tomorrow','ticked-yesterday'],'a ticked one keeps its 24 hours');
});

import {cleanOrder,cleanSizes,toggle,resize,DEFAULT} from '../ui/home-widgets.js';
test('Home widgets: three slots, only the Calendar may be wide (two slots)',()=>{
 assert.deepEqual(cleanSizes({calendar:'wide',weather:'wide',timer:'huge',music:'wide'}),{calendar:'wide'},'only the Calendar may be wide');
 assert.deepEqual(cleanOrder(['music','gamble','weather']),['weather','calendar','timer'],'retired Now Playing and Lucky leave their place to defaults');
 assert.equal(toggle(['calendar','timer'],'prayer',{calendar:'wide'}),null,'a wide Calendar and one more fill Home');
 assert.deepEqual(resize(['calendar','todo'],{},'calendar'),{calendar:'wide'});
 assert.equal(resize(['calendar','todo','timer'],{},'calendar'),null,'no room to grow');
 assert.equal(resize(['weather','todo'],{},'weather'),null,'other widgets have one size');
 assert.deepEqual(cleanOrder(['calendar','todo','timer'],{calendar:'wide'}),DEFAULT,'a saved Home that no longer fits falls back');
 assert.deepEqual(cleanOrder(null),DEFAULT,'nothing saved: the classic three');
 assert.deepEqual(cleanOrder(['water','weather','water','clock','nope']),['water','weather','calendar'],'unknown (and retired) and repeated ids go, a default fills the gap');
 assert.deepEqual(cleanOrder(['prayer']),['prayer'],'a Home the person emptied on purpose stays so');
 assert.deepEqual(cleanOrder(['weather','calendar','timer','prayer']),DEFAULT,'more than three: back to the default');
 assert.equal(toggle(['weather','calendar','timer'],'prayer'),null,'a full row refuses another widget');
 assert.deepEqual(toggle(['weather','calendar'],'screen'),['weather','calendar','screen']);
 assert.deepEqual(toggle(['weather','calendar','water'],'calendar'),['weather','water'],'toggling a shown widget removes it');
});
import {monthDays,skyOf} from '../ui/logic.js';
test('the calendar lays out a whole month from Sunday, and the weather card picks its sky',()=>{
 const nov=monthDays(2026,10);assert.equal(nov[0].getDate(),1,'1 Nov 2026 is a Sunday: no blanks');assert.equal(nov.length,30);
 const feb=monthDays(2027,1);assert.equal(feb.filter(d=>!d).length,1,'1 Feb 2027 is a Monday');assert.equal(feb.length-1,28);
 assert.equal(monthDays(2028,1).filter(Boolean).length,29,'leap year');
 assert.deepEqual([skyOf(0),skyOf(2,false),skyOf(3,false),skyOf(45),skyOf(61),skyOf(81,false),skyOf(73),skyOf(95)],['clear-day','partly-night','cloudy-night','fog-day','rain-day','rain-night','snow-day','storm-day']);
});
import {duration,nextStep} from '../ui/home-cards.js';
import {nextPrayer,timesFor} from '../ui/prayer.js';
import {alarmTimes,dueLabel} from '../ui/logic.js';
import {parseSymbols,marketName} from '../ui/home-cards.js';
test('Screen Time, Water, Qibla and Prayer helpers',()=>{
 assert.deepEqual([duration(0),duration(30),duration(125),duration(4000)],['0m','<1m','2m','1h 07m']);
 assert.deepEqual([nextStep(30),nextStep(120),nextStep(undefined)],[45,30,30],'the interval cycles and starts over');
 const jakarta=[-6.2,106.8];
 const t=timesFor(new Date(2026,10,7,12),...jakarta,'kemenag','shafi');
 assert(t.fajr<t.dhuhr&&t.dhuhr<t.asr&&t.asr<t.maghrib&&t.maghrib<t.isha,'in order');
 assert.equal(nextPrayer(new Date(2026,10,7,23,0),...jakarta,'kemenag','shafi').name,'fajr','after Isha comes the next Fajr');
});

test('Alarm rings, to-do due labels and Markets symbols',()=>{
 const now=new Date(2026,10,7,9,0).getTime();
 const t=alarmTimes('08:30',now,3);assert.equal(t.length,3);assert.equal(new Date(t[0]).getDate(),8,'08:30 has passed today: tomorrow first');assert.equal(new Date(t[1]).getDate(),9);
 assert.equal(new Date(alarmTimes('21:15',now,1)[0]).getDate(),7,'later today');assert.deepEqual(alarmTimes('25:00',now),[],'not a time');
 const n=new Date(2026,10,7,12,0);
 assert.deepEqual([dueLabel('2026-11-07','09:00',n),dueLabel('2026-11-07','15:30',n),dueLabel('2026-11-08','09:00',n),dueLabel('2026-11-10','09:00',n),dueLabel('2026-12-01','09:00',n)],['Overdue','15:30','Tomorrow','Tue','1 Dec']);
 assert.deepEqual(parseSymbols(' btc-usd,gc=f  ^jkse '),['BTC-USD','GC=F','^JKSE']);assert.equal(parseSymbols('a b c d e f g h i'),null,'eight at most');
 assert.equal(parseSymbols('<script>'),null);assert.deepEqual([marketName('GC=F'),marketName('BBCA.JK'),marketName('SOL-USD')],['Gold','BBCA','SOL']);
});
