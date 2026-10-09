import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync, mkdtempSync, mkdirSync, copyFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const wasm = fileURLToPath(new URL('../', import.meta.url));
const repo = dirname(wasm.replace(/[\\/]$/, ''));
const read = path => readFileSync(path, 'utf8');
// The proof helpers' full relative import closure, plus their documentation.
const required = [
  'book_pdf_proof.mjs', 'book_session.mjs', 'book_worker.mjs', 'book_retained.mjs', 'pdf_page.mjs',
  'demo/book_pdf_controls.mjs', 'BOOK_PDF_PROOF.md',
];

test('PDF proof runtime closure and documentation are declared in the published package', () => {
  const manifest = JSON.parse(read(join(wasm, 'package.json')));
  for (const path of required) assert(manifest.files.includes(path), `package omits ${path}`);
  assert.equal(new Set(manifest.files).size, manifest.files.length);
  assert.match(read(join(wasm, 'demo/book_pdf_controls.mjs')), /from "\.\.\/book_pdf_proof\.mjs"/);
});

for (const name of ['check-wasm-package.sh', 'dsr-wasm-package.sh']) {
  test(`${name} stages the PDF helpers with an importable dependency closure`, async () => {
    const script = read(join(repo, 'scripts', name)), paths = new Set();
    for (const match of script.matchAll(/for file in ([\s\S]*?); do\s+cp "wasm\/(demo\/)?\$file" "\$(?:PACKAGE|package_dir)\/(?:demo\/)?\$file"\s+done/g)) {
      for (const file of match[1].replace(/\\\n/g, ' ').trim().split(/\s+/)) paths.add((match[2] ?? '') + file);
    }
    for (const path of required) assert(paths.has(path), `${name} omits ${path}`);
    assert.match(script, /node --test wasm\/tests\/book_pdf\*\.test\.mjs/);
    const stage = mkdtempSync(join(tmpdir(), 'fmd-book-pdf-package-'));
    for (const path of required) {
      mkdirSync(dirname(join(stage, path)), { recursive: true });
      copyFileSync(join(wasm, path), join(stage, path));
    }
    assert.equal(typeof (await import(pathToFileURL(join(stage, 'demo/book_pdf_controls.mjs')))).createBookPdfControls, 'function');
    assert.equal(typeof (await import(pathToFileURL(join(stage, 'book_pdf_proof.mjs')))).createBookPdfProof, 'function');
  });
}

async function bootstrap(fail = false) {
  // Load the UNCHANGED publisher entrypoint as a real ES module. Only imports
  // for the independent subsystems are explicitly stubbed in the staging tree.
  // Actual proof behavior is covered by the session/controller/browser suites.
  const stage = mkdtempSync(join(tmpdir(), 'fmd-book-pdf-bootstrap-'));
  mkdirSync(join(stage, 'demo'));
  writeFileSync(join(stage, 'package.json'), '{"type":"module"}');
  copyFileSync(join(wasm, 'demo/book.js'), join(stage, 'demo/book.js'));
  const events = new Map(), made = [], workers = [], lifecycle = [];
  const statuses = new Map();
  const host = {
    worker() { const value = { id: workers.length, dispose() { lifecycle.push(`worker-${value.id}:dispose`); } }; workers.push(value); return value; },
    create(name, options) {
      made.push({ name, options });
      if (name === 'pdf' && fail) throw Error('proof unavailable');
      return Object.fromEntries(['suspend', 'resume', 'dispose', 'detach'].map(action => [action, () => lifecycle.push(`${name}:${action}`)]));
    },
  };
  const dependencies = {
    'book-worker.js': 'export const createBookWorker = () => globalThis.__bookPdfBoot.worker();',
    'demo/book_collection.mjs': 'export const createBookCollection = () => ({identity:"collection"});',
    'demo/book_controls.mjs': 'export const createBookControls = options => globalThis.__bookPdfBoot.create("controls", options);',
    'demo/book_pdf_controls.mjs': 'export const createBookPdfPanel = () => {}; export const createBookPdfControls = options => globalThis.__bookPdfBoot.create("pdf", options);',
    // Font and image authoring joined the publisher after this stub list
    // (d15040f, f905593); each is an independent subsystem like the others.
    'demo/book_image_controls.mjs': 'export const createBookImagePanel = () => {}; export const createBookImageControls = options => globalThis.__bookPdfBoot.create("images", options);',
    'demo/book_font_controls.mjs': 'export const createBookFontPanel = () => {}; export const createBookFontControls = options => globalThis.__bookPdfBoot.create("fonts", options);',
    'demo/book_inspection_controls.mjs': 'export const createBookInspectionPanel = () => {}; export const createBookLinkPanel = () => {}; export const createBookInspectionControls = options => globalThis.__bookPdfBoot.create("inspection", options); export const createBookLinkControls = options => globalThis.__bookPdfBoot.create("links", options);',
    ...Object.fromEntries(['Library', 'Preview', 'Search'].map(name => [
      `demo/book_${name.toLowerCase()}_controls.mjs`,
      `export const createBook${name}Controls = options => globalThis.__bookPdfBoot.create("${name.toLowerCase()}", options);`,
    ])),
  };
  for (const [path, source] of Object.entries(dependencies)) writeFileSync(join(stage, path), source);
  const old = Object.fromEntries(['document', 'window', '__bookPdfBoot'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  globalThis.__bookPdfBoot = host;
  globalThis.document = { querySelector(id) { if (!statuses.has(id)) statuses.set(id, {}); return statuses.get(id); } };
  globalThis.window = { confirm() { return true; }, addEventListener(type, listener) { events.set(type, listener); } };
  try {
    await import(pathToFileURL(join(stage, 'demo/book.js')));
    return { made, workers, lifecycle, statuses, events };
  } finally {
    for (const [key, descriptor] of Object.entries(old)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  }
}

test('publisher bootstrap gives proofing its own worker and retires it before the source owner', async () => {
  const boot = await bootstrap();
  const proof = boot.made.find(item => item.name === 'pdf');
  assert(proof, 'publisher never installs PDF proofing');
  for (const other of boot.made.filter(item => item.name !== 'pdf' && item.options.worker))
    assert.notEqual(proof.options.worker, other.options.worker, other.name);
  const count = boot.workers.length;
  boot.events.get('pagehide')({ persisted: true });
  assert(boot.lifecycle.indexOf('pdf:suspend') < boot.lifecycle.indexOf('controls:suspend'));
  boot.events.get('pageshow')({ persisted: true });
  assert(boot.lifecycle.includes('pdf:resume'));
  assert.equal(boot.workers.length, count);
  boot.events.get('pagehide')({ persisted: false });
  assert(boot.lifecycle.indexOf('pdf:dispose') < boot.lifecycle.indexOf('controls:dispose'));
});

test('proof initialization failure disposes only its worker and leaves the remaining publisher features installed', async () => {
  const boot = await bootstrap(true);
  const proof = boot.made.find(item => item.name === 'pdf');
  assert(proof, 'publisher never attempts PDF proofing');
  assert.deepEqual(boot.lifecycle, [`worker-${proof.options.worker.id}:dispose`]);
  assert.match(boot.statuses.get('#book-pdf-status').textContent, /Ordinary PDF\/EPUB\/site exports/);
  for (const name of ['controls', 'library', 'preview', 'search', 'inspection', 'links'])
    assert(boot.made.some(item => item.name === name), `lost ${name}`);
});
