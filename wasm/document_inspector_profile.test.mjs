// Production inspector and transport with explicit in-process endpoints and
// synthetic native reports. Real worker preemption has separate worker tests.
import assert from "node:assert/strict";
import test from "node:test";
import { createDocumentInspector, installDocumentInspector } from "./document_inspector.mjs";
import { OwnedWorkerRpc } from "./worker_transport.mjs";
import { statsFixture, auditFixture } from "./tests/document_inspector_fixture.mjs";

const source = "\ufeffTerm\r\n: **Definition β**\r\n";
const gate = () => {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
};
function endpoints(transformReply) {
  let closed = false;
  const make = () => {
    const listeners = new Map();
    return {
      sent: [], received: [], stops: 0,
      addEventListener(type, listener) { if (!listeners.has(type)) listeners.set(type, new Set()); listeners.get(type).add(listener); },
      removeEventListener(type, listener) { listeners.get(type)?.delete(listener); },
      receive(data) { if (!closed) { this.received.push(data); for (const fn of listeners.get("message") ?? []) fn({ data }); } },
      terminate() { this.stops++; closed = true; },
    };
  };
  const client = make(), server = make();
  client.postMessage = (message, transfer = []) => {
    client.sent.push(message);
    const captured = structuredClone(message, { transfer });
    queueMicrotask(() => server.receive(captured));
  };
  server.postMessage = (message, transfer = []) => {
    const changed = transformReply ? transformReply(message) : message;
    const captured = structuredClone(changed, { transfer });
    queueMicrotask(() => client.receive(captured));
  };
  return { client, server };
}
function fixture(t, settings = {}, inspectorOptions = {}) {
  const pair = endpoints(settings.transformReply), calls = [];
  let loads = 0, starts = 0;
  const engine = {
    async documentStats(...args) {
      calls.push({ stage: "stats", args });
      settings.entered?.resolve();
      if (settings.hold) await settings.hold.promise;
      const report = statsFixture(args[0]);
      if (args[1]?.profile !== undefined && settings.omit !== "stats") report.profile = args[1].profile;
      Object.assign(report, settings.stats);
      return report;
    },
    async accessibilityAudit(...args) {
      calls.push({ stage: "audit", args });
      const report = auditFixture();
      if (args[1]?.profile !== undefined && settings.omit !== "audit") report.profile = args[1].profile;
      Object.assign(report, settings.audit);
      return report;
    },
  };
  const stop = installDocumentInspector(pair.server, async () => {
    loads++; settings.loadEntered?.resolve();
    if (settings.loadHold) await settings.loadHold.promise;
    return engine;
  });
  const api = createDocumentInspector({ timeoutMs: 0, ...inspectorOptions,
    workerFactory() { starts++; return pair.client; } });
  t.after(() => { api.dispose(); stop(); });
  return { api, pair, calls, get loads() { return loads; }, get starts() { return starts; } };
}

test("explicit grammar reaches both complete native reports and their portable JSON", async t => {
  for (const profile of ["commonmark-gfm", "gfm-plus"]) {
    const f = fixture(t), report = await f.api.inspect(source, { profile });
    assert.deepEqual(f.calls.map(call => call.args), [[source, { profile }], [source, { profile }]]);
    assert.ok(f.calls.every(call => Object.isFrozen(call.args[1])));
    assert.equal(report.profile, profile);
    assert.equal(report.stats.profile, profile); assert.equal(report.accessibility.profile, profile);
    assert.equal(report.sourceLength, new TextEncoder().encode(source).length);
    assert.equal(JSON.parse(report.text()).profile, profile);
    assert.ok(Object.isFrozen(report.stats.outline)); assert.ok(Object.isFrozen(report.accessibility.findings));
    assert.equal(f.pair.client.sent[0].args.length, 3);
    assert.equal(f.loads, 1);
  }
});

test("worker dispatch owns source and profile before asynchronous renderer loading", async t => {
  const loadHold = gate(), loadEntered = gate(), f = fixture(t, { loadHold, loadEntered });
  const pending = f.api.inspect("alpha", { profile: "gfm-plus" });
  await loadEntered.promise;
  const received = f.pair.server.received[0];
  received.args[0] = "bravo"; // Same byte length defeats a length-only check.
  received.args[2].profile = "commonmark-gfm";
  loadHold.resolve();
  const report = await pending;
  assert.equal(report.sourceLength, 5); assert.equal(report.profile, "gfm-plus");
  assert.deepEqual(f.calls.map(call => call.args), [
    ["alpha", { profile: "gfm-plus" }], ["alpha", { profile: "gfm-plus" }],
  ]);
});

test("omitted grammar preserves legacy request shape, native calls and report fields", async t => {
  const f = fixture(t), first = await f.api.inspect(source);
  const repeated = await f.api.inspect(source, { profile: undefined });
  assert.equal(first.text(), repeated.text());
  assert.equal(Object.hasOwn(first, "profile"), false);
  assert.equal(Object.hasOwn(first.stats, "profile"), false);
  assert.equal(Object.hasOwn(first.accessibility, "profile"), false);
  assert.ok(f.pair.client.sent.every(message => message.args.length === 2));
  assert.ok(f.calls.every(call => call.args.length === 1));
});

test("profile admission rejects aliases, accessors and resource options before starting", async t => {
  const f = fixture(t);
  for (const controls of [{ profile: "gfm" }, { profile: null }, { profile: true },
    { get profile() { assert.fail("profile getter"); } },
    { profile: "gfm-plus", get images() { assert.fail("image getter"); } },
    { profile: "gfm-plus", fontAssets: [] }, { allowRawHtml: true }])
    await assert.rejects(f.api.inspect(source, controls), { code: "INVALID_OPTIONS" });
  assert.equal(f.starts, 0); assert.equal(f.loads, 0); assert.equal(f.calls.length, 0);
});

test("queued analysis owns its dialect and charges it before retaining the request", async t => {
  const hold = gate(), entered = gate(), f = fixture(t, { hold, entered });
  const first = f.api.inspect("first");
  await entered.promise;
  const before = f.api.pendingBytes, controls = { profile: "commonmark-gfm" };
  const second = f.api.inspect(source, controls);
  assert.equal(f.api.pendingBytes - before, 512 + 2 * source.length + 64 + 2 * controls.profile.length);
  controls.profile = "gfm-plus";
  hold.resolve();
  await first;
  const report = await second;
  assert.equal(report.profile, "commonmark-gfm");
  assert.deepEqual(f.calls.slice(2).map(call => call.args), [
    [source, { profile: "commonmark-gfm" }], [source, { profile: "commonmark-gfm" }],
  ]);
  assert.equal(f.api.pendingBytes, 0);
  const bounded = fixture(t, {}, { maxPendingBytes: 512 + 2 * source.length });
  await assert.rejects(bounded.api.inspect(source, { profile: "gfm-plus" }), { code: "WORKER_QUEUE_FULL" });
  assert.equal(bounded.starts, 0);
  await bounded.api.inspect(source);
});

test("old or contradictory native analysis receipts fail without partial reports or replay", async t => {
  for (const settings of [{ omit: "stats" }, { omit: "audit" },
    { stats: { profile: "commonmark-gfm" } }, { audit: { profile: "commonmark-gfm" } }]) {
    const f = fixture(t, settings);
    const first = assert.rejects(f.api.inspect(source, { profile: "gfm-plus" }), { code: "UNSUPPORTED_WASM_PACKAGE" });
    const queued = assert.rejects(f.api.inspect("never"), { code: "SESSION_LOST" });
    await Promise.all([first, queued]);
    assert.equal(f.calls.length, settings.omit === "audit" || settings.audit ? 2 : 1);
    assert.equal(f.api.disposed, true); assert.equal(f.api.pendingBytes, 0);
    assert.equal(f.pair.client.sent.length, 1);
  }
});

test("profile receipts count toward the exact combined UTF-8 output limit", async t => {
  const complete = await fixture(t).api.inspect(source, { profile: "commonmark-gfm" });
  const exact = fixture(t, {}, { maxOutputBytes: complete.bytes.length });
  assert.equal((await exact.api.inspect(source, { profile: "commonmark-gfm" })).text(), complete.text());
  const below = fixture(t, {}, { maxOutputBytes: complete.bytes.length - 1 });
  await assert.rejects(below.api.inspect(source, { profile: "commonmark-gfm" }), { code: "BUDGET_EXCEEDED" });
  assert.equal(below.api.disposed, true);
});

test("raw profile-bearing RPC validates options before loading the renderer", async t => {
  const f = fixture(t), rpc = new OwnedWorkerRpc(f.pair.client, { timeoutMs: 0 });
  t.after(() => rpc.dispose());
  for (const options of [{ profile: "gfm" }, { images: [] }, null, []])
    await assert.rejects(rpc.request("inspect", 512, () => ({ args: [source, 8000, options] })), { code: "INVALID_OPTIONS" });
  assert.equal(f.loads, 0);
  const output = await rpc.request("inspect", 512, () => ({ args: [source, 8000, { profile: "gfm-plus" }] }));
  assert.equal(output.profile, "gfm-plus");
  assert.equal(JSON.parse(new TextDecoder().decode(output.bytes)).profile, "gfm-plus");
  assert.equal(f.loads, 1);
});

test("a forged response cannot substitute a different dialect or release queued work", async t => {
  const cases = [
    { expected: "UNSUPPORTED_WASM_PACKAGE", edit(report, value) {
      delete report.profile; delete report.stats.profile; delete report.accessibility.profile; delete value.profile;
    } },
    { expected: "UNSUPPORTED_WASM_PACKAGE", edit(report, value) {
      report.profile = report.stats.profile = report.accessibility.profile = value.profile = "commonmark-gfm";
    } },
    { expected: "WORKER_PROTOCOL_ERROR", edit(report) { report.stats.profile = "commonmark-gfm"; } },
    { expected: "WORKER_PROTOCOL_ERROR", edit(report, value) { value.profile = "commonmark-gfm"; } },
  ];
  for (const entry of cases) {
    const f = fixture(t, { transformReply(message) {
      if (!message.ok) return message;
      const value = message.value, report = JSON.parse(new TextDecoder().decode(value.bytes));
      entry.edit(report, value);
      // Keep the transfer's original storage attached; deliver the newly owned
      // malformed bytes to exercise the client's independent admission.
      value.bytes = new TextEncoder().encode(JSON.stringify(report));
      return message;
    } });
    const first = assert.rejects(f.api.inspect(source, { profile: "gfm-plus" }), { code: entry.expected });
    const queued = assert.rejects(f.api.inspect("never"));
    await Promise.all([first, queued]);
    assert.equal(f.api.disposed, true); assert.equal(f.pair.client.sent.length, 1);
    assert.equal(f.api.pendingBytes, 0);
  }
});

test("profiled in-flight cancellation closes ownership and a new inspector can restart", async t => {
  const hold = gate(), entered = gate(), f = fixture(t, { hold, entered });
  const controller = new AbortController();
  const result = assert.rejects(f.api.inspect(source, { profile: "gfm-plus", signal: controller.signal }), { code: "ABORTED" });
  await entered.promise;
  controller.abort(); await result;
  assert.equal(f.api.disposed, true); assert.equal(f.pair.client.stops, 1);
  hold.resolve();
  const restarted = await fixture(t).api.inspect(source, { profile: "commonmark-gfm" });
  assert.equal(restarted.profile, "commonmark-gfm");
});
