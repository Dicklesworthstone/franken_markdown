import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
const read = name => readFile(new URL(name, import.meta.url), 'utf8');
const required = ['native_source_project.mjs', 'native_source_project_ui.mjs', 'NATIVE_SOURCE_PROJECT.md'];

test('project modules and guide are included in the existing public exporter package', async () => {
  const manifest = JSON.parse(await read('./package.json'));
  for (const file of required) { assert.ok(manifest.files.includes(file), file); assert.ok((await read('./' + file)).length); }
  assert.equal(manifest.exports['./interactive/exporter'].import, './native_workspace.js');
  const entry = await read('./native_workspace.js');
  for (const name of ['createNativeSourceProject', 'readNativeSourceProject', 'NATIVE_SOURCE_PROJECT_LIMITS']) assert.ok(entry.includes(name), name);
  assert.ok(!manifest.files.some(name => name.includes('native_source_project_fixture')), 'Explicit test adapters must not ship');
});
for (const script of ['check-wasm-package.sh', 'dsr-wasm-package.sh']) test(script + ' retains explicit assembly and native qualification', async () => {
  const text = await read('../scripts/' + script);
  const match = text.match(/for file in ([\s\S]*?); do\s*cp "wasm\/\$file"/);
  assert.ok(match);
  const files = match[1].replace(/\\\n/g, ' ').trim().split(/\s+/);
  for (const file of required) assert.ok(files.includes(file), file);
  assert.match(text, /node --test wasm\/native_source_project\*\.test\.mjs/);
  assert.match(text, /node wasm\/native_source_project_smoke\.mjs/);
  // Existing native-package qualification remains in place, not replaced by a
  // source/adapter test or a higher artifact size threshold.
  assert.match(text, /wasm-bindgen/);
  assert.match(text, /(?:BUDGET_RAW=7900000|len\(d\)<=7_900_000)/);
  assert.match(text, /(?:BUDGET_GZIP=3400000|<=3_400_000)/);
  assert.match(text, /cmp /);
});
test('native smoke invokes only package entry points and explicit matching artifacts', async () => {
  const source = await read('./native_source_project_smoke.mjs');
  assert.ok(!source.includes('projectExporterFixtureModule'));
  assert.ok(!source.includes('native_source_project_fixture'));
  assert.match(source, /createNativeWorkspaceExporter\(\{wasm, bindings\}\)/);
  assert.match(source, /renderProject\(project.bytes\)/);
  assert.match(source, /renderPdf/);
});
