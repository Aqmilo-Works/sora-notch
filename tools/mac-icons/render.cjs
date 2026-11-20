// Makes Sora's macOS artwork from the logo (Sora-Release/logo/sora-logo.svg), with Playwright (test-env) and Edge:
//   icons/mac-icon.png        1024 px app icon on Apple's grid (and icons/mac/, its .icns sizes): the tile 824 px with rounded corners, a soft shadow, a
//                             transparent margin (macOS 26 frames a full-bleed icon in grey);
//   icons/dmg-background.png  the install window, light (Finder writes the icons' names in black): Sora on the left,
//                             Applications on the right, an arrow between;
//   icons/tray-mac.png        the menu bar icon: the face's two eyes in black on transparent (a template image);
//   icons/icon.png, icon.ico  the same logo for Windows and the Store.
// usage: node tools/mac-icons/render.cjs
const path = require('path');
const fs = require('fs');
const { chromium } = require((() => { try { return require.resolve('playwright'); } catch { return require('path').resolve(__dirname, '..', '..', '..', 'test-env', 'node_modules', 'playwright'); } })());
const full = path.resolve(__dirname, '..', '..');
const svg = fs.readFileSync(path.resolve(full, '..', 'logo', 'sora-logo.svg'), 'utf8');
const inner = svg.slice(svg.indexOf('>', svg.indexOf('<svg')) + 1, svg.lastIndexOf('</svg>'));
const icon = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs><filter id="drop" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="10" stdDeviation="14" flood-color="#000" flood-opacity=".32"/></filter></defs>
  <rect x="100" y="100" width="824" height="824" rx="185" fill="#000" filter="url(#drop)"/>
  <svg x="100" y="100" width="824" height="824" viewBox="0 0 1024 1024">${inner.replace('rx="224"', 'rx="230"')}</svg>
</svg>`;
// The eyes alone (">" and the oval), as the menu bar shows the app: black, the system tints it.
const tray = `<svg xmlns="http://www.w3.org/2000/svg" width="72" height="36" viewBox="120 495 900 450">
  <path d="M262 668 Q392 712 478 752 Q390 802 276 868" fill="none" stroke="#000" stroke-width="86" stroke-linecap="round" stroke-linejoin="round"/>
  <ellipse cx="788" cy="708" rx="104" ry="158" transform="rotate(10 788 708)" fill="#000"/>
</svg>`;
const W = 660, H = 400;
const dmg = `<div style="width:${W}px;height:${H}px;position:relative;font-family:-apple-system,'SF Pro Text','Segoe UI',sans-serif;
  background:radial-gradient(120% 90% at 50% 0%,#ffffff 0%,#f2f2f5 55%,#e7e7ec 100%);color:#1c1c1e;overflow:hidden">
  <div style="position:absolute;left:0;right:0;top:34px;text-align:center;font-size:22px;font-weight:700;letter-spacing:-.2px">Sora Notch</div>
  <div style="position:absolute;left:0;right:0;top:66px;text-align:center;font-size:13px;color:#6e6e73">Drag Sora into Applications</div>
  <svg style="position:absolute;left:270px;top:186px" width="120" height="28" viewBox="0 0 120 28"><path d="M6 14h96M88 4l14 10-14 10" fill="none" stroke="#a1a1a8" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/></svg>
  <div style="position:absolute;left:0;right:0;bottom:22px;text-align:center;font-size:11px;color:#8e8e93">First open: System Settings › Privacy &amp; Security › Open Anyway</div>
</div>`;
(async () => {
  const browser = await chromium.launch({ channel: 'msedge' });
  const shoot = async (html, file, w, h, scale) => {
    const page = await browser.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: scale });
    await page.setContent(`<html><body style="margin:0;background:transparent">${html}</body></html>`);
    await page.screenshot({ path: path.join(full, 'icons', file), omitBackground: true, clip: { x: 0, y: 0, width: w, height: h } });
    await page.close();
  };
  await shoot(icon, 'mac-icon.png', 1024, 1024, 1);
  // The sizes an .icns holds (the bundler makes it from these; "@2x" marks the double-density one).
  fs.mkdirSync(path.join(full, 'icons', 'mac'), { recursive: true });
  for (const n of [16, 32, 64, 128, 256, 512]) await shoot(icon.replace('width="1024" height="1024"', `width="${n}" height="${n}"`), `mac/${n}x${n}.png`, n, n, 1);
  await shoot(icon, 'mac/512x512@2x.png', 1024, 1024, 1);
  await shoot(tray, 'tray-mac.png', 72, 36, 1);
  await shoot(dmg, 'dmg-background.png', W, H, 1);
  // Windows, the Store's logos (msix/build-msix.ps1) and the README use the Mac icon itself: the same tile, corners,
  // shadow and margin, so Sora has one icon everywhere. icon.ico holds PNG images at the sizes Windows asks for.
  const win = n => icon.replace('width="1024" height="1024"', `width="${n}" height="${n}"`);
  await shoot(win(256), 'icon.png', 256, 256, 1);
  const sizes = [16, 24, 32, 48, 64, 256], pngs = [];
  for (const n of sizes) { await shoot(win(n), `ico-${n}.png`, n, n, 1); pngs.push(fs.readFileSync(path.join(full, 'icons', `ico-${n}.png`))); fs.unlinkSync(path.join(full, 'icons', `ico-${n}.png`)); }
  const head = Buffer.alloc(6 + 16 * sizes.length);
  head.writeUInt16LE(0, 0); head.writeUInt16LE(1, 2); head.writeUInt16LE(sizes.length, 4);
  let at = head.length;
  sizes.forEach((n, i) => { const o = 6 + 16 * i; head[o] = n % 256; head[o + 1] = n % 256; head.writeUInt16LE(1, o + 4); head.writeUInt16LE(32, o + 6); head.writeUInt32LE(pngs[i].length, o + 8); head.writeUInt32LE(at, o + 12); at += pngs[i].length; });
  fs.writeFileSync(path.join(full, 'icons', 'icon.ico'), Buffer.concat([head, ...pngs]));
  await browser.close();
  console.log('icons/mac-icon.png, icons/tray-mac.png, icons/dmg-background.png, icons/icon.png, icons/icon.ico');
})();
