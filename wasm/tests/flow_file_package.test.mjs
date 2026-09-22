import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (path) => readFileSync(resolve(root, path), "utf8");
const manifest = JSON.parse(read("wasm/package.json"));
const runtime = ["demo/flow_file_session.mjs", "demo/flow_file_controls.mjs", "FILES.md"];

test("file saving and guide ship exactly once without changing package exports", () => {
  for (const path of runtime) {
    assert.equal(manifest.files.filter((item) => item === path).length, 1, path);
    assert.ok(read(`wasm/${path}`).length > 0, path);
  }
  assert.ok(manifest.sideEffects.includes("./demo/flow-source.js"));
  for (const key of [".", "./book", "./flow", "./book-worker"])
    assert.ok(manifest.exports[key], key);
});

test("both real package assemblers copy every new runtime and guide", () => {
  for (const path of ["scripts/check-wasm-package.sh", "scripts/dsr-wasm-package.sh"]) {
    const script = read(path);
    const loops = [...script.matchAll(/for file in ([\s\S]*?); do\n\s+cp "wasm\/(.*?)\$file"/g)];
    const copied = loops.flatMap(([, names, prefix]) =>
      names
        .replace(/\\\n/g, " ")
        .trim()
        .split(/\s+/)
        .map((name) => prefix + name),
    );
    for (const file of runtime) assert.ok(copied.includes(file), `${path}: ${file}`);
  }
});

test("source entrypoint mounts saving before draft initialization and drops file authority on suspension", () => {
  const source = read("wasm/demo/flow-source.js");
  assert.match(source, /import \{ createFileControls \} from "\.\/flow_file_controls\.mjs"/);
  assert.ok(
    source.indexOf("files = createFileControls") < source.indexOf("drafts = createDraftControls"),
  );
  const hide = source.slice(source.indexOf('window.addEventListener("pagehide"'));
  assert.ok(hide.indexOf("files?.dispose()") < hide.indexOf("controls?.dispose()"));
  assert.match(hide, /files = null/);
  assert.match(hide, /retained = \{ document: controls\.snapshot\(\), view: source\.value \}/);
  assert.doesNotMatch(hide, /retained = [^\n]*(handle|files:|session:)/);
  assert.match(
    source,
    /onReplace: \(\) => source\.dispatchEvent\(new Event\("fmd-document-replaced"\)\)/,
  );
});

test("all source-entrypoint static dependencies are real shipped files, without renderer imports", () => {
  const visited = new Set();
  function visit(path) {
    if (visited.has(path)) return;
    visited.add(path);
    const source = read(path),
      packagePath = relative(resolve(root, "wasm"), resolve(root, path));
    assert.ok(manifest.files.includes(packagePath), packagePath);
    for (const [, specifier] of source.matchAll(/^import\s[^;]*?from\s["']([^"']+)["']/gm)) {
      assert.ok(specifier.startsWith("."), specifier);
      const next = relative(root, resolve(root, dirname(path), specifier));
      assert.ok(next.startsWith("wasm/"), next);
      assert.doesNotMatch(next, /\/pkg\/|flow-canvas\.js|franken_markdown\.js/);
      visit(next);
    }
  }
  visit("wasm/demo/flow-source.js");
  for (const path of [
    "flow_file_session.mjs",
    "flow_file_controls.mjs",
    "flow_draft_store.mjs",
    "flow_document.mjs",
  ]) {
    assert.ok(visited.has(`wasm/demo/${path}`));
  }
});

test("source page no longer promises that explicit saves cannot overwrite files", () => {
  const html = read("wasm/demo/flow-canvas.html");
  assert.doesNotMatch(html, /Files are never uploaded or overwritten/);
  assert.match(html, /optional direct file controls below save only after an explicit action/);
  assert.match(html, /src="\.\/flow-source\.js"/);
  assert.match(html, /src="\.\/flow-canvas\.js"/);
});

test("guide distinguishes conflict detection from OS-wide atomicity and permission revocation", () => {
  const guide = read("wasm/FILES.md");
  assert.match(guide, /not an operating-system-wide compare-and-swap/);
  assert.match(guide, /does not\nrevoke any site permission/);
  assert.match(guide, /not continuous monitoring/);
  assert.match(guide, /no passing native-browser evidence/);
});
