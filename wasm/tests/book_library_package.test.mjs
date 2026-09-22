// Static inventory checks: not generated-WASM, native storage or browser proof.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { posix } from "node:path";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
const manifest = JSON.parse(read("wasm/package.json"));
const files = [
  "demo/book_library_store.mjs",
  "demo/book_library_session.mjs",
  "demo/book_library_controls.mjs",
];
test("library runtime modules and their direct imports are declared for publication", () => {
  for (const path of files) {
    assert(manifest.files.includes(path), path);
    for (const match of read(`wasm/${path}`).matchAll(/from\s+["']([^"']+)["']/g)) {
      const dependency = posix.normalize(posix.join(posix.dirname(path), match[1]));
      assert(manifest.files.includes(dependency), `${path} imports missing ${dependency}`);
    }
  }
  assert(manifest.files.includes("LIBRARY.md"));
  assert(read("wasm/LIBRARY.md").includes("Autosave"));
});
test("both package assemblers copy each library dependency and its usage guide", () => {
  for (const script of ["scripts/check-wasm-package.sh", "scripts/dsr-wasm-package.sh"]) {
    const text = read(script);
    const loops = [...text.matchAll(/for file in ([\s\S]*?); do\s+cp "wasm\/([^\n]+)"/g)];
    const demo = loops.find((match) => match[2].startsWith("demo/"));
    const root = loops.find((match) => match[2].startsWith("$file"));
    assert(demo, script);
    assert(root, script);
    for (const file of files)
      assert(demo[1].split(/\s+/).includes(posix.basename(file)), `${script}: ${file}`);
    assert(root[1].split(/\s+/).includes("LIBRARY.md"), script);
  }
});
test("publisher entry starts library independently and tears it down before source disposal", () => {
  const entry = read("wasm/demo/book.js"),
    html = read("wasm/demo/book.html");
  assert(entry.includes('from "./book_library_controls.mjs"'));
  assert(entry.includes("library?.dispose(); controls.dispose();"));
  for (const id of [
    "library-save",
    "library-copy",
    "library-open",
    "library-new",
    "library-delete",
    "library-autosave",
    "library-books",
    "library-versions",
  ]) {
    assert.equal([...html.matchAll(new RegExp(`id="${id}"`, "g"))].length, 1, id);
  }
});
