import assert from "node:assert/strict";
import test from "node:test";
import { createBatchIterator } from "./document_batch.mjs";

const tick = () => new Promise(resolve => setImmediate(resolve));
const check = (name, fn) => test(name, { timeout: 2000 }, fn);
const item = source => ({ format: "html", source, name: `doc-${source}` });
function harness() {
  const operations = [], renderers = [];
  function create() {
    let disposed = false, pending;
    const renderer = {
      get disposed() { return disposed; },
      render(format, source) {
        assert.equal(disposed, false);
        assert.equal(pending, undefined);
        return new Promise((resolve, reject) => {
          const operation = {
            source, renderer,
            finish() { if (pending === operation) { pending = undefined; resolve(source); } },
            fail() { if (pending === operation) { pending = undefined; disposed = true; reject(new Error("render failed")); } },
          };
          pending = operation;
          operations.push(operation);
        });
      },
      dispose() { pending?.fail(); disposed = true; },
    };
    renderers.push(renderer);
    return renderer;
  }
  return { operations, renderers,
    batch: (source, options = {}) => createBatchIterator(source, { order: "completion", ...options }, create) };
}

check("completion order frees fast lanes while the first document is still running", async () => {
  const h = harness();
  const stream = h.batch([item("slow"), item("fast"), item("later")]);
  const first = stream.next();
  await tick();
  h.operations[1].finish();
  assert.deepEqual((await first).value, { index: 1, name: "doc-fast", status: "fulfilled", value: "fast" });
  assert.equal(h.operations.length, 2, "delivery alone does not refill the window");
  const next = stream.next();
  await tick();
  assert.equal(h.operations[2].renderer, h.operations[1].renderer);
  h.operations[2].finish();
  assert.equal((await next).value.index, 2);
  const last = stream.next();
  h.operations[0].finish();
  assert.equal((await last).value.index, 0);
  assert.equal((await stream.next()).done, true);
  assert.equal(h.renderers.length, 2);
});

check("receipts retain actual settlement order across consumer pauses", async () => {
  const h = harness();
  const stream = h.batch([0, 1, 2, 3].map(item), { concurrency: 4 });
  const first = stream.next();
  await tick();
  h.operations[2].finish();
  assert.equal((await first).value.index, 2);
  h.operations[3].finish();
  h.operations[1].finish();
  h.operations[0].finish();
  await tick();
  for (const index of [3, 1, 0]) assert.equal((await stream.next()).value.index, index);
  assert.equal((await stream.next()).done, true);
});

check("a permanently slow lane cannot cause unbounded admission or worker creation", async () => {
  const h = harness();
  let pulled = 0;
  function* source() { for (let i = 0; i < 301; i++) { pulled++; yield item(i); } }
  const stream = h.batch(source(), { concurrency: 2 });
  for (let i = 1; i <= 300; i++) {
    const next = stream.next();
    await tick();
    assert.equal(pulled, i + 1);
    h.operations[i].finish();
    assert.equal((await next).value.index, i);
    assert.equal(h.renderers.length, 2);
  }
  await stream.return();
  assert.ok(h.renderers.every(renderer => renderer.disposed));
});

check("failed fast documents are reported immediately and only their lane is replaced", async () => {
  const h = harness();
  const stream = h.batch([item("slow"), item("bad"), item("next")]);
  const first = stream.next();
  await tick();
  h.operations[1].fail();
  const receipt = (await first).value;
  assert.equal(receipt.index, 1);
  assert.equal(receipt.status, "rejected");
  assert.equal(h.operations[0].renderer.disposed, false);
  const second = stream.next();
  await tick();
  assert.notEqual(h.operations[2].renderer, h.operations[1].renderer);
  h.operations[2].finish();
  assert.equal((await second).value.index, 2);
  await stream.return();
  assert.equal(h.operations.length, 3, "failed input must never be replayed");
});

check("async completion order yields fast output while the producer and head both stall", async () => {
  const h = harness();
  let pulls = 0, closed = 0;
  const stream = h.batch({ [Symbol.asyncIterator]() { return {
    next() { return pulls++ < 2 ? Promise.resolve({ value: item(pulls) }) : new Promise(() => {}); },
    return() { closed++; return Promise.resolve({ done: true }); },
  }; } }, { concurrency: 4 });
  const first = stream.next();
  await tick();
  h.operations[1].finish();
  assert.equal((await first).value.index, 1);
  const next = stream.next();
  await tick();
  h.operations[0].finish();
  assert.equal((await next).value.index, 0);
  assert.equal(pulls, 3);
  await stream.return();
  assert.equal(closed, 1);
});

check("completion waits are immediately interruptible even with no settled document", async () => {
  const h = harness(), controller = new AbortController();
  const stream = h.batch([item("a"), item("b")], { signal: controller.signal });
  const next = stream.next();
  const rejected = assert.rejects(next, { code: "ABORTED" });
  await tick();
  controller.abort();
  await rejected;
  assert.ok(h.renderers.every(renderer => renderer.disposed));
});

check("explicit input ordering retains the existing default behavior", async () => {
  const h = harness();
  const stream = h.batch([item("a"), item("b")], { order: "input" });
  const first = stream.next();
  let delivered = false;
  first.then(() => { delivered = true; });
  await tick();
  h.operations[1].finish();
  await tick();
  assert.equal(delivered, false);
  h.operations[0].finish();
  assert.equal((await first).value.index, 0);
  assert.equal((await stream.next()).value.index, 1);
  await stream.return();
});

test("unsupported order values are rejected before workers or input are opened", () => {
  const h = harness();
  for (const order of ["unordered", "", true, 1, {}, []]) {
    assert.throws(() => h.batch([], { order }), { code: "INVALID_OPTIONS" });
  }
  assert.equal(h.renderers.length, 0);
});
