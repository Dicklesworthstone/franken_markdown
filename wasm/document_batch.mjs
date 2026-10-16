// Pull-driven scheduling for stateless document workers. The public entry fixes
// createRenderer to createWorkerRenderer; this module never loads WASM itself.
export class DocumentBatchError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "DocumentBatchError";
    this.code = code;
  }
}
const fail = (code, message) => { throw new DocumentBatchError(code, message); };

function record(value, keys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    fail("INVALID_OPTIONS", `${label} must be a plain object`);
  const copy = {};
  for (const key of Reflect.ownKeys(value)) {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    if (!keys.includes(key) || !descriptor || !Object.hasOwn(descriptor, "value"))
      fail("INVALID_OPTIONS", `unsupported ${label} field or accessor`);
    if (descriptor.value !== undefined) copy[key] = descriptor.value;
  }
  return copy;
}

/** Internal adapter seam. The public document-batch entry supplies the existing
 * stateless renderer, including its validation, snapshots and worker protocol. */
export function createBatchIterator(documents, options, createRenderer) {
  const settings = record(options, ["concurrency", "worker", "signal"], "batch option");
  const concurrency = settings.concurrency ?? 2;
  if (!Number.isInteger(concurrency) || concurrency < 1 || concurrency > 8)
    fail("INVALID_OPTIONS", "concurrency must be an integer from 1 through 8");
  const worker = record(settings.worker ?? {},
    ["workerFactory", "timeoutMs", "maxPendingBytes", "maxOutputBytes"], "batch worker option");
  const signal = settings.signal;
  if (signal !== undefined && (!signal || typeof signal.aborted !== "boolean"
      || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
    fail("INVALID_OPTIONS", "signal must be an AbortSignal");
  const asyncMethod = documents?.[Symbol.asyncIterator];
  const asynchronous = asyncMethod !== undefined && asyncMethod !== null;
  const iterate = asynchronous ? asyncMethod : documents?.[Symbol.iterator];
  if (typeof iterate !== "function")
    fail("INVALID_OPTIONS", "documents must be a synchronous or asynchronous iterable");

  // A factory must not lend one live/terminated endpoint to several owners.
  // Throw BEFORE returning a duplicate: the existing renderer must not terminate
  // an endpoint belonging to another lane while cleaning up failed construction.
  if (typeof worker.workerFactory === "function") {
    const factory = worker.workerFactory;
    const seen = new WeakSet();
    worker.workerFactory = () => {
      const endpoint = factory();
      if (endpoint !== null && ["object", "function"].includes(typeof endpoint)) {
        if (seen.has(endpoint)) fail("INVALID_WORKER", "batch workerFactory must return a fresh owned worker");
        seen.add(endpoint);
      }
      return endpoint;
    };
  }
  const configuration = Object.freeze({ ...worker, maxPendingOperations: 1 });
  // Validate worker settings now, but createWorkerRenderer is lazy: no Worker
  // or WASM starts until a document is actually admitted by next().
  const lanes = [{ renderer: createRenderer(configuration) }];
  let stopped = false, failed = false, interruption;
  const waiting = new Set();
  // Unlike repeatedly racing one long-lived cancellation promise, this retains
  // no per-document reactions after a wait completes. Only live waits are owned.
  function waitFor(pending) {
    return new Promise((resolve, reject) => {
      const cancel = () => { waiting.delete(cancel); resolve(); };
      waiting.add(cancel);
      Promise.resolve(pending).then(
        value => { waiting.delete(cancel); resolve(value); },
        error => { waiting.delete(cancel); reject(error); },
      );
      if (stopped) cancel();
    });
  }
  function stop(reason, isFailure = false) {
    if (stopped) return;
    stopped = true;
    interruption = reason;
    failed = isFailure;
    for (const lane of lanes) {
      try { lane.renderer?.dispose(); } catch { /* Still release other lanes. */ }
    }
    for (const cancel of waiting) cancel();
  }
  function interrupted() {
    if (failed) throw interruption;
    return stopped;
  }
  const onAbort = () => stop(new DocumentBatchError("ABORTED", "document batch was aborted"), true);

  function submit(lane, item, index) {
    let name, pending;
    try {
      const job = record(item, ["format", "source", "options", "controls", "name"], "batch document");
      if (job.name !== undefined && (typeof job.name !== "string" || job.name.length > 4096))
        fail("INVALID_OPTIONS", "document name must be a string of at most 4096 code units");
      name = job.name;
      if (interrupted()) fail("ABORTED", "document batch was closed before admission");
      if (lane.renderer?.disposed) {
        const previous = lane.renderer;
        lane.renderer = null;
        previous.dispose();
      }
      lane.renderer ??= createRenderer(configuration);
      // Do not defer this call to a promise reaction: render() validates and
      // snapshots source options/assets synchronously at admission.
      pending = lane.renderer.render(job.format, job.source, job.options, job.controls);
    } catch (reason) {
      pending = Promise.reject(reason);
    }
    const identity = name === undefined ? { index } : { index, name };
    // Attach both handlers immediately. Later results may finish before the
    // first; neither rejection nor large outputs escape the bounded window.
    return Promise.resolve(pending).then(
      value => Object.freeze({ ...identity, status: "fulfilled", value }),
      reason => Object.freeze({ ...identity, status: "rejected", reason }),
    );
  }

  async function* run() {
    let iterator, exhausted = false, listening = false, caught = false;
    try {
      if (interrupted()) return;
      if (signal) {
        listening = true;
        signal.addEventListener("abort", onAbort, { once: true });
        if (signal.aborted) onAbort();
      }
      if (interrupted()) return;
      iterator = iterate.call(documents);
      if (!iterator || typeof iterator.next !== "function")
        fail("INVALID_OPTIONS", "documents returned an invalid iterator");
      const window = [], available = [...lanes];
      let index = 0, pull;
      while (!interrupted()) {
        while (!exhausted && window.length < concurrency) {
          let step;
          if (asynchronous) {
            // At most one next() is outstanding, and its reserved slot counts
            // against the window. Observe failure immediately, even if canceled.
            pull ??= Promise.resolve(iterator.next()).then(
              value => ({ kind: "input", value }),
              error => ({ kind: "error", error }),
            );
            // A slow network/file producer must not hold a completed head hostage
            // merely because the window has spare capacity. Keep the pending pull
            // across this yield; do not start another until it has been consumed.
            const ready = await waitFor(window.length
              ? Promise.race([pull, window[0].pending.then(() => ({ kind: "output" }))])
              : pull);
            if (interrupted()) return;
            if (ready.kind === "output") break;
            pull = undefined;
            if (ready.kind === "error") throw ready.error;
            step = ready.value;
          } else {
            // Preserve immediate admission/snapshots for synchronous iterables.
            step = iterator.next();
          }
          if (!step || typeof step !== "object" || typeof step.then === "function")
            fail("INVALID_OPTIONS", "document iterator returned an invalid step");
          if (interrupted()) return;
          if (step.done) { exhausted = true; break; }
          if (!Number.isSafeInteger(index))
            fail("BUDGET_EXCEEDED", "document batch index exhausted");
          let lane = available.shift();
          if (!lane) { lane = { renderer: null }; lanes.push(lane); }
          window.push({ lane, pending: submit(lane, step.value, index++) });
          if (interrupted()) return;
        }
        if (!window.length) break;
        const entry = window.shift();
        const receipt = await waitFor(entry.pending);
        if (interrupted()) return;
        yield receipt;
        // Only consumer demand opens a slot. A slow first document or paused
        // consumer never causes a completed-output queue to grow without bound.
        available.push(entry.lane);
      }
    } catch (error) {
      caught = true;
      throw error;
    } finally {
      stop();
      if (listening) {
        try { signal.removeEventListener("abort", onAbort); } catch { /* Already closed. */ }
      }
      if (iterator && !exhausted) {
        try {
          // Request cleanup exactly once, but do not let an uncooperative source
          // keep owned workers or the consumer open. Async generators can queue
          // return() behind a stalled next(); the producer must cancel its own I/O.
          // Observe asynchronous cleanup rejection without masking the cause.
          Promise.resolve(iterator.return?.()).catch(() => {});
        } catch (error) { if (!caught && !failed) throw error; }
      }
    }
  }
  const stream = run();
  // Async-generator return() alone waits behind an outstanding next(). Stop
  // owned workers immediately so a hanging render cannot delay iterator close.
  return Object.freeze({
    next: () => stream.next(),
    return(value) { stop(); return stream.return(value); },
    throw(error) { stop(error, true); return stream.throw(error); },
    [Symbol.asyncIterator]() { return this; },
  });
}
