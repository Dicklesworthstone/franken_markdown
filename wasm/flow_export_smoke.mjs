// Generated-WASM integration, not a native double. Both package assemblers run
// this after building the matching artifact. No source-text tests replace it.
// Usage: node wasm/flow_export_smoke.mjs <assembled-package-dir> <generated.wasm>
import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";
import { readFile } from "node:fs/promises";
import { deflateSync } from "node:zlib";
import { pathToFileURL } from "node:url";
import path from "node:path";
const [packageArgument, wasmArgument] = process.argv.slice(2);
if (!packageArgument || !wasmArgument) throw new Error("expected assembled package and generated WASM paths");
const packageDir = path.resolve(packageArgument), wasmPath = path.resolve(wasmArgument);
const entry = name => pathToFileURL(path.join(packageDir, name)).href;
const { init, renderHtml, renderPdf } = await import(entry("franken_markdown.js"));
const { createFlowSession } = await import(entry("flow.js"));
const { createWorkerFlowSession } = await import(entry("flow-worker.js"));
await init(new Uint8Array(await readFile(wasmPath)));
function workerFactory() {
  const worker = new Worker(new URL("./tests/flow_worker_wasm_node.mjs", import.meta.url), { workerData: { packageDir, wasmPath } });
  const listeners = new Map(), errors = new Set();
  worker.on("error", () => {});
  worker.on("exit", () => { for (const listener of errors) listener(new Event("error")); });
  return {
    postMessage: (message, transfer) => worker.postMessage(message, transfer),
    terminate: () => worker.terminate(),
    addEventListener(kind, listener) {
      const wrapped = kind === "message" ? data => listener({ data }) : error => listener({ error });
      listeners.set(listener, wrapped); worker.on(kind, wrapped); if (kind === "error") errors.add(listener);
    },
    removeEventListener(kind, listener) { worker.off(kind, listeners.get(listener)); listeners.delete(listener); errors.delete(listener); }
  };
}
// Generate a tiny, valid truecolor PNG without depending on a repository image.
function chunk(name, data) {
  const kind = Buffer.from(name), all = Buffer.concat([kind, data]);
  let crc = 0xffffffff;
  for (const byte of all) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  const size = Buffer.alloc(4), sum = Buffer.alloc(4);
  size.writeUInt32BE(data.length); sum.writeUInt32BE((crc ^ 0xffffffff) >>> 0);
  return Buffer.concat([size, all, sum]);
}
const header = Buffer.alloc(13); header.writeUInt32BE(2, 0); header.writeUInt32BE(1, 4); header[8] = 8; header[9] = 2;
const png = new Uint8Array(Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header),
  chunk("IDAT", deflateSync(Buffer.from([0, 255, 0, 0, 0, 0, 255]))), chunk("IEND", Buffer.alloc(0))]));
let source = "# Export Guide\n\nCafé with **bold**, *italic* and [a link](#export-guide).\n\n"
  + "| Name | Value |\n| --- | --- |\n| measured | original Markdown |\n\n![local](local.png)\n\n![again](local.png)\n";
const direct = await createFlowSession(source, { font: "serif", viewportWidth: 360 });
let remote;
const missing = error => error.code === "UNRESOLVED_EXPORT_ASSET";
async function resolveImages() {
  for (const session of [direct, remote]) {
    // Collect the inventory before delivery changes it.
    const requests = (await session.pendingAssets({ limit: 2048 })).requests;
    for (const request of requests) await session.provideAsset({ requestId: request.id,
      generation: request.generation, width: 2, height: 1, bytes: png });
  }
}
async function parity(label) {
  for (const format of ["html", "pdf"]) {
    const options = { title: "Export proof", lang: "en", toc: true, tocDepth: 3,
      ...(format === "pdf" ? { pageNumbers: true, metadataEpochSeconds: 0 } : { darkMode: "auto" }) };
    const a = await direct.exportDocument(format, options, direct.token);
    const b = await remote.exportDocument(format, options, remote.token);
    const oracle = await (format === "html" ? renderHtml : renderPdf)(source, {
      ...options, font: "serif", allowRawHtml: false, pdfImages: [{ destination: "local.png", bytes: png }]
    });
    assert.deepEqual(a.bytes, oracle.bytes, `${label}: ${format} direct export/core parity`);
    assert.deepEqual(b.bytes, oracle.bytes, `${label}: ${format} worker export/core parity`);
    assert.deepEqual(b.diagnostics, oracle.diagnostics); assert.equal(b.assetCount, 1);
    assert.equal(b.sourceLengthBytes, new TextEncoder().encode(source).length);
    if (format === "html") assert(new TextDecoder().decode(b.bytes).includes("data:image/png;base64,"));
    else assert.equal(new TextDecoder().decode(b.bytes.subarray(0, 5)), "%PDF-");
    console.log(`flow-export: ${label} ${format} native-core/direct/worker bytes and diagnostics: PASS`);
  }
}
try {
  remote = await createWorkerFlowSession(source, { font: "serif", viewportWidth: 360 }, { workerFactory, timeoutMs: 60000 });
  await assert.rejects(direct.exportDocument("pdf", {}, direct.token), missing);
  await assert.rejects(remote.exportDocument("html", {}, remote.token), missing);
  const old = remote.token;
  await resolveImages(); await parity("initial");
  await assert.rejects(remote.exportDocument("pdf", {}, old), error => error.code === "STALE_LAYOUT");
  source = source.replace("Café", "Edited café");
  direct.replaceSource(source, { expectedRevision: direct.revision });
  await remote.replaceSource(source, { expectedRevision: remote.revision });
  await assert.rejects(remote.exportDocument("pdf", {}, remote.token), missing);
  await resolveImages(); await parity("edited");
  assert(png.length > 0, "caller-owned image bytes were never detached");
} finally { direct.dispose(); remote?.dispose(); }
