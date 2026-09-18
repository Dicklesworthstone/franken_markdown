// Package/entrypoint inventory checks, not generated-WASM or browser proof.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
const read = path => readFileSync(new URL(path, import.meta.url), "utf8");
const rootFiles = ["book-worker.js", "book-worker.d.ts", "book_worker.mjs", "book_worker_entry.js", "BOOK.md"];
const demoFiles = ["book.html", "book.js", "book_collection.mjs", "book_controls.mjs"];
const manifest = JSON.parse(read("../package.json"));
test("the public worker and complete publishing workbench are in the package manifest", () => {
  assert.deepEqual(manifest.exports["./book-worker"], { types: "./book-worker.d.ts", import: "./book-worker.js" });
  assert.equal(manifest.exports["./book-worker/worker"], "./book_worker_entry.js");
  for (const path of [...rootFiles, ...demoFiles.map(path => `demo/${path}`)]) {
    assert(manifest.files.includes(path), path);
    assert(existsSync(new URL(`../${path}`, import.meta.url)), path);
  }
  for (const path of ["./book_worker_entry.js", "./demo/book.js"]) assert(manifest.sideEffects.includes(path), path);
});
for (const script of ["dsr-wasm-package.sh", "check-wasm-package.sh"]) test(`${script} copies every added runtime and demo dependency`, () => {
  const source = read(`../../scripts/${script}`);
  const loops = [...source.matchAll(/for file in ([\s\S]*?); do([\s\S]*?)done/g)];
  const copies = prefix => loops.filter(match => match[2].includes(`cp "wasm/${prefix}$file"`))
    .flatMap(match => match[1].replace(/\\\n/g, " ").trim().split(/\s+/));
  for (const file of rootFiles) assert(copies("").includes(file), file);
  for (const file of demoFiles) assert(copies("demo/").includes(file), file);
});
test("only the dedicated worker entry imports the actual book renderer", () => {
  assert.match(read("../book_worker_entry.js"), /import \* as book from "\.\/book\.js"/);
  assert.match(read("../book_worker_entry.js"), /installBookWorker\(self, book\)/);
  assert.match(read("../book-worker.js"), /new Worker\(new URL\("\.\/book_worker_entry\.js", import\.meta\.url\), \{ type: "module" \}\)/);
  assert.doesNotMatch(read("../demo/book.js"), /from ["'][^"']*(?:pkg\/|franken_markdown\.js)/);
  assert.match(read("../demo/book.js"), /createBookWorker\(\)/);
});
test("the live Flow editor opens publishing separately without losing its current document", () => {
  const source = read("../demo/flow-canvas.html");
  assert.match(source, /href="\.\/book\.html" target="_blank" rel="noopener"/);
  assert.match(source, /src="\.\/flow-source\.js"/);
  assert.match(source, /src="\.\/flow-canvas\.js"/);
  const html = read("../demo/book.html");
  const ids = [...html.matchAll(/id="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const match of html.matchAll(/(?:for|aria-labelledby|aria-describedby)="([^"]+)"/g)) {
    for (const id of match[1].split(/\s+/)) assert(ids.includes(id), id);
  }
  assert.match(html, /src="\.\/book\.js"/);
});
