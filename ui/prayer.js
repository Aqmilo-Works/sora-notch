// Prayer times, worked out on this PC from a latitude and a longitude (no network, no account).
// Standard astronomical equations for the sun's position and the twilight angles (the same ones the open PrayTimes method uses;
// written out here, no code copied). Times are rounded up to the next minute, as official timetables do. Results can differ from
// a printed timetable by a minute or two.
const R = Math.PI / 180;
const sin = d => Math.sin(d * R), cos = d => Math.cos(d * R), tan = d => Math.tan(d * R);
const asin = x => Math.asin(x) / R, acos = x => Math.acos(x) / R, atan2 = (y, x) => Math.atan2(y, x) / R, acot = x => Math.atan(1 / x) / R;
const fix = (a, b) => { a -= b * Math.floor(a / b); return a < 0 ? a + b : a; };

/** Fajr and Isha angles below the horizon (Isha may be minutes after Maghrib instead), and a few minutes of caution on Dhuhr. */
export const PRAYER_METHODS = {
  kemenag:   { name: 'Kemenag (Indonesia)', fajr: 20, isha: 18, dhuhrMin: 2 },
  mwl:       { name: 'Muslim World League', fajr: 18, isha: 17 },
  makkah:    { name: 'Umm al-Qura (Makkah)', fajr: 18.5, ishaMin: 90 },
  egypt:     { name: 'Egyptian authority', fajr: 19.5, isha: 17.5 },
  karachi:   { name: 'Karachi', fajr: 18, isha: 18 },
  isna:      { name: 'ISNA (North America)', fajr: 15, isha: 15 },
  singapore: { name: 'MUIS (Singapore)', fajr: 20, isha: 18 },
  malaysia:  { name: 'JAKIM (Malaysia)', fajr: 20, isha: 18 },
  diyanet:   { name: 'Diyanet (Turkey)', fajr: 18, isha: 17 }
};
export const DEFAULT_METHOD = 'kemenag';

function julian(y, m, d) {
  if (m <= 2) { y -= 1; m += 12; }
  const a = Math.floor(y / 100), b = 2 - a + Math.floor(a / 4);
  return Math.floor(365.25 * (y + 4716)) + Math.floor(30.6001 * (m + 1)) + d + b - 1524.5;
}
function sunPosition(jd) {
  const D = jd - 2451545.0, g = fix(357.529 + 0.98560028 * D, 360), q = fix(280.459 + 0.98564736 * D, 360);
  const L = fix(q + 1.915 * sin(g) + 0.020 * sin(2 * g), 360), e = 23.439 - 0.00000036 * D;
  const ra = fix(atan2(cos(e) * sin(L), cos(L)) / 15, 24);
  return { decl: asin(sin(e) * sin(L)), eqt: q / 15 - ra };
}

/**
 * Times for one day as minutes after local midnight.
 * lat/lon in degrees, tz = hours from UTC for that day (the PC's own), asr = 'shafi' or 'hanafi'.
 */
export function prayerTimes({ lat, lon, year, month, day, tz, method = DEFAULT_METHOD, asr = 'shafi' }) {
  const m = PRAYER_METHODS[method] || PRAYER_METHODS[DEFAULT_METHOD];
  const jd = julian(year, month, day) - lon / (15 * 24);
  const midDay = t => fix(12 - sunPosition(jd + t).eqt, 24);
  const angleTime = (angle, t, ccw) => {
    const decl = sunPosition(jd + t).decl, noon = midDay(t);
    const h = 1 / 15 * acos((-sin(angle) - sin(decl) * sin(lat)) / (cos(decl) * cos(lat)));
    return noon + (ccw ? -h : h);
  };
  const asrTime = (factor, t) => { const decl = sunPosition(jd + t).decl; return angleTime(-acot(factor + tan(Math.abs(lat - decl))), t); };
  let t = { fajr: 5, sunrise: 6, dhuhr: 12, asr: 13, sunset: 18, isha: 18 };
  for (let i = 0; i < 2; i++) {
    const p = Object.fromEntries(Object.entries(t).map(([k, v]) => [k, v / 24]));
    t = {
      fajr: angleTime(m.fajr, p.fajr, true), sunrise: angleTime(0.833, p.sunrise, true), dhuhr: midDay(p.dhuhr),
      asr: asrTime(asr === 'hanafi' ? 2 : 1, p.asr), sunset: angleTime(0.833, p.sunset), isha: angleTime(m.isha ?? 18, p.isha)
    };
  }
  const shift = tz - lon / 15;
  const out = { fajr: t.fajr + shift, sunrise: t.sunrise + shift, dhuhr: t.dhuhr + shift + (m.dhuhrMin || 0) / 60, asr: t.asr + shift, maghrib: t.sunset + shift, isha: t.isha + shift };
  if (m.ishaMin) out.isha = out.maghrib + m.ishaMin / 60;
  const minutes = {};
  for (const [k, h] of Object.entries(out)) minutes[k] = Number.isFinite(h) ? Math.ceil(fix(h, 24) * 60 - 1e-6) : null;
  return minutes;
}

export const PRAYERS = ['fajr', 'dhuhr', 'asr', 'maghrib', 'isha'];
export const PRAYER_NAMES = { fajr: 'Fajr', sunrise: 'Sunrise', dhuhr: 'Dhuhr', asr: 'Asr', maghrib: 'Maghrib', isha: 'Isha' };
export const clock = min => min == null ? '--:--' : String(Math.floor(min / 60) % 24).padStart(2, '0') + ':' + String(min % 60).padStart(2, '0');

/** The times of the day containing `now`, using the PC's time zone. */
export function timesFor(date, lat, lon, method, asr) {
  return prayerTimes({ lat, lon, year: date.getFullYear(), month: date.getMonth() + 1, day: date.getDate(), tz: -date.getTimezoneOffset() / 60, method, asr });
}
/** Which prayer comes next and in how many minutes (the next Fajr if all of today's have passed). */
export function nextPrayer(now, lat, lon, method, asr) {
  const today = timesFor(now, lat, lon, method, asr), minute = now.getHours() * 60 + now.getMinutes() + now.getSeconds() / 60;
  for (const p of PRAYERS) if (today[p] != null && today[p] > minute) return { name: p, at: today[p], in: today[p] - minute, tomorrow: false };
  const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1), tomorrow = timesFor(next, lat, lon, method, asr);
  return { name: 'fajr', at: tomorrow.fajr, in: 24 * 60 - minute + (tomorrow.fajr ?? 0), tomorrow: true };
}
export function untilText(minutes) {
  const m = Math.max(0, Math.ceil(minutes));
  return m >= 60 ? Math.floor(m / 60) + ' h ' + String(m % 60).padStart(2, '0') + ' m' : m + ' min';
}

