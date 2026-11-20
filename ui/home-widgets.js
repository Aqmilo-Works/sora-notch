// Home as a row of three slots the person fills and arranges, so the notch keeps one calm shape. Every widget takes one
// slot; the Calendar can also be wide (two slots: the month and the day's list). An edit mode puts the widget picker in
// the header, a remove button on each widget and, on the Calendar, a handle on its right edge to make it wide or
// square; drag to reorder. Kept per user.
export const WIDGETS = {
  weather: { name: 'Weather', icon: 'sun' },
  calendar: { name: 'Calendar', icon: 'calendar' },
  timer: { name: 'Focus', icon: 'hourglass' },
  screen: { name: 'Screen Time', icon: 'chart' },
  water: { name: 'Water', icon: 'drop' },
  prayer: { name: 'Prayer', icon: 'moon' },
  todo: { name: 'To-do', icon: 'check' },
  alarm: { name: 'Alarm', icon: 'clock' },
  markets: { name: 'Markets', icon: 'chart' },
  system: { name: 'System', icon: 'gauge' },
  steam: { name: 'Steam', icon: 'steam' },
};
/** Widgets that may be wide (two slots). */
export const WIDE = ['calendar'];
export const COLUMNS = 3;
export const DEFAULT = ['weather', 'calendar', 'timer'];
const KEY = 'sora.home-widgets', SIZES = 'sora.home-sizes';
const slots = (order, sizes) => order.reduce((n, id) => n + (sizes[id] === 'wide' ? 2 : 1), 0);
/** Only known wide sizes are kept; everything else is one slot. */
export function cleanSizes(value) {
  const out = {};
  if (value && typeof value === 'object') for (const id of WIDE) if (value[id] === 'wide') out[id] = 'wide';
  return out;
}

/** A saved order that is still valid (known, unique, fits), else the default. Widgets that no longer exist (Music,
 *  Clock, Tray, Now Playing, Lucky) leave their place to a default one, so an older Home is still full. */
export function cleanOrder(value, sizes = {}) {
  if (!Array.isArray(value)) return [...DEFAULT];
  const order = [...new Set(value.filter(id => id in WIDGETS))];
  if (value.some(id => !(id in WIDGETS))) for (const id of DEFAULT) if (!order.includes(id) && slots(order, sizes) < COLUMNS) order.push(id);
  return order.length && slots(order, sizes) <= COLUMNS ? order : [...DEFAULT];
}
/** Adds or removes a widget; refuses (null) when there is no room for it. */
export function toggle(order, id, sizes = {}) {
  if (order.includes(id)) return order.filter(x => x !== id);
  return slots(order, sizes) + (sizes[id] === 'wide' ? 2 : 1) <= COLUMNS ? [...order, id] : null;
}
/** One slot <-> wide for a widget that allows it; refuses (null) when the wide one would not fit. */
export function resize(order, sizes, id) {
  if (!WIDE.includes(id)) return null;
  const next = { ...sizes };
  if (next[id] === 'wide') delete next[id]; else next[id] = 'wide';
  return slots(order, next) <= COLUMNS ? next : null;
}

export function initHomeWidgets({ icon, onChange = () => {}, toast = null }) {
  const home = document.getElementById('home'), notch = document.getElementById('notch');
  const chips = document.getElementById('widget-chips'), hint = document.getElementById('widget-hint');
  const el = id => home.querySelector(`[data-widget="${id}"]`);
  let order, sizes, saved, editing = false;
  try { sizes = cleanSizes(JSON.parse(localStorage.getItem(SIZES))); } catch { sizes = cleanSizes({}); }
  try { order = cleanOrder(JSON.parse(localStorage.getItem(KEY)), sizes); } catch { order = [...DEFAULT]; }
  saved = { order: [...order], sizes: { ...sizes } };
  // The picker holds more chips than the header is wide: the mouse wheel scrolls it sideways.
  chips.addEventListener('wheel', e => { if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) { chips.scrollLeft += e.deltaY; e.preventDefault(); } }, { passive: false });

  function apply() {
    for (const id of Object.keys(WIDGETS)) { el(id).hidden = !order.includes(id); el(id).dataset.size = sizes[id] || 'square'; }
    // Shown ones first, in order, so the row places them in that order. Only moved when the order changed: moving a
    // node restarts it, which would cut the Calendar's width animation short.
    const now = [...home.children].filter(w => w.dataset.widget && !w.hidden).map(w => w.dataset.widget);
    if (now.join() !== order.join()) {
      for (const id of order) home.append(el(id));
      for (const id of Object.keys(WIDGETS)) if (!order.includes(id)) home.append(el(id));
    }
    drawChips();
    onChange(order);
  }
  function drawChips() {
    if (!editing) return;
    chips.replaceChildren(...Object.entries(WIDGETS).map(([id, w]) => {
      const b = document.createElement('button');
      b.type = 'button'; b.className = 'wp-chip' + (order.includes(id) ? ' on' : ''); b.dataset.id = id;
      b.setAttribute('aria-pressed', String(order.includes(id)));
      b.innerHTML = icon(w.icon) + `<span>${w.name}</span>`;
      b.onclick = () => {
        const next = toggle(order, id, sizes);
        if (!next) return refuse(b);
        if (!next.length) return refuse(b, 'Keep at least one widget.');
        say(''); order = next; apply();
      };
      return b;
    }));
  }
  function refuse(b, why = 'No room. Remove or shrink one first.') { say(why); b.classList.remove('nope'); void b.offsetWidth; b.classList.add('nope'); }
  let sayTimer = 0;
  // Said at the bottom centre, like every other notice, when the page gives a toast; else in the header.
  function say(text) { if (toast) { if (text) toast(text); return; } hint.textContent = text; clearTimeout(sayTimer); if (text) sayTimer = setTimeout(() => hint.textContent = '', 2600); }

  function enter() { if (editing) return; editing = true; saved = { order: [...order], sizes: { ...sizes } }; notch.classList.add('home-editing'); say(''); drawChips(); }
  function leave(keep) {
    if (!editing) return;
    editing = false; notch.classList.remove('home-editing');
    if (keep) { saved = { order: [...order], sizes: { ...sizes } }; try { localStorage.setItem(KEY, JSON.stringify(order)); localStorage.setItem(SIZES, JSON.stringify(sizes)); } catch {} }
    else { order = [...saved.order]; sizes = { ...saved.sizes }; apply(); }
  }

  // Remove buttons, one per widget, shown only while editing.
  for (const id of Object.keys(WIDGETS)) {
    const x = document.createElement('button');
    x.type = 'button'; x.className = 'w-remove'; x.setAttribute('aria-label', 'Remove ' + WIDGETS[id].name);
    x.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17"/></svg>';
    x.onclick = e => { e.stopPropagation(); if (order.length === 1) return refuse(x, 'Keep at least one widget.'); order = order.filter(o => o !== id); apply(); };
    el(id).append(x);
    if (!WIDE.includes(id)) continue;
    const r = document.createElement('button');
    r.type = 'button'; r.className = 'w-resize'; r.setAttribute('aria-label', 'Change size of ' + WIDGETS[id].name);
    r.innerHTML = '<svg viewBox="0 0 24 24" aria-hidden="true"><path class="grow" d="m9 6 6 6-6 6"/><path class="shrink" d="m15 6-6 6 6 6"/></svg>';
    r.onclick = e => { e.stopPropagation(); const next = resize(order, sizes, id); if (!next) return refuse(r); say(''); sizes = next; apply(); };
    el(id).append(r);
  }

  // Drag to reorder: the widget follows the pointer, the others slide aside (FLIP).
  let drag = null;
  home.addEventListener('pointerdown', e => {
    if (!editing || e.button !== 0 || e.target.closest('.w-remove,.w-resize')) return;
    const w = e.target.closest('.widget'); if (!w || w.hidden) return;
    drag = { w, id: w.dataset.widget, x0: e.clientX, left0: w.getBoundingClientRect().left, tx: 0, moved: false };
    w.setPointerCapture(e.pointerId);
  });
  home.addEventListener('pointermove', e => {
    if (!drag) return;
    const dx = e.clientX - drag.x0;
    if (!drag.moved && Math.abs(dx) < 4) return;
    drag.moved = true; drag.w.classList.add('dragging');
    const others = order.filter(id => id !== drag.id).map(id => el(id));
    let at = others.findIndex(o => { const r = o.getBoundingClientRect(); return e.clientX < r.left + r.width / 2; });
    if (at < 0) at = others.length;
    const next = others.map(o => o.dataset.widget); next.splice(at, 0, drag.id);
    if (next.join() !== order.join()) {
      const before = new Map(others.map(o => [o, o.getBoundingClientRect().left]));
      order = next; for (const id of order) home.append(el(id)); for (const id of Object.keys(WIDGETS)) if (!order.includes(id)) home.append(el(id));
      for (const [o, left] of before) {
        const d = left - o.getBoundingClientRect().left; if (!d) continue;
        o.style.transition = 'none'; o.style.transform = `translateX(${d}px)`; void o.offsetWidth;
        o.style.transition = 'transform .22s cubic-bezier(.2,.8,.2,1)'; o.style.transform = '';
      }
    }
    // Keep it under the pointer: where it should be, minus where the layout now puts it.
    const natural = drag.w.getBoundingClientRect().left - drag.tx;
    drag.tx = drag.left0 + dx - natural;
    drag.w.style.transition = 'none'; drag.w.style.transform = `translateX(${drag.tx}px)`;
  });
  const drop = () => {
    if (!drag) return;
    const w = drag.w; drag = null;
    w.classList.remove('dragging'); w.style.transition = 'transform .22s cubic-bezier(.2,.8,.2,1)'; w.style.transform = '';
    onChange(order);
  };
  home.addEventListener('pointerup', drop); home.addEventListener('pointercancel', drop);

  document.getElementById('home-edit').onclick = enter;
  document.getElementById('widget-done').onclick = () => leave(true);
  document.getElementById('widget-cancel').onclick = () => leave(false);
  document.getElementById('widget-done').innerHTML = icon('check');
  document.getElementById('widget-cancel').innerHTML = icon('close');
  apply();
  return { has: id => order.includes(id), leave: () => leave(false), editing: () => editing };
}
