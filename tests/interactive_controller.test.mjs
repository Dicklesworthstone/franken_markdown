// Execute the shipped controller against a small, stateful DOM adapter.
// Real-browser save/reopen coverage is in support/interactive_browser_check.py.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
const controller = readFileSync(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
const renderer = readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8');
const template = readFileSync(new URL('../src/interactive.rs', import.meta.url), 'utf8');
const ids = [...template.matchAll(/id="([^"]+)"/g)].map(match => match[1]);
// Ids inside the static <header class="fmd-app-header">, for header-scoped queries.
const headerMarkup = template.slice(template.indexOf('<header class="fmd-app-header">'), template.indexOf('</header>'));
const headerIds = new Set([...headerMarkup.matchAll(/id="([^"]+)"/g)].map(match => match[1]));
const escape = text => String(text).replace(/[&<>"']/g, ch => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[ch]));
function element() {
  const classes = new Set(), attributes = new Map();
  return {
    value: '', textContent: '', innerHTML: '', handlers: {}, removed: false,
    addEventListener(name, fn) { this.handlers[name] = fn; },
    remove() { this.removed = true; },
    getAttribute(name) { return attributes.has(name) ? attributes.get(name) : null; },
    setAttribute(name, value) { attributes.set(name, String(value)); },
    removeAttribute(name) { attributes.delete(name); },
    classList: {
      add(name) { classes.add(name); }, remove(name) { classes.delete(name); },
      contains(name) { return classes.has(name); },
      toggle(name) { classes.has(name) ? classes.delete(name) : classes.add(name); }
    }
  };
}
function setup({source = '# Original', title = 'Document', scale = '', read = false, fail = '', normalizeTextarea = false, native = null,
  imageAssets = null, initialPreview = null, actualRenderer = null} = {}) {
  const elements = new Map(ids.map(id => [id, element()]));
  // The JSON script's id is in an escaped Rust string, rather than a raw literal.
  elements.set('fmd-raw-source', {...element(), textContent: JSON.stringify(source)});
  if (normalizeTextarea) {
    let value = '';
    Object.defineProperty(elements.get('fmd-editor'), 'value', {
      get() { return value; }, set(text) { value = text.replace(/\r\n?/g, '\n'); }
    });
  }
  const get = id => { assert.ok(elements.has(id), id); return elements.get(id); };
  get('stats-drawer').querySelector = selector => get(selector.slice(1));
  get('fmd-content').innerHTML = initialPreview ?? '<svg id="native-diagram"></svg><p>Native render</p>';
  const assetData = imageAssets === null ? null : {...element(), textContent: JSON.stringify(imageAssets)
    .replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029')};
  get('fmd-app-body').classList.add(read ? 'view-read' : 'view-split');
  const timers = new Map(), blobs = new Map(), downloads = [], revoked = [], anchors = [], renders = [], printed = [], copies = [];
  const styles = new Map([['--fmd-base', scale]]);
  let next = 0, failure = fail;
  const absent = new Set([
    'body > script#fmd-image-assets[type="application/json"]',
    'body > script#fmd-native-runtime[type="application/json"]',
    'body > dialog#fmd-document-settings',
  ]);
  const header = {...element(),
    querySelector(selector) {
      assert.ok(selector.startsWith('#'), selector);
      const id = selector.slice(1);
      return headerIds.has(id) ? get(id) : null;
    }
  };
  const document = {
    title, getElementById: get, ...element(),
    // Like a real DOM: resolve the template's static elements and return null
    // for optional ones it does not emit (image assets, native runtime payload,
    // dialogs the controller creates on demand). Unknown selectors are errors.
    querySelector(selector) {
      if (selector === 'body > script#fmd-raw-source[type="application/json"]') return get('fmd-raw-source');
      if (selector === 'body > #stats-drawer') return get('stats-drawer');
      if (selector === 'body > .fmd-app-header') return header;
      if (selector === 'body > script#fmd-image-assets[type="application/json"]') return assetData;
      assert.ok(absent.has(selector), selector);
      return null;
    },
    body: {...element(), appendChild(node) { anchors.push(node); }},
    createElement(tag) {
      assert.equal(tag, 'a');
      return {...element(), click() {
        if (failure === 'click') throw Error('click failed');
        downloads.push({filename: this.download, blob: blobs.get(this.href), url: this.href});
      }};
    },
    documentElement: {...element(),
      style: {getPropertyValue(key) { return styles.get(key) || ''; }, setProperty(key, value) { styles.set(key, value); }},
      cloneNode(deep) {
        assert.equal(deep, true);
        const data = {...element(), textContent: get('fmd-raw-source').textContent};
        const textarea = {...element(), textContent: source};
        const status = {...element(), textContent: get('fmd-save-status').textContent};
        const drawer = element(); drawer.classList.add('open');
        const preview = get('fmd-content').innerHTML;
        const assets = assetData ? {...element(), textContent: assetData.textContent} : null;
        const nodes = new Map([
          ['body > script#fmd-raw-source[type="application/json"]', data],
          ['body > #fmd-app-body > #editor-pane > textarea#fmd-editor', textarea],
          ['body > #stats-drawer', drawer],
          ['#editor-pane > .fmd-pane-header > #fmd-save-status', status]
        ]);
        if (assets) nodes.set('body > script#fmd-image-assets[type="application/json"]', assets);
        const copy = {...element(), data, textarea, status, drawer, preview, assets,
          // The clone holds only the template's static elements. Controls the
          // controller adds at runtime are absent from it and resolve to null;
          // a static element this adapter does not model is a harness error.
          querySelector(selector) {
            if (nodes.has(selector)) return nodes.get(selector);
            const id = selector.match(/#([\w-]+)$/)?.[1];
            assert.ok(id && !ids.includes(id), selector);
            return null;
          },
          get outerHTML() { return '<html><body><textarea>' + escape(textarea.textContent) + '</textarea><main>' + preview
            + '</main><script type="application/json" id="fmd-raw-source">' + data.textContent + '</script>'
            + (assets ? '<script type="application/json" id="fmd-image-assets">' + assets.textContent + '</script>' : '') + '</body></html>'; }
        };
        copies.push(copy); return copy;
      }
    }
  };
  const window = {...element(), print() { printed.push(get('fmd-content').innerHTML); }};
  if (native) Object.defineProperty(window, '__fmdNativeRuntime', {value: native});
  const context = vm.createContext({document, window, Blob,
    URL: {
      createObjectURL(blob) { if (failure === 'url') throw Error('URL failed'); const url = 'blob:test-' + ++next; blobs.set(url, blob); return url; },
      revokeObjectURL(url) { revoked.push(url); }
    },
    parseMarkdownClient(text, bindings) {
      if (failure === 'render') throw Error('render failed');
      renders.push(text); return actualRenderer ? actualRenderer(text, bindings) : '<p>' + escape(text) + '</p>';
    },
    setTimeout(fn, delay) { const id = ++next; timers.set(id, {fn, delay}); return id; },
    clearTimeout(id) { timers.delete(id); }
  });
  vm.runInContext(controller, context);
  function click(id) { get(id).handlers.click(); }
  function edit(text) { get('fmd-editor').value = text; get('fmd-editor').handlers.input(); }
  function fire(event, properties = {}) {
    const obj = {...properties, prevented: false, preventDefault() { this.prevented = true; }};
    const target = event === 'keydown' ? document : window;
    target.handlers[event](obj); return obj;
  }
  function tick(delay) {
    for (const [id, entry] of [...timers]) if (entry.delay === delay) { timers.delete(id); entry.fn(); }
  }
  return {get, click, edit, fire, tick, renders, printed, downloads, revoked, timers, anchors, copies, styles,
    fail(value) { failure = value; }};
}

test('startup preserves lossless source and the full native preview', () => {
  const s = setup({source: '\n\r\0é中😀'});
  assert.equal(s.get('fmd-editor').value, '\n\r\0é中😀');
  assert.equal(s.renders.length, 0);
  s.click('btn-export-pdf');
  assert.match(s.printed[0], /native-diagram/);
});

test('print synchronously flushes the latest edit and cancels stale debounce work', () => {
  const s = setup(); s.edit('superseded'); s.edit('Latest');
  assert.equal(s.renders.length, 0);
  s.click('btn-export-pdf');
  assert.deepEqual(s.renders, ['Latest']);
  assert.deepEqual(s.printed, ['<p>Latest</p>']);
  assert.equal(s.timers.size, 0);
  s.fire('beforeprint'); assert.equal(s.renders.length, 1);
});

test('browser print menu also flushes pending input without issuing another print', () => {
  const s = setup(); s.edit('Browser menu'); s.fire('beforeprint');
  assert.equal(s.get('fmd-content').innerHTML, '<p>Browser menu</p>');
  assert.equal(s.printed.length, 0); assert.equal(s.timers.size, 0);
});

test('undo to the original source restores native rendering and removes the leave warning', () => {
  const s = setup(); s.edit('changed'); s.tick(150);
  assert.ok(s.fire('beforeunload').prevented);
  s.edit('# Original'); s.tick(150);
  assert.match(s.get('fmd-content').innerHTML, /native-diagram/);
  assert.deepEqual(s.renders, ['changed']);
  assert.equal(s.fire('beforeunload').prevented, false);
});

test('Markdown download captures exact text, including controls and Unicode, without marking it saved', async () => {
  const s = setup(); const text = '\n\r\n\0\u001f\u2028\u2029é中😀 <script>bad</script>';
  s.edit(text); s.click('btn-save-markdown');
  assert.equal(await s.downloads[0].blob.text(), text);
  assert.equal(s.downloads[0].blob.type, 'text/markdown;charset=utf-8');
  assert.equal(s.downloads[0].filename, 'Document.md');
  assert.ok(s.fire('beforeunload').prevented);
  assert.match(s.get('fmd-save-status').textContent, /Download started/);
  assert.equal(s.renders.length, 0);
});

test('workspace export flushes the preview and safely embeds current source into a detached copy', async () => {
  const s = setup(); const text = '\n</ScRiPt><script>bad</script>\u2028\u2029\0é';
  s.edit(text); s.get('stats-drawer').classList.add('open'); s.click('btn-save-html');
  const copy = s.copies[0];
  assert.equal(JSON.parse(copy.data.textContent), text);
  assert.doesNotMatch(copy.data.textContent, /[<\u2028\u2029]/);
  assert.equal(copy.textarea.textContent, text);
  assert.equal(copy.status.textContent, '');
  assert.equal(copy.drawer.classList.contains('open'), false);
  assert.equal(s.get('stats-drawer').classList.contains('open'), true);
  assert.equal(JSON.parse(s.get('fmd-raw-source').textContent), '# Original');
  assert.equal(s.get('fmd-editor').value, text);
  const output = await s.downloads[0].blob.text();
  assert.ok(output.startsWith('<!DOCTYPE html>\n<html>'));
  assert.ok(output.includes(copy.preview));
  assert.equal(s.downloads[0].blob.type, 'text/html;charset=utf-8');
  assert.equal(s.downloads[0].filename, 'Document.html');
});

test('saving unchanged workspace keeps native-only content rather than re-rendering', async () => {
  const s = setup(); s.click('btn-save-html');
  assert.equal(s.renders.length, 0);
  assert.match(await s.downloads[0].blob.text(), /native-diagram/);
});

test('downloads release each object URL once and remove temporary anchors', () => {
  const s = setup(); s.click('btn-save-markdown'); s.click('btn-save-markdown');
  assert.equal(s.downloads.length, 2); assert.equal(s.revoked.length, 0);
  assert.ok(s.anchors.every(anchor => anchor.removed));
  s.tick(30000); assert.deepEqual(s.revoked, s.downloads.map(d => d.url));
  s.fire('pagehide'); assert.equal(s.revoked.length, 2); assert.equal(s.timers.size, 0);
});

test('page exit cleans pending download resources and their timers', () => {
  const s = setup(); s.click('btn-save-html'); s.fire('pagehide');
  assert.deepEqual(s.revoked, [s.downloads[0].url]);
  assert.equal(s.timers.size, 0);
});

test('download failure releases resources and preserves modified source', () => {
  const s = setup({fail: 'click'}); s.edit('keep this'); s.click('btn-save-markdown');
  assert.equal(s.downloads.length, 0); assert.equal(s.revoked.length, 1);
  assert.ok(s.anchors[0].removed);
  assert.equal(s.get('fmd-editor').value, 'keep this');
  assert.ok(s.fire('beforeunload').prevented);
  assert.match(s.get('fmd-save-status').textContent, /click failed/);
});

test('failed rendering neither starts printing nor exports stale HTML and can be retried', () => {
  const s = setup({fail: 'render'}); s.edit('latest'); s.click('btn-export-pdf'); s.click('btn-save-html');
  assert.equal(s.printed.length, 0); assert.equal(s.downloads.length, 0);
  assert.match(s.get('fmd-content').innerHTML, /native-diagram/);
  s.fail(''); s.click('btn-save-html'); assert.equal(s.downloads.length, 1);
  assert.equal(s.get('fmd-content').innerHTML, '<p>latest</p>');
});

test('download URL allocation failure is reported without changing the editor', () => {
  const s = setup({fail: 'url'}); s.edit('keep'); s.click('btn-save-markdown');
  assert.match(s.get('fmd-save-status').textContent, /URL failed/);
  assert.equal(s.get('fmd-editor').value, 'keep'); assert.equal(s.anchors.length, 0);
});

test('Ctrl/Cmd S and Shift S download the selected format without repeated saves', () => {
  const s = setup();
  assert.ok(s.fire('keydown', {key:'s', ctrlKey:true}).prevented);
  assert.ok(s.fire('keydown', {key:'S', metaKey:true, shiftKey:true}).prevented);
  assert.ok(s.fire('keydown', {key:'s', ctrlKey:true, repeat:true}).prevented);
  assert.equal(s.fire('keydown', {key:'s', ctrlKey:true, altKey:true}).prevented, false);
  assert.deepEqual(s.downloads.map(d => d.filename), ['Document.md', 'Document.html']);
});

test('filenames cannot include path separators, controls or reserved device names', () => {
  for (const title of ['../unsafe\\name:bad\0', 'CON', 'aux.txt', ' ... ', '😀'.repeat(100)]) {
    const s = setup({title}); s.click('btn-save-markdown');
    const filename = s.downloads[0].filename;
    assert.doesNotMatch(filename, /[<>:"/\\|?*\u0000-\u001f\u007f]/);
    assert.ok(Array.from(filename).length <= 83);
    assert.doesNotMatch(filename, /^(con|aux)(\.|$)/i);
  }
});

test('reopened view and zoom controls continue from their saved state', () => {
  const s = setup({scale: '24px', read: true});
  s.click('btn-zoom-in'); assert.equal(s.get('btn-zoom-reset').textContent, '160%');
  s.click('btn-toggle-view'); assert.ok(s.get('fmd-app-body').classList.contains('view-split'));
});


test('browser newline normalization never dirties or rewrites untouched source bytes', async () => {
  const original = '\r\n# CRLF\r\n\r\nBody\r';
  const s = setup({source: original, normalizeTextarea: true});
  assert.equal(s.get('fmd-editor').value, '\n# CRLF\n\nBody\n');
  assert.equal(s.fire('beforeunload').prevented, false);
  s.click('btn-export-pdf');
  assert.equal(s.renders.length, 0);
  s.click('btn-save-markdown');
  assert.equal(await s.downloads[0].blob.text(), original);
  s.click('btn-save-html');
  assert.equal(JSON.parse(s.copies[0].data.textContent), original);
  s.edit('change'); s.tick(150);
  s.edit('\n# CRLF\n\nBody\n'); s.tick(150);
  s.click('btn-save-markdown');
  assert.equal(await s.downloads[2].blob.text(), original);
  assert.match(s.get('fmd-content').innerHTML, /native-diagram/);
});

function nativeAdapter() {
  const calls = [];
  const native = {version:1, diagnostics:[], failure:'',
    render(source, preview, display) {
      if (this.failure) throw Error(this.failure);
      calls.push({kind:'html', source, display});
      preview.innerHTML = '<p data-native="true">' + escape(source) + '</p>';
    },
    pdf(source) {
      if (this.failure) throw Error(this.failure);
      calls.push({kind:'pdf', source});
      return new TextEncoder().encode('%PDF-1.7\n' + source);
    }
  };
  return {native, calls};
}

test('native readiness renders current source and never calls the reduced parser, including undo', () => {
  const {native,calls}=nativeAdapter(); const s=setup({native});
  s.edit('typed while loading'); s.fire('fmd-native-ready');
  assert.equal(calls[0].source,'typed while loading'); assert.deepEqual(s.renders,[]);
  s.edit('# Original'); s.tick(150);
  assert.equal(calls[1].source,'# Original'); assert.match(s.get('fmd-content').innerHTML,/data-native/);
  assert.deepEqual(s.renders,[]);
});

test('native startup failure preserves source and preview, blocks stale exports, and leaves Markdown downloadable', async () => {
  const {native}=nativeAdapter();native.failure='Native renderer is loading';const s=setup({native});
  s.edit('never lose me');s.tick(150);s.click('btn-save-html');s.click('btn-export-pdf');
  assert.match(s.get('fmd-save-status').textContent,/loading/);assert.deepEqual(s.downloads,[]);
  assert.match(s.get('fmd-content').innerHTML,/native-diagram/);assert.deepEqual(s.printed,[]);assert.deepEqual(s.renders,[]);
  s.click('btn-save-markdown');assert.equal(await s.downloads[0].blob.text(),'never lose me');
  native.failure='';s.fire('fmd-native-ready');assert.match(s.get('fmd-content').innerHTML,/never lose me/);
  assert.ok(s.fire('beforeunload').prevented);
});

test('native PDF downloads contain the latest source before debounce, without browser printing', async () => {
  const {native,calls}=nativeAdapter();const s=setup({native});
  s.edit('superseded');s.edit('current PDF');s.click('btn-export-pdf');
  assert.deepEqual(calls,[{kind:'pdf',source:'current PDF'}]);
  assert.equal(s.downloads[0].filename,'Document.pdf');assert.equal(s.downloads[0].blob.type,'application/pdf');
  assert.equal(await s.downloads[0].blob.text(),'%PDF-1.7\ncurrent PDF');
  assert.deepEqual(s.printed,[]);assert.deepEqual(s.renders,[]);
});

test('native Ctrl/Cmd P uses the core exporter and suppresses repeated print requests', () => {
  const {native,calls}=nativeAdapter();const s=setup({native});
  assert.ok(s.fire('keydown',{key:'p',ctrlKey:true}).prevented);
  assert.ok(s.fire('keydown',{key:'P',metaKey:true,repeat:true}).prevented);
  assert.equal(calls.length,1);assert.equal(s.downloads[0].filename,'Document.pdf');assert.deepEqual(s.printed,[]);
});

test('native zoom and theme changes re-render unchanged source with explicit view settings', () => {
  const {native,calls}=nativeAdapter();const s=setup({native});s.fire('fmd-native-ready');
  s.click('btn-zoom-in');assert.equal(calls.at(-1).display.scale,1.1);
  s.click('btn-theme-toggle');assert.equal(calls.at(-1).display.theme,'dark');
  s.click('btn-theme-toggle');assert.equal(calls.at(-1).display.theme,'light');
  s.click('btn-zoom-reset');assert.equal(calls.at(-1).display.scale,1);
  assert.ok(calls.every(call=>call.source==='# Original'));assert.deepEqual(s.renders,[]);
});

test('native warnings remain visible after preview and PDF export, while runtime errors do not masquerade as fallback success', () => {
  const {native}=nativeAdapter();native.diagnostics=[{message:'Missing supplied image'}];const s=setup({native});
  s.fire('fmd-native-ready');assert.match(s.get('fmd-save-status').textContent,/Missing supplied image/);
  s.click('btn-export-pdf');assert.match(s.get('fmd-save-status').textContent,/Download started.*Missing supplied image/);
  s.fire('fmd-native-error',{detail:'Native runtime failed: bad binary'});
  assert.match(s.get('fmd-save-status').textContent,/bad binary/);assert.deepEqual(s.renders,[]);
});

test('readiness already settled before controller registration still renders edits made during startup', async () => {
  const {native,calls}=nativeAdapter();native.ready=Promise.resolve(true);const s=setup({native});
  s.edit('newest startup source');await native.ready;await Promise.resolve();
  assert.equal(calls[0].source,'newest startup source');assert.deepEqual(s.renders,[]);
});

function actualImageRenderer(code = renderer) {
  const context = vm.createContext({Map});
  vm.runInContext(code, context);
  return (source, bindings) => context.parseMarkdownClient(source, bindings);
}

// Read the fixed application-owned slots emitted by our native fixture and the
// detached DOM adapter. Source data is JSON, not textarea text or rendered HTML.
function readImageWorkspace(html) {
  const data = id => {
    const match = html.match(new RegExp('<script[^>]*\\bid="' + id + '"[^>]*>([\\s\\S]*?)</script>'));
    return match ? JSON.parse(match[1]) : null;
  };
  const source = data('fmd-raw-source');
  assert.equal(typeof source, 'string');
  const preview = html.match(/<div class="fmd-content" id="fmd-content">([\s\S]*?)<\/div>\n  <\/main>/)?.[1]
    ?? html.match(/<main>([\s\S]*?)<\/main>/)?.[1];
  assert.equal(typeof preview, 'string');
  return {source, imageAssets: data('fmd-image-assets'), initialPreview: preview};
}

function displayedImages(html) {
  return [...html.matchAll(/<img\b[^>]*>/g)].map(([tag]) => {
    const source = tag.match(/\bsrc="([^"]*)"/)?.[1];
    const data = source?.match(/^data:(image\/[^;]+);base64,([A-Za-z0-9+/]+={0,2})$/i);
    assert.ok(data, 'workspace fixture must display a self-contained image');
    return {mime: data[1].toLowerCase(), bytes: Buffer.from(data[2], 'base64').toString('hex'),
      alt: tag.match(/\balt="([^"]*)"/)?.[1]};
  });
}

async function verifyImageSaveCycles(fixture, parser) {
  const expected = displayedImages(fixture.initialPreview);
  const originalAssets = fixture.imageAssets;
  for (let generation = 1; generation <= 3; generation++) {
    const page = setup({...fixture, actualRenderer: parser});
    assert.deepEqual(displayedImages(page.get('fmd-content').innerHTML), expected);
    const source = fixture.source + '\n\nUnrelated paragraph ' + generation + '.\n';
    page.edit(source);
    page.click('btn-export-pdf'); // Lightweight printing flushes real rendering.
    assert.deepEqual(displayedImages(page.printed[0]), expected, 'images changed during an unrelated edit');
    page.click('btn-save-html');
    const saved = await page.downloads[0].blob.text();
    page.click('btn-save-markdown');
    assert.equal(await page.downloads[1].blob.text(), source, 'source encoding changed during display normalization');
    assert.equal(JSON.parse(page.get('fmd-raw-source').textContent), fixture.source, 'saving changed the live baseline');
    fixture = readImageWorkspace(saved);
    assert.equal(fixture.source, source);
    assert.deepEqual(fixture.imageAssets, originalAssets, 'saved bindings changed');
    assert.deepEqual(displayedImages(fixture.initialPreview), expected, 'saved preview changed images');
  }
}

test('encoded source images survive production edit, print, save and three reopen generations', async () => {
  const bytes = Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" width="16" height="8"><text>é中😀</text></svg>');
  const canonical = 'data:image/svg+xml;base64,' + bytes.toString('base64');
  const parser = actualImageRenderer();
  for (const destination of [
    'data:image/svg+xml,' + encodeURIComponent(bytes.toString()),
    'DATA:IMAGE/SVG+XML;CHARSET=UTF-8;BASE64,' + bytes.toString('base64'),
    'data:image/svg+xml;base64,' + encodeURIComponent(bytes.toString('base64')),
  ]) {
    await verifyImageSaveCycles({source: '# Portable\n\n![Plot](' + destination + ')', imageAssets: null,
      initialPreview: '<h1>Portable</h1><p><img src="' + canonical + '" alt="Plot"></p>'}, parser);
  }
});

test('explicit rejection and first binding precedence survive production saves and reopening', async () => {
  const destination = 'data:image/svg+xml;base64,' + Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"/>').toString('base64');
  const replacement = 'data:image/svg+xml;base64,' + Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><text>Replacement</text></svg>').toString('base64');
  const parser = actualImageRenderer();
  for (const [assets, expected] of [
    [[[destination, null]], '<p>Plot</p>'],
    [[[destination, null], [destination, replacement]], '<p>Plot</p>'],
    [[[destination, replacement], [destination, null]], '<p><img src="' + replacement + '" alt="Plot"></p>'],
  ]) {
    await verifyImageSaveCycles({source: '![Plot](' + destination + ')', imageAssets: assets,
      initialPreview: expected}, parser);
  }
});

test('an unused rejected binding remains rejected after saving, reopening and reinserting its source', async () => {
  const destination = 'data:image/svg+xml;base64,' + Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"/>').toString('base64');
  const parser = actualImageRenderer(), assets = [[destination, null]];
  const page = setup({source: '![Plot](' + destination + ')', imageAssets: assets,
    initialPreview: '<p>Plot</p>', actualRenderer: parser});
  page.edit('Images temporarily removed.'); page.click('btn-save-html');
  const reopened = readImageWorkspace(await page.downloads[0].blob.text());
  assert.deepEqual(reopened.imageAssets, assets);
  const next = setup({...reopened, actualRenderer: parser});
  next.edit('![Plot](' + destination + ')\n\nRestored reference.'); next.click('btn-save-html');
  assert.deepEqual(displayedImages(next.get('fmd-content').innerHTML), []);
  assert.deepEqual(readImageWorkspace(await next.downloads[0].blob.text()).imageAssets, assets);
});

// The Rust integration test supplies a freshly generated workspace on stdin.
// Execute its actual bundled renderer, not whichever JS happens to be in the
// checkout, then run the unchanged production controller through save/reopen.
if (process.argv.includes('--native-image-workspace')) {
  const html = readFileSync(0, 'utf8');
  test('native-generated image bindings and the bundled renderer retain their initial meaning through editing', async () => {
    const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
    assert.equal(typeof script, 'string');
    const end = script.indexOf('\n(function() {');
    assert.ok(end > 0, 'native workspace must contain renderer followed by controller');
    const embeddedController = script.slice(end + 1, end + 1 + controller.length);
    assert.equal(embeddedController, controller, 'fixture must use the same controller being executed');
    await verifyImageSaveCycles(readImageWorkspace(html), actualImageRenderer(script.slice(0, end)));
  });
}
