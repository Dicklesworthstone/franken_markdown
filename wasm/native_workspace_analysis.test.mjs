import assert from 'node:assert/strict';
import test from 'node:test';
import {Worker} from 'node:worker_threads';
import {createNativeWorkspaceRenderer, bootNativeWorkspace} from './interactive_runtime.mjs';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';

// Explicit native ABI fixture: exercises the production runtime and real worker
// threads, not Rust parsing, readability heuristics or PDF accessibility quality.
function fixtureBindings() {
  function stats(markdown) {
    if (markdown === 'block') {
      Atomics.store(globalThis.fixtureLatch, 0, 1);
      Atomics.wait(globalThis.fixtureLatch, 1, 0);
    }
    const value = {schema: 'fmd-document-stats-v1', bytes: new TextEncoder().encode(markdown).length,
      lines: 2, words: 3, characters: 9, sentences: 1, syllables: 4,
      reading_time_secs: 1, speaking_time_secs: 2, flesch_reading_ease: 87.5,
      flesch_kincaid_grade: 2.5, reading_ease_label: 'Easy',
      structure: {paragraphs: 1, code_blocks: 2, headings_total: 2, images: 1, links_total: 1},
      outline: [{level: 1, text: '中𝄞', slug: '中𝄞'}, {level: 3, text: '<img onerror=alert(1)>', slug: 'constructor'}],
      findings: [{severity: 'warning', code: 'broken_internal_anchor', message: '#missing'},
        {severity: 'info', code: 'fixture_source', message: markdown}], future_field: {retained: true}};
    if (markdown === 'bad-schema') value.schema = 'different-schema';
    if (markdown === 'wrong-bytes') value.bytes++;
    if (markdown === 'bad-count') value.words = -1;
    if (markdown === 'bad-score') value.flesch_reading_ease = null;
    if (markdown === 'bad-outline') value.outline[0].level = 7;
    if (markdown === 'bad-findings') value.findings[0].message = {};
    if (markdown === 'too-many-headings') value.outline = Array(16385).fill(value.outline[0]);
    if (markdown === 'too-many-findings') value.findings = Array(16385).fill(value.findings[0]);
    if (markdown === 'oversized-ascii') return ' '.repeat(8 * 1024 * 1024 + 1);
    if (markdown === 'oversized-unicode') return '中'.repeat(3 * 1024 * 1024);
    if (markdown === 'bad-unicode') return '\ud800';
    if (markdown === 'non-string') return value;
    if (markdown === 'empty') return '';
    if (markdown === 'invalid-json') return '{';
    return JSON.stringify(value, null, 2);
  }
  function audit(markdown) {
    if (markdown === 'audit-error') throw new Error('Native audit failed');
    return JSON.stringify({schema_version: '1', target: 'pdf', page_count: 1,
      findings: [{severity: 'warning', code: 'missing_alt_text', detail: markdown}], audit_extension: 42});
  }
  function result(mime, data) {
    return {mimeType: mime, bytes: new TextEncoder().encode(data),
      diagnosticsJson: () => '[{"message":"preview-only diagnostic"}]', free() {}};
  }
  return {documentStats: stats, accessibilityAudit: audit,
    renderHtmlConfiguredAdvanced: () => result('text/html', '<html><head></head><body>preview</body></html>'),
    renderPdfConfiguredMulti: () => result('application/pdf', '%PDF-fixture')};
}
const bindingsSource = `const adapter = (${fixtureBindings.toString()})();
export const {documentStats, accessibilityAudit, renderHtmlConfiguredAdvanced, renderPdfConfiguredMulti} = adapter;
export default async function init(input) { await WebAssembly.instantiate(input.module_or_path); }`;
function payload(source = bindingsSource) {
  return {version: 1, wasm: 'AGFzbQEAAAA=', bindings: source,
    options: {font: 'serif', darkMode: 'disabled', fontScale: 1.25, title: 'Document', lang: 'fr',
      toc: true, tocDepth: 3, pageNumbers: true, codeLineNumbers: true},
    images: [{destination: 'chart.png', bytes: 'AQID'}], fonts: []};
}
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
class NodeEndpoint extends EventTarget {
  constructor(source, data, latch) {
    super();
    this.stopped = false;
    this.thread = new Worker(`
      const {parentPort, workerData} = require('node:worker_threads');
      globalThis.fixtureLatch = workerData.latch && new Int32Array(workerData.latch);
      // Node lacks blob-module import: adapt only fixture binding transport,
      // leaving the production serialized worker and runtime source unchanged.
      URL.createObjectURL = () => 'data:text/javascript;base64,' + Buffer.from(workerData.bindings).toString('base64');
      URL.revokeObjectURL = () => {};
      globalThis.self = {postMessage: (data, transfer) => parentPort.postMessage(data, transfer)};
      ${source}
      parentPort.on('message', data => self.onmessage({data}));
    `, {eval: true, workerData: {bindings: data.bindings, latch}});
    this.thread.on('message', data => this.dispatchEvent(new MessageEvent('message', {data})));
    this.thread.on('error', () => this.dispatchEvent(new Event('error')));
    this.thread.on('messageerror', () => this.dispatchEvent(new Event('messageerror')));
  }
  postMessage(data) { this.thread.postMessage(data); }
  terminate() { this.stopped = true; return this.thread.terminate(); }
}
function client(t, data = payload(), config = {}, latch) {
  const endpoints = [];
  const api = createWorkspacePreviewWorker(createNativeWorkspaceRenderer, data, {timeoutMs: 10000,
    workerFactory(source) { const endpoint = new NodeEndpoint(source, data, latch); endpoints.push(endpoint); return endpoint; }, ...config});
  t.after(() => api.dispose());
  return {api, data, endpoints};
}

test('analysis capabilities are optional and native reports preserve exact source independently of rendering', () => {
  const bindings = fixtureBindings(), renderer = createNativeWorkspaceRenderer(bindings, payload());
  assert.deepEqual(renderer.analysisFormats, ['stats', 'accessibility']);
  assert.ok(Object.isFrozen(renderer.analysisFormats));
  renderer.html('# Preview'); const findings = renderer.diagnostics;
  const source = '\ufeff# 中𝄞\r\n\r\n';
  const stats = JSON.parse(renderer.analyze('stats', source));
  assert.equal(stats.bytes, Buffer.byteLength(source));
  assert.equal(stats.findings[1].message, source);
  const audit = JSON.parse(renderer.analyze('accessibility', source));
  assert.equal(audit.findings[0].detail, source);
  assert.equal(renderer.diagnostics, findings);
  assert.deepEqual(renderer.exportFormats, ['html', 'pdf']);
});

test('old bindings keep rendering usable without falsely advertising native analysis', () => {
  const bindings = fixtureBindings(); delete bindings.documentStats; delete bindings.accessibilityAudit;
  const renderer = createNativeWorkspaceRenderer(bindings, payload());
  assert.deepEqual(renderer.analysisFormats, []);
  for (const kind of ['stats', 'accessibility', 'constructor', '__proto__']) {
    assert.throws(() => renderer.analyze(kind, '# Source'), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  }
  assert.match(renderer.html('# Still works'), /preview/);
  assert.equal(new TextDecoder().decode(renderer.pdf('still works')), '%PDF-fixture');
});

test('native admission rejects bad report strings and source Unicode before serialization', () => {
  const renderer = createNativeWorkspaceRenderer(fixtureBindings(), payload());
  assert.throws(() => renderer.analyze('stats', '\ud800'), /unpaired surrogate/);
  for (const source of ['non-string', 'empty', 'oversized-ascii', 'oversized-unicode', 'bad-unicode']) {
    assert.throws(() => renderer.analyze('stats', source), /analysis report/);
  }
});

test('real worker returns exact JSON bytes, validated reports and additive fields; renders remain usable', async t => {
  const {api, data, endpoints} = client(t);
  assert.equal(endpoints.length, 0);
  const source = '\ufeff# 中𝄞\r\n';
  for (const kind of ['stats', 'accessibility']) {
    const result = await api.analyzeDocument(kind, source, data);
    assert.equal(result.kind, kind); assert.equal(result.mimeType, 'application/json');
    assert.equal(result.bytes.byteOffset, 0); assert.equal(result.bytes.buffer.byteLength, result.bytes.byteLength);
    assert.deepEqual(result.report, JSON.parse(new TextDecoder().decode(result.bytes)));
    if (kind === 'stats') {
      assert.equal(result.report.bytes, Buffer.byteLength(source));
      assert.equal(result.report.future_field.retained, true);
      assert.equal(new TextDecoder().decode(result.bytes), fixtureBindings().documentStats(source));
    } else assert.equal(result.report.audit_extension, 42);
    result.bytes.fill(0);
    assert.equal(api.pendingOperations, 0);
  }
  const pdf = await api.exportDocument('pdf', '# Still working', data);
  assert.equal(pdf.format, 'pdf');
  const preview = await api.render('# View', {scale: 1}, data);
  assert.match(preview.html, /preview/);
  assert.equal(preview.diagnostics[0].message, 'preview-only diagnostic');
});

test('invalid analysis requests are rejected before creating a worker', async t => {
  const {api, data, endpoints} = client(t);
  for (const kind of ['constructor', '__proto__', 'toString', 'pdf', '', null, {}]) {
    await assert.rejects(api.analyzeDocument(kind, '# Text', data), {code: 'ANALYSIS_OPTIONS'});
  }
  await assert.rejects(api.analyzeDocument('stats', '\ud800', data), {code: 'ANALYSIS_UNICODE'});
  await assert.rejects(api.analyzeDocument('stats', '# Text', {}), {code: 'ANALYSIS_OPTIONS'});
  await assert.rejects(api.analyzeDocument('stats', 'a'.repeat(32 * 1024 * 1024 + 1), data), {code: 'ANALYSIS_LIMIT'});
  assert.equal(endpoints.length, 0);
});

test('worker rejects schema, cardinality, JSON and type errors with a recoverable explicit failure', async t => {
  const {api, data} = client(t);
  for (const source of ['bad-schema', 'bad-count', 'bad-score', 'bad-outline', 'bad-findings',
    'too-many-headings', 'too-many-findings', 'invalid-json', 'non-string', 'bad-unicode', 'empty',
    'oversized-ascii', 'oversized-unicode']) {
    await assert.rejects(api.analyzeDocument('stats', source, data), {code: 'ANALYSIS_RENDER_FAILED'}, source);
    assert.equal(api.closed, false);
    assert.equal((await api.analyzeDocument('stats', '# Retry', data)).report.schema, 'fmd-document-stats-v1');
  }
  await assert.rejects(api.analyzeDocument('accessibility', 'audit-error', data), {code: 'ANALYSIS_RENDER_FAILED'});
});

test('host rejects a report that describes a different source length', async t => {
  const {api, data} = client(t);
  await assert.rejects(api.analyzeDocument('stats', 'wrong-bytes', data), {code: 'ANALYSIS_PROTOCOL'});
  assert.equal(api.closed, true);
});

test('missing native report bindings preserve an actionable package error and working exports', async t => {
  const data = payload(bindingsSource.replace('documentStats, accessibilityAudit,', 'noStats, noAudit,'));
  const {api} = client(t, data);
  await assert.rejects(api.analyzeDocument('stats', '# Source', data), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  assert.equal((await api.exportDocument('pdf', '# Source', data)).format, 'pdf');
});

test('analysis cannot be superseded by preview, another report or an export, and cancellation terminates it', async t => {
  const buffer = new SharedArrayBuffer(8), latch = new Int32Array(buffer);
  const {api, data, endpoints} = client(t, payload(), {}, buffer);
  const first = assert.rejects(api.analyzeDocument('stats', 'block', data), {code: 'ANALYSIS_CANCELLED'});
  await assert.rejects(api.analyzeDocument('accessibility', '# New', data), {code: 'ANALYSIS_BUSY'});
  await assert.rejects(api.render('# New', {scale: 1}, data), {code: 'ANALYSIS_BUSY'});
  await assert.rejects(api.exportDocument('pdf', '# New', data), {code: 'EXPORT_BUSY'});
  for (let i = 0; i < 400 && !Atomics.load(latch, 0); i++) await sleep(5);
  assert.equal(Atomics.load(latch, 0), 1);
  api.invalidate(); await first;
  assert.equal(api.closed, true); assert.equal(api.pendingOperations, 0); assert.equal(endpoints[0].stopped, true);
});

test('analysis timeout and disposal settle requests and release worker resources', async t => {
  const {api, data} = client(t, payload(), {timeoutMs: 100}, new SharedArrayBuffer(8));
  await assert.rejects(api.analyzeDocument('stats', 'block', data), {code: 'ANALYSIS_TIMEOUT'});
  assert.equal(api.closed, true); assert.equal(api.pendingOperations, 0);
  const second = client(t);
  const stopped = assert.rejects(second.api.analyzeDocument('stats', '# Pending', second.data), {code: 'ANALYSIS_CLOSED'});
  second.api.dispose(); await stopped;
  await assert.rejects(second.api.analyzeDocument('stats', '# Late', second.data), {code: 'ANALYSIS_CLOSED'});
});

function fakeEndpoint(reply) {
  return new class extends EventTarget {
    postMessage(request) {
      queueMicrotask(() => this.dispatchEvent(new MessageEvent('message', {data: request.type === 'init'
        ? {type: 'ready'} : {type: 'result', id: request.id, ...reply(request)}})));
    }
    terminate() { this.stopped = true; }
  }();
}
test('host independently validates identity, MIME, exact buffer ownership, UTF-8 and JSON schema', async t => {
  const data = payload(), json = fixtureBindings().documentStats('# Source');
  const bytes = new TextEncoder().encode(json);
  const borrowed = new Uint8Array(bytes.length + 1); borrowed.set(bytes, 1);
  for (const patch of [{kind: 'accessibility'}, {mimeType: 'text/html'}, {bytes: borrowed.subarray(1)},
    {bytes: new Uint8Array([0xff])}, {bytes: new TextEncoder().encode('{}')},
    {bytes: new Uint8Array(new SharedArrayBuffer(8))}, {bytes: new Uint8Array(8 * 1024 * 1024 + 1)}]) {
    const endpoint = fakeEndpoint(() => ({kind: 'stats', mimeType: 'application/json', bytes, ...patch}));
    const {api} = client(t, data, {workerFactory: () => endpoint});
    await assert.rejects(api.analyzeDocument('stats', '# Source', data), {code: 'ANALYSIS_PROTOCOL'});
    assert.equal(endpoint.stopped, true); assert.equal(api.closed, true);
  }
});

async function boot(t, createPreview, data = payload()) {
  const globals = {document: globalThis.document, window: globalThis.window};
  const create = URL.createObjectURL, revoke = URL.revokeObjectURL;
  const node = {textContent: JSON.stringify(data)};
  globalThis.document = {querySelector: () => node}; globalThis.window = new EventTarget();
  URL.createObjectURL = () => 'data:text/javascript;base64,' + Buffer.from(data.bindings).toString('base64');
  URL.revokeObjectURL = () => {};
  t.after(() => {
    URL.createObjectURL = create; URL.revokeObjectURL = revoke;
    for (const [key, value] of Object.entries(globals)) {
      if (value === undefined) delete globalThis[key]; else globalThis[key] = value;
    }
  });
  bootNativeWorkspace(createNativeWorkspaceRenderer, createPreview);
  const engine = window.__fmdNativeRuntime;
  assert.deepEqual(engine.analysisFormats, []);
  assert.equal(await engine.ready, true);
  return {engine, node};
}

test('boot shares one slot, rejects stale results and never modifies saved JSON or preview diagnostics', async t => {
  const jobs = []; let starts = 0, stops = 0;
  const {engine, node} = await boot(t, () => {
    starts++;
    return {analyzeDocument(kind, markdown) { return new Promise((resolve, reject) => jobs.push({kind, markdown, resolve, reject})); },
      exportDocument() { return new Promise((resolve, reject) => jobs.push({resolve, reject})); },
      dispose() { stops++; }};
  });
  assert.deepEqual(engine.analysisFormats, ['stats', 'accessibility']);
  const before = node.textContent, diagnostics = engine.diagnostics;
  let current = true;
  const first = assert.rejects(engine.analyzeDocument('stats', '# Old', () => current), {code: 'ANALYSIS_CANCELLED'});
  assert.equal(engine.analysisPending, true); assert.equal(engine.exportPending, true);
  assert.throws(() => engine.exportDocument('pdf', '# New'), {code: 'EXPORT_BUSY'});
  assert.throws(() => engine.analyzeDocument('stats', '# New'), {code: 'ANALYSIS_BUSY'});
  assert.throws(() => engine.applySettingsAsync({title: 'new'}, '# New', {}, {scale: 1}), {code: 'SETTINGS_BUSY'});
  await sleep(0); current = false; jobs[0].resolve({kind: 'stats'}); await first;
  assert.equal(engine.analysisPending, false); assert.equal(stops, 1);
  assert.equal(node.textContent, before); assert.equal(engine.diagnostics, diagnostics);
  const success = engine.analyzeDocument('accessibility', '# Current');
  await sleep(0); jobs[1].resolve({kind: 'accessibility', report: {findings: []}});
  assert.equal((await success).kind, 'accessibility'); assert.equal(starts, 2);
  const exporting = engine.exportDocument('pdf', '# Source'); await sleep(0);
  engine.cancelAnalysis(); // must not cancel an unrelated export
  assert.equal(engine.exportPending, true); assert.equal(engine.analysisPending, false);
  jobs[2].resolve({format: 'pdf'}); await exporting;
  assert.equal(node.textContent, before);
});

test('boot cancellation, page lifecycle and missing worker capability fail without synchronous fallback', async t => {
  const jobs = [];
  const {engine} = await boot(t, () => {
    let reject;
    return {analyzeDocument() { return new Promise((yes, no) => {reject = no; jobs.push(yes); }); },
      dispose() { reject?.(new Error('disposed')); }};
  });
  const cancelled = assert.rejects(engine.analyzeDocument('stats', '# Current'), {code: 'ANALYSIS_CANCELLED'});
  await sleep(0); engine.cancelAnalysis(); await cancelled;
  const hidden = assert.rejects(engine.analyzeDocument('accessibility', '# Current'), {code: 'ANALYSIS_CANCELLED'});
  await sleep(0); window.dispatchEvent(new Event('pagehide')); await hidden;
  assert.throws(() => engine.analyzeDocument('stats', '# Hidden'), {code: 'ANALYSIS_SUSPENDED'});
  window.dispatchEvent(new Event('pageshow'));
  const fresh = engine.analyzeDocument('stats', '# Restored'); await sleep(0);
  jobs[2]({kind: 'stats'}); assert.equal((await fresh).kind, 'stats');
});

test('boot rejects missing report bindings before worker creation', async t => {
  const data = payload(bindingsSource.replace('documentStats, accessibilityAudit,', 'noStats, noAudit,'));
  const {engine} = await boot(t, () => assert.fail('must not start a worker'), data);
  assert.deepEqual(engine.analysisFormats, []);
  assert.throws(() => engine.analyzeDocument('stats', '# Source'), {code: 'UNSUPPORTED_WASM_PACKAGE'});
});

test('boot old worker mismatch is explicit and releases the document slot', async t => {
  const {engine} = await boot(t, () => ({dispose() {}}));
  await assert.rejects(engine.analyzeDocument('stats', '# Source'), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  assert.equal(engine.analysisPending, false); assert.equal(engine.exportPending, false);
});
