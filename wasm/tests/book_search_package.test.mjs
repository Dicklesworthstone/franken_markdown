// Static package and UI wiring checks, not native-browser or WASM evidence.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { posix } from "node:path";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
const manifest = JSON.parse(read("wasm/package.json"));
const modules = [
  "demo/book_controls.mjs",
  "demo/book_source_search.mjs",
  "demo/book_search_controls.mjs",
];
test("the new authoring modules and all direct imports are declared for publication", () => {
  for (const path of modules) {
    assert(manifest.files.includes(path), path);
    for (const match of read(`wasm/${path}`).matchAll(/from\s+["']([^"']+)["']/g)) {
      assert(
        manifest.files.includes(posix.normalize(posix.join(posix.dirname(path), match[1]))),
        `${path}: ${match[1]}`,
      );
    }
  }
  assert(manifest.files.includes("BOOK_EDITING.md"));
});
test("both package assemblers copy the real authoring modules and guide", () => {
  for (const path of ["scripts/check-wasm-package.sh", "scripts/dsr-wasm-package.sh"]) {
    const loops = [...read(path).matchAll(/for file in ([\s\S]*?); do\s+cp "wasm\/([^\n]+)"/g)];
    const root = loops.find((match) => match[2].startsWith("$file")),
      demo = loops.find((match) => match[2].startsWith("demo/"));
    assert(root?.[1].split(/\s+/).includes("BOOK_EDITING.md"), path);
    for (const name of ["book_controls.mjs", "book_source_search.mjs", "book_search_controls.mjs"])
      assert(demo?.[1].split(/\s+/).includes(name), `${path}: ${name}`);
  }
});
test("actual publisher entry initializes search and tears it down before the source owner", () => {
  const entry = read("wasm/demo/book.js");
  assert(entry.includes('import { createBookSearchControls } from "./book_search_controls.mjs"'));
  assert(entry.includes("createBookSearchControls({ root: document, controls, collection,"));
  assert(entry.includes("confirm: text => window.confirm(text)"));
  assert(
    entry.includes(
      "search?.suspend(); preview?.suspend(); library?.suspend(); controls.suspend();",
    ),
  );
  assert(
    entry.includes(
      "search?.dispose(); preview?.dispose(); library?.dispose(); controls.dispose();",
    ),
  );
  assert(entry.includes("search?.resume();"));
});
test("all actual search controller elements exist once in the publisher markup", () => {
  const html = read("wasm/demo/book.html"),
    controller = read("wasm/demo/book_search_controls.mjs");
  const ids = JSON.parse(`[${controller.match(/const ids = \[([\s\S]*?)\];/)[1]}]`);
  const found = [...html.matchAll(/id="([^"]+)"/g)].map((match) => match[1]);
  assert.equal(new Set(found).size, found.length, "duplicate HTML ids");
  for (const id of ids) assert.equal(found.filter((value) => value === id).length, 1, id);
  for (const id of ["search-query", "search-replacement", "search-scope", "search-results"])
    assert(html.includes(`for="${id}"`), id);
  assert(html.includes('id="search-status" role="status"'));
});
test("source tools do not introduce a renderer, persist source or start a worker", () => {
  const code =
    read("wasm/demo/book_search_controls.mjs") + read("wasm/demo/book_source_search.mjs");
  for (const forbidden of [
    /innerHTML/,
    /new Worker\(/,
    /fetch\(/,
    /localStorage/,
    /indexedDB/,
    /replaceProject\(/,
  ])
    assert(!forbidden.test(code), String(forbidden));
});
