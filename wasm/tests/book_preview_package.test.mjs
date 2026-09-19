// Static wiring/package checks, not a generated-WASM or browser acceptance gate.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { posix } from "node:path";
const read = path => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
const manifest = JSON.parse(read("wasm/package.json"));
const modules = ["book_site_preview.mjs", "book_preview_frame.mjs", "demo/book_preview_controls.mjs"];
test("all preview modules and direct imports are declared in the shipped package", () => {
  for (const path of modules) {
    assert(manifest.files.includes(path), path);
    for (const match of read(`wasm/${path}`).matchAll(/from\s+["']([^"']+)["']/g)) {
      assert(manifest.files.includes(posix.normalize(posix.join(posix.dirname(path), match[1]))), match[1]);
    }
  }
  assert(manifest.files.includes("PREVIEW.md"));
});
test("both assemblers include the preview worker dependencies, controller and guide", () => {
  for (const path of ["scripts/check-wasm-package.sh", "scripts/dsr-wasm-package.sh"]) {
    const loops = [...read(path).matchAll(/for file in ([\s\S]*?); do\s+cp "wasm\/([^\n]+)"/g)];
    const root = loops.find(match => match[2].startsWith("$file")), demo = loops.find(match => match[2].startsWith("demo/"));
    for (const name of ["book_site_preview.mjs", "book_preview_frame.mjs", "PREVIEW.md"]) assert(root?.[1].split(/\s+/).includes(name), `${path}: ${name}`);
    assert(demo?.[1].split(/\s+/).includes("book_preview_controls.mjs"), path);
  }
});
test("actual worker entry delegates preview to the same book engine, not an alternate renderer", () => {
  const entry = read("wasm/book_worker_entry.js");
  assert(entry.includes('import * as book from "./book.js"')); assert(entry.includes('from "./book_site_preview.mjs"'));
  assert(entry.includes("renderBookPreview(book, files, options)"));
  assert(read("wasm/book-worker.d.ts").includes('format: "preview"'));
});
test("actual publisher starts preview independently and tears it down before its source owner", () => {
  const entry = read("wasm/demo/book.js"), html = read("wasm/demo/book.html");
  assert(entry.includes('from "./book_preview_controls.mjs"')); assert(entry.includes("worker: createBookWorker({ maxOutputBytes:"));
  assert(entry.includes("preview?.dispose(); library?.dispose(); controls.dispose();"));
  assert(entry.includes("preview?.suspend();")); assert(entry.includes("preview?.resume();"));
  const iframe = html.match(/<iframe\b[^>]*id="preview-frame"[^>]*>/)?.[0]; assert(iframe); assert(iframe.includes('sandbox="allow-scripts"')); assert(!iframe.includes("allow-same-origin"));
  for (const id of ["preview-build", "preview-auto", "preview-cancel", "preview-pages", "preview-next", "preview-previous", "preview-status"]) {
    assert.equal([...html.matchAll(new RegExp(`id="${id}"`, "g"))].length, 1, id);
  }
});
