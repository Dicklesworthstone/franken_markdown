import assert from 'node:assert/strict';
import {test} from 'node:test';
import {Script} from 'node:vm';
import {pdfPageGeometry} from './pdf_page.mjs';
import {registerNativePdfProof} from './native_pdf_proof_ui.mjs';
import {projectExporterFixtureModule, projectFixtureEngine, nativeSourceProjectFixture, PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS} from './native_source_project_fixture.mjs';
const api = await projectExporterFixtureModule();
const runtime = () => ({wasm: new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]), bindings: '// trusted host runtime'});

test('public creation requires no native factory or runtime initialization', () => {
  const out = api.createNativeSourceProject(PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS());
  assert.equal(out.format, 'native-source-project'); assert.equal(out.filename('example'), 'example.fmd.json');
  const restored = api.readNativeSourceProject(out.bytes);
  assert.equal(restored.source, PROJECT_FIXTURE_SOURCE); assert.equal(restored.options.fontAssets[0].weight, 525);
  assert.equal(restored.options.pdfImages.length, 2); assert.equal(restored.options.allowRawHtml, false);
});
test('public creation/read/creation has canonical byte parity over complete options', () => {
  const first = api.createNativeSourceProject(PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS());
  const restored = api.readNativeSourceProject(first.bytes);
  const second = api.createNativeSourceProject(restored.source, restored.options);
  assert.deepEqual(second.bytes, first.bytes);
});
test('encoded project geometry returns exactly to the shared native ABI normalizer', () => {
  for (const page of [undefined, {}, {size: 'a4', orientation: 'landscape', margins: 35.375}, {size: {widthPt: 600.125, heightPt: 800.25}, margins: {leftPt: 44.0625}}]) {
    const out = api.createNativeSourceProject('x', {page});
    const {options} = api.readNativeSourceProject(out.bytes);
    assert.deepEqual(pdfPageGeometry(options.page), pdfPageGeometry(page));
    assert.equal(Object.hasOwn(options, 'page'), page !== undefined);
  }
});
test('assets are captured before the returned project is exposed', () => {
  const options = PROJECT_FIXTURE_OPTIONS(), source = PROJECT_FIXTURE_SOURCE;
  const output = api.createNativeSourceProject(source, options);
  options.pdfImages[0].bytes.fill(0); options.fontAssets[0].bytes.fill(0); options.title = 'later';
  const restored = api.readNativeSourceProject(output.bytes);
  assert.equal(restored.options.title, 'Portable source'); assert.equal(restored.options.pdfImages[0].bytes[0], 60);
  assert.deepEqual([...restored.options.fontAssets[0].bytes], [0, 1, 2, 3]);
});
test('pooled byte subviews and DataViews preserve only the selected resource bytes', () => {
  const backing = new Uint8Array([9, 1, 2, 3, 9]);
  const out = api.createNativeSourceProject('', {pdfImages: [{destination: 'a', bytes: new DataView(backing.buffer, 1, 3)}]});
  assert.deepEqual([...api.readNativeSourceProject(out.bytes).options.pdfImages[0].bytes], [1, 2, 3]);
});
test('metadata omission, epoch zero and exact metadata newlines survive the public workflow', () => {
  for (const metadataEpochSeconds of [undefined, 0, 1700000000]) {
    const first = api.createNativeSourceProject('', {metadataEpochSeconds, title: 'a\r\nb\r'});
    const {options} = api.readNativeSourceProject(first.bytes);
    assert.equal(options.metadataEpochSeconds, metadataEpochSeconds); assert.equal(options.title, 'a\r\nb\r');
    assert.equal(Object.hasOwn(options, 'metadataEpochSeconds'), metadataEpochSeconds !== undefined);
  }
});
test('unsafe and unsupported creation settings fail instead of silently losing options', () => {
  for (const options of [{allowRawHtml: true}, {customCss: 'body{}'}, {running: {}}, {wasm: new Uint8Array([1])}, {fontScale: 'lg'}])
    assert.throws(() => api.createNativeSourceProject('', options));
});
test('renderProject uses only the exporter-selected engine and preserves its full input', async () => {
  const first = projectFixtureEngine('selected-A'), second = projectFixtureEngine('selected-B');
  let loads = 0;
  const a = await api.createWorkspaceExporterWithLoader(runtime(), async () => { loads++; return first.engine; });
  const b = await api.createWorkspaceExporterWithLoader(runtime(), async () => { loads++; return second.engine; });
  const input = api.createNativeSourceProject(PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS());
  const actual = a.renderProject(input.bytes), direct = a.render(PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS());
  assert.deepEqual(actual.bytes, direct.bytes); assert.equal(loads, 2);
  assert.equal(first.captures[0].source, PROJECT_FIXTURE_SOURCE);
  assert.equal(first.captures[0].payload.options.metadataEpochSeconds, 0);
  assert.equal(first.captures[0].payload.images.length, 2); assert.equal(first.captures[0].payload.fonts[0].weight, 525);
  assert.notDeepEqual(b.renderProject(input.bytes).bytes, actual.bytes);
});
test('project schema cannot replace the selected runtime or inject bindings', async () => {
  const {engine, captures} = projectFixtureEngine();
  const exporter = await api.createWorkspaceExporterWithLoader(runtime(), async () => engine);
  const base = JSON.parse(api.createNativeSourceProject('document').text());
  for (const field of ['wasm', 'bindings', 'runtime', 'url', '__proto__']) {
    const value = JSON.parse(JSON.stringify(base)); Object.defineProperty(value, field, {value: 'throw Error("untrusted")', enumerable: true});
    assert.throws(() => exporter.renderProject(JSON.stringify(value)), {code: 'INVALID_SOURCE_PROJECT'});
  }
  assert.equal(captures.length, 0);
});
test('invalid project is rejected before any renderer call', async () => {
  const {engine, captures} = projectFixtureEngine();
  const exporter = await api.createWorkspaceExporterWithLoader(runtime(), async () => engine);
  assert.throws(() => exporter.renderProject('<html>not a project</html>'));
  const base = JSON.parse(api.createNativeSourceProject('x').text()); base.options.pageGeometry = [100, 100, 0, 0, 0, 0];
  assert.throws(() => exporter.renderProject(JSON.stringify(base)));
  assert.equal(captures.length, 0);
});
test('restored mutable options permit deliberate edits without mutating the original project', () => {
  const first = api.createNativeSourceProject(PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS());
  const restored = api.readNativeSourceProject(first.bytes); restored.options.title = 'Edited'; restored.options.pdfImages[0].bytes[0] = 0;
  const next = api.createNativeSourceProject(restored.source + ' edit', restored.options);
  assert.equal(api.readNativeSourceProject(next.bytes).options.title, 'Edited');
  assert.equal(api.readNativeSourceProject(first.bytes).options.title, 'Portable source');
});
test('bootstrap contains self-contained project code and remains valid JavaScript', async () => {
  const html = await nativeSourceProjectFixture();
  assert.ok(html.includes('function installNativeSourceProjectDownload'));
  assert.ok(html.includes('function createNativeSourceProjectCodec'));
  assert.ok(html.includes('function(host)'));
  const bootstrap = html.match(/<script id="fmd-native-bootstrap">([\s\S]*?)<\/script>/)[1];
  new Script(bootstrap);
  assert.ok(html.indexOf('function installNativeSourceProjectDownload') < html.indexOf('// fmd-async-preview-v1'));
  assert.ok(html.includes('worker-src blob:')); assert.ok(!html.includes('<script src='));
  assert.ok(!html.includes("'unsafe-eval'"));
});
test('source script-end sequences and code-like resources remain data in projects and HTML', async () => {
  const source = '</script><script>throw Error("source")</script> <!-- 😀';
  const {engine} = projectFixtureEngine();
  const exporter = await api.createWorkspaceExporterWithLoader(runtime(), async () => engine);
  const out = api.createNativeSourceProject(source, {title: source});
  assert.ok(!out.text().includes('</script>'));
  assert.equal(api.readNativeSourceProject(out.bytes).source, source);
  const html = exporter.renderProject(out.bytes).text();
  assert.ok(!html.includes('<script>throw Error("source")'));
  const bootstrap = html.match(/<script id="fmd-native-bootstrap">([\s\S]*?)<\/script>/)[1]; new Script(bootstrap);
});
test('existing lossless host is forwarded once after PDF UI registration, not reconstructed', () => {
  // Loading-state DOM adapter defers the existing PDF UI. The optional tool host
  // forwards immediately and arranges its own DOM-ready installation.
  const listeners = [], host = {source: () => '\ufeffexact\r\n', ready: () => true, download() {}};
  const runtime = {beginExportDocument() {}};
  const original = globalThis.document;
  globalThis.document = {readyState: 'loading', addEventListener: (...args) => listeners.push(args)};
  try {
    let seen, calls = 0;
    registerNativePdfProof(() => { throw Error('PDF UI is deferred'); }, runtime, value => { calls++; seen = value; });
    runtime.installPdfProof(host); runtime.installPdfProof(host);
    assert.equal(calls, 1); assert.equal(seen, host); assert.equal(seen.source(), '\ufeffexact\r\n');
    assert.equal(listeners.length, 1);
  } finally { if (original === undefined) delete globalThis.document; else globalThis.document = original; }
});
test('legacy two-argument PDF registrar and host validation remain unchanged', () => {
  const runtime = {beginExportDocument() {}}, previous = globalThis.document;
  globalThis.document = {readyState: 'loading', addEventListener() {}};
  try {
    registerNativePdfProof(() => ({}), runtime);
    assert.throws(() => runtime.installPdfProof({}), /lossless/);
    runtime.installPdfProof({source: () => '', ready: () => true, download() {}});
  } finally { if (previous === undefined) delete globalThis.document; else globalThis.document = previous; }
});
