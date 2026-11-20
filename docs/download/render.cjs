// Renders the README's download buttons (buttons.html) to windows.png and macos.png at 3x, and a preview of all three
// (with Microsoft's own Store badge) on GitHub's dark and light backgrounds. usage: node docs/download/render.cjs
const path = require('path');
const { chromium } = require((() => { try { return require.resolve('playwright'); } catch { return require('path').resolve(__dirname, '..', '..', '..', 'test-env', 'node_modules', 'playwright'); } })());
const here = __dirname;
(async () => {
  const b = await chromium.launch({ channel: 'msedge' });
  const p = await b.newPage({ deviceScaleFactor: 3 });
  await p.goto('file:///' + path.join(here, 'buttons.html').replace(/\\/g, '/'));
  for (const id of ['windows', 'macos']) await p.locator('#' + id).screenshot({ path: path.join(here, id + '.png'), omitBackground: true });
  const img = f => 'data:image/png;base64,' + require('fs').readFileSync(path.join(here, f)).toString('base64');
  const row = bg => `<div style="background:${bg};padding:22px 26px;display:flex;gap:8px;align-items:center">
    <img src="${img('windows.png')}" height="44"><img src="https://get.microsoft.com/images/en-us%20dark.svg" height="44"><img src="${img('macos.png')}" height="44"></div>`;
  const q = await b.newPage({ viewport: { width: 620, height: 200 }, deviceScaleFactor: 2 });
  await q.setContent(`<body style="margin:0">${row('#0d1117')}${row('#ffffff')}</body>`);
  await q.waitForTimeout(800);
  await q.screenshot({ path: path.join(here, '..', '..', 'dist', 'download-buttons-preview.png'), fullPage: true });
  await b.close();
  console.log('done');
})();
