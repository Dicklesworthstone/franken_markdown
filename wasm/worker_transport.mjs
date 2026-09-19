// Dedicated-worker RPC. Exactly one request crosses the worker boundary at a
// time; queued work stays cancellable without an unknown remote mutation.
export const WORKER_PROTOCOL = "franken-markdown-worker-v1";

export class FlowWorkerError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = "FlowWorkerError";
    this.code = code;
  }
}
const failure = (code, message) => new FlowWorkerError(code, message);

export function workerLimits(options = {}) {
  const defaults = { maxPendingOperations: 32, maxPendingBytes: 16 * 1024 * 1024, timeoutMs: 30000 };
  if (!options || typeof options !== "object" || Array.isArray(options)) {
    throw failure("INVALID_OPTIONS", "worker limits must be an object");
  }
  for (const key of Object.keys(options)) {
    if (!Object.hasOwn(defaults, key)) throw failure("INVALID_OPTIONS", `unknown worker limit: ${key}`);
  }
  const result = { ...defaults, ...options };
  for (const [key, min, max] of [
    ["maxPendingOperations", 1, 128],
    ["maxPendingBytes", 1, 64 * 1024 * 1024],
    ["timeoutMs", 0, 2147483647]
  ]) {
    if (!Number.isSafeInteger(result[key]) || result[key] < min || result[key] > max) {
      throw failure("INVALID_OPTIONS", `${key} must be an integer in ${min}..=${max}`);
    }
  }
  return Object.freeze(result);
}

function controlOptions(value, defaultTimeout) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw failure("INVALID_OPTIONS", "request control must be an object");
  }
  for (const key of Object.keys(value)) {
    if (key !== "signal" && key !== "timeoutMs") throw failure("INVALID_OPTIONS", `unknown request control: ${key}`);
  }
  const { signal, timeoutMs = defaultTimeout } = value;
  if (!Number.isInteger(timeoutMs) || timeoutMs < 0 || timeoutMs > 2147483647) {
    throw failure("INVALID_OPTIONS", "timeoutMs must be 0 (disabled) or a positive timer duration");
  }
  if (signal !== undefined && (signal === null || typeof signal.aborted !== "boolean"
      || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function")) {
    throw failure("INVALID_OPTIONS", "signal must be an AbortSignal");
  }
  return { signal, timeoutMs };
}

/** Owns a Worker-compatible endpoint. Internal callers must charge retained
 * argument bytes before snapshotting them in prepare(). Payload limits do not
 * claim to bound a WASM heap, a returned snapshot, or all browser memory. */
export class OwnedWorkerRpc {
  #worker;
  #limits;
  #accept;
  #active = null;
  #queue = [];
  #bytes = 0;
  #nextId = 1;
  #closed = false;
  #listeners;

  constructor(worker, options = {}, accept = () => {}) {
    this.#limits = workerLimits(options);
    if (!worker || typeof worker.postMessage !== "function" || typeof worker.terminate !== "function"
        || typeof worker.addEventListener !== "function" || typeof worker.removeEventListener !== "function") {
      throw failure("INVALID_WORKER", "workerFactory must return an owned Worker-compatible endpoint");
    }
    this.#worker = worker;
    this.#accept = accept;
    this.#listeners = {
      message: event => this.#receive(event.data),
      error: () => this.#stop(failure("WORKER_FAILED", "worker failed; this session cannot be reused")),
      messageerror: () => this.#stop(failure("WORKER_PROTOCOL_ERROR", "worker response could not be decoded"))
    };
    try {
      for (const [kind, listener] of Object.entries(this.#listeners)) {
        if (this.#closed) throw failure("WORKER_FAILED", "worker failed during initialization");
        worker.addEventListener(kind, listener);
      }
      if (this.#closed) throw failure("WORKER_FAILED", "worker failed during initialization");
    } catch {
      const error = failure("WORKER_FAILED", "could not initialize the owned worker endpoint");
      this.#stop(error);
      throw error;
    }
  }

  get closed() { return this.#closed; }
  get pendingOperations() { return this.#queue.length + Number(this.#active !== null); }
  get pendingBytes() { return this.#bytes; }

  // prepare runs synchronously only AFTER admission. It snapshots caller data;
  // transfer may therefore contain only buffers owned by that snapshot.
  request(method, retainedBytes, prepare, controls = {}) {
    try {
      if (this.#closed) throw failure("SESSION_DISPOSED", "worker session is closed; create a new session");
      const control = controlOptions(controls, this.#limits.timeoutMs);
      if (control.signal?.aborted) throw failure("ABORTED", "operation was aborted before dispatch");
      if (!Number.isSafeInteger(retainedBytes) || retainedBytes < 0) throw failure("INVALID_ARGUMENT", "invalid request byte charge");
      if (this.pendingOperations >= this.#limits.maxPendingOperations
          || retainedBytes > this.#limits.maxPendingBytes - this.#bytes) {
        throw failure("WORKER_QUEUE_FULL", "worker pending-operation or payload budget exceeded");
      }
      if (this.#nextId > Number.MAX_SAFE_INTEGER) {
        this.#stop(failure("WORKER_PROTOCOL_ERROR", "worker request identity counter exhausted"));
        throw failure("WORKER_PROTOCOL_ERROR", "worker request identity counter exhausted");
      }
      let resolve, reject;
      const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
      const entry = {
        id: this.#nextId++, method, payload: null, ready: false, bytes: retainedBytes, resolve, reject,
        signal: control.signal, onAbort: null, timer: null, done: false
      };
      entry.onAbort = () => this.#cancel(entry, "ABORTED");
      this.#bytes += retainedBytes;
      this.#queue.push(entry);
      // Reserve both budgets and FIFO position BEFORE invoking caller code.
      // A snapshot or signal adapter can reenter request(), cancel, or dispose.
      // The pump must never dispatch an entry whose snapshot is not ready yet.
      try {
        entry.signal?.addEventListener("abort", entry.onAbort, { once: true });
        if (!entry.done && entry.signal?.aborted) this.#cancel(entry, "ABORTED");
        if (!entry.done) {
          if (control.timeoutMs !== 0) entry.timer = setTimeout(() => this.#cancel(entry, "TIMEOUT"), control.timeoutMs);
          const payload = prepare();
          if (!entry.done) {
            entry.payload = payload;
            entry.ready = true;
          }
        }
      } catch (error) {
        // Settle the SAME promise that owns the reservation, not a separate
        // rejected promise with an orphan left in the queue.
        const position = this.#queue.indexOf(entry);
        if (position >= 0) this.#queue.splice(position, 1);
        this.#finish(entry, error);
      }
      this.#pump();
      return promise;
    } catch (error) {
      return Promise.reject(error);
    }
  }

  dispose() {
    this.#stop(failure("SESSION_DISPOSED", "worker session was disposed"), null, true);
  }

  #pump() {
    if (this.#closed || this.#active !== null) return;
    const entry = this.#queue[0];
    if (!entry || !entry.ready) return;
    this.#queue.shift();
    this.#active = entry;
    try {
      this.#worker.postMessage({ protocol: WORKER_PROTOCOL, id: entry.id, method: entry.method,
        args: entry.payload.args }, entry.payload.transfer ?? []);
      // The remote copy now owns its data. Keep the conservative byte charge
      // until acknowledgment, but do not retain the local argument object.
      entry.payload = null;
    } catch {
      this.#stop(failure("WORKER_SEND_FAILED", "could not dispatch operation; session state is no longer usable"), entry);
    }
  }

  #finish(entry, error, value) {
    if (entry.done) return;
    entry.done = true;
    if (entry.timer !== null) clearTimeout(entry.timer);
    this.#bytes -= entry.bytes;
    entry.payload = null;
    try { entry.signal?.removeEventListener("abort", entry.onAbort); }
    catch { /* A signal adapter cannot prevent settlement or retain capacity. */ }
    if (error) entry.reject(error); else entry.resolve(value);
  }

  #cancel(entry, code) {
    if (entry.done) return;
    if (entry === this.#active) {
      // Synchronous WASM cannot service a cancellation message while running.
      // Terminate rather than claim rollback or silently replay a mutation.
      this.#stop(failure(code, "in-flight operation cancelled; worker terminated and session lost"), entry);
      return;
    }
    const position = this.#queue.indexOf(entry);
    if (position < 0) return;
    this.#queue.splice(position, 1);
    this.#finish(entry, failure(code, "queued operation cancelled before dispatch"));
  }

  #receive(message) {
    if (this.#closed) return;
    const entry = this.#active;
    if (!entry || !message || message.protocol !== WORKER_PROTOCOL || message.id !== entry.id
        || typeof message.ok !== "boolean") {
      this.#stop(failure("WORKER_PROTOCOL_ERROR", "unexpected or out-of-order worker acknowledgment"));
      return;
    }
    if (!message.ok) {
      const detail = message.error;
      if (!detail || typeof detail.code !== "string" || !/^[A-Z0-9_]{1,64}$/.test(detail.code)
          || typeof detail.message !== "string" || detail.message.length > 4096 || typeof message.fatal !== "boolean") {
        this.#stop(failure("WORKER_PROTOCOL_ERROR", "invalid worker error acknowledgment"));
        return;
      }
      const error = failure(detail.code, detail.message);
      if (message.fatal) { this.#stop(error, entry); return; }
      this.#active = null;
      this.#finish(entry, error);
    } else {
      try { this.#accept(message.state, entry.method, message.value); }
      catch { this.#stop(failure("WORKER_PROTOCOL_ERROR", "invalid worker state acknowledgment"), entry); return; }
      this.#active = null;
      this.#finish(entry, null, message.value);
    }
    // Promise continuations can cancel undispatched queued work first.
    queueMicrotask(() => this.#pump());
  }

  #stop(reason, culprit = null, sameError = false) {
    if (this.#closed) return;
    this.#closed = true;
    for (const [kind, listener] of Object.entries(this.#listeners)) {
      try { this.#worker.removeEventListener(kind, listener); }
      catch { /* Detach other listeners and terminate even if an adapter fails. */ }
    }
    const waiting = this.#active ? [this.#active, ...this.#queue] : this.#queue;
    this.#active = null;
    this.#queue = [];
    for (const entry of waiting) {
      const error = sameError || culprit === null || culprit === entry ? reason
        : failure("SESSION_LOST", "another operation terminated the worker; queued operations were not replayed");
      this.#finish(entry, error);
    }
    try {
      const terminated = this.#worker.terminate();
      // Browser Worker returns void; Node adapters may return a Promise.
      if (terminated && typeof terminated.catch === "function") terminated.catch(() => {});
    } catch { /* All promises are already settled and the endpoint is closed. */ }
  }
}

// Only a bounded, recognized diagnostic is evidence of a recoverable
// application error. Anything else may follow an incomplete remote mutation.
function workerDiagnostic(error) {
  const fallback = { code: "WORKER_OPERATION_FAILED", message: "worker operation failed", fatal: true };
  try {
    // Engine/host exceptions are not structured-cloned yet. Read accessors
    // once, and fail closed if describing an exception itself throws.
    const code = error?.code;
    const message = error?.message;
    const fatal = error?.fatal;
    const recognized = typeof code === "string" && /^[A-Z0-9_]{1,64}$/.test(code);
    return { code: recognized ? code : fallback.code,
      message: typeof message === "string" ? message.slice(0, 4096) : fallback.message,
      fatal: !recognized || fatal === true };
  } catch { return fallback; }
}

/** Install on a dedicated-worker endpoint. No method lookup/eval/import is
 * driven by message contents: dispatch is an explicit application callback.
 * The supported client sends one request at a time, so no remote queue grows. */
export function serveOwnedWorker(endpoint, dispatch) {
  let busy = false;
  let closed = false;
  let previousId = 0;
  const sendError = (id, error, forceFatal = false) => {
    const detail = workerDiagnostic(error);
    const fatal = forceFatal || detail.fatal;
    if (fatal) closed = true;
    const { code, message } = detail;
    try { endpoint.postMessage({ protocol: WORKER_PROTOCOL, id, ok: false, fatal, error: { code, message } }); }
    catch { closed = true; }
  };
  const receive = async event => {
    if (closed) return;
    const request = event.data;
    const valid = request && request.protocol === WORKER_PROTOCOL && Number.isSafeInteger(request.id)
      && request.id > previousId && typeof request.method === "string" && request.method.length <= 64
      && Array.isArray(request.args) && request.args.length <= 8;
    if (!valid || busy) {
      closed = true;
      sendError(Number.isSafeInteger(request?.id) ? request.id : 0,
        failure("WORKER_PROTOCOL_ERROR", "invalid, overlapping, or replayed worker request"), true);
      return;
    }
    previousId = request.id;
    busy = true;
    let result;
    try {
      result = await dispatch(request.method, request.args);
    } catch (error) {
      // Disposal or a protocol violation may close the endpoint while the
      // dispatcher awaits. Never publish a second terminal acknowledgment.
      if (closed) return;
      sendError(request.id, error);
      busy = false;
      return;
    }
    if (closed) return;
    try {
      endpoint.postMessage({ protocol: WORKER_PROTOCOL, id: request.id, ok: true,
        value: result.value, state: result.state }, result.transfer ?? []);
    } catch {
      closed = true;
      sendError(request.id, failure("WORKER_PROTOCOL_ERROR", "worker could not publish the completed operation"), true);
    }
    busy = false;
  };
  endpoint.addEventListener("message", receive);
  return () => { closed = true; endpoint.removeEventListener("message", receive); };
}
