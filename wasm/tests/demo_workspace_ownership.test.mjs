import assert from "node:assert/strict";
import test from "node:test";
import { createDemoWorkspaceWorker } from "../demo/demo_workspace_worker.mjs";
import { WORKER_PROTOCOL } from "../worker_transport.mjs";

// Explicit recording endpoint: these tests exercise production client and
// transport code, without running a worker, exporter, or generated WASM.
function endpoint(onListen = () => {}) {
  const listeners = new Map();
  return {
    sent: [], terminated: 0,
    addEventListener(kind, listener) { listeners.set(kind, listener); onListen(kind); },
    removeEventListener(kind) { listeners.delete(kind); },
    terminate() { this.terminated++; },
    postMessage(message, transfer) {
      this.sent.push({ message, transfer });
      queueMicrotask(() => listeners.get("message")?.({ data: {
        protocol: WORKER_PROTOCOL, id: message.id, ok: false, fatal: false,
        error: { code: "FIXTURE_STOP", message: "No workspace exporter is installed." },
      } }));
    },
  };
}
function imageInput() {
  const buffer = new Uint8Array([90, 1, 2, 3, 91]).buffer;
  return { buffer, options: { pdfImages: [{ destination: "selected.png", bytes: new DataView(buffer, 1, 3) }] } };
}
function ownsSelectedImage(worker, index = 0, expected = [1, 2, 3]) {
  const { message, transfer } = worker.sent[index];
  assert.equal(message.method, "prepare-workspace");
  const images = message.args[1].pdfImages;
  assert.equal(images.length, 1);
  assert.equal(images[0].destination, "selected.png");
  assert.deepEqual(Array.from(images[0].bytes), expected);
  assert.equal(images[0].bytes.byteOffset, 0);
  assert.equal(images[0].bytes.buffer.byteLength, 3, "do not capture neighboring bytes");
  assert.deepEqual(transfer, [images[0].bytes.buffer]);
}
for (const stage of ["factory", "message", "error", "messageerror"]) {
  for (const change of ["mutation", "detachment"]) {
    test("workspace owns exact image range before " + stage + " " + change, async t => {
      const selected = imageInput();
      const alter = () => {
        if (change === "mutation") new Uint8Array(selected.buffer).fill(9);
        else structuredClone(selected.buffer, { transfer: [selected.buffer] });
      };
      const worker = endpoint(kind => { if (kind === stage) alter(); });
      const client = createDemoWorkspaceWorker({ timeoutMs: 0, workerFactory() {
        if (stage === "factory") alter();
        return worker;
      } });
      t.after(() => client.dispose());
      await assert.rejects(client.prepare("![image](selected.png)", selected.options), { code: "FIXTURE_STOP" });
      ownsSelectedImage(worker);
      assert.notEqual(worker.sent[0].message.args[1].pdfImages[0].bytes.buffer, selected.buffer);
      assert.equal(selected.buffer.byteLength, change === "detachment" ? 0 : 5);
      assert.equal(client.disposed, false);
    });
  }
}

test("workspace startup failure releases its snapshot and retries current image bytes", async t => {
  const selected = imageInput();
  const worker = endpoint();
  let starts = 0;
  const client = createDemoWorkspaceWorker({ timeoutMs: 0, workerFactory() {
    starts++;
    new Uint8Array(selected.buffer).fill(9);
    if (starts === 1) throw Object.assign(new Error("Startup failed."), { code: "FIXTURE_STARTUP" });
    return worker;
  } });
  t.after(() => client.dispose());
  await assert.rejects(client.prepare("source", selected.options), { code: "FIXTURE_STARTUP" });
  assert.equal(client.disposed, false);
  new Uint8Array(selected.buffer).set([90, 4, 5, 6, 91]);
  await assert.rejects(client.prepare("source", selected.options), { code: "FIXTURE_STOP" });
  ownsSelectedImage(worker, 0, [4, 5, 6]);
  assert.equal(starts, 2);
});

test("workspace validation and pre-abort preserve lazy startup", async t => {
  let starts = 0;
  const client = createDemoWorkspaceWorker({ timeoutMs: 0, workerFactory() { starts++; return endpoint(); } });
  t.after(() => client.dispose());
  await assert.rejects(client.prepare("source", { allowRawHtml: true }), { code: "UNSUPPORTED_WORKSPACE_SETTINGS" });
  await assert.rejects(client.prepare("source", { pdfImages: [
    { destination: "same.png", bytes: new Uint8Array([1]) },
    { destination: "same.png", bytes: new Uint8Array([2]) },
  ] }), { code: "INVALID_OPTIONS" });
  await assert.rejects(client.prepare("source", imageInput().options, { signal: { aborted: true } }), { code: "ABORTED" });
  assert.equal(starts, 0);
});

test("workspace warm admission keeps independent snapshots without restarting", async t => {
  const worker = endpoint();
  let starts = 0;
  const client = createDemoWorkspaceWorker({ timeoutMs: 0, workerFactory() { starts++; return worker; } });
  t.after(() => client.dispose());
  await assert.rejects(client.prepare("source", imageInput().options), { code: "FIXTURE_STOP" });
  const selected = imageInput();
  const request = client.prepare("next source", selected.options);
  new Uint8Array(selected.buffer).fill(9);
  await assert.rejects(request, { code: "FIXTURE_STOP" });
  ownsSelectedImage(worker, 1);
  assert.notEqual(worker.sent[0].message.args[1].pdfImages[0].bytes.buffer,
    worker.sent[1].message.args[1].pdfImages[0].bytes.buffer);
  assert.equal(starts, 1);
});
