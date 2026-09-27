import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {createNativePdfProof} from './native_pdf_proof.mjs';
import {bootNativeWorkspace, createNativeWorkspaceRenderer} from './interactive_runtime.mjs';

const tick = () => new Promise(resolve => setImmediate(resolve));
const bytes = text => new TextEncoder().encode(text);
const output = (text = '%PDF-1.7\nproof\n') => ({format: 'pdf', mimeType: 'application/pdf', bytes: bytes(text), diagnostics: []});
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
function fixture(make = createNativePdfProof) {
  const state = {source: '\ufeff# Exact\r\n\r\nText\r', settings: Object.freeze({title: 'Original'}), resources: 0};
  const requests = [];
  let blocked = false;
  const runtime = {beginExportDocument(format, source, current) {
    if (blocked) throw Object.assign(Error('busy'), {code: 'EXPORT_BUSY'});
    const operation = {...deferred(), format, source, current, cancelled: 0};
    requests.push(operation);
    return {promise: operation.promise, cancel() { operation.cancelled++; }};
  }};
  const proof = make(runtime, () => state);
  return {state, requests, proof, block(value) { blocked = value; }};
}

test('exact source and immutable selected PDF bytes are reused without another native render', async () => {
  const f = fixture(), work = f.proof.render(), request = f.requests[0];
  assert.equal(request.format, 'pdf'); assert.equal(request.source, f.state.source); assert.equal(request.current(), true);
  const backing = bytes('prefix%PDF-1.7\nexact\nsuffix');
  const result = {...output(), bytes: backing.subarray(6, backing.length - 6), diagnostics: [{message: '<literal finding>'}]};
  request.resolve(result);
  const proof = await work;
  result.bytes.fill(0); result.diagnostics[0].message = 'changed';
  assert.equal(await proof.blob.text(), '%PDF-1.7\nexact\n');
  assert.equal(proof.blob.type, 'application/pdf'); assert.equal(proof.size, proof.blob.size);
  assert.deepEqual(proof.diagnostics, ['<literal finding>']);
  assert.ok(Object.isFrozen(proof)); assert.ok(Object.isFrozen(proof.diagnostics));
  assert.equal(await f.proof.render(), proof); assert.equal(f.requests.length, 1); assert.equal(f.proof.current, proof);
  assert.equal(f.proof.pending, false);
});

for (const key of ['source', 'settings', 'resources']) {
  test(`${key} change retires both pending and retained proofs without an input event`, async () => {
    const f = fixture(); const first = f.proof.render();
    f.state[key] = key === 'source' ? 'changed' : key === 'settings' ? {title: 'Changed'} : 1;
    assert.equal(f.requests[0].current(), false); f.requests[0].resolve(output());
    await assert.rejects(first, {code: 'PROOF_CANCELLED'}); assert.equal(f.proof.current, null);
    const next = f.proof.render(); f.requests[1].resolve(output()); await next;
    f.state[key] = key === 'source' ? 'changed again' : key === 'settings' ? {title: 'Again'} : 2;
    assert.equal(f.proof.current, null);
  });
}

test('one pending job, no queue; cancellation settles even if the worker never responds', async () => {
  const f = fixture(), first = f.proof.render();
  await assert.rejects(f.proof.render(), {code: 'PROOF_BUSY'}); assert.equal(f.requests.length, 1);
  f.proof.cancel(); await assert.rejects(first, {code: 'PROOF_CANCELLED'});
  assert.equal(f.requests[0].cancelled, 1); assert.equal(f.proof.pending, false);
  f.proof.cancel(); assert.equal(f.requests[0].cancelled, 1);
  const next = f.proof.render(); f.requests[1].resolve(output(' %PDF-wrong'));
  await assert.rejects(next, {code: 'PROOF_RESULT'});
});

test('late cancelled results and errors cannot overwrite a replacement proof', async () => {
  for (const reject of [false, true]) {
    const f = fixture(), old = f.proof.render();
    f.proof.invalidate(); await assert.rejects(old, {code: 'PROOF_CANCELLED'});
    const next = f.proof.render();
    if (reject) f.requests[0].reject(Error('late failure')); else f.requests[0].resolve(output('%PDF-old'));
    await tick(); assert.equal(f.proof.pending, true); assert.equal(f.proof.current, null);
    f.requests[1].resolve(output('%PDF-new')); assert.equal(await (await next).blob.text(), '%PDF-new');
  }
});

test('edit/undo round trips still retire the captured revision', async () => {
  const f = fixture(), old = f.proof.render(), source = f.state.source;
  f.state.source = 'edit'; f.proof.invalidate(); f.state.source = source;
  await assert.rejects(old, {code: 'PROOF_CANCELLED'});
  f.requests[0].resolve(output()); await tick(); assert.equal(f.proof.current, null);
});

test('busy admission does not cancel an unrelated native operation; explicit retry works', async () => {
  const f = fixture(); f.block(true);
  await assert.rejects(f.proof.render(), {code: 'EXPORT_BUSY'});
  f.proof.cancel(); assert.equal(f.requests.length, 0); assert.equal(f.proof.pending, false);
  f.block(false); const next = f.proof.render(); f.requests[0].resolve(output()); await next;
});

test('failed/invalid native results are never retained and never trigger implicit retries', async () => {
  const invalid = [null, {...output(), format: 'html'}, {...output(), mimeType: 'text/html'},
    {...output(), bytes: new Uint8Array()}, {...output(), bytes: new DataView(new ArrayBuffer(8))},
    {...output(), bytes: bytes('<html>')}, {...output(), diagnostics: {}},
    {...output(), diagnostics: [{message: 7}]}, {...output(), diagnostics: Array(4097).fill({message: 'x'})},
    {...output(), diagnostics: [{message: 'x'.repeat(1024 * 1024 + 1)}]}];
  for (const value of invalid) {
    const f = fixture(), work = f.proof.render(); f.requests[0].resolve(value);
    await assert.rejects(work, {code: 'PROOF_RESULT'});
    assert.equal(f.proof.current, null); assert.equal(f.requests.length, 1); assert.equal(f.proof.pending, false);
  }
  const f = fixture(), work = f.proof.render(); f.requests[0].reject(Object.assign(Error('timeout'), {code: 'EXPORT_TIMEOUT'}));
  await assert.rejects(work, {code: 'EXPORT_TIMEOUT'}); assert.equal(f.requests.length, 1);
});

test('invalid providers fail closed; asynchronous render consistently rejects', async () => {
  assert.throws(() => createNativePdfProof({}, () => ({})), TypeError);
  const native = {beginExportDocument() { throw Error('must not render'); }};
  for (const value of [null, {}, {source: 's', settings: null, resources: 0}]) {
    const proof = createNativePdfProof(native, () => value);
    await assert.rejects(proof.render(), {code: 'PROOF_SNAPSHOT'});
  }
  const proof = createNativePdfProof(native, () => { throw Error('snapshot failure'); });
  await assert.rejects(proof.render(), /snapshot failure/);
});

test('disposal releases retained output and permanently closes the session', async () => {
  const f = fixture(), work = f.proof.render(); f.requests[0].resolve(output()); await work;
  f.proof.dispose(); f.proof.dispose(); assert.equal(f.proof.current, null);
  await assert.rejects(f.proof.render(), {code: 'PROOF_CLOSED'});
  const g = fixture(), pending = g.proof.render(); g.proof.dispose();
  await assert.rejects(pending, {code: 'PROOF_CANCELLED'}); assert.equal(g.requests[0].cancelled, 1);
});

test('standalone serialization needs no imports or module closures', async () => {
  const make = vm.runInNewContext('(' + createNativePdfProof.toString() + ')', {Blob, Uint8Array, ArrayBuffer});
  const f = fixture(make), work = f.proof.render(); f.requests[0].resolve(output());
  assert.equal(await (await work).blob.text(), '%PDF-1.7\nproof\n');
});

// Execute the production bootstrap/renderer. Only the DOM/URL and native
// worker transport are adapters; these tests do not claim a compiled WASM PDF.
async function withRuntime(run) {
  const names = ['window', 'document', 'URL'], before = names.map(name => Object.getOwnPropertyDescriptor(globalThis, name));
  const payload = {version: 1, options: {}, images: [], fonts: [], wasm: 'AGFzbQEAAAA=', bindings: 'adapter'};
  const data = {textContent: JSON.stringify(payload)}, workers = [];
  let renderer;
  globalThis.window = new EventTarget(); globalThis.document = {querySelector: () => data};
  globalThis.URL = {createObjectURL: () => 'data:text/javascript,' + encodeURIComponent(
    'export default async ({module_or_path}) => { await WebAssembly.instantiate(module_or_path); }'), revokeObjectURL() {}};
  const bindings = {renderHtmlConfiguredAdvanced() {}, renderPdfConfiguredMulti() {}, documentStats() {}};
  function workerFactory() {
    const job = {...deferred(), calls: [], stopped: 0}; workers.push(job);
    return {exportDocument(...args) { job.calls.push(args); return job.promise; },
      analyzeDocument(...args) { job.calls.push(args); return job.promise; },
      dispose() { job.stopped++; job.reject(Error('disposed')); }};
  }
  try {
    bootNativeWorkspace((_bindings, saved) => (renderer = createNativeWorkspaceRenderer(bindings, saved)), workerFactory);
    const engine = window.__fmdNativeRuntime; assert.equal(await engine.ready, true);
    await run({engine, workers, renderer, data});
  } finally {
    names.forEach((name, i) => before[i] ? Object.defineProperty(globalThis, name, before[i]) : delete globalThis[name]);
  }
}

test('native export handles cancel only their own accepted operation', () => withRuntime(async ({engine, workers}) => {
  const old = engine.beginExportDocument('pdf', 'old'); assert.ok(Object.isFrozen(old));
  await tick(); workers[0].resolve(output()); await old.promise;
  const next = engine.beginExportDocument('html', 'new'); await tick();
  assert.equal(old.cancel(), false); assert.equal(engine.exportPending, true); assert.equal(workers[1].stopped, 0);
  assert.equal(next.cancel(), true); assert.equal(next.cancel(), false);
  await assert.rejects(next.promise, {code: 'EXPORT_CANCELLED'}); assert.equal(engine.exportPending, false);
}));

test('retired export handles cannot cancel document analysis', () => withRuntime(async ({engine, workers}) => {
  const old = engine.beginExportDocument('pdf', 'old'); await tick(); workers[0].resolve(output()); await old.promise;
  const analysis = engine.analyzeDocument('stats', 'new'); await tick();
  assert.equal(old.cancel(), false); assert.equal(engine.analysisPending, true); assert.equal(workers[1].stopped, 0);
  workers[1].resolve({kind: 'stats'}); await analysis;
}));

test('cancel before worker creation releases the native slot without starting a worker', () => withRuntime(async ({engine, workers}) => {
  const handle = engine.beginExportDocument('pdf', 's'); assert.equal(handle.cancel(), true);
  await assert.rejects(handle.promise, {code: 'EXPORT_CANCELLED'}); assert.equal(workers.length, 0);
}));

test('native revision tracks settings and resource commits/rollback, not preview reads', () => withRuntime(async ({engine, renderer}) => {
  assert.equal(engine.documentRevision, 0);
  const settings = renderer.stageSettings({title: 'Revised'}); settings.commit();
  assert.equal(engine.documentRevision, 1); settings.rollback(); assert.equal(engine.documentRevision, 2);
  renderer.stageImages([{destination: 'new.png', bytes: Uint8Array.of(1)}]).commit();
  assert.equal(engine.documentRevision, 3);
  renderer.stageFonts([{slot: 'body-regular', bytes: Uint8Array.of(2)}]).commit();
  assert.equal(engine.documentRevision, 4); assert.equal(engine.documentRevision, 4);
}));

test('proof session uses the actual shared native operation slot and preserves preview diagnostics', () => withRuntime(async ({engine, workers}) => {
  const state = {source: '\ufeffexact\r\n'};
  const session = createNativePdfProof(engine, () => ({source: state.source, settings: engine.settings, resources: engine.documentRevision}));
  const diagnostics = engine.diagnostics, work = session.render();
  assert.throws(() => engine.analyzeDocument('stats', 's'), {code: 'ANALYSIS_BUSY'});
  await tick(); assert.equal(workers[0].calls[0][1], state.source);
  workers[0].resolve({...output(), diagnostics: [{message: 'PDF only'}]}); await work;
  assert.equal(engine.diagnostics, diagnostics); assert.equal(engine.exportPending, false);
  assert.equal(workers[0].stopped, 1); session.dispose();
}));
