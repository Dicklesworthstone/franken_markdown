import { sourceText } from "./flow_session.mjs";
import { OwnedWorkerRpc, FlowWorkerError, workerLimits, serveOwnedWorker } from "./worker_transport.mjs";
import { fields, flowToken, normalizeFlowRequest, requestWeight, snapshotArguments, acknowledgedState } from "./flow_worker_protocol.mjs";

const convert = error => error instanceof FlowWorkerError ? error
  : new FlowWorkerError(typeof error?.code === "string" ? error.code : "WORKER_OPERATION_FAILED",
    error instanceof Error ? error.message : "flow worker operation failed", { cause: error });

// The factory is code supplied by the importing host, never a string received
// from a worker message. Public entrypoint supplies the dedicated module URL.
export async function createWorkerFlowSessionWith(factory, source, options = {}, runtime = {}) {
  let rpc;
  let worker;
  try {
    fields(runtime, ["signal", "startupTimeoutMs", "timeoutMs", "maxPendingOperations", "maxPendingBytes"], "worker options");
    const configured = {};
    for (const key of ["timeoutMs", "maxPendingOperations", "maxPendingBytes"]) {
      if (runtime[key] !== undefined) configured[key] = runtime[key];
    }
    const limits = workerLimits(configured);
    const startupTimeout = runtime.startupTimeoutMs ?? 60000;
    if (!Number.isInteger(startupTimeout) || startupTimeout < 0 || startupTimeout > 2147483647) {
      throw new FlowWorkerError("INVALID_OPTIONS", "invalid startupTimeoutMs");
    }
    if (runtime.signal !== undefined && (runtime.signal === null || typeof runtime.signal.aborted !== "boolean"
        || typeof runtime.signal.addEventListener !== "function" || typeof runtime.signal.removeEventListener !== "function")) {
      throw new FlowWorkerError("INVALID_OPTIONS", "signal must be an AbortSignal");
    }
    if (runtime.signal?.aborted) throw new FlowWorkerError("ABORTED", "worker creation was aborted");
    const initial = normalizeFlowRequest("create", [source, options]);
    if (requestWeight(initial) > limits.maxPendingBytes) throw new FlowWorkerError("WORKER_QUEUE_FULL", "source exceeds worker ingress budget");
    if (typeof factory !== "function") throw new FlowWorkerError("INVALID_WORKER", "workerFactory must be a function");
    let state = null;
    worker = factory();
    rpc = new OwnedWorkerRpc(worker, limits, (value, method, result) => {
      const next = acknowledgedState(value);
      if (state && (BigInt(next.token.revision) < BigInt(state.token.revision)
          || BigInt(next.token.layoutRevision) < BigInt(state.token.layoutRevision))) {
        throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "worker revisions moved backwards");
      }
      if (["edit", "editBytes", "replaceSource", "reflow", "provideAsset", "reloadAssets"].includes(method)) {
        const token = flowToken(result);
        if (token.revision !== next.token.revision || token.layoutRevision !== next.token.layoutRevision) {
          throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "mutation acknowledgment does not match worker state");
        }
      }
      if (["snapshot", "readingOrder", "pendingAssets", "hitTest", "selectText"].includes(method)) {
        const token = flowToken(result);
        if (result.schemaVersion !== 1 || token.revision !== next.token.revision
            || token.layoutRevision !== next.token.layoutRevision) {
          throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "snapshot acknowledgment has inconsistent revisions");
        }
        const key = { snapshot: "items", readingOrder: "nodes", pendingAssets: "requests" }[method];
        if (key) {
          const end = result.offset + result[key]?.length;
          if (!Number.isInteger(result.offset) || result.offset < 0 || !Number.isInteger(result.total)
              || result.total < result.offset || !Array.isArray(result[key]) || result[key].length > 2048
              || end > result.total || (end < result.total && end === result.offset)
              || result.nextOffset !== (end < result.total ? end : null)) {
            throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "snapshot page did not advance consistently");
          }
        }
      }
      if (method === "getSource") sourceText(result);
      state = next;
    });
    await rpc.request("create", requestWeight(initial), () => ({ args: initial }),
      { signal: runtime.signal, timeoutMs: startupTimeout });
    const alive = () => { if (rpc.closed) throw new FlowWorkerError("SESSION_DISPOSED", "worker session is closed"); };
    const call = (method, args, controls) => {
      try {
        alive();
        const normalized = normalizeFlowRequest(method, args);
        return rpc.request(method, requestWeight(normalized), () => snapshotArguments(method, normalized), controls);
      } catch (error) { return Promise.reject(convert(error)); }
    };
    const api = {
      get disposed() { return rpc.closed; },
      // These are the last ACKNOWLEDGED values. Await mutations before reading
      // their new token; no speculative source or revision is published locally.
      get token() { alive(); return { ...state.token }; },
      get revision() { alive(); return state.token.revision; },
      get layoutRevision() { alive(); return state.token.layoutRevision; },
      get layoutOptions() { alive(); return { ...state.layout }; },
      get pendingOperations() { return rpc.pendingOperations; },
      get pendingBytes() { return rpc.pendingBytes; },
      getSource(control) { return call("getSource", [], control); },
      edit(start, end, replacement, options, control) { return call("edit", [start, end, replacement, options], control); },
      editBytes(start, end, replacement, options, control) { return call("editBytes", [start, end, replacement, options], control); },
      replaceSource(source, options, control) { return call("replaceSource", [source, options], control); },
      reflow(options, token, control) { return call("reflow", [options, token], control); },
      provideAsset(result, control) { return call("provideAsset", [result], control); },
      reloadAssets(revision, control) { return call("reloadAssets", [revision], control); },
      snapshot(options, control) { return call("snapshot", [options], control); },
      readingOrder(options, control) { return call("readingOrder", [options], control); },
      pendingAssets(options, control) { return call("pendingAssets", [options], control); },
      hitTest(x, y, token, control) { return call("hitTest", [x, y, token], control); },
      selectText(index, start, end, token, control) { return call("selectText", [index, start, end, token], control); },
      copySource(start, end, revision, control) { return call("copySource", [start, end, revision], control); },
      fontBytes(id, control) { return call("fontBytes", [id], control); },
      assetBytes(id, revision, control) { return call("assetBytes", [id, revision], control); },
      pages(options = {}, control) {
        alive(); fields(options, ["limit", "glyphs", "token"], "iterator options");
        // Capture at iterator creation, not lazily inside the async generator.
        const normalized = normalizeFlowRequest("snapshot", [{ ...options, token: options.token ?? state.token }])[0];
        return (async function* () {
          let offset = 0;
          do {
            const page = await call("snapshot", [{ ...normalized, offset }], control);
            yield page;
            offset = page.nextOffset;
          } while (offset !== null);
        })();
      },
      dispose() { rpc.dispose(); }
    };
    return Object.freeze(api);
  } catch (error) {
    if (rpc) rpc.dispose();
    else if (worker && typeof worker.terminate === "function") {
      try { const result = worker.terminate(); result?.catch?.(() => {}); } catch { /* already failed */ }
    }
    throw convert(error);
  }
}

/** Fixed worker-side allowlist over the existing synchronous flow facade. */
export function installFlowWorker(endpoint, createSession) {
  let session = null;
  const stop = serveOwnedWorker(endpoint, async (method, args) => {
    try {
      const normalized = normalizeFlowRequest(method, args);
      let value;
      if (method === "create") {
        if (session) throw new FlowWorkerError("SESSION_EXISTS", "worker already owns a flow session");
        session = await createSession(...normalized);
        value = null;
      } else {
        if (!session) throw new FlowWorkerError("SESSION_NOT_READY", "create the flow session first");
        // normalizeFlowRequest is an own-key allowlist. Neither constructors,
        // arbitrary property paths, eval, imports nor dispose are remotely callable.
        value = method === "getSource" ? session.source : session[method](...normalized);
      }
      const state = { token: session.token, layout: session.layoutOptions };
      let transfer = [];
      if (value instanceof Uint8Array) {
        // The facade returns owned bytes. Copy again at this ownership boundary
        // so an alternative backend cannot accidentally detach its own storage.
        value = new Uint8Array(value);
        transfer = [value.buffer];
      }
      return { value, state, transfer };
    } catch (error) {
      const converted = convert(error);
      if (method === "create" || ["WASM_ERROR", "INVALID_WASM_RESPONSE", "WORKER_OPERATION_FAILED"].includes(converted.code)) {
        converted.fatal = true;
      }
      throw converted;
    }
  });
  return () => { stop(); session?.dispose(); session = null; };
}
