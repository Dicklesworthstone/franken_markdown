import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile, writeFile, mkdir, mkdtemp} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, dirname, relative, resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {fixture} from './native_pdf_proof_fixture.mjs';

const root = fileURLToPath(new URL('./', import.meta.url));
const manifest = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
const builders = ['check-wasm-package.sh', 'dsr-wasm-package.sh'];
const required = ['native_pdf_proof.mjs', 'native_pdf_proof_ui.mjs', 'NATIVE_PDF_PROOF.md'];

async function closure(entry, seen = new Set()) {
  if (seen.has(entry)) return seen;
  seen.add(entry);
  assert.ok(manifest.files.includes(entry), `Missing npm inventory: ${entry}`);
  const source = await readFile(join(root, entry), 'utf8');
  for (const match of source.matchAll(/(?:import|export)\s+[^;]*?\sfrom\s*['"](\.[^'"]+)['"]/g)) {
    const next = relative(root, resolve(root, dirname(entry), match[1]));
    assert.ok(!next.startsWith('..'), 'Package imports must remain in the package');
    await closure(next, seen);
  }
  return seen;
}

test('proof helpers ship in npm and in both explicit package copy loops', async () => {
  for (const file of required) assert.ok(manifest.files.includes(file), file);
  for (const builder of builders) {
    const script = await readFile(new URL('../scripts/' + builder, import.meta.url), 'utf8');
    const loop = script.match(/for file in ([\s\S]*?); do\s*cp "wasm\/\$file"/);
    assert.ok(loop, builder + ' copy loop');
    const files = loop[1].replace(/\\\n/g, ' ').trim().split(/\s+/);
    for (const file of required) assert.ok(files.includes(file), builder + ': ' + file);
    assert.match(script, /node --test wasm\/native_pdf_proof\.test\.mjs wasm\/native_pdf_proof_package\.test\.mjs/);
  }
});

test('isolated exporter loads solely from its declared transitive package files', async () => {
  const files = await closure('native_workspace.js');
  for (const file of ['interactive_export.mjs', 'interactive_runtime.mjs', 'interactive_preview.mjs',
    'pdf_page.mjs', 'native_pdf_proof.mjs', 'native_pdf_proof_ui.mjs']) assert.ok(files.has(file), file);
  const target = await mkdtemp(join(tmpdir(), 'fmd-proof-package-'));
  await writeFile(join(target, 'package.json'), JSON.stringify(manifest));
  for (const file of files) {
    const path = join(target, file); await mkdir(dirname(path), {recursive: true});
    await writeFile(path, await readFile(join(root, file)));
  }
  // No generated pkg/, repo-relative module lookup or implicit WASM initialization.
  const imported = await import(pathToFileURL(join(target, 'native_workspace.js')).href);
  assert.equal(typeof imported.createNativeWorkspaceExporter, 'function');
  assert.equal(imported.NATIVE_WORKSPACE_LIMITS.sourceBytes, 32 * 1024 * 1024);
});

test('standalone assembly embeds proof code before the production lossless source host', async () => {
  const html = await fixture();
  assert.ok(html.includes('function registerNativePdfProof'));
  assert.ok(html.includes('function createNativePdfProof'));
  assert.ok(html.indexOf('function registerNativePdfProof') < html.indexOf('// fmd-pdf-proof-host-v1'));
  assert.ok(html.includes('source: currentSource'));
  assert.ok(html.includes("copy.querySelector('body > dialog#fmd-pdf-proof')?.remove()"));
  assert.ok(html.includes('object-src blob:'));
  assert.ok(html.includes("frame-src 'self' about: blob:"));
  assert.ok(html.includes('sandbox="allow-same-origin"'));
  assert.ok(html.includes('default-src &#39;none&#39;; img-src data:; font-src data:;'));
  assert.ok(!html.includes("'unsafe-eval'"));
  assert.ok(!html.includes('<script src='));
});
