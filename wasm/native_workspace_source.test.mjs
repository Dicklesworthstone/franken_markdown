import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {test} from 'node:test';
import vm from 'node:vm';

// Execute the whole shipped controller. DOM, clocks, FileReader and native
// renderer functions are explicit adapters; File/Blob and UTF-8 codecs are real.
const controller = await readFile(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
const original = '\r\n# Original\r\n\r\n![retained](chart.png)\r\n';
const imported = '\ufeff\r\n# Imported é中😀\rline\n\0</script><script>bad()</script>\r\n';
const tick = () => new Promise(resolve => setImmediate(resolve));

function fixture({source = original, native = true, readerAvailable = true, publishing = null, ready, access} = {}) {
  const downloads = [], objects = new Map(), revoked = [], readers = [], timers = new Map(), confirmations = [], calls = [];
  const observers = new Set();
  let timerId = 0, urlId = 0, autoRead = true, confirm = () => true, previewFailure = null, acceptWrite = null;
  const doc = new EventTarget(), win = new EventTarget();
  function mutation(record) {
    for (const observer of observers) {
      if (!observer.targets.some(([target, options]) => (target === record.target
          || (options.subtree && target.contains(record.target)))
          && (record.type === 'childList' ? options.childList
            : options.attributes && (!options.attributeFilter || options.attributeFilter.includes(record.attributeName))))) continue;
      observer.records.push(record);
      if (!observer.scheduled) {
        observer.scheduled = true;
        queueMicrotask(() => {
          observer.scheduled = false;
          const records = observer.takeRecords();
          if (records.length) observer.callback(records);
        });
      }
    }
  }
  class Observer {
    constructor(callback) { this.callback = callback; this.targets = []; this.records = []; observers.add(this); }
    observe(target, options) { this.targets.push([target, options]); }
    takeRecords() { return this.records.splice(0); }
  }
  class Element extends EventTarget {
    constructor(tag = 'span', id = '') {
      super(); this.tagName = tag.toUpperCase(); this.id = id; this.children = []; this.parentNode = null;
      this.textContent = ''; this.innerHTML = ''; this._value = ''; this.hidden = false; this.isConnected = true;
      for (const key of ['disabled', 'readOnly']) {
        let value = false;
        Object.defineProperty(this, key, {get: () => value, set: next => {
          if (value === Boolean(next)) return;
          value = Boolean(next);
          mutation({type: 'attributes', target: this, attributeName: key.toLowerCase()});
        }});
      }
      this.style = {getPropertyValue: () => '16px', setProperty() {}};
      const classes = new Set();
      this.classList = {add: key => classes.add(key), remove: key => classes.delete(key), contains: key => classes.has(key),
        toggle(key) { if (classes.has(key)) { classes.delete(key); return false; } classes.add(key); return true; }};
      this.selectionStart = 0; this.selectionEnd = 0; this.selectionDirection = 'none'; this.scrollTop = 0;
      this.files = []; this.attributes = {};
    }
    get value() { return this._value; }
    set value(value) {
      if (this.id === 'fmd-editor' && acceptWrite) acceptWrite(value);
      this._value = this.tagName === 'TEXTAREA' ? String(value).replace(/\r\n?/g, '\n') : String(value);
    }
    appendChild(child) {
      if (child.parentNode) child.remove();
      child.parentNode = this; this.children.push(child);
      for (const node of [child, ...child.descendants()]) node.isConnected = this.isConnected;
      mutation({type: 'childList', target: this, addedNodes: [child], removedNodes: []});
      return child;
    }
    insertBefore(child, ref) { child.parentNode = this; this.children.splice(this.children.indexOf(ref), 0, child); }
    replaceChildren(...children) { this.children = []; for (const child of children) this.appendChild(child); }
    remove() {
      this.removed = true;
      const parent = this.parentNode;
      parent?.children.splice(parent.children.indexOf(this), 1); this.parentNode = null;
      for (const node of [this, ...this.descendants()]) node.isConnected = false;
      if (parent) mutation({type: 'childList', target: parent, addedNodes: [], removedNodes: [this]});
    }
    setAttribute(key, value) { this.attributes[key] = value; }
    getAttribute(key) { return Object.hasOwn(this.attributes, key) ? this.attributes[key] : null; }
    removeAttribute(key) { delete this.attributes[key]; }
    querySelector(selector) {
      const id = selector.match(/#([\w-]+)/)?.[1];
      return this.children.flatMap(child => [child, ...child.descendants()]).find(child => child.id === id) ?? null;
    }
    *descendants() { for (const child of this.children) { yield child; yield* child.descendants(); } }
    contains(node) { return this === node || [...this.descendants()].includes(node); }
    focus() { doc.activeElement = this; }
    setSelectionRange(start, end, direction = 'none') { this.selectionStart = start; this.selectionEnd = end; this.selectionDirection = direction; }
    click() {
      if (this.disabled) return;
      if (this.tagName === 'A') downloads.push({blob: objects.get(this.href), filename: this.download});
      else this.dispatchEvent(new Event('click'));
    }
  }
  doc.body = new Element('body'); doc.documentElement = new Element('html'); doc.title = 'Document';
  doc.contains = node => doc.body.contains(node) || doc.documentElement.contains(node);
  const header = doc.body.appendChild(new Element('header', 'header'));
  const toolbar = header.appendChild(new Element('div'));
  for (const id of ['toggle-view', 'zoom-in', 'zoom-out', 'zoom-reset', 'theme-toggle', 'stats-toggle', 'save-markdown', 'save-html', 'export-pdf']) {
    toolbar.appendChild(new Element('button', 'btn-' + id));
  }
  const body = doc.body.appendChild(new Element('div', 'fmd-app-body')); body.classList.add('view-split');
  const editor = body.appendChild(new Element('textarea', 'fmd-editor'));
  if (access) editor[access] = true;
  const preview = body.appendChild(new Element('div', 'fmd-content')); preview.innerHTML = 'ORIGINAL RENDER';
  const stats = doc.body.appendChild(new Element('div', 'stats-drawer'));
  for (const id of ['stat-words', 'stat-chars', 'stat-read-time', 'stat-readability', 'btn-stats-close']) stats.appendChild(new Element('span', id));
  for (const id of ['source-line-count', 'fmd-save-status', 'view-mode-icon', 'view-mode-label']) body.appendChild(new Element('span', id));
  const data = {textContent: JSON.stringify(source)}, assets = {textContent: JSON.stringify([['chart.png', 'data:image/png;base64,AQID']])};
  doc.getElementById = id => [doc.body, ...doc.body.descendants()].find(element => element.id === id) ?? null;
  doc.createElement = tag => new Element(tag);
  doc.querySelector = selector => selector.includes('fmd-raw-source') ? data
    : selector.includes('fmd-image-assets') ? assets : selector.includes('.fmd-app-header') ? header
      : selector.includes('stats-drawer') ? stats : null;
  let clone = null;
  doc.documentElement.cloneNode = () => {
    const nodes = new Map();
    clone = {nodes, attributes: {}, removeAttribute(key) { delete this.attributes[key]; }, querySelector(selector) {
      if (!nodes.has(selector)) nodes.set(selector, new Element());
      return nodes.get(selector);
    }, get outerHTML() { return JSON.stringify([...nodes].map(([selector, element]) => ({selector, text: element.textContent, removed: !!element.removed}))); }};
    return clone;
  };
  win.confirm = text => { confirmations.push(text); return confirm(text); };
  win.print = () => { calls.push(['print']); };
  if (native) Object.defineProperty(win, '__fmdNativeRuntime', {value: {
    version: 1, diagnostics: [], ...(publishing ? {html: publishing, ready} : {}),
    render(source, _preview, display) { if (previewFailure) throw previewFailure; calls.push(['render', source, display]); },
    pdf(source) { calls.push(['pdf', source]); return new TextEncoder().encode('%PDF-ADAPTER\n' + source); },
  }});
  class Reader {
    constructor() { this.readyState = 0; this.aborts = 0; readers.push(this); }
    readAsArrayBuffer(file) {
      this.readyState = 1; this.file = file;
      if (autoRead) file.arrayBuffer().then(buffer => { if (this.readyState === 1) this.complete(buffer); });
    }
    complete(buffer) { this.result = buffer; this.readyState = 2; this.onload?.(); }
    abort() { this.readyState = 2; this.aborts++; this.onabort?.(); }
  }
  const context = {document: doc, window: win, console, Event, TextDecoder, TextEncoder, ArrayBuffer, Blob, MutationObserver: Observer,
    FileReader: readerAvailable ? Reader : undefined,
    URL: {createObjectURL(blob) { const url = 'blob:adapter/' + ++urlId; objects.set(url, blob); return url; },
      revokeObjectURL(url) { revoked.push(url); objects.delete(url); }},
    setTimeout(fn, delay) { const id = ++timerId; timers.set(id, {fn, delay}); return id; }, clearTimeout(id) { timers.delete(id); },
    parseMarkdownClient(source, images) { if (previewFailure) throw previewFailure; calls.push(['parse', source, [...images]]); return 'rendered:' + source; }};
  vm.runInNewContext(controller, context, {filename: 'interactive_controller.js'});
  const el = id => doc.getElementById(id);
  const api = {
    doc, win, editor, preview, calls, readers, confirmations, downloads, timers, revoked, el,
    get clone() { return clone; },
    get status() { return el('fmd-save-status').textContent; },
    get reader() { return readers.at(-1); },
    setAutoRead(value) { autoRead = value; }, setConfirm(fn) { confirm = fn; },
    failPreview(error) { previewFailure = error; }, rejectWrites(fn) { acceptWrite = fn; },
    begin() { el('btn-open-markdown').click(); },
    select(files) { const picker = el('fmd-source-picker'); picker.files = files; picker.dispatchEvent(new Event('change')); },
    async open(file) { api.begin(); api.select([file]); await tick(); await tick(); },
    type(text, notify = true) { editor.value = text; if (notify) editor.dispatchEvent(new Event('input')); },
    fire(delay) { for (const [id, timer] of [...timers]) if (timer.delay === delay) { timers.delete(id); timer.fn(); } },
    dirty() { const event = new Event('beforeunload', {cancelable: true}); win.dispatchEvent(event); return event.defaultPrevented; },
    async markdown() { el('btn-save-markdown').click(); return Buffer.from(await downloads.at(-1).blob.arrayBuffer()); },
  };
  return api;
}
const file = (text = imported, name = 'chapter.markdown') => new File([text], name);

test('real File bytes preserve UTF-8 BOM, mixed newlines, NUL, Unicode and script-looking text', async () => {
  for (const native of [true, false]) {
    const f = fixture({native}); let inputs = 0; f.editor.addEventListener('input', () => inputs++);
    await f.open(file());
    assert.equal(f.editor.value, imported.replace(/\r\n?/g, '\n'));
    assert.deepEqual(await f.markdown(), Buffer.from(imported)); assert.equal(inputs, 1);
    assert.equal(f.dirty(), true); assert.equal(f.confirmations.length, 1);
    assert.match(f.confirmations[0], /retained/); assert.match(f.confirmations[0], /No other files/);
    assert.equal(f.el('btn-undo-source-open').disabled, false);
    f.fire(150); assert.equal(f.calls.at(-1)[1], imported);
    if (!native) assert.deepEqual(JSON.parse(JSON.stringify(f.calls.at(-1)[2])), [['chart.png', 'data:image/png;base64,AQID']]);
  }
});

test('native PDF export sees imported exact source before preview debounce', async () => {
  const f = fixture(); await f.open(file()); f.el('btn-export-pdf').click();
  assert.deepEqual(f.calls, [['pdf', imported]]); assert.equal(f.downloads.at(-1).filename, 'Document.pdf');
});

test('Undo restores source bytes, selection and clean baseline rather than normalized text', async () => {
  const f = fixture(); f.editor.setSelectionRange(4, 9, 'backward'); f.editor.scrollTop = 120;
  await f.open(file()); f.el('btn-undo-source-open').click();
  assert.deepEqual(await f.markdown(), Buffer.from(original)); assert.equal(f.dirty(), false);
  assert.deepEqual([f.editor.selectionStart, f.editor.selectionEnd, f.editor.selectionDirection, f.editor.scrollTop], [4, 9, 'backward', 120]);
  assert.equal(f.el('btn-undo-source-open').disabled, true);
});

test('replacement tracks byte changes even when both textarea views are identical', async () => {
  const f = fixture({source: 'same\ntext'}); await f.open(file('same\r\ntext'));
  assert.equal(f.dirty(), true); assert.deepEqual(await f.markdown(), Buffer.from('same\r\ntext'));
  f.el('btn-undo-source-open').click(); assert.equal(f.dirty(), false);
});

test('unchanged source does not prompt or destroy the existing replacement undo', async () => {
  const f = fixture(); await f.open(file()); await f.open(file());
  assert.equal(f.confirmations.length, 1); assert.match(f.status, /already matches/);
  f.el('btn-undo-source-open').click(); assert.deepEqual(await f.markdown(), Buffer.from(original));
});

test('successive opens retain only the preceding source; edits retire replacement undo', async () => {
  const f = fixture(); await f.open(file()); await f.open(file('second'));
  f.el('btn-undo-source-open').click(); assert.deepEqual(await f.markdown(), Buffer.from(imported));
  assert.equal(f.el('btn-undo-source-open').disabled, true);
  await f.open(file('third')); f.type('new typing'); f.el('btn-undo-source-open').click();
  assert.equal(f.editor.value, 'new typing'); assert.equal(f.el('btn-undo-source-open').disabled, true);
});

test('programmatic edits without input cannot be overwritten by Undo', async () => {
  const f = fixture(); await f.open(file()); f.type('undispatched', false); f.el('btn-undo-source-open').click();
  assert.equal(f.editor.value, 'undispatched'); assert.match(f.status, /discard newer edits/);
});

test('empty file replaces source only after confirmation and can be undone', async () => {
  const f = fixture(); await f.open(file('', 'empty.TXT'));
  assert.equal(f.confirmations.length, 1); assert.equal(f.editor.value, '');
  assert.equal((await f.markdown()).length, 0); f.el('btn-undo-source-open').click();
  assert.deepEqual(await f.markdown(), Buffer.from(original));
});

test('reject invalid UTF-8 and UTF-16 without modifying source or asking to discard it', async () => {
  for (const bytes of [[0xc3, 0x28], [0xed, 0xa0, 0x80], [0xff, 0xfe, 0x61, 0], [0xef, 0xbb]]) {
    const f = fixture(); await f.open(new File([Uint8Array.from(bytes)], 'bad.md'));
    assert.match(f.status, /Source not opened/); assert.equal(f.confirmations.length, 0);
    assert.equal(f.dirty(), false); assert.deepEqual(await f.markdown(), Buffer.from(original));
  }
});

test('wrong extension, excessive names, multiple files and oversize files are rejected before I/O', async () => {
  for (const value of [file('x', 'script.html'), file('x', 'no-extension'), file('x', 'x'.repeat(1024) + '.md'),
    {name: 'huge.md', size: 32 * 1024 * 1024 + 1}, {name: 'bad.md', size: -1}, {name: 'bad.md', size: NaN}]) {
    const f = fixture(); await f.open(value); assert.equal(f.readers.length, 0); assert.match(f.status, /Source not opened/);
  }
  const f = fixture(); f.begin(); f.select([file(), file()]); await tick();
  assert.match(f.status, /one Markdown source file/); assert.equal(f.readers.length, 0);
});

test('old source admission rejects malformed editor Unicode before staging a replacement', async () => {
  const f = fixture(); f.type('\ud800'); await f.open(file());
  assert.match(f.status, /invalid Unicode/); assert.equal(f.editor.value, '\ud800'); assert.equal(f.readers.length, 0);
});

test('picker cancellation and declined confirmation leave exact source and undo untouched', async () => {
  const f = fixture(); await f.open(file()); f.begin(); f.el('fmd-source-picker').dispatchEvent(new Event('cancel'));
  assert.equal(f.el('btn-open-markdown').disabled, false); assert.equal(f.el('btn-undo-source-open').disabled, false);
  f.setConfirm(() => false); await f.open(file('declined'));
  assert.deepEqual(await f.markdown(), Buffer.from(imported)); f.el('btn-undo-source-open').click();
  assert.deepEqual(await f.markdown(), Buffer.from(original));
});

test('only one picker/read is active, and read cancellation aborts underlying I/O', async () => {
  const f = fixture(); f.setAutoRead(false); f.begin(); f.select([file()]); f.begin(); f.select([file('overlap')]);
  assert.equal(f.readers.length, 1); const late = f.reader.onload; const reader = f.reader;
  f.type('newer edit'); await tick(); assert.equal(reader.aborts, 1); assert.equal(f.el('btn-open-markdown').disabled, false);
  reader.result = await file().arrayBuffer(); late(); await tick();
  assert.equal(f.editor.value, 'newer edit'); assert.equal(f.confirmations.length, 0);
});

test('unnotified edits during file reading are detected by exact snapshot checks', async () => {
  const f = fixture(); f.setAutoRead(false); f.begin(); f.select([file()]);
  f.type('undispatched', false); f.reader.complete(await file().arrayBuffer()); await tick();
  assert.match(f.status, /Source changed/); assert.equal(f.editor.value, 'undispatched'); assert.equal(f.confirmations.length, 0);
});

test('edit and undo back to the same view still invalidate a pending read', async () => {
  const f = fixture(); f.setAutoRead(false); f.begin(); f.select([file()]);
  f.type('temporary'); f.type(original); await tick();
  assert.equal(f.reader.aborts, 1); assert.equal(f.confirmations.length, 0); assert.deepEqual(await f.markdown(), Buffer.from(original));
});

test('confirmation-time edits with and without input never get replaced', async () => {
  for (const notify of [true, false]) {
    const f = fixture(); f.setConfirm(() => { f.type('during confirmation', notify); return true; });
    await f.open(file()); assert.equal(f.editor.value, 'during confirmation'); assert.equal(f.el('btn-undo-source-open').disabled, true);
  }
});

function revokeAccess(f, kind) {
  if (kind === 'readOnly' || kind === 'disabled') {
    f.editor[kind] = true;
    return () => { f.editor[kind] = false; };
  }
  const node = kind === 'ancestor' ? f.editor.parentNode : f.editor;
  const parent = node.parentNode;
  node.remove();
  return () => { parent.appendChild(node); };
}

for (const access of ['readOnly', 'disabled']) {
  test(access + ' source cannot open a file, including before any observer callback', async () => {
    const f = fixture({access});
    assert.equal(f.el('btn-open-markdown').disabled, true);
    assert.equal(f.el('fmd-source-picker').disabled, true);
    f.el('btn-open-markdown').dispatchEvent(new Event('click'));
    f.select([file()]); await tick();
    assert.equal(f.readers.length, 0); assert.equal(f.confirmations.length, 0);
    assert.deepEqual(await f.markdown(), Buffer.from(original));
    f.editor[access] = false; await tick();
    await f.open(file());
    assert.deepEqual(await f.markdown(), Buffer.from(imported));
  });

  test(access + ' blocks Undo immediately and retires that history after access returns', async () => {
    const f = fixture(); await f.open(file());
    f.editor[access] = true;
    f.el('btn-undo-source-open').dispatchEvent(new Event('click'));
    assert.deepEqual(await f.markdown(), Buffer.from(imported));
    assert.equal(f.el('btn-undo-source-open').disabled, true);
    f.editor[access] = false; await tick();
    assert.equal(f.el('btn-open-markdown').disabled, false);
    assert.equal(f.el('btn-undo-source-open').disabled, true);
    await f.open(file('new replacement'));
    f.el('btn-undo-source-open').click();
    assert.deepEqual(await f.markdown(), Buffer.from(imported));
  });

  test(access + ' round trip inside confirmation cannot publish the selected source', async () => {
    const f = fixture();
    f.setConfirm(() => { const restore = revokeAccess(f, access); restore(); return true; });
    await f.open(file());
    assert.equal(f.confirmations.length, 1);
    assert.deepEqual(await f.markdown(), Buffer.from(original));
    assert.equal(f.el('btn-undo-source-open').disabled, true);
  });
}

for (const kind of ['readOnly', 'disabled', 'editor', 'ancestor']) {
  test(kind + ' ownership loss aborts a read and an old callback cannot affect a fresh replacement', async () => {
    const f = fixture(); f.setAutoRead(false); f.begin(); f.select([file()]);
    const old = f.reader, late = old.onload;
    const restore = revokeAccess(f, kind); await tick();
    assert.equal(old.aborts, 1); assert.equal(f.el('btn-open-markdown').disabled, true);
    assert.deepEqual(await f.markdown(), Buffer.from(original));
    restore(); await tick();
    f.begin(); f.select([file('fresh')]);
    const current = f.reader;
    old.result = await file().arrayBuffer(); late(); await tick();
    assert.equal(f.confirmations.length, 0);
    assert.equal(f.el('btn-open-markdown').disabled, true);
    current.complete(await file('fresh').arrayBuffer()); await tick();
    assert.deepEqual(await f.markdown(), Buffer.from('fresh'));
    assert.equal(f.confirmations.length, 1);
    f.el('btn-undo-source-open').click();
    assert.deepEqual(await f.markdown(), Buffer.from(original));
  });

  test(kind + ' round trip in the same task is observed before a file callback or Undo can replace source', async () => {
    const f = fixture(); f.setAutoRead(false); f.begin(); f.select([file()]);
    const buffer = await file().arrayBuffer();
    const restore = revokeAccess(f, kind); restore();
    f.reader.complete(buffer); await tick();
    assert.equal(f.confirmations.length, 0);
    assert.deepEqual(await f.markdown(), Buffer.from(original));
    f.setAutoRead(true); await f.open(file());
    const undoRestore = revokeAccess(f, kind); undoRestore();
    f.el('btn-undo-source-open').click();
    assert.deepEqual(await f.markdown(), Buffer.from(imported));
    assert.equal(f.el('btn-undo-source-open').disabled, true);
  });
}

test('a picker chosen after an access round trip cannot reuse the retired opening gesture', async () => {
  const f = fixture(); f.begin();
  const restore = revokeAccess(f, 'readOnly'); restore();
  f.select([file()]); await tick();
  assert.equal(f.readers.length, 0);
  assert.deepEqual(await f.markdown(), Buffer.from(original));
  await f.open(file());
  assert.deepEqual(await f.markdown(), Buffer.from(imported));
});

test('unrelated preview DOM changes preserve a valid pending source read and replacement undo', async () => {
  const f = fixture(); f.setAutoRead(false); f.begin(); f.select([file()]);
  const temporary = f.doc.createElement('span');
  f.preview.appendChild(temporary); temporary.remove(); await tick();
  assert.equal(f.reader.aborts, 0);
  f.reader.complete(await file().arrayBuffer()); await tick();
  f.preview.appendChild(temporary); temporary.remove(); await tick();
  assert.equal(f.el('btn-undo-source-open').disabled, false);
  f.el('btn-undo-source-open').click();
  assert.deepEqual(await f.markdown(), Buffer.from(original));
});

test('composition and page suspension cancel reads, retire undo, and require a new picker gesture', async () => {
  for (const event of ['compositionstart', 'pagehide']) {
    const f = fixture(); await f.open(file()); f.setAutoRead(false); f.begin(); f.select([file('pending')]);
    (event === 'pagehide' ? f.win : f.editor).dispatchEvent(new Event(event)); await tick();
    assert.equal(f.reader.aborts, 1); assert.equal(f.el('btn-open-markdown').disabled, true); assert.equal(f.el('btn-undo-source-open').disabled, true);
    if (event === 'pagehide') f.win.dispatchEvent(new Event('pageshow'));
    else f.editor.dispatchEvent(new Event('compositionend'));
    assert.equal(f.el('btn-open-markdown').disabled, false);
    f.select([file('stale picker')]); await tick(); assert.deepEqual(await f.markdown(), Buffer.from(imported));
  }
});

test('read deadline aborts I/O and permits retry; old completion cannot release the retry', async () => {
  const f = fixture(); f.setAutoRead(false); f.begin(); f.select([file()]); const old = f.reader, late = old.onload;
  f.fire(10000); await tick(); assert.equal(old.aborts, 1); assert.match(f.status, /ten seconds/);
  f.begin(); f.select([file('retry')]); old.result = await file().arrayBuffer(); late(); await tick();
  assert.equal(f.el('btn-open-markdown').disabled, true); f.reader.complete(await file('retry').arrayBuffer()); await tick();
  assert.deepEqual(await f.markdown(), Buffer.from('retry')); assert.equal(f.el('btn-open-markdown').disabled, false);
});

test('mismatched FileReader length and errors preserve source and release controls', async () => {
  const f = fixture(); f.setAutoRead(false); f.begin(); f.select([file()]); f.reader.complete(new ArrayBuffer(1)); await tick();
  assert.match(f.status, /size changed/); assert.equal(f.dirty(), false);
  f.begin(); f.select([file()]); f.reader.onerror(); await tick();
  assert.match(f.status, /Unable to read/); assert.equal(f.el('btn-open-markdown').disabled, false);
});

test('source opening and recovery remain available after a renderer failure', async () => {
  const f = fixture(); f.failPreview(Error('native unavailable')); await f.open(file()); f.fire(150);
  assert.match(f.status, /native unavailable/); assert.deepEqual(await f.markdown(), Buffer.from(imported));
  f.el('btn-undo-source-open').click(); assert.deepEqual(await f.markdown(), Buffer.from(original));
});

test('a failed editor assignment leaves the previous lossless source and preview intact', async () => {
  const f = fixture(); f.rejectWrites(value => { if (value === imported) throw Error('assignment failed'); });
  await f.open(file()); assert.match(f.status, /assignment failed/); assert.equal(f.preview.innerHTML, 'ORIGINAL RENDER');
  assert.deepEqual(await f.markdown(), Buffer.from(original)); assert.equal(f.dirty(), false);
});

test('synchronous input subscribers see the imported source and can retire undo by making newer edits', async () => {
  const f = fixture(); f.editor.addEventListener('input', () => {
    f.el('btn-export-pdf').click(); f.editor.value = 'subscriber edit';
  }, {once: true});
  await f.open(file()); assert.equal(f.calls[0][1], imported); assert.equal(f.editor.value, 'subscriber edit');
  assert.equal(f.el('btn-undo-source-open').disabled, true); assert.deepEqual(await f.markdown(), Buffer.from('subscriber edit'));
});

test('Save HTML serializes current exact source and excludes transient file controls/undo', async () => {
  const f = fixture(); await f.open(file()); f.el('btn-save-html').click();
  const entries = [...f.clone.nodes];
  const data = entries.find(([selector]) => selector.includes('fmd-raw-source'))[1].textContent;
  assert.equal(JSON.parse(data), imported); assert.doesNotMatch(data, /<\/script/);
  assert.equal(entries.find(([selector]) => selector.includes('fmd-source-controls'))[1].removed, true);
  assert.equal(f.el('btn-undo-source-open').disabled, false, 'Saving does not clear live undo or mark a disk save');
  assert.equal(f.dirty(), true);
});

test('absent FileReader keeps established editing and downloads without misleading file controls', async () => {
  const f = fixture({readerAvailable: false}); assert.equal(f.el('btn-open-markdown'), null);
  f.type('ordinary edits'); assert.deepEqual(await f.markdown(), Buffer.from('ordinary edits'));
});


test('plain HTML publication uses current exact source without flushing preview or consuming source undo', async () => {
  const seen = [], html = '<!DOCTYPE html><html><head></head><body>Native publication</body></html>';
  const f = fixture({publishing: (...args) => { seen.push(args); return html; }});
  await f.open(file()); const timers = f.timers.size;
  f.el('btn-publish-html').click();
  assert.deepEqual(seen, [[imported]]); assert.equal(f.calls.length, 0); assert.equal(f.timers.size, timers + 1);
  assert.equal(await f.downloads[0].blob.text(), html); assert.equal(f.downloads[0].filename, 'Document.published.html');
  assert.equal(f.downloads[0].blob.type, 'text/html;charset=utf-8'); assert.equal(f.dirty(), true);
  assert.equal(f.el('btn-undo-source-open').disabled, false);
  f.el('btn-undo-source-open').click(); assert.deepEqual(await f.markdown(), Buffer.from(original));
});

test('publication reads undispatched current edits and does not pass preview zoom or theme', () => {
  const seen = [], f = fixture({publishing: (...args) => { seen.push(args); return '<html><head></head></html>'; }});
  f.el('btn-zoom-in').click(); f.el('btn-theme-toggle').click();
  f.type('new source before debounce', false); f.el('btn-publish-html').click();
  assert.deepEqual(seen, [['new source before debounce']]); assert.equal(f.downloads.length, 1);
  assert.equal(f.calls.at(-1)[1], original, 'Publication did not refresh the older zoomed preview');
});

test('plain publication is capability-gated and readiness failure never enables a false fallback', async () => {
  assert.equal(fixture({native: false}).el('btn-publish-html'), null);
  assert.equal(fixture().el('btn-publish-html'), null);
  for (const makeReady of [() => Promise.resolve(false), () => Promise.reject(Error('init failed'))]) {
    const f = fixture({publishing: () => { throw Error('must not run'); }, ready: makeReady()});
    assert.equal(f.el('btn-publish-html').disabled, true); await tick();
    f.el('btn-publish-html').click(); assert.equal(f.downloads.length, 0); assert.equal(f.el('btn-publish-html').disabled, true);
  }
  let resolve;
  const ready = new Promise(done => { resolve = done; });
  const f = fixture({publishing: () => '<html><head></head></html>', ready});
  assert.equal(f.el('btn-publish-html').disabled, true); resolve(true); await tick();
  f.el('btn-publish-html').click(); assert.equal(f.downloads.length, 1);
});

test('publication failures and invalid output cannot download stale preview content', () => {
  for (const publishing of [() => { throw Error('native failed'); }, () => '', () => undefined, () => Promise.resolve('not synchronous')]) {
    const f = fixture({publishing}); f.type('preserve source'); f.el('btn-publish-html').click();
    assert.equal(f.downloads.length, 0); assert.equal(f.editor.value, 'preserve source');
    assert.equal(f.preview.innerHTML, 'ORIGINAL RENDER'); assert.match(f.status, /Unable to complete/);
  }
});

test('reentrant source changes prevent publication of an obsolete rendered revision', () => {
  let f;
  f = fixture({publishing: () => { f.type('newer source'); return '<html><head></head></html>'; }});
  f.el('btn-publish-html').click(); assert.equal(f.downloads.length, 0);
  assert.equal(f.editor.value, 'newer source'); assert.match(f.status, /changed during HTML publishing/);
});

test('Save HTML omits the transient publication control while retaining it in the live editor', () => {
  const f = fixture({publishing: () => '<html><head></head></html>'}); f.el('btn-save-html').click();
  assert.equal(f.clone.nodes.get('body > .fmd-app-header #btn-publish-html').removed, true);
  assert.ok(f.el('btn-publish-html')); assert.equal(f.dirty(), false);
});
