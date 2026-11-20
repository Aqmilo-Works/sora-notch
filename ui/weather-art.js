// The Weather widget's sky as a small gradient illustration: back to front, the sun or moon, the cloud, then rain,
// snow, lightning or mist, each filled with its own soft gradient. A front layer cuts a thin gap into what is behind
// it, so overlapping shapes stay apart. Drawn in a 100 x 60 box, then measured and fitted (see drawSky).
const circle = (c, x, y, r) => { c.moveTo(x + r, y); c.arc(x, y, r, 0, Math.PI * 2); };
const lin = (c, x0, y0, x1, y1, stops) => { const g = c.createLinearGradient(x0, y0, x1, y1); stops.forEach((s, i) => g.addColorStop(i / (stops.length - 1), s)); return g; };
const rad = (c, x, y, r0, r1, stops) => { const g = c.createRadialGradient(x, y, r0, x, y, r1); stops.forEach((s, i) => g.addColorStop(i / (stops.length - 1), s)); return g; };

const CLOUD = { day: ['#ffffff', '#e9eef5', '#b9c3d1'], night: ['#e6e9f2', '#b7bfd0', '#7d879c'], rain: ['#eef4fb', '#b9c7d9', '#7f90a8'], storm: ['#d9d4ea', '#9e94bd', '#5e557e'], fog: ['#dfe4ec', '#b3bccb', '#8a94a6'] };
const cloudPath = (c, dx, dy, s) => { c.save(); c.translate(50 + dx, 40 + dy); c.scale(s, s); c.translate(-50, -40); c.beginPath();
  circle(c, 32, 36, 13); circle(c, 50, 27, 18); circle(c, 68, 33, 13); c.roundRect(18, 34, 64, 16, 8); c.restore(); };
const cloud = (tone = 'day', dx = 0, dy = 0, s = 1) => ({
  path: c => cloudPath(c, dx, dy, s),
  paint: c => {
    cloudPath(c, dx, dy, s); c.fillStyle = lin(c, 0, 9 + dy, 0, 50 + dy, CLOUD[tone]); c.fill();
    // A soft light from the top left.
    cloudPath(c, dx, dy, s); c.fillStyle = rad(c, 40 + dx, 18 + dy, 0, 34, ['#ffffff59', '#ffffff00']); c.fill();
  },
});
const sun = (x, y, r) => ({
  path: c => { c.beginPath(); circle(c, x, y, r); },
  paint: c => {
    c.beginPath(); circle(c, x, y, r * 1.5); c.fillStyle = rad(c, x, y, r * .8, r * 1.5, ['#ffb34066', '#ffb34000']); c.fill();
    c.beginPath(); circle(c, x, y, r); c.fillStyle = rad(c, x - r * .35, y - r * .4, r * .1, r * 1.15, ['#fff4b8', '#ffd23f', '#ff9500']); c.fill();
  },
});
const moon = (x, y, r) => ({
  path: c => { c.beginPath(); circle(c, x, y, r); },
  paint: c => {
    c.beginPath(); circle(c, x, y, r * 1.45); c.fillStyle = rad(c, x, y, r * .7, r * 1.45, ['#b8c6ff40', '#b8c6ff00']); c.fill();
    c.beginPath(); circle(c, x, y, r); c.fillStyle = lin(c, x - r, y - r, x + r, y + r, ['#ffffff', '#dfe5ff', '#9eacf0']); c.fill();
    c.globalCompositeOperation = 'destination-out'; c.beginPath(); circle(c, x + r * .52, y - r * .43, r * .82); c.fill();
  },
});
// extra: hangs below the cloud and does not count for the size or the place of the drawing.
const drops = { extra: true, stroke: 3.4, path: c => { c.beginPath(); for (const [x, y] of [[40, 45], [54, 47], [68, 45], [47, 54], [61, 54]]) { c.moveTo(x, y); c.lineTo(x - 2.4, y + 5); } },
  paint: c => { c.lineWidth = 3.4; c.lineCap = 'round'; drops.path(c); c.strokeStyle = lin(c, 0, 44, 0, 60, ['#9fd3ff', '#3d8bff']); c.stroke(); } };
const flakes = { extra: true, path: c => { c.beginPath(); for (const [x, y] of [[40, 47], [54, 49], [68, 47], [47, 55], [61, 55]]) circle(c, x, y, 2.6); },
  paint: c => { flakes.path(c); c.fillStyle = lin(c, 0, 44, 0, 58, ['#ffffff', '#cfe3ff']); c.fill(); } };
const bolt = { extra: true, path: c => { c.beginPath(); c.moveTo(55, 33); c.lineTo(43, 48); c.lineTo(51, 48); c.lineTo(45, 60); c.lineTo(61, 43); c.lineTo(53, 43); c.lineTo(59, 33); c.closePath(); },
  paint: c => { bolt.path(c); c.fillStyle = lin(c, 0, 33, 0, 60, ['#fff3a3', '#ffd60a', '#ff9f0a']); c.fill(); } };
const mist = { extra: true, path: c => { c.beginPath(); c.roundRect(22, 46, 50, 4.6, 2.3); c.roundRect(32, 54, 46, 4.6, 2.3); },
  paint: c => { mist.path(c); c.fillStyle = lin(c, 20, 0, 80, 0, ['#ffffff40', '#e6ebf2', '#aab3bf80']); c.fill(); } };

/** Back to front. minor: a sun or moon peeking behind a cloud, which places the drawing but does not size it. */
function layers(sky) {
  const night = sky.endsWith('night'), kind = sky.split('-')[0];
  switch (kind) {
    case 'clear': return [night ? moon(50, 30, 22) : sun(50, 30, 21)];
    case 'partly': return [{ ...(night ? moon(64, 21, 16) : sun(64, 21, 15)), minor: true }, cloud(night ? 'night' : 'day', -8, 8, .86)];
    case 'cloudy': return night ? [{ ...moon(72, 14, 10), minor: true }, cloud('night')] : [cloud('day')];
    case 'fog': return [cloud('fog', 0, -9), mist];
    case 'rain': return [cloud('rain', 0, -9), drops];
    case 'snow': return [cloud('day', 0, -9), flakes];
    default: return [cloud('storm', 0, -11), bolt];
  }
}

// Every sky is sized by its main shape (cloud, sun or moon) to fit one frame (FRAME_W x FRAME_H), so they read as one
// set, and placed with its body (glows aside) at the frame's top-left corner, which the CSS puts on the place name's
// top and the text's left edge. A round sun or moon is a little smaller, as a circle looks bigger than a cloud of the
// same height. MARGIN, round the frame, leaves room for the glows.
const MARGIN = 12, FRAME_W = 64, FRAME_H = 44, OPTICAL = { clear: .95 };
export function drawSky(canvas, sky = 'clear-day') {
  canvas.dataset.sky = sky;
  const w = canvas.clientWidth, h = canvas.clientHeight, dpr = window.devicePixelRatio || 1;
  if (!w || !h) return;
  canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr);
  // Drawn large on a hidden canvas first, then measured and fitted.
  const k = 4 * dpr, list = layers(sky), off = document.createElement('canvas'); off.width = 140 * k; off.height = 100 * k;
  const o = off.getContext('2d', { willReadFrequently: true });
  o.translate(20 * k, 20 * k); o.scale(k, k);
  const paint = skip => list.forEach((layer, i) => {
    if (skip(layer)) return;
    if (i) { // the gap: this layer's outline, a little wider, cut out of everything behind it
      o.save(); o.globalCompositeOperation = 'destination-out'; o.lineWidth = (layer.stroke || 0) + 4.5; o.lineJoin = o.lineCap = 'round';
      layer.path(o); if (!layer.stroke) o.fill(); o.stroke(); o.restore();
    }
    o.save(); layer.paint(o); o.restore();
  });
  const clear = () => { o.save(); o.setTransform(1, 0, 0, 1, 0, 0); o.clearRect(0, 0, off.width, off.height); o.restore(); };
  const box = () => { const d = o.getImageData(0, 0, off.width, off.height).data; let left = off.width, top = off.height, right = 0, bottom = 0;
    for (let y = 0; y < off.height; y++) for (let x = 0; x < off.width; x++) if (d[(y * off.width + x) * 4 + 3] > 150) { if (x < left) left = x; if (x > right) right = x; if (y < top) top = y; if (y > bottom) bottom = y; }
    return { left, top, right, bottom }; };
  paint(l => l.extra || l.minor); const size = box(); if (size.right < size.left) return;
  clear(); paint(l => l.extra); const { left, top } = box();
  clear(); paint(() => false);
  const s = Math.min(FRAME_W * dpr / (size.right - size.left + 1), FRAME_H * dpr / (size.bottom - size.top + 1)) * (OPTICAL[sky.split('-')[0]] || 1);
  const c = canvas.getContext('2d'); c.setTransform(1, 0, 0, 1, 0, 0); c.clearRect(0, 0, canvas.width, canvas.height);
  c.imageSmoothingQuality = 'high'; c.translate(MARGIN * dpr, MARGIN * dpr); c.scale(s, s); c.drawImage(off, -left, -top); c.setTransform(1, 0, 0, 1, 0, 0);
}
