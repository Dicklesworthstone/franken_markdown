import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, mkdtempSync, mkdirSync, copyFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname, posix } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";

// Static helper closure, not a generated WASM package or renderer acceptance
// gate. The staging directories are retained for inspection, not cleaned up.
const wasm = fileURLToPath(new URL("../", import.meta.url));
const repo = dirname(wasm.replace(/[\\/]$/, ""));
const read = path => readFileSync(path, "utf8");
const manifest = JSON.parse(read(join(wasm, "package.json")));
const scripts = ["check-wasm-package.sh", "dsr-wasm-package.sh"];
function closure(entries) {
  const seen = new Set(), queue = [...entries];
  while (queue.length) {
    const path = queue.pop();
    if (seen.has(path)) continue;
    assert(!path.startsWith("../"), path);
    seen.add(path);
    const source = read(join(wasm, path));
    for (const match of source.matchAll(/^\s*import\s+(?:[^;]+?\s+from\s+)?["'](\.[^"']+)["']/gm)) {
      queue.push(posix.normalize(posix.join(posix.dirname(path), match[1])));
    }
  }
  return seen;
}
const required = closure(["demo/book_collection.mjs", "demo/book_controls.mjs"]);
function copied(script) {
  const paths = new Set();
  const loops = /for file in ([\s\S]*?); do\s+cp "wasm\/(demo\/)?\$file" "\$(?:PACKAGE|package_dir)\/(?:demo\/)?\$file"\s+done/g;
  for (const match of script.matchAll(loops)) {
    const prefix = match[2] ?? "";
    for (const name of match[1].replace(/\\\n/g, " ").trim().split(/\s+/)) paths.add(prefix + name);
  }
  return paths;
}

test("portable helper import closure and documentation are declared for publication", () => {
  assert(required.has("demo/book_font_assets.mjs"), "font ownership must be part of the closure");
  assert(required.has("pdf_page.mjs"), "the source facade's transitive dependency must be checked");
  for (const name of [...required, "PORTABLE_BOOK.md"])
    assert(manifest.files.includes(name), `manifest omits ${name}`);
  assert.equal(new Set(manifest.files).size, manifest.files.length);
});

for (const name of scripts) {
  test(`${name} stages every portable helper and loads the helper graph without WASM`, async () => {
    const script = read(join(repo, "scripts", name)), included = copied(script);
    for (const path of [...required, "PORTABLE_BOOK.md"])
      assert(included.has(path), `${name} omits ${path}`);
    assert.match(script, /node --test wasm\/tests\/book_portable\*\.test\.mjs/);
    const stage = mkdtempSync(join(tmpdir(), "fmd-book-portable-"));
    for (const path of [...required, "PORTABLE_BOOK.md"]) {
      mkdirSync(dirname(join(stage, path)), { recursive: true });
      copyFileSync(join(wasm, path), join(stage, path));
    }
    const { createBookCollection } = await import(pathToFileURL(join(stage, "demo/book_collection.mjs")));
    const { createBookControls } = await import(pathToFileURL(join(stage, "demo/book_controls.mjs")));
    assert.equal(typeof createBookControls, "function");
    const book = createBookCollection();
    const output = await book.portableDownload();
    assert.equal(output.filename, "book.fmdbook.bundle.json");
    assert.equal(JSON.parse(await output.blob.text()).images.length, 0);
    book.dispose();
  });
}
