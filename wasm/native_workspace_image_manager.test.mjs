// Production image-manager UI with explicit DOM/read/decode/worker contracts.
// Native resource transactions are real; Chromium covers actual browser APIs.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';

const production = readFileSync(new URL('../src/interactive_import.js', import.meta.url), 'utf8');
const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8DwHwQBEPgD/U6VwW8AAAAASUVORK5CYII=', 'base64');
const source = '\ufeff![One](figures/new.png)\r\n\r\n![Again](figures/new.png)\r';
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
const file = (gate = null, bytes = png) => ({name: 'local.png', size: bytes.length,
  arrayBuffer: () => gate?.promise ?? Promise.resolve(Uint8Array.from(bytes).buffer)});

function fixture({empty = false, bindings = true, decode = () => Promise.resolve()} = {}) {
  const nodes = new Map(), readers = [], jobs = [];
  let running = null, reads = 0, preflights = 0;
  class Element extends EventTarget {
    constructor(tag = 'div', id = '') {
      super(); this.tag = tag; this.children = []; this.files = []; this.style = {getPropertyValue: () => '16px'};
      this.classList = {contains: () => false}; this.attributes = {}; this.observers = [];
      this.isConnected = true; this.open = false; this.textContent = '';
      let identity = '', value = '';
      Object.defineProperty(this, 'id', {get: () => identity, set: next => { identity = next; if (next) nodes.set(next, this); }});
      Object.defineProperty(this, 'value', {get: () => value, set: next => {
        value = String(next); if (this.type === 'file' && value === '') this.files = [];
      }});
      Object.defineProperty(this, 'selectedIndex', {set: index => { this.value = this.children[index]?.value ?? ''; }});
      for (const key of ['disabled', 'readOnly']) {
        let flag = false;
        Object.defineProperty(this, key, {get: () => flag, set: next => {
          if (flag !== next) { flag = next; for (const notify of this.observers) queueMicrotask(notify); }
        }});
      }
      this.id = id;
    }
    appendChild(node) { node.parentNode = this; this.children.push(node); return node; }
    insertBefore(node, before) {
      node.parentNode = this;
      const index = this.children.indexOf(before); this.children.splice(index < 0 ? this.children.length : index, 0, node);
    }
    replaceChildren(...children) { this.children = []; if (this.tag === 'select') this.value = ''; children.forEach(node => this.appendChild(node)); }
    querySelector(selector) { return nodes.get([...selector.matchAll(/#([\w-]+)/g)].at(-1)?.[1]) ?? null; }
    remove() { nodes.delete(this.id); this.parentNode.children = this.parentNode.children.filter(node => node !== this); }
    setAttribute(key, value) { this.attributes[key] = value; }
    focus() {}
    click() { if (!this.disabled) this.dispatchEvent(new Event('click')); }
    showModal() { this.open = true; }
    close() { this.open = false; this.dispatchEvent(new Event('close')); }
  }
  const body = new Element('body'), header = new Element('header', 'header');
  const editor = new Element('textarea', 'fmd-editor'), preview = new Element('div', 'fmd-content');
  const payload = {version: 1, options: {title: 'Keep settings'}, fonts: [],
    images: empty ? [] : [{destination: 'old.png', bytes: png.toString('base64')}]};
  const data = new Element('script', 'fmd-native-runtime'), raw = new Element('script', 'fmd-raw-source');
  data.textContent = JSON.stringify(payload); raw.textContent = JSON.stringify(source);
  editor.value = source.replace(/\r\n?/g, '\n'); preview.textContent = 'Original preview';
  const saved = new Element('button', 'btn-save-html'); header.appendChild(saved); body.appendChild(header);
  new Element('span', 'fmd-save-status');
  const renderer = createNativeWorkspaceRenderer({
    renderHtmlConfiguredAdvanced() { throw Error('Unexpected main-thread rendering'); },
    renderPdfConfiguredMulti() { throw Error('Unexpected main-thread rendering'); },
  }, payload);
  const win = new EventTarget();
  class Reader {
    readyState = 0; result = null; aborted = 0;
    constructor() { readers.push(this); }
    readAsArrayBuffer(selected) {
      reads++; this.readyState = 1;
      const loaded = this.onload, failed = this.onerror;
      selected.arrayBuffer().then(value => { this.result = value; this.readyState = 2; loaded?.(); },
        () => { this.readyState = 2; failed?.(); });
    }
    abort() { this.aborted++; this.readyState = 2; this.onabort?.(); }
  }
  Object.assign(win, {FileReader: Reader, Image: class {
    naturalWidth = 2; naturalHeight = 1; src = '';
    decode() { return decode(); } removeAttribute() { this.src = ''; }
  }, setTimeout, clearTimeout, confirm: () => true, MutationObserver: class {
    constructor(notify) { this.notify = notify; } observe(target) { target.observers.push(this.notify); }
  }});
  function begin(attach, patches, markdown, target, display, current) {
    preflights++;
    const transaction = attach ? renderer.stageImageBindings(patches) : renderer.stageImageChanges(patches);
    const gate = deferred();
    const job = {attach, patches, markdown, display, cancelled: false,
      complete() {
        if (job.cancelled || !current()) { gate.reject(Error('Stale image operation')); return; }
        transaction.commit(() => { data.textContent = JSON.stringify({...payload, images: transaction.images}); target.textContent = 'Approved preview'; });
        running = null; win.dispatchEvent(new Event('fmd-native-images-changed')); gate.resolve(renderer.imageAssets);
      },
      fail() { running = null; gate.reject(Error('Native image validation failed')); },
    };
    jobs.push(job); running = job;
    return {promise: gate.promise, cancel() {
      if (running !== job) return false;
      job.cancelled = true; running = null; gate.reject(Error('Image operation cancelled')); return true;
    }};
  }
  const native = {version: 1, imageMode: 'worker', get imageAssets() { return renderer.imageAssets; },
    get documentRevision() { return renderer.revision; }, get settingsPending() { return running !== null; },
    beginImageChanges: (...args) => begin(false, ...args)};
  if (bindings) native.beginImageBindings = (...args) => begin(true, ...args);
  Object.defineProperty(win, '__fmdNativeRuntime', {value: native});
  const doc = {defaultView: win, body, documentElement: new Element('html'),
    createElement: tag => new Element(tag), querySelector(selector) {
      if (selector === 'body > .fmd-app-header') return header;
      return nodes.get([...selector.matchAll(/#([\w-]+)/g)].at(-1)?.[1]) ?? null;
    }};
  const install = vm.runInNewContext(production + '\nfmdInstallImageManager;', {Uint8Array, ArrayBuffer, btoa});
  install(doc);
  const node = id => nodes.get(id);
  node('btn-document-images').click();
  return {editor, raw, data, preview, payload, renderer, readers, jobs, win, doc, node,
    get reads() { return reads; }, get preflights() { return preflights; },
    choose(key = 'figures/new.png', selected = file()) {
      assert.ok(node('fmd-image-binding-destination'), 'Manager must expose explicit image attachment');
      node('fmd-image-binding-destination').value = key;
      node('fmd-image-binding-destination').dispatchEvent(new Event('input'));
      node('fmd-image-binding-file').files = [selected];
      node('fmd-image-binding-file').dispatchEvent(new Event('change'));
    },
    attach() { node('btn-attach-document-image').click(); },
    edit(value) { editor.value = value; editor.dispatchEvent(new Event('input')); },
    changeKey(value) { node('fmd-image-binding-destination').value = value; node('fmd-image-binding-destination').dispatchEvent(new Event('input')); },
    dirty() { const event = new Event('beforeunload', {cancelable: true}); win.dispatchEvent(event); return event.defaultPrevented; },
  };
}

test('attachment publishes exact explicit keys and original bytes without rewriting source, settings or existing bindings', async () => {
  const f = fixture(), saved = f.data.textContent, view = f.editor.value, raw = f.raw.textContent;
  f.choose(' figures/日本語.png '); f.attach(); await tick();
  assert.equal(f.data.textContent, saved); assert.equal(f.preview.textContent, 'Original preview');
  assert.equal(f.jobs.length, 1); assert.equal(f.jobs[0].attach, true);
  assert.equal(f.jobs[0].patches[0].destination, 'figures/日本語.png');
  assert.equal(f.jobs[0].markdown, view); assert.equal(f.dirty(), false);
  f.jobs[0].complete(); await tick();
  const result = JSON.parse(f.data.textContent);
  assert.equal(result.images[1].destination, 'figures/日本語.png');
  assert.equal(result.images[1].bytes, png.toString('base64'));
  assert.deepEqual(result.images[0], JSON.parse(saved).images[0]); assert.deepEqual(result.options, JSON.parse(saved).options);
  assert.equal(f.editor.value, view); assert.equal(f.raw.textContent, raw); assert.equal(f.dirty(), true);
  assert.equal(f.node('fmd-image-selection').value, 'figures/日本語.png');
  assert.equal(f.node('fmd-image-binding-destination').value, ''); assert.equal(f.node('fmd-image-binding-file').files.length, 0);
});

test('an empty workspace can attach its first resource and then replace it through the existing action', async () => {
  const f = fixture({empty: true}); f.choose(); f.attach(); await tick(); f.jobs[0].complete(); await tick();
  assert.equal(f.renderer.imageAssets.length, 1);
  f.node('fmd-image-replacement').files = [file()]; f.node('fmd-image-replacement').dispatchEvent(new Event('change'));
  f.node('btn-replace-document-image').click(); await tick();
  assert.equal(f.jobs[1].attach, false); f.jobs[1].complete(); await tick();
  assert.equal(f.renderer.imageAssets.length, 1);
});

test('duplicate, invalid Unicode, controls and excessive UTF-8 destination lengths are refused before reading', async () => {
  for (const key of ['old.png', ' old.png ', 'bad\0name', '\ud800', 'x'.repeat(8193), '界'.repeat(2731)]) {
    const f = fixture(), before = f.data.textContent; f.choose(key); f.attach(); await tick();
    assert.equal(f.reads, 0, key); assert.equal(f.preflights, 0); assert.equal(f.data.textContent, before);
    assert.match(f.node('fmd-images-status').textContent, /^Image not changed:/);
  }
});

test('invalid image metadata and decoder failures cannot reach resource preflight', async () => {
  for (const [selected, decode] of [[file(null, Buffer.from('<svg/>')), () => Promise.resolve()],
    [file(), () => Promise.reject(Error('Decode refused'))]]) {
    const f = fixture({decode});
    const before = f.data.textContent;
    f.choose('new.png', selected); f.attach(); await tick();
    assert.equal(f.preflights, 0); assert.equal(f.renderer.imageAssets.length, 1);
    assert.equal(f.data.textContent, before);
  }
});

test('native validation failure leaves attachment retryable without a partial resource or preview change', async () => {
  const f = fixture(), before = f.data.textContent; f.choose(); f.attach(); await tick(); f.jobs[0].fail(); await tick();
  assert.equal(f.data.textContent, before); assert.equal(f.preview.textContent, 'Original preview'); assert.equal(f.dirty(), false);
  assert.equal(f.node('btn-attach-document-image').disabled, false);
  f.attach(); await tick(); f.jobs[1].complete(); await tick(); assert.equal(f.renderer.imageAssets.length, 2);
});

test('canceling a file read physically aborts it and fences its late callback from a new attachment', async () => {
  const gate = deferred(), f = fixture(); f.choose('old-choice.png', file(gate)); f.attach(); await tick();
  f.node('btn-cancel-image-change').click(); await tick(); assert.equal(f.readers[0].aborted, 1);
  f.choose('fresh.png'); f.attach(); await tick(); const currentStatus = f.node('fmd-images-status').textContent;
  gate.resolve(Uint8Array.from(png).buffer); await tick();
  assert.equal(f.jobs.length, 1); assert.equal(f.node('fmd-images-status').textContent, currentStatus);
  f.jobs[0].complete(); await tick(); assert.equal(f.renderer.imageAssets[1].destination, 'fresh.png');
});

test('destination edits and undo retire pending reads rather than rebasing them onto identical text', async () => {
  const gate = deferred(), f = fixture(); f.choose('first.png', file(gate)); f.attach(); await tick();
  f.changeKey('different.png'); f.changeKey('first.png'); gate.resolve(Uint8Array.from(png).buffer); await tick();
  assert.equal(f.preflights, 0); assert.equal(f.readers[0].aborted, 1);
  assert.equal(f.renderer.imageAssets.length, 1);
});

for (const change of ['source', 'destination', 'readOnly', 'disabled', 'composition', 'pagehide', 'close']) {
  test(change + ' cancels worker publication; an abandoned completion cannot publish into a later attachment', async () => {
    const f = fixture(), before = f.data.textContent; f.choose(); f.attach(); await tick(); const old = f.jobs[0];
    if (change === 'source') f.edit('New source');
    else if (change === 'destination') f.changeKey('revised.png');
    else if (change === 'composition') f.node('fmd-document-images').dispatchEvent(new Event('compositionstart'));
    else if (change === 'pagehide') f.win.dispatchEvent(new Event('pagehide'));
    else if (change === 'close') f.node('btn-close-document-images').click();
    else { f.editor[change] = true; f.editor[change] = false; }
    await tick(); assert.equal(old.cancelled, true); assert.equal(f.data.textContent, before);
    if (change === 'composition') {
      assert.equal(f.node('fmd-image-binding-destination').disabled, false);
      assert.equal(f.node('btn-attach-document-image').disabled, true);
      f.node('fmd-document-images').dispatchEvent(new Event('compositionend'));
    }
    else if (change === 'pagehide') f.win.dispatchEvent(new Event('pageshow'));
    else if (change === 'close') f.node('btn-document-images').click();
    f.choose('fresh.png'); f.attach(); await tick(); old.complete(); await tick();
    assert.equal(f.data.textContent, before); assert.equal(f.node('btn-cancel-image-change').disabled, false);
    f.jobs[1].complete(); await tick(); assert.equal(f.renderer.imageAssets[1].destination, 'fresh.png');
    if (change === 'source') assert.equal(f.editor.value, 'New source');
  });
}

test('resource changes during reading revoke an attachment before native preflight', async () => {
  const gate = deferred(), f = fixture(); f.choose('new.png', file(gate)); f.attach(); await tick();
  f.renderer.stageSettings({title: 'New settings'}).commit();
  gate.resolve(Uint8Array.from(png).buffer); await tick();
  assert.equal(f.preflights, 0); assert.equal(f.renderer.imageAssets.length, 1);
});

test('legacy managers keep replacement and removal without advertising unavailable binding support', () => {
  const f = fixture({bindings: false});
  assert.equal(f.node('btn-attach-document-image'), undefined);
  assert.equal(f.node('fmd-image-binding-destination'), undefined);
  assert.equal(f.node('btn-remove-document-image').disabled, false);
});
