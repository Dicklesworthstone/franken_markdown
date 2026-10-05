// Real JS facade, worker protocol, RPC queue and snapshot delta codec; only the
// native renderer and Worker endpoint are doubles. No font/raster/WASM proof.
import assert from "node:assert/strict";
import test from "node:test";
import { createFlowAdapter, FlowError } from "./flow_session.mjs";
import { normalizeFlowRequest } from "./flow_worker_protocol.mjs";
import { createWorkerFlowSessionWith, installFlowWorker } from "./flow_worker_session.mjs";

const turn = () => new Promise((resolve) => setImmediate(resolve));
const code = (expected) => (error) => error?.code === expected;

function native({ revision = "9007199254740993", layoutRevision = "9007199254740995",
  enabled = false, legacy = false } = {}) {
  const raw = {
    revision, layoutRevision, codeHighlighting: enabled,
    source: "```rust\nlet value = 42;\n```\n\n![image](a.png)",
    rejectHighlight: false, highlightCalls: 0, freed: 0, assets: [],
    setCodeHighlighting(expectedRevision, expectedLayout, value) {
      this.highlightCalls++;
      assert.equal(expectedRevision, this.revision);
      assert.equal(expectedLayout, this.layoutRevision);
      if (this.rejectHighlight) throw new FlowError("BUDGET_EXCEEDED", "lexer budget");
      if (this.codeHighlighting !== value) {
        this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
        this.codeHighlighting = value;
      }
    },
    reflow() { this.layoutRevision = String(BigInt(this.layoutRevision) + 1n); },
    replaceSource(expectedRevision, source) {
      if (expectedRevision !== this.revision) throw new FlowError("STALE_REVISION", "source changed");
      this.source = source;
      this.revision = String(BigInt(this.revision) + 1n);
      this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
    },
    provideAsset(id, generation, width, height, bytes) {
      assert.equal(generation, this.revision);
      this.assets.push({ id, bytes: bytes?.slice() });
      this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
    },
    snapshotJson(revision, layoutRevision, offset, limit) {
      const items = [{ kind: "text", text: "let", colorRole: this.codeHighlighting ? "tok-kw" : "code" }];
      return JSON.stringify({ schemaVersion: 1, revision, layoutRevision,
        offset, total: items.length, nextOffset: null, items: items.slice(offset, offset + limit) });
    },
    free() { this.freed++; },
  };
  if (legacy) delete raw.setCodeHighlighting;
  return raw;
}

function channel(raw, changeReply = () => {}) {
  const clientListeners = new Map(), serverListeners = new Map();
  const sent = [], replies = [], held = [];
  let stopped = false, stop;
  const listen = (map) => ({
    addEventListener(kind, listener) {
      if (!map.has(kind)) map.set(kind, new Set());
      map.get(kind).add(listener);
    },
    removeEventListener(kind, listener) { map.get(kind)?.delete(listener); },
  });
  const deliver = (map, data) => queueMicrotask(() => {
    if (!stopped) for (const listener of map.get("message") ?? []) listener({ data });
  });
  const client = {
    ...listen(clientListeners),
    postMessage(message, transfer = []) {
      const data = structuredClone(message, { transfer });
      sent.push(data);
      deliver(serverListeners, data);
    },
    terminate() { if (!stopped) { stopped = true; stop(); } },
  };
  const wire = { hold: false, sent, replies, client,
    flush() { wire.hold = false; for (const data of held.splice(0)) deliver(clientListeners, data); } };
  const server = {
    ...listen(serverListeners),
    postMessage(message, transfer = []) {
      const data = structuredClone(message, { transfer });
      changeReply(data, sent.find((request) => request.id === data.id));
      replies.push(data);
      if (wire.hold) held.push(data);
      else deliver(clientListeners, data);
    },
  };
  stop = installFlowWorker(server, () => createFlowAdapter(raw));
  return wire;
}
async function open(t, options, changeReply) {
  const raw = native(options), wire = channel(raw, changeReply);
  const session = await createWorkerFlowSessionWith(() => wire.client, raw.source, {}, { timeoutMs: 0 });
  t.after(() => session.dispose());
  return { raw, wire, session };
}

test("negotiates mode without changing the legacy state schema; publishes only after acknowledgment", async (t) => {
  const { raw, wire, session } = await open(t);
  assert.equal(session.supportsCodeHighlighting, true);
  assert.equal(session.codeHighlighting, false);
  assert.deepEqual(Object.keys(wire.replies[0].state).sort(), ["layout", "token"]);
  const before = session.token;
  wire.hold = true;
  const changed = session.setCodeHighlighting(true, before);
  await turn();
  assert.equal(raw.codeHighlighting, true);
  assert.equal(session.codeHighlighting, false);
  assert.deepEqual(session.token, before);
  wire.flush();
  assert.deepEqual(await changed, { revision: before.revision,
    layoutRevision: String(BigInt(before.layoutRevision) + 1n) });
  assert.equal(session.codeHighlighting, true);
  assert.deepEqual(Object.keys(await session.setCodeHighlighting(true, session.token)).sort(),
    ["layoutRevision", "revision"]);
  assert.equal(session.layoutRevision, String(BigInt(before.layoutRevision) + 1n));
});

test("mode survives source edits, reflow and asset completion; delta snapshots see changes", async (t) => {
  const { raw, session } = await open(t);
  assert.equal((await session.snapshot()).items[0].colorRole, "code");
  await session.setCodeHighlighting(true, session.token);
  assert.equal((await session.snapshot()).items[0].colorRole, "tok-kw");
  await session.reflow({ viewportWidth: 200 }, session.token);
  await session.provideAsset({ requestId: "1", generation: session.revision,
    width: 20, height: 10, bytes: new Uint8Array([7, 8]) });
  await session.replaceSource("```rust\nlet other = 1;\n```", { expectedRevision: session.revision });
  assert.equal(session.codeHighlighting, true);
  assert.equal((await session.snapshot()).items[0].colorRole, "tok-kw");
  assert.equal(raw.assets.length, 1);
  assert.deepEqual(raw.assets[0].bytes, new Uint8Array([7, 8]));
  await session.setCodeHighlighting(false, session.token);
  assert.equal((await session.snapshot()).items[0].colorRole, "code");
});

test("stale tokens and recoverable budget failures retain acknowledged mode and permit retry", async (t) => {
  const { raw, session } = await open(t);
  const old = session.token;
  await session.reflow({ viewportWidth: 100 }, old);
  await assert.rejects(session.setCodeHighlighting(true, old), code("STALE_LAYOUT"));
  assert.equal(raw.highlightCalls, 0);
  const before = session.token;
  raw.rejectHighlight = true;
  await assert.rejects(session.setCodeHighlighting(true, before), code("BUDGET_EXCEEDED"));
  assert.deepEqual(session.token, before);
  assert.equal(session.codeHighlighting, false);
  assert.equal(session.disposed, false);
  raw.rejectHighlight = false;
  await session.setCodeHighlighting(true, before);
  assert.equal(session.codeHighlighting, true);
});

test("validates booleans and lossless identities before enqueue and on the worker allowlist", async (t) => {
  const { wire, session } = await open(t);
  const count = wire.sent.length;
  for (const value of [0, 1, "true", null, {}, undefined]) {
    await assert.rejects(session.setCodeHighlighting(value, session.token), code("INVALID_ARGUMENT"));
    assert.throws(() => normalizeFlowRequest("setCodeHighlighting", [value, session.token]),
      code("INVALID_ARGUMENT"));
  }
  await assert.rejects(session.setCodeHighlighting(true, { revision: 1, layoutRevision: "1" }),
    code("INVALID_IDENTITY"));
  assert.equal(wire.sent.length, count);
  const token = session.token;
  await session.setCodeHighlighting(true, {
    revision: BigInt(token.revision), layoutRevision: BigInt(token.layoutRevision),
  });
  assert.equal(wire.sent.at(-1).args[1].revision, token.revision);
});

test("queued arguments are owned; queued cancellation does not toggle or consume capacity", async (t) => {
  const { raw, wire, session } = await open(t);
  wire.hold = true;
  const read = session.getSource();
  const expected = session.token;
  const cancelled = new AbortController();
  const dropped = session.setCodeHighlighting(true, expected, { signal: cancelled.signal });
  const rejected = assert.rejects(dropped, code("ABORTED"));
  cancelled.abort();
  await rejected;
  const change = session.setCodeHighlighting(true, expected);
  expected.revision = "0";
  expected.layoutRevision = "0";
  await turn();
  assert.equal(raw.highlightCalls, 0);
  assert.equal(session.codeHighlighting, false);
  wire.flush();
  await read;
  await change;
  assert.equal(raw.highlightCalls, 1);
  assert.equal(session.codeHighlighting, true);
  assert.equal(session.pendingBytes, 0);
  assert.equal(session.pendingOperations, 0);
});

test("in-flight cancellation loses the session and never replays queued mutations", async (t) => {
  const { raw, wire, session } = await open(t);
  wire.hold = true;
  const controller = new AbortController();
  const active = session.setCodeHighlighting(true, session.token, { signal: controller.signal });
  const queued = session.replaceSource("must not publish", { expectedRevision: session.revision });
  const activeCheck = assert.rejects(active, code("ABORTED"));
  const queuedCheck = assert.rejects(queued, code("SESSION_LOST"));
  await turn();
  assert.equal(raw.highlightCalls, 1);
  controller.abort();
  await Promise.all([activeCheck, queuedCheck]);
  assert.equal(session.disposed, true);
  assert.equal(raw.freed, 1);
  assert.notEqual(raw.source, "must not publish");
  assert.equal(session.pendingBytes, 0);
  assert.throws(() => session.codeHighlighting, code("SESSION_DISPOSED"));
  wire.flush();
  await turn();
  assert.equal(raw.highlightCalls, 1);
});

test("legacy native and legacy worker packages remain readable but never fake highlighting", async (t) => {
  for (const legacy of [true, false]) {
    const { wire, session } = await open(t, { legacy }, legacy ? undefined : (message, request) => {
      if (request.method === "create" && message.ok) message.value = null;
    });
    assert.equal(session.supportsCodeHighlighting, false);
    assert.equal(session.codeHighlighting, false);
    const count = wire.sent.length;
    await assert.rejects(session.setCodeHighlighting(true, session.token), code("UNSUPPORTED_WASM_PACKAGE"));
    assert.equal(wire.sent.length, count);
    assert.equal(typeof await session.getSource(), "string");
  }
});

test("creation honors an already enabled native session and rejects malformed capability replies", async (t) => {
  const { session } = await open(t, { enabled: true });
  assert.equal(session.codeHighlighting, true);
  for (const value of [
    { supportsCodeHighlighting: "true", codeHighlighting: false },
    { supportsCodeHighlighting: true },
    { supportsCodeHighlighting: false, codeHighlighting: true },
    { supportsCodeHighlighting: true, codeHighlighting: "false" },
  ]) {
    const raw = native();
    const wire = channel(raw, (message, request) => {
      if (request.method === "create") message.value = { supportsViewport: false, ...value };
    });
    await assert.rejects(createWorkerFlowSessionWith(() => wire.client, "hello"), code("WORKER_PROTOCOL_ERROR"));
    assert.equal(raw.freed, 1);
  }
});

test("corrupted syntax acknowledgments terminate before a queued mutation is dispatched", async (t) => {
  for (const damage of ["mode", "previous", "source", "layout", "wrong-request"]) {
    const { raw, wire, session } = await open(t, {}, (message, request) => {
      if (request.method !== "setCodeHighlighting" || !message.ok) return;
      if (damage === "mode") message.value.codeHighlighting = "true";
      if (damage === "previous") message.value.previousToken.layoutRevision = "0";
      if (damage === "source") message.value.revision = message.state.token.revision = "9007199254740994";
      if (damage === "layout") message.state.layout.viewportWidth = 300;
      if (damage === "wrong-request") {
        // An internally consistent no-op reply for an actual enabling request.
        message.value.codeHighlighting = false;
        message.value.layoutRevision = message.state.token.layoutRevision = request.args[1].layoutRevision;
      }
    });
    const active = session.setCodeHighlighting(true, session.token);
    const queued = session.replaceSource("must not run", { expectedRevision: session.revision });
    const activeCheck = assert.rejects(active, code("WORKER_PROTOCOL_ERROR"));
    const queuedCheck = assert.rejects(queued, (error) =>
      ["SESSION_LOST", "SESSION_DISPOSED"].includes(error?.code));
    await Promise.all([activeCheck, queuedCheck]);
    assert.equal(session.disposed, true, damage);
    assert.equal(wire.sent.some((request) => request.method === "replaceSource"), false, damage);
    assert.notEqual(raw.source, "must not run");
  }
});

test("public creation publishes only the requested initial mode after both acknowledgments", async (t) => {
  const { createWorkerFlowSession } = await import("./flow-worker.js");
  const raw = native();
  let wire;
  wire = channel(raw, (_message, request) => {
    if (request.method === "setCodeHighlighting") wire.hold = true;
  });
  t.after(() => wire.client.terminate());
  let published = false;
  const creating = createWorkerFlowSession(raw.source, {}, {
    workerFactory: () => wire.client, initialCodeHighlighting: true,
  }).then((session) => { published = true; return session; });
  await turn();
  assert.equal(raw.codeHighlighting, true);
  assert.equal(published, false);
  assert.deepEqual(wire.sent.map((request) => request.method), ["create", "setCodeHighlighting"]);
  wire.flush();
  const session = await creating;
  assert.equal(session.codeHighlighting, true);
  assert.equal(session.revision, "9007199254740993");
  assert.equal(session.layoutRevision, "9007199254740996");
});

test("invalid startup mode is rejected before allocating a worker", async () => {
  const { createWorkerFlowSession } = await import("./flow-worker.js");
  let allocations = 0;
  for (const initialCodeHighlighting of [1, "true", null, {}]) {
    await assert.rejects(createWorkerFlowSession("hello", {}, {
      workerFactory: () => { allocations++; throw new Error("must not allocate"); },
      initialCodeHighlighting,
    }), code("INVALID_OPTIONS"));
  }
  assert.equal(allocations, 0);
});

test("startup rejects unsupported or failed syntax setup and releases the new worker", async () => {
  for (const legacy of [true, false]) {
    const raw = native({ legacy });
    raw.rejectHighlight = true;
    const wire = channel(raw);
    await assert.rejects(createWorkerFlowSessionWith(() => wire.client, raw.source, {}, {
      initialCodeHighlighting: true,
    }), code(legacy ? "UNSUPPORTED_WASM_PACKAGE" : "BUDGET_EXCEEDED"));
    assert.equal(raw.freed, 1);
    assert.equal(raw.codeHighlighting, false);
  }
});

test("explicit plain startup remains compatible with legacy packages and omitted mode preserves native defaults", async (t) => {
  for (const [legacy, enabled, initialCodeHighlighting, expected] of [
    [true, false, false, false], [false, true, undefined, true], [false, true, false, false],
  ]) {
    const raw = native({ legacy, enabled }), wire = channel(raw);
    const session = await createWorkerFlowSessionWith(() => wire.client, raw.source, {}, {
      initialCodeHighlighting,
    });
    t.after(() => session.dispose());
    assert.equal(session.codeHighlighting, expected);
    assert.equal(raw.highlightCalls, initialCodeHighlighting === false && enabled ? 1 : 0);
  }
});

test("startup syntax setup shares the original deadline rather than renewing it", async (t) => {
  let now = 0;
  const delays = [];
  const timer = globalThis.setTimeout;
  t.mock.method(performance, "now", () => now);
  t.mock.method(globalThis, "setTimeout", (callback, delay, ...args) => {
    delays.push(delay);
    return timer(callback, delay, ...args);
  });
  const raw = native();
  const wire = channel(raw, (_message, request) => {
    if (request.method === "create") now = 200;
  });
  const session = await createWorkerFlowSessionWith(() => wire.client, raw.source, {}, {
    initialCodeHighlighting: true, startupTimeoutMs: 300,
  });
  t.after(() => session.dispose());
  assert.deepEqual(delays, [300, 100]);
  assert.equal(session.codeHighlighting, true);
});

test("an exhausted startup budget never turns into an unlimited syntax operation", async (t) => {
  let now = 0;
  t.mock.method(performance, "now", () => now);
  const raw = native();
  const wire = channel(raw, (_message, request) => {
    if (request.method === "create") now = 301;
  });
  await assert.rejects(createWorkerFlowSessionWith(() => wire.client, raw.source, {}, {
    initialCodeHighlighting: true, startupTimeoutMs: 300,
  }), code("TIMEOUT"));
  assert.deepEqual(wire.sent.map((request) => request.method), ["create"]);
  assert.equal(raw.freed, 1);
});

test("aborting during startup syntax setup rejects creation and terminates the owned worker", async () => {
  const raw = native(), wire = channel(raw);
  const controller = new AbortController();
  wire.hold = true;
  const creating = createWorkerFlowSessionWith(() => wire.client, raw.source, {}, {
    initialCodeHighlighting: true, signal: controller.signal,
  });
  const rejected = assert.rejects(creating, code("ABORTED"));
  await turn();
  // Release create, then hold the following mode reply while native setup runs.
  wire.flush();
  queueMicrotask(() => { wire.hold = true; });
  await turn();
  assert.equal(raw.highlightCalls, 1);
  controller.abort();
  await rejected;
  assert.equal(raw.freed, 1);
  wire.flush();
});

test("worker-factory time is charged before dispatch and a late plain acknowledgment cannot bypass startup expiry", async (t) => {
  let now = 0;
  t.mock.method(performance, "now", () => now);
  for (const expireInFactory of [true, false]) {
    now = 0;
    const raw = native();
    const wire = channel(raw, () => { now = 500; });
    let terminated = 0;
    const terminate = wire.client.terminate;
    wire.client.terminate = () => { terminated++; terminate(); };
    await assert.rejects(createWorkerFlowSessionWith(() => {
      if (expireInFactory) now = 500;
      return wire.client;
    }, raw.source, {}, { startupTimeoutMs: 300 }), code("TIMEOUT"));
    assert.equal(terminated, 1);
    assert.equal(wire.sent.length, expireInFactory ? 0 : 1);
    assert.equal(raw.highlightCalls, 0);
  }
});

test("explicitly disabled startup deadline remains disabled through syntax setup", async (t) => {
  const raw = native(), wire = channel(raw);
  const delays = [];
  const timer = globalThis.setTimeout;
  t.mock.method(globalThis, "setTimeout", (callback, delay, ...args) => {
    delays.push(delay);
    return timer(callback, delay, ...args);
  });
  const session = await createWorkerFlowSessionWith(() => wire.client, raw.source, {}, {
    initialCodeHighlighting: true, startupTimeoutMs: 0, timeoutMs: 1,
  });
  t.after(() => session.dispose());
  assert.equal(session.codeHighlighting, true);
  assert.deepEqual(delays, []);
});
