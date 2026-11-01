// Execute the shipped image-insertion controller with explicit DOM, FileReader
// and decoder contracts. Real Chromium races live in the browser companion.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
import {deflateSync} from 'node:zlib';

const production = readFileSync(new URL('../src/interactive_import.js', import.meta.url), 'utf8');
const tick = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
};
function chunk(tag, data) {
  const bytes = Buffer.alloc(data.length + 12);
  bytes.writeUInt32BE(data.length); bytes.write(tag, 4); data.copy(bytes, 8);
  return bytes; // The decoder is explicit; real-browser fixtures carry CRCs.
}
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(2); ihdr.writeUInt32BE(1, 4); ihdr[8] = 8; ihdr[9] = 6;
const png = Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(Buffer.from([0, 255, 0, 0, 255, 0, 255, 0, 255]))), chunk('IEND', Buffer.alloc(0))]);
const buffer = () => Uint8Array.from(png).buffer;
function file(name = 'image.png', gate = null) {
  return {name, size: png.length, reads: 0, arrayBuffer() { this.reads++; return gate?.promise ?? Promise.resolve(buffer()); }};
}

function fixture({source = 'Original draft', decode = null, native = true} = {}) {
  const elements = new Map(), readers = [], images = [], timers = new Map();
  let serial = 0, staged = 0, commits = 0, rollbacks = 0, resources = [], commitHook = null;
  class Element extends EventTarget {
    constructor(id = '') {
      super(); this.id = id; this.value = ''; this.textContent = ''; this.hidden = false;
      this.files = []; this.style = {}; this.isConnected = true; this.selectionStart = this.selectionEnd = 0;
      this.observers = []; this.classList = {contains: () => false};
      for (const key of ['disabled', 'readOnly']) {
        let value = false;
        Object.defineProperty(this, key, {get: () => value, set: next => {
          if (value !== next) { value = next; for (const notify of this.observers) queueMicrotask(notify); }
        }});
      }
      if (id) elements.set(id, this);
    }
    focus() {}
    click() { this.dispatchEvent(new Event('click')); }
    setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
    setRangeText(text, start, end) {
      this.value = this.value.slice(0, start) + text + this.value.slice(end);
      this.setSelectionRange(start + text.length, start + text.length);
    }
    insertBefore(node) { node.parentNode = this; elements.set(node.id, node); }
  }
  const editor = new Element('fmd-editor'), button = new Element('btn-insert-image');
  const picker = new Element('fmd-image-picker'), status = new Element('fmd-save-status');
  const app = new Element('fmd-app-body'), header = new Element('header');
  button.parentNode = picker.parentNode = header;
  editor.value = source; editor.setSelectionRange(source.length, source.length);
  const win = new EventTarget();
  const document = {
    defaultView: win,
    querySelector(selector) {
      const id = [...selector.matchAll(/#([\w-]+)/g)].at(-1)?.[1];
      if (id) return elements.get(id) ?? null;
      return selector === 'body > .fmd-app-header' ? header : null;
    },
    createElement: () => new Element(),
  };
  class Reader {
    readyState = 0; result = null; aborts = 0;
    constructor() { readers.push(this); }
    readAsArrayBuffer(selected) {
      this.readyState = 1;
      // Keep the captured callbacks to exercise queued, late completion even
      // after abort clears the reader's handlers. The real owner must retire it.
      const loaded = this.onload, failed = this.onerror;
      Promise.resolve(selected.arrayBuffer()).then(bytes => {
        this.result = bytes; this.readyState = 2; loaded?.();
      }, () => { this.readyState = 2; failed?.(); });
    }
    abort() { this.aborts++; this.readyState = 2; this.onabort?.(); }
  }
  class Image {
    naturalWidth = 2; naturalHeight = 1; src = '';
    constructor() { images.push(this); }
    decode() { return decode?.(images.length) ?? Promise.resolve(); }
    removeAttribute(name) { if (name === 'src') this.src = ''; }
  }
  if (native) Object.defineProperty(win, '__fmdNativeRuntime', {value: {
    version: 1, render() {}, pdf() {}, stageImages(assets) {
      staged++; const before = resources;
      return {commit() { resources = [...resources, ...assets]; commits++; commitHook?.(); },
        rollback() { resources = before; rollbacks++; }};
    },
  }});
  const context = vm.createContext({document, window: win, MutationObserver: class {
    constructor(notify) { this.notify = notify; }
    observe(target) { target.observers.push(this.notify); }
  }, FileReader: Reader, Image, Event, btoa, Uint8Array, ArrayBuffer, TextEncoder,
  crypto: {getRandomValues(bytes) { bytes.fill(++serial); return bytes; }},
  setTimeout(fn, delay) { const id = ++serial; timers.set(id, {fn, delay}); return id; },
  clearTimeout(id) { timers.delete(id); }});
  vm.runInContext(production, context);
  return {editor, button, picker, status, readers, images, win,
    get staged() { return staged; }, get commits() { return commits; },
    get rollbacks() { return rollbacks; }, get resources() { return resources; },
    set commitHook(value) { commitHook = value; },
    select(files) { picker.files = files; picker.dispatchEvent(new Event('change')); },
    edit(text) { editor.value = text; editor.dispatchEvent(new Event('input')); },
    compose(type) { editor.dispatchEvent(new Event(type)); },
    cancel() { const control = elements.get('btn-cancel-image-import'); assert.ok(control, 'Image import must expose cancellation'); control.click(); },
    expire() { const entry = [...timers.values()].find(timer => timer.delay === 10000); assert.ok(entry); entry.fn(); },
    beforeHandoff(callback) {
      // Deliver an event after the actual helper returns verified bytes but
      // before the caller receives them. Only promise delivery changes here.
      context.importHandoff = callback;
      vm.runInContext(`{ const read = fmdReadImageImport;
        fmdReadImageImport = (...args) => read(...args).then(images => { importHandoff(); return images; });
      }`, context);
    },
  };
}

test('a valid native image batch commits once and inserts only at the captured selection', async () => {
  const f = fixture({source: 'Before selection after'}); let events = 0;
  f.editor.addEventListener('input', () => events++);
  f.editor.setSelectionRange(7, 16);
  f.select([file('one.png'), file('two.png')]); await tick();
  assert.equal(f.staged, 1); assert.equal(f.commits, 1); assert.equal(f.resources.length, 2);
  assert.match(f.editor.value, /^Before \n\n!\[one\.png\]/);
  assert.match(f.editor.value, /!\[two\.png\].*\n\n after$/s);
  assert.equal(events, 1); assert.equal(f.button.disabled, false);
  assert.match(f.status.textContent, /^Inserted 2 images/);
});

for (const access of ['readOnly', 'disabled', 'isConnected']) {
  test(`${access} denies initial admission and a pending image read without staging assets`, async () => {
    const blocked = access === 'isConnected' ? false : true;
    const initial = fixture(), chosen = file(); initial.editor[access] = blocked;
    initial.select([chosen]); await tick();
    assert.equal(chosen.reads, 0); assert.equal(initial.staged, 0);
    assert.equal(initial.editor.value, 'Original draft');
    const gate = deferred(), f = fixture(); f.select([file('slow.png', gate)]);
    f.editor[access] = blocked; await tick();
    gate.resolve(buffer()); await tick();
    assert.equal(f.editor.value, 'Original draft'); assert.equal(f.staged, 0);
    if (access !== 'isConnected') assert.equal(f.readers[0]?.aborts ?? 0, 1);
  });
}

test('composition without any input event aborts a pending read and requires a fresh import', async () => {
  const gate = deferred(), f = fixture(); f.select([file('slow.png', gate)]);
  f.compose('compositionstart'); await tick();
  assert.equal(f.readers[0]?.aborts ?? 0, 1); assert.equal(f.button.disabled, true);
  const refused = file('during.png'); f.select([refused]); await tick(); assert.equal(refused.reads, 0);
  f.compose('compositionend'); f.select([file('fresh.png')]); await tick();
  const current = f.editor.value, message = f.status.textContent;
  gate.resolve(buffer()); await tick();
  assert.equal(f.editor.value, current); assert.equal(f.status.textContent, message);
  assert.equal(f.commits, 1); assert.match(current, /fresh\.png/); assert.doesNotMatch(current, /slow\.png/);
});

test('source edit and undo retire the old read immediately and leave retries independent', async () => {
  const gate = deferred(), f = fixture(); f.select([file('old.png', gate)]);
  f.edit('New source'); f.edit('Original draft'); await tick();
  assert.equal(f.readers[0]?.aborts ?? 0, 1); assert.equal(f.button.disabled, false);
  f.select([file('retry.png')]); await tick(); const current = f.editor.value;
  gate.resolve(buffer()); await tick();
  assert.equal(f.editor.value, current); assert.equal(f.commits, 1); assert.match(current, /retry\.png/);
});

test('read-only round trips during decoding revoke the operation even when the bytes and value match', async () => {
  const decoded = deferred(), f = fixture({decode: () => decoded.promise});
  f.select([file()]); await tick(); assert.equal(f.images.length, 1);
  f.editor.readOnly = true; f.editor.readOnly = false; await tick();
  assert.equal(f.images[0].src, ''); assert.equal(f.button.disabled, false);
  decoded.resolve(); await tick();
  assert.equal(f.editor.value, 'Original draft'); assert.equal(f.staged, 0);
});

test('cancellation after final decoding but before the importer continuation cannot stage resources', async () => {
  const decoded = deferred(), f = fixture({decode: () => decoded.promise});
  f.beforeHandoff(() => f.edit('New source'));
  f.select([file()]); await tick();
  decoded.resolve();
  await tick();
  assert.equal(f.editor.value, 'New source'); assert.equal(f.staged, 0);
  assert.equal(f.commits, 0); assert.equal(f.button.disabled, false);
});

test('page suspension aborts physical reads, and a late callback cannot overwrite the resumed job', async () => {
  const old = deferred(), next = deferred(), f = fixture();
  f.select([file('old.png', old)]); f.win.dispatchEvent(new Event('pagehide')); await tick();
  assert.equal(f.readers[0]?.aborts ?? 0, 1); assert.equal(f.button.disabled, true);
  f.win.dispatchEvent(new Event('pageshow')); f.select([file('new.png', next)]);
  const message = f.status.textContent; old.resolve(buffer()); await tick();
  assert.equal(f.button.disabled, true); assert.equal(f.status.textContent, message);
  assert.equal(f.staged, 0); assert.equal(f.editor.value, 'Original draft');
  next.resolve(buffer()); await tick(); assert.equal(f.commits, 1); assert.match(f.editor.value, /new\.png/);
});

for (const phase of ['reading', 'decoding']) {
  test(`explicit cancellation while ${phase} releases the importer without admitting late bytes`, async () => {
    const gate = deferred(), f = fixture({decode: phase === 'decoding' ? index => index === 1 ? gate.promise : undefined : null});
    f.select([file('old.png', phase === 'reading' ? gate : null)]); await tick();
    f.cancel(); await tick(); assert.equal(f.button.disabled, false);
    if (phase === 'reading') assert.equal(f.readers[0]?.aborts ?? 0, 1);
    else assert.equal(f.images[0].src, '');
    f.select([file('new.png')]); await tick(); const source = f.editor.value, message = f.status.textContent;
    gate.resolve(buffer()); await tick();
    assert.equal(f.editor.value, source); assert.equal(f.status.textContent, message);
    assert.equal(f.commits, 1); assert.match(source, /new\.png/); assert.doesNotMatch(source, /old\.png/);
  });
  test(`${phase} timeout keeps source and resources unchanged and permits a clean retry`, async () => {
    const gate = deferred(), f = fixture({decode: phase === 'decoding' ? index => index === 1 ? gate.promise : undefined : null});
    f.select([file('timed-out.png', phase === 'reading' ? gate : null)]); await tick(); f.expire(); await tick();
    assert.equal(f.editor.value, 'Original draft'); assert.equal(f.staged, 0);
    assert.equal(f.button.disabled, false); assert.match(f.status.textContent, /ten seconds|timed out/);
    if (phase === 'reading') assert.equal(f.readers[0]?.aborts ?? 0, 1);
    f.select([file('retry.png')]); await tick(); gate.resolve(buffer()); await tick();
    assert.equal(f.commits, 1); assert.match(f.editor.value, /retry\.png/);
  });
}

test('picker ownership is retired by composition and suspension without rebasing an old choice', async () => {
  for (const kind of ['composition', 'suspension']) {
    const f = fixture(); f.button.click();
    if (kind === 'composition') { f.compose('compositionstart'); f.compose('compositionend'); }
    else { f.win.dispatchEvent(new Event('pagehide')); f.win.dispatchEvent(new Event('pageshow')); }
    const old = file('old-choice.png'); f.select([old]); await tick();
    assert.equal(old.reads, 0); assert.equal(f.staged, 0);
    f.button.click(); f.select([file('fresh-choice.png')]); await tick();
    assert.equal(f.commits, 1); assert.match(f.editor.value, /fresh-choice\.png/);
  }
});

test('losing editor ownership during resource commit rolls the unchanged-source transaction back', async () => {
  const f = fixture(); f.commitHook = () => { f.editor.readOnly = true; };
  f.select([file()]); await tick();
  assert.equal(f.commits, 1); assert.equal(f.rollbacks, 1);
  assert.equal(f.resources.length, 0); assert.equal(f.editor.value, 'Original draft');
});

test('lightweight image insertion retains its portable data-URI source contract', async () => {
  const f = fixture({native: false}); f.select([file()]); await tick();
  assert.match(f.editor.value, /!\[image\.png\]\(data:image\/png;base64,/);
  assert.equal(f.staged, 0); assert.match(f.status.textContent, /^Inserted 1 image/);
});
