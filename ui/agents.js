// The Agents tab (Labs): what the coding agents (Claude Code, Codex, Gemini CLI, Antigravity) are doing, the cards that
// ask you something, and the files they changed. One card on the left (the focused session: what you asked, the agent's
// own task list, what it runs now; or an approval, a question, its end), plan usage on the right.
// All data comes from src/agents.rs; this file only draws it and sends decisions back. Nothing is allowed without a click.
// The card layouts follow Coucou's island (github.com/louis-cfm/coucou, MIT, Copyright (c) 2026 Louis Raillé).
const CLICK_GUARD = 600;
const NAMES = {'claude-code': 'Claude Code', 'claude-desktop': 'Claude', gemini: 'Gemini CLI', codex: 'Codex', cursor: 'Cursor', copilot: 'Copilot CLI', opencode: 'OpenCode', amp: 'Amp', antigravity: 'Antigravity', muse: 'Muse Code', hermes: 'Hermes'};
const STATES = {idle: 'Ready', thinking: 'Thinking', working: 'Working', waiting: 'Needs you', finished: 'Done', error: 'Stopped'};
const BUSY = ['working', 'thinking', 'waiting'];
const SVG = {
  done: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="m3.8 8.4 2.7 2.7 5.7-6.2"/></svg>',
  now: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3.8 4.2 4.2L6 12.2"/></svg>',
  todo: '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="3.6"/></svg>',
  run: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="m3.5 5 3 3-3 3M8.5 11.5h4"/></svg>',
};
const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text !== undefined) e.textContent = text; return e; };
const svgEl = (tag, cls, svg) => { const e = el(tag, cls); e.innerHTML = svg; return e; };

/** "2m 05s" for a running turn, "3m" once it is over. */
export function elapsed(ms, still) {
  const s = Math.max(0, Math.floor(ms / 1000)), m = Math.floor(s / 60), h = Math.floor(m / 60);
  if (h) return `${h}h ${m % 60}m`;
  if (still) return m ? `${m}m` : `${s}s`;
  return m ? `${m}m ${String(s % 60).padStart(2, '0')}s` : `${s}s`;
}
/** Up to `max` plan items around the one in progress, and how many were left out before and after. */
export function planWindow(plan, max = 3) {
  const now = plan.findIndex(p => p.status === 'in_progress'), next = plan.findIndex(p => p.status !== 'completed');
  const focus = now >= 0 ? now : next >= 0 ? next : plan.length - 1;
  const start = Math.max(0, Math.min(focus - 1, plan.length - max));
  return {items: plan.slice(start, start + max), before: start, after: Math.max(0, plan.length - start - max)};
}

export function createAi({invoke, notify, mark, onKind}) {
  const main = document.getElementById('ai-main'), side = document.getElementById('ai-side'), running = document.getElementById('ai-running') || document.createElement('span');
  // connected: an agent's hooks are installed or a session was seen (null: not known yet). Until then no session or recap actions.
  let sessions = [], pending = [], active = false, clock = 0, connected = null, focus = null;
  const acked = new Set(), shownAt = new Map(), dismissed = new Map(), step = new Map(), picks = new Map();

  const label = a => NAMES[a] || a;
  const dot = state => { const d = el('i', 'ai-dot'); d.dataset.state = state; return d; };
  // "● korus  Claude Code ············ Working · 1m 20s"
  const who = (state, project, text, right) => {
    const w = el('div', 'ai-who');
    w.append(dot(state), el('b', '', project || 'Session'), el('span', '', text));
    if (right) w.append(right);
    return w;
  };
  const send = (id, decision) => invoke('agent_decide', {id, decision}).catch(() => {});
  const guarded = id => performance.now() - (shownAt.get(id) || 0) < CLICK_GUARD;
  // A button that does nothing in the first moments after its card appeared (the click was aimed at something else).
  const btn = (cls, text, onClick, id) => { const b = el('button', cls, text); b.type = 'button'; b.onclick = () => { if (id && guarded(id)) return; onClick(); }; return b; };
  const key = k => el('kbd', '', k);

  // "Edits · billing.ts +12 −3" -> verb, target, added, removed.
  function stepRow(text, cls) {
    const m = /^(.*?) · (.*?)(?: \+(\d+) −(\d+))?$/.exec(text) || [, '', text], row = el('div', 'ai-step ' + cls);
    row.append(svgEl('span', 'ic', cls === 'now' ? SVG.now : SVG.done));
    if (m[1]) row.append(el('span', 'v', m[1]));
    row.append(el('span', 't', m[2]));
    if (m[3] !== undefined) row.append(el('span', 'add', '+' + m[3]), el('span', 'del', '−' + m[4]));
    return row;
  }
  function planRows(plan, still) {
    const {items, before, after} = planWindow(plan), list = el('div', 'ai-list');
    for (const p of items) {
      const cls = p.status === 'completed' ? 'done' : p.status === 'in_progress' && !still ? 'now' : 'todo', row = el('div', 'ai-step ' + cls);
      row.append(svgEl('span', 'ic', SVG[cls === 'todo' && p.status === 'in_progress' ? 'now' : cls]), el('span', 't', p.text));
      list.append(row);
    }
    return list;
  }
  const timeOf = s => { const t = el('time', 'ai-time'); t.dataset.from = s.started || s.at; t.dataset.still = String(!BUSY.includes(s.state)); t.dataset.to = s.at; t.textContent = elapsed((BUSY.includes(s.state) ? Date.now() : s.at) - (s.started || s.at), !BUSY.includes(s.state)); return t; };
  function stateTag(s) {
    const r = el('span', 'ai-state'), plan = s.plan || [];
    r.dataset.state = s.state;
    const done = plan.filter(p => p.status === 'completed').length;
    r.append(el('em', '', STATES[s.state] || s.state));
    if (plan.length) r.append(el('small', '', `${done}/${plan.length}`));
    r.append(timeOf(s));
    return r;
  }

  // The ring on the left: the share of the agent's tasks done (or a slow sweep while it works without a list), the count
  // in the middle; amber while it waits for you, green when done, red when stopped.
  // The agent's state as a Siri-style orb (CSS in labs.css): soft colours melting on white, turning faster the busier
  // the agent is. The card is rebuilt on every update, so each orb starts its animations at the shared clock (--t0)
  // and carries on where the last one was instead of jumping back to the start.
  const ORB = {idle: {speed: 1, sat: .85, glow: .5, breathe: true}, working: {speed: 1.6, sat: 1, glow: .8}, thinking: {speed: 2.2, sat: 1, glow: .7},
    waiting: {speed: .8, sat: 1, glow: .6, breathe: true}, finished: {speed: .6, sat: .9, glow: .5}, error: {speed: .5, sat: .25, glow: .3}};
  function progress(s) {
    const box = el('div', 'ai-wave ai-orb'), m = ORB[s.state] || ORB.idle; box.dataset.state = s.state; box.setAttribute('aria-hidden', 'true');
    if (m.breathe) box.dataset.breathe = '';
    const set = {'--t0': -(performance.now() / 1000) + 's', '--spin': 9 / m.speed + 's', '--drift': 24 / (1 + m.speed) + 's', '--sat': m.sat, '--glow': m.glow * .7};
    for (const k in set) box.style.setProperty(k, set[k]);
    const disc = el('div', 'ai-orb-disc');
    disc.append(el('span', 'ai-orb-layer ai-orb-sheen'), el('span', 'ai-orb-layer ai-orb-rim'));
    box.append(el('div', 'ai-orb-glow'), disc);
    return box;
  }
  function overview(s) {
    const card = el('div', 'ai-card'); card.dataset.kind = 'overview'; card.dataset.state = s.state;
    const plan = s.plan || [], steps = s.steps || [], body = el('div', 'ai-body');
    card.append(progress(s), body);
    const add = (...x) => body.append(...x);
    add(who(s.state, s.project, label(s.agent), stateTag(s)));
    add(el('p', 'ai-prompt', s.prompt || (s.state === 'idle' ? 'Ready when you are' : 'Working on your request')));
    if (plan.length) {
      add(planRows(plan, false));
      if (s.action && BUSY.includes(s.state)) { const n = el('div', 'ai-now'); n.append(svgEl('span', 'ic', SVG.run), el('span', '', s.action)); add(n); }
    } else if (steps.length) {
      const list = el('div', 'ai-list');
      list.append(...steps.map((t, i) => stepRow(t, i === steps.length - 1 && BUSY.includes(s.state) ? 'now' : 'done')));
      add(list);
    } else if (s.state === 'thinking') add(el('div', 'ai-list ai-hint', 'Thinking…'));
    return card;
  }

  function approval(p) {
    const card = el('div', 'ai-card'); card.dataset.kind = 'approval';
    const head = who('waiting', p.project, label(p.agent) + ' wants to ' + p.verb);
    if (mark) head.prepend(mark(p.agent));
    const code = el('pre', 'ai-code', p.detail || p.tool);
    const row = el('div', 'ai-actions');
    const deny = btn('ai-btn', 'Deny', () => send(p.id, 'deny'), p.id); deny.append(key('N')); row.append(deny);
    if (p.always) { const a = btn('ai-btn ai-always', 'Always allow', () => send(p.id, 'always'), p.id); a.dataset.tip = p.always; row.append(a); }
    const allow = btn('ai-btn primary', 'Allow', () => send(p.id, 'allow'), p.id); allow.append(key('Y'));
    row.append(allow);
    card.append(head, code, row);
    return card;
  }

  function question(p) {
    const card = el('div', 'ai-card'); card.dataset.kind = 'question';
    const qs = p.questions || [], i = Math.min(step.get(p.id) || 0, qs.length - 1), q = qs[i];
    if (!q) return approval(p);
    const answers = picks.get(p.id) || {};
    // "Terminal" (answer there instead) sits in the header, so the options get the room below.
    const right = el('span', 'ai-right');
    const term = btn('ai-link', 'Terminal', () => send(p.id, 'terminal')); term.setAttribute('aria-label', 'Answer in terminal');
    right.append(term);
    const qhead = who('asking', p.project, label(p.agent) + ' asks' + (qs.length > 1 ? ` · ${i + 1} of ${qs.length}` : ''), right);
    if (mark) qhead.prepend(mark(p.agent));
    card.append(qhead);
    const text = el('p', 'ai-q', q.question), opts = el('div', 'ai-opts'), chosen = new Set();
    const finish = value => {
      answers[q.question] = value; picks.set(p.id, answers);
      if (i + 1 < qs.length) { step.set(p.id, i + 1); render(); } else send(p.id, {answers});
    };
    for (const o of q.options) {
      const b = btn('ai-opt', '', () => {
        if (!q.multiSelect) return finish(o.label);
        chosen.has(o.label) ? chosen.delete(o.label) : chosen.add(o.label);
        b.setAttribute('aria-pressed', String(chosen.has(o.label)));
        done.disabled = !chosen.size;
      }, p.id);
      b.append(el('b', '', o.label));
      if (o.description) { b.append(el('small', '', o.description)); }
      if (q.multiSelect) b.setAttribute('aria-pressed', 'false');
      opts.append(b);
    }
    const done = btn('ai-btn primary', 'Done', () => { if (chosen.size) finish(q.options.map(o => o.label).filter(l => chosen.has(l))); }, p.id);
    done.disabled = true;
    card.append(text, opts);
    // Several answers: Done under them. One answer: a click on it is the answer.
    if (q.multiSelect) { const foot = el('div', 'ai-actions'); foot.append(done); card.append(foot); }
    return card;
  }

  function ended(s) {
    const bad = s.state === 'error', card = el('div', 'ai-card'), plan = s.plan || [], body = el('div', 'ai-body'); card.dataset.kind = s.state;
    card.append(progress(s), body);
    body.append(who(s.state, s.project, label(s.agent), stateTag(s)));
    if (s.prompt) body.append(el('p', 'ai-prompt dim', s.prompt));
    body.append(el('p', 'ai-last', bad ? s.error || 'The session stopped on an error.' : s.last || 'Done.'));
    if (plan.length) body.append(el('div', 'ai-more', `${plan.filter(p => p.status === 'completed').length} of ${plan.length} tasks done`));
    const row = el('div', 'ai-actions');
    row.append(btn('ai-btn', 'Open terminal', () => invoke('agent_open_terminal', {session: s.id}).catch(e => notify?.(String(e)))), btn('ai-btn primary', 'OK', () => { dismissed.set(s.id, s.at); if (focus === s.id) focus = null; render(); }));
    body.append(row);
    return card;
  }

  function pickMain() {
    const open = [...pending].sort((a, b) => a.at - b.at)[0];
    if (open) return {kind: open.tool === 'AskUserQuestion' ? 'question' : 'approval', p: open};
    const fresh = x => (x.state === 'finished' || x.state === 'error') && dismissed.get(x.id) !== x.at;
    // The session picked in the switcher stays on the card while it is still there.
    const s = sessions.find(x => x.id === focus) || sessions.find(x => BUSY.includes(x.state)) || sessions.find(fresh) || sessions[0];
    if (!s) return {kind: 'empty'};
    return fresh(s) ? {kind: 'ended', s} : {kind: 'overview', s};
  }

  // The canvas swirl below now draws only the orb on the share picture (a CSS orb cannot be painted onto a canvas).
  // How lively the orb is per state: speed of the swirl and how far the ribbons swing.
  const ENERGY = {working: 1, thinking: .8, waiting: .5, finished: .15, error: .15, idle: .3};
  // Blue while the session sits empty (idle, nothing asked yet); black and white once it has something going on.
  const BLUE = ['#1d4ed8', '#4f9dff', '#a8d4ff'], MONO = ['#55555a', '#a1a1a6', '#f4f4f6'];
  function drawWave(c, state, t) {
    const g = c.getContext('2d'), W = c.width, H = c.height, cx = W / 2, cy = H / 2, R = H * .44;
    const e = ENERGY[state] ?? .3, pal = state === 'idle' ? BLUE : MONO, k = t / 1000 * (.35 + .9 * e);
    g.clearRect(0, 0, W, H);
    g.globalCompositeOperation = 'lighter';
    // The rim: three crescents (a circle minus an off-centre one), thick on one side, thin on the other, turning at their own pace.
    pal.forEach((col, i) => {
      const a = k * (1 - i * .35) * (i % 2 ? -1 : 1) + i * 2.1, d = R * (.16 + .08 * Math.sin(k * 1.3 + i)), r = R * (1 - i * .03);
      g.beginPath();
      g.arc(cx, cy, r, 0, Math.PI * 2);
      g.arc(cx + Math.cos(a) * d, cy + Math.sin(a) * d, r - d * .55 - 1.5, 0, Math.PI * 2, true);
      g.shadowColor = col; g.shadowBlur = 10;
      g.fillStyle = col + (i === 2 ? '88' : '40');
      g.fill();
    });
    // The swirl: a silky wave through the sphere, a few strands with a little spread, clipped to the ball.
    g.save();
    g.beginPath(); g.arc(cx, cy, R - 2, 0, Math.PI * 2); g.clip();
    g.translate(cx, cy); g.rotate(-.55 + Math.sin(k * .7) * .25);
    for (let j = 0; j < 4; j++) {
      g.beginPath();
      for (let x = -R; x <= R; x += 2) {
        const u = x / R, y = Math.sin(u * 2.6 + k * 1.6 + j * .22) * R * (.22 + .2 * e) * (1 - u * u * .6) + (j - 1.5) * 2.2 + R * .18;
        x === -R ? g.moveTo(x, y) : g.lineTo(x, y);
      }
      g.shadowColor = pal[1]; g.shadowBlur = 8;
      g.strokeStyle = pal[j === 1 ? 2 : 1] + (j === 1 ? 'cc' : '55');
      g.lineWidth = j === 1 ? 2.2 : 1.4;
      g.stroke();
    }
    g.restore();
    g.globalCompositeOperation = 'source-over'; g.shadowBlur = 0;
  }
  // Plan usage (Settings > AI Agent, off by default): Claude's 5-hour and weekly limits, Codex's windows; green under 50 %,
  // orange to 80 %, red above.
  let usage = null;
  const resetIn = ms => { const d = ms - Date.now(); if (!(d > 0)) return ''; const h = Math.floor(d / 3600000), m = Math.floor(d % 3600000 / 60000); return h >= 24 ? `${Math.floor(h / 24)}d ${h % 24}h` : h ? `${h}h ${m}m` : `${m}m`; };
  function usageBox() {
    if (!usage) return null;
    const rows = [];
    const add = (name, w, span) => {
      if (!w || typeof w.used !== 'number') return;
      const r = el('div', 'ai-limit'); r.dataset.level = w.used >= 80 ? 'high' : w.used >= 50 ? 'mid' : 'low';
      const bar = el('i'); bar.style.setProperty('--fill', Math.max(2, Math.min(100, w.used)) + '%');
      const left = w.resets ? resetIn(w.resets) : '';
      const label = el('span', '', span ? `${name} · ${span}` : name); if (left) label.append(el('small', '', left));
      r.append(label, el('b', '', Math.round(w.used) + '%'), bar);
      rows.push(r);
    };
    const span = w => !w?.mins ? 'limit' : w.mins >= 10080 ? 'week' : w.mins >= 1440 ? Math.round(w.mins / 1440) + ' d' : Math.round(w.mins / 60) + ' h';
    if (usage.claude) { add('Claude', usage.claude.five_hour, '5 h'); add('Claude', usage.claude.seven_day, 'week'); }
    if (usage.codex) { add('Codex', usage.codex.primary, span(usage.codex.primary)); add('Codex', usage.codex.secondary, span(usage.codex.secondary)); }
    // Antigravity: one row, its most used model (each model has its own pool, no 5 h / week windows).
    const agy = [...(usage.antigravity?.models || [])].sort((a, b) => b.used - a.used)[0];
    if (agy) add('Antigravity', agy, agy.label.replace(/^(Gemini|Claude|GPT)[- ]/, ''));
    if (!rows.some(r => r.classList.contains('ai-limit'))) return null;
    const box = el('div', 'ai-usage'); box.append(el('h3', '', 'Plan usage'), ...rows);
    return box;
  }
  // ---- a new Claude Code session, the weekly recap (sheets that take the main card; an approval always comes first)
  let sheet = null, folders = [], recap = null, recapCurrent = false, hideProjects = false, redraw = true;
  // The agents a session can start with (their CLI is on this PC): Claude Code, Codex.
  const nw = {folder: null, prompt: '', agent: 'claude-code'};
  let launchers = [];
  const openSheet = async name => {
    sheet = name; redraw = true;
    if (name === 'new') {
      try { folders = await invoke('agent_folders'); } catch { folders = []; } if (!nw.folder || !folders.includes(nw.folder)) nw.folder = folders[0] || nw.folder;
      try { launchers = await invoke('agent_launchers') || []; } catch { launchers = []; } if (!launchers.includes(nw.agent)) nw.agent = launchers[0] || nw.agent;
    }
    if (name === 'recap') { try { recap = await invoke('agent_recap', {current: recapCurrent}); } catch { recap = null; } }
    render();
  };
  const closeSheet = () => { if (sheet === 'recap' && recap?.week && !recapCurrent) invoke('agent_recap_seen', {week: recap.week}).catch(() => {}); sheet = null; redraw = true; render(); };
  const base = p => String(p).split(/[\\/]/).filter(Boolean).pop() || p;
  function newCard() {
    const card = el('div', 'ai-card'), body = el('div', 'ai-body'); card.dataset.kind = 'new';
    const head = el('div', 'ai-sheet-head'), chips = el('div', 'ai-folders');
    head.append(el('p', 'ai-sheet-title', 'New session'), chips); body.append(head);
    for (const f of folders.slice(0, 5)) { const b = btn('ai-folder' + (f === nw.folder ? ' on' : ''), base(f), () => { nw.folder = f; redraw = true; render(); }); b.setAttribute('aria-pressed', String(f === nw.folder)); b.setAttribute('aria-label', f); chips.append(b); }
    if (nw.folder && !folders.slice(0, 5).includes(nw.folder)) { const b = btn('ai-folder on', base(nw.folder), () => {}); b.setAttribute('aria-pressed', 'true'); chips.append(b); }
    chips.append(btn('ai-folder pick', 'Choose…', async () => { const f = await invoke('agent_pick_folder').catch(() => null); if (f) { nw.folder = f; redraw = true; render(); } }));
    const input = el('textarea', 'ai-input'); input.rows = 2; input.placeholder = 'What should it do?'; input.value = nw.prompt; input.setAttribute('aria-label', 'Prompt');
    input.oninput = () => { nw.prompt = input.value; };
    const start = async () => {
      if (!launchers.length) { notify?.('Install Claude Code or Codex first.'); return; }
      if (!nw.folder) { notify?.('Pick a folder first.'); return; }
      try { await invoke('agent_new_session', {folder: nw.folder, prompt: nw.prompt, agent: nw.agent}); nw.prompt = ''; notify?.(label(nw.agent) + ' opens in a new window.'); closeSheet(); } catch (e) { notify?.(String(e)); }
    };
    input.onkeydown = e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); start(); } if (e.key === 'Escape') { e.preventDefault(); closeSheet(); } };
    // Which agent: a chip each (mark + name), left of the buttons; with one agent there is nothing to pick.
    const row = el('div', 'ai-actions'), pick = el('div', 'ai-agents');
    if (launchers.length > 1) for (const a of launchers) {
      const b = btn('ai-folder ai-agent' + (a === nw.agent ? ' on' : ''), '', () => { nw.agent = a; redraw = true; render(); });
      if (mark) b.append(mark(a)); b.append(el('span', 'ai-agent-name', label(a)));
      b.setAttribute('aria-pressed', String(a === nw.agent)); pick.append(b);
    }
    row.append(pick, btn('ai-btn', 'Cancel', closeSheet), btn('ai-btn primary', 'Start', start));
    body.append(input, row); card.append(body);
    setTimeout(() => input.focus(), 30);
    return card;
  }
  const hm = min => { const m = Math.round(min || 0); return m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m}m`; };
  const top = o => Object.entries(o || {}).sort((a, b) => b[1] - a[1])[0]?.[0] || '';
  function recapStats(w) {
    return [['With agents', hm(w.minutes)], ['Prompts', w.turns], ['Files', w.files], ['Lines', `+${w.added} −${w.removed}`], ['Commands', w.commands], ['Permissions', w.permissions], ['Questions', w.questions], ['Longest turn', hm(w.longest_minutes)]];
  }
  const recapLine = w => [top(w.agents) && 'Mostly ' + label(top(w.agents)), !hideProjects && top(w.projects) && 'on ' + top(w.projects), top(w.days) && 'busiest on ' + top(w.days)].filter(Boolean).join(' · ');
  function recapCard() {
    const card = el('div', 'ai-card'), body = el('div', 'ai-body'); card.dataset.kind = 'recap';
    const w = recap?.data;
    const head = el('p', 'ai-sheet-title', recapCurrent ? 'This week so far' : 'Your week with agents'); if (recap?.week) head.append(el('small', '', 'Week of ' + recap.week)); body.append(head);
    if (!w || !w.turns) body.append(el('p', 'ai-prompt dim', recap?.on === false ? 'The weekly recap is off (Settings > AI Agent).' : 'Nothing counted for this week yet.'));
    // The card keeps the six main numbers (one airy row); the saved image has all of them.
    else { const g = el('div', 'ai-recap'); for (const [k, v] of recapStats(w).filter(([k]) => k !== 'Permissions' && k !== 'Questions')) { const c = el('div'); c.append(el('b', '', String(v)), el('small', '', k)); g.append(c); } body.append(g, el('p', 'ai-recap-line', recapLine(w))); }
    const row = el('div', 'ai-actions');
    row.append(btn('ai-btn', recapCurrent ? 'Last week' : 'This week', () => { recapCurrent = !recapCurrent; openSheet('recap'); }));
    if (w && w.turns) {
      const hide = btn('ai-btn', hideProjects ? 'Show project' : 'Hide project', () => { hideProjects = !hideProjects; redraw = true; render(); }); hide.setAttribute('aria-pressed', String(hideProjects));
      row.append(hide, btn('ai-btn', 'Save image', async () => { try { notify?.('Saved to ' + await invoke('agent_recap_save', {week: recap.week, png: shareImage(w)})); } catch (e) { notify?.(String(e)); } }));
    }
    row.append(btn('ai-btn primary', 'Close', closeSheet));
    body.append(row); card.append(body);
    return card;
  }
  // The share picture: 1080 × 1920, black, the orb, the numbers. Nothing but the counts (and the project, unless hidden).
  function shareImage(w) {
    const c = document.createElement('canvas'); c.width = 1080; c.height = 1920;
    const g = c.getContext('2d');
    g.fillStyle = '#000'; g.fillRect(0, 0, 1080, 1920);
    const orb = document.createElement('canvas'); orb.width = 520; orb.height = 520; drawWave(orb, 'idle', performance.now()); g.drawImage(orb, 280, 120);
    g.fillStyle = '#f5f5f7'; g.textAlign = 'center'; g.font = '700 64px Inter, Segoe UI, sans-serif'; g.fillText(recapCurrent ? 'This week so far' : 'My week with agents', 540, 760);
    g.fillStyle = '#8e8e93'; g.font = '400 36px Inter, Segoe UI, sans-serif'; g.fillText('Week of ' + recap.week, 540, 820);
    g.fillStyle = '#a1a1a6'; g.font = '400 34px Inter, Segoe UI, sans-serif'; g.fillText(recapLine(w), 540, 1580);
    recapStats(w).forEach(([k, v], i) => {
      const x = 270 + (i % 2) * 540, y = 960 + Math.floor(i / 2) * 150;
      g.fillStyle = '#f5f5f7'; g.font = '700 56px Inter, Segoe UI, sans-serif'; g.fillText(String(v), x, y);
      g.fillStyle = '#8e8e93'; g.font = '400 30px Inter, Segoe UI, sans-serif'; g.fillText(k, x, y + 46);
    });
    g.fillStyle = '#4f9dff'; g.font = '600 34px Inter, Segoe UI, sans-serif'; g.fillText('Sora', 540, 1860);
    return c.toDataURL('image/png');
  }
  // A file dropped on the tab: onto a session in the list, or the one in the main card.
  async function dropFile(p) {
    const k = devicePixelRatio || 1, at = document.elementFromPoint(p.x / k, p.y / k);
    const id = at?.closest('[data-session]')?.dataset.session || main.querySelector('[data-session]')?.dataset.session;
    const path = (p.paths || [])[0];
    if (!path) return;
    if (!id) { notify?.('Drop it on a session.'); return; }
    try { notify?.((await invoke('agent_drop', {session: id, path})) === 'typed' ? 'Path typed into the session.' : 'No window found: the path is copied.'); } catch (e) { notify?.(String(e)); }
  }
  function render() {
    const busy = sessions.filter(s => BUSY.includes(s.state)).length;
    running.textContent = busy ? busy + ' running' : '';
    running.hidden = !active || !busy;
    if (!active) return;
    const m = pickMain();
    let card;
    // A sheet being filled in stays as it is while sessions update (the text you type is not redrawn away).
    const sheetOn = sheet && !m.p;
    if (sheetOn && !redraw && main.dataset.kind === sheet) card = null;
    else if (sheetOn) card = sheet === 'new' ? newCard() : recapCard();
    // The idle card is kept as it is (its splash animation would start over on every update).
    else if (m.kind === 'empty' && main.dataset.kind === 'empty' && main.dataset.connected === String(connected)) card = null;
    else if (m.kind === 'empty') {
      card = el('div', 'ai-card'); card.dataset.kind = 'empty'; main.dataset.connected = String(connected);
      // Idle: the agents' marks resting on an orbit, waiting for a session.
      const orbit = el('div', 'ai-orbit'); orbit.setAttribute('aria-hidden', 'true');
      if (mark) for (const a of ['claude-code', 'codex', 'gemini']) orbit.append(mark(a));
      const text = el('div', 'ai-empty-text'), start = el('div', 'ai-actions');
      if (connected !== false) { text.append(el('p', 'ai-empty', 'No agents running'), el('p', 'ai-empty-hint', 'Sessions show up here while they work.')); if (connected) start.append(btn('ai-btn', 'New session', () => openSheet('new')), btn('ai-btn', 'Weekly recap', () => { recapCurrent = false; openSheet('recap'); })); }
      else { text.append(el('p', 'ai-empty', 'No agent connected'), el('p', 'ai-empty-hint', 'Connect Claude Code, Codex or Gemini first.')); start.append(btn('ai-btn', 'Open Settings', () => invoke('open_settings', {section: 'agent'}).catch(() => {}))); }
      card.append(orbit, text); text.append(start);
    }
    else card = m.kind === 'approval' ? approval(m.p) : m.kind === 'question' ? question(m.p) : m.kind === 'ended' ? ended(m.s) : overview(m.s);
    redraw = false;
    if (card) {
      if (m.s && !sheetOn) card.dataset.session = m.s.id; else if (m.p) card.dataset.session = m.p.session;
      main.dataset.kind = card.dataset.kind;
      main.replaceChildren(card);
      // A question asks the panel for its tall size (labs.js), so the whole question and every option fit.
      onKind?.(card.dataset.kind, card);
    }
    if (m.p) { if (!shownAt.has(m.p.id)) shownAt.set(m.p.id, performance.now()); if (!acked.has(m.p.id)) { acked.add(m.p.id); invoke('agent_ack', {id: m.p.id}).catch(() => {}); } }
    // The right column: the switcher (two or more agents at work) above plan usage. A waiting approval still comes first.
    const limits = usageBox(), pick = switcher(m.s && !sheetOn && !m.p ? m.s.id : null);
    side.replaceChildren(...[pick, limits].filter(Boolean));
    // A sheet takes the whole width: the side column would only crowd it.
    side.hidden = !(limits || pick) || sheetOn;
  }
  // One row per live session (busy, or ended and not yet dismissed): its mark, project and state; a click puts it on the card.
  function switcher(shown) {
    const live = sessions.filter(s => BUSY.includes(s.state) || ((s.state === 'finished' || s.state === 'error') && dismissed.get(s.id) !== s.at)).slice(0, 4);
    if (live.length < 2) return null;
    const box = el('div', 'ai-switch'), row = el('div'); box.append(el('h3', '', `Agents · ${live.length}`), row);
    for (const s of live) {
      const b = btn('ai-pick' + (s.id === shown ? ' on' : ''), '', () => { focus = s.id; redraw = true; render(); });
      b.dataset.state = s.state; b.setAttribute('aria-pressed', String(s.id === shown));
      b.setAttribute('aria-label', `${s.project || label(s.agent)} · ${label(s.agent)} · ${STATES[s.state] || s.state}`);
      b.append(mark ? mark(s.agent) : el('b', '', label(s.agent)[0]), dot(s.state));
      row.append(b);
    }
    return box;
  }
  // The running time ticks once a second, only while the tab shows a busy session.
  function tick() {
    for (const t of main.querySelectorAll('.ai-time[data-still=false]')) t.textContent = elapsed(Date.now() - Number(t.dataset.from), false);
  }

  document.addEventListener('keydown', e => {
    if (!active || e.ctrlKey || e.metaKey || e.altKey || e.target.closest?.('input,textarea')) return;
    const m = pickMain();
    if (m.kind !== 'approval' || guarded(m.p.id)) return;
    if (e.key === 'y' || e.key === 'Y') send(m.p.id, 'allow'); else if (e.key === 'n' || e.key === 'N') send(m.p.id, 'deny');
  });

  return {
    setSessions(list) { sessions = [...(list || [])].sort((a, b) => b.at - a.at); if (sessions.length) connected = true; render(); },
    setConnected(on) { connected = !!on || sessions.length > 0 || pending.length > 0; render(); },
    drop: p => dropFile(p),
    // Monday: last week's recap shows once, the first time the tab opens.
    async checkRecap() { if (!connected) return; try { const r = await invoke('agent_recap', {current: false}); if (r?.fresh && !sheet) { recap = r; recapCurrent = false; sheet = 'recap'; redraw = true; render(); } } catch {} },
    newSession: () => openSheet('new'),
    setUsage(u) { usage = u && (u.claude || u.codex || u.antigravity) ? u : null; render(); },
    setPending(list) {
      pending = list || [];
      const ids = new Set(pending.map(p => p.id));
      for (const m of [acked, shownAt, step, picks]) for (const k of [...m.keys()]) if (!ids.has(k)) m.delete(k);
      render();
    },
    enter() {
      active = true; render();
      clearInterval(clock); clock = setInterval(tick, 1000);
    },
    leave() { active = false; running.hidden = true; clearInterval(clock); },
    waiting: () => pending.length,
  };
}
