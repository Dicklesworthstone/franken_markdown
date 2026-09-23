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

function fixture({source = original, native = true, readerAvailable = true} = {}) {
  const downloads = [], objects = new Map(), revoked = [], readers = [], timers = new Map(), confirmations = [], calls = [];
  let timerId = 0, urlId = 0, autoRead = true, confirm = () => true, previewFailure = null, acceptWrite = null;
  const doc = new EventTarget(), win = new EventTarget();
  class Element extends EventTarget {
    constructor(tag = 'span', id = '') {
      super(); this.tagName = tag.toUpperCase(); this.id = id; this.children = []; this.parentNode = null;
      this.textContent = ''; this.innerHTML = ''; this._value = ''; this.disabled = false; this.hidden = false;
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
    appendChild(child) { child.parentNode = this; this.children.push(child); return child; }
    insertBefore(child, ref) { child.parentNode = this; this.children.splice(this.children.indexOf(ref), 0, child); }
    replaceChildren(...children) { this.children = []; for (const child of children) this.appendChild(child); }
    remove() { this.removed = true; this.parentNode?.children.splice(this.parentNode.children.indexOf(this), 1); }
    setAttribute(key, value) { this.attributes[key] = value; }
    querySelector(selector) {
      const id = selector.match(/#([\w-]+)/)?.[1];
      return this.children.flatMap(child => [child, ...child.descendants()]).find(child => child.id === id) ?? null;
    }
    *descendants() { for (const child of this.children) { yield child; yield* child.descendants(); } }
    focus() { doc.activeElement = this; }
    setSelectionRange(start, end, direction = 'none') { this.selectionStart = start; this.selectionEnd = end; this.selectionDirection = direction; }
    click() {
      if (this.disabled) return;
      if (this.tagName === 'A') downloads.push({blob: objects.get(this.href), filename: this.download});
      else this.dispatchEvent(new Event('click'));
    }
  }
  doc.body = new Element('body'); doc.documentElement = new Element('html'); doc.title = 'Document';
  const header = doc.body.appendChild(new Element('header', 'header'));
  const toolbar = header.appendChild(new Element('div'));
  for (const id of ['toggle-view', 'zoom-in', 'zoom-out', 'zoom-reset', 'theme-toggle', 'stats-toggle', 'save-markdown', 'save-html', 'export-pdf']) {
    toolbar.appendChild(new Element('button', 'btn-' + id));
  }
  const body = doc.body.appendChild(new Element('div', 'fmd-app-body')); body.classList.add('view-split');
  const editor = body.appendChild(new Element('textarea', 'fmd-editor'));
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
    clone = {nodes, querySelector(selector) {
      if (!nodes.has(selector)) nodes.set(selector, new Element());
      return nodes.get(selector);
    }, get outerHTML() { return JSON.stringify([...nodes].map(([selector, element]) => ({selector, text: element.textContent, removed: !!element.removed}))); }};
    return clone;
  };
  win.confirm = text => { confirmations.push(text); return confirm(text); };
  win.print = () => { calls.push(['print']); };
  if (native) Object.defineProperty(win, '__fmdNativeRuntime', {value: {
    version: 1, diagnostics: [],
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
  const context = {document: doc, window: win, console, Event, TextDecoder, TextEncoder, ArrayBuffer, Blob,
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
