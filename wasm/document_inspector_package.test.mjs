// Package wiring checks are not generated-WASM or native-rendering proof.
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
const read = file => readFile(new URL(file, import.meta.url), "utf8");

test("public inspection subpath is lazy without neighboring generated bindings", async () => {
  const manifest = JSON.parse(await read("./package.json"));
  const entry = manifest.exports["./document-inspector"];
  assert.equal(entry.types, "./document-inspector.d.ts");
  const api = await import(entry.import);
  const inspector = api.createDocumentInspector({ workerFactory() { assert.fail("unexpected start"); } });
  assert.equal(inspector.pendingOperations, 0);
  inspector.dispose();
});

test("manifest and both assemblers ship the complete inspection entry and worker", async () => {
  const manifest = JSON.parse(await read("./package.json"));
  assert.equal(manifest.exports["./document-inspector/worker"], "./document_inspector_worker.js");
  assert.ok(manifest.sideEffects.includes("./document_inspector_worker.js"));
  for (const name of ["document-inspector.js", "document-inspector.d.ts", "document_inspector.mjs",
    "document_inspector_worker.js", "DOCUMENT_INSPECTION.md"]) {
    assert.ok(manifest.files.includes(name), name);
    assert.ok((await read(`./${name}`)).length > 0, name);
    for (const script of ["../scripts/check-wasm-package.sh", "../scripts/dsr-wasm-package.sh"])
      assert.ok((await read(script)).includes(name), `${script}: ${name}`);
  }
  for (const script of ["../scripts/check-wasm-package.sh", "../scripts/dsr-wasm-package.sh"]) {
    const code = await read(script);
    assert.match(code, /node --test wasm\/document_inspector\*\.test\.mjs/);
    assert.match(code, /node wasm\/document_inspector_smoke\.mjs/);
  }
});

test("worker fixes its loader in code and package entry does not import WASM", async () => {
  assert.match(await read("./document_inspector_worker.js"), /installDocumentInspector\(self, \(\) => import\("\.\/franken_markdown\.js"\)\)/);
  assert.doesNotMatch(await read("./document-inspector.js"), /import\s+.*franken_markdown/);
});

test("Document Lab ships with the demo, preserves existing rendering, and states audit scope", async () => {
  const manifest = JSON.parse(await read("./package.json"));
  assert.ok(manifest.files.includes("demo/document_lab.mjs"));
  assert.ok(manifest.sideEffects.includes("./demo/document_lab.mjs"));
  const html = await read("./demo/index.html");
  for (const id of ["document-lab", "open-document-lab", "inspect-document", "download-inspection",
    "inspection-status", "inspection-results", "inspection-metrics", "inspection-structure",
    "inspection-outline", "inspection-quality", "inspection-accessibility", "markdown", "render", "download-pdf", "download-html"])
    assert.equal(html.split(`id="${id}"`).length - 1, 1, id);
  assert.match(html, /src="\.\/document_lab\.mjs"/);
  assert.match(html, /src="\.\/demo\.js"/);
  assert.match(html, /id="preview"[^>]+sandbox=""/);
  assert.match(html, /No findings is not PDF\/UA certification/);
  for (const script of ["../scripts/check-wasm-package.sh", "../scripts/dsr-wasm-package.sh"])
    assert.match(await read(script), /cp wasm\/demo\/document_lab\.mjs/);
});
