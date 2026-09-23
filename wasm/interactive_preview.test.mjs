import assert from 'node:assert/strict';
import {test} from 'node:test';
import {Worker as Thread} from 'node:worker_threads';
import vm from 'node:vm';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';

const payload = {version: 1, options: {font: 'serif'}, images: [], fonts: [],
  wasm: 'AGFzbQEAAAA=', bindings: `
let initialized = false;
export default async function({module_or_path}) {
  await WebAssembly.instantiate(module_or_path);
  initialized = true;
}
export function render(text, options) {
  if (!initialized) throw new Error('WASM not initialized');
  if (text === 'throw') throw new Error('Native rejection');
  if (text === 'busy') { while (true) {} }
  return '<head></head><p>' + options.font + ':' + text + '</p>';
}`};
const display = {scale: 1};
const state = {options: payload.options, images: payload.images};
function renderer(bindings, data) {
  if (data.options.font === 'bad') throw new Error('Invalid settings');
  return {html: text => bindings.render(text, data.options), diagnostics: [{message: 'native diagnostic'}]};
}
function fake() {
  const handlers = new Map(), messages = [];
  let terminations = 0;
  const worker = {
    addEventListener(type, handler) { handlers.set(type, handler); },
    removeEventListener(type, handler) { if (handlers.get(type) === handler) handlers.delete(type); },
    postMessage(value) { messages.push(structuredClone(value)); },
    terminate() { terminations++; },
    emit(data) { handlers.get('message')?.({data}); },
    fail(type = 'error') { handlers.get(type)?.({preventDefault() {}}); },
  };
  const client = createWorkspacePreviewWorker(renderer, payload, {workerFactory: () => worker, timeoutMs: 1000});
  const render = (text, options = state) => client.render(text, display, options);
  const success = (id, html = 'output') => worker.emit({type: 'result', id, html, diagnostics: []});
  return {worker, client, messages, render, success, handlers, get terminations() {return terminations;}};
}
const rejected = (promise, code) => assert.rejects(promise, err => err.code === code);

test('lazy startup retains only the latest pending request and does not copy resources on every edit', async () => {
  const s = fake(); assert.equal(s.messages.length, 0);
  const first = rejected(s.render('old'), 'PREVIEW_SUPERSEDED');
  const second = s.render('latest');
  assert.equal(s.client.pendingOperations, 1);
  assert.equal(s.messages.length, 1);
  s.worker.emit({type: 'ready'});
  assert.equal(s.messages[1].markdown, 'latest');
  assert.deepEqual(s.messages[1].state, state);
  s.success(2); assert.equal((await second).html, 'output'); await first;
  const third = s.render('next');
  assert.equal(s.messages[2].state, null);
  s.success(3); await third; s.client.dispose();
  assert.equal(s.terminations, 1); assert.equal(s.handlers.size, 0);
});

test('one active render and one queued revision bound the queue under continuous input', async () => {
  const s = fake(); const old = rejected(s.render('running'), 'PREVIEW_SUPERSEDED');
  s.worker.emit({type: 'ready'});
  const superseded = [];
  for (let i = 0; i < 50; i++) superseded.push(rejected(s.render('obsolete ' + i), 'PREVIEW_SUPERSEDED'));
  const latest = s.render('retained');
  assert.equal(s.client.pendingOperations, 2);
  assert.equal(s.messages.length, 2);
  s.success(1, 'must not publish');
  assert.equal(s.messages.at(-1).markdown, 'retained');
  s.success(52, 'current'); assert.equal((await latest).html, 'current');
  await old; await Promise.all(superseded); s.client.dispose();
});

test('invalidation rejects both waiting consumers but reuses a settled worker without stale publication', async () => {
  const s = fake(); const a = rejected(s.render('active'), 'PREVIEW_SUPERSEDED');
  s.worker.emit({type: 'ready'});
  const b = rejected(s.render('queued'), 'PREVIEW_SUPERSEDED');
  s.client.invalidate(); s.success(1);
  assert.equal(s.messages.length, 2); assert.equal(s.client.pendingOperations, 0);
  await a; await b;
  const c = s.render('after'); s.success(3); await c; s.client.dispose();
});

test('source admission precedes supersession and preserves exact BOM/newlines and supplementary Unicode', async () => {
  const s = fake(), source = '\ufeff\r\nA😀\r';
  const valid = s.render(source); s.worker.emit({type: 'ready'});
  await rejected(s.render('\ud800'), 'PREVIEW_UNICODE');
  await rejected(s.render('é'.repeat(16 * 1024 * 1024 + 1)), 'PREVIEW_LIMIT');
  assert.equal(s.messages[1].markdown, source);
  s.success(1); await valid; s.client.dispose();
});

test('state revisions travel with their source and failed revisions cannot poison the cache', async () => {
  const s = fake(); const a = rejected(s.render('a'), 'PREVIEW_SUPERSEDED'); s.worker.emit({type: 'ready'});
  const next = {options: {font: 'sans'}, images: [{destination: 'x', bytes: 'AQ=='}]};
  const b = rejected(s.render('b', next), 'PREVIEW_RENDER_FAILED');
  s.success(1); assert.deepEqual(s.messages.at(-1).state, next);
  s.worker.emit({type: 'error', id: 2, message: 'Bad revision'}); await a; await b;
  const c = s.render('c', next); assert.deepEqual(s.messages.at(-1).state, next);
  s.success(3); await c; s.client.dispose();
});

test('malformed responses terminate the worker and settle every waiter', async () => {
  for (const bad of [null, {type: 'ready'}, {type: 'result', id: 9, html: 'x'},
    {type: 'result', id: 2, html: '\udc00', diagnostics: []},
    {type: 'result', id: 2, html: '', diagnostics: []},
    {type: 'result', id: 2, html: 'x', diagnostics: [null]}]) {
    const s = fake(); const a = rejected(s.render('a'), 'PREVIEW_SUPERSEDED'); s.worker.emit({type: 'ready'});
    const b = s.render('b'); const catchB = assert.rejects(b);
    s.success(1); s.worker.emit(bad); await a; await catchB;
    assert.equal(s.client.closed, true); assert.equal(s.terminations, 1); assert.equal(s.handlers.size, 0);
    s.client.dispose(); assert.equal(s.terminations, 1);
  }
});

test('native render errors are recoverable, whereas initialization and transport errors are terminal', async () => {
  const s = fake(); const a = rejected(s.render('bad'), 'PREVIEW_RENDER_FAILED'); s.worker.emit({type: 'ready'});
  s.worker.emit({type: 'error', id: 1, message: 'parser refusal'}); await a;
  const b = s.render('good'); s.success(2); await b; s.client.dispose();
  for (const kind of ['error', 'messageerror', 'init']) {
    const f = fake(); const p = assert.rejects(f.render('keep'));
    if (kind === 'init') f.worker.emit({type: 'error', message: 'invalid WASM'}); else f.worker.fail(kind);
    await p; assert.equal(f.terminations, 1); assert.equal(f.client.closed, true);
  }
});

test('constructor and postMessage failures reject without leaking an owned worker', async () => {
  const broken = createWorkspacePreviewWorker(renderer, payload, {workerFactory() {throw Error('blocked');}});
  await rejected(broken.render('source', display, state), 'PREVIEW_START_FAILED'); assert.equal(broken.closed, true);
  const s = fake(); s.worker.postMessage = () => {throw Error('clone');};
  await rejected(s.render('source'), 'PREVIEW_START_FAILED'); assert.equal(s.terminations, 1);
});

test('explicit disposal settles startup and active work, and an invalid request never starts a worker', async () => {
  const s = fake(); await rejected(s.client.render('x', {scale: NaN}, state), 'PREVIEW_OPTIONS');
  assert.equal(s.messages.length, 0);
  const p = rejected(s.render('startup'), 'PREVIEW_CLOSED'); s.client.dispose(); await p;
  await rejected(s.render('later'), 'PREVIEW_CLOSED');
  const t = fake(); const q = rejected(t.render('active'), 'PREVIEW_CLOSED'); t.worker.emit({type: 'ready'});
  t.client.dispose(); await q; assert.equal(t.terminations, 1);
});

// Real worker_threads + actual WebAssembly initialization. Only browser Blob URL
// loading is adapted to data URLs; renderer exports remain explicit test doubles.
function realWorker(code) {
  const thread = new Thread(`
    const {parentPort} = require('node:worker_threads');
    globalThis.self = globalThis;
    globalThis.Blob = class { constructor(parts) {this.text = parts.join('');} };
    URL.createObjectURL = blob => 'data:text/javascript;base64,' + Buffer.from(blob.text).toString('base64');
    URL.revokeObjectURL = () => {};
    globalThis.postMessage = value => parentPort.postMessage(value);
    parentPort.on('message', data => globalThis.onmessage({data}));
    ${code}
  `, {eval: true});
  const wrappers = new Map();
  return {
    addEventListener(type, fn) { const wrapped = data => fn(type === 'message' ? {data} : data); wrappers.set(fn, wrapped); thread.on(type, wrapped); },
    removeEventListener(type, fn) {const wrapped = wrappers.get(fn); if (wrapped) thread.off(type, wrapped);},
    postMessage: value => thread.postMessage(value), terminate: () => thread.terminate(),
  };
}

test('serialized production worker initializes WASM, renders exact source and applies resource revisions', async () => {
  const s = createWorkspacePreviewWorker(renderer, payload, {workerFactory: realWorker});
  try {
    const result = await s.render('real\r\n😀', display, state);
    assert.equal(result.html, '<head></head><p>serif:real\r\n😀</p>');
    assert.equal(result.diagnostics[0].message, 'native diagnostic');
    await rejected(s.render('throw', display, state), 'PREVIEW_RENDER_FAILED');
    await rejected(s.render('bad', display, {options: {font: 'bad'}, images: []}), 'PREVIEW_RENDER_FAILED');
    const next = await s.render('recovered', display, {options: {font: 'sans'}, images: []});
    assert.match(next.html, /sans:recovered/);
  } finally {s.dispose();}
});

test('a busy renderer cannot block the host, and its deadline terminates even a superseded active job', async () => {
  const s = createWorkspacePreviewWorker(renderer, payload, {workerFactory: realWorker, timeoutMs: 200});
  try {
    await s.render('warm', display, state);
    const busy = rejected(s.render('busy', display, state), 'PREVIEW_SUPERSEDED');
    const next = rejected(s.render('latest', display, state), 'PREVIEW_TIMEOUT');
    let beats = 0; const timer = setInterval(() => {beats++;}, 10);
    try {await busy; await next;} finally {clearInterval(timer);}
    assert.ok(beats > 2, 'host event loop kept executing');
    assert.equal(s.closed, true); assert.equal(s.pendingOperations, 0);
  } finally {s.dispose();}
});

test('the entire factory serializes without module globals and rejects unavailable workers explicitly', async () => {
  const fn = vm.runInNewContext('(' + createWorkspacePreviewWorker.toString() + ')', {setTimeout, clearTimeout});
  const s = fn(renderer, payload);
  await rejected(s.render('keep', display, state), 'PREVIEW_START_FAILED');
  assert.doesNotMatch(createWorkspacePreviewWorker.toString(), /<\/script/i);
});
