// The Markets menu's codes: BBCA, BTC, GOLD and friends map to the symbols the quotes come from.
import test from 'node:test';
import assert from 'node:assert/strict';
import {CATALOG,codeOf,nameOf,tagOf,listKind,findLocal,guessIdx,kindOfRemote} from '../ui/market-codes.js';

test('a code finds its market, exact match first', () => {
  assert.equal(findLocal('bbca')[0].symbol, 'BBCA.JK');
  assert.equal(findLocal('BTC')[0].symbol, 'BTC-USD');
  assert.equal(findLocal('eth')[0].symbol, 'ETH-USD');
  assert.equal(findLocal('gold')[0].symbol, 'GC=F');
  assert.equal(findLocal('xau')[0].symbol, 'GC=F', 'XAU is another name for gold');
  assert.equal(findLocal('emas')[0].symbol, 'GC=F');
  assert.equal(findLocal('aapl')[0].symbol, 'AAPL', 'US shares too');
  assert.equal(findLocal('tsla')[0].symbol, 'TSLA');
  assert.equal(findLocal('nikkei')[0].symbol, '^N225');
  assert.equal(findLocal('wti')[0].symbol, 'CL=F');
});
test('a prefix and a name word also match, and the kind filter holds', () => {
  assert.ok(findLocal('bb').some(c => c.code === 'BBRI'));
  assert.ok(findLocal('bank').every(c => c.kind === 'stock'));
  assert.ok(findLocal('b', 'crypto').every(c => c.kind === 'crypto'));
  assert.ok(findLocal('shib', 'crypto').some(c => c.code === 'SHIB'));
  assert.deepEqual(findLocal(''), []);
});
test('the lists are broad: Indonesian and US shares, many coins, metals and other commodities', () => {
  const stocks = listKind('stock'), crypto = listKind('crypto'), com = listKind('commodity');
  assert.ok(stocks.filter(c => c.mkt === 'ID').length >= 60, 'Indonesian shares');
  assert.ok(stocks.filter(c => c.mkt === 'US').length >= 60, 'US shares');
  assert.ok(crypto.length >= 80, 'coins: ' + crypto.length);
  assert.ok(com.length >= 10);
  for (const code of ['BBCA', 'BBRI', 'TLKM', 'GOTO', 'ANTM', 'AAPL', 'NVDA', 'TSLA', 'SPY', 'IHSG', 'SPX']) assert.ok(stocks.some(c => c.code === code), code);
  for (const code of ['BTC', 'ETH', 'SOL', 'XRP', 'DOGE', 'ADA', 'LINK', 'TON', 'PEPE', 'SUI', 'UNI', 'AAVE', 'NEAR', 'TRX']) assert.ok(crypto.some(c => c.code === code), code);
  for (const code of ['GOLD', 'SILVER', 'PLAT', 'COPPER', 'OIL', 'BRENT']) assert.ok(com.some(c => c.code === code), code);
});
test('codes and symbols are unique, every entry is complete, and All puts the most wanted first', () => {
  assert.equal(new Set(CATALOG.map(c => c.code)).size, CATALOG.length);
  assert.equal(new Set(CATALOG.map(c => c.symbol)).size, CATALOG.length);
  for (const c of CATALOG) { assert.ok(c.code && c.symbol && c.name, c.code); assert.ok(['stock', 'crypto', 'commodity', 'fx'].includes(c.kind), c.code + ' ' + c.kind); assert.match(c.symbol, /^[A-Za-z0-9^=.-]{1,16}$/, c.symbol); }
  assert.deepEqual(listKind('all').slice(0, 3).map(c => c.code), ['GOLD', 'BTC', 'ETH']);
  assert.equal(listKind('all').length, CATALOG.length);
});
test('an unknown four-letter code is guessed as an Indonesian share, and says so', () => {
  assert.equal(guessIdx('ZZZZ').symbol, 'ZZZZ.JK');
  assert.equal(guessIdx('ZZZZ').guess, true);
  assert.equal(guessIdx('BBCA'), null, 'known codes are not guessed');
  assert.equal(guessIdx('AB'), null);
  assert.equal(guessIdx('BTC1'), null);
});
test('short codes, names and tags for symbols, with a fallback for symbols that are in no list', () => {
  assert.deepEqual(['BBCA.JK', 'BTC-USD', 'GC=F', '^JKSE', '^GSPC'].map(codeOf), ['BBCA', 'BTC', 'GOLD', 'IHSG', 'SPX']);
  assert.equal(codeOf('XYZZ.JK'), 'XYZZ');
  assert.equal(codeOf('QQQQ-USD'), 'QQQQ');
  assert.equal(codeOf('IDR=X'), 'USDIDR');
  assert.equal(nameOf('BBCA.JK'), 'Bank Central Asia');
  assert.equal(nameOf('XYZZ.JK'), null);
  assert.deepEqual([findLocal('bbca')[0], findLocal('aapl')[0], findLocal('ihsg')[0], findLocal('btc')[0]].map(tagOf), ['IDX', 'US', 'Index', '']);
});
test('Yahoo results are sorted into the same kinds', () => {
  assert.equal(kindOfRemote({symbol: 'XYZ-USD', kind: 'Cryptocurrency'}), 'crypto');
  assert.equal(kindOfRemote({symbol: 'XYZZ.JK', kind: 'Equity'}), 'stock');
  assert.equal(kindOfRemote({symbol: 'GC=F', kind: 'Futures'}), 'commodity');
  assert.equal(kindOfRemote({symbol: 'CL=F', kind: 'Futures'}), 'commodity');
  assert.equal(kindOfRemote({symbol: 'EURGBP=X', kind: 'Currency'}), 'fx');
});
