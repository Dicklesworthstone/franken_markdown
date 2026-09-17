// Actual generated-WASM outline proof, separate from synthetic Canvas fixtures.
// Usage: node wasm/flow_outlines_smoke.mjs <assembled-package> <generated.wasm>
import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import path from "node:path";
const [packageArgument, wasmArgument] = process.argv.slice(2);
if (!packageArgument || !wasmArgument) throw new Error("expected assembled package and generated WASM paths");
const packageDir = path.resolve(packageArgument), wasmPath = path.resolve(wasmArgument);
const entry = name => pathToFileURL(path.join(packageDir, name)).href;
const { init, createFlowSession } = await import(entry("flow.js"));
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
      listeners.set(listener, wrapped); worker.on(kind, wrapped);
      if (kind === "error") errors.add(listener);
    },
    removeEventListener(kind, listener) { worker.off(kind, listeners.get(listener)); listeners.delete(listener); errors.delete(listener); }
  };
}
const source = "# Outlines\n\nOffice AV café **bold** *italic* ⇒\n\n```text\na\tb\n```";
const direct = await createFlowSession(source, {viewportWidth: 360});
let remote;
try {
  remote = await createWorkerFlowSession(source, {viewportWidth: 360}, {workerFactory, timeoutMs: 60000});
  const requests = new Map();
  for (const page of direct.pages({glyphs: true, limit: 256})) for (const item of page.items) {
    if (item.kind !== "text") continue;
    for (const glyph of item.fontRun.glyphs) {
      if (!requests.has(glyph.fontId)) requests.set(glyph.fontId, new Set());
      requests.get(glyph.fontId).add(glyph.glyphId);
    }
  }
  assert(requests.size >= 3, "styles identify multiple real faces");
  let seenInk = false, seenBlank = false;
  const before = direct.token;
  for (const [fontId, set] of requests) {
    const ids = [...set];
    const a = direct.glyphOutlines(fontId, ids);
    const b = await remote.glyphOutlines(fontId, ids);
    assert.deepEqual(a, b, "production worker and synchronous native outlines agree");
    assert(a.unitsPerEm > 0 && a.ascent > a.descent);
    assert.deepEqual(a.glyphs.map(g => g.glyphId), ids);
    for (const glyph of a.glyphs) {
      if (!glyph.commands.length) seenBlank = true;
      else {
        seenInk = true;
        assert.equal(glyph.commands[0][0], "M");
        assert.equal(glyph.commands.at(-1)[0], "Z");
        for (const command of glyph.commands) assert(command.slice(1).every(Number.isFinite));
      }
    }
    const metrics = await remote.glyphOutlines(fontId, []);
    assert.equal(metrics.unitsPerEm, a.unitsPerEm); assert.equal(metrics.glyphs.length, 0);
    if (ids.length) assert.deepEqual(direct.glyphOutlines(fontId, [ids[0], ids[0]]).glyphs, [a.glyphs[0], a.glyphs[0]]);
  }
  assert(seenInk && seenBlank, "real outlines include both ink and blank spaces");
  assert.deepEqual(direct.token, before, "outlining does not mutate document/layout identity");
  await assert.rejects(remote.glyphOutlines("0", [1]), error => error.code === "UNKNOWN_FONT");
  await assert.rejects(remote.glyphOutlines([...requests.keys()][0], new Uint16Array(257)), error => error.code === "BUDGET_EXCEEDED");
  assert.equal(await remote.getSource(), source, "a refused request leaves the worker usable");
  console.log("flow-outlines: actual glyph paths, blank glyphs, font metrics, batch order and worker parity: PASS");
} finally { remote?.dispose(); direct.dispose(); }
