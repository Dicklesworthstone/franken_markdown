import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import {createNativeSourceProjectCodec, NATIVE_SOURCE_PROJECT_LIMITS as L,
  readNativeSourceProject} from './native_source_project.mjs';

const codec = createNativeSourceProjectCodec();
const b64 = bytes => Buffer.from(bytes).toString('base64');
const payload = (patch = {}) => ({source: '# Original\n', options: {}, images: [], fonts: [], ...patch});
const serialize = data => codec.encode(data).text();
const fails = (fn, code = 'INVALID_SOURCE_PROJECT') => assert.throws(fn, error => error.code === code);
const reduced = options => createNativeSourceProjectCodec(options);

test('lossless data-only project restores source, settings, images, fonts and exact paper', () => {
  const source = '\uFEFF# Café\r\n\r\n![图](fmd-import/a.png)\r\n\rLast line 🧪';
  const settings = {font: 'serif', darkMode: 'disabled', fontScale: 1.125, title: '</script>\nTitle',
    author: 'Author', lang: 'nl', metadataEpochSeconds: 0, pageNumbers: true,
    codeLineNumbers: true, toc: true, tocDepth: 6, pageGeometry: [595.123, 841.456, 20.1, 21.2, 22.3, 23.4]};
  const input = payload({source, options: settings,
    images: [{destination: 'fmd-import/a.png', bytes: b64([0, 128, 255])},
      {destination: 'https://example.invalid/never-fetch', bytes: b64([3, 2, 1])}],
    fonts: [{slot: 'mono-regular', bytes: b64([5, 6]), weight: 525},
      {slot: 'body-regular', bytes: b64([7, 8, 9])}]});
  const output = codec.encode(input), prepared = codec.decode(output.bytes);
  assert.equal(output.sourceLength, Buffer.byteLength(source));
  assert.equal(prepared.source, source);
  assert.deepEqual(prepared.options, settings);
  assert.deepEqual(prepared.fonts.map(font => font.slot), ['body-regular', 'mono-regular']);
  assert(!Object.hasOwn(prepared, 'wasm'));
  assert(!Object.hasOwn(prepared, 'bindings'));
  const restored = readNativeSourceProject(output.bytes);
  assert.equal(restored.source, source);
  assert.equal(restored.options.allowRawHtml, false);
  assert.equal(restored.options.metadataEpochSeconds, 0);
  assert.deepEqual(restored.options.page, {size: {widthPt: 595.123, heightPt: 841.456},
    margins: {topPt: 20.1, rightPt: 21.2, bottomPt: 22.3, leftPt: 23.4}});
  assert(!Object.hasOwn(restored.options, 'pageGeometry'));
  assert.deepEqual(Array.from(restored.options.pdfImages[0].bytes), [0, 128, 255]);
  assert.deepEqual(Array.from(restored.options.fontAssets[0].bytes), [7, 8, 9]);
  assert.equal(restored.options.fontAssets[1].weight, 525);
});

test('empty source and omitted optional settings stay distinct from explicit empty/zero', () => {
  const data = codec.decode(serialize(payload({source: ''})));
  assert.equal(data.source, '');
  for (const key of ['title', 'author', 'lang', 'metadataEpochSeconds', 'tocDepth', 'pageGeometry'])
    assert(!Object.hasOwn(data.options, key), key);
  assert.equal(codec.encode(payload({source: ''})).sourceLength, 0);
  const explicit = codec.decode(serialize(payload({options: {title: '', author: '', lang: '', metadataEpochSeconds: 0}})));
  assert.equal(explicit.options.title, '');
  assert.equal(explicit.options.metadataEpochSeconds, 0);
  assert(!Object.hasOwn(readNativeSourceProject(serialize(payload())).options, 'page'));
});

test('no network, DOM, workers or WASM are needed, including the serialized factory', () => {
  const isolated = vm.runInNewContext('(' + createNativeSourceProjectCodec.toString() + ')()',
    {TextEncoder, TextDecoder, Blob, atob});
  const output = isolated.encode(payload({source: 'offline'}));
  assert.equal(isolated.decode(output.bytes).source, 'offline');
  assert.deepEqual(JSON.parse(output.text()).images, []);
});

test('source and optional metadata are escaped as data, not binding code', () => {
  const source = '</script><!-- <script>globalThis.attacked=true</script>\u2028\u2029\0\b\f\t\r\n"\\';
  const text = serialize(payload({source, options: {title: source}}));
  assert(!text.includes('<'));
  assert(!text.includes('\u2028'));
  assert(!text.includes('\u2029'));
  assert.equal(codec.decode(text).source, source);
  assert.equal(codec.decode(text).options.title, source);
  assert.equal(globalThis.attacked, undefined);
});

test('settings and encoded resource snapshots cannot mutate after creation', () => {
  const input = payload({images: [{destination: 'one', bytes: 'AQ=='}],
    fonts: [{slot: 'body-bold', bytes: 'Ag=='}], options: {pageGeometry: [200, 300, 20, 20, 20, 20]}});
  const output = codec.encode(input), data = codec.decode(output.bytes);
  input.images[0].destination = 'changed'; input.fonts.length = 0; input.options.pageGeometry[0] = 1000;
  assert.equal(data.images[0].destination, 'one');
  assert.equal(data.fonts.length, 1);
  assert.equal(data.options.pageGeometry[0], 200);
  for (const value of [data, data.options, data.options.pageGeometry, data.images, data.images[0], data.fonts, data.fonts[0]])
    assert(Object.isFrozen(value));
  assert.throws(() => { data.options.toc = true; });
});

test('decoded native byte views are independently owned on every read', () => {
  const original = codec.encode(payload({images: [{destination: 'a', bytes: 'AQID'}],
    fonts: [{slot: 'body-regular', bytes: 'BAUG'}]}));
  const one = readNativeSourceProject(original.bytes), two = readNativeSourceProject(original.bytes);
  one.options.pdfImages[0].bytes.fill(255); one.options.fontAssets[0].bytes.fill(255);
  assert.deepEqual([...two.options.pdfImages[0].bytes], [1, 2, 3]);
  assert.deepEqual([...two.options.fontAssets[0].bytes], [4, 5, 6]);
  one.options.title = 'owned draft'; one.options.pdfImages.push({destination: 'new', bytes: new Uint8Array([8])});
  assert.equal(two.options.title, undefined); assert.equal(two.options.pdfImages.length, 1);
});

test('published byte buffers are host-owned and their output helpers reflect mutations', async () => {
  const output = codec.encode(payload());
  assert.equal(output.format, 'native-source-project');
  assert.equal(output.extension, 'json');
  assert.equal(output.mimeType, 'application/json');
  assert.equal(await output.blob().text(), output.text());
  const original = output.text(); output.bytes[0] = 32;
  assert.notEqual(output.text(), original);
});

test('byte admission respects Buffer/DataView/subarray offsets, including shadowed getters', () => {
  const output = codec.encode(payload({source: 'selected bytes'}));
  const packed = Buffer.concat([Buffer.from('WRONG'), output.bytes, Buffer.from('TRAILER')]);
  const subarray = packed.subarray(5, 5 + output.bytes.length);
  assert.equal(codec.decode(subarray).source, 'selected bytes');
  const view = new DataView(subarray.buffer, subarray.byteOffset, subarray.byteLength);
  Object.defineProperty(view, 'byteOffset', {get() { throw new Error('must not read shadow'); }});
  Object.defineProperty(view, 'buffer', {get() { throw new Error('must not read shadow'); }});
  assert.equal(codec.decode(view).source, 'selected bytes');
  assert.equal(codec.decode(output.bytes.buffer).source, 'selected bytes');
});

test('shared, spoofed and detached project buffers are rejected', () => {
  for (const value of [new SharedArrayBuffer(10), new Uint8Array(new SharedArrayBuffer(10)),
    {[Symbol.toStringTag]: 'ArrayBuffer', byteLength: 1}, null, 4, []]) fails(() => codec.decode(value));
  const bytes = codec.encode(payload()).bytes;
  structuredClone(bytes, {transfer: [bytes.buffer]});
  fails(() => codec.decode(bytes));
});

test('an optional JSON-file BOM is accepted but a source BOM is never removed', () => {
  const json = serialize(payload({source: '\uFEFF\uFEFFsource\r\n'}));
  for (const input of [json, '\uFEFF' + json, new TextEncoder().encode('\uFEFF' + json)])
    assert.equal(codec.decode(input).source, '\uFEFF\uFEFFsource\r\n');
});

test('invalid UTF-8 and invalid JSON have stable project errors', () => {
  for (const value of ['', '{', '[1]', 'null', '"text"', '{"source":NaN}', new Uint8Array([0xff])])
    fails(() => codec.decode(value));
});

for (const name of ['wasm', 'bindings', 'url', 'runtime', 'version', '__proto__']) {
  test('untrusted projects cannot carry executable/runtime field: ' + name, () => {
    const object = JSON.parse(serialize(payload()));
    Object.defineProperty(object, name, {value: 'never execute me', enumerable: true});
    fails(() => codec.decode(JSON.stringify(object)));
  });
}

for (const value of [undefined, null, 1, 'v2', 'fmd-native-source-project-v2']) {
  test('unsupported project schema is not guessed: ' + value, () => {
    const object = JSON.parse(serialize(payload())); object.schema = value;
    fails(() => codec.decode(JSON.stringify(object)), 'UNSUPPORTED_SOURCE_PROJECT');
  });
}

for (const field of ['source', 'options', 'images', 'fonts']) {
  test('required project data cannot silently disappear: ' + field, () => {
    const object = JSON.parse(serialize(payload())); delete object[field];
    fails(() => codec.decode(JSON.stringify(object)));
  });
}

test('descriptor validation rejects accessors and sparse arrays without invoking them', () => {
  let called = false;
  const getter = {get() { called = true; throw new Error('getter ran'); }, enumerable: true};
  const top = payload(); Object.defineProperty(top, 'source', getter);
  fails(() => codec.encode(top));
  const options = {}; Object.defineProperty(options, 'title', getter);
  fails(() => codec.encode(payload({options})));
  const image = {destination: 'x'}; Object.defineProperty(image, 'bytes', getter);
  fails(() => codec.encode(payload({images: [image]})));
  const images = [null]; Object.defineProperty(images, '0', getter);
  fails(() => codec.encode(payload({images})));
  fails(() => codec.encode(payload({images: Array(1)})));
  const geometry = [200, 300, 20, 20, 20, 20]; Object.defineProperty(geometry, '2', getter);
  fails(() => codec.encode(payload({options: {pageGeometry: geometry}})));
  assert.equal(called, false);
});

test('objects, arrays and options cannot hide extra fields or symbols', () => {
  for (const input of [payload({x: 1}), payload({options: {customCss: 'script'}}),
    payload({options: {allowRawHtml: true}}), payload({options: {running: {}}}),
    payload({fonts: [{slot: 'body-bold', bytes: 'AA==', href: 'x'}]})]) fails(() => codec.encode(input));
  const images = []; images.extra = true; fails(() => codec.encode(payload({images})));
  const options = {}; options[Symbol('hidden')] = true; fails(() => codec.encode(payload({options})));
  fails(() => codec.encode(payload({options: new Date()})));
  assert.equal(codec.decode(serialize(payload({options: Object.create(null)}))).options.font, 'sans');
});

for (const [key, values] of Object.entries({font: ['mono', null], darkMode: ['dark'], fontScale: [0.49, 3.01, NaN, Infinity, '1'],
  title: [1, null, '\ud800'], author: [false], lang: [23, '\udfff'], metadataEpochSeconds: [-1, 0.1, NaN, Infinity, 2 ** 53],
  pageNumbers: [1], codeLineNumbers: ['false'], toc: [null], tocDepth: [0, 7, 1.1]})) {
  test('strict native setting admission: ' + key, () => {
    for (const value of values) fails(() => codec.encode(payload({options: {[key]: value}})));
  });
}

test('metadata UTF-8 bounds count bytes, not characters', () => {
  fails(() => codec.encode(payload({options: {title: 'é'.repeat(32769)}})), 'SOURCE_PROJECT_LIMIT');
  fails(() => codec.encode(payload({options: {lang: 'é'.repeat(513)}})), 'SOURCE_PROJECT_LIMIT');
  assert.equal(codec.decode(serialize(payload({options: {lang: 'é'.repeat(512)}}))).options.lang.length, 512);
});

test('all paper geometry bounds, including native f32 intermediate rounding, are revalidated', () => {
  const bad = [[], [200, 300, 20, 20, 20], [200, 300, 20, 20, 20, 20, 0],
    [143, 300, 0, 0, 0, 0], [200, 14401, 0, 0, 0, 0], [200, 300, -1, 0, 0, 0],
    [200, 300, 0, 100, 0, 29], [200, 300, 229, 0, 0, 0], [200, 300, NaN, 0, 0, 0],
    [200, 300, 0, Infinity, 0, 0], [200, 300, 0, '0', 0, 0]];
  for (const pageGeometry of bad) fails(() => codec.encode(payload({options: {pageGeometry}})));
  // Double subtraction leaves exactly 72, while staged native f32 produces less.
  let found = null;
  for (let i = 1; i < 2000 && !found; i++) {
    const width = 200 + i / 10000, left = 37.12345, right = width - left - 72;
    const f = Math.fround(Math.fround(Math.fround(width) - Math.fround(left)) - Math.fround(right));
    if (width - left - right >= 72 && f < 72) found = [width, 300, 10, right, 10, left];
  }
  assert(found); fails(() => codec.encode(payload({options: {pageGeometry: found}})));
});

test('resource byte encodings must be canonical and within their decoded budget', () => {
  for (const bytes of ['', 'A', 'AA', 'AA=', 'AA===', 'AB==', 'AAB=', 'A A=', 'AA-_', '=AAA', '====', 'AA\n='])
    fails(() => codec.encode(payload({images: [{destination: 'a', bytes}]})));
  for (const bytes of ['AA==', 'AAA=', 'AAAA', '////', '/w==', '//8='])
    assert.equal(codec.decode(serialize(payload({images: [{destination: 'a', bytes}]}))).images[0].bytes, bytes);
  const small = reduced({assetBytes: 2});
  fails(() => small.encode(payload({images: [{destination: 'a', bytes: 'AAAA'}]})), 'SOURCE_PROJECT_LIMIT');
});

test('image destinations are preserved verbatim, not opened, rewritten or interpreted as paths', () => {
  for (const destination of ['../../elsewhere', '__proto__', 'https://never.fetch/image.png', 'data:provided-key', '图/a.png']) {
    const out = readNativeSourceProject(serialize(payload({images: [{destination, bytes: 'AA=='}]})));
    assert.equal(out.options.pdfImages[0].destination, destination);
  }
  for (const destination of ['', ' a', 'a ', 'a\n', 'a\0b', 'a\u0080b', '\ud800'])
    fails(() => codec.encode(payload({images: [{destination, bytes: 'AA=='}]})));
  fails(() => codec.encode(payload({images: [{destination: 'a', bytes: 'AA=='}, {destination: 'a', bytes: 'AQ=='}]})));
});

test('font slots, weights and deterministic slot order preserve omitted weight', () => {
  for (const slot of ['none', '__proto__', null]) fails(() => codec.encode(payload({fonts: [{slot, bytes: 'AA=='}]})));
  for (const weight of [0, 1001, NaN, Infinity, 100.5, null, '400'])
    fails(() => codec.encode(payload({fonts: [{slot: 'body-regular', bytes: 'AA==', weight}]})));
  fails(() => codec.encode(payload({fonts: [{slot: 'body-regular', bytes: 'AA=='}, {slot: 'body-regular', bytes: 'AQ=='}]})));
  const fonts = ['mono-regular', 'body-bold-italic', 'body-italic', 'body-bold', 'body-regular'].map(slot => ({slot, bytes: 'AA=='}));
  assert.equal(serialize(payload({fonts})), serialize(payload({fonts: fonts.slice().reverse()})));
  assert(!Object.hasOwn(codec.decode(serialize(payload({fonts}))).fonts[0], 'weight'));
});

test('all resource count, combined size and destination budgets are admitted before decoding', () => {
  const small = reduced({sourceBytes: 8, assetBytes: 3, totalAssetBytes: 4, imageCount: 2,
    destinationBytes: 4, totalDestinationBytes: 5});
  const image = (destination, bytes = 'AA==') => ({destination, bytes});
  const original = globalThis.atob;
  globalThis.atob = () => { throw new Error('must not allocate decoded resources'); };
  try {
    for (const data of [payload({source: 'x'.repeat(9)}), payload({source: 'é'.repeat(5)}),
      payload({source: '', images: [image('1'), image('2'), image('3')]}),
      payload({source: '', images: [image('12345')]}),
      payload({source: '', images: [image('123'), image('456')]}),
      payload({source: '', images: [image('a', 'AAAA')], fonts: [{slot: 'body-bold', bytes: 'AAA='}]})])
      fails(() => small.encode(data), 'SOURCE_PROJECT_LIMIT');
    const data = payload({source: '', images: [image('a', 'AAAA')], fonts: [{slot: 'body-bold', bytes: 'AA=='}]});
    const accepted = small.encode(data); assert.equal(small.decode(accepted.bytes).fonts.length, 1);
  } finally { globalThis.atob = original; }
});

test('exact escaped UTF-8 output budget is enforced before aggregate serialization', () => {
  for (const source of ['', 'ordinary', 'é🧪', '<"\\\b\t\n\f\r\0\u2028\u2029'.repeat(20)]) {
    const data = payload({source}), output = codec.encode(data), size = output.bytes.length;
    assert.equal(reduced({outputBytes: size}).encode(data).bytes.length, size);
    fails(() => reduced({outputBytes: size - 1}).encode(data), 'SOURCE_PROJECT_LIMIT');
    fails(() => reduced({outputBytes: size - 1}).decode(output.bytes), 'SOURCE_PROJECT_LIMIT');
    fails(() => reduced({outputBytes: size - 1}).decode(output.text()), 'SOURCE_PROJECT_LIMIT');
  }
});

test('resource decoding spans chunks and every base64 padding length', () => {
  for (const length of [1, 2, 3, 24575, 24576, 24577, 49153]) {
    const bytes = Uint8Array.from({length}, (_, i) => i % 251);
    const out = readNativeSourceProject(serialize(payload({images: [{destination: 'a', bytes: b64(bytes)}]})));
    assert.deepEqual(out.options.pdfImages[0].bytes, bytes);
  }
});

test('limits cannot be raised, malformed or accessor-driven', () => {
  for (const key of Object.keys(L)) for (const value of [0, -1, 1.5, L[key] + 1, Infinity, undefined])
    fails(() => reduced({[key]: value}));
  fails(() => reduced({unknown: 1}));
  let called = false;
  fails(() => reduced({get sourceBytes() { called = true; return 1; }}));
  assert.equal(called, false);
});

test('project filenames are portable and never paths', () => {
  const output = codec.encode(payload());
  assert.equal(output.filename(), 'document.fmd.json');
  assert.equal(output.filename('guide'), 'guide.fmd.json');
  assert.equal(output.filename('CON'), 'document.fmd.json');
  assert.equal(output.filename('LPT2.txt'), 'document.fmd.json');
  assert.equal(output.filename('../a\\b?. '), '.._a_b_.fmd.json');
  assert.equal(output.filename('...'), 'document.fmd.json');
  assert.equal(Array.from(output.filename('🧪'.repeat(90)).split('.fmd.json')[0]).length, 80);
});

test('seeded source/resource round trips keep all original bytes and native option values', () => {
  let state = 0x7150ac;
  const next = maximum => { state = (Math.imul(state, 1664525) + 1013904223) >>> 0; return state % maximum; };
  const alphabet = ['x', 'é', '中', '🧪', '\n', '\r', '\t', '\0', '<', '"', '\\', '\uFEFF', '\u2028', '\u2029'];
  for (let round = 0; round < 750; round++) {
    let source = ''; for (let i = next(120); i > 0; i--) source += alphabet[next(alphabet.length)];
    const bytes = Uint8Array.from({length: next(63) + 1}, () => next(256));
    const input = payload({source, options: {font: next(2) ? 'sans' : 'serif', fontScale: 0.5 + next(251) / 100,
      metadataEpochSeconds: next(1000), pageNumbers: !!next(2), title: source},
      images: [{destination: 'resources/' + round, bytes: b64(bytes)}],
      fonts: [{slot: 'body-regular', bytes: b64(bytes), weight: next(1000) + 1}]});
    const output = codec.encode(input), decoded = codec.decode(output.bytes), restored = readNativeSourceProject(output.bytes);
    assert.equal(decoded.source, source); assert.equal(restored.source, source);
    assert.equal(restored.options.title, source);
    assert.deepEqual(restored.options.pdfImages[0].bytes, bytes);
    assert.deepEqual(restored.options.fontAssets[0].bytes, bytes);
    assert.equal(restored.options.fontAssets[0].weight, input.fonts[0].weight);
    assert.equal(codec.encode(decoded).text(), output.text());
  }
});
