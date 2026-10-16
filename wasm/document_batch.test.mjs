import assert from "node:assert/strict";
import test from "node:test";
import { createBatchIterator, DocumentBatchError } from "./document_batch.mjs";

const tick = () => new Promise(resolve => setImmediate(resolve));
const item = (source, extra = {}) => ({ format: "html", source, ...extra });
const coded = code => Object.assign(new Error(code), { code });

// A controllable stateless-renderer adapter, not a WASM/rendering mock proof.
// The production entry supplies createWorkerRenderer. These tests exercise
// scheduling, admission timing and ownership without compiling a native engine.
function harness() {
  const state = { created: [], submitted: [], configurations: [], active: 0, peak: 0 };
  function create(configuration) {
    assert.equal(configuration.maxPendingOperations, 1);
    state.configurations.push(configuration);
    let disposed = false, pending = null, endpoint;
    const renderer = {
      get disposed() { return disposed; },
      render(format, source, options = {}, controls = {}) {
        if (disposed) return Promise.reject(coded("SESSION_DISPOSED"));
        if (controls.signal?.aborted) return Promise.reject(coded("ABORTED"));
        assert.equal(pending, null, "a lane must never hide queued operations");
        if (format !== "html" || typeof source !== "string") return Promise.reject(coded("INVALID_OPTIONS"));
        if (source === "sync-throw") throw coded("INVALID_OPTIONS");
        if (configuration.workerFactory && !endpoint) endpoint = configuration.workerFactory();
        const captured = structuredClone(options);
        let resolve, reject;
        const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
        const cleanup = () => {
          if (!pending) return false;
          pending = null;
          state.active--;
          controls.signal?.removeEventListener("abort", abort);
          return true;
        };
        const operation = {
          source, captured, renderer,
          finish(value = source) { if (cleanup()) resolve(value); },
          fail(code = "RENDER_FAILED", fatal = true) {
            if (!cleanup()) return;
            if (fatal) disposed = true;
            reject(coded(code));
          },
        };
        const abort = () => operation.fail("ABORTED");
        pending = operation;
        state.submitted.push(operation);
        state.active++;
        state.peak = Math.max(state.peak, state.active);
        controls.signal?.addEventListener("abort", abort, { once: true });
        return promise;
      },
      dispose() {
        if (pending) pending.fail("SESSION_DISPOSED");
        disposed = true;
      },
    };
    state.created.push(renderer);
    return renderer;
  }
  return { state, create, batch: (items, options = {}) => createBatchIterator(items, options, create) };
}

test("construction is lazy; an empty input starts no render and releases its wrapper", async () => {
  const h = harness();
  let iterated = 0;
  const source = { *[Symbol.iterator]() { iterated++; } };
  const stream = h.batch(source);
  assert.equal(iterated, 0);
  assert.equal(h.state.submitted.length, 0);
  assert.deepEqual(await stream.next(), { value: undefined, done: true });
  assert.equal(iterated, 1);
  assert.ok(h.state.created.every(renderer => renderer.disposed));
});

test("fixed window preserves input order despite reverse completion and consumer pauses", async () => {
  const h = harness();
  let pulled = 0;
  function* source() {
    for (let i = 0; i < 6; i++) { pulled++; yield item(String(i), { name: `doc-${i}` }); }
  }
  const stream = h.batch(source(), { concurrency: 3 });
  const first = stream.next();
  await tick();
  assert.equal(pulled, 3);
  assert.equal(h.state.peak, 3);
  h.state.submitted[2].finish();
  h.state.submitted[1].finish();
  await tick();
  assert.equal(pulled, 3, "completions alone must not pull more input");
  h.state.submitted[0].finish();
  const receipt = (await first).value;
  assert.deepEqual(receipt, { index: 0, name: "doc-0", status: "fulfilled", value: "0" });
  assert.ok(Object.isFrozen(receipt));
  await tick();
  assert.equal(pulled, 3, "a paused consumer holds the window closed");
  const second = await stream.next();
  assert.equal(second.value.index, 1);
  assert.equal(pulled, 4);
  assert.equal(h.state.created.length, 3, "healthy workers are reused");
  assert.ok(h.state.peak <= 3);
  await stream.return();
});

test("serial concurrency reuses one renderer and returns every result exactly once", async () => {
  const h = harness();
  const stream = h.batch([item("a"), item("b"), item("c")], { concurrency: 1 });
  for (let i = 0; i < 3; i++) {
    const next = stream.next();
    await tick();
    h.state.submitted[i].finish();
    assert.equal((await next).value.index, i);
  }
  assert.equal((await stream.next()).done, true);
  assert.equal(h.state.created.length, 1);
  assert.equal(h.state.active, 0);
  assert.deepEqual(h.state.submitted.map(operation => operation.source), ["a", "b", "c"]);
});

test("fatal failures replace only the affected lane and never replay failed documents", async () => {
  const h = harness();
  const stream = h.batch([item("bad"), item("good"), item("next")]);
  const first = stream.next();
  await tick();
  const healthy = h.state.submitted[1].renderer;
  h.state.submitted[0].fail();
  assert.equal((await first).value.reason.code, "RENDER_FAILED");
  assert.equal(healthy.disposed, false);
  h.state.submitted[1].finish();
  assert.equal((await stream.next()).value.value, "good");
  h.state.submitted[2].finish();
  assert.equal((await stream.next()).value.value, "next");
  assert.equal((await stream.next()).done, true);
  assert.equal(h.state.created.length, 3);
  assert.deepEqual(h.state.submitted.map(operation => operation.source), ["bad", "good", "next"]);
});

test("recoverable validation failures reuse their lane; synchronous throws become receipts", async () => {
  const h = harness();
  const stream = h.batch([item("sync-throw"), { ...item("x"), format: "invalid" }, item("ok")], { concurrency: 1 });
  assert.equal((await stream.next()).value.reason.code, "INVALID_OPTIONS");
  assert.equal((await stream.next()).value.reason.code, "INVALID_OPTIONS");
  const last = stream.next();
  await tick();
  h.state.submitted[0].finish();
  assert.equal((await last).value.value, "ok");
  assert.equal(h.state.created.length, 1);
  await stream.return();
});

test("options and exact asset views are captured at admission, not at later completion", async () => {
  const h = harness();
  const bytes = new Uint8Array([9, 1, 2, 9]);
  const options = { title: "before", pdfImages: [{ destination: "x", bytes: bytes.subarray(1, 3) }] };
  const stream = h.batch([item("a", { options }), item("b", { options })]);
  const first = stream.next();
  options.title = "after";
  bytes.fill(7);
  assert.equal(h.state.submitted.length, 2);
  for (const operation of h.state.submitted) {
    assert.equal(operation.captured.title, "before");
    assert.deepEqual([...operation.captured.pdfImages[0].bytes], [1, 2]);
    operation.finish();
  }
  await first;
  await stream.return();
});

test("return interrupts an outstanding next without waiting for the render to finish", async () => {
  const h = harness();
  let closed = 0;
  function* source() { try { while (true) yield item("hanging"); } finally { closed++; } }
  const stream = h.batch(source());
  const next = stream.next();
  await tick();
  const returned = stream.return("closed");
  assert.equal(h.state.active, 0, "return must dispose synchronously");
  assert.equal((await next).done, true);
  assert.deepEqual(await returned, { done: true, value: "closed" });
  assert.equal(closed, 1);
  assert.equal(h.state.submitted.length, 2);
});

test("breaking for-await releases all workers and closes a non-exhausted producer once", async () => {
  const h = harness();
  let closed = 0;
  function* source() { try { for (let i = 0; i < 10; i++) yield item(String(i)); } finally { closed++; } }
  const stream = h.batch(source());
  const consume = (async () => { for await (const receipt of stream) { assert.equal(receipt.index, 0); break; } })();
  await tick();
  h.state.submitted[0].finish();
  await consume;
  assert.equal(h.state.active, 0);
  assert.equal(closed, 1);
  assert.equal(h.state.submitted.length, 2);
});

test("global abort interrupts a hanging head and disposes unrelated active lanes", async () => {
  const h = harness(), controller = new AbortController();
  const stream = h.batch([item("a"), item("b")], { signal: controller.signal });
  const next = stream.next();
  const rejected = assert.rejects(next, { code: "ABORTED" });
  await tick();
  controller.abort();
  assert.equal(h.state.active, 0);
  await rejected;
  assert.ok(h.state.created.every(renderer => renderer.disposed));
});

test("abort while paused at a yield still terminates workers immediately", async () => {
  const h = harness(), controller = new AbortController();
  const stream = h.batch([item("a"), item("b"), item("c")], { signal: controller.signal });
  const first = stream.next();
  await tick();
  h.state.submitted[0].finish();
  await first;
  controller.abort();
  assert.equal(h.state.active, 0);
  await assert.rejects(stream.next(), { code: "ABORTED" });
  assert.equal(h.state.submitted.length, 2);
});

test("pre-aborted batches never iterate input or start a worker", async () => {
  const h = harness(), controller = new AbortController();
  controller.abort();
  let pulled = false;
  const stream = h.batch({ *[Symbol.iterator]() { pulled = true; yield item("x"); } }, { signal: controller.signal });
  await assert.rejects(stream.next(), { code: "ABORTED" });
  assert.equal(pulled, false);
  assert.equal(h.state.submitted.length, 0);
});

test("per-document abort is a receipt and leaves other documents running", async () => {
  const h = harness(), controller = new AbortController();
  const stream = h.batch([item("a", { controls: { signal: controller.signal } }), item("b"), item("c")]);
  const first = stream.next();
  await tick();
  controller.abort();
  assert.equal((await first).value.reason.code, "ABORTED");
  assert.equal(h.state.submitted[1].renderer.disposed, false);
  h.state.submitted[1].finish();
  assert.equal((await stream.next()).value.value, "b");
  h.state.submitted[2].finish();
  assert.equal((await stream.next()).value.value, "c");
  await stream.return();
});

test("TIMEOUT kills only its lane; later documents can use a fresh worker", async () => {
  const h = harness();
  const stream = h.batch([item("slow"), item("later")], { concurrency: 1 });
  const first = stream.next();
  await tick();
  h.state.submitted[0].fail("TIMEOUT");
  assert.equal((await first).value.reason.code, "TIMEOUT");
  const second = stream.next();
  await tick();
  h.state.submitted[1].finish();
  assert.equal((await second).value.value, "later");
  assert.equal(h.state.created.length, 2);
  await stream.return();
});

test("source iteration failures abort the batch and release already-admitted work", async () => {
  const h = harness();
  function* source() { yield item("active"); throw coded("SOURCE_FAILED"); }
  const stream = h.batch(source());
  await assert.rejects(stream.next(), { code: "SOURCE_FAILED" });
  assert.equal(h.state.active, 0);
  assert.ok(h.state.created.every(renderer => renderer.disposed));
});

test("an abort raised by the producer prevents the returned item from being admitted", async () => {
  const h = harness(), controller = new AbortController();
  function* source() { controller.abort(); yield item("never"); }
  const stream = h.batch(source(), { signal: controller.signal });
  await assert.rejects(stream.next(), { code: "ABORTED" });
  assert.equal(h.state.submitted.length, 0);
});

test("malformed documents, sparse arrays and accessors fail individually without executing getters", async () => {
  const h = harness();
  let reads = 0;
  const accessor = { format: "html", get source() { reads++; return "bad"; } };
  const documents = [undefined, accessor, { ...item("x"), unknown: true }, item("x", { name: "x".repeat(4097) })];
  const stream = h.batch(documents, { concurrency: 1 });
  for (let i = 0; i < documents.length; i++) {
    const receipt = (await stream.next()).value;
    assert.equal(receipt.index, i);
    assert.equal(receipt.status, "rejected");
    assert.equal(receipt.reason.code, "INVALID_OPTIONS");
  }
  assert.equal((await stream.next()).done, true);
  assert.equal(reads, 0);
  assert.equal(h.state.submitted.length, 0);
});

test("batch/worker option validation rejects unsupported keys and accessors", () => {
  const h = harness();
  for (const options of [null, [], { concurrency: 0 }, { concurrency: 9 }, { concurrency: 1.5 },
    { concurrency: NaN }, { unknown: true }, { worker: { maxPendingOperations: 2 } },
    { worker: [] }, { signal: {} }, { [Symbol("hidden")]: 1 }]) {
    assert.throws(() => h.batch([], options), DocumentBatchError);
  }
  let reads = 0;
  assert.throws(() => h.batch([], { get concurrency() { reads++; return 1; } }), DocumentBatchError);
  assert.equal(reads, 0);
  assert.throws(() => h.batch(null), DocumentBatchError);
  assert.throws(() => h.batch({ [Symbol.asyncIterator]: true }), DocumentBatchError);
});

test("worker settings are copied once, frozen, and passed to every replacement", async () => {
  const h = harness(), worker = { timeoutMs: 25, maxPendingBytes: 1024, maxOutputBytes: 2048 };
  const stream = h.batch([item("a"), item("b")], { concurrency: 1, worker });
  worker.timeoutMs = 999;
  const first = stream.next();
  await tick();
  h.state.submitted[0].fail();
  await first;
  const second = stream.next();
  await tick();
  h.state.submitted[1].finish();
  await second;
  for (const config of h.state.configurations) {
    assert.equal(config.timeoutMs, 25);
    assert.equal(config.maxPendingBytes, 1024);
    assert.ok(Object.isFrozen(config));
  }
  await stream.return();
});

test("a duplicate owned-worker endpoint cannot corrupt another active lane", async () => {
  const h = harness(), endpoint = {};
  const stream = h.batch([item("a"), item("b")], { worker: { workerFactory: () => endpoint } });
  const first = stream.next();
  await tick();
  assert.equal(h.state.submitted.length, 1);
  assert.equal(h.state.submitted[0].renderer.disposed, false);
  h.state.submitted[0].finish();
  await first;
  assert.equal((await stream.next()).value.reason.code, "INVALID_WORKER");
  await stream.return();
});

test("factory failures become per-document receipts and are not retried for that document", async () => {
  const h = harness();
  let calls = 0;
  const stream = h.batch([item("bad"), item("good")], { concurrency: 1,
    worker: { workerFactory() { if (++calls === 1) throw coded("FACTORY_FAILED"); return {}; } } });
  assert.equal((await stream.next()).value.reason.code, "FACTORY_FAILED");
  const second = stream.next();
  await tick();
  h.state.submitted[0].finish();
  assert.equal((await second).value.value, "good");
  assert.equal(calls, 2);
  assert.deepEqual(h.state.submitted.map(operation => operation.source), ["good"]);
  await stream.return();
});

test("invalid iterator results close the batch; throwing cleanup does not hide the source error", async () => {
  const h = harness();
  for (const next of [() => null, () => { throw coded("ORIGINAL"); }]) {
    const stream = h.batch({ [Symbol.iterator]() { return { next, return() { throw coded("CLEANUP"); } }; } });
    await assert.rejects(stream.next(), error => error.code !== "CLEANUP");
    assert.ok(h.state.created.every(renderer => renderer.disposed));
  }
});

test("return before first next does not open the source", async () => {
  const h = harness();
  let opened = 0;
  const stream = h.batch({ *[Symbol.iterator]() { opened++; yield item("x"); } });
  assert.equal((await stream.return()).done, true);
  assert.equal((await stream.next()).done, true);
  assert.equal(opened, 0);
  assert.equal(h.state.submitted.length, 0);
});

test("signal-listener registration failures still clean up the renderer", async () => {
  const h = harness();
  let removed = 0;
  const signal = { aborted: false, addEventListener() { throw coded("SIGNAL_FAILED"); }, removeEventListener() { removed++; } };
  await assert.rejects(h.batch([item("x")], { signal }).next(), { code: "SIGNAL_FAILED" });
  assert.equal(removed, 1);
  assert.ok(h.state.created.every(renderer => renderer.disposed));
});

test("all receipts remain in input order for a larger bounded batch", async () => {
  const h = harness();
  const stream = h.batch(Array.from({ length: 100 }, (_, i) => item(String(i))), { concurrency: 4 });
  for (let i = 0; i < 100; i++) {
    const next = stream.next();
    await tick();
    for (const operation of [...h.state.submitted].reverse()) operation.finish();
    const receipt = (await next).value;
    assert.equal(receipt.index, i);
    assert.equal(receipt.value, String(i));
    assert.ok(h.state.peak <= 4);
    assert.ok(h.state.created.length <= 4);
  }
  assert.equal((await stream.next()).done, true);
  assert.equal(h.state.active, 0);
});


test("a promise-returning producer is rejected rather than treated as a synchronous step", async () => {
  const h = harness();
  const stream = h.batch({ [Symbol.iterator]() { return { next: () => Promise.resolve({ done: true }) }; } });
  await assert.rejects(stream.next(), { code: "INVALID_OPTIONS" });
  assert.equal(h.state.submitted.length, 0);
});

test("throw interrupts an outstanding next even when the thrown value is null", async () => {
  const h = harness();
  const stream = h.batch([item("hanging")]);
  const next = stream.next().then(() => "unexpected success", reason => reason);
  await tick();
  const thrown = stream.throw(null).then(() => "unexpected success", reason => reason);
  assert.equal(h.state.active, 0);
  assert.equal(await next, null);
  assert.equal(await thrown, null);
});
