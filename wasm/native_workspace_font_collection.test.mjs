// Execute the production worker protocol. The binding fixture tests ownership
// and dispatch; actual font extraction is separately exercised by Rust tests.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {Worker as Thread} from 'node:worker_threads';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';

const face = () => new Uint8Array([0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
const input = () => new Uint8Array([116, 116, 99, 102, 1, 2, 3]);
const payload = {version: 1, options: {font: 'serif'}, images: [], fonts: [],
  wasm: 'AGFzbQEAAAA=', bindings: `
export default async function({module_or_path}) { await WebAssembly.instantiate(module_or_path); }
export function extractFontCollectionFace(bytes, index) {
  if (index === 3) throw Error('FONT_COLLECTION_FACE_INDEX: selected face does not exist');
  const face = new Uint8Array(12); face[1] = 1;
  face[4] = bytes[4]; face[5] = index;
  return face;
}`};
function renderer() { return {html: () => '<p>ok</p>', diagnostics: []}; }
function fake() {
  const handlers = new Map(), messages = [];
  let stopped = 0;
  const worker = {
    addEventListener(type, fn) { handlers.set(type, fn); },
    removeEventListener(type, fn) { if (handlers.get(type) === fn) handlers.delete(type); },
    postMessage(value, transfers) { messages.push(structuredClone(value, {transfer: transfers ?? []})); },
    terminate() { stopped++; },
    emit(data) { handlers.get('message')?.({data}); },
  };
  const client = createWorkspacePreviewWorker(renderer, payload, {workerFactory: () => worker});
  return {worker, client, messages, get stopped() { return stopped; }};
}

test('collection selection owns exact view ranges before worker initialization and never detaches caller memory', async () => {
  for (const wrap of [bytes => bytes.subarray(1, 8), bytes => new DataView(bytes.buffer, 1, 7),
    bytes => Buffer.from(bytes.buffer, 1, 7)]) {
    const s = fake(), bytes = new Uint8Array([99, ...input(), 99]);
    const pending = s.client.extractFontCollectionFace(wrap(bytes), 1);
    bytes.fill(77);
    assert.equal(s.messages.length, 1);
    s.worker.emit({type: 'ready'});
    assert.deepEqual(s.messages[1].bytes, input());
    assert.equal(bytes.byteLength, 9);
    assert.equal(s.messages[1].faceIndex, 1);
    const selected = face();
    s.worker.emit({type: 'result', id: 1, operation: 'font-collection', faceIndex: 1, bytes: selected});
    assert.equal(await pending, selected);
    s.client.dispose(); assert.equal(s.stopped, 1);
  }
});

test('invalid indices, shared buffers, detached buffers and excessive inputs reject before a worker starts', async () => {
  const s = fake();
  for (const index of [undefined, null, -1, 0.1, NaN, Infinity, 1024, '0']) {
    await assert.rejects(s.client.extractFontCollectionFace(input(), index), /integer/);
  }
  const detached = input(); structuredClone(detached, {transfer: [detached.buffer]});
  for (const bytes of [new Uint8Array(), new SharedArrayBuffer(12), new Uint8Array(new SharedArrayBuffer(12)),
    detached, new Uint8Array(32 * 1024 * 1024 + 1), {buffer: input().buffer}]) {
    await assert.rejects(s.client.extractFontCollectionFace(bytes, 0));
  }
  assert.equal(s.messages.length, 0); s.client.dispose();
});

test('collection result identity, owned bytes and standalone signatures are enforced', async () => {
  for (const patch of [{faceIndex: 2}, {operation: 'render'}, {bytes: new Uint8Array(12)},
    {bytes: face().subarray(0, 11)}, {bytes: new Uint8Array(32 * 1024 * 1024 + 1)}]) {
    const s = fake(), pending = s.client.extractFontCollectionFace(input(), 1);
    s.worker.emit({type: 'ready'});
    s.worker.emit({type: 'result', id: 1, operation: 'font-collection', faceIndex: 1, bytes: face(), ...patch});
    await assert.rejects(pending, error => error.code === 'FONT_COLLECTION_PROTOCOL');
    assert.equal(s.client.closed, true); assert.equal(s.stopped, 1);
  }
});

test('collection work cannot be superseded by previews and cancellation discards late faces', async () => {
  const s = fake(), pending = s.client.extractFontCollectionFace(input(), 0);
  s.worker.emit({type: 'ready'});
  await assert.rejects(s.client.render('source', {scale: 1}, {options: payload.options, images: []}),
    error => error.code === 'FONT_COLLECTION_BUSY');
  await assert.rejects(s.client.extractFontCollectionFace(input(), 1), error => error.code === 'FONT_COLLECTION_BUSY');
  s.client.invalidate();
  s.worker.emit({type: 'result', id: 1, operation: 'font-collection', faceIndex: 0, bytes: face()});
  await assert.rejects(pending, error => error.code === 'FONT_COLLECTION_CANCELLED');
  assert.equal(s.stopped, 1);
});

function threadFactory(source) {
  // Node lacks browser Blob imports. Only translate that transport boundary;
  // the serialized production worker, message protocol and binding call run.
  const code = `const {parentPort} = require('node:worker_threads');
    const urls = new Map(); let sequence = 0;
    globalThis.self = globalThis;
    self.postMessage = (message, transfer) => parentPort.postMessage(message, transfer);
    URL.createObjectURL = blob => { const id = 'blob:test-' + ++sequence; urls.set(id, blob); return id; };
    URL.revokeObjectURL = id => urls.delete(id);
    async function importBlob(url) { const text = await urls.get(url).text();
      return import('data:text/javascript;base64,' + Buffer.from(text).toString('base64')); }
    parentPort.on('message', data => self.onmessage({data}));
    ${source.replace('await import(url)', 'await importBlob(url)')}`;
  const worker = new Thread(code, {eval: true});
  const callbacks = new Map();
  return {
    postMessage(message, transfer) { worker.postMessage(message, transfer); },
    addEventListener(type, fn) {
      const handler = type === 'message' ? data => fn({data}) : error => fn(error);
      callbacks.set(type, {fn, handler}); worker.on(type, handler);
    },
    removeEventListener(type, fn) {
      const entry = callbacks.get(type);
      if (entry?.fn === fn) { worker.off(type, entry.handler); callbacks.delete(type); }
    },
    terminate() { return worker.terminate(); },
  };
}

test('real worker transport calls the selected native binding and preserves later rendering', async () => {
  const client = createWorkspacePreviewWorker(renderer, payload, {workerFactory: threadFactory});
  try {
    const selected = await client.extractFontCollectionFace(input(), 2);
    assert.equal(selected[4], 1); assert.equal(selected[5], 2);
    await assert.rejects(client.extractFontCollectionFace(input(), 3), error =>
      error.code === 'FONT_COLLECTION_FACE_INDEX' && /selected face does not exist/.test(error.message));
    assert.equal((await client.render('source', {scale: 1}, {options: payload.options, images: []})).html, '<p>ok</p>');
    assert.equal((await client.extractFontCollectionFace(input(), 1))[5], 1);
  } finally { client.dispose(); }
});

test('old embedded bindings refuse collection selection without breaking ordinary previews', async () => {
  const old = {...payload, bindings: 'export default async function() {}'};
  const client = createWorkspacePreviewWorker(renderer, old, {workerFactory: threadFactory});
  try {
    await assert.rejects(client.extractFontCollectionFace(input(), 0), error => error.code === 'UNSUPPORTED_WASM_PACKAGE');
    assert.equal((await client.render('source', {scale: 1}, {options: payload.options, images: []})).html, '<p>ok</p>');
  } finally { client.dispose(); }
});
