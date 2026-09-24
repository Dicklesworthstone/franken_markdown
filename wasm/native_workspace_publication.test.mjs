import assert from 'node:assert/strict';
import test from 'node:test';
import { Worker } from 'node:worker_threads';
import { createNativeWorkspaceRenderer, bootNativeWorkspace } from './interactive_runtime.mjs';
import { createWorkspacePreviewWorker } from './interactive_preview.mjs';

// Explicit native ABI double: these bytes are envelope/ownership fixtures, not
// PDF/EPUB conformance or Rust typesetting evidence. Worker tests execute the
// production serialized runtime in real Node threads and instantiate empty WASM.
function fixtureBindings() {
  const outputs = {html: ['text/html', '<html><head></head><body>fixture</body></html>'],
    pdf: ['application/pdf', '%PDF-fixture'], epub: ['application/epub+zip', 'PK\x03\x04fixture'],
    svg: ['image/svg+xml', '<?xml version="1.0"?>\n<svg xmlns="http://www.w3.org/2000/svg"/>']};
  const calls = [], storage = [];
  let freed = 0;
  function render(format, args) {
    if (storage.some(bytes => !bytes.length)) throw new Error('Native storage was detached');
    if (args[0] === 'block') {
      Atomics.store(globalThis.fixtureLatch, 0, 1);
      Atomics.wait(globalThis.fixtureLatch, 1, 0);
    }
    const [mime, text] = outputs[format];
    calls.push({format, args});
    const bytes = new TextEncoder().encode(args[0] === 'bad-signature' ? 'broken' : text);
    storage.push(bytes);
    return {bytes, mimeType: args[0] === 'bad-mime' ? 'application/javascript' : mime,
      diagnosticsJson: () => JSON.stringify([{severity: 'warning', start: 0, end: 0,
        code: 'fixture_publication', scope: 'document',
        message: JSON.stringify({format, args}, (_key, value) => ArrayBuffer.isView(value) ? [...value] : value)}]),
      free() { freed++; }};
  }
  return {calls, get freed() { return freed; },
    renderHtmlConfiguredAdvanced: (...args) => render('html', args),
    renderPdfConfiguredMulti: (...args) => render('pdf', args),
    renderPdfConfiguredPage: (...args) => render('pdf', args),
    renderEpubConfiguredAdvanced: (...args) => render('epub', args),
    renderSvgConfiguredResources: (...args) => render('svg', args)};
}
const bindingSource = `const adapter = (${fixtureBindings.toString()})();
export const {renderHtmlConfiguredAdvanced, renderPdfConfiguredMulti,
renderPdfConfiguredPage, renderEpubConfiguredAdvanced, renderSvgConfiguredResources} = adapter;
export default async function init(input) { await WebAssembly.instantiate(input.module_or_path); }`;
const state = () => ({version: 1, wasm: 'AGFzbQEAAAA=', bindings: bindingSource,
  options: {font: 'serif', darkMode: 'disabled', fontScale: 1.25, title: 'Document',
    author: 'PDF author', lang: 'fr', toc: true, tocDepth: 3, pageNumbers: true,
    codeLineNumbers: true, metadataEpochSeconds: 0},
  images: [{destination: 'chart.png', bytes: 'AQID'}],
  fonts: [{slot: 'body-regular', bytes: 'BAU=', weight: 650}, {slot: 'mono-regular', bytes: 'Bg=='}]});
const resultArgs = value => JSON.parse(value.diagnostics[0].message).args;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

class NodeEndpoint extends EventTarget {
  constructor(source, payload, latch) {
    super();
    this.thread = new Worker(`
      const {parentPort, workerData} = require('node:worker_threads');
      globalThis.fixtureLatch = workerData.latch && new Int32Array(workerData.latch);
      // Node does not import blob: modules. Only the explicit fixture binding
      // module is mapped to a data URL; the production worker source is intact.
      URL.createObjectURL = () => 'data:text/javascript;base64,' + Buffer.from(workerData.bindings).toString('base64');
      URL.revokeObjectURL = () => {};
      globalThis.self = {postMessage: (message, transfer) => parentPort.postMessage(message, transfer)};
      ${source}
      parentPort.on('message', data => self.onmessage({data}));
    `, {eval: true, workerData: {bindings: payload.bindings, latch}});
    this.thread.on('message', data => this.dispatchEvent(new MessageEvent('message', {data})));
    this.thread.on('error', () => this.dispatchEvent(new Event('error')));
    this.thread.on('messageerror', () => this.dispatchEvent(new Event('messageerror')));
    this.stopped = false;
  }
  postMessage(data, transfer) { this.thread.postMessage(data, transfer); }
  terminate() { this.stopped = true; return this.thread.terminate(); }
}
function client(t, payload = state(), configuration = {}, latch) {
  const endpoints = [];
  const api = createWorkspacePreviewWorker(createNativeWorkspaceRenderer, payload, {
    timeoutMs: 5000, workerFactory(source) {
      const endpoint = new NodeEndpoint(source, payload, latch); endpoints.push(endpoint); return endpoint;
    }, ...configuration,
  });
  t.after(() => api.dispose());
  return {api, payload, endpoints};
}

test('EPUB and SVG use the complete native ABI and committed document settings', () => {
  const bindings = fixtureBindings(), payload = state();
  const renderer = createNativeWorkspaceRenderer(bindings, payload);
  assert.deepEqual(renderer.exportFormats, ['html', 'pdf', 'epub', 'svg']);
  assert.equal(Object.isFrozen(renderer.exportFormats), true);
  renderer.html('# View', {scale: 2, theme: 'dark'});
  for (const format of ['epub', 'svg']) {
    const output = renderer[format]('\ufeff# Source\r\n中𝄞');
    assert.ok(output instanceof Uint8Array);
    const {args} = bindings.calls.at(-1);
    assert.equal(args[0], '\ufeff# Source\r\n中𝄞');
    assert.equal(args[1], 'serif'); assert.equal(args[2], 'disabled');
    const start = format === 'epub' ? 9 : 5;
    assert.deepEqual(args[start], ['chart.png']);
    assert.deepEqual([...args[start + 1]], [1, 2, 3]);
    assert.deepEqual([...args[start + 2]], [3]);
    assert.deepEqual(args.slice(start + 3, start + 8).map(value => [...value]), [[4, 5], [], [], [], [6]]);
    assert.deepEqual([...args[start + 8]], [650, 0, 0, 0, 0]);
    if (format === 'epub') assert.deepEqual(args.slice(3, 9), ['Document', 'fr', 1.25, undefined, true, 3]);
    else assert.deepEqual(args.slice(3, 5), [1.25, undefined]);
    assert.equal(renderer.diagnostics[0].code, 'fixture_publication');
    assert.equal(renderer.diagnostics[0].scope, 'document');
  }
  assert.equal(bindings.freed, 3);
});

test('resource and settings transactions survive EPUB/SVG publication and saved-payload reopening', () => {
  const payload = state(), bindings = fixtureBindings();
  const renderer = createNativeWorkspaceRenderer(bindings, payload);
  const addition = new Uint8Array([99, 7, 8, 88]);
  const image = renderer.stageImages([{destination: 'new.png', bytes: addition.subarray(1, 3)}]);
  addition.fill(9); image.commit();
  const settings = renderer.stageSettings({title: 'Revised', fontScale: 1.5}); settings.commit();
  renderer.epub('# New');
  assert.deepEqual(bindings.calls.at(-1).args[9], ['chart.png', 'new.png']);
  assert.deepEqual([...bindings.calls.at(-1).args[10]], [1, 2, 3, 7, 8]);
  assert.equal(bindings.calls.at(-1).args[3], 'Revised');
  const reopened = createNativeWorkspaceRenderer(bindings, JSON.parse(JSON.stringify(payload)));
  reopened.svg('# Reopened');
  assert.deepEqual(bindings.calls.at(-1).args[5], ['chart.png', 'new.png']);
  assert.equal(bindings.calls.at(-1).args[3], 1.5);
  settings.rollback(); renderer.epub('# Previous settings');
  assert.equal(bindings.calls.at(-1).args[3], 'Document');
});

test('old bindings retain HTML/PDF but never substitute resource-losing legacy exports', () => {
  const bindings = fixtureBindings();
  delete bindings.renderEpubConfiguredAdvanced; delete bindings.renderSvgConfiguredResources;
  bindings.renderEpubConfigured = bindings.renderSvgConfigured = () => assert.fail('legacy fallback called');
  const renderer = createNativeWorkspaceRenderer(bindings, state());
  assert.deepEqual(renderer.exportFormats, ['html', 'pdf']);
  for (const format of ['epub', 'svg']) assert.throws(() => renderer[format]('# Source'), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  assert.match(renderer.html('# Still usable'), /<html>/);
  assert.equal(new TextDecoder().decode(renderer.pdf('# Still usable')), '%PDF-fixture');
});

test('native publication rejects malformed Unicode and always releases invalid result handles', () => {
  const bindings = fixtureBindings(), renderer = createNativeWorkspaceRenderer(bindings, state());
  for (const format of ['epub', 'svg']) {
    assert.throws(() => renderer[format]('\ud800'), /unpaired surrogate/);
    assert.throws(() => renderer[format]('bad-mime'), /output envelope/);
  }
  assert.equal(bindings.calls.length, 2); assert.equal(bindings.freed, 2);
});

test('serialized production worker publishes all four formats and owns exact byte buffers', async t => {
  const {api, payload, endpoints} = client(t);
  assert.equal(endpoints.length, 0);
  for (const [format, mime] of [['html', 'text/html;charset=utf-8'], ['pdf', 'application/pdf'],
    ['epub', 'application/epub+zip'], ['svg', 'image/svg+xml']]) {
    const result = await api.exportDocument(format, '\ufeff# 中𝄞\r\n', payload);
    assert.equal(result.format, format); assert.equal(result.mimeType, mime);
    assert.equal(result.bytes.byteOffset, 0); assert.equal(result.bytes.byteLength, result.bytes.buffer.byteLength);
    assert.equal(resultArgs(result)[0], '\ufeff# 中𝄞\r\n');
    assert.equal(result.diagnostics[0].scope, 'document');
    assert.equal(result.diagnostics[0].code, 'fixture_publication');
    // Mutating the caller's output cannot detach or corrupt the renderer cache.
    result.bytes.fill(0);
    assert.equal(api.pendingOperations, 0);
  }
  assert.equal(endpoints.length, 1);
  const epub = await api.exportDocument('epub', '# Latest', payload);
  assert.deepEqual(resultArgs(epub).slice(3, 9), ['Document', 'fr', 1.25, null, true, 3]);
  assert.deepEqual(resultArgs(epub)[10], [1, 2, 3]);
  assert.deepEqual(resultArgs(epub)[12], [4, 5]);
});

test('publication admission refuses invalid formats and Unicode before worker creation', async t => {
  let starts = 0;
  const {api, payload} = client(t, state(), {workerFactory() { starts++; assert.fail('unexpected worker'); }});
  for (const format of ['constructor', '__proto__', 'toString', 'zip', null]) {
    await assert.rejects(api.exportDocument(format, '# Text', payload), {code: 'EXPORT_OPTIONS'});
  }
  await assert.rejects(api.exportDocument('epub', '\ud800', payload), {code: 'EXPORT_UNICODE'});
  assert.equal(starts, 0);
});

test('worker reports unsupported bindings without losing working formats', async t => {
  const payload = state();
  payload.bindings = bindingSource.replace('renderEpubConfiguredAdvanced, renderSvgConfiguredResources', 'unusedEpub, unusedSvg');
  const {api} = client(t, payload);
  for (const format of ['epub', 'svg']) {
    await assert.rejects(api.exportDocument(format, '# Source', payload), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  }
  assert.equal((await api.exportDocument('pdf', '# Still working', payload)).format, 'pdf');
});

test('worker refuses malformed native EPUB/SVG output rather than publishing mislabeled files', async t => {
  const {api, payload} = client(t);
  for (const format of ['epub', 'svg']) {
    await assert.rejects(api.exportDocument(format, 'bad-signature', payload), {code: 'EXPORT_RENDER_FAILED'});
    await assert.rejects(api.exportDocument(format, 'bad-mime', payload), {code: 'EXPORT_RENDER_FAILED'});
    assert.equal((await api.exportDocument(format, '# Valid retry', payload)).format, format);
  }
});

test('cancellation terminates an in-flight EPUB worker and refuses overlapping exports', async t => {
  const latchBuffer = new SharedArrayBuffer(8), latch = new Int32Array(latchBuffer);
  const {api, payload, endpoints} = client(t, state(), {}, latchBuffer);
  const first = assert.rejects(api.exportDocument('epub', 'block', payload), {code: 'EXPORT_CANCELLED'});
  await assert.rejects(api.exportDocument('svg', '# Overlap', payload), {code: 'EXPORT_BUSY'});
  for (let i = 0; i < 400 && !Atomics.load(latch, 0); i++) await delay(5);
  assert.equal(Atomics.load(latch, 0), 1);
  api.invalidate(); await first;
  assert.equal(api.closed, true); assert.equal(api.pendingOperations, 0); assert.equal(endpoints[0].stopped, true);
});

test('SVG export deadline includes a blocked native render and closes its worker', async t => {
  const {api, payload} = client(t, state(), {timeoutMs: 150}, new SharedArrayBuffer(8));
  await assert.rejects(api.exportDocument('svg', 'block', payload), {code: 'EXPORT_TIMEOUT'});
  assert.equal(api.closed, true); assert.equal(api.pendingOperations, 0);
});

function fakeEndpoint(reply) {
  return new class extends EventTarget {
    postMessage(request) {
      queueMicrotask(() => this.dispatchEvent(new MessageEvent('message', {data:
        request.type === 'init' ? {type: 'ready'} : {type: 'result', id: request.id, ...reply(request)}})));
    }
    terminate() { this.stopped = true; }
  }();
}

test('host rejects corrupt signatures, MIME and borrowed response slices at publication boundary', async t => {
  const payload = state();
  for (const patch of [
    {mimeType: 'application/javascript'}, {format: 'pdf'},
    {bytes: new TextEncoder().encode('not an epub')}, {bytes: new Uint8Array([0, 80, 75, 3, 4]).subarray(1)},
  ]) {
    const endpoint = fakeEndpoint(() => ({format: 'epub', mimeType: 'application/epub+zip',
      bytes: new Uint8Array([80, 75, 3, 4]), diagnostics: [], ...patch}));
    const {api} = client(t, payload, {workerFactory: () => endpoint});
    await assert.rejects(api.exportDocument('epub', '# Source', payload), {code: 'EXPORT_PROTOCOL'});
    assert.equal(api.closed, true); assert.equal(endpoint.stopped, true);
  }
});

test('boot engine gates capabilities and cancels stale EPUB/SVG results before publication', async t => {
  const payload = state(), globals = {document: globalThis.document, window: globalThis.window};
  const createUrl = URL.createObjectURL, revokeUrl = URL.revokeObjectURL;
  globalThis.document = {querySelector: () => ({textContent: JSON.stringify(payload)})};
  globalThis.window = new EventTarget();
  URL.createObjectURL = () => 'data:text/javascript;base64,' + Buffer.from(bindingSource).toString('base64');
  URL.revokeObjectURL = () => {};
  t.after(() => {
    URL.createObjectURL = createUrl; URL.revokeObjectURL = revokeUrl;
    for (const [key, value] of Object.entries(globals)) {
      if (value === undefined) delete globalThis[key]; else globalThis[key] = value;
    }
  });
  let complete, reject, stops = 0, starts = 0;
  bootNativeWorkspace(createNativeWorkspaceRenderer, (_factory, captured) => {
    starts++;
    assert.equal(captured.options.fontScale, 1.25);
    return {exportDocument: () => new Promise((yes, no) => {complete = yes; reject = no;}),
      dispose() { stops++; reject?.(new Error('disposed')); }};
  });
  const engine = window.__fmdNativeRuntime;
  assert.deepEqual(engine.exportFormats, []);
  assert.equal(await engine.ready, true);
  assert.deepEqual(engine.exportFormats, ['html', 'pdf', 'epub', 'svg']);
  let current = true;
  const stale = assert.rejects(engine.exportDocument('epub', '# Old', () => current), {code: 'EXPORT_CANCELLED'});
  await delay(0); current = false; complete({format: 'epub'}); await stale;
  assert.equal(stops, 1); assert.equal(engine.exportPending, false);
  const suspended = assert.rejects(engine.exportDocument('svg', '# Current'), {code: 'EXPORT_CANCELLED'});
  await delay(0); window.dispatchEvent(new Event('pagehide')); await suspended;
  assert.equal(stops >= 2, true); assert.equal(engine.exportPending, false);
  assert.throws(() => engine.exportDocument('svg', '# Suspended'), {code: 'EXPORT_SUSPENDED'});
  window.dispatchEvent(new Event('pageshow'));
  const successful = engine.exportDocument('svg', '# Restored');
  await delay(0); complete({format: 'svg', diagnostics: []});
  assert.equal((await successful).format, 'svg'); assert.equal(starts, 3);
});
