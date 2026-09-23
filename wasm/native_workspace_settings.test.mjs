import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {createNativeWorkspaceRenderer, bootNativeWorkspace} from './interactive_runtime.mjs';

const utf8 = new TextEncoder();
const plain = value => JSON.parse(JSON.stringify(value));
function fixture({page = true} = {}) {
  const payload = {version: 1, options: {font: 'serif', darkMode: 'auto', fontScale: 1.25,
    title: 'Original', author: 'Author', lang: 'fr', metadataEpochSeconds: 0,
    pageNumbers: false, codeLineNumbers: false, toc: false, tocDepth: 3},
    images: [{destination: 'chart.png', bytes: 'AQID'}],
    fonts: [{slot: 'body-regular', weight: 555, bytes: 'BAUG'}]};
  const calls = [], freed = [];
  let htmlFailure = null;
  const bindings = {};
  for (const method of ['renderHtmlConfiguredAdvanced', 'renderPdfConfiguredMulti', ...(page ? ['renderPdfConfiguredPage'] : [])]) {
    bindings[method] = (...args) => {
      calls.push({method, args});
      if (method.includes('Html') && htmlFailure) throw htmlFailure;
      const text = method.includes('Html') ? '<html><head></head><body>' + args[6] + '</body></html>' : '%PDF-ADAPTER\n';
      return {mimeType: method.includes('Html') ? 'text/html' : 'application/pdf', bytes: utf8.encode(text),
        diagnosticsJson: () => JSON.stringify([{message: String(args[3])}]), free() { freed.push(method); }};
    };
  }
  return {payload, bindings, calls, freed, failHtml(error) { htmlFailure = error; },
    renderer: createNativeWorkspaceRenderer(bindings, payload)};
}
const patch = {font: 'sans', darkMode: 'disabled', fontScale: 1.75, title: 'Revised',
  author: 'New author', lang: 'de', toc: true, tocDepth: 5, pageNumbers: true, codeLineNumbers: true,
  pageGeometry: [792, 612, 24, 36, 48, 60]};

// Real boot function + real empty WASM initialization; only the renderer ABI and
// DOM/URL facilities are adapters. Browser acceptance is tested separately.
async function withBoot(run, options) {
  const f = fixture(options), globals = ['document', 'window', 'URL'];
  const before = globals.map(key => Object.getOwnPropertyDescriptor(globalThis, key));
  let text = JSON.stringify({...f.payload, wasm: 'AGFzbQEAAAA=', bindings: 'trusted fixture'}), rejectWrite = false;
  const data = {get textContent() { return text; }, set textContent(value) {
    if (rejectWrite) { rejectWrite = false; throw Error('publish failed'); }
    text = value;
  }};
  const preview = {childNodes: [{old: true}], fail: false, replaceChildren(...children) {
    this.childNodes = children;
    if (this.fail) { this.fail = false; throw Error('preview failed'); }
  }};
  const revoked = [];
  globalThis.window = new EventTarget();
  globalThis.document = {querySelector() { return data; }, createElement(tag) {
    assert.equal(tag, 'iframe');
    return {style: {}, attributes: {}, setAttribute(key, value) { this.attributes[key] = value; }, addEventListener() {}};
  }};
  globalThis.URL = {createObjectURL() {
    return 'data:text/javascript,' + encodeURIComponent('export default async ({module_or_path}) => { await WebAssembly.instantiate(module_or_path); }');
  }, revokeObjectURL(url) { revoked.push(url); }};
  try {
    bootNativeWorkspace((_bindings, saved) => {
      f.renderer = createNativeWorkspaceRenderer(f.bindings, saved);
      return f.renderer;
    });
    const engine = window.__fmdNativeRuntime;
    assert.equal(engine.settings, null);
    assert.throws(() => engine.applySettings({}, '', preview), /loading/);
    assert.equal(await engine.ready, true);
    assert.equal(revoked.length, 1);
    await run({...f, engine, data, preview, rejectNextWrite() { rejectWrite = true; }});
  } finally {
    globals.forEach((key, i) => {
      if (before[i]) Object.defineProperty(globalThis, key, before[i]);
      else delete globalThis[key];
    });
  }
}

test('settings and caller geometry are owned immutable snapshots', () => {
  const f = fixture(), input = plain(patch);
  const tx = f.renderer.stageSettings(input);
  input.font = 'serif'; input.pageGeometry[0] = 1;
  assert.equal(tx.settings.font, 'sans'); assert.equal(tx.settings.pageGeometry[0], 792);
  assert.ok(Object.isFrozen(tx.settings)); assert.ok(Object.isFrozen(tx.settings.pageGeometry));
  assert.throws(() => { tx.settings.pageGeometry[0] = 1; }, TypeError);
  f.payload.options.font = 'sans';
  assert.equal(f.renderer.settings.font, 'serif', 'Initial caller options cannot mutate the engine');
  tx.commit(); assert.equal(f.renderer.settings, tx.settings);
});

test('staging and preview preflight do not mutate committed settings or diagnostics', () => {
  const f = fixture(); f.renderer.html('original');
  const before = plain(f.renderer.settings), diagnostics = f.renderer.diagnostics;
  const tx = f.renderer.stageSettings(patch);
  assert.match(tx.html('current source'), /1.75/);
  assert.deepEqual(plain(f.renderer.settings), before);
  assert.equal(f.renderer.diagnostics, diagnostics);
  f.renderer.pdf('before commit'); assert.equal(f.calls.at(-1).args[3], 'Original');
  tx.commit(); assert.equal(f.renderer.diagnostics[0].message, 'Revised');
});

test('committed typography, metadata, TOC and page settings reach exact HTML/PDF ABI positions', () => {
  const f = fixture(); f.renderer.stageSettings(patch).commit();
  f.renderer.html('latest\r\nsource', {scale: 1.2, theme: 'dark'});
  let a = f.calls.at(-1).args;
  assert.equal(a.length, 19);
  assert.deepEqual(a.slice(0, 7), ['latest\r\nsource', 'sans', 'auto', 'Revised', undefined, false, 2.1]);
  assert.deepEqual(a.slice(16), ['de', true, 5]);
  f.renderer.pdf('PDF now'); const call = f.calls.at(-1); a = call.args;
  assert.equal(call.method, 'renderPdfConfiguredPage'); assert.equal(a.length, 28);
  assert.deepEqual(a.slice(0, 8), ['PDF now', 'sans', 'disabled', 'Revised', 'New author', 0, false, true]);
  assert.deepEqual(a.slice(20, 27), [true, 1.75, 'de', true, 5, undefined, false]);
  assert.deepEqual([...a[27]], patch.pageGeometry);
  assert.deepEqual([...a[12]], []); assert.deepEqual([...a[11]], [4, 5, 6]);
  assert.equal(a[16][0], 555); assert.equal(f.freed.length, 2);
});

test('settings updates reuse image/font buffers and do not revoke asset identities', () => {
  const f = fixture(); f.renderer.pdf('before'); const a = f.calls.at(-1).args;
  f.renderer.stageSettings(patch).commit(); f.renderer.pdf('after'); const b = f.calls.at(-1).args;
  for (const i of [8, 9, 10, 11, 12, 13, 14, 15, 16]) assert.equal(b[i], a[i], 'ABI resource slot ' + i);
});

test('clearing page configuration restores legacy PDF dispatch without losing other settings', () => {
  const f = fixture(); f.renderer.stageSettings(patch).commit();
  f.renderer.stageSettings({pageGeometry: undefined, author: undefined}).commit(); f.renderer.pdf('x');
  assert.equal(f.calls.at(-1).method, 'renderPdfConfiguredMulti');
  assert.equal(f.calls.at(-1).args.length, 27); assert.equal(f.calls.at(-1).args[21], 1.75);
  assert.equal(Object.hasOwn(f.renderer.settings, 'pageGeometry'), false);
  assert.equal(Object.hasOwn(f.renderer.settings, 'author'), false);
});

test('old packages accept typography but reject explicit page settings before committing', () => {
  const f = fixture({page: false}); f.renderer.stageSettings({fontScale: 2, toc: true}).commit();
  assert.throws(() => f.renderer.stageSettings({pageGeometry: patch.pageGeometry}), error => error.code === 'UNSUPPORTED_WASM_PACKAGE');
  f.renderer.pdf('x'); assert.equal(f.calls.at(-1).method, 'renderPdfConfiguredMulti');
  assert.equal(f.renderer.settings.fontScale, 2); assert.equal(f.renderer.settings.toc, true);
});

test('patches cannot introduce unsupported unsafe options, accessors, symbols or inherited values', () => {
  const f = fixture(); let reads = 0;
  const accessor = Object.defineProperty({}, 'font', {get() { reads++; return 'sans'; }});
  for (const value of [null, [], new Date(), {allowRawHtml: true}, {css: 'body{}'}, {page: {}},
    Object.create({font: 'sans'}), {[Symbol('font')]: 'sans'}, accessor]) {
    assert.throws(() => f.renderer.stageSettings(value));
  }
  assert.equal(reads, 0); assert.equal(f.calls.length, 0);
});

test('font scale, enums, boolean flags and TOC bounds are validated before rendering', () => {
  const f = fixture();
  for (const value of [{font: 'mono'}, {font: undefined}, {darkMode: 'dark'}, {fontScale: NaN},
    {fontScale: Infinity}, {fontScale: 0.49}, {fontScale: 3.01}, {fontScale: '1'},
    {pageNumbers: 1}, {codeLineNumbers: 'false'}, {toc: null}, {tocDepth: 0}, {tocDepth: 7}, {tocDepth: 1.5}]) {
    assert.throws(() => f.renderer.stageSettings(value));
  }
  assert.equal(f.calls.length, 0); assert.equal(f.renderer.settings.fontScale, 1.25);
});

test('metadata limits count UTF-8 and reject invalid surrogate sequences', () => {
  const f = fixture();
  for (const value of [{title: 1}, {lang: '中'.repeat(342)}, {author: 'é'.repeat(32769)},
    {title: '\ud800'}, {lang: '\udfff'}, {metadataEpochSeconds: -1}, {metadataEpochSeconds: 0.5},
    {metadataEpochSeconds: Number.MAX_SAFE_INTEGER + 1}]) assert.throws(() => f.renderer.stageSettings(value));
  f.renderer.stageSettings({title: 'é'.repeat(32768), lang: '中'.repeat(341), metadataEpochSeconds: 0}).commit();
  assert.equal(f.renderer.settings.title.length, 32768); assert.equal(f.renderer.settings.metadataEpochSeconds, 0);
});

test('page geometry preserves the six-number contract and checks f32 content extents', () => {
  const f = fixture();
  const bad = [[144, 144, 72, 0, 72, 0], [143, 792, 0, 0, 0, 0], [612, 792, 0, -1, 0, 0],
    [14401, 792, 0, 0, 0, 0], [612, 792, 0, Infinity, 0, 0], [612, 792, 0, 0, 0],
    [144.000006, 144, 0, 0, 0, 72.000006]];
  assert.ok(bad.at(-1)[0] - bad.at(-1)[5] >= 72);
  for (const pageGeometry of bad) assert.throws(() => f.renderer.stageSettings({pageGeometry}), /geometry/);
  f.renderer.stageSettings({pageGeometry: [144, 144, 0, -0, 72, 72]}).commit();
  assert.deepEqual(f.renderer.settings.pageGeometry, [144, 144, 0, 0, 72, 72]);
});

test('page admission never calls element getters or iterators and refuses sparse arrays', () => {
  const f = fixture(); let reads = 0;
  const getter = [...patch.pageGeometry]; Object.defineProperty(getter, '1', {get() { reads++; return 612; }});
  const iterator = [...patch.pageGeometry]; iterator[Symbol.iterator] = () => { reads++; throw Error('iteration'); };
  for (const pageGeometry of [getter, iterator, new Array(6), new Float64Array(patch.pageGeometry), null]) {
    assert.throws(() => f.renderer.stageSettings({pageGeometry}), /geometry/);
  }
  assert.equal(reads, 0);
});

test('saved settings are revalidated on reopen, not trusted because they came from JSON', () => {
  for (const patch of [{fontScale: 0}, {tocDepth: 9}, {allowRawHtml: true}, {lang: '\ud800'}]) {
    const f = fixture(); Object.assign(f.payload.options, patch);
    assert.throws(() => createNativeWorkspaceRenderer(f.bindings, f.payload));
  }
});

test('settings persist through repeated serialization/reopening with exact resources and paper', () => {
  const f = fixture(); f.renderer.stageSettings(patch).commit();
  for (let i = 0; i < 3; i++) {
    const saved = plain(f.payload), renderer = createNativeWorkspaceRenderer(f.bindings, saved);
    assert.deepEqual(plain(renderer.settings), plain(f.renderer.settings));
    renderer.pdf('reopened'); const a = f.calls.at(-1).args;
    assert.deepEqual([...a[27]], patch.pageGeometry); assert.deepEqual([...a[9]], [1, 2, 3]);
    assert.equal(a[16][0], 555);
  }
});

test('two settings transactions cannot publish or roll back over a newer revision', () => {
  const f = fixture(), a = f.renderer.stageSettings({fontScale: 2}), b = f.renderer.stageSettings({fontScale: 3});
  a.commit(); let published = false;
  assert.throws(() => b.commit(() => { published = true; }), /Stale/); assert.equal(published, false);
  assert.throws(() => b.html('x'), /Stale/);
  const c = f.renderer.stageSettings({font: 'sans'}); c.commit();
  assert.throws(() => a.rollback(), /Stale/); assert.equal(f.renderer.settings.font, 'sans');
});

test('settings and image changes share a generation so saved JSON cannot overwrite newer resources', () => {
  const f = fixture(), addition = [{destination: 'new.png', bytes: Uint8Array.of(9)}];
  const image = f.renderer.stageImages(addition), settings = f.renderer.stageSettings({fontScale: 2});
  settings.commit(); assert.throws(() => image.commit(), /Stale/);
  const older = f.renderer.stageSettings({toc: true}); f.renderer.stageImages(addition).commit();
  assert.throws(() => older.commit(), /Stale/); assert.throws(() => settings.rollback(), /Stale/);
  f.renderer.stageSettings({toc: true}).commit(); f.renderer.pdf('x');
  assert.deepEqual(f.calls.at(-1).args[8], ['chart.png', 'new.png']);
});

test('failed publication leaves settings unchanged and a successful retry can be rolled back', () => {
  const f = fixture(), previous = plain(f.renderer.settings), tx = f.renderer.stageSettings(patch);
  assert.throws(() => tx.commit(() => { throw Error('storage'); }), /storage/);
  assert.deepEqual(plain(f.renderer.settings), previous);
  tx.commit(); assert.equal(f.renderer.settings.font, 'sans');
  assert.throws(() => tx.rollback(() => { throw Error('storage'); }), /storage/);
  assert.equal(f.renderer.settings.font, 'sans'); tx.rollback();
  assert.deepEqual(plain(f.renderer.settings), previous); assert.deepEqual(plain(f.payload.options), previous);
  assert.throws(() => tx.commit(), /Stale/); assert.throws(() => tx.rollback(), /Stale/);
});

test('publication callbacks cannot reenter a settings or image commit', () => {
  for (const outerKind of ['settings', 'images']) {
    const f = fixture(), image = f.renderer.stageImages([{destination: 'new.png', bytes: Uint8Array.of(9)}]);
    const settings = f.renderer.stageSettings({fontScale: 2});
    const [outer, inner] = outerKind === 'settings' ? [settings, image] : [image, settings];
    assert.throws(() => outer.commit(() => inner.commit()), /publication/);
    assert.equal(f.renderer.settings.fontScale, 1.25);
    f.renderer.pdf('x'); assert.deepEqual(f.calls.at(-1).args[8], ['chart.png']);
    outer.commit(); assert.throws(() => inner.commit(), /Stale/);
  }
});

test('preflight rendering failures preserve committed settings and diagnostics', () => {
  const f = fixture(); f.renderer.html('x'); const previous = f.renderer.diagnostics;
  const tx = f.renderer.stageSettings(patch); f.failHtml(Error('native error'));
  assert.throws(() => tx.html('x'), /native error/);
  assert.equal(f.renderer.settings.title, 'Original'); assert.equal(f.renderer.diagnostics, previous);
  f.failHtml(null); assert.match(tx.html('recovered'), /1.75/); tx.commit();
  assert.equal(f.renderer.settings.title, 'Revised');
});

test('rendered envelope errors free results without changing committed diagnostics', () => {
  const f = fixture(); f.renderer.html('x'); const previous = f.renderer.diagnostics;
  let frees = 0;
  f.bindings.renderHtmlConfiguredAdvanced = () => ({bytes: utf8.encode('missing head'), mimeType: 'text/html',
    diagnosticsJson: () => '[{"message":"candidate"}]', free() { frees++; }});
  assert.throws(() => f.renderer.stageSettings(patch).html('x'), /head/);
  assert.equal(frees, 1); assert.equal(f.renderer.diagnostics, previous);
});

test('serialized factory remains self-contained and does not expose a settings override through html', () => {
  const f = fixture(), factory = vm.runInNewContext('(' + createNativeWorkspaceRenderer.toString() + ')',
    {atob, btoa, Uint8Array, Uint32Array, Float64Array, TextEncoder, TextDecoder});
  const renderer = factory(f.bindings, f.payload); renderer.stageSettings(patch).commit();
  renderer.html('x', {}, {fontScale: 99}); assert.equal(f.calls.at(-1).args[6], 1.75);
  renderer.pdf('x'); assert.deepEqual([...f.calls.at(-1).args[27]], patch.pageGeometry);
  assert.doesNotMatch(createNativeWorkspaceRenderer.toString() + bootNativeWorkspace.toString(), /<\/script/i);
});

test('boot applies native settings, preview and escaped saved JSON in one operation', async () => {
  await withBoot(({engine, preview, data}) => {
    const title = '</script><script>bad()</script>\u2028&';
    const settings = engine.applySettings({...patch, title}, '\r\nCurrent source', preview, {scale: 1});
    assert.equal(settings, engine.settings); assert.equal(settings.title, title);
    assert.equal(JSON.parse(data.textContent).options.title, title);
    assert.doesNotMatch(data.textContent, /<\/script|\u2028/);
    assert.match(data.textContent, /\\u003c/);
    assert.match(preview.childNodes[0].srcdoc, /1.75/);
    assert.equal(preview.childNodes[0].attributes.sandbox, 'allow-same-origin');
    assert.match(preview.childNodes[0].srcdoc, /default-src 'none'/);
  });
});

test('boot fails atomically on native errors, persistence errors and DOM replacement failures', async () => {
  await withBoot(({engine, failHtml, preview, data, rejectNextWrite}) => {
    const before = data.textContent, nodes = preview.childNodes.slice(), settings = engine.settings;
    failHtml(Error('render failed'));
    assert.throws(() => engine.applySettings(patch, 'x', preview), /render failed/); failHtml(null);
    assert.equal(data.textContent, before); assert.deepEqual(preview.childNodes, nodes);
    rejectNextWrite(); assert.throws(() => engine.applySettings(patch, 'x', preview), /publish failed/);
    assert.equal(data.textContent, before); assert.equal(engine.settings, settings);
    preview.fail = true; assert.throws(() => engine.applySettings(patch, 'x', preview), /preview failed/);
    assert.equal(data.textContent, before); assert.deepEqual(preview.childNodes, nodes); assert.equal(engine.settings, settings);
    engine.applySettings(patch, 'recovered', preview); assert.equal(engine.settings.fontScale, 1.75);
  });
});

test('boot settings publication preserves imported resources and invalidates stale image publication', async () => {
  await withBoot(({engine, preview, data}) => {
    const add = [{destination: 'new.png', bytes: Uint8Array.of(9)}];
    const stale = engine.stageImages(add); engine.applySettings({fontScale: 2}, 'x', preview);
    assert.throws(() => stale.commit(), /Stale/);
    engine.stageImages(add).commit(); engine.applySettings({toc: true}, 'x', preview);
    const saved = JSON.parse(data.textContent);
    assert.deepEqual(saved.images, [{destination: 'chart.png', bytes: 'AQID'}, {destination: 'new.png', bytes: 'CQ=='}]);
    assert.equal(saved.options.fontScale, 2); assert.equal(saved.options.toc, true);
    assert.equal(saved.fonts[0].weight, 555);
  });
});
