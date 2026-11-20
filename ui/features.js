// The Board (what was copied) and the Chat history tabs. Data lives in Rust (board.rs, history.rs),
// on this PC only; this file draws it. No classic controls: segmented buttons, chips and one switch, in the flat black style.
const APP_COLOR = { WhatsApp: '#25d366', Telegram: '#2aabee', Discord: '#5865f2', Messenger: '#0a7cff', Instagram: '#e1306c', Signal: '#3a76f0', LINE: '#06c755', WeChat: '#07c160', Slack: '#e01e5a', Teams: '#6264a7', Zalo: '#0068ff', Facebook: '#1877f2', X: '#e7e7ea', TikTok: '#25f4ee', Gmail: '#ea4335', Outlook: '#0078d4', Snapchat: '#fffc00', ChatGPT: '#10a37f', Claude: '#d97757', Perplexity: '#1fb8cd', Copilot: '#6d6df0', Gemini: '#3186ff', Grok: '#c9c9ce', DeepSeek: '#4d6bfe', Mistral: '#fa520f', Qwen: '#615ced', Kimi: '#1783ff', Lark: '#3370ff' };

const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };
const ago = secs => {
  const d = Math.max(0, Date.now() / 1000 - secs);
  if (d < 60) return 'now';
  if (d < 3600) return Math.floor(d / 60) + ' min';
  if (d < 86400) return Math.floor(d / 3600) + ' h';
  if (d < 172800) return 'Yesterday';
  return new Date(secs * 1000).toLocaleDateString('en-GB', { day: 'numeric', month: 'short' });
};
const hostOf = url => { try { return new URL(/^https?:\/\//i.test(url) ? url : 'https://' + url).hostname.replace(/^www\./, ''); } catch { return url; } };

export function initFeatures({ invoke, listen, $, icon, toast, showError, notch, sendTo }) {
  // ---- a small context menu of our own (no native one)
  let menuEl = null;
  const closeMenu = () => { menuEl?.remove(); menuEl = null; };
  function menu(e, items) {
    e.preventDefault(); closeMenu();
    menuEl = el('div', 'fx-menu');
    menuEl.setAttribute('role', 'menu');
    for (const it of items) {
      if (!it) { menuEl.append(el('hr')); continue; }
      const b = el('button', it.danger ? 'danger' : '', it.label);
      b.type = 'button'; b.setAttribute('role', 'menuitem');
      b.onclick = ev => { ev.stopPropagation(); closeMenu(); it.run(); };
      menuEl.append(b);
    }
    notch.append(menuEl);
    const r = notch.getBoundingClientRect(), k = r.width / notch.offsetWidth || 1;
    const w = menuEl.offsetWidth, h = menuEl.offsetHeight;
    menuEl.style.left = Math.max(6, Math.min(notch.offsetWidth - w - 6, (e.clientX - r.left) / k)) + 'px';
    menuEl.style.top = Math.max(6, Math.min(notch.offsetHeight - h - 6, (e.clientY - r.top) / k)) + 'px';
  }
  document.addEventListener('pointerdown', e => { if (menuEl && !menuEl.contains(e.target)) closeMenu(); }, true);
  document.addEventListener('keydown', e => { if (e.key === 'Escape' && menuEl) { closeMenu(); e.stopPropagation(); } }, true);
  window.addEventListener('blur', closeMenu);

  // A group of buttons of which one is on.
  function segmented(host, options, value, onPick) {
    host.replaceChildren();
    for (const [v, label, count] of options) {
      const b = el('button', 'fx-seg' + (v === value ? ' on' : ''), label);
      if (count != null) b.append(el('small', '', String(count)));
      b.type = 'button'; b.setAttribute('aria-pressed', String(v === value));
      b.onclick = () => onPick(v);
      host.append(b);
    }
  }

  // ================= Board
  // A shelf of clean cards, like the clipboard shelf of Supaste: search, type filters with counts, one row that runs sideways.
  // A card has its content on top and a small footer (kind, age); text is dark on paper, a colour fills its card, a picture fills its card.
  const board = { items: [], filter: 'all', q: '', sel: '' };

  const FILTERS = [['all', 'All'], ['text', 'Text'], ['code', 'Code'], ['link', 'Links'], ['image', 'Images']];
  // Code is its own filter; Text is the rest of what was typed or copied as words.
  const matches = (i, v) => v === 'all' || (v === 'code' ? i.tag === 'code' : v === 'text' ? i.kind === 'text' && i.tag !== 'code' : i.kind === v);
  async function loadBoard() {
    try { const s = await invoke('board_list'); board.items = s?.items || []; if (s?.capture === false) invoke('board_set_capture', { on: true }).catch(() => {}); } catch (e) { showError(e); }
    drawBoard();
  }
  // A colour as CSS: a bare hex (FFF2A5) gets its #.
  const cssColor = t => { t = t.trim(); return /^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(t) ? '#' + t : t; };
  const KIND = { text: ['Text', '#ff8a5c'], code: ['Code', '#4fd1b0'], email: ['Email', '#4fd1b0'], phone: ['Phone', '#4fd1b0'], color: ['Color', '#a98bff'], image: ['Image', '#4f9dff'], link: ['Link', '#4f9dff'] };
  // Black or white text, whichever reads on this colour; a colour the page cannot parse gets white.
  const inkOn = css => { const c = document.createElement('canvas').getContext('2d'); c.fillStyle = '#000'; c.fillStyle = css; const m = /^#([0-9a-f]{6})/i.exec(c.fillStyle); if (!m) return '#fff'; const n = parseInt(m[1], 16); return (0.299 * (n >> 16) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255)) > 150 ? '#111' : '#fff'; };
  // HEX, RGB, CMYK and HSL of a colour the page can parse (the canvas turns any CSS colour into #rrggbb).
  function colorRows(css, saved) {
    const c = document.createElement('canvas').getContext('2d'); c.fillStyle = '#000'; c.fillStyle = css;
    const m = /^#([0-9a-f]{6})$/i.exec(c.fillStyle); if (!m) return [['Saved', saved]];
    const n = parseInt(m[1], 16), r = n >> 16, g = (n >> 8) & 255, b = n & 255, R = r / 255, G = g / 255, B = b / 255;
    const k = 1 - Math.max(R, G, B), cm = k === 1 ? [0, 0, 0] : [(1 - R - k) / (1 - k), (1 - G - k) / (1 - k), (1 - B - k) / (1 - k)];
    const mx = Math.max(R, G, B), mn = Math.min(R, G, B), d = mx - mn, l = (mx + mn) / 2;
    const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
    const h = d === 0 ? 0 : mx === R ? ((G - B) / d + 6) % 6 : mx === G ? (B - R) / d + 2 : (R - G) / d + 4;
    const p = v => Math.round(v * 100) + '%';
    return [['Saved', saved], ['HEX', m[0].toUpperCase()], ['RGB', `rgb(${r}, ${g}, ${b})`], ['CMYK', `cmyk(${cm.map(p).join(', ')}, ${p(k)})`], ['HSL', `hsl(${Math.round(h * 60)}, ${p(s)}, ${p(l)})`]];
  }
  // The preview under the shelf: the whole text, the picture as copied, the full link or every reading of a colour. The panel
  // grows to fit it (up to 634; a longer one scrolls inside): the page and the notch take the same height (panel_tall).
  const NORMAL = 234, MOST = 634, CHROME = 66;
  let fitted = 0;
  function fit() {
    requestAnimationFrame(() => {
      if (!board.sel) return;
      const h = Math.round(Math.min(MOST, Math.max(NORMAL, CHROME + $('board').offsetHeight))), first = !fitted;
      if (h === fitted) return;
      fitted = h; notch.style.setProperty('--pv-h', h + 'px');
      // Closed again before the panel answered (fast clicks): it must not grow on the shelf.
      (first ? invoke('panel_view', { name: 'board-preview' }) : Promise.resolve()).then(() => fitted === h && invoke('panel_tall', { px: h - NORMAL })).catch(() => {});
    });
  }
  const release = () => { fitted = 0; notch.style.removeProperty('--pv-h'); };
  // An empty id closes the preview.
  function setPreview(id) {
    board.sel = id || '';
    if (id) { notch.dataset.view = 'board-preview'; showPreview(); return; }
    release(); notch.dataset.view = 'board';
    invoke('panel_view', { name: 'board' }).catch(() => {});
    showPreview();
  }
  // The cards stay as they are (only the picked one changes), so the second click of a double-click lands on the same card.
  function showPreview() {
    for (const c of $('bd-grid').children) c.classList.toggle('sel', c.dataset.id === board.sel);
    const open = board.items.find(i => i.id === board.sel);
    drawPreview(open);
    $('board').classList.toggle('previewing', !!open);
    if (open) fit();
  }
  const sentence = t => t.toLowerCase().replace(/(^\s*|[.!?]\s+)(\p{L})/gu, (_, p, c) => p + c.toUpperCase());
  const title = t => t.toLowerCase().replace(/(^|\s)(\p{L})/gu, (_, p, c) => p + c.toUpperCase());
  // ---- a link's page: its own title, description and picture, fetched by the app only when this preview asks. A link with a query
  // that looks like it carries a one-time token or a login (reset, verify, magic link, access_token, user:pass@) waits for a click: a visit
  // could use it up. Everything else (a video, an article, a repository) loads at once.
  // A link copied without http(s):// may be a word that only looks like a domain (budi.design): it also waits for a click.
  const askFirst = u => !/^https?:\/\//i.test(u.trim()) || sensitive(u);
  const sensitive = u => /[?&#][^=&#]*(token|key|code|auth|sig|session|reset|verify|confirm|magic|otp|pass|secret|ticket|invite|access|hash|jwt|state|nonce|credential)/i.test(u) || /\/(reset|verify|confirm|activate|unsubscribe|magic|invite|login|signin|sign-in|auth|oauth|callback)/i.test(u) || /\/\/[^/]*@/.test(u);
  const linkCache = new Map(); // address -> { state: 'loading' | 'ok' | 'err', data, note }
  const PLAY = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5.5v13l11-6.5Z"/></svg>';
  // The card on the shelf shows the page too, so it is drawn again when the page arrives.
  const refreshLink = it => {
    document.querySelector(`.bd-card[data-id="${it.id}"]`)?.replaceWith(boardCard(it));
    const b = document.querySelector('#bd-preview .pv-link'); if (b && board.sel === it.id) { renderLink(b, it); fit(); }
  };
  // The icon of the program something was copied from, once per program.
  const appIcons = new Map(); // program file -> Promise of a picture
  // A failure is not kept: the next drawing asks again. Without an icon the card keeps its dot.
  function appIcon(it, img) {
    if (!appIcons.has(it.src)) appIcons.set(it.src, invoke('board_app_icon', { src: it.src }).catch(() => { appIcons.delete(it.src); return ''; }));
    appIcons.get(it.src).then(u => { if (u) img.src = u; else img.replaceWith(el('i', 'bd-dot')); });
  }
  const fromApp = (it, cls) => { const im = el('img', cls); im.alt = ''; im.draggable = false; appIcon(it, im); return im; };
  function loadLink(it) {
    const url = it.text; linkCache.set(url, { state: 'loading' }); refreshLink(it);
    invoke('board_link_preview', { url })
      .then(d => linkCache.set(url, d ? { state: 'ok', data: d } : { state: 'err', note: 'No preview.' }))
      .catch(e => linkCache.set(url, { state: 'err', note: String(e?.message || e || 'No preview.') }))
      .finally(() => refreshLink(it));
  }
  function renderLink(body, it) {
    body.replaceChildren();
    const c = linkCache.get(it.text);
    if (c?.state === 'ok') {
      const d = c.data;
      body.classList.add('rich');
      if (d.image) {
        const im = el('img', 'pv-og' + (d.kind === 'image' ? ' whole' : '')); im.alt = ''; im.draggable = false; im.onload = () => fit(); im.src = d.image;
        // A video's picture is a play button: it opens the video in the browser.
        if (d.kind === 'video') {
          const play = el('button', 'pv-play'); play.type = 'button'; play.setAttribute('aria-label', 'Play in browser');
          play.innerHTML = PLAY; play.prepend(im); play.onclick = () => invoke('board_open_link', { id: it.id }).catch(showError); body.append(play);
        } else body.append(im);
      }
      const text = el('div', 'pv-ogtext');
      const site = el('small', 'pv-site');
      if (d.icon) { const ic = el('img', 'pv-ico'); ic.alt = ''; ic.draggable = false; ic.src = d.icon; site.append(ic); }
      site.append(document.createTextNode(d.site || hostOf(it.text)));
      text.append(site, el('b', '', d.title || hostOf(it.text)));
      if (d.description) text.append(el('p', 'pv-desc', d.description));
      text.append(el('span', 'pv-url', it.text));
      body.append(text);
      return;
    }
    body.classList.remove('rich');
    body.append(el('b', '', hostOf(it.text)), el('span', 'pv-url', it.text));
    if (c?.state === 'loading') body.append(el('span', 'pv-note', 'Loading the page…'));
    else if (c?.state === 'err') body.append(el('span', 'pv-note', c.note));
    else if (askFirst(it.text)) { const b = el('button', 'pv-btn ghost pv-load', 'Load page preview'); b.type = 'button'; b.onclick = () => loadLink(it); body.append(b); }
    else setTimeout(() => { if (board.sel === it.id && !linkCache.has(it.text)) loadLink(it); }, 0);
  }
  // ---- a picture: the wheel zooms it (at the cursor) from "fits the preview" up to 16 times, a drag moves it. Nothing undoes a zoom
  // by itself (not even fast clicks): only showing another item resets it. The picture is laid out at its own size, so a zoom shows the pixels of the original (the small copy only until it loads).
  const fullImages = new Map(); // picture id -> Promise of the picture as copied (a data address)
  let zoomMemo = null; // the zoom of the picture on show: kept when the preview is drawn again (a new copy arrived), dropped with the item
  // Scan text: Windows reads the picture (OCR, on this PC) and every word it finds gets a box on the picture. Pick any
  // amount, like selecting text: drag from one word to another (across lines too), Shift+click to stretch the pick,
  // Ctrl+click to add or drop one word. Copy puts only the picked words on the clipboard, in reading order.
  function scanText(box, it, button) {
    const body = box.querySelector('.pv-image');
    if (box.querySelector('.pv-ocrbar')) { box.querySelector('.pv-ocrbar').remove(); body.querySelector('.pv-ocr')?.remove(); body.classList.remove('scanning'); button.classList.remove('on'); return; }
    button.classList.add('on');
    const bar = el('div', 'pv-ocrbar'), note = el('span', 'pv-ocrnote', 'Reading the text...');
    bar.append(note); box.append(bar); fit();
    invoke('board_ocr', { id: it.id }).then(lines => {
      if (!bar.isConnected) return;
      const words = (lines || []).flatMap((l, li) => l.words.map(w => ({ ...w, li })));
      if (!words.length) { note.textContent = 'No text found in this picture.'; return; }
      const layer = el('div', 'pv-ocr'); layer.style.width = it.w + 'px'; layer.style.height = it.h + 'px';
      let picked = new Set(), anchor = null, base = null;
      const pb = (cls, label, run) => { const b = el('button', 'pv-btn ' + cls); b.type = 'button'; b.append(label); b.onclick = run; return b; };
      const all = pb('ghost', 'Select all', () => pick(picked.size < words.length ? new Set(words.keys()) : new Set()));
      // Words of one line join with a space, lines with a new line.
      const textOf = () => { let out = '', last = -1; for (const i of [...picked].sort((a, b) => a - b)) { const w = words[i]; out += last < 0 ? w.text : (w.li === last ? ' ' : '\n') + w.text; last = w.li; } return out; };
      const copy = pb('copy', 'Copy', () => { if (picked.size) invoke('board_put_text', { text: textOf() }).then(() => toast('Copied ' + picked.size + (picked.size === 1 ? ' word' : ' words'))).catch(showError); });
      const pick = set => {
        picked = set;
        boxes.forEach((b, i) => b.classList.toggle('on', set.has(i)));
        note.textContent = set.size ? set.size + ' of ' + words.length + ' words picked' : 'Drag over the text you want';
        copy.disabled = !set.size; all.textContent = set.size === words.length ? 'Clear' : 'Select all';
      };
      const range = (a, b) => { const s = new Set(base); for (let i = Math.min(a, b); i <= Math.max(a, b); i++) s.add(i); return s; };
      // Each box reaches halfway to its neighbours on the line and spans the line's height, so picked words read as one band.
      const boxes = words.map((w, i) => {
        const line = words.filter(o => o.li === w.li), top = Math.min(...line.map(o => o.y)), bottom = Math.max(...line.map(o => o.y + o.h));
        const prev = words[i - 1]?.li === w.li ? words[i - 1] : null, next = words[i + 1]?.li === w.li ? words[i + 1] : null, pad = Math.max(2, (bottom - top) * 0.18);
        const left = prev ? (prev.x + prev.w + w.x) / 2 : w.x - pad, right = next ? (w.x + w.w + next.x) / 2 : w.x + w.w + pad;
        const b = el('span', 'ocr-word');
        Object.assign(b.style, { left: left + 'px', top: top - pad + 'px', width: right - left + 'px', height: bottom - top + pad * 2 + 'px' });
        b.classList.toggle('first', !prev); b.classList.toggle('last', !next);
        b.addEventListener('pointerdown', e => {
          if (e.button !== 0) return; e.stopPropagation(); e.preventDefault();
          if (e.ctrlKey) { const s = new Set(picked); s.has(i) ? s.delete(i) : s.add(i); anchor = i; base = null; pick(s); return; }
          if (e.shiftKey && anchor !== null) { base = new Set(); pick(range(anchor, i)); base = null; return; }
          // A click on the only picked word drops it; otherwise a new pick starts here and grows while the button is held.
          if (picked.size === 1 && picked.has(i)) { anchor = null; pick(new Set()); return; }
          anchor = i; base = new Set(); pick(range(i, i));
        });
        b.addEventListener('pointerenter', () => { if (base) pick(range(anchor, i)); });
        return b;
      });
      const stop = () => { base = null; };
      window.addEventListener('pointerup', stop); window.addEventListener('pointercancel', stop);
      layer.append(...boxes); body.append(layer); body.classList.add('scanning'); body._place?.();
      bar.append(all, copy); pick(new Set()); fit();
    }).catch(e => { if (bar.isConnected) note.textContent = String(e?.message || e); });
  }
  function zoomImage(body, it, chip) {
    const img = body.querySelector('img');
    img.style.width = it.w + 'px'; img.style.height = it.h + 'px';
    let s = 1, tx = 0, ty = 0, fitS = 1, atFit = true, drag = null;
    if (zoomMemo?.id === it.id) ({ s, tx, ty, atFit } = zoomMemo);
    const apply = () => {
      const w = it.w * s, h = it.h * s, bw = body.clientWidth, bh = body.clientHeight;
      tx = w <= bw ? (bw - w) / 2 : Math.min(0, Math.max(bw - w, tx)); ty = h <= bh ? (bh - h) / 2 : Math.min(0, Math.max(bh - h, ty));
      img.style.transform = `translate(${tx}px,${ty}px) scale(${s})`;
      // The scanned lines sit on the picture and move and zoom with it.
      const layer = body.querySelector('.pv-ocr'); if (layer) { layer.style.transform = img.style.transform; layer.style.setProperty('--px', 1 / s + 'px'); }
      img.style.imageRendering = s >= 3 ? 'pixelated' : 'auto';
      body.classList.toggle('zoomed', !atFit);
      chip.textContent = atFit ? 'Scroll to zoom' : Math.round(s * 100) + '%';
      zoomMemo = { id: it.id, s, tx, ty, atFit };
    };
    const refit = () => { if (!body.clientWidth || !body.clientHeight) return; fitS = Math.min(body.clientWidth / it.w, body.clientHeight / it.h); s = atFit ? fitS : Math.max(s, fitS); apply(); };
    body._place = apply;
    new ResizeObserver(refit).observe(body);
    const at = e => { const r = body.getBoundingClientRect(), k = r.width / body.clientWidth || 1; return [(e.clientX - r.left) / k, (e.clientY - r.top) / k, k]; };
    body.addEventListener('wheel', e => {
      e.preventDefault();
      const [mx, my] = at(e), ns = Math.min(16, Math.max(fitS, s * (e.deltaY < 0 ? 1.2 : 1 / 1.2)));
      if (ns === s) return;
      tx = mx - (mx - tx) * ns / s; ty = my - (my - ty) * ns / s; s = ns; atFit = ns <= fitS * 1.001; if (atFit) s = fitS; apply();
    }, { passive: false });
    body.addEventListener('pointerdown', e => { if (atFit || e.button !== 0) return; const [, , k] = at(e); drag = { x: e.clientX, y: e.clientY, tx, ty, k }; body.setPointerCapture(e.pointerId); body.classList.add('moving'); });
    body.addEventListener('pointermove', e => { if (!drag) return; tx = drag.tx + (e.clientX - drag.x) / drag.k; ty = drag.ty + (e.clientY - drag.y) / drag.k; apply(); });
    const end = () => { drag = null; body.classList.remove('moving'); };
    body.addEventListener('pointerup', end); body.addEventListener('pointercancel', end);
  }
  // ---- code as in a terminal: a window bar (the language), then the lines; a shell command gets its prompt, longer code its line numbers
  const PROMPT = { Shell: '$', PowerShell: 'PS>', Batch: 'C:\\>' };
  function terminal(it) {
    const win = el('div', 'pv-term'), bar = el('div', 'pv-term-bar');
    bar.append(el('i'), el('i'), el('i'), el('span', '', it.lang || 'Code'));
    const lines = it.text.replace(/\r/g, '').split('\n'), prompt = PROMPT[it.lang] && lines.length <= 8 ? PROMPT[it.lang] : '';
    const pre = el('pre', 'pv-term-body' + (prompt ? ' prompted' : ''));
    lines.forEach((l, i) => {
      const row = el('span', 'ln');
      row.append(el('b', prompt ? 'ps' : 'no', prompt ? (l.trim() ? prompt : '') : String(i + 1)), document.createTextNode(l || ' '));
      pre.append(row);
    });
    win.append(bar, pre);
    return win;
  }
  function drawPreview(it) {
    const box = $('bd-preview'); box.hidden = !it; box.replaceChildren();
    if (!it) return;
    const key = it.kind === 'link' || it.kind === 'image' ? it.kind : it.tag;
    const [kindName] = KIND[key] || KIND.text;
    const isColor = it.kind === 'text' && it.tag === 'color';
    const rows = isColor ? colorRows(cssColor(it.text), it.text.trim()) : [];
    const head = el('div', 'pv-head');
    head.append(el('span', 'pv-chip', kindName));
    if (it.kind === 'image') head.append(el('span', 'pv-chip', it.w + ' × ' + it.h));
    else if (it.kind !== 'link' && !isColor) {
      if (it.tag === 'code' && it.lang) head.append(el('span', 'pv-chip', it.lang));
      head.append(el('span', 'pv-chip', (n => n + (n === 1 ? ' line' : ' lines'))(it.text.split('\n').length)));
      if (it.tag !== 'code') head.append(el('span', 'pv-chip', it.text.length + ' chars'));
    }
    let zoomChip = null;
    if (it.kind === 'image') { zoomChip = el('span', 'pv-chip pv-zoomchip', 'Scroll to zoom'); head.append(zoomChip); }
    const btn = (cls, label, run) => { const b = el('button', 'pv-btn ' + cls); b.type = 'button'; b.append(label); b.onclick = run; return b; };
    // Copy is one button with a small arrow beside it for the other forms (a colour as RGB, text in capitals, a link's host...).
    const put = text => invoke('board_put_text', { text }).then(() => toast('Copied')).catch(showError);
    const forms = isColor ? [...rows.filter(r => r[0] !== 'Saved').map(r => ({ label: 'Copy ' + r[0], run: () => put(r[1]) })), null, { label: 'Copy Text', run: () => put(it.text) }]
      : it.kind === 'link' ? [{ label: 'Copy Link', run: () => put(it.text) }, { label: 'Copy Host', run: () => put(hostOf(it.text)) }]
      : it.kind === 'image' ? (it.ocr ? [{ label: 'Copy Picture', run: () => invoke('board_copy', { id: it.id }).then(() => toast('Copied')).catch(showError) }, { label: 'Copy Text in Picture', run: () => put(it.ocr) }] : [])
      : [{ label: 'Copy Text', run: () => put(it.text) }, null, { label: 'Copy as UPPERCASE', run: () => put(it.text.toUpperCase()) }, { label: 'Copy as lowercase', run: () => put(it.text.toLowerCase()) }, { label: 'Copy as Sentence case', run: () => put(sentence(it.text)) }, { label: 'Copy as Title Case', run: () => put(title(it.text)) }];
    const split = el('div', 'pv-split');
    const copy = btn('copy', 'Copy', () => invoke('board_copy', { id: it.id }).then(() => toast('Copied')).catch(showError));
    split.append(copy);
    if (forms.length) {
      const more = btn('copy more', '', () => { const r = more.getBoundingClientRect(); menu({ preventDefault() {}, clientX: r.left, clientY: r.bottom + 4 }, forms); });
      more.innerHTML = icon('down'); more.setAttribute('aria-label', 'More ways to copy'); split.append(more);
    }
    const SAVE = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v11m0 0-4-4m4 4 4-4M5 19.5h14"/></svg>', SHARE = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20.5 4 3.5 11l6.5 2.5L12.5 20Zm0 0L10 13.5"/></svg>';
    const save = () => invoke('board_save', { id: it.id }).then(n => toast('Saved ' + n + ' in Downloads')).catch(showError);
    const share = () => invoke('board_share', { id: it.id }).then(sendTo).catch(showError);
    const saveBtn = btn('ghost', '', save), shareBtn = btn('ghost', '', share);
    saveBtn.innerHTML = SAVE + '<span>Save</span>'; shareBtn.innerHTML = SHARE + '<span>Share</span>';
    const GLOBE = '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.6 2.5 3.8 5.5 3.8 9s-1.2 6.5-3.8 9c-2.6-2.5-3.8-5.5-3.8-9S9.4 5.5 12 3Z"/></svg>', TERM = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4.5" width="18" height="15" rx="3"/><path d="m7.5 10 3 2.5-3 2.5M13 15h3.5"/></svg>';
    let extra = null;
    // The words Windows read in the picture (OCR, on this PC).
    if (it.kind === 'image') { extra = btn('ghost scan', '', () => scanText(box, it, extra)); extra.innerHTML = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 8V5.5A1.5 1.5 0 0 1 5.5 4H8m8 0h2.5A1.5 1.5 0 0 1 20 5.5V8m0 8v2.5a1.5 1.5 0 0 1-1.5 1.5H16m-8 0H5.5A1.5 1.5 0 0 1 4 18.5V16M8 9h8m-8 3h8m-8 3h5"/></svg><span>Scan text</span>'; }
    else if (it.kind === 'link') { extra = btn('icon', '', () => invoke('board_open_link', { id: it.id }).catch(showError)); extra.innerHTML = GLOBE; extra.setAttribute('aria-label', 'Open in browser'); }
    else if (it.kind === 'text' && it.tag === 'code') {
      // Opens a terminal and runs nothing: the code is put on the clipboard for you to paste and run yourself.
      extra = btn('icon', '', () => invoke('board_put_text', { text: it.text }).then(() => invoke('board_open_terminal')).then(n => toast(n + ' opened. The code is copied: paste it and press Enter.')).catch(showError));
      extra.innerHTML = TERM; extra.setAttribute('aria-label', 'Open a terminal');
    }
    const keep = btn('icon' + (it.keep ? ' on' : ''), '', () => invoke('board_keep', { id: it.id, keep: !it.keep }).catch(showError));
    keep.innerHTML = icon('pin'); keep.setAttribute('aria-label', it.keep ? 'Unkeep' : 'Keep');
    const close = btn('icon', '', () => setPreview(''));
    close.innerHTML = icon('close'); close.setAttribute('aria-label', 'Close preview');
    const tools = el('div', 'pv-tools'); tools.append(split, saveBtn, ...(it.kind === 'image' ? [shareBtn] : []), ...(extra ? [extra] : []), keep, close); head.append(tools);
    const body = el('div', 'pv-body ' + (it.kind === 'image' ? 'pv-image' : it.kind === 'link' ? 'pv-link' : isColor ? 'pv-color' : it.tag === 'code' ? 'pv-code' : 'pv-text'));
    if (it.kind === 'image') {
      const img = el('img'); img.src = it.thumb || ''; img.alt = ''; img.draggable = false; body.append(img);
      body.style.aspectRatio = it.w + ' / ' + it.h;
      queueMicrotask(() => zoomImage(body, it, zoomChip));
      // The card holds a small copy; the picture as copied replaces it as soon as it is read, once per picture (kept for the last few).
      if (!fullImages.has(it.id)) { fullImages.set(it.id, invoke('board_image', { id: it.id }).catch(() => null)); if (fullImages.size > 6) fullImages.delete(fullImages.keys().next().value); }
      fullImages.get(it.id).then(u => { if (u && board.sel === it.id && img.isConnected) img.src = u; });
    } else if (it.kind === 'link') renderLink(body, it);
    else if (isColor) {
      const sw = el('i', 'pv-swatch'); sw.style.background = cssColor(it.text);
      const list = el('dl', 'pv-rows');
      for (const [k, v] of rows) list.append(el('dt', '', k), el('dd', '', v));
      body.append(sw, list);
    } else if (it.tag === 'code') body.append(terminal(it));
    else body.append(el('pre', '', it.text));
    box.append(head, body);
  }
  function boardCard(it) {
    const key = it.kind === 'link' || it.kind === 'image' ? it.kind : it.tag;
    const [kind0, dot] = KIND[key] || KIND.text;
    const kindName = it.tag === 'code' && it.lang ? it.lang : kind0;
    const c = el('button', 'bd-card ' + it.kind + ' tag-' + it.tag + (it.keep ? ' kept' : '') + (it.id === board.sel ? ' sel' : ''));
    c.type = 'button'; c.dataset.id = it.id; c.style.setProperty('--dot', dot);
    const body = el('div', 'bd-body');
    if (it.kind === 'image') {
      const img = el('img'); img.src = it.thumb || ''; img.alt = ''; img.draggable = false;
      body.append(img);
    } else if (it.kind === 'link') {
      // The page itself once it is read: its picture, else its icon and title. A link that may carry a one-time token is not visited.
      const c0 = linkCache.get(it.text), d = c0?.state === 'ok' ? c0.data : null;
      if (d?.image) {
        c.classList.add('og'); const im = el('img'); im.src = d.image; im.alt = ''; im.draggable = false; body.append(im);
        if (d.kind === 'video') { const badge = el('i', 'bd-play'); badge.innerHTML = PLAY; body.append(badge); }
      }
      else if (d) {
        const site = el('small', 'bd-site');
        if (d.icon) { const ic = el('img', 'bd-ico'); ic.src = d.icon; ic.alt = ''; ic.draggable = false; site.append(ic); }
        site.append(d.site || hostOf(it.text)); body.append(site, el('b', 'bd-title', d.title || hostOf(it.text)));
      } else {
        body.append(el('b', 'bd-host', hostOf(it.text)), el('span', 'bd-url', it.text.replace(/^https?:\/\/(www\.)?/, '')));
        if (!c0 && !askFirst(it.text)) setTimeout(() => { if (!linkCache.has(it.text)) loadLink(it); }, 0);
      }
    } else if (it.tag === 'color') {
      c.style.setProperty('--fill', cssColor(it.text)); c.style.setProperty('--ink', inkOn(cssColor(it.text)));
      body.append(el('span', 'bd-hex', it.text.trim()));
    } else { body.append(el('span', 'bd-text', it.text.slice(0, 400))); }
    const foot = el('div', 'bd-foot');
    // Where it came from, as on Supaste: the program's icon and name in place of the dot and the kind (the preview still names the kind).
    foot.append(it.src ? fromApp(it, 'bd-app') : el('i', 'bd-dot'), el('span', 'bd-kind', it.app || kindName), el('span', 'bd-age', ago(it.at)));
    if (it.app) foot.title = kindName + ' copied from ' + it.app;
    const actions = el('div', 'bd-actions');
    const act = (svg, title, on, run) => { const a = el('span', 'bd-act' + (on ? ' on' : '')); a.innerHTML = icon(svg); a.title = title; a.onclick = ev => { ev.stopPropagation(); run(); }; return a; };
    actions.append(act('pin', it.keep ? 'Unkeep' : 'Keep', it.keep, () => invoke('board_keep', { id: it.id, keep: !it.keep }).catch(showError)), act('close', 'Remove', false, () => invoke('board_remove', { id: it.id }).catch(showError)));
    c.append(body, foot, actions);
    const copy = () => invoke('board_copy', { id: it.id }).then(() => toast('Copied')).catch(showError);
    // Every click opens or hides the preview, however fast they come. A card never copies on a click: Copy is in the preview.
    const toggle = () => setPreview(board.sel === it.id ? '' : it.id);
    c.onclick = toggle;
    c.oncontextmenu = e => menu(e, [
      { label: 'Copy', run: copy },
      { label: 'Preview', run: () => setPreview(it.id) },
      { label: 'Save', run: () => invoke('board_save', { id: it.id }).then(n => toast('Saved ' + n + ' in Downloads')).catch(showError) },
      ...(it.kind === 'image' ? [{ label: 'Share', run: () => invoke('board_share', { id: it.id }).then(sendTo).catch(showError) }] : []),
      null,
      { label: it.keep ? 'Unkeep' : 'Keep', run: () => invoke('board_keep', { id: it.id, keep: !it.keep }).catch(showError) },
      { label: 'Remove', danger: true, run: () => invoke('board_remove', { id: it.id }).catch(showError) },
    ]);
    c.onkeydown = e => { if (e.key === ' ') { e.preventDefault(); toggle(); } };
    return c;
  }
  const haystack = i => (i.kind === 'image' ? 'image screenshot ' + i.w + 'x' + i.h + ' ' + (i.ocr || '') : i.text + ' ' + i.tag).toLowerCase();
  function drawBoard() {
    const q = board.q.trim().toLowerCase();
    const found = board.items.filter(i => !q || haystack(i).includes(q));
    segmented($('bd-filters'), FILTERS.map(([v, l]) => [v, l, found.filter(i => matches(i, v)).length]), board.filter, v => { board.filter = v; drawBoard(); });
    // What is kept comes first.
    const shown = found.filter(i => matches(i, board.filter));
    const items = [...shown.filter(i => i.keep), ...shown.filter(i => !i.keep)];
    const grid = $('bd-grid');
    grid.replaceChildren(...items.map(boardCard));
    $('bd-empty').hidden = items.length > 0;
    $('bd-empty').querySelector('p').textContent = q ? 'No match.' : board.items.length ? 'Nothing of this kind yet.' : 'Copy text, a link or a screenshot and it lands here.';
    $('board-clear').classList.toggle('hidden', !board.items.some(i => !i.keep));
    // The preview opens only on a click: a removed or filtered-out item closes it.
    if (board.sel && !items.some(i => i.id === board.sel)) { setPreview(''); return; }
    showPreview();
  }
  // Leaving the tab (or any other view change) closes the preview; the panel height is handled by whoever changed the view.
  new MutationObserver(() => { if (board.sel && notch.dataset.view !== 'board-preview') { board.sel = ''; release(); drawBoard(); } }).observe(notch, { attributes: true, attributeFilter: ['data-view'] });
  document.addEventListener('keydown', e => { if (e.key === 'Escape' && board.sel && !menuEl) { e.stopPropagation(); setPreview(''); } }, true);
  // A reopened panel asks for the height of the tab it shows: the preview is closed so both agree.
  listen('panel-show', () => { if (board.sel) { board.sel = ''; release(); notch.dataset.view = 'board'; } if ($('board').classList.contains('active')) drawBoard(); });
  // The board runs sideways: the wheel scrolls it along.
  $('bd-grid').parentElement.addEventListener('wheel', e => { if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) { e.currentTarget.scrollLeft += e.deltaY; e.preventDefault(); } }, { passive: false });
  $('bd-search').oninput = e => { board.q = e.target.value; drawBoard(); };
  $('board-clear').onclick = () => invoke('board_clear').then(() => toast('Clipboard cleared. Kept items stay.')).catch(showError);
  listen('board-changed', loadBoard);

  // ================= Chat history
  // replyTo: the chat whose reply box is open ("wa:<chat id>", from whatsapp.rs), with its draft and last error.
  const chat = { items: [], enabled: true, cat: 'social', app: 'all', q: '', replyTo: '', draft: '', replyError: '', sending: false };
  async function loadHistory() {
    try { const s = await invoke('history_list'); chat.items = s?.items || []; if (s?.enabled === false) invoke('history_set_enabled', { on: true }).catch(() => {}); } catch (e) { showError(e); }
    drawHistory();
  }
  function drawHistory() {
    segmented($('hs-cat'), [['social', 'Social media'], ['ai', 'AI']], chat.cat, v => { chat.cat = v; chat.app = 'all'; drawHistory(); });
    const side = chat.items.filter(i => i.cat === chat.cat);
    const apps = [...new Set(side.map(i => i.app))];
    if (chat.app !== 'all' && !apps.includes(chat.app)) chat.app = 'all';
    const chips = $('hs-apps'); chips.replaceChildren();
    for (const a of ['all', ...apps]) {
      const b = el('button', 'fx-chip' + (a === chat.app ? ' on' : '')); b.type = 'button';
      if (a !== 'all') { b.append(logo(a, 'hs-mini', side.find(m => m.app === a && m.aumid)?.aumid)); b.style.setProperty('--c', marks[a] ? hex(marks[a].color) : (APP_COLOR[a] || '#4f9dff')); }
      b.append(document.createTextNode(a === 'all' ? 'All' : a));
      b.onclick = () => { chat.app = a; drawHistory(); }; chips.append(b);
    }
    const q = chat.q.trim().toLowerCase();
    const rows = side.filter(i => (chat.app === 'all' || i.app === chat.app) && (!q || (i.title + ' ' + i.detail).toLowerCase().includes(q)));
    $('hs-empty').hidden = rows.length > 0;
    $('hs-empty').querySelector('p').textContent = side.length ? 'No match.' : chat.cat === 'ai' ? 'Notifications of AI apps show up here.' : 'Notifications of chat and social apps show up here.';
    $('history-clear').classList.toggle('hidden', !side.length);
    $('hs-list').replaceChildren(...timeline(rows));
    const box = $('hs-list').querySelector('.hs-reply input');
    if (box && !chat.sending) { box.focus({ preventScroll: true }); box.setSelectionRange(box.value.length, box.value.length); }
  }
  // A linked WhatsApp chat (whatsapp.rs) or a Telegram one (telegram.rs) is answered here: a click on its notice, or
  // Reply on its card, opens the box. The key says which: "wa:<chat>" or "tg:<chat>".
  function openReply(key) {
    if (chat.replyTo !== key) { chat.draft = ''; chat.replyError = ''; }
    Object.assign(chat, { cat: 'social', app: 'all', q: '', replyTo: key }); $('hs-search').value = '';
    drawHistory();
    $('hs-list').querySelector('.hs-reply')?.scrollIntoView({ block: 'nearest' });
  }
  // "typing…" goes to the chat only while the user really types: at most every 4 s, and stopped when the box empties or closes.
  let typedAt = 0;
  function typing(on) {
    if (!chat.replyTo || (on && Date.now() - typedAt < 4000) || (!on && !typedAt)) return;
    typedAt = on ? Date.now() : 0;
    invoke('chat_typing', { key: chat.replyTo, on }).catch(() => {});
  }
  async function sendReply() {
    typedAt = 0;
    const text = chat.draft.trim(); if (!text || chat.sending) return;
    chat.sending = true; chat.replyError = ''; drawHistory();
    try { await invoke(chat.replyTo.startsWith('tg:') ? 'tg_send' : 'wa_send', { chat: chat.replyTo.slice(3), text }); chat.draft = ''; chat.replyTo = ''; toast('Sent'); }
    catch (e) { chat.replyError = String(e?.message || e || 'Not sent.'); }
    chat.sending = false; drawHistory();
  }
  function replyBox() {
    const box = el('div', 'hs-reply');
    const input = el('input'); input.type = 'text'; input.placeholder = 'Reply'; input.spellcheck = true; input.autocomplete = 'off'; input.maxLength = 4096; input.value = chat.draft; input.disabled = chat.sending; input.setAttribute('aria-label', 'Reply');
    input.oninput = () => { chat.draft = input.value; send.disabled = !input.value.trim(); typing(input.value.trim() !== ''); };
    input.onkeydown = e => { if (e.key === 'Enter' && !e.isComposing) { e.preventDefault(); sendReply(); } else if (e.key === 'Escape') { e.stopPropagation(); typing(false); chat.replyTo = ''; drawHistory(); } };
    const send = el('button', 'hs-send', chat.sending ? 'Sending' : 'Send'); send.type = 'button'; send.disabled = chat.sending || !chat.draft.trim(); send.onclick = sendReply;
    box.append(input, send);
    if (chat.replyError) { const err = el('p', 'hs-reply-err', chat.replyError); err.setAttribute('role', 'alert'); box.append(err); }
    return box;
  }
  // The apps' own marks, as the notifications show them.
  const marks = {};
  const hex = n => '#' + Number(n).toString(16).padStart(6, '0');
  // An installed app shows its own icon (asked for once, kept); the mark above is the stand-in, then the first letter.
  const icons = {};
  function logo(app, cls, aumid) {
    const m = marks[app], node = el('span', cls);
    if (aumid && icons[app] === undefined) { icons[app] = null; invoke('app_icon', { aumid }).then(u => { icons[app] = u; drawHistory(); }).catch(() => {}); }
    if (icons[app]) { node.classList.add('real'); const im = el('img'); im.src = icons[app]; im.alt = ''; im.draggable = false; node.append(im); }
    else if (m) { node.classList.add('logo'); node.style.setProperty('--bg', hex(m.color)); node.style.setProperty('--fg', hex(m.fg)); node.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="${m.path}"/></svg>`; }
    else { node.dataset.l = (app || '?').slice(0, 1).toUpperCase(); node.style.setProperty('--c', APP_COLOR[app] || '#4f9dff'); }
    return node;
  }
  invoke('app_marks').then(list => { for (const m of list || []) marks[m.name] = m; drawHistory(); }).catch(() => {});
  // Messages of one sender in a row (within half an hour) make one entry on a timeline; the rail runs down the left, an entry is
  // a card with the sender and the time, and its messages hang under it on a branch. Folded, it shows the newest two.
  const open = new Set();
  function timeline(rows) {
    const groups = [], answerable = new Set();
    for (const i of rows) {
      const g = groups[groups.length - 1];
      if (g && g.app === i.app && g.title === i.title && g.last.at - i.at < 1800) { g.items.push(i); g.last = i; } else groups.push({ app: i.app, title: i.title, items: [i], last: i, key: i.app + '|' + i.title + '|' + i.id });
    }
    return groups.map(g => {
      const row = el('div', 'hs-g');
      const node = logo(g.app, 'hs-node', g.items.find(m => m.aumid)?.aumid);
      const card = el('div', 'hs-card');
      const head = el('div', 'hs-ghead');
      const many = g.items.length > 2, isOpen = open.has(g.key);
      head.append(el('b', '', g.title || g.app), el('span', 'hs-app', g.app));
      if (g.items.length > 1) head.append(el('span', 'hs-count', String(g.items.length)));
      head.append(el('time', '', ago(g.items[0].at)));
      // Only the newest card of a chat gets the Reply button and box.
      const key = g.items.find(m => /^(wa|tg):/.test(m.reply || ''))?.reply, replies = !!key && !answerable.has(key);
      if (key) answerable.add(key);
      if (replies && chat.replyTo !== key) { const r = el('button', 'hs-replyto', 'Reply'); r.type = 'button'; r.onclick = () => openReply(key); head.append(r); }
      if (many) {
        const chev = el('button', 'hs-chev' + (isOpen ? ' open' : '')); chev.type = 'button'; chev.innerHTML = icon('down'); chev.setAttribute('aria-label', isOpen ? 'Fold' : 'Unfold'); chev.setAttribute('aria-expanded', String(isOpen));
        chev.onclick = () => { if (isOpen) open.delete(g.key); else open.add(g.key); drawHistory(); };
        head.append(chev);
      }
      const list = el('div', 'hs-msgs');
      const shown = many && !isOpen ? g.items.slice(0, 2) : g.items.slice(0, 10);
      for (const m of shown) {
        const line = el('div', 'hs-msg');
        line.append(el('span', 'hs-text', m.detail || '(no text)'));
        if (many && isOpen) line.append(el('time', '', ago(m.at)));
        line.oncontextmenu = e => menu(e, [
          { label: 'Copy message', run: () => invoke('board_put_text', { text: m.detail || m.title }).then(() => toast('Copied')).catch(showError) },
          { label: 'Delete', danger: true, run: () => invoke('history_remove', { id: m.id }).catch(showError) },
        ]);
        list.append(line);
      }
      if (many && isOpen && g.items.length > shown.length) list.append(el('span', 'hs-more', '+ ' + (g.items.length - shown.length) + ' older'));
      card.append(head, list);
      if (replies && chat.replyTo === key) card.append(replyBox());
      row.append(node, card);
      return row;
    });
  }
  $('hs-search').oninput = e => { chat.q = e.target.value; drawHistory(); };
  $('history-clear').onclick = () => invoke('history_clear', { cat: chat.cat }).then(() => toast('Cleared')).catch(showError);
  listen('history-changed', loadHistory);

  // Opened by name (a click on a copied-link card), or when a view is shown.
  const entered = name => { if (name === 'board') loadBoard(); if (name === 'history') { loadHistory(); invoke('chat_take_reply').then(openWanted).catch(() => {}); } };
  listen('chat-reply', () => invoke('chat_take_reply').then(openWanted).catch(() => {}));
  // A clicked notice: "cat:ai" / "cat:social" shows that side of the history, a chat key opens its reply box.
  function openWanted(k) { if (!k) return; if (!k.startsWith('cat:')) { openReply(k); return; } Object.assign(chat, { cat: k.slice(4), app: 'all', q: '', replyTo: '' }); $('hs-search').value = ''; drawHistory(); }
  loadBoard(); loadHistory();
  return { entered };
}
