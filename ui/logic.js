export function parseLrc(text, keepEmpty = false) {
  const lines = []; const offset = Number(text.match(/\[offset:([+-]?\d+)\]/i)?.[1] || 0) / 1000;
  for (const line of text.split(/\r?\n/)) {
    const matches = [...line.matchAll(/\[(\d+):(\d{2}(?:\.\d+)?)\]/g)];
    const lyric = line.replace(/\[[^\]]*\]/g, '').trim();
    for (const m of matches) if (lyric || keepEmpty) lines.push({ time: Math.max(0, Number(m[1]) * 60 + Number(m[2]) + offset), text: lyric });
  }
  return lines.sort((a, b) => a.time - b.time);
}
export function timerRemaining(timer, now = Date.now()) { return timer.end ? Math.max(0, Math.ceil((timer.end - now) / 1000)) : Math.max(0, timer.remaining); }
export function dateKey(date) { return `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`; }
export function formatTime(seconds) { const n = Math.max(0, Math.floor(seconds || 0)); return `${Math.floor(n/60)}:${String(n%60).padStart(2,'0')}`; }
export function fileSize(bytes) { return bytes >= 1048576 ? `${(bytes/1048576).toFixed(1)} MB` : bytes >= 1024 ? `${Math.round(bytes/1024)} KB` : `${bytes} B`; }
export function weatherLabel(code, day = true) {
  if (code === 0) return [day ? 'Clear sky' : 'Clear night', day ? 'sun' : 'moon'];
  if (code === 1) return ['Mostly clear', day ? 'sun' : 'moon'];
  if (code === 2) return ['Partly cloudy', 'cloud'];
  if (code === 3) return ['Cloudy', 'cloud'];
  if (code <= 48) return ['Foggy', 'cloud'];
  if (code <= 57) return ['Drizzle', 'rain'];
  if (code === 61 || code === 66 || code === 80) return ['Light rain', 'rain'];
  if (code <= 67 || (code >= 80 && code <= 82)) return ['Rain', 'rain'];
  if (code <= 86) return ['Snow', 'cloud'];
  return ['Thunderstorms', 'rain'];
}
/** The weather widget's colour family for a WMO code, with -day or -night: clear, partly, cloudy, fog, rain, snow, storm. */
export function skyOf(code, day = true) {
  const sky = code <= 1 ? 'clear' : code === 2 ? 'partly' : code === 3 ? 'cloudy' : code <= 48 ? 'fog' : code <= 67 || (code >= 80 && code <= 82) ? 'rain' : code <= 86 ? 'snow' : 'storm';
  return sky + (day ? '-day' : '-night');
}
/** The days of a month (month 0-11) in calendar rows, Sunday first: null for the blanks before the 1st. */
export function monthDays(year, month) {
  const first = new Date(year, month, 1).getDay(), count = new Date(year, month + 1, 0).getDate();
  return [...Array(first).fill(null), ...Array.from({ length: count }, (_, i) => new Date(year, month, i + 1))];
}

export function isFutureEvent(date,time,now=Date.now()){
 if(!/^\d{4}-\d{2}-\d{2}$/.test(date)||!/^\d{2}:\d{2}$/.test(time))return false;
 const value=new Date(date+'T'+time+':00');return Number.isFinite(value.getTime())&&dateKey(value)===date&&value.getHours()===Number(time.slice(0,2))&&value.getMinutes()===Number(time.slice(3))&&value.getTime()>now;
}

export function lyricChunks(text, limit=30) {
 const value=text.trim();if(!value)return [''];
 const tokens=/[\u3040-\u30ff\u3400-\u9fff]/u.test(value)?Array.from(new Intl.Segmenter(undefined,{granularity:'word'}).segment(value),x=>x.segment):value.match(/\S+|\s+/gu);
 const parts=[];let line='';for(const token of tokens){if(line.trim()&&(line+token).trim().length>limit&&token.trim()){parts.push(line.trim());line=token}else line+=token}if(line.trim())parts.push(line.trim());return parts;
}

// Parentheses are a presentation hint, not inferred audio or invented timing.
export function vocalParts(text){
 const parts=[];let start=0,depth=0,open=-1;
 for(let i=0;i<text.length;i++){const c=text[i];if(c==='('||c==='（'){if(depth++===0){open=i;if(i>start)parts.push({text:text.slice(start,i).trim(),backing:false})}}else if((c===')'||c==='）')&&depth>0&&--depth===0){parts.push({text:text.slice(open,i+1),backing:true});start=i+1;open=-1}}
 if(depth>0)parts.push({text:text.slice(open).trim(),backing:false});else if(start<text.length)parts.push({text:text.slice(start).trim(),backing:false});
 return parts.filter(p=>p.text.length).length?parts.filter(p=>p.text.length):[{text:'',backing:false}];
}

// One visual section per provider timestamp, regardless of wrapping or backing vocals.
export function lyricSections(lines){
 const sections=[];lines.forEach((line,source)=>{const previous=sections.at(-1);if(previous&&previous.time===line.time){if(line.text)previous.text=[previous.text,line.text].filter(Boolean).join(' ');previous.source=source;}else sections.push({...line,source})});return sections;
}

// Smooth small polling jitter; seeks, pauses and session changes snap immediately.
export function reconcileMedia(previous, fresh, now){
 if(!previous||!previous.playing||!fresh.playing||previous.source!==fresh.source||previous.title!==fresh.title||previous.artist!==fresh.artist)return fresh;
 const predicted=previous.position+Math.max(0,now-previous.sampled)/1000*(previous.playback_rate||1);
 const delta=fresh.position-predicted;
 if(Number.isFinite(delta)&&Math.abs(delta)<.3)fresh.position=Math.max(0,Math.min(fresh.duration,predicted+delta*.3));
 return fresh;
}

// Ticked-off events stay (struck through) for a day after they were ticked, then go. An older tick without a time starts its day now.
// Events left unticked go once their day is over.
export const DONE_KEEP_MS=24*60*60*1000;
export function expireDone(events,now=Date.now()){
  let changed=false;const today=dateKey(new Date(now));
  const kept=events.filter(e=>{if(!e.done){const keep=!(typeof e.date==='string'&&e.date<today);if(!keep)changed=true;return keep}if(!Number.isFinite(e.doneAt)){e.doneAt=now;changed=true}const keep=now-e.doneAt<DONE_KEEP_MS;if(!keep)changed=true;return keep});
  return {events:kept,changed};
}
/** The next `days` rings of a daily alarm at "HH:MM", in ms, after `now`. */
export function alarmTimes(time, now = Date.now(), days = 7) {
  const [h, m] = String(time).split(':').map(Number), out = [];
  if (!(h >= 0 && h < 24 && m >= 0 && m < 60)) return out;
  const d = new Date(now); d.setHours(h, m, 0, 0);
  if (d.getTime() <= now) d.setDate(d.getDate() + 1);
  for (let i = 0; i < days; i++) { out.push(d.getTime()); d.setDate(d.getDate() + 1); }
  return out;
}
/** When a to-do is due, in a few words: "14:30" today, "Overdue", "Tomorrow", a weekday within the week, else "12 Nov". */
export function dueLabel(date, time, now = new Date()) {
  const at = new Date(date + 'T' + time), today = dateKey(now), tomorrow = new Date(now); tomorrow.setDate(now.getDate() + 1);
  if (at.getTime() < now.getTime()) return 'Overdue';
  if (date === today) return time;
  if (date === dateKey(tomorrow)) return 'Tomorrow';
  return (at - now) < 6 * 864e5 ? at.toLocaleDateString('en-US', { weekday: 'short' }) : at.toLocaleDateString('en-GB', { day: 'numeric', month: 'short' });
}
