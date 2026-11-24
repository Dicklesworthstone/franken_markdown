import assert from "node:assert/strict";
import test from "node:test";
import { createWorkerRenderer } from "./document_worker.mjs";
import { createDocumentInspector } from "./document_inspector.mjs";
import { createDemoWorkspaceWorker } from "./demo/demo_workspace_worker.mjs";
import { WORKER_PROTOCOL } from "./worker_transport.mjs";

// These endpoints exercise real client/transport lifecycle code. They return
// only an explicit fixture error; no worker, native renderer or WASM is run.
function endpoint(onListen = () => {}) {
  const listeners = new Map();
  return {
    sent: [], terminated: 0, removed: [],
    addEventListener(kind, listener) { listeners.set(kind, listener); onListen(kind); },
    removeEventListener(kind) { this.removed.push(kind); listeners.delete(kind); },
    terminate() { this.terminated++; },
    postMessage(message) {
      this.sent.push(message);
      queueMicrotask(() => listeners.get("message")?.({ data: {
        protocol: WORKER_PROTOCOL, id: message.id, ok: false, fatal: false,
        error: { code: "FIXTURE_STOP", message: "No renderer is installed in this endpoint." },
      } }));
    },
  };
}
const surfaces = [
  { name: "document rendering", create: createWorkerRenderer, request: client => client.renderPdf("captured") },
  { name: "document inspection", create: createDocumentInspector, request: client => client.inspect("captured") },
  { name: "editable workspace", create: createDemoWorkspaceWorker, request: client => client.prepare("captured") },
];
for (const surface of surfaces) {
  for (const stage of ["factory", "message", "error", "messageerror"]) {
    test(surface.name + " retires disposal during " + stage + " startup without dispatch", async t => {
      let client, starts = 0;
      const worker = endpoint(kind => { if (kind === stage) client.dispose(); });
      client = surface.create({ timeoutMs: 0, workerFactory() {
        starts++;
        if (stage === "factory") client.dispose();
        return worker;
      } });
      t.after(() => client.dispose());
      await assert.rejects(surface.request(client), { code: "SESSION_DISPOSED" });
      assert.equal(client.disposed, true);
      assert.equal(worker.sent.length, 0, "retired source must never cross the worker boundary");
      assert.equal(worker.terminated, 1);
      if (stage !== "factory")
        assert.deepEqual(worker.removed, ["message", "error", "messageerror"]);
      if ("pendingBytes" in client) {
        assert.equal(client.pendingBytes, 0);
        assert.equal(client.pendingOperations, 0);
      }
      client.dispose();
      assert.equal(worker.terminated, 1);
      await assert.rejects(surface.request(client), { code: "SESSION_DISPOSED" });
      assert.equal(starts, 1, "disposed clients must not restart or replay work");
    });
  }

  test(surface.name + " preserves normal startup and reusable transactional refusals", async t => {
    let starts = 0;
    const worker = endpoint();
    const client = surface.create({ timeoutMs: 0, workerFactory() { starts++; return worker; } });
    t.after(() => client.dispose());
    await assert.rejects(surface.request(client), { code: "FIXTURE_STOP" });
    await assert.rejects(surface.request(client), { code: "FIXTURE_STOP" });
    assert.equal(client.disposed, false);
    assert.equal(worker.sent.length, 2);
    assert.equal(starts, 1);
    assert.equal(worker.terminated, 0);
    client.dispose();
    assert.equal(worker.terminated, 1);
  });
}

test("startup disposal fences every document render and analysis route", async t => {
  const requests = [
    client => client.renderHtml("captured"),
    client => client.renderPdf("captured"),
    client => client.renderSvg("captured"),
    client => client.renderEpub("captured"),
    client => client.renderInteractiveHtml("captured"),
    client => client.publish("captured", [{ format: "html" }, { format: "pdf" }]),
    client => client.verifyPdf("captured"),
    client => client.compare("before", "after"),
  ];
  for (const request of requests) {
    let client;
    const worker = endpoint(kind => { if (kind === "message") client.dispose(); });
    client = createWorkerRenderer({ timeoutMs: 0, workerFactory: () => worker });
    t.after(() => client.dispose());
    await assert.rejects(request(client), { code: "SESSION_DISPOSED" });
    assert.equal(worker.sent.length, 0);
    assert.equal(worker.terminated, 1);
    assert.equal(client.pendingBytes, 0);
  }
});
