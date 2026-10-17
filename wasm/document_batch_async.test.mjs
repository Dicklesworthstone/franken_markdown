import assert from "node:assert/strict";
import test from "node:test";
import { createBatchIterator, DocumentBatchError } from "./document_batch.mjs";

const tick = () => new Promise(resolve => setImmediate(resolve));
const item = source => ({ format: "html", source });
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
// Scheduler-only adapter: this does not claim to test the Rust/WASM engine.
function harness() {
  const operations = [], renderers = [];
  function create() {
    let disposed = false, pending;
    const renderer = {
      get disposed() { return disposed; },
      render(format, source, options) {
        assert.equal(disposed, false);
        assert.equal(pending, undefined, "one operation per lane");
        const result = deferred();
        pending = result;
        operations.push({ source, options: structuredClone(options),
          finish(value = source) { if (pending === result) { pending = undefined; result.resolve(value); } } });
        return result.promise;
      },
      dispose() { disposed = true; pending?.reject(new Error("disposed")); pending = undefined; },
    };
    renderers.push(renderer);
    return renderer;
  }
  return { operations, renderers, batch: (source, options = {}) => createBatchIterator(source, options, create) };
}

// Timeouts turn an accidental wait on an uncooperative source into a failure.
const check = (name, fn) => test(name, { timeout: 2000 }, fn);

check("async construction and return before first next never open the producer", async () => {
  const h = harness();
  let opened = 0;
  const stream = h.batch({ async *[Symbol.asyncIterator]() { opened++; yield item("x"); } });
  assert.equal(opened, 0);
  assert.deepEqual(await stream.return("closed"), { value: "closed", done: true });
  assert.equal(opened, 0);
  assert.equal(h.operations.length, 0);
  assert.ok(h.renderers.every(renderer => renderer.disposed));
});

check("async inputs obey the fixed window, input order and consumer backpressure", async () => {
  const h = harness();
  let pulled = 0, closed = 0;
  async function* source() {
    try { for (let i = 0; i < 20; i++) { pulled++; yield item(String(i)); } }
    finally { closed++; }
  }
  const stream = h.batch(source(), { concurrency: 3 });
  const first = stream.next();
  await tick();
  assert.equal(pulled, 3);
  assert.equal(h.operations.length, 3);
  h.operations[2].finish();
  h.operations[1].finish();
  await tick();
  assert.equal(pulled, 3);
  h.operations[0].finish();
  assert.equal((await first).value.index, 0);
  await tick();
  assert.equal(pulled, 3);
  assert.equal((await stream.next()).value.index, 1);
  assert.ok(pulled <= 4);
  await stream.return();
  await tick();
  assert.equal(closed, 1);
  assert.ok(h.renderers.length <= 3);
  assert.ok(h.renderers.every(renderer => renderer.disposed));
});

check("async iteration takes precedence without touching the synchronous method", async () => {
  const h = harness();
  const source = {
    async *[Symbol.asyncIterator]() { yield item("async"); },
    get [Symbol.iterator]() { throw new Error("must not read sync iterator"); },
  };
  const stream = h.batch(source);
  const first = stream.next();
  await tick();
  assert.equal(h.operations[0].source, "async");
  h.operations[0].finish();
  assert.equal((await first).value.value, "async");
  assert.equal((await stream.next()).done, true);
});

check("a stalled next input does not withhold an already-rendered document", async () => {
  const h = harness(), gate = deferred();
  let pulls = 0, closed = 0;
  const stream = h.batch({ [Symbol.asyncIterator]() { return {
    next() { return ++pulls === 1 ? Promise.resolve({ value: item("first") }) : gate.promise; },
    return() { closed++; return Promise.resolve({ done: true }); },
  }; } }, { concurrency: 4 });
  const first = stream.next();
  await tick();
  assert.equal(pulls, 2);
  h.operations[0].finish();
  assert.equal((await first).value.value, "first");
  await tick();
  assert.equal(pulls, 2, "no new pulls while the consumer is paused");
  const second = stream.next();
  await tick();
  assert.equal(pulls, 2, "never overlap next calls on the source");
  gate.resolve({ done: true });
  assert.equal((await second).done, true);
  assert.equal(closed, 0, "an exhausted iterator does not need closing");
});

check("abort interrupts pending input and observes late next/cleanup rejections", async () => {
  const h = harness(), gate = deferred(), controller = new AbortController();
  let closed = 0;
  const stream = h.batch({ [Symbol.asyncIterator]() { return {
    next: () => gate.promise,
    return() { closed++; return Promise.reject(new Error("late cleanup failure")); },
  }; } }, { signal: controller.signal });
  const next = stream.next();
  const rejected = assert.rejects(next, { code: "ABORTED" });
  await tick();
  controller.abort();
  assert.ok(h.renderers.every(renderer => renderer.disposed));
  await rejected;
  assert.equal(closed, 1);
  gate.reject(new Error("late source failure"));
  await tick();
  assert.equal(h.operations.length, 0);
});

check("return does not wait for either stalled async input or stalled cleanup", async () => {
  const h = harness(), gate = deferred();
  let closed = 0;
  const stream = h.batch({ [Symbol.asyncIterator]() { return {
    next: () => gate.promise,
    return() { closed++; return new Promise(() => {}); },
  }; } });
  const next = stream.next();
  await tick();
  const returned = stream.return("closed");
  assert.ok(h.renderers.every(renderer => renderer.disposed));
  assert.deepEqual(await next, { value: undefined, done: true });
  assert.deepEqual(await returned, { value: "closed", done: true });
  gate.resolve({ value: item("too late") });
  await tick();
  assert.equal(closed, 1);
  assert.equal(h.operations.length, 0);
});

check("throw(null) interrupts pending input without losing the original value", async () => {
  const h = harness();
  const stream = h.batch({ [Symbol.asyncIterator]() { return { next: () => new Promise(() => {}) }; } });
  const next = stream.next().then(() => "unexpected success", error => error);
  await tick();
  const thrown = stream.throw(null).then(() => "unexpected success", error => error);
  assert.equal(await next, null);
  assert.equal(await thrown, null);
  assert.ok(h.renderers.every(renderer => renderer.disposed));
});

check("async source failures release active workers and preserve the primary error", async () => {
  const h = harness(), original = new Error("source failed");
  let pulls = 0, closed = 0;
  const stream = h.batch({ [Symbol.asyncIterator]() { return {
    next() { return ++pulls === 1 ? Promise.resolve({ value: item("active") }) : Promise.reject(original); },
    return() { closed++; return Promise.reject(new Error("cleanup failed")); },
  }; } });
  await assert.rejects(stream.next(), error => error === original);
  assert.equal(h.operations.length, 1);
  assert.equal(closed, 1);
  assert.ok(h.renderers.every(renderer => renderer.disposed));
  await tick();
});

check("invalid async methods, iterators and resolved steps are rejected", async () => {
  const h = harness();
  for (const value of [0, false, {}, "async"]) {
    assert.throws(() => h.batch({ [Symbol.asyncIterator]: value, *[Symbol.iterator]() {} }), DocumentBatchError);
  }
  for (const value of [null, 3, "step"]) {
    await assert.rejects(h.batch({ [Symbol.asyncIterator]() { return { next: () => Promise.resolve(value) }; } }).next(), { code: "INVALID_OPTIONS" });
  }
  await assert.rejects(h.batch({ [Symbol.asyncIterator]() { return {}; } }).next(), { code: "INVALID_OPTIONS" });
  assert.equal(h.operations.length, 0);
});

check("a producer-triggered abort never admits the returned asynchronous item", async () => {
  const h = harness(), controller = new AbortController();
  const stream = h.batch({ [Symbol.asyncIterator]() { return {
    async next() { controller.abort(); return { value: item("never") }; },
  }; } }, { signal: controller.signal });
  await assert.rejects(stream.next(), { code: "ABORTED" });
  assert.equal(h.operations.length, 0);
});

check("null async method falls back to synchronous iteration and immediate snapshots", async () => {
  const h = harness(), options = { title: "before", bytes: new Uint8Array([1, 2]) };
  const documents = [{ ...item("x"), options }];
  documents[Symbol.asyncIterator] = null;
  const stream = h.batch(documents);
  const first = stream.next();
  options.title = "after";
  options.bytes.fill(9);
  assert.deepEqual(h.operations[0].options, { title: "before", bytes: new Uint8Array([1, 2]) });
  h.operations[0].finish();
  await first;
  await stream.return();
});

check("large async batches yield every document once with bounded lane reuse", async () => {
  const h = harness();
  let pulling = false, pulls = 0;
  const stream = h.batch({ [Symbol.asyncIterator]() { return {
    async next() {
      assert.equal(pulling, false);
      pulling = true;
      await Promise.resolve();
      pulling = false;
      return pulls < 300 ? { value: item(String(pulls++)) } : { done: true };
    },
  }; } }, { concurrency: 4 });
  for (let i = 0; i < 300; i++) {
    const next = stream.next();
    await tick();
    for (const operation of h.operations) operation.finish();
    const receipt = (await next).value;
    assert.equal(receipt.index, i);
    assert.equal(receipt.value, String(i));
    assert.ok(pulls <= i + 4);
    assert.ok(h.renderers.length <= 4);
  }
  assert.equal((await stream.next()).done, true);
  assert.ok(h.renderers.every(renderer => renderer.disposed));
});

check("ReadableStream input is canceled when the batch consumer breaks", async () => {
  const h = harness();
  let canceled = 0, generated = 0;
  const source = new ReadableStream({
    pull(controller) { controller.enqueue(item(String(generated++))); },
    cancel() { canceled++; },
  }, { highWaterMark: 0 });
  const stream = h.batch(source, { concurrency: 2 });
  const consume = (async () => { for await (const receipt of stream) { assert.equal(receipt.index, 0); break; } })();
  await tick();
  h.operations[0].finish();
  await consume;
  await tick();
  assert.equal(canceled, 1);
  assert.equal(generated, 2);
  assert.ok(h.renderers.every(renderer => renderer.disposed));
});
