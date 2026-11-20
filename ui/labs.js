// Labs tabs in the panel (docs/sora): Agents and Meeting. Each tab shows only when its module is on in Settings >
// Labs. Data and every check live in Rust (agents.rs, meeting.rs, local_ai.rs); this file draws and asks.
// Every tab fits the normal panel; only reading a transcript or summary opens the tall one. One main action per screen.
// Pictures carry meaning: the meeting app's own logo, the live sound as a waveform. No classic controls.
import {createAi} from './agents.js';

const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };
const ICON = {
  back: '<path d="m14.5 6-6 6 6 6"/>', more: '<circle cx="5" cy="12" r="1.9"/><circle cx="12" cy="12" r="1.9"/><circle cx="19" cy="12" r="1.9"/>',
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"/>',
  micoff: '<path d="M15 10V6a3 3 0 0 0-5.7-1.3M9 9v2a3 3 0 0 0 4.8 2.4M5.5 11a6.5 6.5 0 0 0 10.8 4.9M18.5 11c0 .9-.2 1.7-.5 2.5M12 17.5V21M4 4l16 16"/>',
  pause: '<path d="M9 6v12M15 6v12"/>', play: '<path d="M8 5.5v13l10-6.5Z"/>',
  plus: '<path d="M12 5v14M5 12h14"/>', copy: '<rect x="8" y="8" width="11" height="11" rx="2"/><path d="M5 15V6a1 1 0 0 1 1-1h9"/>',
  check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>', todo: '<circle cx="12" cy="12" r="7.5"/><path d="m8.8 12.2 2.2 2.2 4.4-4.6"/>', ask: '<path d="M9.2 9.3a3 3 0 1 1 4.3 2.7c-.9.4-1.5 1.1-1.5 2v.6M12 17.6v.2"/>',
  spark: '<path d="M12 3.5l1.7 4.6 4.8 1.9-4.8 1.9L12 16.5l-1.7-4.6-4.8-1.9 4.8-1.9ZM18.5 15l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8Z"/>',
  speaker: '<path d="M5 9.5h3l4.5-3.5v12L8 14.5H5ZM16 9a4 4 0 0 1 0 6M18.5 6.5a7.5 7.5 0 0 1 0 11"/>', eye: '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="2.8"/>',
  wave: '<path d="M4.5 10v4M8.25 7v10M12 4v16M15.75 7v10M19.5 10v4"/>',
  person: '<circle cx="12" cy="8.5" r="3.5"/><path d="M5 19.5c1.2-3.3 4-5 7-5s5.8 1.7 7 5"/>',
  calendar: '<rect x="4.5" y="5.5" width="15" height="14" rx="2.5"/><path d="M4.5 10h15M8.5 3.5v4M15.5 3.5v4"/>',
};
const svg = name => `<svg viewBox="0 0 24 24" aria-hidden="true">${ICON[name]}</svg>`;
const icon = (name, cls = '') => { const s = el('span', 'lb-glyph ' + cls); s.innerHTML = svg(name); return s; };
const button = (cls, text, onClick) => { const b = el('button', cls, text); b.type = 'button'; b.onclick = onClick; return b; };
const iconButton = (name, label, onClick, cls = '') => { const b = button('lb-icon ' + cls, null, onClick); b.innerHTML = svg(name); b.setAttribute('aria-label', label); return b; };
const clock = s => { s = Math.max(0, Math.round(s || 0)); const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), r = s % 60; return (h ? h + ':' + String(m).padStart(2, '0') : m) + ':' + String(r).padStart(2, '0'); };
const mmss = s => { s = Math.max(0, Math.floor(s)); return String(Math.floor(s / 60)).padStart(2, '0') + ':' + String(s % 60).padStart(2, '0'); };
const day = ms => ms ? new Date(ms).toLocaleString('en-GB', {day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit'}) : '';
const msg = e => String(e?.message || e || 'Something went wrong.');
// "me", "others", "p1"… as the name shown above a line, or the name given to that speaker.
const speakerName = (w, names) => names?.[w] || (w === 'me' ? 'You' : w === 'others' ? 'Others' : /^p\d+$/.test(w || '') ? 'Person ' + w.slice(1) : '');
// Brand marks (Simple Icons, CC0; see logos/SOURCES.txt): the mark in white on the brand's own colour.
const BRANDS = {
  zoom: ['#0b5cff', 'zoom'], 'microsoft teams': ['#5b5fc7', 'microsoftteams'], webex: ['#0a7f9c', 'webex'], slack: ['#4a154b', 'slack'], lark: ['#3370ff', ''],
  chrome: ['#1a73e8', 'chrome'], edge: ['#0c6fbf', 'edge'], firefox: ['#e25920', 'firefox'], brave: ['#fb542b', 'brave'], opera: ['#d4141e', 'opera'], vivaldi: ['#d63a3a', 'vivaldi'],
  claude: ['#c96442', 'app:Claude'], openai: ['#0f8f6f', 'openai'], gemini: ['#2f6fdb', 'app:Gemini'], antigravity: ['#2f6fdb', 'app:Gemini'], qwen: ['#615ced', 'app:Qwen'],
  copilot: ['#5b5bd6', 'app:Copilot'],
};
// Meeting apps in their own colours (see logos/SOURCES.txt), not a white mark on a tile.
const LOGOS = {'google meet': 'logos/color/googlemeet.png', zoom: 'logos/color/zoom.png', 'microsoft teams': 'logos/color/teams.png', slack: 'logos/color/slack.png', webex: 'logos/color/webex.png', lark: 'logos/color/lark.png',
  chrome: 'logos/color/chrome.png', edge: 'logos/color/edge.png', firefox: 'logos/color/firefox.png', brave: 'logos/color/brave.png', opera: 'logos/color/opera.png', vivaldi: 'logos/color/vivaldi.png'};
function mark(name, size = 'm', fallbackIcon = '') {
  const key = String(name || '').toLowerCase(), b = BRANDS[key];
  if (LOGOS[key]) { const t = el('span', 'lb-mark logo ' + size); const img = el('img'); img.src = LOGOS[key]; img.alt = ''; img.draggable = false; t.append(img); t.setAttribute('aria-hidden', 'true'); return t; }
  const t = el('span', 'lb-mark ' + size);
  t.style.setProperty('--c', b?.[0] || '#2c2c2e');
  const src = b?.[1]?.startsWith('app:') ? APP_MARKS[b[1].slice(4)] : b?.[1] ? `logos/${b[1]}.svg` : '';
  if (src) { const m = el('i'); m.style.setProperty('--m', `url("${src}")`); t.append(m); }
  else if (fallbackIcon) t.append(icon(fallbackIcon));
  else t.append(el('b', '', (name || '?').replace(/[^A-Za-z0-9]/g, '').slice(0, 1).toUpperCase() || '?'));
  t.setAttribute('aria-hidden', 'true');
  return t;
}
/** Each AI agent shown with its maker's mark, as the chat apps are: Claude Code and Claude with Claude's, Codex with
 * OpenAI's, Gemini CLI and Antigravity with Gemini's. One without a mark here (OpenCode, Cursor, Amp...) shows its initial. */
const AGENT_BRAND = {'claude-code': 'claude', 'claude-desktop': 'claude', codex: 'openai', gemini: 'gemini', antigravity: 'antigravity', copilot: 'copilot', qwen: 'qwen'};
/** Marks Sora already carries for chat and AI apps (notch_ui MARKS), as SVG data for the same tiles. */
const APP_MARKS = {};

export function initLabs({invoke, listen, $, toast, showError}) {
  const notch = $('notch');
  const flags = {agents: false, meeting: false, local_ai: false, store: false};
  let current = '';
  function showTabs() { for (const id of ['history-button', 'history-clear']) { const b = $(id); if (b && flags.store) b.style.display = 'none'; } for (const t of ['agents', 'meeting']) { const b = document.querySelector(`nav [data-tab="${t}"]`); if (b) b.hidden = !flags[t]; } }
  invoke('labs_state').then(f => { Object.assign(flags, f || {}); showTabs(); }).catch(() => {});
  invoke('app_marks').then(list => { for (const m of list || []) APP_MARKS[m.name] = 'data:image/svg+xml,' + encodeURIComponent(`<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'><path d='${m.path}'/></svg>`); if (current === 'agents') ai.enter(); }).catch(() => {});
  listen('labs-changed', e => { Object.assign(flags, e.payload || {}); showTabs(); if (current) entered(current); });

  let menuEl = null;
  const closeMenu = () => { menuEl?.remove(); menuEl = null; };
  function menu(anchor, items) {
    closeMenu();
    menuEl = el('div', 'fx-menu'); menuEl.setAttribute('role', 'menu');
    for (const it of items) { if (!it) { menuEl.append(el('hr')); continue; } const b = button(it.danger ? 'danger' : '', it.label, ev => { ev.stopPropagation(); closeMenu(); it.run(); }); b.setAttribute('role', 'menuitem'); menuEl.append(b); }
    notch.append(menuEl);
    const r = notch.getBoundingClientRect(), a = anchor.getBoundingClientRect(), k = r.width / notch.offsetWidth || 1;
    menuEl.style.left = Math.max(6, Math.min(notch.offsetWidth - menuEl.offsetWidth - 6, (a.right - r.left) / k - menuEl.offsetWidth)) + 'px';
    menuEl.style.top = Math.min(notch.offsetHeight - menuEl.offsetHeight - 6, (a.bottom - r.top) / k + 4) + 'px';
    menuEl.querySelector('button')?.focus();
  }
  document.addEventListener('pointerdown', e => { if (menuEl && !menuEl.contains(e.target)) closeMenu(); }, true);
  document.addEventListener('keydown', e => { if (e.key === 'Escape' && menuEl) { closeMenu(); e.stopPropagation(); } }, true);

  let tall = false;
  function setTall(on) {
    tall = on; notch.dataset.view = on ? 'meeting-detail' : (current || 'meeting');
    invoke('panel_view', {name: on ? 'meeting-detail' : (current || 'meeting')}).catch(() => {});
  }
  listen('panel-show', () => { if (tall && current === 'meeting') setTimeout(() => setTall(true), 60); });

  // ================= Agents
  // An agent's question gets a taller panel (agents-question), just as tall as the question needs (normal height up to
  // 384): its card is measured once laid out, and the page and the notch take the same height.
  const NORMAL = 234, TALLEST = 384;
  let agentsTall = 0;
  function setAgentsTall(card) {
    if (current !== 'agents') return;
    if (!card) { if (!agentsTall) return; agentsTall = 0; notch.dataset.view = 'agents'; notch.style.removeProperty('--q-h'); invoke('panel_view', {name: 'agents'}).catch(() => {}); return; }
    requestAnimationFrame(() => {
      const gap = parseFloat(getComputedStyle(card).rowGap) || 0, kids = [...card.children];
      const need = kids.reduce((h, c) => h + c.offsetHeight, 0) + gap * Math.max(0, kids.length - 1);
      const h = Math.round(Math.min(TALLEST, Math.max(NORMAL, notch.offsetHeight - card.offsetHeight + need)));
      if (h === agentsTall) return;
      agentsTall = h; notch.style.setProperty('--q-h', h + 'px'); notch.dataset.view = 'agents-question';
      invoke('panel_view', {name: 'agents-question'}).then(() => invoke('panel_tall', {px: h - NORMAL})).catch(() => {});
    });
  }
  listen('panel-show', () => { if (agentsTall && current === 'agents') setTimeout(() => { const h = agentsTall; invoke('panel_view', {name: 'agents-question'}).then(() => invoke('panel_tall', {px: h - NORMAL})).catch(() => {}); }, 60); });
  const ai = createAi({invoke, notify: m => toast(m), onKind: (k, card) => setAgentsTall(k === 'question' ? card : null), mark: agent => mark(AGENT_BRAND[agent] || agent, 's')});
  async function loadAgents() { try { ai.setPending(await invoke('agent_pending')); } catch {} try { ai.setSessions(await invoke('agents_state')); } catch {} try { ai.setConnected(Object.values(await invoke('agent_hooks') || {}).some(Boolean)); } catch { ai.setConnected(false); } loadUsage(); }
  // Plan usage asks Codex at most once a minute (Rust side); Claude's numbers come with Claude Code's replies.
  async function loadUsage() { try { ai.setUsage(await invoke('agent_usage')); } catch {} }
  listen('agents-usage', () => loadUsage());
  listen('agents-drop', e => ai.drop(e.payload || {}));
  listen('agents-updated', e => ai.setSessions(e.payload || []));
  listen('agents-pending', e => ai.setPending(e.payload || []));

  // ================= Meeting
  const mt = {scan: null, picked: null, mic: false, state: {state: 'idle'}, items: [], open: null, detail: undefined, ai: null, view: 'summary', query: '', confirm: null, consent: true, levels: []};
  const box = $('meeting');
  const BARS = 44;
  async function scanMeetings() {
    try { mt.scan = await invoke('meeting_scan'); if (!mt.scan.candidates.some(c => c.pid === mt.picked)) mt.picked = mt.scan.pick ?? mt.scan.candidates[0]?.pid ?? null; } catch (e) { mt.scan = {error: msg(e), candidates: []}; }
    if (!mt.open && !live()) drawMeeting();
  }
  async function loadMeetings() {
    try { const l = await invoke('meeting_list'); mt.items = l.items || []; mt.consent = !!l.consent_seen; } catch { mt.items = []; }
    try { mt.state = await invoke('meeting_state'); } catch {}
    mt.open ? drawDetail() : drawMeeting();
  }
  const live = () => mt.state?.state === 'recording' || mt.state?.state === 'paused';
  // The live sound as a waveform: one bar per reading, newest on the right.
  function wave() { const w = el('div', 'lb-wave'); for (let i = 0; i < BARS; i++) w.append(el('i')); return w; }
  function paintWave() {
    const w = box.querySelector('.lb-wave'); if (!w) return;
    const bars = w.children, pad = BARS - mt.levels.length;
    for (let i = 0; i < BARS; i++) { const v = i < pad ? 0 : mt.levels[i - pad]; bars[i].style.height = Math.max(8, Math.round(Math.sqrt(v) * 100)) + '%'; }
    const t = box.querySelector('.lb-time'); if (t) t.textContent = clock(mt.state.seconds);
  }
  function drawMeeting() {
    box.replaceChildren();
    const card = el('div', 'lb-card lb-call'), side = el('div', 'lb-side');
    box.append(card, side);
    const s = mt.state || {state: 'idle'};
    if (live()) {
      card.classList.add('live');
      const top = el('div', 'lb-row');
      const who = el('div', 'lb-who'); who.append(el('span', 'lb-sub', s.meta?.provider || 'Recording'), el('span', 'lb-time', clock(s.seconds)));
      top.append(mark(s.meta?.provider, 'l', 'speaker'), who, el('span', 'lb-grow'), el('span', 'lb-pill rec' + (s.state === 'paused' ? ' paused' : ''), s.state === 'paused' ? 'Paused' : 'REC'));
      const row = el('div', 'lb-row lb-acts');
      row.append(wave(), iconButton(s.state === 'paused' ? 'play' : 'pause', s.state === 'paused' ? 'Resume' : 'Pause', async () => { try { mt.state = await invoke('meeting_pause', {paused: s.state !== 'paused'}); drawMeeting(); } catch (e) { showError(e); } }, 'round'),
        button('lb-stop', null, async () => { try { await invoke('meeting_stop'); } catch (e) { showError(e); } mt.levels = []; loadMeetings(); }));
      row.lastChild.setAttribute('aria-label', 'Stop recording');
      card.append(top, row);
      if (s.warning) card.append(el('div', 'lb-warn', s.warning));
      requestAnimationFrame(paintWave);
    } else if (mt.confirm) {
      const top = el('div', 'lb-row'); top.append(mark(mt.confirm.provider, 'l', 'speaker'), el('div', 'lb-name', mt.confirm.title));
      card.append(top, el('p', 'lb-text', mt.confirm.text));
      const row = el('div', 'lb-row lb-acts lb-end');
      row.append(button('lb-ghost', 'Cancel', () => { mt.confirm = null; drawMeeting(); }), button('lb-primary rec', 'Record', () => { const c = mt.confirm; mt.confirm = null; start(c.req); }));
      card.append(row);
    } else {
      const sc = mt.scan, list = sc?.candidates || [], pick = list.find(c => c.pid === mt.picked);
      const recBtn = button('lb-rec', null, () => {
        if (!pick) return;
        const req = {pid: pick.pid, mode: pick.browser ? 'browser' : 'app', mic: mt.mic, broad_ok: false, title: pick.title, provider: pick.provider};
        if (pick.browser) { mt.confirm = {provider: pick.provider, title: `All of ${pick.provider}'s sound`, text: 'A browser plays every tab together, so other tabs are recorded too.', req: {...req, broad_ok: true}}; drawMeeting(); return; }
        start(req);
      });
      recBtn.setAttribute('aria-label', pick ? `Record ${pick.provider}` : 'Record'); recBtn.disabled = !pick || sc?.isolation === false;
      const micBtn = iconButton(mt.mic ? 'mic' : 'micoff', mt.mic ? 'Microphone on' : 'Microphone off', () => { mt.mic = !mt.mic; drawMeeting(); }, 'round' + (mt.mic ? ' on' : ''));
      micBtn.setAttribute('aria-pressed', String(mt.mic));
      const moreBtn = iconButton('more', 'More', e => menu(e.currentTarget, [
        {label: 'Scan again', run: scanMeetings},
        {label: 'Record all PC audio…', run: () => { mt.confirm = {provider: '', title: 'All PC audio', text: 'Everything this PC plays is recorded: music, videos and alerts too.', req: {pid: 0, mode: 'system', mic: mt.mic, broad_ok: true, title: 'All PC audio', provider: 'All PC audio'}}; drawMeeting(); }},
      ]), 'round');
      if (pick) {
        const top = el('div', 'lb-row');
        const who = el('div', 'lb-who'); who.append(el('span', 'lb-name', pick.browser ? pick.title.replace(/ [-–] .*$/, '') || pick.provider : pick.provider));
        const st = el('span', 'lb-sub'); st.append(el('i', 'lb-dot ' + (pick.confidence === 'high' ? 'live' : '')), document.createTextNode(pick.browser ? `In ${pick.provider}` : pick.confidence === 'high' ? 'In a call' : 'Maybe a call'));
        who.append(st);
        top.append(mark(pick.provider, 'l'), who);
        card.append(top);
        if (list.length > 1) {
          const others = el('div', 'lb-row lb-chips');
          for (const c of list) { const b = button('lb-chip' + (c.pid === mt.picked ? ' on' : ''), null, () => { mt.picked = c.pid; drawMeeting(); }); b.append(mark(c.provider, 'xs'), el('span', '', c.provider)); b.setAttribute('aria-pressed', String(c.pid === mt.picked)); others.append(b); }
          card.append(others);
        }
      } else {
        // Nothing to record yet: a quiet radar that says Sora is listening for a call.
        const top = el('div', 'lb-row');
        const radar = el('span', 'lb-radar'); radar.append(el('span', 'sweep'), icon('wave'));
        const who = el('div', 'lb-who'); who.append(el('span', 'lb-name', sc ? 'No call yet' : 'Looking…'), el('span', 'lb-sub', sc?.isolation === false ? `This Windows can't record one app alone.` : 'Zoom, Teams, Meet and Webex show up here.'));
        top.append(radar, who);
        card.append(top);
      }
      const row = el('div', 'lb-row lb-acts');
      const label = el('div', 'lb-who');
      if (pick) { label.append(el('span', 'lb-rec-label', 'Record')); label.append(el('span', 'lb-hint', mt.consent ? (mt.mic ? 'Call sound and your mic' : 'Only the call sound') : 'Let everyone know you record')); }
      if (pick) row.append(recBtn, label); row.append(el('span', 'lb-grow'), micBtn, moreBtn);
      card.append(row);
    }
    // Recent recordings
    const head = el('div', 'lb-side-head'); head.append(el('span', '', 'Recordings'), el('span', 'lb-count', mt.items.length ? String(mt.items.length) : ''));
    side.append(head);
    if (!mt.items.length) { const e = el('div', 'lb-empty'); e.append(icon('wave', 'big'), el('span', '', 'Your recordings stay on this PC.')); side.append(e); }
    for (const it of mt.items) {
      const b = button('lb-item', '', () => openDetail(it.meta.id));
      const txt = el('span', 'lb-item-text'); txt.append(el('span', 'lb-item-name', it.meta.title || it.meta.id), el('span', 'lb-item-sub', `${day(it.meta.started)} · ${clock(it.meta.duration_s)}`));
      b.append(mark(it.meta.provider, 's', 'speaker'), txt);
      if (it.summary) b.append(icon('spark', 'lb-tag ai')); else if (it.transcript) b.append(icon('wave', 'lb-tag'));
      side.append(b);
    }
  }
  async function start(req) {
    try { if (!mt.consent) { invoke('meeting_consent_seen').catch(() => {}); mt.consent = true; } mt.levels = []; mt.state = await invoke('meeting_start', {req}); }
    catch (e) { showError(e); }
    drawMeeting();
  }
  // While recording on screen: a reading every 150 ms feeds the waveform and the clock (no full redraw).
  let poll = 0;
  function livePoll(on) {
    clearInterval(poll); if (!on) return;
    poll = setInterval(async () => {
      if (current !== 'meeting' || mt.open) return;
      try { const s = await invoke('meeting_state'); const was = live(); mt.state = s; if (!live()) { if (was) loadMeetings(); return; } mt.levels.push(s.state === 'paused' ? 0 : s.level || 0); if (mt.levels.length > BARS) mt.levels.shift(); if (!box.querySelector('.lb-wave')) drawMeeting(); else paintWave(); } catch {}
    }, 150);
  }
  // Plays a recording from a line of its transcript: the call's sound and your mic together, read once as bytes (the
  // page gets no file path). The line being heard is lit and follows the sound.
  const player = {id: null, tracks: [], urls: [], at: null, segs: []};
  const markPlaying = at => { player.at = at; for (const r of box.querySelectorAll('.lb-seg')) r.classList.toggle('playing', r.dataset.id === at); };
  function stopPlayer() { for (const a of player.tracks) a.pause(); markPlaying(null); }
  function dropPlayer() { stopPlayer(); for (const u of player.urls) URL.revokeObjectURL(u); Object.assign(player, {id: null, tracks: [], urls: [], segs: []}); }
  async function loadPlayer(id) {
    if (player.id === id) return player.tracks.length > 0;
    dropPlayer(); player.id = id;
    const get = async mic => {
      try {
        const b = new Uint8Array(await invoke('meeting_audio', {id, mic}));
        const u = URL.createObjectURL(new Blob([b], {type: b[0] === 0x4f ? 'audio/ogg' : 'audio/wav'})); player.urls.push(u);
        const a = new Audio(u); a.preload = 'auto'; return a;
      } catch (e) { if (!mic) showError(e); return null; }
    };
    const [app, mic] = await Promise.all([get(false), get(true)]);
    if (player.id !== id) return false;
    player.tracks = [app, mic].filter(Boolean);
    if (app) {
      app.onended = stopPlayer;
      app.ontimeupdate = () => { if (!player.at || app.paused) return; const t = app.currentTime; let at = player.at; for (const s of player.segs) { if (s.start <= t + 0.05) at = s.id; else break; } if (at !== player.at) markPlaying(at); };
    }
    return !!app;
  }
  async function playFrom(id, segs, s) {
    if (player.at === s.id && player.tracks[0] && !player.tracks[0].paused) { stopPlayer(); return; }
    stopPlayer();
    if (!await loadPlayer(id)) return;
    player.segs = segs;
    for (const a of player.tracks) { a.currentTime = s.start; a.play().catch(() => {}); }
    markPlaying(s.id);
  }
  document.addEventListener('visibilitychange', () => { if (document.hidden) stopPlayer(); });
  async function openDetail(id) {
    if (player.id !== id) dropPlayer();
    mt.open = id; mt.detail = undefined; mt.query = ''; setTall(true); drawDetail();
    try { mt.detail = await invoke('ai_transcript', {id}); } catch { mt.detail = null; }
    try { mt.ai = flags.local_ai ? await invoke('ai_status') : null; } catch { mt.ai = null; }
    mt.view = mt.detail?.summary ? 'summary' : 'transcript';
    drawDetail();
  }
  function closeDetail() { dropPlayer(); mt.open = null; setTall(false); drawMeeting(); }
  const installed = id => (mt.ai?.parts || []).some(p => p.part.id === id && p.installed);
  function aiPick(group) { const parts = mt.ai?.parts || []; const want = mt.ai?.recommend?.[group]; const got = parts.filter(p => p.part.group === group && p.part.kind === 'model' && p.installed); return (got.find(p => p.part.id === want) || got[0])?.part.id; }
  // Sends this transcript to the cloud AI set in Settings > Meeting; the button names where it goes.
  function cloudSummary(it) { toast('Sending the transcript to ' + mt.ai.cloud.label); runAi('cloud_ai_summarize', {id: it.meta.id}); }
  function drawDetail() {
    const it = mt.items.find(i => i.meta.id === mt.open);
    if (!it) return closeDetail();
    box.replaceChildren();
    const page = el('div', 'lb-detail'); box.append(page);
    const head = el('div', 'lb-row lb-head');
    const who = el('div', 'lb-who'); who.append(el('span', 'lb-title', it.meta.title || it.meta.id), el('span', 'lb-sub', `${day(it.meta.started)} · ${clock(it.meta.duration_s)}`));
    head.append(iconButton('back', 'Back', closeDetail), mark(it.meta.provider, 'm', 'speaker'), who, el('span', 'lb-grow'));
    const t = mt.detail?.transcript, sum = mt.detail?.summary?.summary, job = mt.ai?.job, cloud = mt.ai?.cloud;
    const onSummary = !!(t && sum && mt.view === 'summary');
    const aiChat = {label: 'Copy for an AI chat', run: () => invoke('ai_copy', {id: it.meta.id, external: true}).then(() => toast('Copied. Nothing was sent.')).catch(showError)};
    if (job && job.id === it.meta.id) { const j = el('span', 'lb-job'); const bar = el('span', 'lb-bar'); const fill = el('i'); fill.style.width = job.progress + '%'; bar.append(fill); j.append(el('span', '', job.stage), bar, button('lb-ghost', 'Cancel', () => invoke('ai_cancel').catch(() => {}))); head.append(j); }
    else if (flags.local_ai && !t) head.append(installed('whisper-engine') && aiPick('transcript') ? button('lb-primary', 'Transcribe', () => runAi('ai_transcribe', {id: it.meta.id, model: aiPick('transcript'), language: 'default', mode: 'balanced'})) : button('lb-ghost', 'Get package', () => invoke('open_settings', {section: 'meeting'}).catch(showError)));
    else if (flags.local_ai && !sum && installed('llama-engine') && aiPick('summary')) { const b = button('lb-primary', null, () => runAi('ai_summarize', {id: it.meta.id, model: aiPick('summary'), mode: 'balanced'})); b.append(icon('spark'), el('span', '', 'Summarize')); head.append(b); }
    else if (flags.local_ai && !sum && cloud?.ready) { const b = button('lb-primary', null, () => cloudSummary(it)); b.append(icon('spark'), el('span', '', 'Summarize with ' + cloud.label)); head.append(b); }
    head.append(iconButton('play', 'Play', () => invoke('meeting_open', {id: it.meta.id, reveal: false}).catch(showError)), iconButton('more', 'More', e => menu(e.currentTarget, [
      // The menu follows the open tab: copy, save and redo the summary or the transcript, not both.
      ...(onSummary ? [{label: 'Copy summary', run: () => invoke('ai_copy_summary', {id: it.meta.id}).then(() => toast('Copied')).catch(showError)}, aiChat,
        {label: 'Save as Markdown', run: () => exportSummary(it, 'md')}, null,
        ...(installed('llama-engine') && aiPick('summary') ? [{label: 'Summarize again', run: () => runAi('ai_summarize', {id: it.meta.id, model: aiPick('summary'), mode: 'balanced'})}] : []),
        ...(cloud?.ready ? [{label: 'Summarize again with ' + cloud.label, run: () => cloudSummary(it)}] : []), null]
      : t ? [{label: 'Copy transcript', run: () => invoke('ai_copy', {id: it.meta.id, external: false}).then(() => toast('Copied')).catch(showError)}, aiChat,
        {label: 'Save as Markdown', run: () => exportAs(it, 'md')}, null,
        // The cloud is a second choice beside a local summary (the main button when there is no local one).
        ...(!sum && cloud?.ready && installed('llama-engine') && aiPick('summary') ? [{label: 'Summarize with ' + cloud.label, run: () => cloudSummary(it)}] : []),
        ...(installed('whisper-engine') ? [{label: 'Transcribe again', run: () => runAi('ai_transcribe', {id: it.meta.id, model: aiPick('transcript'), language: 'default', mode: 'balanced'})}] : []), null] : []),
      {label: 'Show in folder', run: () => invoke('meeting_open', {id: it.meta.id, reveal: true}).catch(showError)},
      {label: 'Delete', danger: true, run: () => invoke('meeting_delete', {id: it.meta.id}).then(() => { toast('Deleted'); mt.open = null; setTall(false); loadMeetings(); }).catch(showError)},
    ])));
    page.append(head);
    if (it.meta.note) page.append(el('div', 'lb-hint', it.meta.note));
    if (t && sum) {
      const seg = el('div', 'fx-segs lb-switch');
      for (const [v, label] of [['summary', 'Summary'], ['transcript', 'Transcript']]) { const b = button('fx-seg' + (mt.view === v ? ' on' : ''), label, () => { mt.view = v; drawDetail(); }); b.setAttribute('aria-pressed', String(mt.view === v)); seg.append(b); }
      page.append(seg);
    }
    const body = el('div', 'fx-scroll lb-body');
    if (mt.detail === undefined) body.append(el('div', 'lb-empty', 'Loading…'));
    else if (!t) { const e = el('div', 'lb-empty'); e.append(icon('wave', 'big'), el('span', '', flags.local_ai ? 'Transcribe it to read and search what was said.' : 'Install the Meeting package in Settings for a transcript.')); body.append(e); }
    else if (mt.view === 'summary' && sum) body.append(summaryView(sum, t, mt.detail.summary.cloud));
    else body.append(transcriptView(it, t));
    page.append(body);
  }
  async function exportSummary(it, f) { try { await invoke('ai_export_summary', {id: it.meta.id, format: f}); toast('Summary saved next to the recording'); } catch (e) { showError(e); } }
  async function exportAs(it, f) { try { await invoke('ai_export', {id: it.meta.id, format: f}); toast('Saved next to the recording'); } catch (e) { showError(e); } }
  function transcriptView(it, t) {
    const wrap = el('div', 'lb-tx');
    const q = el('input', 'lb-search'); q.type = 'text'; q.placeholder = 'Search what was said'; q.value = mt.query; q.spellcheck = false; q.setAttribute('aria-label', 'Search transcript');
    const list = el('div', 'lb-segs');
    // Search hides the lines that do not match, and a turn with none left.
    const filter = () => {
      const v = mt.query.trim().toLowerCase();
      for (const turn of list.children) { let any = false; for (const r of turn.querySelectorAll('.lb-seg')) { r.hidden = !!v && !r.querySelector('.lb-st').textContent.toLowerCase().includes(v); any ||= !r.hidden; } turn.hidden = !any; }
    };
    q.oninput = () => { mt.query = q.value; filter(); };
    // One block per turn: who speaks (avatar and name, once), then their lines, each with its time on the right.
    let turn = null, prev = null;
    for (const s of t.segments) {
      const who = s.speaker || '';
      if (!turn || who !== prev) {
        turn = el('div', 'lb-turn'); if (who) turn.dataset.who = who;
        const name = speakerName(who, t.names);
        if (name) { const head = el('div', 'lb-turn-head'), b = el('b', 'lb-spk', name); if (who !== 'me') nameable(b, it, t, who); head.append(el('span', 'lb-av', !t.names?.[who] && /^p\d+$/.test(who) ? who.slice(1) : name.slice(0, 1).toUpperCase()), b); turn.append(head); }
        list.append(turn); prev = who;
      }
      const row = el('div', 'lb-seg'); row.dataset.id = s.id; if (who) row.dataset.who = who;
      const text = el('span', 'lb-st', s.text); text.contentEditable = 'plaintext-only'; text.spellcheck = false; text.title = 'Click to edit';
      text.onblur = () => { const v = text.textContent.trim(); if (v !== s.text) { s.text = v; invoke('ai_transcript_save', {id: it.meta.id, segments: t.segments}).then(() => toast('Saved')).catch(showError); } };
      text.onkeydown = e => { if (e.key === 'Enter') { e.preventDefault(); text.blur(); } };
      const ts = button('lb-ts', mmss(s.start), () => playFrom(it.meta.id, t.segments, s)); ts.title = 'Play from here'; ts.setAttribute('aria-label', 'Play from ' + mmss(s.start));
      if (player.at === s.id) row.classList.add('playing');
      row.append(text, ts); turn.append(row);
    }
    wrap.append(q, list); requestAnimationFrame(filter);
    return wrap;
  }
  // A speaker's name above a line: click it to rename that speaker everywhere in this transcript (Person 1 -> Budi).
  // Enter saves, Esc cancels, an empty name brings back "Person 1".
  function nameable(b, it, t, speaker) {
    b.title = 'Click to rename'; b.tabIndex = 0; b.setAttribute('role', 'button');
    const edit = () => { if (b.isContentEditable) return; b.contentEditable = 'plaintext-only'; b.spellcheck = false; b.focus(); getSelection().selectAllChildren(b); };
    b.onclick = edit;
    b.onkeydown = e => {
      if (!b.isContentEditable) { if (e.key === 'Enter') { e.preventDefault(); edit(); } return; }
      if (e.key === 'Enter') { e.preventDefault(); b.blur(); }
      else if (e.key === 'Escape') { e.preventDefault(); b.textContent = speakerName(speaker, t.names); b.blur(); }
    };
    b.onblur = () => {
      b.contentEditable = 'false';
      const v = b.textContent.trim().slice(0, 40);
      if (v === speakerName(speaker, t.names)) return;
      invoke('ai_speaker_name', {id: it.meta.id, speaker, name: v}).then(() => {
        t.names = {...t.names}; if (v) t.names[speaker] = v; else delete t.names[speaker];
        drawDetail(); toast('Renamed');
      }).catch(e => { showError(e); drawDetail(); });
    };
  }
  function summaryView(s, t, cloudLabel) {
    const wrap = el('div', 'lb-sum');
    if (s.overview) { const o = el('div', 'lb-over'); o.append(icon('spark'), el('p', 'lb-text', s.overview)); wrap.append(o); }
    const jump = id => { mt.view = 'transcript'; drawDetail(); requestAnimationFrame(() => { const r = box.querySelector(`.lb-seg[data-id="${id}"]`); r?.scrollIntoView({block: 'center'}); r?.classList.add('hit'); setTimeout(() => r?.classList.remove('hit'), 1500); }); };
    const first = i => t.segments.find(x => x.id === (i.supportingSegmentIds || [])[0]);
    const pill = (glyph, text) => { const p = el('span', 'lb-pill'); p.append(icon(glyph), el('span', '', text)); return p; };
    // One section per kind of point, one point per row: what, then (for a to-do) who and when, and the time to jump to.
    const section = (title, kind, items, fill) => {
      if (!items?.length) return false;
      const sec = el('section', 'lb-sec ' + kind), h = el('div', 'lb-sec-head');
      h.append(icon(kind === 'dec' ? 'check' : kind === 'todo' ? 'todo' : 'ask'), el('span', '', title), el('span', 'lb-count', String(items.length)));
      sec.append(h);
      for (const i of items) {
        const r = el('div', 'lb-point'), body = el('div', 'lb-ib'); fill(body, i);
        r.append(el('i', 'lb-bullet'), body);
        const seg = first(i); if (seg) r.append(button('lb-cite', mmss(seg.start), () => jump(seg.id)));
        sec.append(r);
      }
      wrap.append(sec); return true;
    };
    const any = [
      section('Decisions', 'dec', s.decisions, (b, d) => b.append(el('p', 'lb-it', d.text))),
      section('To do', 'todo', s.actionItems, (b, a) => {
        b.append(el('p', 'lb-it', a.task));
        if (a.owner || a.deadline) { const m = el('div', 'lb-meta'); if (a.owner) m.append(pill('person', a.owner)); if (a.deadline) m.append(pill('calendar', a.deadline)); b.append(m); }
      }),
      section('Open questions', 'ask', s.openQuestions, (b, q) => b.append(el('p', 'lb-it', q.text))),
    ].some(Boolean);
    if (s.topics?.length) {
      const sec = el('section', 'lb-sec topics'), h = el('div', 'lb-sec-head'); h.append(el('span', '', 'Topics'));
      const row = el('div', 'lb-topics');
      for (const tp of s.topics) { const seg = first(tp); const c = button('lb-topic', tp.title, () => seg && jump(seg.id)); if (seg) c.title = 'Read from ' + mmss(seg.start); row.append(c); }
      sec.append(h, row); wrap.append(sec);
    }
    if (!any && !s.overview) wrap.append(el('div', 'lb-empty', 'Nothing clear came out. Read the transcript.'));
    wrap.append(el('div', 'lb-hint', cloudLabel ? `Made by ${cloudLabel} (cloud AI). Check it against the transcript.` : 'Made by AI on this PC. Check it against the transcript.'));
    return wrap;
  }
  async function runAi(cmd, args) {
    try { const run = invoke(cmd, args); setTimeout(async () => { try { mt.ai = await invoke('ai_status'); if (mt.open) drawDetail(); } catch {} }, 300); await run; }
    catch (e) { showError(e); }
    await loadMeetings(); if (mt.open) openDetail(mt.open);
  }
  listen('meeting-state', e => { const was = live(); mt.state = e.payload || {state: 'idle'}; if (current === 'meeting' && !mt.open && was !== live()) drawMeeting(); });
  listen('meeting-history', () => { if (current === 'meeting') loadMeetings(); });
  listen('ai-job', e => { if (mt.ai) mt.ai.job = e.payload; if (current === 'meeting' && mt.open) drawDetail(); });

  // ================= tabs
  let ticker = 0;
  function entered(name) {
    if (current === 'agents' && name !== 'agents') { ai.leave(); agentsTall = 0; notch.style.removeProperty('--q-h'); }
    if (name !== 'meeting' && mt.open) { dropPlayer(); mt.open = null; tall = false; }
    current = name;
    clearInterval(ticker); closeMenu(); livePoll(false);
    if (name === 'agents') { ai.enter(); loadAgents().then(() => ai.checkRecap()); }
    if (name === 'meeting') { mt.open = null; tall = false; loadMeetings(); scanMeetings(); livePoll(true); ticker = setInterval(() => { if (!mt.open && !live()) scanMeetings(); }, 8000); }
  }
  loadAgents();
  return {entered};
}
