import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";

const source = ["one", "two", "three"].map((text) => `${text} ${"x".repeat(100)}`).join("\n");
const readers = { snapshot: "items", readingOrder: "nodes", pendingAssets: "requests" };

async function create(t, data = {}, text = source) {
  const tracking = { sent: [], packets: [], terminated: false };
  const session = await createWorkerFlowSessionWith(
    () => {
      const worker = new Worker(new URL("./tests/flow_page_worker_fixture.mjs", import.meta.url), {
        workerData: data,
      });
      const listeners = new Map();
      worker.on("error", () => {});
      return {
        postMessage(message, transfer) {
          tracking.sent.push(message.method);
          worker.postMessage(message, transfer);
        },
        terminate() {
          tracking.terminated = true;
          return worker.terminate();
        },
        addEventListener(kind, listener) {
          const wrapped = kind === "message" ? (message) => {
            if (message.value?.kind) tracking.packets.push(message.value.kind);
            listener({ data: message });
          } : (error) => listener({ error });
          listeners.set(listener, wrapped);
          worker.on(kind, wrapped);
        },
        removeEventListener(kind, listener) {
          worker.off(kind, listeners.get(listener));
          listeners.delete(listener);
        },
      };
    },
    text,
    {},
    { timeoutMs: 5000, startupTimeoutMs: 5000 },
  );
  t.after(() => session.dispose());
  return { session, tracking };
}

function closed(session, tracking) {
  assert.equal(session.disposed, true);
  assert.equal(tracking.terminated, true);
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.pendingBytes, 0);
}

for (const [method, key] of Object.entries(readers)) {
  test(`${method} accepts full, short-tail and exact-end pages and recoverable invalid offsets`, async (t) => {
    const { session } = await create(t);
    const token = session.token;
    const full = await session[method]({ offset: 0, limit: 2, token });
    assert.equal(full[key].length, 2);
    assert.equal(full.nextOffset, 2);
    const tail = await session[method]({ offset: 2, limit: 2, token });
    assert.equal(tail[key].length, 1);
    assert.equal(tail.nextOffset, null);
    const end = await session[method]({ offset: 3, limit: 2, token });
    assert.deepEqual(end[key], []);
    assert.equal(end.nextOffset, null);
    await assert.rejects(session[method]({ offset: 4, token }), { code: "INVALID_PAGE" });
    assert.equal(session.disposed, false);
    assert.equal((await session[method]({ token }))[key].length, 3);
  });

  test(`${method} accepts an empty inventory`, async (t) => {
    const { session } = await create(t, {}, "");
    const page = await session[method]({ token: session.token });
    assert.equal(page.offset, 0);
    assert.equal(page.total, 0);
    assert.equal(page.nextOffset, null);
    assert.deepEqual(page[key], []);
  });

  test(`${method} rejects the wrong page before dispatching a queued edit`, async (t) => {
    const { session, tracking } = await create(t, { fault: "offset", faultMethod: method });
    const page = session[method]({ offset: 1, limit: 1, token: session.token });
    const edit = session.replaceSource("must not be sent", { expectedRevision: session.revision });
    await Promise.all([
      assert.rejects(page, { code: "WORKER_PROTOCOL_ERROR" }),
      assert.rejects(edit, { code: "SESSION_DISPOSED" }),
    ]);
    assert.equal(tracking.sent.includes("replaceSource"), false);
    closed(session, tracking);
  });

  for (const [fault, limit] of [["short", 2], ["long", 1], ["total", 1]]) {
    test(`${method} rejects internally advancing ${fault} inventory replies`, async (t) => {
      const { session, tracking } = await create(t, { fault, faultMethod: method });
      await assert.rejects(session[method]({ limit }), { code: "WORKER_PROTOCOL_ERROR" });
      closed(session, tracking);
    });
  }

  for (const fault of ["revision", "layoutRevision"]) {
    test(`${method} rejects another ${fault} even when the worker acknowledges it`, async (t) => {
      const { session, tracking } = await create(t, { fault, faultMethod: method });
      await assert.rejects(session[method]({ token: session.token }), { code: "WORKER_PROTOCOL_ERROR" });
      closed(session, tracking);
    });
  }

  test(`${method} preserves omitted-token reads at dispatch after an edit`, async (t) => {
    const { session } = await create(t);
    const token = session.token;
    const edit = session.replaceSource("updated", { expectedRevision: token.revision });
    const page = session[method]();
    await edit;
    const result = await page;
    assert.equal(result.revision, "2");
    assert.equal(result.layoutRevision, "2");
    assert.equal(result[key].length, 1);
    await assert.rejects(session[method]({ token }), { code: "STALE_REVISION" });
    assert.equal(session.disposed, false);
  });
}

test("legacy full snapshot replies retain the same request fence", async (t) => {
  const good = await create(t, { legacy: true });
  assert.equal((await good.session.snapshot({ offset: 1, limit: 1 })).items[0].index, 1);
  assert.equal(good.tracking.sent.includes("snapshot"), true);
  assert.equal(good.tracking.sent.includes("snapshotDelta"), false);
  const bad = await create(t, { legacy: true, fault: "offset" });
  await assert.rejects(bad.session.snapshot({ offset: 1, limit: 1 }), { code: "WORKER_PROTOCOL_ERROR" });
  closed(bad.session, bad.tracking);
});

test("negotiated deltas preserve caller isolation and valid pagination", async (t) => {
  const { session, tracking } = await create(t);
  const options = { offset: 1, limit: 1, token: session.token };
  const first = await session.snapshot(options);
  first.items[0].text = "caller modification";
  const second = await session.snapshot(options);
  assert.equal(second.items[0].text, source.split("\n")[1]);
  assert.deepEqual(tracking.packets, ["full", "delta"]);
  const pages = [];
  for await (const page of session.pages({ limit: 2 })) pages.push(page);
  assert.deepEqual(pages.map((page) => [page.offset, page.items.length, page.nextOffset]), [
    [0, 2, 2], [2, 1, null],
  ]);
});

for (const fault of ["offset", "revision", "layoutRevision"]) {
  test(`decoded snapshot deltas still reject a mismatched ${fault}`, async (t) => {
    const { session, tracking } = await create(t, { fault, faultAt: 2 });
    const options = { offset: 1, limit: 1, token: session.token };
    await session.snapshot(options);
    await assert.rejects(session.snapshot(options), { code: "WORKER_PROTOCOL_ERROR" });
    assert.deepEqual(tracking.packets, ["full", "delta"]);
    closed(session, tracking);
  });
}

test("pages rejects a replayed cursor instead of yielding the same page forever", async (t) => {
  const { session, tracking } = await create(t, { fault: "offset", faultAt: 2 });
  const pages = session.pages({ limit: 1 });
  assert.equal((await pages.next()).value.offset, 0);
  await assert.rejects(pages.next(), { code: "WORKER_PROTOCOL_ERROR" });
  closed(session, tracking);
});

for (const method of ["hitTest", "selectText"]) {
  const read = (session, token) => method === "hitTest"
    ? session.hitTest(0, 0, token) : session.selectText(0, 0, 3, token);
  test(`${method} accepts the requested normalized snapshot token`, async (t) => {
    const { session } = await create(t);
    const result = await read(session, { revision: 1n, layoutRevision: 1n });
    assert.equal(result.revision, "1");
    assert.equal(result.layoutRevision, "1");
    if (method === "selectText") assert.equal(result.text, "one");
  });
  for (const fault of ["revision", "layoutRevision"]) {
    test(`${method} rejects another acknowledged ${fault}`, async (t) => {
      const { session, tracking } = await create(t, { fault, faultMethod: method });
      await assert.rejects(read(session, session.token), { code: "WORKER_PROTOCOL_ERROR" });
      closed(session, tracking);
    });
  }
}

test("queued page options and tokens are copied before caller mutation", async (t) => {
  const { session } = await create(t);
  const first = session.getSource();
  const options = { offset: 1, limit: 1, token: session.token };
  const page = session.snapshot(options);
  options.offset = 2;
  options.limit = 2;
  options.token.revision = "999";
  await first;
  const result = await page;
  assert.equal(result.offset, 1);
  assert.equal(result.items.length, 1);
  assert.equal(result.revision, "1");
});

test("queued page cancellation stays recoverable and releases operation budgets", async (t) => {
  const { session, tracking } = await create(t);
  const active = session.getSource();
  const controller = new AbortController();
  const cancelled = session.readingOrder({ limit: 1 }, { signal: controller.signal });
  const rejection = assert.rejects(cancelled, { code: "ABORTED" });
  controller.abort();
  await rejection;
  assert.equal(await active, source);
  assert.equal(tracking.sent.includes("readingOrder"), false);
  assert.equal(session.disposed, false);
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.pendingBytes, 0);
  assert.equal((await session.snapshot({ limit: 1 })).items.length, 1);
});
