// Generated-WASM proof: real worker entry and synchronous entry must produce
// identical geometry, glyphs, links and reading semantics after the same edits.
// Usage: node wasm/flow_worker_smoke.mjs <assembled-package-dir> <generated.wasm>
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { Worker } from "node:worker_threads";

const [packageArgument, wasmArgument] = process.argv.slice(2);
if (!packageArgument || !wasmArgument)
  throw new Error("expected assembled package and generated WASM paths");
const packageDir = path.resolve(packageArgument),
  wasmPath = path.resolve(wasmArgument);
const entry = (name) => pathToFileURL(path.join(packageDir, name)).href;
const { init, createFlowSession } = await import(entry("flow.js"));
const { createWorkerFlowSession } = await import(entry("flow-worker.js"));
await init(new Uint8Array(await readFile(wasmPath)));
function workerFactory() {
  const worker = new Worker(new URL("./tests/flow_worker_wasm_node.mjs", import.meta.url), {
    workerData: { packageDir, wasmPath },
  });
  const listeners = new Map();
  const errors = new Set();
  worker.on("error", () => {});
  worker.on("exit", () => {
    for (const listener of errors) listener(new Event("error"));
  });
  return {
    postMessage: (message, transfer) => worker.postMessage(message, transfer),
    terminate: () => worker.terminate(),
    addEventListener(kind, listener) {
      const wrapped =
        kind === "message" ? (data) => listener({ data }) : (error) => listener({ error });
      listeners.set(listener, wrapped);
      worker.on(kind, wrapped);
      if (kind === "error") errors.add(listener);
    },
    removeEventListener(kind, listener) {
      worker.off(kind, listeners.get(listener));
      listeners.delete(listener);
      errors.delete(listener);
    },
  };
}
const source =
  "# Worker Guide\n\nCafé with **bold**, *italic* and [link](#worker-guide).\n\n" +
  "| Name | Value |\n| --- | --- |\n| measured | wrapped cell text |\n\n" +
  "![one](one.png)\n\n![two](two.png)\n\n```rust\nlet x = 1;\n```\n";
const options = { viewportWidth: 360 };
const direct = await createFlowSession(source, options);
const remote = await createWorkerFlowSession(source, options, {
  workerFactory,
  timeoutMs: 60000,
  startupTimeoutMs: 60000,
});
async function equivalent(label) {
  const expected = [...direct.pages({ limit: 7, glyphs: true })].flatMap((page) => page.items);
  const actual = [];
  for await (const page of remote.pages({ limit: 3, glyphs: true })) actual.push(...page.items);
  assert.deepEqual(actual, expected, `${label}: every drawing item and owned glyph run`);
  assert.deepEqual(
    (await remote.snapshot()).totalBounds,
    direct.snapshot().totalBounds,
    `${label}: bounds`,
  );
  assert.deepEqual(
    (await remote.readingOrder({ limit: 2048 })).nodes,
    direct.readingOrder({ limit: 2048 }).nodes,
    `${label}: accessible reading semantics`,
  );
  assert.deepEqual(
    (await remote.pendingAssets()).requests,
    direct.pendingAssets().requests,
    `${label}: pending assets`,
  );
  console.log(`flow-worker: ${label}: PASS`);
  return actual;
}
try {
  const items = await equivalent("initial real-WASM worker layout");
  const text = items.find((item) => item.kind === "text" && item.text.length > 0);
  assert(text?.fontRun);
  assert.deepEqual(
    await remote.fontBytes(text.fontRun.fontId),
    direct.fontBytes(text.fontRun.fontId),
  );
  assert.deepEqual(
    await remote.selectText(text.index, 0, 1, remote.token),
    direct.selectText(text.index, 0, 1, direct.token),
  );
  const shown = remote.token;
  const requests = (await remote.pendingAssets()).requests;
  for (const request of requests.slice().reverse()) {
    const result = {
      requestId: request.id,
      generation: request.generation,
      width: 640,
      height: 240,
      bytes: new Uint8Array([1, 2, 3]),
    };
    await remote.provideAsset(result);
    direct.provideAsset(result);
    assert.equal(result.bytes.byteLength, 3, "caller buffer remains owned");
  }
  await equivalent("out-of-order assets");
  await assert.rejects(remote.hitTest(10, 10, shown), (error) => error.code === "STALE_LAYOUT");
  const resized = { viewportWidth: 240 };
  await remote.reflow(resized, remote.token);
  direct.reflow(resized, direct.token);
  await equivalent("viewport reflow");
  const revised = source.replace("Café", "Edited café");
  await remote.replaceSource(revised, { expectedRevision: remote.revision });
  direct.replaceSource(revised, { expectedRevision: direct.revision });
  assert.equal(await remote.getSource(), revised);
  await equivalent("transactional source replacement");
  const oldRevision = requests[0].generation;
  await assert.rejects(
    remote.provideAsset({
      requestId: requests[0].id,
      generation: oldRevision,
      width: 1,
      height: 1,
    }),
    (error) => error.code === "STALE_REVISION" || error.code === "STALE_ASSET_GENERATION",
  );
  const fresh = await createFlowSession(revised, resized);
  try {
    const freshItems = [...fresh.pages({ glyphs: true })].flatMap((page) => page.items);
    const remoteItems = [];
    for await (const page of remote.pages({ glyphs: true })) remoteItems.push(...page.items);
    assert.deepEqual(remoteItems, freshItems, "edited worker equals a fresh real-WASM render");
  } finally {
    fresh.dispose();
  }
  console.log(
    "flow-worker: generated WASM, worker/synchronous equivalence, edits, fonts, selection and assets: PASS",
  );
} finally {
  direct.dispose();
  remote.dispose();
}
await assert.rejects(remote.snapshot(), (error) => error.code === "SESSION_DISPOSED");
