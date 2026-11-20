// Short codes people actually type for the Markets menu: BBCA for Bank Central Asia, BTC for Bitcoin, GOLD for gold.
// Each code maps to the Yahoo symbol the quotes come from. The data is in market-data.js; this file is the search and the
// naming. Pure functions (tested in tests/markets.test.mjs).
import { MARKETS } from './market-data.js';

/** Other names a code goes by, so "xau", "emas" and "bitcoin" all find what they mean. */
const ALIASES = {
  GOLD: ['XAU', 'EMAS', 'GC'], SILVER: ['XAG', 'PERAK', 'SI'], PLAT: ['PLATINUM', 'XPT'], PALL: ['PALLADIUM', 'XPD'], COPPER: ['HG', 'TEMBAGA'],
  OIL: ['WTI', 'MINYAK', 'CL', 'CRUDE'], BRENT: ['BZ'], GAS: ['NG', 'NATGAS'],
  BTC: ['BITCOIN'], ETH: ['ETHEREUM'], XRP: ['RIPPLE'], DOGE: ['DOGECOIN'], BNB: ['BINANCE'], SOL: ['SOLANA'],
  IHSG: ['JKSE', 'COMPOSITE'], SPX: ['SP500', 'GSPC', 'S&P'], NDX: ['NASDAQ', 'IXIC'], DJI: ['DOW', 'DOWJONES'], N225: ['NIKKEI'], HSI: ['HANGSENG'],
  USDIDR: ['IDR', 'RUPIAH', 'USD'], 'BRK-B': ['BRK', 'BERKSHIRE'], GOOGL: ['GOOGLE'], META: ['FACEBOOK'],
};

/** kind: 'stock' (shares, ETFs and indexes), 'crypto', 'commodity' (gold, silver, oil and the like), 'fx'. */
export const CATALOG = MARKETS.map(([code, symbol, name, kind, mkt]) => ({ code, symbol, name, kind, mkt, alias: ALIASES[code] || [] }));

const BY_SYMBOL = new Map(CATALOG.map(c => [c.symbol, c]));

/** The short code shown for a symbol: the catalog code, else the symbol without .JK, -USD or =X. */
export function codeOf(symbol) {
  return BY_SYMBOL.get(symbol)?.code || String(symbol).replace(/^\^/, '').replace(/\.[A-Z]+$/, '').replace(/-USD$/, '').replace(/=[A-Z]$/, '');
}

/** The readable name for a symbol, from the catalog when it is there. */
export function nameOf(symbol) { return BY_SYMBOL.get(symbol)?.name || null; }

/** A short tag for where a stock trades: IDX, US, or Index. Empty for everything else. */
export function tagOf(entry) {
  if (entry.kind !== 'stock') return '';
  return String(entry.symbol).startsWith('^') ? 'Index' : entry.mkt === 'ID' ? 'IDX' : entry.mkt === 'US' ? 'US' : '';
}

/** What 'All' lists first: the ones people look for most. */
const FIRST = ['GOLD', 'BTC', 'ETH', 'IHSG', 'BBCA', 'BBRI', 'SOL', 'AAPL', 'NVDA', 'USDIDR'];
/** Catalog entries of one kind, in catalog order; 'all' puts the most wanted first. */
export function listKind(kind = 'all') {
  const rows = CATALOG.filter(c => kind === 'all' || c.kind === kind);
  return kind === 'all' ? [...FIRST.map(code => rows.find(c => c.code === code)).filter(Boolean), ...rows.filter(c => !FIRST.includes(c.code))] : rows;
}

/** Local matches for what was typed: the code first (exact, then prefix), then aliases and names. At most `limit`. */
export function findLocal(query, kind = 'all', limit = 14) {
  const q = String(query || '').trim().toUpperCase();
  if (!q) return [];
  const rank = c => {
    const names = [c.code, ...c.alias];
    if (c.code === q || c.symbol.toUpperCase() === q) return 0;
    if (names.includes(q)) return 1;
    if (c.code.startsWith(q) || c.alias.some(a => a.startsWith(q))) return 2;
    if (c.name.toUpperCase().split(/[\s,/]+/).some(w => w.startsWith(q))) return 3;
    if (q.length >= 3 && c.name.toUpperCase().includes(q)) return 4;
    return 9;
  };
  return CATALOG.filter(c => (kind === 'all' || c.kind === kind) && rank(c) < 9).sort((a, b) => rank(a) - rank(b)).slice(0, limit);
}

/** A four-letter code that is in no list is most likely an Indonesian share: BBCA becomes BBCA.JK (a guess, labelled as one). */
export function guessIdx(query) {
  const q = String(query || '').trim().toUpperCase();
  return /^[A-Z]{4}$/.test(q) && !CATALOG.some(c => c.code === q) ? { code: q, symbol: q + '.JK', name: 'Indonesia Stock Exchange (guess)', kind: 'stock', mkt: 'ID', guess: true } : null;
}

/** The kind of a Yahoo search result, from its type label (null when unknown). */
export function kindOfRemote(r) {
  const t = String(r.kind || r.type || '').toLowerCase(), s = String(r.symbol || '');
  if (t.includes('crypto') || /-USD$/.test(s)) return 'crypto';
  if (t.includes('currency') || /=X$/.test(s)) return 'fx';
  if (t.includes('future') || t.includes('commodity') || /=F$/.test(s)) return 'commodity';
  if (t.includes('equity') || t.includes('index') || t.includes('etf')) return 'stock';
  return null;
}
