// Production facade, worker transport and report admission. The Rust class is
// an explicit double; these tests do not claim execution of generated WASM.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createBookBindings, bookLinkOptions, parseBookLinkReport } from '../book_session.mjs';
import { createBookWorkerClient, installBookWorker } from '../book_worker.mjs';
const encoder = new TextEncoder();
const files = [{ path: 'a.md', source: '# A\n\n{{#include part.md}}' }, { path: 'b.md', source: '# B' }];
const sources = [{ path: 'part.md', source: '[link](b.md#missing)' }];
const finding = { code: 'missing_anchor', destination: '/b.md#missing', message: 'The target heading is missing.' };
const report = () => ({ schema: 'fmd-book-link-report-v1', scope: 'expanded-html-navigation', chapters: [
  { path: 'a.md', checked: 1, external: 2, unchecked: 3, findings: [{ ...finding }] },
  { path: 'b.md', checked: 0, external: 0, unchecked: 0, findings: [] }
], summary: { findings: 0, checked: -100 } });
const encoded = value => encoder.encode(JSON.stringify(value));
const gate = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
function harness({ check = () => JSON.stringify(report()), load = async () => {}, supports = true } = {}) {
  const state = { created: 0, freed: 0, calls: [], sources: null };
  class EngineBook {
    constructor(paths, source) { state.created++; this.sourceLength = source.join('').length; this.chapterCount = paths.length; }
    static fromSources(paths, source, resources, content) {
      state.sources = { paths, source, resources, content }; return new this(paths, [...source, ...content]);
    }
    setMetadata() {} setCustomCss() {} setTheme() {} setNavigation() {}
    setImage() { throw new Error('image bytes must not reach the checker'); }
    setFont() { throw new Error('font bytes must not reach the checker'); }
    validateLinks() { state.calls.push('check'); return check(); }
    renderPdf() { state.calls.push('pdf'); return new Uint8Array([37]); }
    renderEpub() { throw new Error('must not render EPUB for checks'); }
    renderSite() { throw new Error('must not render HTML for checks'); }
    free() { state.freed++; }
  }
  if (!supports) EngineBook.prototype.validateLinks = undefined;
  const api = createBookBindings(async () => { await load(); return EngineBook; });
  return { api, state };
}
function privateOptions() {
  const value = { includeSources: sources };
  for (const key of ['images', 'fontAssets', 'customCss', 'title', 'fontScale']) {
    Object.defineProperty(value, key, { get() { throw new Error(`must not access ${key}`); } });
  }
  return value;
}

test('one-shot check expands explicitly supplied sources without rendering or loading assets', async () => {
  const { api, state } = harness();
  const result = await api.checkBookLinks(files, privateOptions());
  assert.equal(result.format, 'book-links'); assert.equal(result.extension, 'json');
  assert.equal(result.mimeType, 'application/json'); assert.equal(result.blob().type, 'application/json');
  assert.equal(result.filename(), 'book-links.json');
  assert.equal(state.created, 1); assert.equal(state.freed, 1); assert.deepEqual(state.calls, ['check']);
  assert.deepEqual(state.sources.resources, ['part.md']);
  const checked = parseBookLinkReport(result.bytes, ['a.md', 'b.md']);
  assert.equal(checked.summary.findings, 1); assert.equal(checked.summary.checked, 1);
});

test('retained books check and render without constructing a second book', async () => {
  const { api, state } = harness();
  const book = await api.createBook(files, { includeSources: sources });
  const first = book.validateLinks(); book.renderPdf(); const second = book.validateLinks();
  assert.deepEqual(first.bytes, second.bytes); assert.equal(state.created, 1); assert.equal(state.freed, 0);
  book.dispose(); book.dispose(); assert.equal(state.freed, 1);
  assert.throws(() => book.validateLinks(), /disposed/);
});

test('source snapshots precede asynchronous engine initialization', async () => {
  const hold = gate(), { api, state } = harness({ load: () => hold.promise });
  const chapters = structuredClone(files), resources = structuredClone(sources);
  const pending = api.checkBookLinks(chapters, { includeSources: resources });
  chapters[0].source = 'edited'; resources[0].source = 'changed'; resources.push({ path: 'extra.md', source: 'late' });
  hold.resolve(); await pending;
  assert.equal(state.sources.source[0], files[0].source); assert.equal(state.sources.content[0], sources[0].source);
  assert.deepEqual(state.sources.resources, ['part.md']);
});

test('outdated WASM and Rust errors never become clean reports and always free owned books', async () => {
  const old = harness({ supports: false });
  await assert.rejects(old.api.checkBookLinks(files), /lacks FmdBook.validateLinks/); assert.equal(old.state.freed, 1);
  const broken = harness({ check() { throw 'book_validation: report budget exceeded'; } });
  await assert.rejects(broken.api.checkBookLinks(files), /report budget/); assert.equal(broken.state.freed, 1);
});

test('report bytes and Unicode are admitted before UTF-8 encoding', async () => {
  for (const value of [null, 'x'.repeat(4 * 1024 * 1024 + 1), 'bad\ud800']) {
    const { api, state } = harness({ check: () => value });
    await assert.rejects(api.checkBookLinks(files)); assert.equal(state.freed, 1);
  }
});

test('link-only option selection preserves explicit expansion policy and rejects invalid sources', async () => {
  assert.deepEqual(bookLinkOptions(privateOptions()), { includeSources: sources, expandIncludes: undefined });
  const { api, state } = harness();
  await assert.rejects(api.checkBookLinks(files, { includeSources: sources, expandIncludes: false }), /requires expandIncludes/);
  await assert.rejects(api.checkBookLinks([], {})); await assert.rejects(api.checkBookLinks(files, null));
  assert.equal(state.created, 0);
});

function endpoint(engine, state) {
  let receive; const listeners = new Map(); let stopped = false;
  const worker = {
    addEventListener(kind, fn) { listeners.set(kind, fn); },
    removeEventListener(kind) { listeners.delete(kind); },
    terminate() { stopped = true; state.terminated++; },
    postMessage(data, transfer) {
      const captured = structuredClone(data, { transfer }); state.requests.push(captured);
      queueMicrotask(() => { if (!stopped) void receive({ data: captured }); });
    }
  };
  installBookWorker({ addEventListener(kind, fn) { receive = fn; }, postMessage(data, transfer) {
    const captured = structuredClone(data, { transfer });
    queueMicrotask(() => { if (!stopped) listeners.get('message')?.({ data: captured }); });
  } }, engine);
  return worker;
}
function client(engine, options = {}) {
  const state = { terminated: 0, requests: [] };
  return { state, api: createBookWorkerClient({ workerFactory: () => endpoint(engine, state), ...options }) };
}

test('real worker protocol transports only chapter/include sources and the bounded report', async () => {
  const h = harness(), c = client(h.api);
  try {
    const result = await c.api.render(files, 'links', privateOptions());
    assert.equal(result.format, 'book-links'); assert.equal(result.extension, 'json');
    assert.equal(c.state.terminated, 1); assert.equal(h.state.freed, 1);
    const request = c.state.requests[0];
    assert.deepEqual(request.options.includeSources, sources);
    assert.deepEqual(request.options.images, []); assert.deepEqual(request.options.fontAssets, []);
    assert.equal(request.options.customCss, undefined);
    assert.equal(parseBookLinkReport(result.bytes, ['a.md', 'b.md']).summary.findings, 1);
  } finally { c.api.dispose(); }
});

test('worker-side admission independently strips assets from direct link-check messages', async () => {
  let receive, reply, called = 0;
  installBookWorker({ addEventListener(_, fn) { receive = fn; }, postMessage(data) { reply = data; } }, {
    async checkBookLinks(chapters, options) {
      called++; assert.deepEqual(options.images, []); assert.deepEqual(options.fontAssets, []);
      assert.deepEqual(options.includeSources, sources); return { bytes: encoded(report()), sourceLength: 10 };
    }
  });
  await receive({ data: { schemaVersion: 1, id: 1, format: 'links', files, options: privateOptions(), maxOutputBytes: 4096 } });
  assert.equal(called, 1); assert.equal(reply.error, undefined);
});

test('cancellation and AbortSignal terminate checks and allow a later retry', async () => {
  const hold = gate(); let calls = 0;
  const c = client({ async checkBookLinks() {
    if (++calls === 1) await hold.promise; return { bytes: encoded(report()), sourceLength: 10 };
  } });
  try {
    const abort = new AbortController(), pending = c.api.render(files, 'links', { includeSources: sources }, { signal: abort.signal });
    const rejected = assert.rejects(pending, { code: 'EXPORT_CANCELLED' });
    await new Promise(resolve => setImmediate(resolve)); abort.abort(); await rejected;
    assert.equal(c.api.busy, false); hold.resolve();
    const result = await c.api.render(files, 'links'); assert.equal(result.format, 'book-links');
    assert.equal(c.state.terminated, 2);
  } finally { hold.resolve(); c.api.dispose(); }
});

test('worker deadlines and output limits fail explicitly, not with partial report bytes', async () => {
  const stalled = client({ checkBookLinks: () => new Promise(() => {}) }, { timeoutMs: 10 });
  await assert.rejects(stalled.api.render(files, 'links'), { code: 'EXPORT_TIMEOUT' });
  assert.equal(stalled.state.terminated, 1); stalled.api.dispose();
  const huge = client({ checkBookLinks: async () => ({ bytes: new Uint8Array(100), sourceLength: 1 }) }, { maxOutputBytes: 50 });
  await assert.rejects(huge.api.render(files, 'links'), { code: 'OUTPUT_LIMIT' }); huge.api.dispose();
});

test('report admission recomputes summaries and freezes only validated fields', () => {
  const input = report(); input.chapters[0].unknown = { html: '<script>bad</script>' };
  const result = parseBookLinkReport(encoded(input), ['a.md', 'b.md']);
  assert.deepEqual(result.summary, { chapters: 2, checked: 1, external: 2, unchecked: 3, findings: 1 });
  assert.equal(result.chapters[0].unknown, undefined);
  assert(Object.isFrozen(result)); assert(Object.isFrozen(result.chapters)); assert(Object.isFrozen(result.chapters[0].findings[0]));
});

test('wrong chapter identities, schema drift, fabricated counts and malformed Unicode fail shut', () => {
  for (const mutate of [
    value => { value.schema = 'future'; }, value => { value.scope = 'unexpanded-source-chapters'; },
    value => { value.chapters.reverse(); }, value => { value.chapters[0].path = 'another.md'; },
    value => { value.chapters[0].checked = 0; }, value => { value.chapters[0].external = -1; },
    value => { value.chapters[0].unchecked = 1.5; }, value => { value.chapters[1].checked = 250000; },
    value => { value.chapters[0].findings[0].code = 'future'; }, value => { value.chapters[0].findings[0].message = 'bad\ud800'; },
    value => { value.chapters[0].findings[0].destination = 'x'.repeat(8193); }
  ]) {
    const input = report(); mutate(input);
    assert.throws(() => parseBookLinkReport(encoded(input), ['a.md', 'b.md']), { code: 'INVALID_LINK_REPORT' });
  }
  for (const bytes of [new Uint8Array(), new Uint8Array([255]), new Uint8Array(new SharedArrayBuffer(10)), new Uint8Array(4 * 1024 * 1024 + 1)]) {
    assert.throws(() => parseBookLinkReport(bytes, ['a.md', 'b.md']), { code: 'INVALID_LINK_REPORT' });
  }
});

test('report decoding respects view offsets and preserves inert hostile-looking text', () => {
  const value = report(); value.chapters[0].findings[0].destination = '</script><img onerror="bad">';
  const bytes = encoded(value), backing = new Uint8Array(bytes.length + 20); backing.set(bytes, 10);
  const result = parseBookLinkReport(backing.subarray(10, 10 + bytes.length), ['a.md', 'b.md']);
  assert.equal(result.chapters[0].findings[0].destination, value.chapters[0].findings[0].destination);
});
