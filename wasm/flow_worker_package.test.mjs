// Package/entrypoint checks; real rendering is a separate generated-WASM gate.
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const root = new URL("./", import.meta.url);
const manifest = JSON.parse(await readFile(new URL("package.json", root), "utf8"));

test("package subpath resolves without loading generated WASM on the calling thread", async () => {
  const entry = await import("@franken-suite/franken-markdown/flow-worker");
  assert.equal(typeof entry.createWorkerFlowSession, "function");
  assert.equal(typeof entry.FlowWorkerError, "function");
  await assert.rejects(entry.createWorkerFlowSession("source"), error => error.code === "WORKER_UNAVAILABLE");
});

test("worker runtime files and type entry are explicitly shipped", async () => {
  const required = ["flow-worker.js", "flow-worker.d.ts", "flow_worker.js", "flow_worker_session.mjs",
    "flow_worker_protocol.mjs", "worker_transport.mjs", "WORKER.md"];
  assert.deepEqual(manifest.exports["./flow-worker"], { types: "./flow-worker.d.ts", import: "./flow-worker.js" });
  assert.equal(manifest.exports["./flow-worker/worker"], "./flow_worker.js");
  assert(manifest.sideEffects.includes("./flow_worker.js"), "bundlers must retain the worker's message handler");
  for (const file of required) {
    assert(manifest.files.includes(file), `${file}: npm manifest`);
    assert((await readFile(new URL(file, root))).length > 0, `${file}: source exists`);
  }
});

test("both assembly routes include all worker runtime files and the real-WASM proof", async () => {
  for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = await readFile(new URL(`../scripts/${name}`, root), "utf8");
    for (const file of ["flow-worker.js", "flow-worker.d.ts", "flow_worker.js", "flow_worker_session.mjs",
      "flow_worker_protocol.mjs", "worker_transport.mjs", "WORKER.md"]) {
      assert(script.includes(file), `${name}: ${file}`);
    }
    assert(script.includes("node wasm/flow_worker_smoke.mjs"), `${name}: generated-WASM worker smoke`);
  }
});

test("failed custom worker construction rejects without falling back to main-thread rendering", async () => {
  const { createWorkerFlowSession } = await import("@franken-suite/franken-markdown/flow-worker");
  let called = 0;
  await assert.rejects(createWorkerFlowSession("source", {}, { workerFactory() { called++; throw new Error("policy blocked worker"); } }),
    error => error.message === "policy blocked worker");
  assert.equal(called, 1);
});
