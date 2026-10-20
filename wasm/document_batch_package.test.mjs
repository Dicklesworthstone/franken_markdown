import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const manifest = JSON.parse(await readFile(new URL("./package.json", import.meta.url), "utf8"));
const batchFiles = ["document-batch.js", "document-batch.d.ts", "document_batch.mjs", "DOCUMENT_BATCH.md"];

test("the public document-batch subpath resolves without importing WASM", () => {
  assert.deepEqual(manifest.exports["./document-batch"], {
    types: "./document-batch.d.ts", import: "./document-batch.js",
  });
  assert.equal(import.meta.resolve("@franken-suite/franken-markdown/document-batch"),
    new URL("./document-batch.js", import.meta.url).href);
});

test("the publish manifest retains the batch entry, implementation, types and guide", async () => {
  for (const name of batchFiles) {
    assert.ok(manifest.files.includes(name), `${name} must ship in npm packages`);
    assert.ok((await readFile(new URL(name, import.meta.url))).length > 0);
  }
  for (const dependency of ["document_worker.mjs", "worker_transport.mjs", "pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"]) {
    assert.ok(manifest.files.includes(dependency), `${dependency} must ship with the entry`);
  }
});

test("both package assemblers copy all batch files and run the scheduler tests", async () => {
  for (const script of ["dsr-wasm-package.sh", "check-wasm-package.sh"]) {
    const text = await readFile(new URL(`../scripts/${script}`, import.meta.url), "utf8");
    const copyLoop = text.match(/for file in ([\s\S]*?); do\n  cp "wasm\/\$file"/);
    assert.ok(copyLoop, `package copy loop missing in ${script}`);
    const copied = copyLoop[1].replaceAll("\\", "").split(/\s+/);
    for (const name of batchFiles) assert.ok(copied.includes(name), `${script} must copy ${name}`);
    assert.ok(text.includes("node --test wasm/document_batch*.test.mjs"), `${script} must run batch regressions`);
  }
});
