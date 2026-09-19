// Static integration/inventory checks, not a generated-WASM package build.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { posix } from 'node:path';
const read=path=>readFileSync(new URL(`../../${path}`,import.meta.url),'utf8');
const manifest=JSON.parse(read('wasm/package.json'));
test('inspection modules, guide and their relative imports are declared for publication',()=>{
  for(const path of ['book_inspection.mjs','demo/book_inspection_controls.mjs','INSPECTION.md'])assert(manifest.files.includes(path),path);
  for(const path of ['book_inspection.mjs','demo/book_inspection_controls.mjs','book_worker_entry.js']){
    for(const match of read(`wasm/${path}`).matchAll(/from\s+["']([^"']+)["']/g))assert(manifest.files.includes(posix.normalize(posix.join(posix.dirname(path),match[1]))),`${path}: ${match[1]}`);
  }
});
test('both real package assemblers copy the inspection core, controller and guide',()=>{
  for(const path of ['scripts/check-wasm-package.sh','scripts/dsr-wasm-package.sh']){
    const loops=[...read(path).matchAll(/for file in ([\s\S]*?); do\s+cp "wasm\/([^\n]+)"/g)];
    const root=loops.find(v=>v[2].startsWith('$file')),demo=loops.find(v=>v[2].startsWith('demo/'));
    for(const name of ['book_inspection.mjs','INSPECTION.md'])assert(root?.[1].split(/\s+/).includes(name),`${path}: ${name}`);
    assert(demo?.[1].split(/\s+/).includes('book_inspection_controls.mjs'),path);
  }
});
test('actual publisher and worker entry retain previous features and attach the source-only inspector',()=>{
  const entry=read('wasm/demo/book.js'),worker=read('wasm/book_worker_entry.js');
  assert(entry.includes('createBookInspectionPanel(document)'));assert(entry.includes('maxOutputBytes: 2 * 1024 * 1024'));
  assert(entry.includes('inspection?.dispose(); search?.dispose(); preview?.dispose(); library?.dispose(); controls.dispose();'));
  for(const name of ['createBookSearchControls','createBookPreviewControls','createBookLibraryControls'])assert(entry.includes(name));
  assert(worker.includes('inspectBook: files => inspectBook(documents, files)'));assert(worker.includes('from "./franken_markdown.js"'));
  assert(read('wasm/book-worker.d.ts').includes('Promise<BookInspectionOutput>'));
});
