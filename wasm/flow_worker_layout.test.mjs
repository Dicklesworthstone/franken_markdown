import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";
import { OwnedWorkerRpc, WORKER_PROTOCOL } from "./worker_transport.mjs";

// Real worker threads run the production protocol and synchronous JS adapter.
// Only the raw native binding is doubled, as in flow_worker.test.mjs. Corruption
// is introduced at the client transport boundary, never in the request oracle.
function fixture(t, corrupt = () => {}, data = {}) {
  const tracking = { sent: [], terminated: 0, workers: [], stops: [] };
  let session;
  t.after(async () => {
    session?.dispose();
    await Promise.all(tracking.workers.map(worker => worker.terminate()));
    await Promise.all(tracking.stops);
  });
  return {
    tracking,
    async create(options = {}) {
      session = await createWorkerFlowSessionWith(() => {
        const worker = new Worker(new URL("./tests/flow_worker_fixture.mjs", import.meta.url), {
          workerData: data,
        });
        tracking.workers.push(worker);
        worker.on("error", () => {});
        const listeners = new Map();
        let request;
        return {
          postMessage(message, transfer) {
            request = structuredClone(message);
            tracking.sent.push(request);
            worker.postMessage(message, transfer);
          },
          addEventListener(kind, listener) {
            const wrapped = kind === "message" ? message => {
              corrupt(message, request);
              listener({ data: message });
            } : error => listener({ error });
            listeners.set(listener, wrapped);
            worker.on(kind, wrapped);
          },
          removeEventListener(kind, listener) {
            worker.off(kind, listeners.get(listener));
            listeners.delete(listener);
          },
          terminate() {
            tracking.terminated++;
            const stopped = worker.terminate();
            tracking.stops.push(stopped);
            return stopped;
          },
        };
      }, "# Current source", options, { timeoutMs: 5000, startupTimeoutMs: 5000 });
      return session;
    },
  };
}

test("creation acknowledges exactly the normalized viewport and typography", async t => {
  const f = fixture(t);
  const options = { font: "serif", viewportWidth: 417.1, bodySize: 12.6, codeSize: 11.2, lineHeight: 18.4 };
  const session = await f.create(options);
  assert.deepEqual(session.layoutOptions, {
    viewportWidth: Math.fround(417.1), bodySize: Math.fround(12.6),
    codeSize: Math.fround(11.2), lineHeight: Math.fround(18.4),
  });
  assert.equal(await session.getSource(), "# Current source");
  assert.equal(f.tracking.terminated, 0);
});

for (const [key, wrong] of [["viewportWidth", 800], ["bodySize", 14], ["codeSize", 13], ["lineHeight", 20]]) {
  test(`creation refuses a worker that acknowledges a different ${key}`, async t => {
    const f = fixture(t, (message, request) => {
      if (request.method === "create" && message.ok) message.state.layout[key] = wrong;
    });
    await assert.rejects(f.create({ viewportWidth: 417.1, bodySize: 12.6, codeSize: 11.2, lineHeight: 18.4 }),
      { code: "WORKER_PROTOCOL_ERROR" });
    assert.equal(f.tracking.terminated, 1);
    assert.deepEqual(f.tracking.sent.map(message => message.method), ["create"]);
  });
}

const reflowFaults = {
  "ignored viewport width": message => { message.state.layout.viewportWidth = 800; },
  "ignored body size": message => { message.state.layout.bodySize = 14; },
  "ignored code size": message => { message.state.layout.codeSize = 13; },
  "ignored line height": message => { message.state.layout.lineHeight = 20; },
  "changed source revision": message => { message.value.revision = message.state.token.revision = "2"; },
  "unchanged layout revision": message => { message.value.layoutRevision = message.state.token.layoutRevision = "1"; },
};
for (const [name, corrupt] of Object.entries(reflowFaults)) {
  test(`reflow refuses ${name} before dispatching a queued edit`, async t => {
    const f = fixture(t, (message, request) => {
      if (request.method === "reflow" && message.ok) corrupt(message);
    });
    const session = await f.create();
    const reflow = session.reflow({ viewportWidth: 360, bodySize: 16, codeSize: 12, lineHeight: 24 }, session.token);
    const edit = session.replaceSource("must not be sent", { expectedRevision: session.revision });
    await Promise.all([
      assert.rejects(reflow, { code: "WORKER_PROTOCOL_ERROR" }),
      assert.rejects(edit, { code: "SESSION_LOST" }),
    ]);
    assert.equal(session.disposed, true);
    assert.equal(session.pendingOperations, 0);
    assert.equal(session.pendingBytes, 0);
    assert.equal(f.tracking.terminated, 1);
    assert.deepEqual(f.tracking.sent.map(message => message.method), ["create", "reflow"]);
  });
}

test("a partial reflow cannot change an unrequested typography field", async t => {
  const f = fixture(t, (message, request) => {
    if (request.method === "reflow" && message.ok) message.state.layout.bodySize = 15;
  });
  const session = await f.create();
  await assert.rejects(session.reflow({ viewportWidth: 360 }, session.token), { code: "WORKER_PROTOCOL_ERROR" });
  assert.equal(session.disposed, true);
  assert.equal(f.tracking.terminated, 1);
});

test("a success reply cannot silently rebase a stale reflow token", async t => {
  const f = fixture(t, (message, request) => {
    if (request.method === "reflow" && !message.ok) {
      message.ok = true;
      message.value = { revision: "1", layoutRevision: "2" };
      message.state = { token: { ...message.value }, layout: {
        viewportWidth: 360, bodySize: 14, codeSize: 13, lineHeight: 20,
      } };
    }
  });
  const session = await f.create();
  await assert.rejects(session.reflow({ viewportWidth: 360 }, { revision: "1", layoutRevision: "0" }),
    { code: "WORKER_PROTOCOL_ERROR" });
  assert.equal(session.disposed, true);
  assert.equal(f.tracking.terminated, 1);
});

test("queued partial reflows validate against the preceding acknowledged layout", async t => {
  const f = fixture(t);
  const session = await f.create();
  const token = session.token;
  const first = session.reflow({ viewportWidth: 640.1, bodySize: 16.2, lineHeight: 24.5 }, token);
  // A caller may explicitly predict the next token. Validation must still use
  // the first successful reflow's geometry, not the old layout at enqueue.
  const second = session.reflow({ codeSize: 15.1 }, {
    revision: token.revision, layoutRevision: String(BigInt(token.layoutRevision) + 1n),
  });
  const [one, two] = await Promise.all([first, second]);
  assert.deepEqual(one, { revision: "1", layoutRevision: "2" });
  assert.deepEqual(two, { revision: "1", layoutRevision: "3" });
  assert.deepEqual(session.layoutOptions, {
    viewportWidth: Math.fround(640.1), bodySize: Math.fround(16.2),
    codeSize: Math.fround(15.1), lineHeight: Math.fround(24.5),
  });
  assert.equal(session.disposed, false);
  assert.equal(f.tracking.terminated, 0);
});

test("recoverable reflow errors preserve previous geometry and permit retry", async t => {
  const f = fixture(t);
  const session = await f.create({ viewportWidth: 700 });
  const token = session.token, layout = session.layoutOptions;
  await assert.rejects(session.reflow({ viewportWidth: 13 }, token), { code: "SHAPING" });
  assert.deepEqual(session.token, token);
  assert.deepEqual(session.layoutOptions, layout);
  await session.reflow({ viewportWidth: 360.2 }, token);
  assert.equal(session.layoutOptions.viewportWidth, Math.fround(360.2));
  await session.reflow({}, session.token);
  assert.equal(session.disposed, false);
});

test("request validation fails before shared acknowledged state can change", async t => {
  const listeners = new Map(), sent = [];
  let terminated = 0;
  const initial = { viewportWidth: 800 };
  let accepted = initial;
  const worker = {
    addEventListener: (kind, listener) => listeners.set(kind, listener),
    removeEventListener: kind => listeners.delete(kind),
    postMessage: message => sent.push(message),
    terminate: () => { terminated++; },
  };
  const rpc = new OwnedWorkerRpc(worker, {}, state => { accepted = state; });
  t.after(() => rpc.dispose());
  const pending = rpc.request("reflow", 1, () => ({ args: [] }), {}, state => {
    assert.equal(accepted, initial);
    if (state.viewportWidth !== 360) throw new Error("wrong requested width");
  });
  const queued = rpc.request("edit", 1, () => ({ args: [] }));
  const results = Promise.all([
    assert.rejects(pending, { code: "WORKER_PROTOCOL_ERROR" }),
    assert.rejects(queued, { code: "SESSION_LOST" }),
  ]);
  listeners.get("message")({ data: {
    protocol: WORKER_PROTOCOL, id: sent[0].id, ok: true,
    value: null, state: { viewportWidth: 100 },
  } });
  await results;
  assert.equal(accepted, initial, "a failed request must not publish its claimed state");
  assert.equal(sent.length, 1);
  assert.equal(terminated, 1);
  assert.equal(rpc.pendingOperations, 0);
  assert.equal(rpc.pendingBytes, 0);
});
