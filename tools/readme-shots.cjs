// README gallery: the real panel UI (ui/) with made-up sample data, one shot per tab, each framed on the banner's soft
// background with a title. No real calendar, files, place, messages or names. usage:
//   node tools/readme-shots.cjs [outDir]   (default ../Sora-GitHub/docs/preview)
// STORE=1: the Store edition (no Meeting tab); RAW=1: only the bare panel shots (raw-<tab>.png, for tools/store-art.cjs);
// SCALE=n: pixel density of the shots (default 2).
const { chromium } = require((() => { try { return require.resolve('playwright'); } catch { return require('path').resolve(__dirname, '..', '..', 'test-env', 'node_modules', 'playwright'); } })());
const http = require('node:http'), fs = require('node:fs'), path = require('node:path');
const root = path.resolve(__dirname, '..', 'ui');
const out = process.argv[2] || path.resolve(__dirname, '..', '..', 'Sora-GitHub', 'docs', 'preview');
fs.mkdirSync(out, { recursive: true });
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png', '.woff2': 'font/woff2', '.json': 'application/json' };
const server = http.createServer((req, res) => {
  const u = req.url.split('?')[0], file = path.join(root, u === '/' ? 'index.html' : u.slice(1));
  if (!file.startsWith(root) || !fs.existsSync(file)) { res.writeHead(404); return res.end(); }
  res.setHeader('Content-Type', types[path.extname(file)] || 'application/octet-stream'); res.end(fs.readFileSync(file));
});
const STORE = process.env.STORE === '1', RAW = process.env.RAW === '1', SCALE = Number(process.env.SCALE || 2);
const ALL = [
  ['home', 'Home', 'Weather, calendar, focus timer and more widgets'],
  ['music', 'Music', 'Now playing with synced lyrics'],
  ['shelf', 'Tray', 'Keep files close, convert or send them'],
  ['send', 'Send', 'Files to your phone over Wi-Fi, and back'],
  ['board', 'Board', 'Everything you copy, on a row of cards'],
  ['agents', 'AI Agent', 'Follow and approve your coding agents'],
  ['history', 'Chat', 'WhatsApp, Telegram and Discord in the notch'],
  ['meeting', 'Meeting', 'Record a call, read the transcript and summary'],
];
const SHOTS = ALL.filter(s => !(STORE && (s[0] === 'meeting' || s[0] === 'history')));
// The chat apps' own marks (src/marks.rs), for the Chat shot.
const marksRs = fs.readFileSync(path.resolve(__dirname, '..', 'src', 'marks.rs'), 'utf8');
const MARKS = ['WhatsApp', 'Telegram', 'Discord'].map(n => { const m = marksRs.match(new RegExp(`Mark \{ name: "${n}",[^}]*?color: 0x([0-9a-f]+), fg: 0x([0-9a-f]+),[^}]*?path: "([^"]+)" \}`)); return m && { name: n, color: parseInt(m[1], 16), fg: parseInt(m[2], 16), path: m[3] }; }).filter(Boolean);
(async () => {
  await new Promise(r => server.listen(18772, '127.0.0.1', r));
  const browser = await chromium.launch({ channel: 'msedge' });
  const page = await browser.newPage({ viewport: { width: 1000, height: 520 }, deviceScaleFactor: SCALE });
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(([store, marks]) => {
    const now = Date.now(), sec = Math.floor(now / 1000);
    const art = (a, b) => { const c = document.createElement('canvas'); c.width = 96; c.height = 96; const g = c.getContext('2d'); const gr = g.createLinearGradient(0, 0, 96, 96); gr.addColorStop(0, a); gr.addColorStop(1, b); g.fillStyle = gr; g.fillRect(0, 0, 96, 96); return c.toDataURL(); };
    const items = [
      { id: 'a', kind: 'link', tag: 'link', text: 'https://github.com/Aqmilo-Works/sora-notch', site: 'GitHub', media: false, at: sec - 30, keep: false },
      { id: 'b', kind: 'text', tag: 'text', text: 'Design review moved to Thursday 14:00. Bring the signed forms.', at: sec - 600, keep: true },
      { id: 'c', kind: 'image', tag: 'image', text: '', w: 1920, h: 1080, at: sec - 4000, keep: false, thumb: art('#4f9dff', '#1f3fd8'), app: 'Browser', src: '', ocr: 'Invoice 4471' },
      { id: 'd', kind: 'text', tag: 'code', lang: 'Rust', text: 'fn main() {\n  println!("hello");\n}', at: sec - 9000, keep: false },
      { id: 'e', kind: 'text', tag: 'color', text: '#4f9dff', at: sec - 9500, keep: false }];
    const shelf = [{ path: 'C:\\Work\\proposal.pdf', name: 'proposal.pdf', size: 1230000, directory: false, available: true }, { path: 'C:\\Work\\cover.png', name: 'cover.png', size: 480000, directory: false, available: true }, { path: 'C:\\Work\\notes.txt', name: 'notes.txt', size: 2400, directory: false, available: true }, { path: 'C:\\Work\\demo.mp4', name: 'demo.mp4', size: 42000000, directory: false, available: true }];
    const sessions = [
      { id: 's1', agent: 'claude-code', project: 'webshop', state: 'working', steps: ['Reads · cart.ts', 'Edits · cart.ts +12 −3', 'Edits · cart.test.ts +28', 'Runs · npm test'], count: 4, at: now, cwd: 'C:\code\webshop', prompt: 'Fix the cart total when a coupon is applied', plan: [{ id: '1', text: 'Find where the total is computed', status: 'completed' }, { id: '2', text: 'Apply the coupon before tax', status: 'completed' }, { id: '3', text: 'Add a test for coupons', status: 'completed' }, { id: '4', text: 'Run the test suite', status: 'in_progress' }, { id: '5', text: 'Summarize the change', status: 'pending' }], action: 'Runs · npm test', started: now - 248000, last: '', error: '', changes: [{ path: 'C:\code\webshop\cart.ts', file: 'cart.ts', added: 12, removed: 3, preview: [], hidden: false, at: now }, { path: 'C:\code\webshop\cart.test.ts', file: 'cart.test.ts', added: 28, removed: 0, preview: [], hidden: false, at: now }] },
      { id: 's2', agent: 'codex', project: 'docs-site', state: 'working', steps: ['Edits · guide.md +40 −8'], count: 1, at: now - 20000, cwd: 'C:\code\docs-site', prompt: 'Update the install guide', plan: [{ id: '1', text: 'Rewrite the install steps', status: 'in_progress' }], action: 'Edits · guide.md', started: now - 95000, last: '', error: '', changes: [] },
      { id: 's3', agent: 'gemini', project: 'api', state: 'done', steps: ['Runs · go test ./...'], count: 1, at: now - 600000, cwd: 'C:\code\api', prompt: 'Add rate limiting', plan: [], action: '', started: now - 1500000, last: 'Rate limiting added', error: '', changes: [] }];
    const usage = { claude: { five_hour: { used: 42, resets: now + 2 * 3600000, mins: 300 }, seven_day: { used: 18, resets: now + 3 * 86400000, mins: 10080 }, at: now } };
    const meetings = [
      { meta: { id: 'm1', title: 'Weekly sync', provider: 'Zoom', started: now - 3600000, duration_s: 1830, mode: 'app', isolated: true, mic: true, recovered: false, note: '' }, size: 350e6, mic: true, transcript: true, summary: true },
      { meta: { id: 'm2', title: 'Design review', provider: 'Google Meet', started: now - 90000000, duration_s: 2410, mode: 'app', isolated: true, mic: true, recovered: false, note: '' }, size: 420e6, mic: true, transcript: true, summary: false }];
    const cands = [{ pid: 100, provider: 'Zoom', title: 'Zoom Meeting', exe: 'zoom.exe', confidence: 'high', score: 5, reasons: ['App is running', 'Playing sound now'], isolated: true, browser: false }];
    window.__TAURI__ = { event: { listen: async () => () => {} }, core: { invoke: async (name, args = {}) => {
      if (name === 'desktop_preferences') return { preferences: { shortcut: 'Ctrl+Alt+Space', hide_maximized: false, game_mode: 'off', edge_handle: true, notch_position: 'center', notch_display: 'primary', notch_size: 100, feature_agents: true, feature_meeting: true }, shortcut_active: true };
      if (name === 'weather') return { current: { temperature_2m: 28.4, apparent_temperature: 31.2, relative_humidity_2m: 68, weather_code: 2, is_day: 1 } };
      if (name === 'monitors') return ['Display 1'];
      if (name === 'labs_state') return { agents: true, meeting: !store, local_ai: !store, whatsapp: false, telegram: false, store };
      if (name === 'app_version') return '1.0.0';
      if (name === 'media_status') return { title: 'Golden Hour', artist: 'Aurora Lane', album: 'Night Drive', source: 'Spotify.exe', playing: true, position: 62, duration: 214, can_play: true, can_seek: true, can_next: true, can_previous: true };
      if (name === 'online_lyrics') return { syncedLyrics: '[00:00]City lights are fading slow\n[00:30]Hold on to the golden hour\n[01:00]Every road leads back to you\n[01:05]Stay awhile and watch it glow\n[01:12]We never have to let it go' };
      if (name === 'shelf_list' || name === 'shelf_pick') return shelf;
      if (name === 'board_list') return { capture: true, items };
      if (name === 'file_thumbnail') {
        // A page for documents, a picture for images, a dark frame with a play mark for video.
        const c = document.createElement('canvas'); c.width = 96; c.height = 128; const g = c.getContext('2d'), p = args.path;
        if (p.endsWith('.png')) { const gr = g.createLinearGradient(0, 0, 96, 128); gr.addColorStop(0, '#ffb199'); gr.addColorStop(1, '#ff5e8a'); g.fillStyle = gr; g.fillRect(0, 0, 96, 128); g.fillStyle = 'rgba(255,255,255,.85)'; g.beginPath(); g.arc(66, 34, 12, 0, 7); g.fill(); g.fillStyle = 'rgba(80,20,60,.55)'; g.beginPath(); g.moveTo(0, 128); g.lineTo(36, 70); g.lineTo(60, 100); g.lineTo(74, 86); g.lineTo(96, 128); g.fill(); }
        else if (p.endsWith('.mp4')) { g.fillStyle = '#23232d'; g.fillRect(0, 0, 96, 128); g.fillStyle = '#fff'; g.beginPath(); g.moveTo(38, 48); g.lineTo(62, 64); g.lineTo(38, 80); g.fill(); }
        else { g.fillStyle = '#f7f7f5'; g.fillRect(0, 0, 96, 128); g.fillStyle = p.endsWith('.pdf') ? '#e5484d' : '#4f9dff'; g.fillRect(0, 0, 96, 22); g.fillStyle = '#cfcfcf'; for (let y = 36; y < 118; y += 12) g.fillRect(12, y, y % 24 ? 72 : 52, 5); }
        return c.toDataURL(); }
      if (name === 'send_list') return [{ path: 'C:\Work\cover.png', name: 'cover.png', size: 480000, available: true, directory: false }, { path: 'C:\Work\proposal.pdf', name: 'proposal.pdf', size: 1230000, available: true, directory: false }, { path: 'C:\Work\demo.mp4', name: 'demo.mp4', size: 42000000, available: true, directory: false }];
      if (name === 'send_scan') return [{ id: 'p1', alias: 'Pixel 8', model: 'Pixel', address: '192.168.1.8:53317', protocol: 'https', kind: 'localsend' }, { id: 'p2', alias: 'iPad', model: 'iPad', address: '192.168.1.9:53317', protocol: 'https', kind: 'localsend' }, { id: 'p3', alias: 'Galaxy S24', model: 'Samsung', address: 'quickshare', protocol: 'quickshare', kind: 'quickshare' }];
      if (name === 'agents_state') return sessions;
      if (name === 'agent_pending') return [];
      if (name === 'agent_usage') return usage;
      if (name === 'agent_hooks') return { claude_code: true, codex: true };
      if (name === 'meeting_scan') return { candidates: cands, pick: 100, build: 26300, isolation: true, recording: false };
      if (name === 'meeting_list') return { items: meetings, folder: 'Documents\\Sora Recordings', consent_seen: true };
      if (name === 'meeting_state') return { state: 'idle' };
      if (name === 'app_marks') return marks;
      if (name === 'history_list') return { enabled: true, items: [
        { id: 1, cat: 'social', app: 'WhatsApp', title: 'Nadia', detail: 'Are we still on for lunch at 12?', at: sec - 90, reply: 'wa:1@s.whatsapp.net' },
        { id: 2, cat: 'social', app: 'WhatsApp', title: 'Nadia', detail: 'I can book the table', at: sec - 60 },
        { id: 3, cat: 'social', app: 'Telegram', title: 'Design team', detail: 'Sam: the new mockups are in the folder', at: sec - 900, reply: 'tg:2' },
        { id: 4, cat: 'social', app: 'Discord', title: '#release', detail: 'Version 1.0 is out, thanks everyone!', at: sec - 3600 }] };
      return null;
    } } };
  }, [STORE, MARKS]);
  await page.goto('http://127.0.0.1:18772/index.html'); await page.evaluate(() => document.fonts.ready); await page.waitForTimeout(700);
  await page.mouse.click(500, 30); await page.waitForTimeout(1600);
  await page.addStyleTag({ content: 'html,body{background:transparent!important}' });
  const raw = {};
  for (const [tab] of SHOTS) {
    // The chat history opens the tall panel; for the gallery it keeps the normal height (rounded foot in the frame).
    if (tab === 'history') { await page.locator('#history-button').click(); await page.waitForTimeout(1400); await page.addStyleTag({ content: '#notch{height:234px!important;max-height:234px!important;overflow:hidden!important}' }); await page.waitForTimeout(300); }
    else if (tab !== 'home') { await page.locator(`.nav[data-tab="${tab}"]`).click(); await page.waitForTimeout(tab === 'send' ? 3500 : 1400); }
    const f = path.join(out, `raw-${tab}.png`);
    await page.screenshot({ path: f, omitBackground: true, clip: { x: 140, y: 0, width: 720, height: 234 } });
    raw[tab] = 'data:image/png;base64,' + fs.readFileSync(f).toString('base64');
  }
  if (RAW) { await browser.close(); server.close(); console.log('raw shots ok', out, errors.slice(0, 3)); return; }
  // Each shot framed: the banner's warm background, the panel hanging from a slim screen edge, a title and one line.
  const frame = await browser.newPage({ viewport: { width: 900, height: 420 }, deviceScaleFactor: 2 });
  for (const [tab, title, line] of SHOTS) {
    await frame.setContent(`<html><head><link rel="stylesheet" href="http://127.0.0.1:18772/fonts.css"></head><body style="margin:0;width:900px;height:420px;overflow:hidden;font-family:Poppins,'Segoe UI',sans-serif;
      background:radial-gradient(70% 90% at 15% 10%,#fbf6ef 0%,#f3ede6 45%,transparent 70%),radial-gradient(60% 80% at 90% 95%,#e6ddf7 0%,transparent 70%),radial-gradient(50% 60% at 55% 100%,#fbe3d6 0%,transparent 70%),#f4efe9">
      <div style="position:absolute;left:44px;top:34px"><div style="font-size:44px;font-weight:600;color:#141414;letter-spacing:-.8px">${title}</div>
      <div style="font-size:22px;color:#6f6a66;margin-top:2px">${line}</div></div>
      <div style="position:absolute;left:30px;right:30px;top:150px;height:22px;background:linear-gradient(90deg,#3a3a3c,#232325 20%,#232325);border-radius:8px 8px 0 0"></div>
      <div style="position:absolute;left:90px;top:172px;width:720px;height:234px;filter:drop-shadow(0 18px 30px rgba(40,30,60,.28))"><img src="${raw[tab]}" width="720" height="234" style="display:block"></div>
      </body></html>`);
    await frame.waitForTimeout(500);
    await frame.screenshot({ path: path.join(out, `${tab}.png`) });
    fs.unlinkSync(path.join(out, `raw-${tab}.png`));
  }
  await browser.close(); server.close();
  console.log('shots ok', out, errors.slice(0, 3));
})().catch(e => { console.error(e); process.exit(1); });
