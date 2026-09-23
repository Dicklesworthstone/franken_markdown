import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {bootNativeWorkspace, createNativeWorkspaceRenderer} from './interactive_runtime.mjs';

// Actual runtime and bootstrap, with explicit native ABI/DOM/URL adapters.
// Bootstrap really instantiates an empty WASM module; this is not Rust rendering.
const utf8 = new TextEncoder();
const escape = text => String(text).replace(/[&<>"']/g, ch => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
function fixture() {
  const payload = {version: 1, options: {font: 'serif', darkMode: 'auto', fontScale: 1.25,
    title: 'Published title', lang: 'fr', toc: true, tocDepth: 4, author: 'Retained PDF author', metadataEpochSeconds: 0},
    images: [{destination: 'plot.png', bytes: 'AQID'}, {destination: 'unused.svg', bytes: 'BAU='}],
    fonts: [{slot: 'body-regular', weight: 550, bytes: 'BgcI'}, {slot: 'mono-regular', weight: 430, bytes: 'CQo='}],
    wasm: 'AGFzbQEAAAA=', bindings: 'SAVED TRUSTED BINDINGS SENTINEL'};
  const calls = [], freed = [], outputs = [];
  const bindings = {
    renderHtmlConfiguredAdvanced(...args) {
      calls.push(args);
      const result = {mimeType: 'text/html;charset=utf-8',
        bytes: utf8.encode('<!DOCTYPE html><html lang="' + escape(args[16]) + '"><head><title>' + escape(args[3])
          + '</title><style>@media (prefers-color-scheme: dark) { body { color: white } }</style></head><body>'
          + '<main><p>' + escape(args[0]) + '</p></main></body></html>'),
        diagnosticsJson: () => '[{"code":"fixture_diagnostic","message":"ABI adapter warning"}]',
        free() { freed.push(result); result.bytes.fill(0); }};
      outputs.push(result); return result;
    },
    renderPdfConfiguredMulti(...args) { calls.push(args); return {mimeType: 'application/pdf', bytes: utf8.encode('%PDF-ADAPTER'), diagnosticsJson: () => '[]', free() {}}; },
  };
  return {payload, bindings, calls, freed, outputs};
}

async function withBoot(run, {failStartup = false} = {}) {
  const f = fixture(), globals = ['document', 'window', 'URL'];
  const descriptors = globals.map(key => Object.getOwnPropertyDescriptor(globalThis, key));
  const data = {textContent: JSON.stringify(f.payload)};
  const created = [], revoked = [];
  const preview = {childNodes: [{original: true}], replaceChildren(...children) { this.childNodes = children; }};
  globalThis.window = new EventTarget();
  globalThis.document = {querySelector() { return data; }, createElement(tag) {
    const node = {tag, style: {}, setAttribute() {}, addEventListener() {}}; created.push(node); return node;
  }};
  globalThis.URL = {createObjectURL() {
    const body = failStartup ? "throw Error('startup refused')" : 'await WebAssembly.instantiate(module_or_path)';
    return 'data:text/javascript,' + encodeURIComponent('export default async ({module_or_path}) => {' + body + ';}');
  }, revokeObjectURL(url) { revoked.push(url); }};
  let saved, renderer;
  try {
    bootNativeWorkspace((_module, payload) => { saved = payload; renderer = createNativeWorkspaceRenderer(f.bindings, payload); return renderer; });
    const engine = window.__fmdNativeRuntime;
    assert.throws(() => engine.html('not yet'), /loading/);
    const ready = await engine.ready;
    assert.equal(ready, !failStartup); assert.equal(revoked.length, 1);
    await run({...f, engine, renderer, saved, data, created, preview});
  } finally {
    globals.forEach((key, index) => {
      if (descriptors[index]) Object.defineProperty(globalThis, key, descriptors[index]); else delete globalThis[key];
    });
  }
}

test('publication is a full native document with its offline policy, not a workspace snapshot', async () => {
  await withBoot(({engine, data, created, calls, freed}) => {
    const before = data.textContent, source = '\ufeff\r\n# Current é中😀\r\n';
    const html = engine.html(source);
    assert.match(html, /^<!DOCTYPE html><html lang="fr"><head><meta http-equiv="Content-Security-Policy"/);
    assert.match(html, /default-src 'none'; img-src data:; font-src data:/);
    assert.match(html, /<title>Published title<\/title>/);
    assert.doesNotMatch(html, /fmd-native-runtime|fmd-raw-source|fmd-editor|SAVED TRUSTED BINDINGS SENTINEL|AGFzbQEAAAA=/);
    assert.equal(calls[0][0], source); assert.equal(data.textContent, before); assert.equal(created.length, 0);
    assert.equal(freed.length, 1); assert.ok(freed[0].bytes.every(byte => byte === 0));
    assert.match(html, /# Current é中😀/, 'Document owns decoded output before the handle is freed');
  });
});

test('publication uses all nineteen advanced HTML ABI arguments and exact resource views', async () => {
  await withBoot(({engine, calls}) => {
    engine.html('![plot](plot.png)'); const a = calls[0];
    assert.equal(a.length, 19);
    assert.deepEqual(a.slice(0, 7), ['![plot](plot.png)', 'serif', 'auto', 'Published title', undefined, false, 1.25]);
    assert.deepEqual(a.slice(7, 12).map(bytes => [...bytes]), [[6, 7, 8], [], [], [], [9, 10]]);
    assert.deepEqual([...a[12]], [550, 0, 0, 0, 430]);
    assert.deepEqual(a[13], ['plot.png', 'unused.svg']); assert.deepEqual([...a[14]], [1, 2, 3, 4, 5]);
    assert.deepEqual([...a[15]], [3, 2]); assert.deepEqual(a.slice(16), ['fr', true, 4]);
  });
});

test('publishing is independent of preview zoom, forced theme and stale rendered source', async () => {
  await withBoot(({engine, calls, preview, created}) => {
    engine.render('older preview', preview, {scale: 2, theme: 'dark'});
    const nodes = preview.childNodes;
    assert.match(nodes[0].srcdoc, /@media all/); assert.equal(calls[0][6], 2.5);
    const html = engine.html('latest source', {scale: 3, theme: 'dark'});
    assert.equal(calls[1][0], 'latest source'); assert.equal(calls[1][6], 1.25); assert.equal(calls[1][2], 'auto');
    assert.match(html, /@media \(prefers-color-scheme: dark\)/); assert.doesNotMatch(html, /@media all/);
    assert.equal(preview.childNodes, nodes); assert.equal(created.length, 1);
  });
});

test('applied document settings reach publications while staged settings remain unapplied', async () => {
  await withBoot(({engine, renderer, preview, calls, data}) => {
    engine.applySettings({font: 'sans', fontScale: 1.75, title: 'Changed', lang: 'de', tocDepth: 2, darkMode: 'disabled'}, 'x', preview);
    const pending = renderer.stageSettings({fontScale: 3, title: 'Unapplied draft'});
    const before = data.textContent;
    engine.html('publish'); const a = calls.at(-1);
    assert.deepEqual([a[1], a[2], a[3], a[6], a[16], a[17], a[18]], ['sans', 'disabled', 'Changed', 1.75, 'de', true, 2]);
    assert.equal(data.textContent, before);
    pending.commit(); assert.equal(engine.settings.title, 'Unapplied draft', 'Rendering does not mutate transaction generation');
  });
});

test('committed image additions are reusable in publishing without repacking or consuming undo transactions', async () => {
  await withBoot(({engine, calls, data}) => {
    const transaction = engine.stageImages([{destination: 'imported.png', bytes: Uint8Array.of(11, 12)}]);
    transaction.commit(); const before = data.textContent;
    engine.html('![new](imported.png)'); const a = calls.at(-1);
    assert.deepEqual(a[13], ['plot.png', 'unused.svg', 'imported.png']);
    assert.deepEqual([...a[14]], [1, 2, 3, 4, 5, 11, 12]);
    engine.html('again'); const b = calls.at(-1);
    for (const index of [7, 8, 9, 10, 11, 12, 13, 14, 15]) assert.equal(b[index], a[index]);
    assert.equal(data.textContent, before); transaction.rollback();
    engine.html('restored'); assert.deepEqual(calls.at(-1)[13], ['plot.png', 'unused.svg']);
  });
});

test('publishing reports native diagnostics, preserves escaped source and never invokes a second parser', async () => {
  await withBoot(({engine}) => {
    const html = engine.html('</script><script>alert(1)</script>');
    assert.doesNotMatch(html, /<script/i); assert.match(html, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
    assert.equal(engine.diagnostics[0].code, 'fixture_diagnostic');
  });
});

test('startup failure explicitly blocks publishing rather than using the old preview', async () => {
  await withBoot(({engine, calls, created}) => {
    assert.throws(() => engine.html('source'), /Native runtime failed: startup refused/);
    assert.equal(calls.length, 0); assert.equal(created.length, 0);
  }, {failStartup: true});
});

test('native exceptions preserve serialized state and the current preview', async () => {
  await withBoot(({engine, bindings, preview, data, created}) => {
    const before = data.textContent, nodes = preview.childNodes;
    bindings.renderHtmlConfiguredAdvanced = () => { throw Error('Native renderer failed'); };
    assert.throws(() => engine.html('new revision'), /Native renderer failed/);
    assert.equal(data.textContent, before); assert.equal(preview.childNodes, nodes); assert.equal(created.length, 0);
  });
});

test('invalid native envelopes, encoding and document heads are rejected and freed', async () => {
  await withBoot(({engine, bindings, data, created}) => {
    const before = data.textContent;
    for (const kind of ['mime', 'bytes', 'empty', 'diagnostics', 'utf8', 'head']) {
      let frees = 0;
      const result = {mimeType: 'text/html', bytes: utf8.encode('<html><head></head></html>'),
        diagnosticsJson: () => '[]', free() { frees++; }};
      if (kind === 'mime') result.mimeType = 'application/pdf';
      if (kind === 'bytes') result.bytes = 'not bytes';
      if (kind === 'empty') result.bytes = new Uint8Array();
      if (kind === 'diagnostics') result.diagnosticsJson = () => '{}';
      if (kind === 'utf8') result.bytes = Uint8Array.of(255);
      if (kind === 'head') result.bytes = utf8.encode('<p>not a complete document</p>');
      bindings.renderHtmlConfiguredAdvanced = () => result;
      assert.throws(() => engine.html('x'), undefined, kind); assert.equal(frees, 1, kind);
    }
    assert.equal(data.textContent, before); assert.equal(created.length, 0);
  });
});

test('unpaired editor surrogates are refused before HTML, PDF or staged preview ABI dispatch', () => {
  const f = fixture(), renderer = createNativeWorkspaceRenderer(f.bindings, f.payload);
  for (const text of ['\ud800', '\udc00', 'a\udbffb', '\udc00\ud800']) {
    assert.throws(() => renderer.html(text), /unpaired surrogate/);
    assert.throws(() => renderer.pdf(text), /unpaired surrogate/);
    assert.throws(() => renderer.stageSettings({title: 'candidate'}).html(text), /unpaired surrogate/);
  }
  assert.equal(f.calls.length, 0);
});

test('source admission bounds actual UTF-8 bytes and preserves valid non-BMP pairs', () => {
  const f = fixture(), renderer = createNativeWorkspaceRenderer(f.bindings, f.payload);
  for (const value of [undefined, null, 123, 'x'.repeat(32 * 1024 * 1024 + 1), '😀'.repeat(8 * 1024 * 1024 + 1)]) {
    assert.throws(() => renderer.html(value), /32 MiB/);
  }
  assert.equal(f.calls.length, 0);
  renderer.html('\ufeff\r\n😀é中'); assert.equal(f.calls[0][0], '\ufeff\r\n😀é中');
});

test('updated source admission and publishing remain self-contained when serialized into a workspace', () => {
  const f = fixture();
  const factory = vm.runInNewContext('(' + createNativeWorkspaceRenderer.toString() + ')',
    {atob, btoa, Uint8Array, Uint32Array, TextDecoder, TextEncoder});
  const renderer = factory(f.bindings, f.payload);
  assert.match(renderer.html('😀'), /😀/); assert.throws(() => renderer.html('\ud800'), /unpaired surrogate/);
  assert.doesNotMatch(bootNativeWorkspace.toString() + createNativeWorkspaceRenderer.toString(), /<\/script/i);
});
