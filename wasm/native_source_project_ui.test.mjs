import assert from 'node:assert/strict';
import {test} from 'node:test';
import {runInNewContext} from 'node:vm';
import {Worker as NodeWorker} from 'node:worker_threads';
import {createNativeSourceProjectCodec, readNativeSourceProject} from './native_source_project.mjs';
import {installNativeSourceProjectDownload} from './native_source_project_ui.mjs';

// Explicit DOM/URL/timer adapters. The serialized worker body is production code,
// exercised both in a separate VM realm and in a real Node worker below.
class Target {
  handlers = new Map();
  addEventListener(kind, fn) { if (!this.handlers.has(kind)) this.handlers.set(kind, new Set()); this.handlers.get(kind).add(fn); }
  removeEventListener(kind, fn) { this.handlers.get(kind)?.delete(fn); }
  emit(kind, event = {}) { for (const fn of [...(this.handlers.get(kind) ?? [])]) fn(event); }
}
class Node extends Target {
  constructor(tag, env) { super(); this.tagName = tag; this.env = env; this.children = []; this.style = {}; this.attributes = {}; this.textContent = ''; }
  setAttribute(key, value) { this.attributes[key] = value; }
  appendChild(node) { this.env.beforeAppend?.(node); node.parentNode = this; this.children.push(node); return node; }
  insertBefore(node, before) { node.parentNode = this; this.children.splice(this.children.indexOf(before), 0, node); }
  remove() { if (this.parentNode) { const index = this.parentNode.children.indexOf(this); if (index >= 0) this.parentNode.children.splice(index, 1); } this.parentNode = null; }
  querySelector(selector) { const id = selector.replace(/^#/, ''); for (const child of this.children) { if (child.id === id) return child; const found = child.querySelector(selector); if (found) return found; } return null; }
  click() { if (this.tagName === 'a') { this.env.beforeClick?.(this); this.env.downloads.push({href: this.href, filename: this.download, blob: this.env.urls.get(this.href)}); } else if (!this.disabled) this.emit('click'); }
}
const saved = () => ({version: 1, wasm: 'NOT_A_LOADED_RUNTIME', bindings: 'throw Error("must never execute");',
  options: {font: 'serif', title: 'Draft', metadataEpochSeconds: 0, pageGeometry: [600.125, 800.25, 30, 40, 50, 60]},
  images: [{destination: 'unused.png', bytes: 'AQID'}], fonts: [{slot: 'body-regular', bytes: 'BAUG', weight: 575}]});
function fixture({loading = false, noWorker = false, serialized = false} = {}) {
  const h = {source: '\ufeff# Exact\r\n\rBody 😀\n', hostReady: true, downloads: [], urls: new Map(), allUrls: new Map(), revoked: [], workers: [], timers: new Map(), observers: [], sequence: 0};
  const doc = new Target(), win = new Target();
  Object.assign(doc, {defaultView: win, readyState: loading ? 'loading' : 'complete', createElement: tag => new Node(tag, h)});
  h.doc = doc; h.win = win; doc.body = new Node('body', h);
  const header = h.header = new Node('header', h), saveHtml = new Node('button', h);
  saveHtml.id = 'btn-save-html'; header.appendChild(saveHtml); doc.body.appendChild(header);
  h.editor = new Node('textarea', h);
  Object.defineProperty(h.editor, 'value', {get() { throw Error('lossless source host must be used instead'); }});
  h.payload = new Node('script', h); h.payload.textContent = JSON.stringify(saved());
  doc.querySelector = selector => ({'body > .fmd-app-header': header,
    'body > #fmd-app-body > #editor-pane > textarea#fmd-editor': h.editor,
    'body > script#fmd-native-runtime[type="application/json"]': h.payload}[selector] ?? null);
  win.setTimeout = fn => { const id = ++h.sequence; h.timers.set(id, fn); return id; };
  win.clearTimeout = id => h.timers.delete(id);
  win.URL = {createObjectURL(blob) { h.beforeUrl?.(blob); const url = 'blob:source-project-' + ++h.sequence; h.urls.set(url, blob); h.allUrls.set(url, blob); return url; },
    revokeObjectURL(url) { h.urls.delete(url); h.revoked.push(url); }};
  win.MutationObserver = class {
    constructor(callback) { this.callback = callback; h.observers.push(this); }
    observe(target) { this.target = target; }
    disconnect() { this.target = null; }
  };
  win.Worker = noWorker ? undefined : class {
    constructor(url) { h.beforeWorker?.(); this.code = h.urls.get(url); this.terminated = false; h.workers.push(this); }
    postMessage(request) { this.request = structuredClone(request); h.beforePost?.(); }
    terminate() { this.terminated = true; h.beforeTerminate?.(); }
    respond(value) { this.onmessage?.({data: value}); }
  };
  h.runtime = {settings: null, settingsPending: false, ready: Promise.resolve()};
  const installer = serialized ? runInNewContext('(' + installNativeSourceProjectDownload.toString() + ')', {Blob, Uint8Array}) : installNativeSourceProjectDownload;
  h.install = () => installer(createNativeSourceProjectCodec, h.runtime, {source: () => { h.beforeSource?.(); return h.source; }, ready: () => h.hostReady}, doc);
  h.ui = h.install();
  h.find = id => header.querySelector('#' + id);
  h.status = () => h.find('fmd-source-project-status')?.textContent;
  h.start = () => h.ui.save();
  h.changeResources = (notify = true) => {
    const value = JSON.parse(h.payload.textContent); value.options.title += '!'; h.payload.textContent = JSON.stringify(value);
    if (notify) for (const observer of h.observers) if (observer.target === h.payload) observer.callback([]);
  };
  h.result = () => { const output = createNativeSourceProjectCodec().encode({source: h.source, ...(({options, images, fonts}) => ({options, images, fonts}))(JSON.parse(h.payload.textContent))}); return {ok: true, bytes: output.bytes, filename: output.filename('Draft')}; };
  h.execute = async (worker = h.workers.at(-1)) => {
    let output;
    const self = {postMessage: value => { output = structuredClone(value); }};
    runInNewContext(await worker.code.text(), {self, TextEncoder, TextDecoder, Blob, atob});
    self.onmessage({data: worker.request}); worker.respond(output); return output;
  };
  h.dispose = () => h.ui.dispose();
  return h;
}

test('saving is explicit and preserves the exact lossless host source and all resources', async () => {
  const h = fixture({serialized: true});
  assert.equal(h.workers.length, 0); assert.equal(h.find('btn-save-source-project').disabled, false);
  assert.equal(h.start(), true); assert.equal(h.start(), false); assert.equal(h.workers.length, 1);
  await h.execute();
  const result = readNativeSourceProject(await h.downloads[0].blob.arrayBuffer());
  assert.equal(result.source, h.source); assert.equal(result.options.metadataEpochSeconds, 0);
  assert.deepEqual([...result.options.pdfImages[0].bytes], [1, 2, 3]);
  assert.deepEqual([...result.options.fontAssets[0].bytes], [4, 5, 6]);
  assert.equal(result.options.fontAssets[0].weight, 575);
  assert.equal(result.options.page.size.widthPt, 600.125);
  assert.match(h.status(), /download requested/); assert.equal(h.workers[0].terminated, true);
  assert.equal(h.timers.size, 0); assert.equal(h.urls.size, 1);
  assert.doesNotMatch(await h.downloads[0].blob.text(), /must never execute|NOT_A_LOADED_RUNTIME/);
  h.dispose(); assert.equal(h.urls.size, 0);
});

test('real worker executes the emitted codec and returns transferable project bytes', async () => {
  const h = fixture(); h.start(); const adapter = h.workers[0];
  const prefix = 'const {parentPort} = require("node:worker_threads"); const self = {postMessage:(data, transfer)=>parentPort.postMessage(data,transfer)};\n';
  const suffix = '\nparentPort.on("message", data => self.onmessage && self.onmessage({data}));';
  const worker = new NodeWorker(prefix + await adapter.code.text() + suffix, {eval: true});
  try {
    const result = await new Promise((resolve, reject) => { worker.once('error', reject); worker.once('message', resolve); worker.postMessage(adapter.request); });
    adapter.respond(result);
    assert.equal(h.downloads.length, 1);
    assert.equal(readNativeSourceProject(await h.downloads[0].blob.arrayBuffer()).source, h.source);
  } finally { await worker.terminate(); h.dispose(); }
});

for (const kind of ['input', 'compositionstart', 'pagehide', 'cancel', 'resource']) test(kind + ' retires encoding and refuses its late completion', () => {
  const h = fixture(); h.start(); const worker = h.workers[0], late = worker.onmessage;
  const result = h.result();
  if (kind === 'pagehide') h.win.emit(kind);
  else if (kind === 'cancel') h.ui.cancel();
  else if (kind === 'resource') h.changeResources();
  else h.editor.emit(kind);
  assert.equal(worker.terminated, true); assert.equal(h.timers.size, 0); assert.equal(h.urls.size, 0);
  late({data: result}); assert.equal(h.downloads.length, 0); h.dispose();
});

for (const change of ['source', 'resources']) test('unannounced ' + change + ' change fences delivery', () => {
  const h = fixture(); h.start(); const result = h.result();
  if (change === 'source') h.source = h.source.replace('Exact', 'Other'); else h.changeResources(false);
  h.workers[0].respond(result);
  assert.equal(h.downloads.length, 0); assert.match(h.status(), /changed/); assert.equal(h.urls.size, 0); h.dispose();
});

test('download activation rechecks after object URL creation', () => {
  const h = fixture(); h.start(); h.beforeUrl = blob => { if (blob.type === 'application/json') h.source += '!'; };
  h.workers[0].respond(h.result()); assert.equal(h.downloads.length, 0); assert.equal(h.urls.size, 0); h.dispose();
});
test('download activation rechecks after inserting the link', () => {
  const h = fixture(); h.start(); h.beforeAppend = node => { if (node.tagName === 'a') h.source += '!'; };
  h.workers[0].respond(h.result()); assert.equal(h.downloads.length, 0); assert.equal(h.urls.size, 0); h.dispose();
});
test('retired worker failure cannot replace a newer status or cancel its work', () => {
  const h = fixture(); h.start(); const fail = h.workers[0].onerror; h.ui.cancel(); h.start();
  const status = h.status(); fail({}); assert.equal(h.status(), status); assert.equal(h.workers[1].terminated, false);
  h.dispose();
});
test('encoding has a deadline and requires explicit retry', () => {
  const h = fixture(); h.start(); const [timeout] = h.timers.values(); timeout();
  assert.match(h.status(), /timed out/); assert.equal(h.workers[0].terminated, true);
  assert.equal(h.downloads.length, 0); assert.equal(h.workers.length, 1); h.start(); assert.equal(h.workers.length, 2); h.dispose();
});
test('without Worker support, save fails explicitly without a synchronous fallback', () => {
  const h = fixture({noWorker: true}); assert.equal(h.start(), false); assert.match(h.status(), /cannot create/);
  assert.equal(h.downloads.length, 0); assert.equal(h.urls.size, 0); h.dispose();
});
test('engine startup failure and pending native settings do not block committed data recovery', async () => {
  const h = fixture(); h.runtime.settingsPending = true; h.runtime.ready = Promise.reject(Error('native failed')); h.runtime.ready.catch(() => {});
  assert.equal(h.start(), true); await h.execute(); assert.equal(h.downloads.length, 1);
  assert.equal(h.runtime.settings, null); assert.equal(h.runtime.settingsPending, true); h.dispose();
});
test('failed startup still installs project controls through the existing lossless host hook', () => {
  const h = fixture(); assert.equal(h.runtime.settings, null);
  h.hostReady = false; assert.equal(h.start(), false); h.hostReady = true;
  assert.equal(h.start(), true); h.dispose();
});
for (const fault of ['beforeWorker', 'beforePost', 'beforeSource']) test('setup failure at ' + fault + ' leaves source and controls usable', () => {
  const h = fixture(); const source = h.source; h[fault] = () => { throw Error('injected setup failure'); };
  assert.equal(h.start(), false); assert.match(h.status(), /failure/); assert.equal(h.urls.size, 0); assert.equal(h.source, source);
  delete h[fault]; assert.equal(h.start(), true); h.dispose();
});
test('worker termination failure does not orphan capacity or later downloads', () => {
  const h = fixture(); h.start(); h.beforeTerminate = () => { throw Error('termination failed'); }; h.ui.cancel();
  assert.equal(h.start(), true); h.dispose(); assert.equal(h.timers.size, 0);
});
for (const value of [null, {ok: 1}, {ok: true, bytes: new Uint8Array(), filename: 'x.fmd.json'},
  {ok: true, bytes: new Uint8Array([1]), filename: '../x.fmd.json'}, {ok: false, message: '<script>not HTML</script>'}]) test('invalid or failed worker result never creates a download ' + JSON.stringify(value), () => {
  const h = fixture(); h.start(); h.workers[0].respond(value); assert.equal(h.downloads.length, 0);
  assert.match(h.status(), /failed/); assert.equal(h.urls.size, 0); assert.equal(h.timers.size, 0); h.dispose();
});
test('worker codec rejects corrupt resources without executing saved binding text', async () => {
  const h = fixture(); const value = saved(); value.images[0].bytes = 'AB=='; h.payload.textContent = JSON.stringify(value);
  h.start(); const response = await h.execute(); assert.equal(response.ok, false);
  assert.equal(h.downloads.length, 0); assert.match(h.status(), /base64/); h.dispose();
});
test('failed click releases the download URL and leaves the draft unchanged', () => {
  const h = fixture(); h.start(); h.beforeClick = () => { throw Error('click failed'); }; h.workers[0].respond(h.result());
  assert.equal(h.downloads.length, 0); assert.equal(h.urls.size, 0); assert.match(h.status(), /click failed/); h.dispose();
});
test('subsequent edits release a completed download and never save automatically', async () => {
  const h = fixture(); h.start(); await h.execute(); assert.equal(h.urls.size, 1);
  h.editor.emit('input'); assert.equal(h.urls.size, 0); assert.equal(h.workers.length, 1); h.dispose();
});
test('save after a completed save replaces its owned URL', async () => {
  const h = fixture(); h.start(); await h.execute(); const first = h.downloads[0].href;
  h.start(); assert.ok(h.revoked.includes(first)); await h.execute(); assert.equal(h.downloads.length, 2);
  assert.equal(h.urls.size, 1); h.dispose();
});
test('composition and page restoration require a fresh explicit save', async () => {
  const h = fixture(); h.editor.emit('compositionstart'); assert.equal(h.start(), false);
  h.editor.emit('compositionend'); assert.equal(h.start(), true); h.win.emit('pagehide'); assert.equal(h.start(), false);
  h.win.emit('pageshow', {persisted: true}); assert.equal(h.workers.length, 1); assert.equal(h.start(), true);
  await h.execute(); assert.equal(h.downloads.length, 1); h.dispose();
});
test('deferred installation and disposal are idempotent', () => {
  const h = fixture({loading: true}); assert.equal(h.find('btn-save-source-project'), null); assert.equal(h.start(), false);
  h.doc.readyState = 'complete'; h.doc.emit('DOMContentLoaded'); h.doc.emit('DOMContentLoaded');
  assert.equal(h.header.children.filter(n => n.id === 'fmd-source-project-controls').length, 1);
  h.dispose(); h.dispose(); assert.equal(h.start(), false); assert.equal(h.observers[0].target, null);
});
test('dispose before DOM readiness prevents installation', () => {
  const h = fixture({loading: true}); h.dispose(); h.doc.emit('DOMContentLoaded'); assert.equal(h.find('btn-save-source-project'), null);
});
test('reopened serialized controls are replaced instead of duplicated', () => {
  const h = fixture(); h.dispose(); h.ui = h.install();
  assert.equal(h.header.children.filter(n => n.id === 'fmd-source-project-controls').length, 1); h.dispose();
});
