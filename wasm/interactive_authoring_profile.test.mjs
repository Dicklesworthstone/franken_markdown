// Whole shipped renderer + controller (+ styled adapter) with explicit DOM and
// timer boundaries. The Markdown parser is never replaced by an ABI/test double.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {test} from 'node:test';
import vm from 'node:vm';

const [renderer, controller, presentation] = await Promise.all([
  '../src/interactive_renderer.js', '../src/interactive_controller.js', '../src/interactive/presentation.js',
].map(path => readFile(new URL(path, import.meta.url), 'utf8')));
const escape = text => String(text).replace(/[&<>"']/g, ch =>
  ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));
const unescape = text => text.replace(/&(quot|#39|lt|gt|amp);/g, (_, key) =>
  ({quot: '"', '#39': "'", lt: '<', gt: '>', amp: '&'}[key]));
const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
const extensions = 'Term\n: Definition with **emphasis**\n\n<!-- pagebreak -->\n\nAfter';
const explicit = profile => JSON.stringify({version: 1, profile});

function fixture({source = '# Original\r\n', profile, profileText, profileType = 'application/json',
  duplicate = false, styled = false, native = false, nativePayload = false, initial = '<h1>Native original</h1>'} = {}) {
  const downloads = [], blobs = new Map(), timers = new Map(), calls = [], copies = [];
  const doc = new EventTarget(), win = new EventTarget();
  let sequence = 0;
  class Element extends EventTarget {
    constructor(tag = 'span', id = '') {
      super(); this.tagName = tag.toUpperCase(); this.id = id; this.children = []; this.parentNode = null;
      this.attrs = new Map(); this.textContent = ''; this._html = ''; this._value = ''; this.disabled = false;
      this.style = {getPropertyValue: () => '16px', setProperty() {}};
      const classes = new Set();
      this.classList = {add: key => classes.add(key), remove: key => classes.delete(key), contains: key => classes.has(key),
        toggle: key => classes.has(key) ? classes.delete(key) : classes.add(key)};
    }
    get value() { return this._value; }
    set value(text) { this._value = this.tagName === 'TEXTAREA' ? String(text).replace(/\r\n?/g, '\n') : String(text); }
    get innerHTML() { return this._html; }
    set innerHTML(text) {
      this._html = text; this.frame = null;
      if (this.tagName === 'TEMPLATE') {
        this.content = {querySelector: selector => selector === 'main.fmd'
          ? {innerHTML: text.match(/<main class="fmd">([\s\S]*)<\/main>/)?.[1] ?? ''} : null,
        querySelectorAll: () => []};
      }
      if (this.id === 'fmd-content' && text.startsWith('<iframe ')) {
        const frame = this.frame = new Element('iframe');
        for (const match of text.matchAll(/([\w-]+)="([^"]*)"/g)) frame.setAttribute(match[1], unescape(match[2]));
        frame.contentDocument = Object.assign(new EventTarget(), {
          body: {scrollHeight: 320}, documentElement: {scrollHeight: 320}, fonts: {ready: Promise.resolve()},
        });
        frame.contentWindow = {focus() {}, print() { calls.push(['styled-print', frame.getAttribute('srcdoc')]); }};
        queueMicrotask(() => frame.dispatchEvent(new Event('load')));
      }
    }
    appendChild(node) { if (node.parentNode) node.remove(); node.parentNode = this; this.children.push(node); return node; }
    insertBefore(node, reference) {
      if (node.parentNode) node.remove(); node.parentNode = this;
      const index = this.children.indexOf(reference);
      if (index < 0) this.children.push(node); else this.children.splice(index, 0, node);
      return node;
    }
    replaceChildren(...nodes) { this.children = []; this._html = ''; for (const node of nodes) this.appendChild(node); }
    remove() { if (this.parentNode) this.parentNode.children.splice(this.parentNode.children.indexOf(this), 1); this.parentNode = null; }
    setAttribute(key, value) { this.attrs.set(key, String(value)); }
    getAttribute(key) { return this.attrs.get(key) ?? null; }
    removeAttribute(key) { this.attrs.delete(key); }
    *descendants() { for (const node of this.children) { yield node; yield* node.descendants(); } }
    querySelectorAll(selector) {
      const direct = selector.match(/^body > script#([\w-]+)(?:\[type="([^"]+)"\])?$/);
      if (direct) return [...this.descendants()].find(node => node.tagName === 'BODY')?.children
        .filter(node => node.tagName === 'SCRIPT' && node.id === direct[1]
          && (!direct[2] || node.getAttribute('type') === direct[2])) ?? [];
      return [];
    }
    querySelector(selector) {
      if (selector.startsWith('body > script#')) return this.querySelectorAll(selector)[0] ?? null;
      if (selector === 'body') return [...this.descendants()].find(node => node.tagName === 'BODY') ?? null;
      if (selector === 'iframe[data-fmd-styled-preview]') return this.frame;
      const id = [...selector.matchAll(/#([\w-]+)/g)].at(-1)?.[1];
      return [...this.descendants()].find(node => node.id === id) ?? null;
    }
    cloneNode() {
      const clone = new Element(this.tagName, this.id);
      clone.textContent = this.textContent; clone._html = this._html; clone.attrs = new Map(this.attrs);
      for (const node of this.children) clone.appendChild(node.cloneNode(true));
      return clone;
    }
    get outerHTML() {
      const tag = this.tagName.toLowerCase();
      const attributes = (this.id ? ' id="' + escape(this.id) + '"' : '')
        + [...this.attrs].map(([key, value]) => ' ' + key + '="' + escape(value) + '"').join('');
      const text = this.tagName === 'SCRIPT' ? this.textContent : escape(this.textContent);
      return '<' + tag + attributes + '>' + (this._html || text || this.children.map(node => node.outerHTML).join('')) + '</' + tag + '>';
    }
    focus() {}
    click() {
      if (this.disabled) return;
      if (this.tagName === 'A') downloads.push({name: this.download, blob: blobs.get(this.href)});
      else this.dispatchEvent(new Event('click', {cancelable: true}));
    }
  }
  const root = doc.documentElement = new Element('html');
  const body = doc.body = root.appendChild(new Element('body')); doc.title = 'Profile document';
  doc.getElementById = id => [...root.descendants()].find(node => node.id === id) ?? null;
  doc.createElement = tag => new Element(tag);
  doc.querySelector = selector => root.querySelector(selector);
  doc.querySelectorAll = selector => root.querySelectorAll(selector);
  function data(id, text, type = 'application/json') {
    const node = body.appendChild(new Element('script', id)); node.setAttribute('type', type); node.textContent = text; return node;
  }
  if (profileText !== undefined || profile !== undefined) {
    data('fmd-authoring-profile', profileText ?? explicit(profile), profileType);
    if (duplicate) data('fmd-authoring-profile', explicit(profile ?? 'gfm-plus'));
  }
  const raw = data('fmd-raw-source', JSON.stringify(source));
  data('fmd-image-assets', JSON.stringify([['chart.png', 'data:image/png;base64,AQID']]));
  if (nativePayload) data('fmd-native-runtime', '{"version":1,"options":{"profile":"commonmark-gfm"}}');
  if (styled) data('fmd-presentation', JSON.stringify({version: 1, css: 'body {color:purple}', title: 'Profile',
    lang: 'en', toc: false, tocDepth: 6}));
  const app = body.appendChild(new Element('div', 'fmd-app-body')); app.classList.add('view-split');
  const pane = app.appendChild(new Element('div', 'editor-pane'));
  const editor = pane.appendChild(new Element('textarea', 'fmd-editor'));
  const status = pane.appendChild(new Element('span', 'fmd-save-status'));
  const preview = app.appendChild(new Element('div', 'fmd-content'));
  preview.innerHTML = styled ? '<iframe data-fmd-styled-preview srcdoc="'
    + escape('<html><body><main class="fmd">' + initial + '</main></body></html>') + '"></iframe>' : initial;
  const stats = body.appendChild(new Element('div', 'stats-drawer'));
  for (const id of ['stat-words', 'stat-chars', 'stat-read-time', 'stat-readability', 'btn-stats-close']) stats.appendChild(new Element('span', id));
  for (const id of ['source-line-count', 'view-mode-icon', 'view-mode-label']) app.appendChild(new Element('span', id));
  for (const name of ['save-markdown', 'save-html', 'export-pdf', 'toggle-view', 'zoom-in', 'zoom-out', 'zoom-reset', 'theme-toggle', 'stats-toggle']) {
    body.appendChild(new Element('button', 'btn-' + name));
  }
  // Saved metadata must remain before this executable script on the next open.
  body.appendChild(new Element('script', 'fmd-test-bundle'));
  const clone = root.cloneNode.bind(root);
  root.cloneNode = () => { const copy = clone(true); copies.push(copy); return copy; };
  win.print = () => calls.push(['print', preview.innerHTML]);
  if (native) Object.defineProperty(win, '__fmdNativeRuntime', {value: {
    version: 1, settings: {profile: 'commonmark-gfm'}, diagnostics: [],
    render(text, target) { calls.push(['native', text]); target.innerHTML = '<p>Native authoritative</p>'; },
    pdf(text) { calls.push(['pdf', text]); return '%PDF'; },
  }});
  class Observer { observe() {} disconnect() {} }
  const context = {document: doc, window: win, Event, Blob, console, queueMicrotask, MutationObserver: Observer,
    setTimeout(callback, delay) { const id = ++sequence; timers.set(id, {callback, delay}); return id; },
    clearTimeout(id) { timers.delete(id); },
    URL: {createObjectURL(blob) { const id = 'blob:' + ++sequence; blobs.set(id, blob); return id; }, revokeObjectURL(id) { blobs.delete(id); }}};
  vm.runInNewContext(renderer + '\n' + (styled ? presentation + '\n' : '') + controller, context, {timeout: 3000});
  return {
    doc, win, editor, preview, status, downloads, calls, copies,
    addManifest(text) { return data('fmd-authoring-profile', text); },
    edit(text) {
      editor.value = text; editor.dispatchEvent(new Event('input'));
      for (const [id, timer] of [...timers]) if (timer.delay === 150) { timers.delete(id); timer.callback(); }
    },
    click(name) { doc.getElementById('btn-' + name).click(); },
    async saved() { return downloads.at(-1).blob.text(); },
    rendered() { return preview.frame ? preview.frame.getAttribute('srcdoc') : preview.innerHTML; },
  };
}

function savedState(html) {
  // The adapter's authored data blocks precede the workspace UI; preview text
  // may contain lookalike scripts and must not be promoted into app metadata.
  const owned = html.slice(0, html.indexOf('<div id="fmd-app-body">'));
  const read = id => owned.match(new RegExp('<script id="' + id + '" type="application/json">([\\s\\S]*?)</script>'))?.[1];
  return {source: JSON.parse(read('fmd-raw-source')), profileText: read('fmd-authoring-profile')};
}

for (const styled of [false, true]) {
  for (const profile of ['commonmark-gfm', 'gfm-plus', undefined]) {
    test('edits, save/reopen and printing retain ' + (profile ?? 'legacy') + (styled ? ' with presentation' : ''), async () => {
      const f = fixture({profile, styled});
      f.edit(extensions);
      assert.equal(f.rendered().includes('<dl>'), profile !== 'commonmark-gfm');
      assert.equal(f.rendered().includes('class="fmd-pagebreak"'), profile !== 'commonmark-gfm');
      f.click('save-html');
      const html = await f.saved(), state = savedState(html);
      assert.equal(state.source, extensions);
      assert.equal(state.profileText, profile === undefined ? undefined : explicit(profile));
      if (profile !== undefined) assert.ok(html.indexOf('id="fmd-authoring-profile"') < html.indexOf('id="fmd-test-bundle"'));
      const reopened = fixture({...state, styled});
      reopened.edit(extensions + '\n\nLater');
      assert.equal(reopened.rendered().includes('<dl>'), profile !== 'commonmark-gfm');
      assert.equal(reopened.rendered().includes('class="fmd-pagebreak"'), profile !== 'commonmark-gfm');
      reopened.click('export-pdf'); await flush();
      assert.equal(reopened.calls.length, 1);
      assert.equal(reopened.calls[0][0], styled ? 'styled-print' : 'print');
      assert.equal(reopened.calls[0][1].includes('<dl>'), profile !== 'commonmark-gfm');
      if (styled) assert.match(reopened.calls[0][1], /color:purple/);
    });
  }
}

test('source bytes and original native preview survive exact undo and reopen', async () => {
  const source = '\ufeff\r\n# Original é中😀\rline\r\n';
  const f = fixture({source, profile: 'commonmark-gfm', initial: '<svg>Native-only diagram</svg>'});
  f.click('save-markdown'); assert.equal(await f.saved(), source);
  f.edit(extensions); f.edit(source);
  assert.equal(f.preview.innerHTML, '<svg>Native-only diagram</svg>');
  f.click('save-html');
  const state = savedState(await f.saved());
  assert.equal(state.source, source);
  const reopened = fixture(state); reopened.click('save-markdown');
  assert.equal(await reopened.saved(), source);
});

test('captured profile survives replaced or duplicate live metadata, and legacy cannot acquire a new dialect', async () => {
  for (const styled of [false, true]) for (const profile of ['commonmark-gfm', undefined]) {
    const f = fixture({profile, styled});
    f.doc.querySelector('body > script#fmd-authoring-profile')?.remove();
    f.addManifest(explicit('gfm-plus')); f.addManifest('{damaged}');
    f.edit(extensions);
    assert.equal(f.rendered().includes('<dl>'), profile === undefined);
    f.click('save-html'); const html = await f.saved(), state = savedState(html);
    assert.equal(state.profileText, profile === undefined ? undefined : explicit(profile));
    assert.equal((html.match(/id="fmd-authoring-profile"/g) ?? []).length, profile === undefined ? 0 : 1);
    f.click('export-pdf'); await flush();
    assert.equal(f.calls.length, 1);
    assert.equal(f.calls[0][1].includes('<dl>'), profile === undefined);
  }
});

test('preview-owned metadata lookalikes never select the authoring dialect', async () => {
  const f = fixture({initial: '<div><script id="fmd-authoring-profile" type="application/json">'
    + explicit('commonmark-gfm') + '</script></div>'});
  f.edit(extensions); assert.match(f.rendered(), /<dl>/);
  f.click('save-html'); assert.equal(savedState(await f.saved()).profileText, undefined);
});

for (const styled of [false, true]) {
  for (const [name, options, reason] of [
    ['JSON', {profileText: '{broken'}, /malformed JSON/],
    ['version', {profileText: '{"version":2,"profile":"gfm-plus"}'}, /unsupported version/],
    ['profile', {profileText: '{"version":1,"profile":"future"}'}, /unknown profile/],
    ['schema', {profileText: '{"version":1}'}, /expected version and profile/],
    ['type', {profile: 'gfm-plus', profileType: 'text/plain'}, /expected JSON metadata/],
    ['duplicates', {profile: 'gfm-plus', duplicate: true}, /duplicate manifests/],
  ]) {
    test('invalid ' + name + ' metadata preserves source and refuses preview/save/print' + (styled ? ' with presentation' : ''), async () => {
      const source = '\ufeff# Recover\r\nexact\r';
      const f = fixture({...options, source, styled});
      assert.match(f.status.textContent, reason);
      f.click('save-html'); assert.equal(f.downloads.length, 0);
      f.click('export-pdf'); await flush(); assert.equal(f.calls.length, 0); assert.match(f.status.textContent, reason);
      const key = Object.assign(new Event('keydown', {cancelable: true}), {key: 'p', ctrlKey: true});
      f.doc.dispatchEvent(key); await flush();
      assert.equal(key.defaultPrevented, true); assert.equal(f.calls.length, 0);
      const initial = f.preview.innerHTML;
      f.edit(extensions); assert.equal(f.preview.innerHTML, initial); assert.match(f.status.textContent, reason);
      f.edit(source); f.click('save-markdown');
      assert.equal(await f.saved(), source);
      assert.equal(f.downloads[0].name, 'Profile document.md');
    });
  }
}

for (const lifecycle of ['composition', 'suspension']) for (const invalid of [false, true]) {
  test('unstyled print refuses ' + lifecycle + (invalid ? ' with malformed metadata' : ' with valid metadata'), async () => {
    const f = fixture(invalid ? {profileText: '{broken'} : {profile: 'commonmark-gfm'});
    if (lifecycle === 'composition') f.editor.dispatchEvent(new Event('compositionstart'));
    else f.win.dispatchEvent(new Event('pagehide'));
    f.edit(extensions); f.click('export-pdf'); await flush();
    assert.equal(f.calls.length, 0);
    assert.match(f.status.textContent, /Finish editing or return to the document before printing/);
    f.click('save-markdown'); assert.equal(await f.saved(), extensions);
    if (lifecycle === 'composition') f.editor.dispatchEvent(new Event('compositionend'));
    else f.win.dispatchEvent(new Event('pageshow'));
    f.click('export-pdf'); await flush();
    assert.equal(f.calls.length, invalid ? 0 : 1);
    if (invalid) assert.match(f.status.textContent, /malformed JSON/);
    else {
      assert.match(f.calls[0][1], /After/);
      assert.equal(f.calls[0][1].includes('<dl>'), false);
    }
  });
}

test('native runtime owns its profile even when lightweight metadata is damaged', async () => {
  const f = fixture({native: true, nativePayload: true, profileText: '{broken'});
  f.edit(extensions);
  assert.match(f.rendered(), /Native authoritative/);
  assert.equal(f.calls[0][0], 'native');
  f.click('save-html');
  const html = await f.saved();
  assert.match(html, /id="fmd-authoring-profile" type="application\/json">\{broken<\/script>/);
  assert.match(html, /"options":\{"profile":"commonmark-gfm"\}/);
  assert.equal(f.doc.querySelector('body > script#fmd-native-runtime[type="application/json"]').textContent,
    '{"version":1,"options":{"profile":"commonmark-gfm"}}');
});

test('styled native runtime without a payload retains native preview and PDF authority', async () => {
  const f = fixture({native: true, styled: true, profileText: '{broken'});
  f.edit(extensions);
  assert.match(f.rendered(), /Native authoritative/);
  assert.equal(f.calls[0][0], 'native');
  assert.equal(f.status.textContent.includes('Invalid saved authoring profile'), false);
  f.click('export-pdf'); await flush();
  assert.equal(f.calls.length, 2); assert.equal(f.calls[1][0], 'pdf');
  assert.equal(f.calls[1][1], extensions);
  assert.equal(f.downloads[0].name, 'Profile document.pdf');
  f.click('save-html');
  assert.match(await f.saved(), /id="fmd-authoring-profile" type="application\/json">\{broken<\/script>/);
});

test('missing native runtime never activates the lightweight parser', () => {
  const f = fixture({nativePayload: true, profile: 'gfm-plus'});
  const initial = f.preview.innerHTML; f.edit(extensions);
  assert.equal(f.preview.innerHTML, initial);
  assert.match(f.status.textContent, /reduced parser will not be used/);
  f.click('export-pdf'); assert.equal(f.calls.length, 0);
});
