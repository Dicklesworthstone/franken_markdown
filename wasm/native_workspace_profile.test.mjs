// Production workspace factories, persistence and real worker transport. Native
// bindings are explicit ABI adapters; parser/render parity has Rust coverage.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {Worker} from 'node:worker_threads';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';
import {createWorkspaceExporterWithLoader, createNativeSourceProject, readNativeSourceProject} from './interactive_export.mjs';
import {createNativeSourceProjectCodec} from './native_source_project.mjs';

const source = '\ufeff*Term\r\n: definition*\r\n\r\n<!-- pagebreak -->\r\n# Café 😀\r';
const geometry = [792, 612, 24, 36, 48, 60];
const profiles = ['commonmark-gfm', 'gfm-plus'];
const utf8 = new TextEncoder();
const magic = Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0);
const routes = [
  ['renderHtmlConfiguredAdvanced', 21, 'text/html'],
  ['renderHtmlConfiguredProfiled', 22, 'text/html'],
  ['renderHtmlConfiguredMetadata', 23, 'text/html'],
  ['renderPdfConfiguredMulti', 28, 'application/pdf'],
  ['renderPdfConfiguredProfiled', 40, 'application/pdf'],
  ['renderEpubConfiguredAdvanced', 19, 'application/epub+zip'],
  ['renderEpubConfiguredProfiled', 20, 'application/epub+zip'],
  ['renderEpubConfiguredMetadata', 21, 'application/epub+zip'],
  ['renderSvgConfiguredResources', 14, 'image/svg+xml'],
  ['renderSvgConfiguredProfiled', 15, 'image/svg+xml'],
  ['renderSvgConfiguredNavigation', 17, 'image/svg+xml'],
];
function payload(options = {}) {
  return {version: 1, options: {font: 'serif', fontScale: 1.25, title: 'Owned title', ...options},
    images: [{destination: 'chart.png', bytes: 'AQID'}],
    fonts: [{slot: 'body-regular', weight: 555, bytes: 'BAUG'}]};
}
function output(name, mimeType, args) {
  const json = JSON.stringify({name, args}, (_, value) => ArrayBuffer.isView(value) ? [...value] : value);
  // Header-bearing ABI adapters exercise the production worker's format guard;
  // these are deliberately not native PDF, EPUB or SVG rendering fixtures.
  const text = mimeType === 'text/html' ? '<html><head></head><body>' + json + '</body></html>'
    : mimeType === 'application/pdf' ? '%PDF-ADAPTER\n' + json
    : mimeType === 'application/epub+zip' ? 'PK\u0003\u0004' + json
    : '<svg xmlns="http://www.w3.org/2000/svg"><desc>'
      + json.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;') + '</desc></svg>';
  const bytes = new TextEncoder().encode(text);
  return {bytes, mimeType, diagnosticsJson: () => JSON.stringify([{code: name, message: name}]), free() {bytes.fill(0);}};
}
function fixture() {
  const calls = [], bindings = {};
  for (const [name, arity, mimeType] of routes) {
    const render = (...args) => {calls.push({name, args}); return output(name, mimeType, args);};
    Object.defineProperty(render, 'length', {value: arity, configurable: true});
    bindings[name] = render;
  }
  for (const name of ['documentStats', 'accessibilityAudit']) {
    bindings[name] = markdown => {calls.push({name, args: [markdown]}); return JSON.stringify({source: markdown});};
    bindings[name + 'Profiled'] = (markdown, profile) => {
      calls.push({name: name + 'Profiled', args: [markdown, profile]});
      return JSON.stringify({source: markdown, profile});
    };
  }
  bindings.renderInteractiveHtmlConfigured = markdown => {
    const shell = '<html><head></head><body><div class="fmd-content" id="fmd-content"></div>\n  </main>\n</div>\n'
      + '<script type="application/json" id="fmd-raw-source">' + JSON.stringify(markdown).replace(/</g, '\\u003c') + '</script>\n'
      + '<script>\nwindow.__fmdNativeRuntime; /* fmd-async-preview-v1 */\n</script></body></html>';
    return {bytes: utf8.encode(shell), mimeType: 'text/html',
      diagnosticsJson: () => '[{"code":"default_shell_dialect","message":"Shell-only finding"}]', free() {}};
  };
  return {bindings, calls};
}
const unsupported = error => error.code === 'UNSUPPORTED_WASM_PACKAGE';
function savedPayload(output) {
  return JSON.parse(output.text().split('<script type="application/json" id="fmd-native-runtime">')[1].split('</script>')[0]);
}

for (const profile of profiles) test(`workspace ${profile} reaches every native publication and analysis`, () => {
  const f = fixture(), r = createNativeWorkspaceRenderer(f.bindings, payload({profile, author: 'Author', lang: 'fr'}));
  assert.match(r.html(source), /Content-Security-Policy/);
  assert.equal(f.calls.at(-1).name, 'renderHtmlConfiguredMetadata');
  let args = f.calls.at(-1).args;
  assert.equal(args.length, 23); assert.equal(args[0], source);
  assert.deepEqual(args.slice(19), [undefined, undefined, profile, 'Author']);
  assert.deepEqual([...args[7]], [4, 5, 6]); assert.equal(args[12][0], 555);
  assert.deepEqual(args[13], ['chart.png']); assert.deepEqual([...args[14]], [1, 2, 3]);
  r.pdf(source); args = f.calls.at(-1).args;
  assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredProfiled'); assert.equal(args.length, 40);
  assert.deepEqual(args.slice(36), ['off', false, undefined, profile]);
  r.epub(source); args = f.calls.at(-1).args;
  assert.equal(f.calls.at(-1).name, 'renderEpubConfiguredMetadata'); assert.equal(args.length, 21);
  assert.deepEqual(args.slice(18), [undefined, profile, 'Author']);
  assert.deepEqual([...args[12]], [4, 5, 6]); assert.deepEqual([...args[10]], [1, 2, 3]);
  r.svg(source); args = f.calls.at(-1).args;
  assert.equal(f.calls.at(-1).name, 'renderSvgConfiguredProfiled'); assert.equal(args.length, 15);
  assert.equal(args[14], profile); assert.equal(args[3], 1.25);
  for (const kind of ['stats', 'accessibility']) {
    assert.deepEqual(JSON.parse(r.analyze(kind, source)), {source, profile});
    assert.equal(f.calls.at(-1).args.length, 2);
    assert.equal(f.calls.at(-1).name, kind === 'stats' ? 'documentStatsProfiled' : 'accessibilityAuditProfiled');
  }
});

test('profiled PDF retains geometry, running images, typography, archival and baseline-grid controls', () => {
  const f = fixture(), r = createNativeWorkspaceRenderer(f.bindings, payload({profile: 'commonmark-gfm',
    pageGeometry: geometry, baselineGridPt: 13.125, pdfA: '2b', pdfAStrict: true,
    typography: {homogeneous: true, lineHeight: 1.4}, strictPagination: true, microtype: 'expansion',
    running: {header: {left: '{title}', rule: true, image: {dest: 'chart.png', position: 'right', heightPt: 16}},
      footer: {center: '{page}'}, skipFirstPage: true}}));
  r.pdf(source); const args = f.calls.at(-1).args;
  assert.equal(args.length, 40); assert.equal(args[0], source);
  assert.deepEqual([...args[27]], geometry);
  assert.deepEqual(args[28], ['{title}', '', '', '', '{page}', '']);
  assert.deepEqual(args.slice(29, 32), [true, false, true]);
  assert.deepEqual(args[32], ['chart.png', '']);
  assert.deepEqual([...args[33]], [1, 0]); assert.deepEqual([...args[34]], [16, 0]);
  assert.equal(args[35], 'homogeneous,theme-spacing,line-height=1.4,strict-pagination,expansion');
  assert.deepEqual(args.slice(36), ['2b', true, 13.125, 'commonmark-gfm']);
});

test('SVG publication preserves configured navigation with or without an explicit dialect', () => {
  for (const profile of [undefined, ...profiles]) {
    const f = fixture(), r = createNativeWorkspaceRenderer(f.bindings, payload({profile, toc: true, tocDepth: 4}));
    r.svg(source); const call = f.calls.at(-1);
    assert.equal(call.name, 'renderSvgConfiguredNavigation'); assert.equal(call.args.length, 17);
    assert.deepEqual(call.args.slice(14), [profile, true, 4]);
    assert.deepEqual([...call.args[8]], [4, 5, 6]);
    delete f.bindings.renderSvgConfiguredNavigation;
    assert.throws(() => r.svg(source), unsupported);
    assert.equal(f.calls.length, 1, 'navigation is never retried through a weaker ABI');
  }
});

test('profiled EPUB works without metadata ABI only when no author would be discarded', () => {
  const f = fixture(); delete f.bindings.renderEpubConfiguredMetadata;
  const r = createNativeWorkspaceRenderer(f.bindings, payload({profile: 'gfm-plus'}));
  r.epub(source); assert.equal(f.calls.at(-1).name, 'renderEpubConfiguredProfiled');
  assert.equal(f.calls.at(-1).args.length, 20);
  assert.deepEqual(f.calls.at(-1).args.slice(18), [undefined, 'gfm-plus']);
  r.stageSettings({author: 'Retain me'}).commit();
  assert.throws(() => r.epub(source), unsupported); assert.equal(f.calls.length, 1);
});

test('missing or short HTML author bindings preserve source recovery, settings and PDF access', () => {
  for (const profile of [undefined, ...profiles]) for (const short of [false, true]) {
    const f = fixture(), author = '  Café e\u0301 中 👩🏽‍💻  ';
    if (short) Object.defineProperty(f.bindings.renderHtmlConfiguredMetadata, 'length', {value: 22});
    else delete f.bindings.renderHtmlConfiguredMetadata;
    const codec = createNativeSourceProjectCodec();
    const saved = codec.encode({source, options: {profile, author}, images: [], fonts: []});
    const restored = codec.decode(saved.bytes);
    assert.equal(restored.source, source); assert.equal(restored.options.author, author);
    const p = payload({profile: restored.options.profile, author: restored.options.author});
    const r = createNativeWorkspaceRenderer(f.bindings, p);
    const tx = r.stageSettings({title: 'Recovered'}); tx.commit();
    assert.equal(r.settings.author, author); assert.equal(r.settings.title, 'Recovered');
    r.pdf(restored.source);
    assert.equal(f.calls.at(-1).args[0], source); assert.equal(f.calls.at(-1).args[4], author);
    const calls = f.calls.length;
    assert.throws(() => r.html(restored.source), unsupported);
    assert.equal(f.calls.length, calls, 'Author HTML must never retry a binding that drops metadata');
    const reopened = createNativeWorkspaceRenderer(f.bindings, JSON.parse(JSON.stringify(p)));
    assert.equal(reopened.settings.author, author);
    assert.throws(() => reopened.stageSettings({title: 'Preview'}).html(source), unsupported);
    reopened.stageSettings({author: ''}).commit(); reopened.html(source);
    assert.equal(f.calls.at(-1).name, profile === undefined ? 'renderHtmlConfiguredAdvanced' : 'renderHtmlConfiguredProfiled');
    assert.equal(f.calls.at(-1).args.length, profile === undefined ? 19 : 22);
    assert.equal(reopened.settings.author, '', 'An explicit empty HTML author stays empty on recovery');
  }
});

test('workspace HTML author snapshots preserve Unicode and whitespace through preflight and rollback', () => {
  const f = fixture(), author = '  Café e\u0301 中 👩🏽‍💻  ';
  const r = createNativeWorkspaceRenderer(f.bindings, payload({profile: 'gfm-plus', author}));
  r.html(source);
  assert.equal(f.calls.at(-1).name, 'renderHtmlConfiguredMetadata');
  assert.deepEqual(f.calls.at(-1).args.slice(19), [undefined, undefined, 'gfm-plus', author]);
  const patch = {author: ' \t '}, tx = r.stageSettings(patch);
  patch.author = 'mutated after staging';
  tx.html(source);
  assert.equal(f.calls.at(-1).args[22], ' \t ');
  assert.equal(r.settings.author, author);
  tx.commit(); r.html(source); assert.equal(f.calls.at(-1).args[22], ' \t ');
  tx.rollback(); r.html(source); assert.equal(f.calls.at(-1).args[22], author);
});

test('omitted profiles preserve legacy routes, argument counts and analysis results', () => {
  const f = fixture();
  for (const name of Object.keys(f.bindings)) if (name.endsWith('Profiled')) delete f.bindings[name];
  delete f.bindings.renderSvgConfiguredNavigation;
  const r = createNativeWorkspaceRenderer(f.bindings, payload());
  for (const [method, name, count] of [['html', 'renderHtmlConfiguredAdvanced', 19],
    ['pdf', 'renderPdfConfiguredMulti', 27], ['epub', 'renderEpubConfiguredMetadata', 21],
    ['svg', 'renderSvgConfiguredResources', 14]]) {
    r[method](source); assert.equal(f.calls.at(-1).name, name); assert.equal(f.calls.at(-1).args.length, count);
  }
  for (const kind of ['stats', 'accessibility']) {
    assert.deepEqual(JSON.parse(r.analyze(kind, source)), {source}); assert.equal(f.calls.at(-1).args.length, 1);
  }
  r.stageSettings({profile: undefined}).commit(); assert.equal(Object.hasOwn(r.settings, 'profile'), false);
});

test('invalid dialects fail before source-project or workspace resource access', () => {
  const f = fixture(), codec = createNativeSourceProjectCodec();
  for (const profile of [null, '', 'gfm', 'CommonMark-GFM', ' gfm-plus ', true, 1, {}]) {
    const p = payload({profile}); Object.defineProperty(p, 'images', {get() {assert.fail('resources were read');}});
    assert.throws(() => createNativeWorkspaceRenderer(f.bindings, p), /profile/);
    assert.throws(() => codec.encode({source, options: {profile}, images: [], fonts: []}), {code: 'INVALID_SOURCE_PROJECT'});
    assert.throws(() => createNativeSourceProject(source, {profile,
      get pdfImages() {assert.fail('resources read before dialect admission');}}), {code: 'INVALID_OPTIONS'});
  }
  assert.equal(f.calls.length, 0);
});

test('missing or short required profile bindings reject atomically before decoding resources', () => {
  for (const [name, arity] of [['renderHtmlConfiguredProfiled', 22], ['renderPdfConfiguredProfiled', 40]])
    for (const short of [false, true]) {
      const f = fixture(), r = createNativeWorkspaceRenderer(f.bindings, payload()), before = r.settings;
      if (short) Object.defineProperty(f.bindings[name], 'length', {value: arity - 1});
      else delete f.bindings[name];
      assert.throws(() => r.stageSettings({profile: 'commonmark-gfm'}), unsupported);
      assert.equal(r.settings, before);
      const p = payload({profile: 'gfm-plus'});
      Object.defineProperty(p, 'images', {get() {assert.fail('assets were decoded');}});
      assert.throws(() => createNativeWorkspaceRenderer(f.bindings, p), unsupported);
      assert.equal(f.calls.length, 0);
    }
});

test('optional publication and analysis cannot silently discard an explicit dialect', () => {
  for (const [method, name, arity] of [['epub', 'renderEpubConfiguredMetadata', 21],
    ['svg', 'renderSvgConfiguredProfiled', 15], ['stats', 'documentStatsProfiled', 2],
    ['accessibility', 'accessibilityAuditProfiled', 2]]) {
    const f = fixture(), r = createNativeWorkspaceRenderer(f.bindings, payload({profile: 'gfm-plus'}));
    Object.defineProperty(f.bindings[name], 'length', {value: arity - 1});
    assert.throws(() => ['stats', 'accessibility'].includes(method) ? r.analyze(method, source) : r[method](source), unsupported);
    assert.equal(f.calls.length, 0);
  }
});

test('profile preflight, commit and rollback keep source, resources and existing state owned', () => {
  const f = fixture(), p = payload(), r = createNativeWorkspaceRenderer(f.bindings, p), before = r.settings;
  r.pdf(source); const original = f.calls.at(-1).args;
  const patch = {profile: 'commonmark-gfm'}, tx = r.stageSettings(patch); patch.profile = 'gfm-plus';
  tx.html(source); assert.equal(f.calls.at(-1).args.at(-1), 'commonmark-gfm');
  assert.equal(r.settings, before); r.pdf(source); assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredMulti');
  tx.commit(); r.pdf(source); assert.equal(f.calls.at(-1).args.at(-1), 'commonmark-gfm');
  for (const index of [8, 9, 10, 11, 12, 13, 14, 15, 16]) assert.equal(f.calls.at(-1).args[index], original[index]);
  tx.rollback(); assert.equal(r.settings, before); assert.equal(Object.hasOwn(p.options, 'profile'), false);
});

test('saved projects and native workspace generations preserve dialect and original bytes', async () => {
  for (const profile of [undefined, ...profiles]) {
    const first = createNativeSourceProject(source, {profile, author: 'Author', toc: true, tocDepth: 4,
      pdfImages: [{destination: 'chart.png', bytes: Uint8Array.of(1, 2, 3)}],
      fontAssets: [{slot: 'body-regular', bytes: Uint8Array.of(4, 5, 6), weight: 555}]});
    const restored = readNativeSourceProject(first.bytes);
    assert.equal(restored.source, source); assert.equal(restored.options.profile, profile);
    assert.equal(Object.hasOwn(restored.options, 'profile'), profile !== undefined);
    assert.deepEqual(createNativeSourceProject(restored.source, restored.options).bytes, first.bytes);
    const f = fixture(), exporter = await createWorkspaceExporterWithLoader({wasm: magic, bindings: 'trusted adapter'}, async () => f.bindings);
    const saved = exporter.renderProject(first.bytes), data = savedPayload(saved);
    assert.equal(data.options.profile, profile); assert.deepEqual(data.images, [{destination: 'chart.png', bytes: 'AQID'}]);
    const next = createNativeWorkspaceRenderer(f.bindings, data);
    next.html(source); assert.equal(f.calls.at(-1).name, 'renderHtmlConfiguredMetadata');
    assert.deepEqual(f.calls.at(-1).args.slice(21), [profile, 'Author']);
    next.pdf(source); assert.equal(f.calls.at(-1).args[0], source);
    assert.equal(saved.diagnostics.some(item => item.code === 'default_shell_dialect'), profile === undefined);
    assert.ok(saved.diagnostics.some(item => item.code === 'renderHtmlConfiguredMetadata'));
  }
});

test('serialized workspace factories retain profile validation without module closures', () => {
  const context = {atob, TextEncoder, TextDecoder, Uint8Array, Uint32Array, Float64Array};
  const create = vm.runInNewContext('(' + createNativeWorkspaceRenderer.toString() + ')', context);
  const codec = vm.runInNewContext('(' + createNativeSourceProjectCodec.toString() + ')', context)();
  const f = fixture(), p = payload({profile: 'commonmark-gfm'});
  const data = codec.decode(codec.encode({source, options: p.options, images: p.images, fonts: p.fonts}).bytes);
  create(f.bindings, {version: 1, ...data}).pdf(source);
  assert.equal(f.calls.at(-1).args.at(-1), 'commonmark-gfm');
});

function workerFactory(code) {
  const thread = new Worker(`const {parentPort} = require('node:worker_threads');
    globalThis.self = globalThis; globalThis.Blob = class {constructor(parts) {this.text = parts.join('');}};
    URL.createObjectURL = blob => 'data:text/javascript;base64,' + Buffer.from(blob.text).toString('base64');
    URL.revokeObjectURL = () => {};
    globalThis.postMessage = (data, transfer) => parentPort.postMessage(data, transfer);
    parentPort.on('message', data => globalThis.onmessage({data})); ${code}`, {eval: true});
  const listeners = new Map();
  return {addEventListener(type, callback) {
    const listener = data => callback(type === 'message' ? {data} : data); listeners.set(callback, listener); thread.on(type, listener);
  }, removeEventListener(type, callback) {const listener = listeners.get(callback); if (listener) thread.off(type, listener); listeners.delete(callback);},
  postMessage: data => thread.postMessage(data), terminate: () => thread.terminate()};
}
test('real background workers retain the captured dialect for preview, publications and analysis', async () => {
  const bindings = `export default async ({module_or_path}) => {await WebAssembly.instantiate(module_or_path);};
    const output = ${output.toString()};
    ${routes.map(([name, arity, mime]) => `export const ${name} = Object.defineProperty((...args) => output('${name}', '${mime}', args), 'length', {value: ${arity}});`).join('\n')}
    export const documentStats = () => '{}'; export const accessibilityAudit = () => '{}';
    export const documentStatsProfiled = (source, profile) => JSON.stringify({source, profile,
      schema: 'fmd-document-stats-v1', bytes: new TextEncoder().encode(source).length,
      lines: 1, words: 1, characters: 1, sentences: 1, syllables: 1,
      reading_time_secs: 1, speaking_time_secs: 1, flesch_reading_ease: 80,
      flesch_kincaid_grade: 3, reading_ease_label: 'Easy', structure: {}, outline: [], findings: []});
    export const accessibilityAuditProfiled = (source, profile) => JSON.stringify({source, profile,
      schema_version: '1', target: 'pdf', findings: []});`;
  const p = {...payload(), wasm: 'AGFzbQEAAAA=', bindings};
  const worker = createWorkspacePreviewWorker(createNativeWorkspaceRenderer, p, {workerFactory, timeoutMs: 10000});
  try {
    const owner = createNativeWorkspaceRenderer(fixture().bindings, payload({profile: 'commonmark-gfm'}));
    const state = {options: owner.settings, images: p.images};
    const pending = worker.render(source, {scale: 1}, state);
    owner.stageSettings({profile: 'gfm-plus'}).commit(); state.options = owner.settings;
    const preview = await pending;
    assert.match(preview.html, /commonmark-gfm/); assert.doesNotMatch(preview.html, /gfm-plus/);
    for (const format of ['html', 'pdf', 'epub', 'svg']) {
      const result = await worker.exportDocument(format, source, state);
      assert.match(new TextDecoder().decode(result.bytes), /gfm-plus/);
    }
    for (const kind of ['stats', 'accessibility']) {
      const report = await worker.analyzeDocument(kind, source, state);
      assert.equal(report.report.source, source); assert.equal(report.report.profile, 'gfm-plus');
    }
  } finally {worker.dispose();}
});
