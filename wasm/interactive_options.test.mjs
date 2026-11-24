// Real public facade and helper modules; generated bindings are explicit ABI
// adapters. Native HTML/font behavior has separate Rust/browser regressions.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {SourceTextModule, SyntheticModule} from 'node:vm';
const source = await readFile(new URL('./franken_markdown.js', import.meta.url), 'utf8');
async function fixture({advanced = true, profiled = true, legacy = true, metadata, gate, failure} = {}) {
  const calls = []; let inits = 0, frees = 0;
  function output(args) {
    if (failure) throw failure;
    return {format: 'interactive-html', mimeType: 'text/html; charset=utf-8', extension: 'html',
      sourceLength: new TextEncoder().encode(args[0]).length, bytes: new TextEncoder().encode('adapter'),
      diagnosticsJson: () => '[]', free() {frees++;}};
  }
  const bindings = {default: async () => {inits++; await gate;}, capabilities() {},
    renderHtmlConfiguredAdvanced() {}, renderPdfConfiguredMulti() {}, renderEpubConfigured() {}, renderSvgConfigured() {},
    accessibilityAudit() {}, documentStats() {}, searchIndex() {}, semanticDiff() {}, renderSemanticDiffHtml() {}};
  if (legacy) bindings.renderInteractiveHtmlConfigured = (...args) => {calls.push({kind: 'legacy', args}); return output(args);};
  if (metadata) bindings.sourceMetadata = metadata;
  if (profiled) {
    const render = (...args) => {calls.push({kind: 'profiled', args}); return output(args);};
    Object.defineProperty(render, 'length', {value: typeof profiled === 'number' ? profiled : 21});
    bindings.renderInteractiveHtmlConfiguredProfiled = render;
  }
  if (advanced) {
    const render = (...args) => {calls.push({kind: 'advanced', args}); return output(args);};
    Object.defineProperty(render, 'length', {value: typeof advanced === 'number' ? advanced : 20});
    bindings.renderInteractiveHtmlConfiguredAdvanced = render;
  }
  const native = new SyntheticModule(Object.keys(bindings), function() {
    for (const [name, value] of Object.entries(bindings)) this.setExport(name, value);
  });
  const module = new SourceTextModule(source);
  const helpers = new Map();
  for (const name of ['pdf_page.mjs', 'pdf_running.mjs', 'render_typography.mjs'])
    helpers.set('./' + name, new SourceTextModule(await readFile(new URL(name, import.meta.url), 'utf8')));
  await module.link(name => name === './pkg/franken_markdown.js' ? native : helpers.get(name));
  await module.evaluate();
  return {api: module.namespace, calls, get inits() {return inits;}, get frees() {return frees;}};
}

test('default interactive calls preserve the six-argument legacy shape and own metadata before init', async () => {
  let release; const gate = new Promise(resolve => {release = resolve;});
  const f = await fixture({advanced: false, gate});
  const options = {title: ' Before ', lang: 'fr', font: 'sans', typeSize: 'md', toc: false, pdfImages: []};
  let value = 'Before'; const pending = f.api.renderInteractiveHtml({toString: () => value}, options);
  value = 'After'; options.title = 'After'; release(); await pending;
  assert.deepEqual(f.calls[0], {kind: 'legacy', args: ['Before', 'sans', undefined, ' Before ', 'fr', 1]});
  assert.equal(f.frees, 1);
});

test('all configured HTML options and exact byte views are captured across asynchronous initialization', async () => {
  let release; const gate = new Promise(resolve => {release = resolve;});
  const f = await fixture({gate}), buffer = Uint8Array.of(9, 1, 2, 3, 9), font = Uint8Array.of(8, 4, 5, 8);
  const options = {font: 'serif', darkMode: 'disabled', title: ' Café ', lang: 'de',
    customCss: 'main {color: red}', fontScale: 1.25, toc: true, tocDepth: 4, htmlFontFormat: 'woff2',
    pdfImages: [{destination: ' chart.png ', bytes: new DataView(buffer.buffer, 1, 3)}],
    fontAssets: [{slot: 'body-regular', weight: 555, bytes: font.subarray(1, 3)}]};
  const pending = f.api.renderInteractiveHtml('# Before 😀', options);
  buffer.fill(0); font.fill(0); options.title = 'After'; options.pdfImages[0].destination = 'later';
  release(); const result = await pending;
  const {kind, args} = f.calls[0]; assert.equal(kind, 'advanced'); assert.equal(args.length, 20);
  assert.deepEqual(args.slice(0, 7), ['# Before 😀', 'serif', 'disabled', ' Café ', 'main {color: red}', false, 1.25]);
  assert.deepEqual([...args[7]], [4, 5]); assert.deepEqual([...args[12]], [555, 0, 0, 0, 0]);
  assert.deepEqual(args[13], ['chart.png']); assert.deepEqual([...args[14]], [1, 2, 3]); assert.deepEqual([...args[15]], [3]);
  assert.deepEqual(args.slice(16), ['de', true, 4, 'woff2']); assert.equal(result.sourceLength, Buffer.byteLength('# Before 😀'));
  assert.equal(f.frees, 1);
});

test('every nondefault presentation needs a matching advanced binding before initialization', async () => {
  for (const options of [{font: 'serif'}, {darkMode: 'disabled'}, {typeSize: 'lg'}, {customCss: 'p{}'},
    {toc: true}, {tocDepth: 3}, {htmlFontFormat: 'woff1'},
    {pdfImages: [{destination: 'x', bytes: Uint8Array.of(1)}]},
    {fontAssets: [{slot: 'mono-regular', bytes: Uint8Array.of(1)}]}]) {
    const f = await fixture({advanced: false});
    await assert.rejects(f.api.renderInteractiveHtml('x', options), {code: 'UNSUPPORTED_WASM_PACKAGE'});
    assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
  }
  const old = await fixture({advanced: 19});
  await assert.rejects(old.api.renderInteractiveHtml('x', {toc: true}), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  assert.equal(old.inits, 0);
});

test('raw passthrough, PDF-only options and option accessors are refused without reading them', async () => {
  const f = await fixture(); let reads = 0;
  const getter = {}; Object.defineProperty(getter, 'customCss', {get() {reads++; return 'p{}';}});
  for (const options of [getter, {allowRawHtml: true}, {pageNumbers: true}, {strictPagination: true},
    {pdfA: '2b'}, {pdfA: 'off'}, {pdfAStrict: true}, {pdfAStrict: false},
    {typography: {}}, {author: 'A'}, {toc: 1}, {tocDepth: 7}, {font: 'mono'}, {htmlFontFormat: 'woff3'}])
    await assert.rejects(f.api.renderInteractiveHtml('x', options));
  assert.equal(reads, 0); assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test('image/font arrays admit dense owned data and reject duplicate keys, getters and shared buffers', async () => {
  const f = await fixture(); let reads = 0;
  const asset = {destination: 'x'}; Object.defineProperty(asset, 'bytes', {get() {reads++; return Uint8Array.of(1);}});
  for (const options of [{pdfImages: [asset]}, {pdfImages: new Array(1)},
    {pdfImages: [{destination: 'x', bytes: Uint8Array.of(1)}, {destination: ' x ', bytes: Uint8Array.of(2)}]},
    {pdfImages: [{destination: 'x', bytes: new Uint8Array(new SharedArrayBuffer(2))}]},
    {fontAssets: [{slot: 'body-regular', bytes: Uint8Array.of(1)}, {slot: 'body-regular', bytes: Uint8Array.of(2)}]}])
    await assert.rejects(f.api.renderInteractiveHtml('x', options));
  assert.equal(reads, 0); assert.equal(f.inits, 0);
});

test('malformed Unicode, metadata and resource counts fail before any native call', async () => {
  const f = await fixture();
  await assert.rejects(f.api.renderInteractiveHtml('\ud800'));
  for (const options of [{title: '\udc00'}, {customCss: 'x'.repeat(1024 * 1024 + 1)},
    {pdfImages: new Array(4097)}, {fontAssets: new Array(6)}, {fontScale: NaN}])
    await assert.rejects(f.api.renderInteractiveHtml('x', options));
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test('resource view branding ignores caller-overridden byte accessors', async () => {
  const f = await fixture(); let reads = 0;
  const image = Uint8Array.of(8, 1, 2, 8).subarray(1, 3);
  const font = new DataView(Uint8Array.of(8, 3, 4, 8).buffer, 1, 2);
  for (const value of [image, font]) for (const key of ['buffer', 'byteOffset', 'byteLength'])
    Object.defineProperty(value, key, {get() {reads++; throw Error('caller property read');}});
  await f.api.renderInteractiveHtml('x', {pdfImages: [{destination: 'x', bytes: image}],
    fontAssets: [{slot: 'body-regular', bytes: font}]});
  assert.deepEqual([...f.calls[0].args[7]], [3, 4]);
  assert.deepEqual([...f.calls[0].args[14]], [1, 2]);
  assert.equal(reads, 0);
});

test('explicitly disabled and empty presentation fields preserve legacy compatibility', async () => {
  const f = await fixture({advanced: false});
  await f.api.renderInteractiveHtml('x', {font: 'sans', darkMode: 'auto', fontScale: 1, customCss: '',
    allowRawHtml: false, toc: false, pdfImages: [], fontAssets: []});
  assert.equal(f.calls[0].kind, 'legacy'); assert.equal(f.calls[0].args.length, 6);
});

test('advanced native refusals propagate unchanged without producing a weaker workspace', async () => {
  const failure = Error('font decoding refused'), f = await fixture({failure});
  await assert.rejects(f.api.renderInteractiveHtml('x', {toc: true}), error => error === failure);
  assert.deepEqual(f.calls.map(call => call.kind), ['advanced']); assert.equal(f.frees, 0);
});


test('explicit lightweight dialects use only the full profiled ABI and acknowledge native success', async () => {
  const f = await fixture({advanced: false, legacy: false});
  for (const profile of ['commonmark-gfm', 'gfm-plus']) {
    const result = await f.api.renderInteractiveHtml('\ufeffTerm\r\n: Definition\r\n', {profile});
    const call = f.calls.at(-1);
    assert.equal(call.kind, 'profiled'); assert.equal(call.args.length, 21);
    assert.equal(call.args[0], '\ufeffTerm\r\n: Definition\r\n'); assert.equal(call.args[20], profile);
    assert.equal(result.profile, profile); assert.equal(Object.isFrozen(result), true);
  }
  assert.equal(f.frees, 2);
});

test('profiled configured resources, source and grammar are captured before initialization yields', async () => {
  let release; const gate = new Promise(resolve => {release = resolve;});
  const f = await fixture({gate, advanced: false, legacy: false});
  const image = Uint8Array.of(9, 1, 2, 9), font = Uint8Array.of(8, 3, 4, 8);
  let source = '\ufeff# Before\r\n';
  const options = {profile: 'commonmark-gfm', title: 'Before', customCss: 'main {color:purple}', toc: true,
    tocDepth: 2, font: 'serif', htmlFontFormat: 'woff2',
    pdfImages: [{destination: 'chart.png', bytes: new DataView(image.buffer, 1, 2)}],
    fontAssets: [{slot: 'body-regular', weight: 555, bytes: font.subarray(1, 3)}]};
  const pending = f.api.renderInteractiveHtml({toString: () => source}, options);
  source = 'Later'; options.profile = 'gfm-plus'; options.title = 'Later'; options.tocDepth = 5;
  options.pdfImages[0].destination = 'later.png'; image.fill(0); font.fill(0);
  release(); const result = await pending, {args, kind} = f.calls[0];
  assert.equal(kind, 'profiled'); assert.equal(args.length, 21);
  assert.equal(args[0], '\ufeff# Before\r\n'); assert.equal(args[3], 'Before');
  assert.equal(args[4], 'main {color:purple}'); assert.equal(args[20], 'commonmark-gfm');
  assert.deepEqual([...args[7]], [3, 4]); assert.deepEqual([...args[12]], [555, 0, 0, 0, 0]);
  assert.deepEqual(args[13], ['chart.png']); assert.deepEqual([...args[14]], [1, 2]);
  assert.deepEqual(args.slice(17, 20), [true, 2, 'woff2']);
  assert.equal(result.profile, 'commonmark-gfm'); assert.equal(f.frees, 1);
});

test('profile capability refusal precedes source coercion and resource inspection without fallback', async () => {
  for (const profiled of [false, 20]) {
    const f = await fixture({profiled}); let reads = 0;
    const unread = () => {reads++; throw Error('input read before capability admission');};
    const resources = new Proxy([], {get: unread, ownKeys: unread});
    for (const profile of ['commonmark-gfm', 'gfm-plus']) {
      await assert.rejects(f.api.renderInteractiveHtml({toString: unread},
        {profile, pdfImages: resources, fontAssets: resources}), {code: 'UNSUPPORTED_WASM_PACKAGE'});
    }
    assert.equal(reads, 0); assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
  }
});

test('lightweight profile admission is canonical and does not coerce source or profile objects', async () => {
  const f = await fixture(); let reads = 0;
  const unread = () => {reads++; throw Error('coercion before profile admission');};
  const resources = new Proxy([], {get: unread, ownKeys: unread});
  for (const profile of [null, '', 'gfm', 'CommonMark-GFM', ' gfm-plus ', 1, false, [], {toString: unread}]) {
    await assert.rejects(f.api.renderInteractiveHtml({toString: unread}, {profile, pdfImages: resources}),
      {code: 'INVALID_OPTIONS'});
  }
  const getter = {}; Object.defineProperty(getter, 'profile', {get: unread});
  await assert.rejects(f.api.renderInteractiveHtml({toString: unread}, getter), /field or accessor/);
  await assert.rejects(f.api.renderInteractiveHtml('source', {profile: 'gfm-plus', unknown: true}), /field or accessor/);
  assert.equal(reads, 0); assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test('undefined profiles preserve both legacy interactive call shapes and omit the receipt', async () => {
  const f = await fixture();
  for (const options of [{}, {toc: true, htmlFontFormat: 'woff2'}]) {
    const original = await f.api.renderInteractiveHtml('source', options);
    const call = f.calls.at(-1);
    const explicitUndefined = await f.api.renderInteractiveHtml('source', {...options, profile: undefined});
    assert.deepEqual(f.calls.at(-1), call);
    assert.equal(call.kind, options.toc ? 'advanced' : 'legacy');
    assert.equal(call.args.length, options.toc ? 20 : 6);
    assert.equal(Object.hasOwn(original, 'profile'), false);
    assert.equal(Object.hasOwn(explicitUndefined, 'profile'), false);
  }
});

test('inherited navigation stays on the selected profiled binding while explicit settings win', async () => {
  const sources = [], f = await fixture({advanced: false, legacy: false,
    metadata(source) {sources.push(source); return '{"title":"From source","lang":"fr","toc":true,"tocDepth":3}';}});
  await f.api.renderInteractiveHtml('---\ntitle: From source\n---\nBody', {profile: 'gfm-plus'});
  assert.equal(f.calls[0].kind, 'profiled');
  assert.equal(f.calls[0].args[3], 'From source'); assert.deepEqual(f.calls[0].args.slice(16), ['fr', true, 3, undefined, 'gfm-plus']);
  await f.api.renderInteractiveHtml('Other source', {profile: 'commonmark-gfm', title: 'Explicit', lang: 'en', toc: false, tocDepth: 1});
  assert.equal(f.calls[1].kind, 'profiled'); assert.equal(f.calls[1].args[3], 'Explicit');
  assert.deepEqual(f.calls[1].args.slice(16), ['en', false, 1, undefined, 'commonmark-gfm']);
  assert.deepEqual(sources, ['---\ntitle: From source\n---\nBody']);
});

test('concurrent dialects keep separate requests and native refusals never retry a legacy binding', async () => {
  let release; const gate = new Promise(resolve => {release = resolve;});
  const f = await fixture({gate});
  const a = f.api.renderInteractiveHtml('first', {profile: 'commonmark-gfm'});
  const b = f.api.renderInteractiveHtml('second', {profile: 'gfm-plus'});
  release(); const results = await Promise.all([a, b]);
  assert.deepEqual(f.calls.map(({kind, args}) => [kind, args[0], args[20]]),
    [['profiled', 'first', 'commonmark-gfm'], ['profiled', 'second', 'gfm-plus']]);
  assert.deepEqual(results.map(result => result.profile), ['commonmark-gfm', 'gfm-plus']);
  const failure = Error('native profile refusal'), failed = await fixture({failure});
  await assert.rejects(failed.api.renderInteractiveHtml('source', {profile: 'commonmark-gfm'}), error => error === failure);
  assert.deepEqual(failed.calls.map(call => call.kind), ['profiled']); assert.equal(failed.frees, 0);
});


test('packages without any interactive export refuse before even inspecting option objects', async () => {
  const f = await fixture({advanced: false, profiled: false, legacy: false}); let reads = 0;
  const unread = () => {reads++; throw Error('options inspected without an interactive capability');};
  const options = new Proxy({}, {get: unread, ownKeys: unread, getPrototypeOf: unread, getOwnPropertyDescriptor: unread});
  await assert.rejects(f.api.renderInteractiveHtml({toString: unread}, options), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  assert.equal(reads, 0); assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});
