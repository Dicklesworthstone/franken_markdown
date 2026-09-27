// Execute the production resource transaction and all four ABI projections.
// The ABI adapter records arguments; it is not a font parser or real WASM build.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';

const b64 = bytes => Buffer.from(bytes).toString('base64');
const initial = () => ({version: 1, options: {font: 'serif', darkMode: 'auto', fontScale: 1.25,
  pageNumbers: true, codeLineNumbers: true, toc: true, tocDepth: 4, title: 'Document', lang: 'fr'},
  images: [{destination: 'plot.png', bytes: b64([4, 5, 6])}],
  fonts: [{slot: 'body-regular', bytes: b64([1, 2, 3]), weight: 450},
    {slot: 'mono-regular', bytes: b64([7, 8, 9])}]});
function fixture(payload = initial()) {
  const calls = []; let freed = 0;
  const result = (mimeType, prefix = '') => ({mimeType, bytes: new TextEncoder().encode(prefix + 'output'),
    diagnosticsJson: () => '[]', free() { freed++; }});
  const record = (format, mime, prefix) => (...args) => {
    calls.push({format, args}); return result(mime, prefix);
  };
  const bindings = {
    renderHtmlConfiguredAdvanced: (...args) => {
      calls.push({format: 'html', args}); return result('text/html', '<!doctype html><html><head></head><body>');
    },
    renderPdfConfiguredMulti: record('pdf', 'application/pdf', '%PDF-'),
    renderPdfConfiguredPage: record('pdf-page', 'application/pdf', '%PDF-'),
    renderEpubConfiguredAdvanced: record('epub', 'application/epub+zip'),
    renderSvgConfiguredResources: record('svg', 'image/svg+xml'),
  };
  return {payload, calls, bindings, renderer: createNativeWorkspaceRenderer(bindings, payload), get freed() { return freed; }};
}
function renderAll(f) {
  f.calls.length = 0;
  f.renderer.html('source'); f.renderer.pdf('source'); f.renderer.epub('source'); f.renderer.svg('source');
  return f.calls.map(({format, args}) => {
    const index = ({html: 7, pdf: 11, 'pdf-page': 11, epub: 12, svg: 8})[format];
    return {format, source: args[0], fonts: args.slice(index, index + 5).map(x => Array.from(x)), weights: Array.from(args[index + 5])};
  });
}
const fontAt = (renderer, slot) => renderer.fontSlots.find(x => x.slot === slot);

test('stage leaves live bytes, settings, images, summaries and saved data untouched', () => {
  const f = fixture(), before = JSON.stringify(f.payload), slots = f.renderer.fontSlots, settings = f.renderer.settings;
  const old = renderAll(f);
  const stage = f.renderer.stageFonts([{slot: 'body-regular', bytes: new Uint8Array([11, 12]), weight: 625}]);
  assert.equal(stage.changed, true);
  assert.equal(JSON.stringify(f.payload), before);
  assert.equal(f.renderer.fontSlots, slots); assert.equal(f.renderer.settings, settings);
  assert.deepEqual(renderAll(f), old);
  assert.deepEqual(stage.fonts, [{slot: 'body-regular', bytes: b64([11, 12]), weight: 625},
    {slot: 'mono-regular', bytes: b64([7, 8, 9])}]);
});

test('one font commit reaches HTML/PDF/EPUB/SVG and retains untouched slots', () => {
  const f = fixture(), settings = f.renderer.settings, images = f.payload.images;
  const stage = f.renderer.stageFonts([{slot: 'body-regular', bytes: new Uint8Array([11, 12]), weight: 625},
    {slot: 'body-bold', bytes: new Uint8Array([22]), weight: 750}]);
  stage.commit();
  for (const call of renderAll(f)) {
    assert.deepEqual(call.fonts, [[11, 12], [22], [], [], [7, 8, 9]], call.format);
    assert.deepEqual(call.weights, [625, 750, 0, 0, 0], call.format);
    assert.equal(call.source, 'source');
  }
  assert.equal(f.renderer.settings, settings); assert.equal(f.payload.images, images);
  assert.equal(f.payload.fonts, stage.fonts);
  assert.ok(Object.isFrozen(f.renderer.fontSlots)); assert.ok(Object.isFrozen(f.renderer.fontSlots[0]));
  assert.equal(f.freed, 4);
});

test('PDF custom geometry ABI retains new fonts and prior paper settings', () => {
  const p = initial(); p.options.pageGeometry = [612, 792, 40, 40, 40, 40];
  const f = fixture(p);
  f.renderer.stageFonts([{slot: 'body-bold-italic', bytes: new Uint8Array([41]), weight: 675}]).commit();
  const calls = renderAll(f), pdf = calls.find(x => x.format === 'pdf-page');
  assert.deepEqual(pdf.fonts[3], [41]); assert.equal(pdf.weights[3], 675);
  assert.deepEqual(Array.from(f.calls.find(x => x.format === 'pdf-page').args.at(-1)), p.options.pageGeometry);
});

test('clearing slots releases bytes and weights; omitted slots stay embedded', () => {
  const f = fixture();
  f.renderer.stageFonts([{slot: 'body-regular', clear: true}]).commit();
  for (const call of renderAll(f)) {
    assert.deepEqual(call.fonts, [[], [], [], [], [7, 8, 9]]);
    assert.deepEqual(call.weights, [0, 0, 0, 0, 0]);
  }
  assert.equal(fontAt(f.renderer, 'body-regular').bytes, 0);
  assert.equal(f.payload.fonts.length, 1);
});

test('weight-only updates reuse exact owned font bytes and can reset the pin', () => {
  const f = fixture();
  const bytes = f.payload.fonts[0].bytes;
  f.renderer.stageFonts([{slot: 'body-regular', weight: 800}]).commit();
  assert.equal(f.payload.fonts[0].bytes, bytes);
  assert.equal(renderAll(f)[0].weights[0], 800);
  f.renderer.stageFonts([{slot: 'body-regular', weight: undefined}]).commit();
  assert.equal(renderAll(f)[0].weights[0], 0);
  assert.ok(!Object.hasOwn(f.payload.fonts[0], 'weight'));
  assert.throws(() => f.renderer.stageFonts([{slot: 'body-bold', weight: 700}]), /Supply font bytes/);
});

test('new byte imports default to native weight rather than accidentally retaining the old pin', () => {
  const f = fixture();
  f.renderer.stageFonts([{slot: 'body-regular', bytes: new Uint8Array([90])}]).commit();
  assert.deepEqual(renderAll(f)[0].weights, [0, 0, 0, 0, 0]);
});

test('staged private byte views survive caller mutation and respect buffer offsets', () => {
  for (const wrap of [b => b.subarray(1, 3), b => new DataView(b.buffer, 1, 2), b => Buffer.from(b.buffer, 1, 2)]) {
    const f = fixture(), buffer = new Uint8Array([99, 11, 12, 99]);
    const view = wrap(buffer);
    const stage = f.renderer.stageFonts([{slot: 'body-bold', bytes: view}]);
    buffer.fill(88); stage.commit();
    assert.deepEqual(renderAll(f)[0].fonts[1], [11, 12]);
    assert.equal(f.payload.fonts.find(x => x.slot === 'body-bold').bytes, b64([11, 12]));
  }
});

test('intrinsic view properties ignore spoofed getters without invoking them', () => {
  for (const view of [new Uint8Array([31, 32]), new DataView(new Uint8Array([31, 32]).buffer)]) {
    for (const key of ['buffer', 'byteOffset', 'byteLength']) Object.defineProperty(view, key,
      {get() { assert.fail('must not invoke shadowed ' + key); }});
    const f = fixture();
    f.renderer.stageFonts([{slot: 'body-bold', bytes: view}]).commit();
    assert.deepEqual(renderAll(f)[0].fonts[1], [31, 32]);
  }
});

test('rollback restores old ABI bytes, settings and the exact previous font summary', () => {
  const f = fixture(), before = JSON.stringify(f.payload), info = f.renderer.fontSlots, settings = f.renderer.settings;
  const old = renderAll(f), stage = f.renderer.stageFonts([{slot: 'mono-regular', clear: true}]);
  stage.commit(); stage.rollback();
  assert.equal(JSON.stringify(f.payload), before);
  assert.equal(f.renderer.fontSlots, info); assert.equal(f.renderer.settings, settings);
  assert.deepEqual(renderAll(f), old);
  assert.throws(() => stage.rollback(), /Stale/); assert.throws(() => stage.commit(), /Stale/);
});

test('publication failures leave the transaction retryable and live data unchanged', () => {
  const f = fixture(), before = JSON.stringify(f.payload), info = f.renderer.fontSlots;
  const stage = f.renderer.stageFonts([{slot: 'body-regular', clear: true}]);
  assert.throws(() => stage.commit(() => { throw Error('storage refused'); }), /storage refused/);
  assert.equal(JSON.stringify(f.payload), before); assert.equal(f.renderer.fontSlots, info);
  stage.commit();
  assert.throws(() => stage.rollback(() => { throw Error('restore refused'); }), /restore refused/);
  assert.equal(fontAt(f.renderer, 'body-regular').bytes, 0);
  stage.rollback(); assert.equal(f.renderer.fontSlots, info);
});

test('font/settings/image transactions invalidate one another across commits and rollbacks', () => {
  for (const change of [f => f.renderer.stageSettings({fontScale: 1.5}),
    f => f.renderer.stageImages([{destination: 'new-' + f.payload.images.length + '.png', bytes: new Uint8Array([42])}]),
    f => f.renderer.stageFonts([{slot: 'body-regular', clear: true}])]) {
    const f = fixture(), first = f.renderer.stageFonts([{slot: 'body-bold', bytes: new Uint8Array([32])}]);
    change(f).commit(); assert.throws(() => first.commit(), /Stale/);
    const second = f.renderer.stageFonts([{slot: 'body-bold', bytes: new Uint8Array([32])}]);
    second.commit(); change(f).commit(); assert.throws(() => second.rollback(), /Stale/);
    const image = f.renderer.stageImages([{destination: 'later.png', bytes: new Uint8Array([9])}]);
    f.renderer.stageFonts([{slot: 'body-bold-italic', bytes: new Uint8Array([33])}]).commit();
    assert.throws(() => image.commit(), /Stale/);
  }
});

test('nested publication cannot sneak in a competing state switch', () => {
  const f = fixture(), stage = f.renderer.stageFonts([{slot: 'mono-regular', clear: true}]);
  const other = f.renderer.stageSettings({fontScale: 2});
  assert.throws(() => stage.commit(() => other.commit()), /publication.*progress/);
  assert.equal(fontAt(f.renderer, 'mono-regular').bytes, 3);
  stage.commit(); assert.throws(() => other.commit(), /Stale/);
});

test('no-op detection compares exact bytes, weight and clearing an unused slot', () => {
  const f = fixture();
  for (const patch of [[{slot: 'body-regular', bytes: new Uint8Array([1, 2, 3]), weight: 450}],
    [{slot: 'body-regular', weight: 450}], [{slot: 'body-bold', clear: true}]]) {
    assert.equal(f.renderer.stageFonts(patch).changed, false);
  }
  assert.equal(f.renderer.stageFonts([{slot: 'body-regular', bytes: new Uint8Array([1, 2, 3])}]).changed, true);
});

test('resource order is canonical regardless of patch order, and reopens identically', () => {
  const f = fixture();
  f.renderer.stageFonts([{slot: 'body-bold-italic', bytes: new Uint8Array([3])},
    {slot: 'body-bold', bytes: new Uint8Array([2])}, {slot: 'body-italic', bytes: new Uint8Array([1])}]).commit();
  assert.deepEqual(f.payload.fonts.map(x => x.slot), ['body-regular', 'body-bold', 'body-italic', 'body-bold-italic', 'mono-regular']);
  const reopened = fixture(JSON.parse(JSON.stringify(f.payload)));
  assert.deepEqual(renderAll(reopened), renderAll(f));
  assert.deepEqual(reopened.renderer.fontSlots, f.renderer.fontSlots);
});

test('malformed slots, weights and ambiguous operations reject before mutation', () => {
  const f = fixture(), before = JSON.stringify(f.payload);
  for (const patches of [[], null, {}, [null], ['body-regular'], [{slot: 'bad', clear: true}],
    [{slot: 'body-bold', clear: true}, {slot: 'body-bold', clear: true}],
    [{slot: 'body-bold', clear: false}], [{slot: 'body-bold', clear: true, bytes: new Uint8Array([1])}],
    [{slot: 'body-bold', clear: true, weight: 700}], [{slot: 'body-bold', unsupported: 1}],
    [{slot: 'body-bold'}], [{slot: 'body-bold', bytes: new Uint8Array()}],
    Array(6).fill({slot: 'body-bold', clear: true})]) {
    assert.throws(() => f.renderer.stageFonts(patches));
  }
  for (const weight of [0, -1, 1001, 1.5, NaN, Infinity, '400', null]) {
    assert.throws(() => f.renderer.stageFonts([{slot: 'body-regular', weight}]), /weight/);
  }
  assert.equal(JSON.stringify(f.payload), before);
});

test('accessors, inherited fields, symbols and sparse patches are rejected without reading field getters', () => {
  const f = fixture();
  const accessor = {slot: 'body-regular', get bytes() { assert.fail('accessor invoked'); }};
  const indexAccessor = []; Object.defineProperty(indexAccessor, '0', {get() { assert.fail('index accessor invoked'); }});
  const inherited = Object.create({slot: 'body-bold'}); inherited.bytes = new Uint8Array([1]);
  const symbol = {slot: 'body-regular', clear: true, [Symbol()]: 1};
  for (const patches of [[accessor], indexAccessor, Array(1), [inherited], [symbol]]) assert.throws(() => f.renderer.stageFonts(patches));
});

test('shared, detached and fake buffers are rejected; ordinary ArrayBuffer is accepted', () => {
  const f = fixture(), detached = new Uint8Array([1]);
  structuredClone(detached.buffer, {transfer: [detached.buffer]});
  for (const bytes of [new SharedArrayBuffer(4), new Uint8Array(new SharedArrayBuffer(4)), detached,
    {buffer: new ArrayBuffer(4), byteOffset: 0, byteLength: 4}, [1, 2], null]) {
    assert.throws(() => f.renderer.stageFonts([{slot: 'body-bold', bytes}]));
  }
  f.renderer.stageFonts([{slot: 'body-bold', bytes: new Uint8Array([3, 2, 1]).buffer}]).commit();
  assert.deepEqual(renderAll(f)[0].fonts[1], [3, 2, 1]);
});

test('source Unicode and resource-independent options remain enforced after font updates', () => {
  const f = fixture(); f.renderer.stageFonts([{slot: 'body-bold', bytes: new Uint8Array([12])}]).commit();
  for (const kind of ['html', 'pdf', 'epub', 'svg']) assert.throws(() => f.renderer[kind]('\ud800'), /surrogate/);
  assert.throws(() => f.renderer.stageSettings({allowRawHtml: true}), /Unsupported/);
  assert.equal(f.renderer.settings.fontScale, 1.25);
});

test('per-font size bound rejects before encoding and preserves live slots', () => {
  const f = fixture(), before = f.renderer.fontSlots;
  assert.throws(() => f.renderer.stageFonts([{slot: 'body-regular', bytes: new Uint8Array(32 * 1024 * 1024 + 1)}]), /32 MiB/);
  assert.equal(f.renderer.fontSlots, before);
});

test('large total font admission runs before copying, using the resulting set not the replaced set', () => {
  const f = fixture(), big = new Uint8Array(32 * 1024 * 1024);
  assert.throws(() => f.renderer.stageFonts(['body-regular','body-bold','body-italic','body-bold-italic']
    .map(slot => ({slot, bytes: big}))), /resource budget/);
  assert.equal(fontAt(f.renderer, 'body-regular').bytes, 3);
  // Small replacements repeated many times must not leak lifetime byte credit.
  for (let i = 0; i < 100; i++) f.renderer.stageFonts([{slot: 'body-regular', bytes: new Uint8Array(1024)}]).commit();
  assert.equal(fontAt(f.renderer, 'body-regular').bytes, 1024);
  f.renderer.stageFonts([{slot: 'body-regular', clear: true}]).commit();
  assert.equal(fontAt(f.renderer, 'body-regular').bytes, 0);
});

test('invalid saved weight pins cannot silently wrap through Uint32 conversion', () => {
  for (const weight of [-1, 0, 1001, 0.5, Infinity, '400']) {
    const p = initial(); p.fonts[0].weight = weight;
    assert.throws(() => fixture(p), /Font weight/);
  }
});
