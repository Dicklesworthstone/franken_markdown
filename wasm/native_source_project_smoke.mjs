// Qualification probe: requires an actual matching generated Rust/WASM package.
// No fixture loader, fallback engine, network fetch or compiler is used here.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {resolve, join} from 'node:path';
import {pathToFileURL} from 'node:url';

const [directory, wasmPath] = process.argv.slice(2);
if (!directory || !wasmPath || process.argv.length !== 4)
  throw Error('Usage: node wasm/native_source_project_smoke.mjs PACKAGE MATCHING_WASM');
const packageDir = resolve(directory);
const api = await import(pathToFileURL(join(packageDir, 'native_workspace.js')).href);
const wasm = new Uint8Array(await readFile(wasmPath));
const bindings = await readFile(join(packageDir, 'pkg/franken_markdown.js'), 'utf8');
const source = '\ufeff# Portable publication\r\n\r\nExact naïve Unicode.\r\n\r\n![Chart](chart.svg)\r\n';
const svg = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect x="2" y="2" width="36" height="26" fill="#246"/></svg>');
const options = {font: 'serif', fontScale: 1.125, title: 'Portable publication', author: 'Probe',
  lang: 'en', metadataEpochSeconds: 0, toc: true, tocDepth: 2, pageNumbers: true,
  page: {size: {widthPt: 600.125, heightPt: 800.25}, margins: {topPt: 30, rightPt: 35, bottomPt: 40, leftPt: 45}},
  pdfImages: [{destination: 'chart.svg', bytes: svg}, {destination: 'unused.svg', bytes: svg}]};
const project = api.createNativeSourceProject(source, options);
const restored = api.readNativeSourceProject(project.bytes);
assert.equal(restored.source, source);
assert.equal(restored.options.metadataEpochSeconds, 0);
assert.deepEqual(restored.options.pdfImages.map(asset => asset.destination), ['chart.svg', 'unused.svg']);
assert.deepEqual(restored.options.pdfImages[0].bytes, svg);
assert.deepEqual(api.createNativeSourceProject(restored.source, restored.options).bytes, project.bytes);
const exporter = await api.createNativeWorkspaceExporter({wasm, bindings});
const directWorkspace = exporter.render(source, options);
const restoredWorkspace = exporter.renderProject(project.bytes);
assert.deepEqual(restoredWorkspace.bytes, directWorkspace.bytes, 'Restored workspace must equal ordinary workspace output');
assert.ok(restoredWorkspace.text().includes('function installNativeSourceProjectDownload'));
assert.ok(restoredWorkspace.text().includes('function installNativeSourceProjectOpen'));
assert.ok(restoredWorkspace.text().includes('function createNativeWorkspacePublisher'));

const renderer = await import(pathToFileURL(join(packageDir, 'franken_markdown.js')).href);
await renderer.init(wasm.slice());
for (const [format, method] of [['html', 'renderHtml'], ['pdf', 'renderPdf']]) {
  const expected = await renderer[method](source, options);
  const actual = await renderer[method](restored.source, restored.options);
  assert.deepEqual(actual.bytes, expected.bytes, format + ' round-trip parity');
  assert.deepEqual(actual.diagnostics, expected.diagnostics, format + ' diagnostic parity');
  console.log('Native source-project round-trip:', format, actual.bytes.length, 'bytes, identical');
}
const invalid = JSON.parse(project.text()); invalid.bindings = 'throw Error("must not execute")';
assert.throws(() => exporter.renderProject(JSON.stringify(invalid)), {code: 'INVALID_SOURCE_PROJECT'});
console.log('Native source-project smoke passed:', project.bytes.length, 'project bytes;', restoredWorkspace.bytes.length, 'workspace bytes');
