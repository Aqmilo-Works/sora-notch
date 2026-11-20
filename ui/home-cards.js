// The Home widgets that are not the classic blocks: Screen Time and Water (data in src/health.rs), Prayer
// (worked out on this PC from the weather location, see prayer.js) and Markets (src/main.rs quotes). Each only refreshes
// while it is on Home.
import { codeOf, nameOf, tagOf, listKind, findLocal, guessIdx, kindOfRemote } from './market-codes.js';
import { PRAYER_METHODS, DEFAULT_METHOD, PRAYERS, PRAYER_NAMES, clock, timesFor, nextPrayer, untilText } from './prayer.js';

/** 90 -> "1.5 h", 4063 -> "68 h", 20 -> "20 min" */
export const playHours = m => m < 60 ? m + ' min' : (m < 600 ? (m / 60).toFixed(1) : Math.round(m / 60)) + ' h';
export const WATER_GOAL = 10;
export const WATER_STEPS = [30, 45, 60, 90, 120];
/** 125 s -> "2m", 4000 s -> "1h 07m" */
export const duration = s => s < 60 ? (s ? '<1m' : '0m') : s < 3600 ? Math.round(s / 60) + 'm' : Math.floor(s / 3600) + 'h ' + String(Math.round(s % 3600 / 60)).padStart(2, '0') + 'm';
/** The next reminder interval after `minutes` (wraps around). */
export const nextStep = minutes => WATER_STEPS[(WATER_STEPS.indexOf(minutes) + 1) % WATER_STEPS.length];
/** A Markets symbol's short name: the common ones by name, else the symbol without its exchange suffix. */
const NAMES = { 'GC=F': 'Gold', 'SI=F': 'Silver', 'BTC-USD': 'Bitcoin', 'ETH-USD': 'Ethereum', '^JKSE': 'IHSG', '^GSPC': 'S&P 500', '^IXIC': 'Nasdaq', '^DJI': 'Dow' };
export const marketName = sym => NAMES[sym] || codeOf(sym);
export const DEFAULT_SYMBOLS = ['BTC-USD', 'GC=F', '^JKSE'];
/** Shown in the search before anything is typed. */
export const POPULAR = [['GC=F', 'Gold'], ['BTC-USD', 'Bitcoin'], ['^JKSE', 'IHSG'], ['BBCA.JK', 'Bank Central Asia'], ['ETH-USD', 'Ethereum'], ['SI=F', 'Silver'], ['^GSPC', 'S&P 500'], ['AAPL', 'Apple'], ['NVDA', 'NVIDIA'], ['IDR=X', 'USD / IDR']];
/** The sparkline of a day's prices in a w x h box: the line, and the area under it. */
export function sparkPath(points, w, h) {
  // Points keep their place in time; a missing price (the market paused) is bridged by the line, never skipped over.
  const v = points.filter(Number.isFinite); if (v.length < 2) return null;
  const lo = Math.min(...v), hi = Math.max(...v), span = hi - lo || 1, n = points.length - 1 || 1;
  const xy = points.map((p, i) => Number.isFinite(p) ? [i / n * w, h - 2 - (p - lo) / span * (h - 4)] : null).filter(Boolean);
  const line = 'M' + xy.map(([x, y]) => x.toFixed(1) + ' ' + y.toFixed(1)).join('L');
  return { line, area: line + 'L' + xy.at(-1)[0].toFixed(1) + ' ' + h + 'L' + xy[0][0].toFixed(1) + ' ' + h + 'Z' };
}
/** "btc-usd, gc=f ^jkse" -> ['BTC-USD','GC=F','^JKSE']; null when one is not a symbol or there are more than eight. */
export function parseSymbols(text) {
  const list = [...new Set(String(text).toUpperCase().split(/[\s,;]+/).filter(Boolean))];
  return list.length && list.length <= 8 && list.every(x => /^[A-Z0-9^=.-]{1,16}$/.test(x)) ? list : null;
}
/** A plain glass: an outline when empty, filled with the pastel water gradient when drunk. */
const GLASS = '<svg viewBox="0 0 30 34" aria-hidden="true"><path d="M3 3h24l-2.6 27a2.5 2.5 0 0 1-2.5 2.3H8.1a2.5 2.5 0 0 1-2.5-2.3Z"/></svg>';

export function initHomeCards({ invoke, native, settings, icon, tab, read, persist, has, visible, showError }) {
  const $ = id => document.getElementById(id);
  let health = null, busy = false;

  // ---- Screen Time and Water
  function renderHealth(h) {
    health = h;
    const p = h.prefs, on = p.screen_time;
    $('screen-off').hidden = on || h.today > 0;
    $('screen-today').hidden = $('screen-apps').hidden = !$('screen-off').hidden;
    $('screen-today').textContent = duration(h.today);
    $('screen-week').textContent = h.week ? '7 days · ' + duration(h.week) : '';
    const top = h.apps[0]?.secs || 1;
    $('screen-apps').replaceChildren(...h.apps.map(a => {
      const r = document.createElement('div'), n = document.createElement('span'), b = document.createElement('i'), t = document.createElement('time');
      r.className = 's-app'; n.textContent = n.title = a.name; b.style.setProperty('--w', Math.max(4, Math.round(a.secs / top * 100)) + '%'); t.textContent = duration(a.secs);
      r.append(n, b, t); return r;
    }));
    if (on && !h.apps.length) { const e = document.createElement('p'); e.className = 's-empty'; e.textContent = 'It fills in as you use your PC.'; $('screen-apps').replaceChildren(e); }

    $('water-glasses').textContent = h.glasses;
    $('water-undo').disabled = !h.glasses;
    $('water-drops').replaceChildren(...Array.from({ length: WATER_GOAL }, (_, i) => { const d = document.createElement('i'); d.className = i < h.glasses ? 'on' : ''; d.innerHTML = GLASS; return d; }));
    $('water-bell').setAttribute('aria-pressed', String(p.water));
    $('water-bell').innerHTML = icon(p.water ? 'bell' : 'bell-off');
    const left = p.water && h.next_secs != null ? h.next_secs : null;
    $('water-every').textContent = !p.water ? 'Reminder off' : p.water_minutes + ' min' + (left == null ? '' : ' · ' + duration(left));
    $('water-every').disabled = !p.water;
  }
  async function refreshHealth() { if (!native) return; try { renderHealth(await invoke('health_get')); } catch {} }
  async function saveHealth(change) {
    if (busy || !health) return; busy = true;
    try { renderHealth(await invoke('health_set', { prefs: { ...health.prefs, ...change } })); } catch (e) { showError(e); } finally { busy = false; }
  }
  $('screen-on').onclick = () => saveHealth({ screen_time: true });
  $('water-bell').onclick = () => saveHealth({ water: !health?.prefs.water });
  $('water-every').onclick = () => saveHealth({ water_minutes: nextStep(health?.prefs.water_minutes) });
  $('water-undo').onclick = async () => { if (!native || !health?.glasses) return; try { renderHealth(await invoke('health_undrank')); } catch (e) { showError(e); } };
  $('water-drank').onclick = async () => { if (!native) return; try { renderHealth(await invoke('health_drank')); } catch (e) { showError(e); } };

  // ---- Qibla and Prayer
  function renderPlace() {
    const lat = Number(settings.latitude), lon = Number(settings.longitude), now = new Date();
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) return;
    const city = settings.city || '';
    $('prayer-city').textContent = city;

    const method = PRAYER_METHODS[settings.prayerMethod] ? settings.prayerMethod : DEFAULT_METHOD, asr = settings.asr === 'hanafi' ? 'hanafi' : 'shafi';
    const times = timesFor(now, lat, lon, method, asr), next = nextPrayer(now, lat, lon, method, asr);
    $('prayer-name').textContent = PRAYER_NAMES[next.name];
    $('prayer-at').textContent = clock(next.at);
    $('prayer-in').textContent = next.at == null ? '' : 'in ' + untilText(next.in);
    $('prayer-list').replaceChildren(...PRAYERS.map(k => {
      const c = document.createElement('div'), n = document.createElement('small'), t = document.createElement('b');
      c.className = 'p-cell' + (k === next.name ? ' next' : '') + (k !== next.name && (next.tomorrow || PRAYERS.indexOf(k) < PRAYERS.indexOf(next.name)) ? ' past' : '');
      n.textContent = PRAYER_NAMES[k]; t.textContent = clock(times[k]); c.append(n, t); return c;
    }));
  }

  // ---- Markets: one market per page (name, price, change, the day's line), the wheel turns the pages.
  let symbols = read('sora.markets', DEFAULT_SYMBOLS); if (!parseSymbols((symbols || []).join(','))) symbols = [...DEFAULT_SYMBOLS];
  let quotes = [], page = 0;
  const money = n => n >= 1000 ? n.toLocaleString('en-US', { maximumFractionDigits: 0 }) : n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: n < 1 ? 4 : 2 });
  const list = $('markets-list');
  function renderMarkets() {
    list.replaceChildren(...symbols.map(sym => {
      const q = quotes.find(x => x.symbol === sym) || { symbol: sym }, ok = Number.isFinite(q.price) && Number.isFinite(q.previous) && q.previous;
      const diff = ok ? q.price - q.previous : 0, pct = ok ? diff / q.previous * 100 : 0, sp = ok && sparkPath(q.points || [], 200, 46);
      const el = document.createElement('div'); el.className = 'mk-page' + (ok ? (diff >= 0 ? ' up' : ' down') : ''); el.style.setProperty('--i', symbols.indexOf(sym));
      const head = document.createElement('div'), name = document.createElement('b'), sub = document.createElement('small');
      head.className = 'mk-top'; name.textContent = marketName(sym); sub.textContent = marketName(sym) === codeOf(sym) ? (nameOf(sym) || sym) : codeOf(sym); head.append(name, sub);
      const price = document.createElement('strong'), ch = document.createElement('span');
      price.textContent = ok ? money(q.price) : (q.error ? 'Not found' : '—'); ch.className = 'mk-ch';
      ch.textContent = ok ? (diff >= 0 ? '+' : '−') + money(Math.abs(diff)) + ' (' + (pct >= 0 ? '+' : '−') + Math.abs(pct).toFixed(2) + '%)' : '';
      el.append(head, price, ch);
      if (sp) el.insertAdjacentHTML('beforeend', '<svg class="mk-spark" viewBox="0 0 200 46" preserveAspectRatio="none" aria-hidden="true"><path class="a" d="' + sp.area + '"/><path class="l" d="' + sp.line + '"/></svg>');
      return el;
    }));
    $('mk-dots').replaceChildren(...symbols.map((_, i) => { const d = document.createElement('i'); d.className = i === page ? 'on' : ''; return d; }));
    list.style.setProperty('--page', page);
  }
  function turn(to) { page = Math.max(0, Math.min(symbols.length - 1, to)); list.style.setProperty('--page', page); [...$('mk-dots').children].forEach((d, i) => d.classList.toggle('on', i === page)); }
    // One page per flick: a wheel or touchpad sends a burst of events, so the burst turns one page, never several.
  let wheelAt = 0;
  list.addEventListener('wheel', e => { e.preventDefault(); const d = Math.abs(e.deltaY) > Math.abs(e.deltaX) ? e.deltaY : e.deltaX, now = performance.now(); if (Math.abs(d) < 2) return; const quiet = now - wheelAt > 220; wheelAt = now; if (quiet) turn(page + (d > 0 ? 1 : -1)); }, { passive: false });
  // Hold the left button and drag sideways: the pages follow the pointer, and let go turns to the nearest page.
  let grab = null;
  list.addEventListener('pointerdown', e => { if (e.button !== 0) return; grab = { x: e.clientX, w: list.getBoundingClientRect().width || 1, at: page }; list.setPointerCapture(e.pointerId); list.classList.add('drag'); });
  list.addEventListener('pointermove', e => { if (grab) list.style.setProperty('--page', Math.max(0, Math.min(symbols.length - 1, grab.at - (e.clientX - grab.x) / grab.w))); });
  const drop = e => { if (!grab) return; const dx = (e.clientX - grab.x) / grab.w; grab = null; list.classList.remove('drag'); turn(Math.abs(dx) < .15 ? page : page - Math.sign(dx)); };
  list.addEventListener('pointerup', drop); list.addEventListener('pointercancel', drop);
  list.addEventListener('keydown', e => { if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') { e.preventDefault(); turn(page + (e.key === 'ArrowRight' ? 1 : -1)); } });
  // Yahoo may answer too-frequent requests with errors: after a failed round the wait doubles (30 s up to 5 min, with some
  // randomness), the last prices stay on screen, and one good round brings back the normal pace. Never twice within 5 s.
  const MARKET_EVERY = 15000, MARKET_MAX = 300000;
  let marketsBusy = false, marketFails = 0, marketHold = 0;
  async function refreshMarkets() {
    renderMarkets(); if (!native || marketsBusy || Date.now() < marketHold) return;
    marketsBusy = true; let bad = true;
    try {
      const got = await invoke('quotes', { symbols }), old = new Map(quotes.map(q => [q.symbol, q]));
      quotes = got.map(q => q.error && Number.isFinite(old.get(q.symbol)?.price) ? old.get(q.symbol) : q);
      bad = got.length > 0 && got.every(q => q.error);
      renderMarkets();
    } catch {} finally {
      marketFails = bad ? marketFails + 1 : 0;
      const wait = bad ? Math.min(MARKET_MAX, MARKET_EVERY * 2 ** marketFails) * (0.8 + Math.random() * 0.4) : 5000;
      marketHold = Date.now() + wait; marketsBusy = false;
      document.querySelector('[data-widget=markets]')?.toggleAttribute('data-delayed', bad);
    }
  }

  // The Markets menu: what is watched (code and name), a search that understands codes (BBCA, BTC, gold) and a type filter.
  let kind = 'all', searchTimer = 0, searchId = 0;
  function saveSymbols(next) { symbols = next; page = Math.min(page, symbols.length - 1); persist('sora.markets', symbols); drawChosen(); }
  function drawChosen() {
    $('mk-count').textContent = symbols.length + '/8';
    $('mk-chosen').replaceChildren(...symbols.map(sym => {
      const b = document.createElement('button'); b.type = 'button'; b.className = 'mk-row'; b.innerHTML = '<b></b><small></small><i aria-hidden="true">×</i>';
      b.querySelector('b').textContent = codeOf(sym); b.querySelector('small').textContent = nameOf(sym) || marketName(sym);
      b.setAttribute('aria-label', 'Stop watching ' + codeOf(sym));
      b.onclick = () => { if (symbols.length === 1) { $('markets-error').textContent = 'Keep at least one.'; return; } $('markets-error').textContent = ''; saveSymbols(symbols.filter(x => x !== sym)); drawResults(shown); };
      return b;
    }));
  }
  const asRow = c => ({ symbol: c.symbol, code: c.code || codeOf(c.symbol), name: c.name, kind: c.kind, mkt: c.mkt });
  let shown = [];
  function drawResults(rows) {
    shown = rows;
    $('mk-results').replaceChildren(...rows.map(r => {
      const b = document.createElement('button'), c = document.createElement('b'), n = document.createElement('span'), tag = document.createElement('em'), act = document.createElement('i'), on = symbols.includes(r.symbol);
      b.type = 'button'; b.className = 'mk-result' + (on ? ' on' : ''); b.setAttribute('role', 'option'); b.setAttribute('aria-selected', String(on));
      b.setAttribute('aria-label', (on ? 'Stop watching ' : 'Watch ') + r.code + ', ' + r.name);
      c.textContent = r.code; n.textContent = r.name; tag.className = 'mk-tag'; tag.textContent = tagOf(r); act.className = 'mk-act'; act.textContent = on ? 'Added' : 'Add'; b.append(c, n, tag, act);
      b.onclick = () => {
        $('markets-error').textContent = '';
        if (on) { if (symbols.length > 1) saveSymbols(symbols.filter(x => x !== r.symbol)); }
        else if (symbols.length >= 8) { $('markets-error').textContent = 'Eight at most. Remove one first.'; return; }
        else saveSymbols([...symbols, r.symbol]);
        drawResults(shown);
      };
      return b;
    }));
    if (!rows.length) { const p = document.createElement('p'); p.className = 'mk-none'; p.textContent = 'No match. Try the code, like BBCA, BTC or gold.'; $('mk-results').append(p); }
  }
  // Codes answer at once from the list; the online search follows a moment later and adds what the list does not know.
  function runSearch() {
    clearTimeout(searchTimer); const q = $('markets-input').value.trim(), id = ++searchId; $('markets-error').textContent = '';
    if (!q) { drawResults(listKind(kind).map(asRow)); return; }
    const local = findLocal(q, kind).map(asRow), guess = !local.length && (kind === 'all' || kind === 'stock') ? guessIdx(q) : null;
    drawResults(guess ? [asRow(guess)] : local);
    searchTimer = setTimeout(async () => {
      if (!native) return;
      try {
        const found = await invoke('market_search', { query: q }); if (id !== searchId) return;
        const seen = new Set(local.map(x => x.symbol));
        const extra = found.map(x => ({ symbol: x.symbol, code: codeOf(x.symbol), name: x.name, kind: kindOfRemote(x) })).filter(x => !seen.has(x.symbol) && (kind === 'all' || x.kind === kind));
        drawResults([...(guess && !extra.length ? [asRow(guess)] : []), ...local, ...extra].slice(0, 12));
      } catch { if (id === searchId) $('markets-error').textContent = 'Online search is offline. The codes in the list still work.'; }
    }, 300);
  }
  $('markets-input').oninput = runSearch;
  document.querySelectorAll('.mk-tabs button').forEach(b => b.onclick = () => { kind = b.dataset.kind; document.querySelectorAll('.mk-tabs button').forEach(x => x.setAttribute('aria-pressed', String(x === b))); runSearch(); });
  $('markets-form').onsubmit = e => { e.preventDefault(); $('mk-results').querySelector('.mk-result:not(.on)')?.click(); };
  $('markets-edit').onclick = () => { tab('markets-editor'); kind = 'all'; document.querySelectorAll('.mk-tabs button').forEach(x => x.setAttribute('aria-pressed', String(x.dataset.kind === 'all'))); $('markets-input').value = ''; drawChosen(); runSearch(); $('markets-input').focus(); };
  $('markets-back').onclick = () => { quotes = []; marketHold = 0; tab('home'); refreshMarkets(); };

  // ---- Steam Games (src/steam.rs): a black card with the game you played last in big letters and its cover in front of the next two, fanned. The front cover starts the game, another one comes forward.
  let games = [], sel = 0, running = 0, loaded = false; const covers = new Map(), asked = new Set();
  const ago = s => { if (!s) return 'Not played yet'; const d = Math.floor((Date.now() / 1000 - s) / 86400); return d <= 0 ? 'Today' : d === 1 ? 'Yesterday' : d < 30 ? d + ' days ago' : d < 365 ? Math.floor(d / 30) + ' mo ago' : 'Over a year ago'; };
  /** The cover once Steam's cache gave it; asks for it the first time. */
  function art(g) {
    if (!covers.has(g.id) && !asked.has(g.id) && native) { asked.add(g.id); invoke('steam_cover', { id: g.id }).then(u => { covers.set(g.id, u); drawSteam(); }).catch(() => {}); }
    return covers.get(g.id);
  }
  function drawSteam() {
    const stack = $('steam-stack'), n = games.length;
    if (!n) {
      // No Steam, or no games in it: the same three covers, drawn as outlines, and a line saying so.
      $('steam-name').textContent = 'No games yet'; $('steam-when').textContent = 'Is Steam installed?'; $('steam-when').classList.remove('live');
      stack.replaceChildren(...[0, 1, 2].map(d => { const g = document.createElement('div'); g.className = 'steam-card ghost'; g.dataset.d = d; if (!d) g.innerHTML = icon('gamepad'); return g; })); return;
    }
    sel = Math.min(sel, n - 1); const g = games[sel];
    $('steam-name').textContent = g.name; $('steam-when').textContent = running === g.id ? 'Playing now' : ago(g.played) + (g.minutes ? ' · ' + playHours(g.minutes) : '');
    $('steam-when').classList.toggle('live', running === g.id);
    if (stack.children.length !== n || stack.querySelector('.ghost')) stack.replaceChildren(...games.map((o, i) => {
      const b = document.createElement('button'); b.type = 'button'; b.className = 'steam-card'; b.setAttribute('aria-label', 'Play ' + o.name); b.textContent = o.name.trim()[0] || '?';
      b.onclick = () => { if (b.dataset.d === '0') { if (native) invoke('steam_launch', { id: games[i].id }).catch(showError); } else { sel = i; drawSteam(); } };
      return b;
    }));
    games.forEach((o, i) => {
      const b = stack.children[i], d = (i - sel + n) % n, a = d < 3 ? art(o) : null;
      b.dataset.d = d; b.style.setProperty('--d', d); b.tabIndex = d ? -1 : 0; b.setAttribute('aria-label', (d ? 'Bring forward ' : 'Play ') + o.name);
      if (a && !b.querySelector('img')) { const im = new Image(); im.alt = ''; im.src = a; b.textContent = ''; b.append(im); }
    });
  }
  // One game per flick: a wheel or touchpad sends a burst of events, so the burst turns one game, never several.
  let steamWheelAt = 0;
  document.querySelector('[data-widget="steam"]').addEventListener('wheel', e => {
    if (!games.length) return; e.preventDefault();
    const d = Math.abs(e.deltaY) > Math.abs(e.deltaX) ? e.deltaY : e.deltaX, now = performance.now(), quiet = now - steamWheelAt > 220; steamWheelAt = now;
    if (Math.abs(d) < 2 || !quiet) return; sel = (sel + (d > 0 ? 1 : -1) + games.length) % games.length; drawSteam();
  }, { passive: false });
  async function refreshSteam() { if (!native) return; try { const [g, now] = await Promise.all([invoke('steam_games'), invoke('steam_running')]); const list = g.slice(0, 10); const first = !loaded; loaded = true;
      if (first || JSON.stringify(list) !== JSON.stringify(games) || now !== running) { if (now && now !== running) sel = Math.max(0, list.findIndex(x => x.id === now)); games = list; running = now; drawSteam(); } } catch {} }

  function refresh() {
    if (has('steam')) refreshSteam();
    if (has('screen') || has('water')) refreshHealth();
    if (has('prayer')) renderPlace();
    if (has('markets')) refreshMarkets();
  }
  setInterval(() => { if (visible() && has('markets')) refreshMarkets(); }, MARKET_EVERY);
  setInterval(() => { if (visible() && (has('screen') || has('water'))) refreshHealth(); }, 15000);
  setInterval(() => { if (visible() && has('prayer')) renderPlace(); }, 20000);
  setInterval(() => { if (visible() && has('steam')) refreshSteam(); }, 15000);
  return { refresh, renderPlace };
}
