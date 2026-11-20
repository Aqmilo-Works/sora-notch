// Settings window. Notch and general options talk to src/desktop.rs.
import { startI18n, setLang, getLang, t } from './i18n.js';
const $ = id => document.getElementById(id);
const native = !!window.__TAURI__;
const invoke = (command, args = {}) => native ? window.__TAURI__.core.invoke(command, args) : Promise.reject(new Error('Run Sora.exe to change settings'));
const message = e => String(e?.message || e || 'Something went wrong.');
let SECTIONS = ['general', 'notch', 'agent', 'meeting', 'chats'];

// ---- window chrome and sections ----
$('w-close').onclick = () => invoke('window_close').catch(() => window.close());
$('w-min').onclick = () => invoke('window_minimize').catch(() => {});
$('quit-button').onclick = () => invoke('quit').catch(() => {});
function head(title, sub = '', advanced = false) { $('pane-title').textContent = title; $('pane-sub').textContent = sub; $('pane-badge').hidden = !advanced; }
function show(name) {
  if (!SECTIONS.includes(name)) name = 'general';
  document.querySelectorAll('.tab').forEach(b => { const on = b.dataset.section === name; b.classList.toggle('active', on); b.setAttribute('aria-current', on ? 'page' : 'false'); });
  document.querySelectorAll('.panel').forEach(p => p.classList.toggle('active', p.id === 'panel-' + name));
  const panel = $('panel-' + name);
  head(panel.dataset.title, panel.dataset.sub, panel.hasAttribute('data-advanced'));
  $('content').scrollTop = 0;
}
document.querySelectorAll('.tab').forEach(b => b.onclick = () => { $('filter').value = ''; applyFilter(); show(b.dataset.section); });
show(new URLSearchParams(location.search).get('section') || 'general');
// The Microsoft Store edition has no Meeting or Chat (the GitHub edition has everything): drop their tabs (search
// skips panels without a tab) and the Beta frame. AI Agent stays.
if (native) invoke('labs_state').then(f => {
  if (!f?.store) return;
  for (const s of ['meeting', 'chats']) document.querySelector(`.tab[data-section="${s}"]`)?.remove();
  SECTIONS = SECTIONS.filter(s => s !== 'meeting' && s !== 'chats');
  document.querySelector('.tab-set.adv')?.remove();
  // The Store keeps it up to date: no update button.
  $('updates-title')?.remove(); $('updates-group')?.remove();
  show(document.querySelector('.tab.active')?.dataset.section || 'general');
}).catch(() => {});
if (native) window.__TAURI__.event.listen('settings-section', e => { $('filter').value = ''; applyFilter(); show(e.payload); });
document.addEventListener('keydown', e => { if (e.key === 'Escape' && !recording) { if ($('filter').value) { $('filter').value = ''; applyFilter(); } else invoke('window_close').catch(() => {}); } });
document.addEventListener('contextmenu', e => e.preventDefault());
$('content').addEventListener('scroll', () => $('tabs').classList.toggle('scrolled', $('content').scrollTop > 0), { passive: true });

// ---- search across every section ----
function applyFilter() {
  const q = $('filter').value.trim().toLowerCase();
  document.body.classList.toggle('filtering', !!q);
  if (!q) {
    document.querySelectorAll('.row,.group,.group-title,.tab').forEach(el => el.classList.remove('hidden'));
    $('no-match').hidden = true;
    show(document.querySelector('.tab.active')?.dataset.section || 'general');
    return;
  }
  let total = 0;
  document.querySelectorAll('.panel').forEach(panel => {
    const item = document.querySelector(`.tab[data-section="${panel.id.slice(6)}"]`);
    if (!item) { panel.classList.remove('active'); return; }
    let found = 0;
    panel.querySelectorAll('.row').forEach(row => { const hit = (row.dataset.find + ' ' + row.textContent).toLowerCase().includes(q); row.classList.toggle('hidden', !hit); if (hit) found++; });
    panel.querySelectorAll('.group').forEach(g => { g.classList.toggle('hidden', !g.querySelector('.row:not(.hidden)')); });
    panel.querySelectorAll('.group-title').forEach(t => { let next = t.nextElementSibling; t.classList.toggle('hidden', !next || next.classList.contains('hidden')); });
    panel.classList.toggle('active', found > 0);
    item.classList.toggle('hidden', found === 0);
    total += found;
  });
  $('no-match').hidden = total > 0;
  head('Search results');
}
$('filter').addEventListener('input', applyFilter);

// ---- shortcut recorder ----
let recording = null;
const KEYNAMES = { ' ': 'Space', ArrowLeft: 'Left', ArrowRight: 'Right', ArrowUp: 'Up', ArrowDown: 'Down' };
const NAMED = ['Space', 'Enter', 'Tab', 'Backspace', 'Delete', 'Insert', 'Home', 'End', 'PageUp', 'PageDown', 'Left', 'Right', 'Up', 'Down'];
const MAC = /Macintosh|Mac OS X/.test(navigator.userAgent);
// A Mac names its modifiers with symbols, in its own order (Control, Option, Shift, Command), without plus signs.
const SYMBOL = { Ctrl: '⌃', Alt: '⌥', Shift: '⇧', Cmd: '⌘' };
const MAC_ORDER = ['Ctrl', 'Alt', 'Shift', 'Cmd'];
const keysLabel = value => {
  if (!MAC || value === 'None') return value;
  const parts = value.split('+'), key = parts.pop();
  return parts.sort((a, b) => MAC_ORDER.indexOf(a) - MAC_ORDER.indexOf(b)).map(p => SYMBOL[p] || p).join('') + key;
};
const modsLabel = mods => MAC ? mods.map(m => SYMBOL[m]).join('') : mods.join(' + ');
// Settings on a Mac: traffic lights on the left; no Beta features (Meeting, the chat apps); no Placement (the notch is
// the camera's, or a capsule in the menu bar), edge handle or Game Mode; full screen is what hides the notch.
if (MAC) {
  document.documentElement.classList.add('mac');
  document.querySelector('.tab-set.adv')?.remove();
  SECTIONS = SECTIONS.filter(s => s !== 'meeting' && s !== 'chats');
  // Inline, so the search filter (which sets .hidden on rows and groups) never brings them back.
  const gone = el => el && el.style.setProperty('display', 'none', 'important');
  const placement = [...document.querySelectorAll('#panel-notch .group-title')].find(h => h.textContent.trim() === 'Placement');
  gone(placement); gone(placement?.nextElementSibling);
  gone(document.getElementById('edge-toggle')?.closest('.row'));
  gone(document.getElementById('game-mode-seg')?.closest('.row'));
  gone(document.getElementById('hide-chat-toggle')?.closest('.group'));
  const max = document.getElementById('maximize-toggle')?.closest('.row');
  if (max) { max.querySelector('strong').textContent = 'Hide over full screen'; max.querySelector('.row-text').insertAdjacentHTML('beforeend', '<small>When an app or video fills the screen</small>'); }
  const startup = document.getElementById('startup-toggle')?.closest('.row')?.querySelector('strong');
  if (startup) startup.textContent = 'Open at login';
  document.getElementById('panel-notch')?.setAttribute('data-sub', 'Shortcut, size and behavior.');
  const seg = (id, label, find) => `<div class="row" data-find="${find}"><span class="row-text"><strong>${label}</strong></span><span class="seg" id="${id}" role="group" aria-label="${label}"><button type="button" data-v="small">Small</button><button type="button" data-v="large">Large</button></span></div>`;
  document.getElementById('shortcut-record')?.closest('.group')?.insertAdjacentHTML('afterend', '<h2 class="group-title">Size</h2><div class="group">' +
    seg('mac-notch-size', 'Notch', 'notch size small large') + seg('mac-notice-size', 'Notifications', 'notification size small large reminder') + '</div>');
}
// kind "notch": Ctrl first, optionally Alt or Shift (a Mac: Command or Control, Option and Shift as wished).
function startRecording(button, kind, capture, done, statusEl, current) {
  if (recording) return;
  capture(true).then(() => {
    recording = { button, kind, capture, done, statusEl, current };
    button.classList.add('recording');
    button.textContent = kind === 'notch' ? (MAC ? 'Press ⌘ or ⌃ first…' : 'Press Ctrl first…') : 'Press a shortcut…';
    statusEl.textContent = MAC ? (kind === 'notch' ? 'Hold ⌘ or ⌃, optionally ⌥ or ⇧, then a key. Esc cancels.' : 'Hold ⌘, ⌃, ⌥ or ⇧, then a key. Esc cancels.')
      : kind === 'notch' ? 'Hold Ctrl, optionally Alt or Shift, then a key. Esc cancels.' : 'Hold Ctrl, Alt, Shift or Win, then a key. Esc cancels.';
  }).catch(() => { statusEl.textContent = 'Could not record a shortcut. Try again.'; });
}
function stopRecording() {
  if (!recording) return;
  const { button, capture, statusEl, current } = recording;
  recording = null; button.classList.remove('recording'); button.textContent = keysLabel(current()); statusEl.textContent = '';
  capture(false).catch(() => {});
}
document.addEventListener('keydown', e => {
  if (!recording) return;
  e.preventDefault(); e.stopImmediatePropagation();
  const { kind, statusEl, button } = recording;
  if (e.key === 'Escape') { stopRecording(); return; }
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && (MAC ? 'Cmd' : 'Win')].filter(Boolean);
  if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) { button.textContent = mods.length ? modsLabel(mods) + (MAC ? '…' : ' + …') : 'Press a shortcut…'; return; }
  if (kind === 'notch' && MAC) {
    if (!e.ctrlKey && !e.metaKey) { statusEl.textContent = 'Press and hold ⌘ or ⌃ first.'; return; }
    if (mods.length > 3) { statusEl.textContent = 'Use at most three of ⌘ ⌃ ⌥ ⇧.'; return; }
  } else if (kind === 'notch') {
    if (!e.ctrlKey) { statusEl.textContent = 'Press and hold Ctrl first.'; return; }
    if (e.metaKey || (e.altKey && e.shiftKey)) { statusEl.textContent = 'Use Ctrl + key, with either Alt or Shift.'; return; }
  } else if (!mods.length) { statusEl.textContent = 'Hold Ctrl, Alt, Shift or Win first.'; return; }
  // The key itself, not what it types: Option+K types a symbol on a Mac.
  // (macOS hotkeys are physical keys, as e.code is; Windows keeps the typed letter, its hotkeys follow the layout.)
  const key = !MAC ? KEYNAMES[e.key] || e.key : e.code === 'Space' ? 'Space' : KEYNAMES[e.key] || (/^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : /^Digit\d$/.test(e.code) ? e.code.slice(5) : e.key);
  if (!(/^[a-z0-9]$/i.test(key) || /^F([1-9]|1[0-9]|2[0-4])$/.test(key) || NAMED.includes(key))) { statusEl.textContent = 'Use a letter, number, F-key, or navigation key.'; return; }
  const value = mods.join('+') + '+' + (key.length === 1 ? key.toUpperCase() : key);
  const { done, capture } = recording;
  recording = null; button.classList.remove('recording');
  capture(false).catch(() => {}).then(() => done(value));
}, true);
window.addEventListener('blur', stopRecording);

// ---- segmented controls ----
function setSeg(seg, value) { seg.querySelectorAll('button').forEach(b => { const on = b.dataset.v === String(value); b.classList.toggle('on', on); b.setAttribute('aria-pressed', String(on)); }); }

// ---- General: language, start with Windows, updates ----
setSeg($('lang-seg'), getLang());
$('lang-seg').onclick = e => { const b = e.target.closest('button[data-v]'); if (b) { setLang(b.dataset.v); setSeg($('lang-seg'), getLang()); } };
startI18n();
let startupBusy = false;
async function loadStartup() {
  const toggle = $('startup-toggle');
  try { toggle.checked = !!(await invoke('startup_enabled')); toggle.disabled = false; $('general-status').textContent = ''; }
  catch { $('general-status').textContent = 'Startup preference unavailable. Reopen Settings to retry.'; }
}
$('startup-toggle').onchange = async () => {
  const toggle = $('startup-toggle'), wanted = toggle.checked;
  startupBusy = true; toggle.disabled = true;
  try { toggle.checked = await invoke('set_startup', { enabled: wanted }); $('general-status').textContent = ''; }
  catch { toggle.checked = !wanted; $('general-status').textContent = 'Could not save. Please try again.'; }
  finally { startupBusy = false; toggle.disabled = false; }
};
let updateInfo = null, updateBusy = false;
function showUpdate(note, { label = 'Check for updates', primary = false, disabled = false } = {}) {
  if (note) $('update-note').textContent = note;
  const b = $('update-button'); b.textContent = label; b.classList.toggle('primary', primary); b.disabled = disabled;
}
function fileSize(bytes) { return bytes >= 1048576 ? (bytes / 1048576).toFixed(1) + ' MB' : Math.round(bytes / 1024) + ' KB'; }
async function checkUpdate() {
  if (updateBusy) return;
  updateBusy = true; showUpdate('', { label: 'Checking…', disabled: true });
  try {
    const info = await invoke('check_update');
    if (!info) throw Error('No answer');
    updateInfo = info;
    if (!info.configured) showUpdate('Updates are not set up for this build.');
    else if (info.available) showUpdate('Version ' + info.latest + (info.size ? ' (' + fileSize(info.size) + ')' : '') + ' is ready.', { label: 'Update to v' + info.latest, primary: true });
    else showUpdate('You are up to date.', { label: 'Check again' });
  } catch (e) { updateInfo = null; showUpdate(message(e) || 'Could not check for updates.'); }
  finally { updateBusy = false; }
}
async function applyUpdate() {
  if (updateBusy) return;
  updateBusy = true; showUpdate('Sora will restart when the download finishes.', { label: 'Downloading 0%', disabled: true, primary: true });
  try { await invoke('apply_update'); }
  catch (e) { updateBusy = false; showUpdate(message(e), { label: 'Update to v' + (updateInfo?.latest || ''), primary: true }); }
}
$('update-button').onclick = () => updateInfo?.available ? applyUpdate() : checkUpdate();
// Uninstall: only for a copy no installer manages (the portable Sora.exe, Sora.app); the rest go through Settings > Apps.
if (native) invoke('uninstall_kind').then(k => { $('uninstall-row').hidden = k !== 'self'; }).catch(() => {});
const closeUninstall = () => { $('uninstall-dialog').hidden = true; $('uninstall-button').focus(); };
$('uninstall-button').onclick = () => { $('un-status').textContent = ''; $('uninstall-dialog').hidden = false; $('un-cancel').focus(); };
$('un-cancel').onclick = closeUninstall;
$('uninstall-dialog').addEventListener('keydown', e => { if (e.key === 'Escape') { e.stopPropagation(); closeUninstall(); } });
$('uninstall-dialog').addEventListener('mousedown', e => { if (e.target === $('uninstall-dialog')) closeUninstall(); });
$('un-ok').onclick = async () => {
  $('un-ok').disabled = true; $('un-cancel').disabled = true;
  try { await invoke('uninstall_sora'); } catch (e) { $('un-status').textContent = message(e); $('un-ok').disabled = false; $('un-cancel').disabled = false; }
};
if (native) {
  window.__TAURI__.event.listen('update-progress', e => { if (updateBusy) $('update-button').textContent = 'Downloading ' + e.payload + '%'; });
  invoke('app_version').then(v => { if (v) $('brand-version').textContent = 'v' + v; }).catch(() => {});
}

// ---- Notch preferences ----
let desktop = null, desktopBusy = false;
const desktopControls = ['shortcut-record', 'maximize-toggle', 'edge-toggle', 'hide-chat-toggle', 'size-input'];
function showDesktop(snapshot) {
  desktop = snapshot.preferences;
  $('shortcut-record').textContent = keysLabel(desktop.shortcut);
  $('maximize-toggle').checked = desktop.hide_maximized;
  $('edge-toggle').checked = desktop.edge_handle !== false;
  $('hide-chat-toggle').checked = !!desktop.hide_chat_text;
  setSeg($('game-mode-seg'), desktop.game_mode);
  setSeg($('position-seg'), desktop.notch_position || 'center');
  if ($('mac-notch-size')) { setSeg($('mac-notch-size'), (desktop.notch_size ?? 100) >= 70 ? 'large' : 'small'); setSeg($('mac-notice-size'), desktop.notice_size === 'large' ? 'large' : 'small'); }
  $('size-input').value = desktop.notch_size ?? 100; $('size-out').textContent = $('size-input').value + '%';
  setSeg($('display-seg'), desktop.notch_display || 'primary');
  setSeg($('meeting-lang-seg'), desktop.meeting_language || 'auto');
  showLabs(desktop);
  $('desktop-status').textContent = !snapshot.shortcut_active && desktop.shortcut !== 'None' ? 'Shortcut unavailable. Record another combination.' : '';
}
const desktopSegs = ['game-mode-seg', 'position-seg', 'display-seg', 'size-range', 'meeting-lang-seg'];
function lockDesktop(on) { desktopControls.forEach(id => $(id).disabled = on); desktopSegs.forEach(id => $(id).classList.toggle('disabled', on)); }
// One button per connected display; with a single display the row is not needed.
async function loadDisplays() {
  let displays = [];
  try { displays = await invoke('notch_displays'); } catch {}
  $('display-row').hidden = displays.length < 2;
  $('display-seg').replaceChildren(...displays.map((d, i) => {
    const b = document.createElement('button'); b.type = 'button'; b.dataset.v = d.id; b.title = d.label;
    b.textContent = d.primary ? 'Primary' : 'Display ' + (i + 1); return b;
  }));
}
async function loadDesktop() {
  lockDesktop(true);
  try { await loadDisplays(); showDesktop(await invoke('desktop_preferences')); lockDesktop(false); }
  catch { $('desktop-status').textContent = 'Could not load preferences. Reopen Settings to retry.'; }
}
async function saveDesktop(change = {}) {
  if (desktopBusy || !desktop) return;
  const previous = { ...desktop };
  // Every saved field goes back (Labs switches included), then the ones this page edits.
  const wanted = { ...desktop, shortcut: desktop.shortcut, hide_maximized: $('maximize-toggle').checked, game_mode: desktop.game_mode, edge_handle: $('edge-toggle').checked, hide_chat_text: $('hide-chat-toggle').checked, notch_position: desktop.notch_position || 'center', notch_display: desktop.notch_display || 'primary', notch_size: desktop.notch_size ?? 100, ...change };
  desktopBusy = true; lockDesktop(true);
  try { showDesktop(await invoke('set_desktop_preferences', { preferences: wanted })); }
  catch (e) { showDesktop({ preferences: previous, shortcut_active: previous.shortcut !== 'None' }); $('desktop-status').textContent = message(e); }
  finally { desktopBusy = false; lockDesktop(false); }
}
['maximize-toggle', 'edge-toggle', 'hide-chat-toggle'].forEach(id => $(id).addEventListener('change', () => saveDesktop()));
$('meeting-lang-seg').onclick = e => { const b = e.target.closest('button[data-v]'); if (b && desktop) saveDesktop({ meeting_language: b.dataset.v }); };
$('game-mode-seg').onclick = e => { const b = e.target.closest('button[data-v]'); if (b && desktop) saveDesktop({ game_mode: b.dataset.v }); };
if ($('mac-notch-size')) {
  $('mac-notch-size').onclick = e => { const b = e.target.closest('button[data-v]'); if (b && desktop) saveDesktop({ notch_size: b.dataset.v === 'large' ? 100 : 40 }); };
  $('mac-notice-size').onclick = e => { const b = e.target.closest('button[data-v]'); if (b && desktop) saveDesktop({ notice_size: b.dataset.v }); };
}
[['position-seg', 'notch_position'], ['display-seg', 'notch_display']].forEach(([id, key]) =>
  $(id).onclick = e => { const b = e.target.closest('button[data-v]'); if (b && desktop) saveDesktop({ [key]: b.dataset.v }); });
$('shortcut-record').onclick = () => startRecording($('shortcut-record'), 'notch', on => invoke('set_shortcut_capture', { capturing: on }), value => saveDesktop({ shortcut: value }), $('desktop-status'), () => desktop?.shortcut || '');

// The notch follows the handle while it is dragged (a preview); the size is saved when the handle is let go.
$('size-input').addEventListener('input', () => { $('size-out').textContent = $('size-input').value + '%'; if (native) invoke('preview_notch_size', { size: Number($('size-input').value) }).catch(() => {}); });
$('size-input').addEventListener('change', async () => { await saveDesktop({ notch_size: Number($('size-input').value) }); if (native) invoke('preview_notch_size', { size: null }).catch(() => {}); });


if (native) { loadStartup(); loadDesktop(); }

// ---- Labs (docs/sora): each module is off until switched on here; saved with the other preferences.
const LABS = { 'lab-agents': 'feature_agents', 'lab-meeting': 'feature_meeting', 'agy-approvals': 'agy_approvals', 'agent-usage': 'agent_usage', 'agent-sounds': 'agent_sounds', 'lab-whatsapp': 'feature_whatsapp', 'lab-telegram': 'feature_telegram' };
function showLabs(p) {
  for (const [id, key] of Object.entries(LABS)) $(id).checked = key === 'agent_sounds' ? p[key] !== false : !!p[key];
  $('labs-agents').hidden = !p.feature_agents; $('labs-meeting').hidden = !p.feature_meeting;
  if (p.feature_agents) { loadHooks(); loadRecapSwitch(); }
  if (p.feature_meeting) { loadMeeting(); loadAi(); loadCloud(); }
  $('labs-whatsapp').hidden = !p.feature_whatsapp;
  if (p.feature_whatsapp) loadWa(); else $('ca-whatsapp').textContent = 'Off';
  $('labs-telegram').hidden = !p.feature_telegram;
  if (p.feature_telegram) loadTg(); else $('ca-telegram').textContent = 'Off';
}
// "More options" style folds: a plain text button that opens the block below it (search opens them all).
document.querySelectorAll('.fold-btn').forEach(b => b.onclick = () => { const f = $(b.dataset.fold), open = !f.classList.contains('open'); f.classList.toggle('open', open); b.setAttribute('aria-expanded', String(open)); });
for (const [id, key] of Object.entries(LABS)) $(id).addEventListener('change', () => { if (id === 'lab-meeting' && $(id).checked) autoTranscript = true; saveDesktop({ [key]: $(id).checked }); });

// Connect/Disconnect first shows the exact change and the backup path; nothing is written until Install/Remove.
function showHooks(st) {
  for (const b of document.querySelectorAll('#agent-rows button[data-agent]')) {
    const id = b.dataset.agent, on = !!st[id.replace('-', '_')];
    b.textContent = on ? 'Disconnect' : 'Connect'; b.dataset.on = String(on);
    document.querySelector('.agent-dot[data-agent="' + id + '"]').classList.toggle('on', on);
  }
  $('agent-status').textContent = '';
}
async function loadHooks() {
  try { showHooks(await invoke('agent_hooks')); } catch {}
  try { const s = await invoke('agent_pipe_status'); if (s.error) $('agent-status').textContent = s.error; } catch {}
}
if ($('agent-sounds-folder')) $('agent-sounds-folder').onclick = () => invoke('agent_sounds_folder').catch(e => { $('agent-status').textContent = message(e); });
if ($('agent-recap')) $('agent-recap').addEventListener('change', () => invoke('agent_recap_set', { on: $('agent-recap').checked }).catch(() => {}));
async function loadRecapSwitch() { try { const r = await invoke('agent_recap', { current: false }); $('agent-recap').checked = r?.on !== false; } catch {} }
let pendingAgent = null;
const closeDialog = () => { $('agent-dialog').hidden = true; if (pendingAgent) pendingAgent.btn.focus(); pendingAgent = null; };
function diffLines(el, removed, added) {
  el.replaceChildren();
  const add = (cls, sign, t) => { const s = document.createElement('div'); s.className = cls; s.textContent = sign + ' ' + t; el.append(s); };
  removed.forEach(t => add('del', '-', t)); added.forEach(t => add('add', '+', t));
  if (!removed.length && !added.length) add('none', '', 'No change.');
}
document.querySelectorAll('#agent-rows button[data-agent]').forEach(b => b.onclick = async () => {
  const on = b.dataset.on !== 'true', name = b.closest('.row').querySelector('strong').textContent;
  b.disabled = true;
  try {
    const p = await invoke('agent_hooks_preview', { agent: b.dataset.agent, on });
    pendingAgent = { agent: b.dataset.agent, on, btn: b };
    $('ad-title').textContent = (on ? 'Install hooks for ' : 'Remove hooks from ') + name;
    $('ad-path').textContent = p.path;
    $('ad-backup').textContent = p.backup || 'None (the file is new)';
    $('ad-ok').textContent = on ? 'Install' : 'Remove';
    diffLines($('ad-diff'), p.removed, p.added);
    $('agent-status').textContent = '';
    $('agent-dialog').hidden = false; $('ad-ok').focus();
  } catch (e) { $('agent-status').textContent = message(e); }
  finally { b.disabled = false; }
});
$('ad-cancel').onclick = closeDialog;
$('agent-dialog').addEventListener('keydown', e => { if (e.key === 'Escape') { e.stopPropagation(); closeDialog(); } });
$('agent-dialog').addEventListener('mousedown', e => { if (e.target === $('agent-dialog')) closeDialog(); });
$('ad-ok').onclick = async () => {
  const p = pendingAgent; if (!p) return;
  $('ad-ok').disabled = true;
  try { showHooks(await invoke('agent_hooks_set', { agent: p.agent, on: p.on })); }
  catch (e) { $('agent-status').textContent = message(e); }
  finally { $('ad-ok').disabled = false; closeDialog(); }
};
// Through Sora's own clipboard code: navigator.clipboard would raise a WebView2 permission prompt.
$('agent-copy').onclick = async () => {
  try { const exe = await invoke('sora_exe'); await invoke('board_put_text', { text: '"' + exe + '" --hook --agent my-agent' }); $('agent-status').textContent = "Copied. Replace my-agent with the tool's name."; }
  catch (e) { $('agent-status').textContent = message(e); }
};

// Meeting recorder: where recordings go, and what this Windows can isolate.
async function loadMeeting() {
  try { const l = await invoke('meeting_list'); $('meeting-folder').textContent = l.folder; } catch {}
}
$('meeting-folder-pick').onclick = async () => { try { const f = await invoke('meeting_pick_folder'); if (f) $('meeting-folder').textContent = f; } catch (e) { $('desktop-status').textContent = message(e); } };

// Meeting packages. The transcript comes with the recorder: switching the recorder on installs the transcript size
// recommended for this PC (that click is the opt-in), and another size can be picked here. The summary is optional,
// on its own cards. A package installs or removes as a whole, one at a time; each shows its size before the click.
// Engines (and the voice detection) stay while another package of the same kind still has its model. Every transcript
// package also brings the voice separation (the call's side as Person 1, 2…); the GPU library ships with Sora.
const gbOf = b => (b / 1073741824 >= 1 ? (b / 1073741824).toFixed(1) + ' GB' : Math.round(b / 1048576) + ' MB');
const PACKAGES = [
  { group: 'transcript', key: 'Lite', model: 'whisper-base-q5' },
  { group: 'transcript', key: 'Balanced', model: 'whisper-small' },
  { group: 'transcript', key: 'Powerful', model: 'whisper-turbo' },
  { group: 'summary', key: 'Short', model: 'qwen35-0.8b' },
  { group: 'summary', key: 'Standard', model: 'qwen35-2b' },
  { group: 'summary', key: 'Deep', model: 'qwen35-4b' },
];
const SHARED = { transcript: ['whisper-engine', 'whisper-vad', 'speaker-engine', 'speaker-seg', 'speaker-voice'], summary: ['llama-engine'] };
let pkgRunning = null, pkgCancel = false, aiSeen = null, autoTranscript = false;
const pkgIds = p => [...SHARED[p.group], p.model];
async function loadAi() {
  let s; try { s = await invoke('ai_status'); } catch (e) { $('ai-status').textContent = message(e); return; }
  aiSeen = s;
  const r = s.recommend, byId = Object.fromEntries(s.parts.map(x => [x.part.id, x]));
  const anyBusy = s.parts.some(x => x.progress?.state === 'downloading') || !!pkgRunning;
  const card = p => {
    const items = pkgIds(p).map(id => byId[id]).filter(Boolean), missing = items.filter(x => !x.installed), m = byId[p.model];
    // Engines are shared by every package of a kind, so only the package being installed (or whose model is downloading) is busy.
    const busy = pkgRunning ? pkgRunning === p.key : m?.progress?.state === 'downloading';
    const total = items.reduce((n, x) => n + x.part.size, 0);
    const got = items.reduce((n, x) => n + (x.installed ? x.part.size : x.progress?.state === 'downloading' ? x.progress.got : 0), 0);
    const el = document.createElement('div'); el.className = 'row pkg-card'; el.dataset.find = 'meeting ' + p.group + ' ' + p.key.toLowerCase() + ' ' + (m?.part.name || '').toLowerCase();
    // Every card has the same parts in the same places, so nothing shifts while one downloads.
    const head = document.createElement('div'); head.className = 'pkg-head';
    const name = document.createElement('strong'); name.textContent = p.key; head.append(name);
    if (p.model === r[p.group]) { const rec = document.createElement('span'); rec.className = 'pkg-rec'; rec.textContent = 'Recommended'; head.append(rec); }
    const what = document.createElement('small'); what.className = 'pkg-model'; what.textContent = m?.part.name || p.model;
    const note = document.createElement('small'); note.className = 'pkg-note'; note.textContent = m?.part.detail || '';
    const size = document.createElement('span'); size.className = 'pkg-size'; size.textContent = gbOf(total);
    const btn = document.createElement('button'); btn.type = 'button'; btn.className = 'button';
    btn.textContent = busy ? 'Cancel' : missing.length ? 'Install' : 'Remove';
    // Progress fills the button itself, so the card keeps its height while downloading.
    if (busy) { const pct = Math.round(100 * got / Math.max(1, total)); btn.classList.add('progress'); btn.style.setProperty('--p', pct + '%'); const n = document.createElement('b'); n.textContent = pct + '%'; btn.append(n); }
    btn.disabled = anyBusy && !busy;
    btn.onclick = () => runPackage(p);
    const foot = document.createElement('div'); foot.className = 'pkg-foot'; foot.append(size, btn);
    el.append(head, what, note, foot);
    el.classList.toggle('on', missing.length === 0);
    return el;
  };
  $('pkgs').replaceChildren(...PACKAGES.filter(p => p.group === 'transcript').map(card));
  $('pkgs-sum').replaceChildren(...PACKAGES.filter(p => p.group === 'summary').map(card));
  // Local AI is on exactly while something of it is installed.
  const any = s.parts.some(x => x.installed);
  if (desktop && !desktopBusy && any !== !!desktop.feature_local_ai) saveDesktop({ feature_local_ai: any });
  // The recorder was just switched on: its transcript comes with it.
  if (autoTranscript) {
    autoTranscript = false;
    const has = PACKAGES.some(p => p.group === 'transcript' && byId[p.model]?.installed);
    const want = PACKAGES.find(p => p.group === 'transcript' && p.model === r.transcript) || PACKAGES[0];
    if (!has && !anyBusy) runPackage(want);
  }
}
async function runPackage(p) {
  if (!aiSeen) return;
  const items = pkgIds(p).map(id => aiSeen.parts.find(x => x.part.id === id)).filter(Boolean);
  $('ai-status').textContent = '';
  if (pkgRunning || items.some(x => x.progress?.state === 'downloading')) { pkgCancel = true; for (const x of items) if (x.progress?.state === 'downloading') invoke('ai_cancel_download', { id: x.part.id }).catch(() => {}); return; }
  const missing = items.filter(x => !x.installed);
  try {
    if (!missing.length) {
      const stay = id => PACKAGES.some(o => o !== p && pkgIds(o).includes(id) && aiSeen.parts.find(x => x.part.id === o.model)?.installed);
      for (const x of items) if (!stay(x.part.id)) await invoke('ai_remove', { id: x.part.id });
    } else {
      pkgRunning = p.key; pkgCancel = false; loadAi();
      for (const x of missing) { if (pkgCancel) break; const run = invoke('ai_download', { id: x.part.id }); setTimeout(loadAi, 300); await run; }
    }
  } catch (e) { if (!pkgCancel && !/cancel/i.test(message(e))) $('ai-status').textContent = message(e); }
  finally { pkgRunning = null; pkgCancel = false; loadAi(); }
}
// ---- Cloud AI summaries (src/cloud_ai.rs): off by default. The key goes to Windows' Credential Manager and never
// comes back to this page; only "a key is saved" does.
let cloud = null;
// The Model menu: the service's own list (Claude's, OpenAI's, or the models Ollama / LM Studio have), fetched once per
// service, address and key. Other… (or a service without a list) switches to typing the name.
let cloudModels = null, cloudModelsFor = '', cloudTyping = false;
// Asking for the model list is also how a saved key is checked: a key the service turns down turns the field red.
let cloudKeyBad = false;
function markKey() {
  const bad = cloudKeyBad && cloud.has_key, k = $('cloud-key');
  k.classList.toggle('bad', bad); k.setAttribute('aria-invalid', String(bad));
  k.placeholder = bad ? 'Key not accepted' : cloud.has_key ? 'Saved. Type to replace' : { claude: 'sk-ant-…', openai: 'sk-…' }[cloud.provider] || 'Not needed for Ollama or LM Studio';
}
$('cloud-key').addEventListener('input', () => $('cloud-key').classList.remove('bad'));
async function loadModels(force) {
  const k = [cloud.provider, cloud.url, cloud.has_key].join('|');
  if (!force && cloudModelsFor === k) return drawModels();
  cloudModelsFor = k; cloudModels = null; cloudKeyBad = false; drawModels();
  $('cloud-model-refresh').classList.add('spin');
  try { cloudModels = await invoke('cloud_ai_models'); }
  catch (e) { cloudModels = []; cloudKeyBad = /did not accept the API key/.test(message(e)); } // no list: the field switches to typing the name
  markKey();
  $('cloud-model-refresh').classList.remove('spin');
  drawModels();
}
function drawModels() {
  const list = cloudModels || [], typing = cloudTyping || (cloudModels !== null && !list.length);
  $('cloud-model').hidden = !typing; $('cloud-model-btn').hidden = typing;
  $('cloud-model-name').textContent = cloud.model || (cloudModels === null && cloudModelsFor ? 'Loading…' : 'Pick a model');
  const menu = $('cloud-menu'), item = (text, cls, run) => { const b = document.createElement('button'); b.type = 'button'; b.className = cls; b.textContent = text; b.onclick = run; return b; };
  menu.replaceChildren(...list.map(m => { const b = item(m, '', () => { showModelMenu(false); cloudSet({ model: m }); }); b.setAttribute('role', 'option'); b.setAttribute('aria-selected', String(m === cloud.model)); return b; }));
  if (cloudModels === null && cloudModelsFor) { const s = document.createElement('small'); s.textContent = 'Loading the list…'; menu.append(s); }
  menu.append(item('Other…', 'cloud-other', () => { showModelMenu(false); cloudTyping = true; drawModels(); $('cloud-model').focus(); }));
}
function showModelMenu(on) { $('cloud-menu').hidden = !on; $('cloud-model-btn').setAttribute('aria-expanded', String(on)); if (on) $('cloud-menu').querySelector('[aria-selected=true], button')?.focus(); }
$('cloud-model-btn').onclick = () => showModelMenu($('cloud-menu').hidden);
$('cloud-model-refresh').onclick = () => { cloudTyping = false; loadModels(true); };
$('cloud-menu').addEventListener('keydown', e => {
  const items = [...$('cloud-menu').querySelectorAll('button')], i = items.indexOf(document.activeElement);
  if (e.key === 'Escape') { e.preventDefault(); showModelMenu(false); $('cloud-model-btn').focus(); }
  else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); items[(i + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length]?.focus(); }
});
document.addEventListener('click', e => { if (!e.target.closest('.cloud-pick')) showModelMenu(false); });
function drawCloud(s) {
  if (cloud && s.provider !== cloud.provider) cloudTyping = false;
  cloud = s;
  $('cloud-on').checked = s.on; $('cloud-more').hidden = !s.on;
  setSeg($('cloud-provider-seg'), s.provider);
  $('cloud-url-field').hidden = s.provider !== 'other';
  $('cloud-url').value = s.url; $('cloud-model').value = s.model; $('cloud-key').value = '';
  $('cloud-model').placeholder = s.provider === 'claude' ? 'claude-opus-5-5' : 'gpt-4.1-mini, llama3.1…';
  $('cloud-key-remove').hidden = !s.has_key;
  markKey();
  // No note under the form: only a failed Save writes one here.
  $('cloud-status').textContent = '';
  // A list needs a Claude key, or an OpenAI-compatible address (a server on this PC needs no key).
  if (s.on && (s.provider === 'other' ? !!s.url : s.has_key)) loadModels(false); else { cloudModels = null; cloudModelsFor = ''; drawModels(); }
}
async function loadCloud() { try { drawCloud(await invoke('cloud_ai_get')); } catch (e) { $('cloud-status').textContent = message(e); } }
const cloudSet = async over => { try { drawCloud(await invoke('cloud_ai_set', { config: { on: cloud.on, provider: cloud.provider, model: cloud.model, url: cloud.url, ...over } })); } catch (e) { $('cloud-status').textContent = message(e); } };
$('cloud-on').addEventListener('change', () => { if (cloud) cloudSet({ on: $('cloud-on').checked }); else $('cloud-on').checked = false; });
$('cloud-provider-seg').onclick = e => { const b = e.target.closest('button[data-v]'); if (b && cloud && b.dataset.v !== cloud.provider) cloudSet({ provider: b.dataset.v, model: b.dataset.v === 'claude' ? 'claude-opus-5-5' : '' }); };
$('cloud-form').addEventListener('submit', async e => {
  e.preventDefault(); if (!cloud) return;
  try {
    const model = $('cloud-model').hidden ? cloud.model : $('cloud-model').value.trim();
    let s = await invoke('cloud_ai_set', { config: { on: cloud.on, provider: cloud.provider, model, url: $('cloud-url').value.trim() } });
    const key = $('cloud-key').value.trim();
    if (key) { s = await invoke('cloud_ai_key', { provider: s.provider, key }); cloudModelsFor = ''; } // a new key is checked again
    drawCloud(s);
  } catch (err) { $('cloud-key').value = ''; $('cloud-status').textContent = message(err); }
});
$('cloud-key-remove').onclick = async () => { try { drawCloud(await invoke('cloud_ai_key', { provider: cloud.provider, key: '' })); } catch (e) { $('cloud-status').textContent = message(e); } };
if (native) window.__TAURI__.event.listen('ai-progress', () => { if (!$('labs-meeting').hidden) loadAiSoon(); });
let aiTimer = 0; function loadAiSoon() { clearTimeout(aiTimer); aiTimer = setTimeout(loadAi, 200); }

// ---- WhatsApp (src/whatsapp.rs): Sora's own linked-device helper. Link shows a QR code; Unlink logs out and deletes the session.
const WA_TEXT = { off: 'Off', missing: 'Helper missing: modules\\sora-wa.exe', starting: 'Connecting…', unlinked: 'Not linked', qr: 'Scan the code below', linked: 'Linked', offline: 'Linked, offline', error: 'Not connected' };
function drawWa(st) {
  const linkedish = st.state === 'linked' || st.state === 'offline';
  $('wa-status').textContent = (WA_TEXT[st.state] || st.state) + (linkedish && st.me ? ' as ' + st.me : '');
  $('ca-whatsapp').textContent = st.state === 'off' ? 'Off' : linkedish ? (st.state === 'linked' ? 'Linked' : 'Offline') : st.state === 'missing' ? 'Helper missing' : st.state === 'qr' ? 'Linking' : 'Not linked';
  const b = $('wa-link');
  b.textContent = linkedish ? 'Unlink' : st.state === 'qr' ? 'New code' : 'Link';
  b.disabled = st.state === 'missing' || st.state === 'starting';
  b.dataset.unlink = String(linkedish);
  $('wa-error').textContent = st.error || '';
  const box = $('wa-qr'); box.hidden = !(st.state === 'qr' && st.qr);
  if (!box.hidden) {
    const { width, modules } = st.qr, c = $('wa-code'), g = c.getContext('2d'), quiet = 3, px = Math.floor(c.width / (width + quiet * 2)), off = Math.floor((c.width - px * (width + quiet * 2)) / 2);
    g.fillStyle = '#fff'; g.fillRect(0, 0, c.width, c.height); g.fillStyle = '#000';
    for (let y = 0; y < width; y++) for (let x = 0; x < width; x++) if (modules[y * width + x] === '1') g.fillRect(off + (x + quiet) * px, off + (y + quiet) * px, px, px);
  }
}
async function loadWa() { try { drawWa(await invoke('wa_state')); } catch (e) { $('wa-error').textContent = message(e); } }
$('wa-link').onclick = async () => {
  const b = $('wa-link'), unlink = b.dataset.unlink === 'true';
  b.disabled = true; $('wa-error').textContent = '';
  try { await invoke(unlink ? 'wa_unlink' : 'wa_link'); } catch (e) { $('wa-error').textContent = message(e); }
  loadWa();
};
if (native) window.__TAURI__.event.listen('wa-state', e => drawWa(e.payload));

// ---- Chats: the apps on the left, the chosen app's settings on the right. Brand marks come from the notch (notch_ui.rs MARKS).
function chatApp(name) {
  for (const b of document.querySelectorAll('.chat-app')) { const on = b.dataset.app === name; b.classList.toggle('on', on); b.setAttribute('aria-selected', String(on)); }
  for (const s of document.querySelectorAll('.chat-sec')) s.classList.toggle('on', s.id === 'chat-' + name);
}
document.querySelectorAll('.chat-app').forEach(b => b.onclick = () => chatApp(b.dataset.app));
$('chat-apps').addEventListener('keydown', e => {
  if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
  const all = [...document.querySelectorAll('.chat-app')], i = all.findIndex(b => b.classList.contains('on'));
  const next = all[(i + (e.key === 'ArrowDown' ? 1 : all.length - 1)) % all.length]; e.preventDefault(); next.focus(); chatApp(next.dataset.app);
});
invoke('app_marks').then(list => {
  const hex = n => '#' + Number(n).toString(16).padStart(6, '0');
  for (const node of document.querySelectorAll('.chat-logo[data-mark]')) {
    const m = (list || []).find(x => x.name === node.dataset.mark); if (!m) { node.textContent = node.dataset.mark[0]; continue; }
    node.style.setProperty('--bg', hex(m.color)); node.style.setProperty('--fg', hex(m.fg));
    node.innerHTML = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="' + m.path + '"/></svg>';
  }
}).catch(() => { for (const node of document.querySelectorAll('.chat-logo[data-mark]')) node.textContent = node.dataset.mark[0]; });

// ---- Telegram (src/telegram.rs, TDLib): sign in with the phone number, the code, then the two-step password if any.
const TG_TEXT = { off: 'Off', missing: 'Engine missing: modules\\sora-tg.exe or tdjson.dll', nokey: 'This build has no Telegram app key', starting: 'Starting…', phone: 'Not signed in', code: 'Waiting for the code', password: 'Waiting for your password', ready: 'Signed in', closing: 'Signing out…', error: 'Not signed in' };
const TG_ASK = { phone: ['Phone number', 'text', '+62 812 3456 7890', 'tel'], code: ['Code', 'text', '12345', 'one-time-code'], password: ['Two-step password', 'password', '', 'current-password'] };
let tgStep = '';
function drawTg(st) {
  $('tg-status').textContent = st.state === 'ready' && st.me ? 'Signed in as ' + st.me : (TG_TEXT[st.state] || st.state);
  $('ca-telegram').textContent = st.state === 'ready' ? 'Signed in' : st.state === 'off' ? 'Off' : st.state === 'missing' || st.state === 'nokey' ? 'Not set up' : 'Not signed in';
  $('tg-out').hidden = st.state !== 'ready';
  $('tg-error').textContent = st.error || '';
  const ask = TG_ASK[st.state]; $('tg-login').hidden = !ask;
  if (ask && tgStep !== st.state) {
    const input = $('tg-input'); input.value = ''; input.type = ask[1]; input.placeholder = ask[2]; input.autocomplete = ask[3]; input.setAttribute('aria-label', ask[0]);
    if (document.activeElement?.closest?.('#chat-telegram')) input.focus();
  }
  if (ask) $('tg-label').textContent = st.hint || ask[0];
  tgStep = ask ? st.state : '';
}
async function loadTg() { try { drawTg(await invoke('tg_state')); } catch (e) { $('tg-error').textContent = message(e); } }
$('tg-login').onsubmit = async e => {
  e.preventDefault();
  const b = $('tg-next'); b.disabled = true; $('tg-error').textContent = '';
  try { await invoke('tg_submit', { value: $('tg-input').value }); } catch (err) { $('tg-error').textContent = message(err); }
  b.disabled = false; loadTg();
};
$('tg-out').onclick = async () => { $('tg-out').disabled = true; try { await invoke('tg_sign_out'); } catch (e) { $('tg-error').textContent = message(e); } $('tg-out').disabled = false; };
if (native) window.__TAURI__.event.listen('tg-state', e => drawTg(e.payload));
