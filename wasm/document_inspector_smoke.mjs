// Executes a supplied GENERATED Rust/WASM package. No renderer double or build
// fallback: missing artifacts/exports fail. This does not certify PDF/UA.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

async function engine(packageDir, wasmPath) {
  const api = await import(pathToFileURL(resolve(packageDir, "franken_markdown.js")));
  await api.init(new Uint8Array(await readFile(wasmPath)));
  return api;
}
if (!isMainThread) {
  const { installDocumentInspector } = await import(pathToFileURL(resolve(workerData.packageDir, "document_inspector.mjs")));
  const handlers = new Map();
  installDocumentInspector({
    addEventListener(type, fn) {
      const wrapped = data => fn({ data });
      handlers.set(fn, wrapped); parentPort.on(type, wrapped);
    },
    removeEventListener(type, fn) { parentPort.off(type, handlers.get(fn)); },
    postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
  }, () => engine(workerData.packageDir, workerData.wasmPath));
} else {
  if (!process.argv[2] || !process.argv[3]) throw new Error("Usage: document_inspector_smoke.mjs PACKAGE WASM");
  const packageDir = resolve(process.argv[2]), wasmPath = resolve(process.argv[3]);
  const { createDocumentInspector } = await import(pathToFileURL(resolve(packageDir, "document-inspector.js")));
  const direct = await engine(packageDir, wasmPath);
  class Endpoint extends EventTarget {
    constructor() {
      super();
      this.worker = new Worker(new URL(import.meta.url), { workerData: { packageDir, wasmPath } });
      this.worker.on("message", data => this.dispatchEvent(new MessageEvent("message", { data })));
      this.worker.on("error", () => this.dispatchEvent(new Event("error")));
      this.worker.on("messageerror", () => this.dispatchEvent(new Event("messageerror")));
    }
    postMessage(data, transfer) { this.worker.postMessage(data, transfer); }
    terminate() { return this.worker.terminate(); }
  }
  const inspector = createDocumentInspector({ workerFactory: () => new Endpoint(), timeoutMs: 30000 });
  try {
    let index = 0;
    for (const source of ["", "# Guide\n\nA short sentence.",
      "# Guide\n\n### Jump\n\n[click here](#missing)\n\n![](missing.png)\n\n[^missing]",
      "# Café 中𝄞\n\n| Task | Status |\n| --- | --- |\n| Work | Done |\n\n- [x] Ready\n\n```js\nconst value = 1;\n```",
    ]) {
      const report = await inspector.inspect(source);
      assert.deepEqual(report.stats, await direct.documentStats(source));
      assert.deepEqual(report.accessibility, await direct.accessibilityAudit(source));
      assert.equal(report.sourceLength, new TextEncoder().encode(source).length);
      assert.deepEqual(JSON.parse(report.text()), {
        schema: report.schema, sourceLength: report.sourceLength,
        stats: report.stats, accessibility: report.accessibility,
      });
      assert.deepEqual((await inspector.inspect(source)).bytes, report.bytes);
      console.log(`inspection-native: fixture=${index++} outcome=PASS`);
    }
    assert.equal(inspector.pendingBytes, 0);
  } finally { inspector.dispose(); }
}
