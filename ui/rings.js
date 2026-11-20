// Small gauges for Home's System widget: a row of rounded segments. Plain DOM, no canvas.

/** How many of `count` marks are lit for a share 0..100 (pure; tested). Any share above 0 lights at least one. */
export function litCount(pct, count) {
  if (!Number.isFinite(pct) || pct <= 0) return 0;
  return Math.max(1, Math.min(count, Math.round(pct / 100 * count)));
}

/** A row of `count` rounded segments, lit from the left. */
export function segments(pct, {count = 14, level = 'ok'} = {}) {
  const row = document.createElement('span'), lit = litCount(pct, count);
  row.className = 'segs'; row.dataset.level = level;
  for (let i = 0; i < count; i++) { const seg = document.createElement('i'); if (i < lit) seg.className = 'on'; row.append(seg); }
  return row;
}
