import assert from 'node:assert/strict';
import {test} from 'node:test';
import {runInNewContext} from 'node:vm';
import {installNativeSourceProjectOpen} from './native_source_project_open_ui.mjs';

// Explicit DOM/URL/worker-operation adapters, not browser or WASM qualification.
class Target {
  handlers = new Map();
  addEventListener(kind, fn) { if (!this.handlers.has(kind)) this.handlers.set(kind, new Set()); this.handlers.get(kind).add(fn); }
  removeEventListener(kind, fn) { this.handlers.get(kind)?.delete(fn); }
  emit(kind, event = {}) { for (const fn of [...(this.handlers.get(kind) ?? [])]) fn({target: this, ...event}); }
}
class Node extends Target {
  constructor(tag, h) { super(); this.tagName = tag; this.h = h; this.children = []; this.style = {}; this.attributes = {}; this.textContent = ''; this.open = false; this.files = []; }
  set value(value) { this.storedValue = value; if (this.tagName === 'input' && value === '') this.files = []; }
  get value() { return this.storedValue ?? ''; }
  setAttribute(key, value) { this.attributes[key] = value; }
  appendChild(node) { this.h.beforeAppend?.(node); node.parentNode = this; this.children.push(node); return node; }
  insertBefore(node, before) { node.parentNode = this; this.children.splice(this.children.indexOf(before), 0, node); }
  replaceChildren(...nodes) { for (const node of this.children) node.parentNode = null; this.children = []; for (const node of nodes) this.appendChild(node); }
  remove() { if (this.parentNode) { const index = this.parentNode.children.indexOf(this); if (index >= 0) this.parentNode.children.splice(index, 1); } this.parentNode = null; }
  querySelector(selector) { const id = selector.slice(selector.indexOf('#') + 1); for (const child of this.children) { if (child.id === id) return child; const found = child.querySelector('#' + id); if (found) return found; } return null; }
  focus() { this.h.active = this; }
  showModal() { this.h.beforeShow?.(); this.open = true; }
  close() { this.open = false; this.emit('close'); }
  click() {
    if (this.disabled) return;
    if (this.tagName === 'a') { this.h.beforeClick?.(); this.h.downloads.push({url: this.href, filename: this.download, blob: this.h.urls.get(this.href)}); }
    else this.emit('click');
  }
}
function result() { return {bytes: new TextEncoder().encode('<!doctype html><p>Opened</p>'), filename: 'Opened.html', diagnostics: [], diagnosticCount: 0}; }
function fixture({loading = false, serialized = false} = {}) {
  const h = {ready: true, source: '\ufeffCURRENT UNSAVED\r\n', jobs: [], downloads: [], urls: new Map(), revoked: [], observers: [], sequence: 0};
  const doc = h.doc = new Target(), win = h.win = new Target();
  Object.assign(doc, {defaultView: win, readyState: loading ? 'loading' : 'complete', createElement: tag => new Node(tag, h)});
  doc.head = new Node('head', h); doc.body = new Node('body', h);
  const header = h.header = new Node('header', h), save = new Node('button', h); save.id = 'btn-save-html'; header.appendChild(save); doc.body.appendChild(header);
  h.editor = new Node('textarea', h); h.editor.value = h.source;
  h.payload = new Node('script', h); h.payload.textContent = '{"version":1,"bindings":"selected trusted engine","wasm":"selected bytes"}';
  doc.querySelector = selector => ({'body > .fmd-app-header': header,
    'body > script#fmd-native-runtime[type="application/json"]': h.payload,
    'body > #fmd-app-body > #editor-pane > textarea#fmd-editor': h.editor}[selector]
    ?? (selector.startsWith('head >') ? doc.head : doc.body).querySelector(selector));
  win.URL = {createObjectURL(blob) { h.beforeUrl?.(); const url = 'blob:opened-' + ++h.sequence; h.urls.set(url, blob); return url; },
    revokeObjectURL(url) { h.urls.delete(url); h.revoked.push(url); }};
  win.MutationObserver = class {
    constructor(callback) { this.callback = callback; h.observers.push(this); }
    observe(target) { this.target = target; }
    disconnect() { this.target = null; }
  };
  h.runtime = {settings: null, settingsPending: false, exportPending: false, ready: Promise.resolve()};
  h.parts = {createNativeSourceProjectOpenWorker(publisher, parts, request, env) {
    h.beforeStart?.(); assert.equal(env, win); assert.equal(parts, h.parts);
    let resolve, reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    const job = {request, resolve, reject, cancelled: false}; h.jobs.push(job);
    return {promise, cancel() { job.cancelled = true; }};
  }};
  const installer = serialized ? runInNewContext('(' + installNativeSourceProjectOpen.toString() + ')', {Blob}) : installNativeSourceProjectOpen;
  h.install = () => installer(() => {}, h.parts, h.runtime,
    {ready: () => { h.beforeReady?.(); return h.ready; }, source() { throw Error('Opening must not read or normalize the current source'); }}, doc);
  h.ui = h.install();
  h.find = id => doc.body.querySelector('#' + id);
  h.input = () => h.find('fmd-project-open-file');
  h.dialog = () => h.find('fmd-open-source-project');
  h.status = () => h.find('fmd-project-open-status')?.textContent;
  h.click = id => h.find(id).click();
  h.pick = (file = new Blob(['{}']), notify = true) => { h.input().files = [file]; if (notify) h.input().emit('change'); return file; };
  h.start = () => { h.ui.open(); const file = h.pick(); h.click('btn-build-source-project'); return file; };
  h.complete = async (index = h.jobs.length - 1, value = result()) => { h.jobs[index].resolve(value); await Promise.resolve(); await Promise.resolve(); };
  h.mutate = (notify = true) => { h.payload.textContent += ' '; if (notify) for (const observer of h.observers) if (observer.target) observer.callback([]); };
  return h;
}

test('serialized controls wait for explicit build and explicit download, leaving the current document untouched', async () => {
  const h = fixture({serialized: true}); assert.equal(h.jobs.length, 0); assert.equal(h.ui.open(), true);
  assert.equal(h.ui.open(), false); assert(h.find('btn-build-source-project').disabled);
  const file = h.pick(); assert.equal(h.jobs.length, 0); const runtime = h.payload.textContent;
  h.click('btn-build-source-project'); h.click('btn-build-source-project'); assert.equal(h.jobs.length, 1);
  assert.equal(h.jobs[0].request.file, file); assert.equal(h.jobs[0].request.runtime, runtime);
  assert.equal(Object.keys(h.jobs[0].request).sort().join(','), 'file,runtime');
  await h.complete(); assert.equal(h.downloads.length, 0); assert.equal(h.urls.size, 0);
  assert.match(h.status(), /Workspace ready/); h.click('btn-download-opened-project');
  assert.equal(h.downloads.length, 1); assert.equal(await h.downloads[0].blob.text(), '<!doctype html><p>Opened</p>');
  assert.equal(h.editor.value, h.source); assert.equal(h.payload.textContent, runtime); assert.equal(h.runtime.settings, null);
  assert.match(h.status(), /download requested/); h.ui.dispose(); assert.equal(h.urls.size, 0);
});

test('repeat download retains the exact prepared bytes without another render', async () => {
  const h = fixture(); h.start(); await h.complete(); h.click('btn-download-opened-project');
  const first = h.downloads[0]; h.click('btn-download-opened-project');
  assert.equal(h.jobs.length, 1); assert.equal(h.downloads[1].blob, first.blob);
  assert(h.revoked.includes(first.url)); assert.equal(h.urls.size, 1); h.ui.dispose();
});

for (const cause of ['selection', 'resources', 'cancel', 'close', 'escape', 'composition', 'suspension', 'dispose']) {
  test(cause + ' retires pending work and refuses a late completion', async () => {
    const h = fixture(); h.start();
    if (cause === 'selection') h.pick();
    if (cause === 'resources') h.mutate();
    if (cause === 'cancel') h.click('btn-cancel-open-project');
    if (cause === 'close') h.click('btn-close-open-project');
    if (cause === 'escape') h.dialog().emit('cancel', {preventDefault() {}});
    if (cause === 'composition') h.editor.emit('compositionstart');
    if (cause === 'suspension') h.win.emit('pagehide');
    if (cause === 'dispose') h.ui.dispose();
    assert(h.jobs[0].cancelled); await h.complete();
    assert.equal(h.downloads.length, 0); assert.equal(h.urls.size, 0);
    if (cause !== 'dispose') { assert(h.find('btn-download-opened-project').disabled); h.ui.dispose(); }
  });
}

for (const cause of ['selection', 'resources']) test('unannounced ' + cause + ' changes are checked at worker completion', async () => {
  const h = fixture(); h.start(); if (cause === 'selection') h.pick(new Blob(['different']), false); else h.mutate(false);
  await h.complete(); assert.match(h.status(), /changed/); assert(h.find('btn-download-opened-project').disabled); h.ui.dispose();
});

for (const stage of ['beforeUrl', 'beforeAppend']) test('download rechecks its snapshot at ' + stage, async () => {
  const h = fixture(); h.start(); await h.complete();
  h[stage] = node => { if (!node || node.tagName === 'a') h.pick(new Blob(['new']), false); };
  h.click('btn-download-opened-project'); assert.equal(h.downloads.length, 0); assert.equal(h.urls.size, 0); h.ui.dispose();
});

test('a programmatic file switch without change cannot download the old prepared result', async () => {
  const h = fixture(); h.start(); await h.complete(); h.pick(new Blob(['other']), false);
  h.click('btn-download-opened-project'); assert.equal(h.downloads.length, 0); assert.match(h.status(), /changed/); h.ui.dispose();
});

test('cancellation allows an explicit retry and old failures cannot change its status', async () => {
  const h = fixture(); h.start(); h.click('btn-cancel-open-project'); h.click('btn-build-source-project');
  const status = h.status(); h.jobs[0].reject(Error('late failure')); await Promise.resolve();
  assert.equal(h.status(), status); assert.equal(h.jobs.length, 2); await h.complete(); assert.match(h.status(), /ready/); h.ui.dispose();
});

test('read or renderer errors are displayed as text and preserve the selected file for explicit retry', async () => {
  const h = fixture(), chosen = h.start(); h.jobs[0].reject(Error('<script>not markup</script>')); await Promise.resolve();
  assert.match(h.status(), /<script>not markup/); assert.match(h.status(), /unchanged/);
  assert.equal(h.input().files[0], chosen); assert.equal(h.jobs.length, 1);
  h.click('btn-build-source-project'); assert.equal(h.jobs.length, 2); h.ui.dispose();
});

test('worker-construction failure leaves controls ready for explicit retry', () => {
  const h = fixture(); h.beforeStart = () => { throw Error('worker unavailable'); };
  h.start(); assert.match(h.status(), /worker unavailable/); assert.equal(h.jobs.length, 0);
  assert.equal(h.find('btn-build-source-project').disabled, false); h.ui.dispose();
});

test('download failure releases its URL but keeps valid prepared bytes for retry', async () => {
  const h = fixture(); h.start(); await h.complete(); h.beforeClick = () => { throw Error('download blocked'); };
  h.click('btn-download-opened-project'); assert.equal(h.urls.size, 0); assert.match(h.status(), /download blocked/);
  delete h.beforeClick; h.click('btn-download-opened-project'); assert.equal(h.downloads.length, 1); assert.equal(h.jobs.length, 1); h.ui.dispose();
});

test('suspension releases files and downloads; restoration never replays work', async () => {
  const h = fixture(); h.start(); await h.complete(); h.click('btn-download-opened-project');
  h.win.emit('pagehide'); assert.equal(h.urls.size, 0); assert.equal(h.input().files.length, 0); assert(!h.dialog().open);
  h.win.emit('pageshow', {persisted: true}); assert.equal(h.jobs.length, 1); assert(h.ui.open());
  assert(h.find('btn-build-source-project').disabled); h.ui.dispose();
});

test('active export/settings lanes block build, while failed native startup does not', async () => {
  const h = fixture(); h.runtime.settingsPending = true; h.ui.open(); h.pick();
  assert(h.find('btn-build-source-project').disabled); assert.equal(h.runtime.settings, null);
  h.runtime.settingsPending = false; h.runtime.exportPending = true; h.dialog().emit('focusin'); assert(h.find('btn-build-source-project').disabled);
  h.runtime.exportPending = false; h.dialog().emit('focusin'); h.click('btn-build-source-project');
  assert.equal(h.jobs.length, 1); await h.complete(); assert.match(h.status(), /Workspace ready/); h.ui.dispose();
});

test('host composition/readiness failures block opening without reading editor source', () => {
  const h = fixture(); h.ready = false; assert.equal(h.ui.open(), false);
  h.ready = true; h.editor.emit('compositionstart'); assert.equal(h.ui.open(), false);
  h.editor.emit('compositionend'); assert.equal(h.ui.open(), true); h.ui.dispose();
});

test('diagnostics are bounded and text-only, not a claim of rendering qualification', async () => {
  const h = fixture(); h.start(); const diagnostics = Array.from({length: 30}, () => '<img src=x> warning');
  await h.complete(0, {...result(), diagnostics, diagnosticCount: 30});
  const list = h.find('fmd-project-open-diagnostics'); assert.equal(list.children.length, 20);
  assert.equal(list.children[0].textContent, diagnostics[0]); assert.match(h.status(), /showing first 20/); h.ui.dispose();
});

test('a missing dialog API has a visible error outside the unopened dialog', () => {
  const h = fixture(); h.beforeShow = () => { throw Error('dialog unavailable'); };
  assert.equal(h.ui.open(), false); assert.match(h.find('fmd-project-open-notice').textContent, /dialog unavailable/); h.ui.dispose();
});

test('deferred installation and disposal are idempotent and remove all owned controls', () => {
  const h = fixture({loading: true}); assert.equal(h.ui.open(), false);
  h.doc.readyState = 'complete'; h.doc.emit('DOMContentLoaded'); h.doc.emit('DOMContentLoaded');
  assert.equal(h.header.children.filter(node => node.id === 'btn-open-source-project').length, 1);
  h.ui.dispose(); h.ui.dispose(); assert.equal(h.find('btn-open-source-project'), null); assert.equal(h.find('fmd-open-source-project'), null);
  assert.equal(h.observers[0].target, null); assert.equal(h.ui.open(), false);
});

test('dispose before DOM readiness never installs or starts work', () => {
  const h = fixture({loading: true}); h.ui.dispose(); h.doc.emit('DOMContentLoaded');
  assert.equal(h.find('btn-open-source-project'), null); assert.equal(h.jobs.length, 0);
});

test('serialized transient controls are replaced, and the old file selection is not restored', () => {
  const h = fixture(); h.ui.open(); h.pick();
  // Simulate a reparse: inert nodes remain, but the prior instance is disposed.
  const oldDialog = h.dialog(); h.ui.dispose(); h.doc.body.appendChild(oldDialog);
  h.ui = h.install(); assert.equal(h.doc.body.children.filter(node => node.id === 'fmd-open-source-project').length, 1);
  assert.equal(h.input().files.length, 0); assert(!h.dialog().open); assert.equal(h.jobs.length, 0); h.ui.dispose();
});

// A file input's cancel event bubbles when the user cancels its picker or
// reselects the same file; it is not an Escape request on the surrounding dialog.
test('file-picker cancellation does not dismiss the dialog or retire prepared output', async () => {
  const h = fixture(); h.start(); await h.complete();
  h.dialog().emit('cancel', {target: h.input(), preventDefault() { throw Error('not a dialog cancellation'); }});
  assert(h.dialog().open); assert.match(h.status(), /Workspace ready/);
  h.click('btn-download-opened-project'); assert.equal(h.downloads.length, 1); h.ui.dispose();
});
