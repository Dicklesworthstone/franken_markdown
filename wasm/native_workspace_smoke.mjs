// Genuine built-package probe, not an ABI adapter. DSR supplies PACKAGE and a
// new retained output path. Never overwrite an earlier proof artifact.
import assert from 'node:assert/strict';
import {readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';

const [directory, output] = process.argv.slice(2);
if (!directory || !output) throw new Error('Usage: node wasm/native_workspace_smoke.mjs PACKAGE OUTPUT.html');
const root = path.resolve(directory);
const {createNativeWorkspaceExporter} = await import(pathToFileURL(path.join(root, 'native_workspace.js')).href);
const wasm = await readFile(path.join(root, 'pkg/franken_markdown_bg.wasm'));
const bindings = await readFile(path.join(root, 'pkg/franken_markdown.js'), 'utf8');
const exporter = await createNativeWorkspaceExporter({wasm, bindings});
const source = '# Native standalone\n\nMath $x^2$ and **bold**.\n\n![Chart](chart.svg)\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nNote[^n].\n\n[^n]: Preserved footnote.\n';
const asset = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="red"/></svg>');
const options = {font:'serif', title:'Native standalone', toc:true, pageNumbers:true, metadataEpochSeconds:0,
  pdfImages:[{destination:'chart.svg', bytes:asset}]};
const first = exporter.render(source, options), again = exporter.render(source, options);
assert.deepEqual(first.bytes, again.bytes);
assert.equal(first.sourceLength, new TextEncoder().encode(source).length);
const html = first.text();
for (const value of ['id="fmd-native-runtime"', 'id="fmd-native-bootstrap"', 'sandbox="allow-same-origin"',
  'Content-Security-Policy', 'data:image/svg+xml;base64,', '&lt;table', 'bootNativeWorkspace']) assert.ok(html.includes(value), value);
const marker = '<script type="application/json" id="fmd-native-runtime">';
const start = html.indexOf(marker) + marker.length;
const payload = JSON.parse(html.slice(start, html.indexOf('</script>', start)));
assert.deepEqual(Buffer.from(payload.wasm,'base64'), wasm);
assert.equal(payload.bindings, bindings);
assert.equal(payload.images[0].destination,'chart.svg');
assert.deepEqual(Buffer.from(payload.images[0].bytes,'base64'), Buffer.from(asset));
assert.equal(payload.options.metadataEpochSeconds,0);
await writeFile(path.resolve(output), first.bytes, {flag:'wx'});
console.log(JSON.stringify({proof:'built-Rust-WASM-native-workspace', bytes:first.bytes.length,
  deterministic:true, diagnostics:first.diagnostics, output:path.resolve(output)}));
