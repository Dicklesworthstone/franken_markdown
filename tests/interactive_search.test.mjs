// Dependency-free source-editing tests. UI behavior has a separate real-browser
// harness; these assertions execute the production matcher and batch planner.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
const code = readFileSync(new URL('../src/interactive_search.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const api = vm.runInContext('FmdSourceSearch', context);
const plain = value => JSON.parse(JSON.stringify(value));
const fails = (fn, pattern) => assert.throws(fn, error => error.code === 'FMD_SOURCE_SEARCH' && pattern.test(error.message));

test('literal Markdown, regex operators and control characters are never patterns', () => {
  for (const query of ['.*+?^${}()|[]\\', '[link](file.md)', 'a\nb', '\0', '\u2028', '$&', '\\1']) {
    const source = `before ${query} between ${query} after`;
    const found = api.find(source, query);
    assert.equal(found.matches.length, 2, query);
    assert.equal(api.replace(found, '<img onerror=x>$&\\1').source,
      'before <img onerror=x>$&\\1 between <img onerror=x>$&\\1 after');
  }
});

test('search returns a complete non-overlapping UTF-16 match set', () => {
  assert.deepEqual(plain(api.find('😀aaaaa😀aa', 'aa').matches), [
    {start: 2, end: 4}, {start: 4, end: 6}, {start: 9, end: 11},
  ]);
  assert.deepEqual(plain(api.find('😀😀😀', '😀😀').matches), [{start: 0, end: 4}]);
  assert.equal(api.replace(api.find('aaaaa', 'aa'), 'x').source, 'xxa');
});

test('Unicode case-insensitive search retains offsets and exact replacement byte sizes', () => {
  const source = 'İ😀K K k é É';
  const found = api.find(source, 'k', false);
  assert.deepEqual(plain(found.matches), [{start: 3, end: 4}, {start: 5, end: 6}, {start: 7, end: 8}]);
  assert.equal(api.replace(found, 'λ').source, 'İ😀λ λ λ é É');
  assert.equal(api.find(source, 'k').matches.length, 1);
  assert.equal(api.find(source, 'é', false).matches.length, 2);
  assert.equal(api.find('i İ ı I', 'i', false).matches.length, 2);
});

test('canonical equivalence and multi-character case expansions are not invented', () => {
  assert.equal(api.find('é e\u0301', 'é').matches.length, 1);
  assert.equal(api.find('ß SS ss', 'ss', false).matches.length, 2);
});

test('empty replacement deletes, identical replacement is a no-op, empty source has no matches', () => {
  assert.equal(api.replace(api.find('abcabc', 'abc'), '').source, '');
  assert.equal(api.replace(api.find('abcabc', 'abc'), 'abc').changed, false);
  assert.equal(api.find('', 'x').matches.length, 0);
  fails(() => api.replace(api.find('', 'x'), 'z'), /no matches/);
  fails(() => api.find('abc', ''), /Enter text/);
});

test('single replacement targets the chosen original range, never replacement-introduced text', () => {
  const found = api.find('x--x--x', 'x');
  const plan = api.replace(found, 'xxx', 1);
  assert.equal(plan.source, 'x--xxx--x');
  assert.equal(plan.start, 3); assert.equal(plan.end, 4); assert.equal(plan.text, 'xxx');
  assert.equal(plan.count, 1); assert.equal(plan.caret, 6);
  assert.equal(api.replace(found, 'xxx').source, 'xxx--xxx--xxx');
  assert.equal(found.source, 'x--x--x');
});

test('batch plan preserves all text outside the smallest affected interval', () => {
  const found = api.find('prefix needle keep needle suffix', 'needle');
  const plan = api.replace(found, 'NEW');
  assert.equal(plan.start, 7); assert.equal(plan.end, 25);
  assert.equal(plan.text, 'NEW keep NEW');
  assert.equal(plan.source, found.source.slice(0, plan.start) + plan.text + found.source.slice(plan.end));
});

test('search results and match ranges are immutable and only genuine snapshots are editable', () => {
  const found = api.find('abc abc', 'abc');
  assert.ok(Object.isFrozen(found)); assert.ok(Object.isFrozen(found.matches)); assert.ok(Object.isFrozen(found.matches[0]));
  assert.throws(() => { found.matches[0].start = 5; }, TypeError);
  fails(() => api.replace({...found}, 'x'), /Find matches/);
  fails(() => api.replace(null, 'x'), /Find matches/);
  for (const index of [-1, 2, 0.5, NaN, Infinity, '0']) fails(() => api.replace(found, 'x', index), /Select/);
});

test('inputs reject malformed Unicode, non-string values and non-boolean flags', () => {
  for (const bad of ['\ud800', '\udfff', 'a\ud800b', '😀\ud800']) {
    fails(() => api.find(bad, 'x'), /Unicode/);
    fails(() => api.find('valid', bad), /Unicode/);
    fails(() => api.replace(api.find('x', 'x'), bad), /Unicode/);
  }
  for (const value of [null, undefined, 1, {}, ['x']]) {
    fails(() => api.find(value, 'x'), /must be text/);
    fails(() => api.find('x', value), /must be text/);
    fails(() => api.replace(api.find('x', 'x'), value), /must be text/);
  }
  fails(() => api.find('x', 'x', 1), /boolean/);
});

test('query and replacement limits count UTF-8 bytes, not UTF-16 units', () => {
  const exact = '😀'.repeat(1024);
  assert.equal(api.find(exact, exact).matches.length, 1);
  fails(() => api.find(exact + 'x', exact + 'x'), /Query.*limit/);
  const replacement = '😀'.repeat(16384);
  assert.equal(api.replace(api.find('x', 'x'), replacement).source, replacement);
  fails(() => api.replace(api.find('x', 'x'), replacement + 'x'), /Replacement.*limit/);
});

test('the 10,000-match ceiling fails closed rather than offering truncated replace-all', () => {
  const found = api.find('x'.repeat(10000), 'x');
  assert.equal(found.matches.length, 10000);
  assert.equal(api.replace(found, 'z').source, 'z'.repeat(10000));
  fails(() => api.find('x'.repeat(10001), 'x'), /No partial/);
  assert.equal(api.find('ok', 'ok').matches.length, 1);
});

test('source admission happens before allocating a full match set', () => {
  fails(() => api.find('a'.repeat(api.limits.sourceBytes + 1), 'a'), /Source.*limit/);
  fails(() => api.find('😀'.repeat(api.limits.sourceBytes / 4 + 1), 'a'), /Source.*limit/);
});

test('batch expansion is rejected before joining large replacement buffers', () => {
  const found = api.find('x'.repeat(10000), 'x');
  fails(() => api.replace(found, 'y'.repeat(65536)), /32 MiB/);
  assert.equal(found.source, 'x'.repeat(10000));
  assert.equal(api.replace(found, '').source, '');
});

test('exact source byte boundary accounts for case-folded matched byte lengths', () => {
  const source = 'a'.repeat(api.limits.sourceBytes - 3) + 'K';
  const found = api.find(source, 'k', false);
  assert.equal(api.replace(found, 'λ').source.length, source.length);
  assert.equal(api.replace(found, '€').changed, true);
  fails(() => api.replace(found, '😀'), /32 MiB/);
});

test('pure editing retains CRLF, BOM, controls and Unicode outside replaced spans', () => {
  const source = '\ufeffA\r\n😀\0A\rA\n';
  assert.equal(api.replace(api.find(source, 'A'), 'Z').source, '\ufeffZ\r\n😀\0Z\rZ\n');
});

test('deterministic 2,000-case differential against literal split/join', () => {
  let seed = 31991;
  const random = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % n; };
  const alphabet = ['a', 'b', ' ', '\n', '\r', '😀', 'é', 'İ', '$', '[', ']', '\\', '\0'];
  const text = n => Array.from({length: n}, () => alphabet[random(alphabet.length)]).join('');
  for (let i = 0; i < 2000; i++) {
    const source = text(random(90)), query = text(1 + random(4)), replacement = text(random(12));
    const found = api.find(source, query);
    assert.equal(found.matches.length, source.split(query).length - 1);
    if (!found.matches.length) continue;
    assert.equal(api.replace(found, replacement).source, source.split(query).join(replacement));
    const index = random(found.matches.length), match = found.matches[index];
    assert.equal(api.replace(found, replacement, index).source,
      source.slice(0, match.start) + replacement + source.slice(match.end));
  }
});

test('long repeated literal prefixes and regex-like queries have a hang guard', () => {
  context.input = 'a'.repeat(2 * 1024 * 1024);
  context.query = 'a'.repeat(4095) + 'b';
  assert.equal(vm.runInContext('FmdSourceSearch.find(input, query).matches.length', context, {timeout: 4000}), 0);
  context.input = '(a+)+$'.repeat(9000);
  context.query = '(a+)+$';
  assert.equal(vm.runInContext('FmdSourceSearch.find(input, query).matches.length', context, {timeout: 4000}), 9000);
});
