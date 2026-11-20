// Microsoft Store listing images, from the real panel (tools/readme-shots.cjs with STORE=1 RAW=1 SCALE=4, made first)
// and the Mac icon (logo/sora-logo.svg, framed as tools/mac-icons/render.cjs does). Writes into Store/listing-images:
//   screenshot-1..5.png  3840 x 2160, a title and one line over the panel, on the banner's warm background;
//   hero.png             3840 x 2160 Super hero art (no text: the Store writes the name over it);
//   box-art.png          2160 x 2160; poster.png 1440 x 2160 (icon and name); tile-300/150/71.png.
// usage: node tools/store-art.cjs
const { chromium } = require((() => { try { return require.resolve('playwright'); } catch { return require('path').resolve(__dirname, '..', '..', 'test-env', 'node_modules', 'playwright'); } })());
const http = require('node:http'), fs = require('node:fs'), path = require('node:path');
const full = path.resolve(__dirname, '..'), ui = path.join(full, 'ui');
const out = path.resolve(full, '..', 'Store', 'listing-images'), raw = path.join(out, 'raw');
const svg = fs.readFileSync(path.resolve(full, '..', 'logo', 'sora-logo.svg'), 'utf8');
const inner = svg.slice(svg.indexOf('>', svg.indexOf('<svg')) + 1, svg.lastIndexOf('</svg>'));
// The Mac icon: the tile 824 of 1024 with rounded corners and a soft shadow (render.cjs).
const icon = (n, shadow = true) => `<svg xmlns="http://www.w3.org/2000/svg" width="${n}" height="${n}" viewBox="0 0 1024 1024">
  <defs><filter id="drop" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="10" stdDeviation="14" flood-color="#000" flood-opacity="${shadow ? '.32' : '0'}"/></filter></defs>
  <rect x="100" y="100" width="824" height="824" rx="185" fill="#000" filter="url(#drop)"/>
  <svg x="100" y="100" width="824" height="824" viewBox="0 0 1024 1024">${inner.replace('rx="224"', 'rx="230"')}</svg></svg>`;
const shot = tab => 'data:image/png;base64,' + fs.readFileSync(path.join(raw, `raw-${tab}.png`)).toString('base64');
const WARM = `radial-gradient(70% 90% at 12% 8%,#fbf6ef 0%,#f3ede6 45%,transparent 70%),radial-gradient(60% 80% at 92% 96%,#e6ddf7 0%,transparent 70%),radial-gradient(50% 60% at 55% 100%,#fbe3d6 0%,transparent 70%),#f4efe9`;
const DARK = `radial-gradient(60% 70% at 18% 20%,#1d2a6b 0%,transparent 70%),radial-gradient(55% 65% at 88% 85%,#3a2a55 0%,transparent 70%),#0b0b10`;
const page = (w, h, bg, body) => `<html><head><link rel="stylesheet" href="http://127.0.0.1:18773/fonts.css"></head>
  <body style="margin:0;width:${w}px;height:${h}px;overflow:hidden;position:relative;font-family:Poppins,'Segoe UI',sans-serif;background:${bg}">${body}</body></html>`;
// The panel hanging from a slim screen edge (the bar ends where the panel starts), w css px wide.
const panel = (tab, x, y, w) => { const h = w * 234 / 720;
  return `<div style="position:absolute;left:${x - w * 0.06}px;top:${y - w * 0.03}px;width:${w * 1.12}px;height:${w * 0.03}px;background:linear-gradient(90deg,#3a3a3c,#232325 20%,#232325);border-radius:${w * 0.012}px ${w * 0.012}px 0 0"></div>
  <img src="${shot(tab)}" style="position:absolute;left:${x}px;top:${y}px;width:${w}px;height:${h}px;filter:drop-shadow(0 ${w * 0.03}px ${w * 0.045}px rgba(40,30,60,.3))">`; };
const SHOTS = [
  ['home', 'Everything at a glance', 'Weather, calendar, focus timer, markets and more, right under the notch.'],
  ['music', 'Music with lyrics', 'Control what is playing in any app, with synced lyrics.'],
  ['shelf', 'A tray for your files', 'Drop files on the notch, convert them or send them to your phone.'],
  ['send', 'Send to your phone', 'Files go over Wi-Fi with LocalSend or Quick Share, and phones can send back.'],
  ['board', 'Everything you copy', 'Text, links, images and code on a row of cards.'],
  ['agents', 'Your AI agents', 'Follow Claude Code, Codex and Gemini CLI, and approve their actions.'],
];
const server = http.createServer((req, res) => {
  const f = path.join(ui, req.url.split('?')[0].slice(1));
  if (!f.startsWith(ui) || !fs.existsSync(f)) { res.writeHead(404); return res.end(); }
  res.setHeader('Content-Type', f.endsWith('.css') ? 'text/css' : f.endsWith('.woff2') ? 'font/woff2' : 'application/octet-stream'); res.end(fs.readFileSync(f));
});
(async () => {
  await new Promise(r => server.listen(18773, '127.0.0.1', r));
  const browser = await chromium.launch({ channel: 'msedge' });
  const render = async (file, w, h, html, scale = 2) => {
    const p = await browser.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: scale });
    await p.setContent(html, { waitUntil: 'networkidle' }); await p.evaluate(() => document.fonts.ready); await p.waitForTimeout(300);
    await p.screenshot({ path: path.join(out, file), omitBackground: file.startsWith('tile') }); await p.close();
  };
  // Screenshots: 1920 x 1080 css at 2x.
  for (const [i, [tab, title, line]] of SHOTS.entries()) {
    await render(`screenshot-${i + 1}.png`, 1920, 1080, page(1920, 1080, WARM, `
      <div style="position:absolute;left:150px;top:120px"><div style="font-size:84px;font-weight:600;letter-spacing:-2px;color:#141414">${title}</div>
      <div style="font-size:36px;color:#6f6a66;margin-top:10px">${line}</div></div>${panel(tab, 210, 470, 1500)}`));
  }
  // Super hero art: no text; the icon and the open panel.
  await render('hero.png', 1920, 1080, page(1920, 1080, WARM, `<div style="position:absolute;left:150px;top:300px">${icon(460)}</div>${panel('home', 720, 360, 1060)}`));
  // Box art and poster.
  await render('box-art.png', 1080, 1080, page(1080, 1080, DARK, `<div style="position:absolute;left:190px;top:190px">${icon(700)}</div>`));
  await render('poster.png', 720, 1080, page(720, 1080, DARK, `<div style="position:absolute;left:110px;top:230px">${icon(500)}</div>
    <div style="position:absolute;left:0;right:0;top:760px;text-align:center;color:#fff;font-size:76px;font-weight:600;letter-spacing:-2px">Sora Notch</div>
    <div style="position:absolute;left:0;right:0;top:860px;text-align:center;color:#a9a9b4;font-size:28px">A friendly notch for your desktop</div>`));
  // Tiles: the icon alone on transparent.
  for (const n of [300, 150, 71]) await render(`tile-${n}.png`, n, n, `<html><body style="margin:0;background:transparent">${icon(n, n > 100)}</body></html>`, 1);
  await browser.close(); server.close();
  console.log('store art ok', out);
})().catch(e => { console.error(e); process.exit(1); });
