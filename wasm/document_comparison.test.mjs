import assert from 'node:assert/strict';
import test from 'node:test';
import {Worker, isMainThread, parentPort, workerData} from 'node:worker_threads';
import {createWorkerRenderer, installDocumentWorker, DOCUMENT_SOURCE_LIMIT, COMPARISON_REPORT_LIMIT} from './document_worker.mjs';
import {WORKER_PROTOCOL} from './worker_transport.mjs';

// Explicit renderer double. These tests execute production RPC/validation in
// actual worker threads; they do not claim to test the Rust semantic algorithm.
function renderer(mode = '') {
  let htmlCalls = 0;
  const retained = [];
  const utf8 = value => new TextEncoder().encode(value);
  const report = (old, next, options) => ({schema: 'fmd-diff-v1',
    old_name: options.oldName ?? 'Before', new_name: options.newName ?? 'Current',
    stats: {unchanged_blocks: 1, inserted_blocks: 2, deleted_blocks: 3, modified_blocks: 4,
      words_inserted: 5, words_deleted: 6, similarity_ratio: 0.1},
    fixture: {old, next, htmlCalls}, extension: {retained: true}});
  const output = (format, source, value) => ({format,
    mimeType: format === 'pdf' ? 'application/pdf' : 'text/html; charset=utf-8',
    extension: format === 'pdf' ? 'pdf' : 'html', sourceLength: utf8(source).length,
    bytes: utf8(value), diagnostics: [{severity: 'warning', start: 0, end: 0,
      code: 'fixture', scope: 'document', message: 'Renderer double'}]});
  const api = {
    async semanticDiff(old, next, options) {
      if (retained.some(bytes => !bytes.byteLength)) throw Error('Renderer storage detached');
      if (old === 'block') {
        parentPort.postMessage({fixture: 'blocked'});
        Atomics.wait(new Int32Array(workerData.latch), 0, 0);
      }
      if (mode === 'wait') await new Promise(resolve => setTimeout(resolve, 80));
      const value = report(old, next, options);
      if (mode === 'schema') value.schema = 'wrong';
      if (mode === 'count') value.stats.inserted_blocks = -1;
      if (mode === 'infinite') value.stats.similarity_ratio = Infinity;
      if (mode === 'ratio') value.stats.similarity_ratio = 1.1;
      if (mode === 'oversized') value.padding = 'x'.repeat(COMPARISON_REPORT_LIMIT);
      if (mode === 'throws') throw Error('native failure');
      return value;
    },
    renderSemanticDiff(old, next, options) {
      htmlCalls++;
      const value = output('diff-html', old + next, '<!doctype html><html><head></head><body>'
        + JSON.stringify({old, next, options}).replaceAll('<', '&lt;') + '</body></html>');
      if (mode === 'length') value.sourceLength--;
      if (mode === 'mime') value.mimeType = 'application/javascript';
      if (mode === 'diagnostic') value.diagnostics[0].start = 1;
      if (mode === 'html-fails') throw Error('html failed after JSON');
      const storage = new Uint8Array(value.bytes.length + 12);
      storage.set(value.bytes, 4); retained.push(storage);
      value.bytes = storage.subarray(4, 4 + value.bytes.length);
      return value;
    },
    renderHtml(source) { return output('html', source, '<html><head></head><body>rendered</body></html>'); },
    renderPdf(source) { return output('pdf', source, '%PDF-fixture'); },
  };
  if (mode === 'missing') delete api.semanticDiff;
  return api;
}

class Endpoint extends EventTarget {
  constructor(mode = '') {
    super();
    this.stopped = false;
    this.blocked = new Promise(resolve => { this.onBlocked = resolve; });
    this.worker = new Worker(new URL(import.meta.url), {workerData: {mode, latch: new SharedArrayBuffer(4)}});
    this.worker.on('message', data => {
      if (data.fixture === 'blocked') this.onBlocked();
      else this.dispatchEvent(new MessageEvent('message', {data}));
    });
    this.worker.on('error', () => this.dispatchEvent(new Event('error')));
    this.worker.on('messageerror', () => this.dispatchEvent(new Event('messageerror')));
  }
  postMessage(data, transfer) { this.worker.postMessage(data, transfer); }
  terminate() { this.stopped = true; return this.worker.terminate(); }
}
function client(t, mode = '', options = {}) {
  const endpoints = [];
  const api = createWorkerRenderer({timeoutMs: 5000, workerFactory() {
    const endpoint = new Endpoint(mode); endpoints.push(endpoint); return endpoint;
  }, ...options});
  t.after(() => api.dispose());
  return {api, endpoints};
}
if (!isMainThread) {
  const listeners = new Map();
  installDocumentWorker({
    addEventListener(_kind, listener) { const wrapped = data => listener({data}); listeners.set(listener, wrapped); parentPort.on('message', wrapped); },
    removeEventListener(_kind, listener) { parentPort.off('message', listeners.get(listener)); },
    postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
  }, async () => renderer(workerData.mode));
} else {
  test('comparison is lazy; one worker returns both native reports with exact source bytes', async t => {
    const {api, endpoints} = client(t);
    assert.equal(endpoints.length, 0);
    const old = '\ufeff# 中𝄞\r\nold', next = '# New\r\n';
    const options = {oldName: '  Before\n中  ', newName: 'After'};
    const expected = {...options};
    const promise = api.compare(old, next, options);
    options.oldName = 'mutated';
    const value = await promise;
    assert.equal(value.oldSourceLength, Buffer.byteLength(old));
    assert.equal(value.newSourceLength, Buffer.byteLength(next));
    assert.equal(value.report.old_name, expected.oldName);
    assert.equal(value.report.fixture.old, old); assert.equal(value.report.fixture.next, next);
    assert.deepEqual(value.report.extension, {retained: true});
    assert.equal(value.html.format, 'diff-html');
    assert.equal(value.html.sourceLength, Buffer.byteLength(old + next));
    assert.ok(value.html.text().includes('Before'));
    assert.equal(value.html.diagnostics[0].scope, 'document');
    assert.equal(value.html.filename('review'), 'review.html');
    assert.equal(value.json.filename('review'), 'review.json');
    assert.equal(value.json.blob().type, 'application/json');
    assert.deepEqual(JSON.parse(value.json.text()), value.report);
    for (const bytes of [value.json.bytes, value.html.bytes]) {
      assert.equal(bytes.byteOffset, 0); assert.equal(bytes.byteLength, bytes.buffer.byteLength);
      bytes.fill(0);
    }
    assert.equal((await api.renderPdf('# Still works')).format, 'pdf');
    assert.equal((await api.compare('', '')).report.fixture.htmlCalls, 1);
    assert.equal(api.pendingOperations, 0); assert.equal(api.pendingBytes, 0);
    assert.equal(endpoints.length, 1);
  });

  test('both source limits, malformed Unicode, options and abort are admitted before startup', async t => {
    const {api, endpoints} = client(t);
    for (const [old, next] of [['\ud800', ''], ['', '\udc00'], ['x'.repeat(DOCUMENT_SOURCE_LIMIT + 1), ''],
      ['', '中'.repeat(Math.floor(DOCUMENT_SOURCE_LIMIT / 3) + 1)]]) {
      await assert.rejects(api.compare(old, next));
    }
    for (const options of [null, [], {font: 'serif'}, {oldName: 1}, {newName: '\ud800'},
      {get oldName() { assert.fail('accessor invoked'); }}, Object.create({oldName: 'inherited'}),
      {[Symbol('field')]: 1}]) await assert.rejects(api.compare('', '', options));
    const stop = new AbortController(); stop.abort();
    await assert.rejects(api.compare('', '', {}, {signal: stop.signal}), {code: 'ABORTED'});
    await assert.rejects(api.render('compare', 'not two sources'), {code: 'INVALID_OPTIONS'});
    assert.equal(endpoints.length, 0);
  });

  test('combined ingress includes both retained source strings', async t => {
    const {api, endpoints} = client(t, '', {maxPendingBytes: 1200});
    await assert.rejects(api.compare('a'.repeat(110), 'b'.repeat(110)), {code: 'WORKER_QUEUE_FULL'});
    assert.equal(endpoints.length, 0);
    assert.ok(await api.compare('a', 'b'));
  });

  test('queued comparisons own labels; queued abort does not cancel the active renderer', async t => {
    const {api} = client(t, 'wait');
    const active = api.compare('first', 'first next');
    const stop = new AbortController();
    const cancelled = assert.rejects(api.compare('cancelled', 'next', {}, {signal: stop.signal}), {code: 'ABORTED'});
    const options = {oldName: 'Kept'};
    const kept = api.compare('kept', 'next', options); options.oldName = 'Changed';
    stop.abort(); await cancelled;
    assert.equal((await active).report.fixture.old, 'first');
    assert.equal((await kept).report.old_name, 'Kept');
    assert.equal(api.disposed, false);
    assert.equal(api.pendingBytes, 0);
  });

  test('active abort terminates synchronous work and loses queued requests without replay', async t => {
    const {api, endpoints} = client(t);
    const stop = new AbortController();
    const active = assert.rejects(api.compare('block', 'next', {}, {signal: stop.signal}), {code: 'ABORTED'});
    const queued = assert.rejects(api.renderHtml('queued'), {code: 'SESSION_LOST'});
    await endpoints[0].blocked; stop.abort(); await Promise.all([active, queued]);
    assert.equal(endpoints[0].stopped, true); assert.equal(api.disposed, true);
    assert.equal(api.pendingBytes, 0); assert.equal(api.pendingOperations, 0);
    await assert.rejects(api.compare('', ''), {code: 'SESSION_DISPOSED'});
  });

  test('comparison deadline terminates a blocked native differ', async t => {
    const {api} = client(t, '', {timeoutMs: 200});
    await assert.rejects(api.compare('block', ''), {code: 'TIMEOUT'});
    assert.equal(api.disposed, true); assert.equal(api.pendingOperations, 0);
  });

  test('queued deadline does not kill a different active comparison', async t => {
    const {api} = client(t, 'wait');
    const active = api.compare('first', 'next');
    await assert.rejects(api.compare('second', 'next', {}, {timeoutMs: 10}), {code: 'TIMEOUT'});
    await active; assert.equal(api.disposed, false);
  });

  for (const mode of ['schema', 'count', 'infinite', 'ratio', 'length', 'mime', 'diagnostic', 'throws', 'html-fails', 'missing', 'oversized']) {
    test(`invalid native ${mode} never publishes a partial comparison`, async t => {
      const {api} = client(t, mode);
      const first = assert.rejects(api.compare('# Before', '# After'), mode === 'missing' ? {code: 'UNSUPPORTED_WASM_PACKAGE'} : undefined);
      const queued = assert.rejects(api.renderHtml('queued'), {code: 'SESSION_LOST'});
      await Promise.all([first, queued]);
      assert.equal(api.disposed, true); assert.equal(api.pendingBytes, 0);
    });
  }

  test('combined output budget counts JSON and HTML, including exact boundary', async t => {
    const baseline = client(t);
    const value = await baseline.api.compare('a', 'b');
    const size = value.json.bytes.length + value.html.bytes.length;
    assert.ok(await client(t, '', {maxOutputBytes: size}).api.compare('a', 'b'));
    await assert.rejects(client(t, '', {maxOutputBytes: size - 1}).api.compare('a', 'b'), {code: 'BUDGET_EXCEEDED'});
  });

  test('host independently checks report JSON, identity and owned buffers', async t => {
    const base = await client(t).api.compare('a', 'b');
    const make = () => ({format: 'revision-comparison', oldSourceLength: 1, newSourceLength: 1,
      json: base.json.bytes.slice(), html: {...base.html, bytes: base.html.bytes.slice(), text: undefined, blob: undefined, filename: undefined}});
    for (const corrupt of [
      value => { value.oldSourceLength = 2; value.html.sourceLength = 3; },
      value => { value.json = new Uint8Array([255]); },
      value => { value.json = new TextEncoder().encode('{}'); },
      value => { value.json = new Uint8Array(new SharedArrayBuffer(8)); },
      value => { const storage = new Uint8Array(value.json.length + 1); storage.set(value.json, 1); value.json = storage.subarray(1); },
      value => { value.html.mimeType = 'text/plain'; },
    ]) {
      const endpoint = new class extends EventTarget {
        postMessage(request) {
          const value = make(); corrupt(value);
          queueMicrotask(() => this.dispatchEvent(new MessageEvent('message', {data: {
            protocol: WORKER_PROTOCOL, id: request.id, ok: true, state: null, value,
          }})));
        }
        terminate() { this.stopped = true; }
      }();
      const {api} = client(t, '', {workerFactory: () => endpoint});
      await assert.rejects(api.compare('a', 'b'), {code: 'WORKER_PROTOCOL_ERROR'});
      assert.equal(api.disposed, true); assert.equal(endpoint.stopped, true);
    }
  });

  test('worker factory reentry and disposal cannot start overlapping work', async t => {
    let api, reentrant;
    api = createWorkerRenderer({workerFactory() {
      reentrant = assert.rejects(api.compare('', ''), {code: 'WORKER_STARTING'});
      api.dispose();
      return {terminate() {}, postMessage() {}, addEventListener() {}, removeEventListener() {}};
    }});
    t.after(() => api.dispose());
    await assert.rejects(api.compare('', ''), {code: 'SESSION_DISPOSED'}); await reentrant;
  });
}
