import { validateFlowExportResult } from "./flow_export.mjs";
import { sourceText, validateViewportPage } from "./flow_session.mjs";
import {
  acknowledgedState,
  fields,
  flowToken,
  normalizeFlowRequest,
  requestWeight,
  snapshotArguments,
} from "./flow_worker_protocol.mjs";
import {
  FlowWorkerError,
  OwnedWorkerRpc,
  SnapshotDeltaDecoder,
  SnapshotDeltaEncoder,
  serveOwnedWorker,
  workerLimits,
} from "./worker_transport.mjs";

const convert = (error) =>
  error instanceof FlowWorkerError
    ? error
    : new FlowWorkerError(
        typeof error?.code === "string" ? error.code : "WORKER_OPERATION_FAILED",
        error instanceof Error ? error.message : "flow worker operation failed",
        { cause: error },
      );

// The factory is code supplied by the importing host, never a string received
// from a worker message. Public entrypoint supplies the dedicated module URL.
export async function createWorkerFlowSessionWith(factory, source, options = {}, runtime = {}) {
  let rpc;
  let worker;
  const snapshotDeltas = new SnapshotDeltaDecoder();
  try {
    fields(
      runtime,
      ["signal", "startupTimeoutMs", "timeoutMs", "maxPendingOperations", "maxPendingBytes",
        "initialCodeHighlighting"],
      "worker options",
    );
    const initialCodeHighlighting = runtime.initialCodeHighlighting;
    if (initialCodeHighlighting !== undefined && typeof initialCodeHighlighting !== "boolean") {
      throw new FlowWorkerError("INVALID_OPTIONS", "initialCodeHighlighting must be boolean");
    }
    const configured = {};
    for (const key of ["timeoutMs", "maxPendingOperations", "maxPendingBytes"]) {
      if (runtime[key] !== undefined) configured[key] = runtime[key];
    }
    const limits = workerLimits(configured);
    const startupTimeout = runtime.startupTimeoutMs ?? 60000;
    if (!Number.isInteger(startupTimeout) || startupTimeout < 0 || startupTimeout > 2147483647) {
      throw new FlowWorkerError("INVALID_OPTIONS", "invalid startupTimeoutMs");
    }
    if (
      runtime.signal !== undefined &&
      (runtime.signal === null ||
        typeof runtime.signal.aborted !== "boolean" ||
        typeof runtime.signal.addEventListener !== "function" ||
        typeof runtime.signal.removeEventListener !== "function")
    ) {
      throw new FlowWorkerError("INVALID_OPTIONS", "signal must be an AbortSignal");
    }
    if (runtime.signal?.aborted)
      throw new FlowWorkerError("ABORTED", "worker creation was aborted");
    const initial = normalizeFlowRequest("create", [source, options]);
    if (requestWeight(initial) > limits.maxPendingBytes)
      throw new FlowWorkerError("WORKER_QUEUE_FULL", "source exceeds worker ingress budget");
    if (typeof factory !== "function")
      throw new FlowWorkerError("INVALID_WORKER", "workerFactory must be a function");
    let state = null;
    let supportsSnapshotDeltas = false;
    let supportsViewportDeltas = false;
    let supportsViewport = false;
    let supportsAssetBatches = false;
    let supportsEditBatches = false;
    let supportsCodeHighlighting = false;
    let codeHighlighting = false;
    // One monotonic startup budget covers creation AND optional syntax setup.
    // Do not publish the session while its requested initial mode is still pending.
    const startupDeadline = startupTimeout === 0 ? null : performance.now() + startupTimeout;
    const startupControl = () => {
      const remaining = startupDeadline === null ? 0 : Math.ceil(startupDeadline - performance.now());
      // Zero disables the RPC deadline; an exhausted finite budget must NOT
      // accidentally become unlimited or receive a fresh startup timeout.
      if (startupDeadline !== null && remaining <= 0)
        throw new FlowWorkerError("TIMEOUT", "worker startup budget exhausted");
      return { signal: runtime.signal, timeoutMs: remaining };
    };
    worker = factory();
    rpc = new OwnedWorkerRpc(worker, limits, (value, method, result) => {
      const next = acknowledgedState(value);
      if (method === "snapshotDelta" || method === "viewportDelta") {
        const page = snapshotDeltas.decode(result);
        // OwnedWorkerRpc validates before resolving this same message value.
        // Define own data properties, never invoke a __proto__ setter.
        for (const key of Object.keys(result)) delete result[key];
        Object.defineProperties(result, Object.getOwnPropertyDescriptors(page));
        method = method === "snapshotDelta" ? "snapshot" : "viewport";
      }
      if (method === "create") {
        // Legacy workers returned null. Capabilities travel in the creation
        // VALUE, not state, so older clients retain their strict state schema.
        if (result !== null && (!result || typeof result.supportsViewport !== "boolean")) {
          throw new FlowWorkerError(
            "WORKER_PROTOCOL_ERROR",
            "invalid worker capability acknowledgment",
          );
        }
        if (
          result?.supportsAssetBatches !== undefined &&
          typeof result.supportsAssetBatches !== "boolean"
        ) {
          throw new FlowWorkerError(
            "WORKER_PROTOCOL_ERROR",
            "invalid asset-batch capability acknowledgment",
          );
        }
        if (
          result?.supportsSnapshotDeltas !== undefined &&
          typeof result.supportsSnapshotDeltas !== "boolean"
        ) {
          throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "invalid snapshot-delta capability");
        }
        if (
          result?.supportsViewportDeltas !== undefined &&
          typeof result.supportsViewportDeltas !== "boolean"
        ) {
          throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "invalid viewport-delta capability");
        }
        if (
          result?.supportsEditBatches !== undefined &&
          typeof result.supportsEditBatches !== "boolean"
        ) {
          throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "invalid edit-batch capability");
        }
        if (
          (result?.supportsCodeHighlighting !== undefined &&
            typeof result.supportsCodeHighlighting !== "boolean") ||
          (result?.supportsCodeHighlighting === true
            ? typeof result.codeHighlighting !== "boolean"
            : result?.codeHighlighting !== undefined && result.codeHighlighting !== false)
        ) {
          throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "invalid syntax-mode capability");
        }
        supportsCodeHighlighting = result?.supportsCodeHighlighting === true;
        codeHighlighting = supportsCodeHighlighting && result.codeHighlighting;
        supportsEditBatches = result?.supportsEditBatches === true;
        supportsViewportDeltas = result?.supportsViewportDeltas === true;
        supportsSnapshotDeltas = result?.supportsSnapshotDeltas === true;
        supportsAssetBatches = result?.supportsAssetBatches === true;
        supportsViewport = result?.supportsViewport === true;
      }
      if (
        state &&
        (BigInt(next.token.revision) < BigInt(state.token.revision) ||
          BigInt(next.token.layoutRevision) < BigInt(state.token.layoutRevision))
      ) {
        throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "worker revisions moved backwards");
      }
      if (
        [
          "edit",
          "editBytes",
          "editMany",
          "replaceSource",
          "reflow",
          "setCodeHighlighting",
          "provideAsset",
          "provideAssets",
          "reloadAssets",
        ].includes(method)
      ) {
        const token = flowToken(result);
        if (
          token.revision !== next.token.revision ||
          token.layoutRevision !== next.token.layoutRevision
        ) {
          throw new FlowWorkerError(
            "WORKER_PROTOCOL_ERROR",
            "mutation acknowledgment does not match worker state",
          );
        }
      }
      if (
        ["snapshot", "viewport", "readingOrder", "pendingAssets", "hitTest", "selectText"].includes(
          method,
        )
      ) {
        const token = flowToken(result);
        if (
          result.schemaVersion !== 1 ||
          token.revision !== next.token.revision ||
          token.layoutRevision !== next.token.layoutRevision
        ) {
          throw new FlowWorkerError(
            "WORKER_PROTOCOL_ERROR",
            "snapshot acknowledgment has inconsistent revisions",
          );
        }
        const key = { snapshot: "items", readingOrder: "nodes", pendingAssets: "requests" }[method];
        if (key) {
          const end = result.offset + result[key]?.length;
          if (
            !Number.isInteger(result.offset) ||
            result.offset < 0 ||
            !Number.isInteger(result.total) ||
            result.total < result.offset ||
            !Array.isArray(result[key]) ||
            result[key].length > 2048 ||
            end > result.total ||
            (end < result.total && end === result.offset) ||
            result.nextOffset !== (end < result.total ? end : null)
          ) {
            throw new FlowWorkerError(
              "WORKER_PROTOCOL_ERROR",
              "snapshot page did not advance consistently",
            );
          }
        }
      }
      if (method === "setCodeHighlighting") {
        const previous = flowToken(result.previousToken);
        if (
          !supportsCodeHighlighting || typeof result.codeHighlighting !== "boolean" ||
          !state || previous.revision !== state.token.revision ||
          previous.layoutRevision !== state.token.layoutRevision ||
          next.token.revision !== previous.revision ||
          BigInt(next.token.layoutRevision) !== BigInt(previous.layoutRevision) +
            BigInt(codeHighlighting !== result.codeHighlighting) ||
          Object.keys(state.layout).some((key) => state.layout[key] !== next.layout[key])
        ) {
          throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "inconsistent syntax-mode acknowledgment");
        }
        codeHighlighting = result.codeHighlighting;
      }
      if (method === "exportDocument") validateFlowExportResult(result, next.token);
      if (method === "getSource") sourceText(result);
      state = next;
    });
    await rpc.request("create", requestWeight(initial), () => ({ args: initial }), startupControl());
    const alive = () => {
      if (rpc.closed) throw new FlowWorkerError("SESSION_DISPOSED", "worker session is closed");
    };
    const call = (method, args, controls) => {
      try {
        alive();
        if (method === "viewport" && !supportsViewport) {
          throw new FlowWorkerError(
            "UNSUPPORTED_WASM_PACKAGE",
            "this worker does not expose indexed viewport queries",
          );
        }
        if (method === "provideAssets" && !supportsAssetBatches) {
          throw new FlowWorkerError(
            "UNSUPPORTED_WASM_PACKAGE",
            "this worker does not expose atomic asset batches",
          );
        }
        if (method === "editMany" && !supportsEditBatches) {
          throw new FlowWorkerError(
            "UNSUPPORTED_WASM_PACKAGE",
            "this worker does not expose atomic source edit batches",
          );
        }
        if (method === "setCodeHighlighting" && !supportsCodeHighlighting) {
          throw new FlowWorkerError(
            "UNSUPPORTED_WASM_PACKAGE",
            "this worker does not expose persistent code highlighting",
          );
        }
        const normalized = normalizeFlowRequest(method, args);
        // Capture an omitted query token before enqueueing, not when the worker
        // eventually reads it behind an edit or reflow.
        if (method === "viewport" && normalized[0].token === undefined)
          normalized[0].token = { ...state.token };
        // Negotiate each private wire method; old workers receive full pages.
        // Capture the acknowledged baseline at enqueue. A superseded baseline
        // costs a full reply, never an implicit rebase or an incorrect page.
        const wireMethod = method === "snapshot" && supportsSnapshotDeltas ? "snapshotDelta"
          : method === "viewport" && supportsViewportDeltas ? "viewportDelta" : method;
        const wireArgs = wireMethod !== method
          ? [normalized[0], snapshotDeltas.baseId] : normalized;
        const pending = rpc.request(
          wireMethod,
          requestWeight(wireArgs),
          () => snapshotArguments(wireMethod, wireArgs),
          controls,
        );
        if (method === "setCodeHighlighting")
          return pending.then((result) => {
            // Promise continuations run before the RPC pumps another request.
            // A mismatched reply must close the session, not dispatch queued edits.
            if (
              result.codeHighlighting !== normalized[0] ||
              result.previousToken.revision !== normalized[1].revision ||
              result.previousToken.layoutRevision !== normalized[1].layoutRevision
            ) {
              rpc.dispose();
              throw new FlowWorkerError("WORKER_PROTOCOL_ERROR", "syntax reply mismatched its request");
            }
            // Mode/previous-token metadata is private to this wire method.
            return flowToken(result);
          });
        if (method === "viewport")
          return pending.then((result) => {
            try {
              return validateViewportPage(result, normalized[0], normalized[0].token);
            } catch (error) {
              rpc.dispose();
              throw new FlowWorkerError(
                "WORKER_PROTOCOL_ERROR",
                "viewport acknowledgment does not match its request",
                { cause: error },
              );
            }
          });
        if (method !== "exportDocument") return pending;
        return pending.then((result) => {
          try {
            validateFlowExportResult(result, normalized[2], normalized[1].maxOutputBytes);
            if (result.format !== normalized[0]) throw new Error("unexpected export format");
            return result;
          } catch {
            rpc.dispose();
            throw new FlowWorkerError(
              "WORKER_PROTOCOL_ERROR",
              "export acknowledgment does not match the requested document",
            );
          }
        });
      } catch (error) {
        return Promise.reject(convert(error));
      }
    };
    const api = {
      get disposed() {
        return rpc.closed;
      },
      get supportsViewport() {
        alive();
        return supportsViewport;
      },
      get supportsAssetBatches() {
        alive();
        return supportsAssetBatches;
      },
      get supportsEditBatches() {
        alive();
        return supportsEditBatches;
      },
      get supportsCodeHighlighting() {
        alive();
        return supportsCodeHighlighting;
      },
      get codeHighlighting() {
        alive();
        return codeHighlighting;
      },
      // These are the last ACKNOWLEDGED values. Await mutations before reading
      // their new token; no speculative source or revision is published locally.
      get token() {
        alive();
        return { ...state.token };
      },
      get revision() {
        alive();
        return state.token.revision;
      },
      get layoutRevision() {
        alive();
        return state.token.layoutRevision;
      },
      get layoutOptions() {
        alive();
        return { ...state.layout };
      },
      get pendingOperations() {
        return rpc.pendingOperations;
      },
      get pendingBytes() {
        return rpc.pendingBytes;
      },
      getSource(control) {
        return call("getSource", [], control);
      },
      exportDocument(format, options, token, control) {
        return call("exportDocument", [format, options, token], control);
      },
      edit(start, end, replacement, options, control) {
        return call("edit", [start, end, replacement, options], control);
      },
      editBytes(start, end, replacement, options, control) {
        return call("editBytes", [start, end, replacement, options], control);
      },
      editMany(edits, options, control) {
        return call("editMany", [edits, options], control);
      },
      replaceSource(source, options, control) {
        return call("replaceSource", [source, options], control);
      },
      setCodeHighlighting(enabled, token, control) {
        return call("setCodeHighlighting", [enabled, token], control);
      },
      reflow(options, token, control) {
        return call("reflow", [options, token], control);
      },
      provideAsset(result, control) {
        return call("provideAsset", [result], control);
      },
      provideAssets(results, control) {
        return call("provideAssets", [results], control);
      },
      reloadAssets(revision, control) {
        return call("reloadAssets", [revision], control);
      },
      snapshot(options, control) {
        return call("snapshot", [options], control);
      },
      viewport(options, control) {
        return call("viewport", [options], control);
      },
      readingOrder(options, control) {
        return call("readingOrder", [options], control);
      },
      pendingAssets(options, control) {
        return call("pendingAssets", [options], control);
      },
      hitTest(x, y, token, control) {
        return call("hitTest", [x, y, token], control);
      },
      selectText(index, start, end, token, control) {
        return call("selectText", [index, start, end, token], control);
      },
      copySource(start, end, revision, control) {
        return call("copySource", [start, end, revision], control);
      },
      fontBytes(id, control) {
        return call("fontBytes", [id], control);
      },
      glyphOutlines(id, glyphIds, control) {
        return call("glyphOutlines", [id, glyphIds], control);
      },
      assetBytes(id, revision, control) {
        return call("assetBytes", [id, revision], control);
      },
      pages(options = {}, control) {
        alive();
        fields(options, ["limit", "glyphs", "token"], "iterator options");
        // Capture at iterator creation, not lazily inside the async generator.
        const normalized = normalizeFlowRequest("snapshot", [
          { ...options, token: options.token ?? state.token },
        ])[0];
        return (async function* () {
          let offset = 0;
          do {
            const page = await call("snapshot", [{ ...normalized, offset }], control);
            yield page;
            offset = page.nextOffset;
          } while (offset !== null);
        })();
      },
      dispose() {
        snapshotDeltas.clear();
        rpc.dispose();
      },
    };
    if (initialCodeHighlighting !== undefined && initialCodeHighlighting !== codeHighlighting) {
      await api.setCodeHighlighting(initialCodeHighlighting, api.token, startupControl());
    }
    if (runtime.signal?.aborted)
      throw new FlowWorkerError("ABORTED", "worker creation was aborted");
    startupControl(); // Also fence a late create acknowledgment without a mode change.
    return Object.freeze(api);
  } catch (error) {
    snapshotDeltas.clear();
    if (rpc) rpc.dispose();
    else if (worker && typeof worker.terminate === "function") {
      try {
        const result = worker.terminate();
        result?.catch?.(() => {});
      } catch {
        /* already failed */
      }
    }
    throw convert(error);
  }
}

/** Fixed worker-side allowlist over the existing flow facade. Exports are awaited
 * before acknowledging state or admitting another operation. */
export function installFlowWorker(endpoint, createSession) {
  let session = null;
  const snapshotDeltas = new SnapshotDeltaEncoder();
  const stop = serveOwnedWorker(endpoint, async (method, args) => {
    try {
      const pageMethod = method === "snapshotDelta" ? "snapshot"
        : method === "viewportDelta" ? "viewport" : null;
      const delta = pageMethod !== null;
      if (delta && (
        !Array.isArray(args) || args.length !== 2 ||
        (args[1] !== null && (!Number.isSafeInteger(args[1]) || args[1] <= 0))
      )) {
        throw new FlowWorkerError("INVALID_ARGUMENT", "invalid snapshot delta request");
      }
      // Private transport methods use the unchanged public page validators.
      // Their baseline is not an option passed into the native core.
      const normalized = normalizeFlowRequest(pageMethod ?? method, delta ? [args[0]] : args);
      let value;
      if (method === "create") {
        if (session)
          throw new FlowWorkerError("SESSION_EXISTS", "worker already owns a flow session");
        session = await createSession(...normalized);
        const highlighting = session.supportsCodeHighlighting === true &&
          typeof session.setCodeHighlighting === "function";
        if (highlighting && typeof session.codeHighlighting !== "boolean")
          throw new FlowWorkerError("INVALID_WASM_RESPONSE", "native syntax mode must be boolean");
        value = {
          supportsCodeHighlighting: highlighting,
          codeHighlighting: highlighting ? session.codeHighlighting : false,
          supportsViewport: session.supportsViewport === true,
          supportsAssetBatches: session.supportsAssetBatches === true,
          supportsEditBatches: session.supportsEditBatches === true && typeof session.editMany === "function",
          supportsSnapshotDeltas: true,
          supportsViewportDeltas: session.supportsViewport === true,
        };
      } else {
        if (!session)
          throw new FlowWorkerError("SESSION_NOT_READY", "create the flow session first");
        if (method === "provideAssets" && session.supportsAssetBatches !== true) {
          throw new FlowWorkerError(
            "UNSUPPORTED_WASM_PACKAGE",
            "this native session does not support atomic asset batches",
          );
        }
        if (method === "editMany" && (
          session.supportsEditBatches !== true || typeof session.editMany !== "function"
        )) {
          throw new FlowWorkerError(
            "UNSUPPORTED_WASM_PACKAGE",
            "this native session does not support atomic source edit batches",
          );
        }
        if (method === "setCodeHighlighting" && (
          session.supportsCodeHighlighting !== true ||
          typeof session.setCodeHighlighting !== "function"
        )) {
          throw new FlowWorkerError("UNSUPPORTED_WASM_PACKAGE", "native code highlighting unavailable");
        }
        // normalizeFlowRequest is an own-key allowlist. Neither constructors,
        // arbitrary property paths, eval, imports nor dispose are remotely callable.
        if (method === "setCodeHighlighting") {
          const previousToken = flowToken(session.token);
          const before = session.codeHighlighting;
          const token = await session.setCodeHighlighting(...normalized);
          if (
            typeof before !== "boolean" || session.codeHighlighting !== normalized[0] ||
            previousToken.revision !== normalized[1].revision ||
            previousToken.layoutRevision !== normalized[1].layoutRevision ||
            token.revision !== previousToken.revision ||
            BigInt(token.layoutRevision) !== BigInt(previousToken.layoutRevision) +
              BigInt(before !== normalized[0])
          ) {
            throw new FlowWorkerError("INVALID_WASM_RESPONSE", "native syntax transition is inconsistent");
          }
          value = { ...token, previousToken, codeHighlighting: session.codeHighlighting };
        } else if (delta) {
          if (pageMethod === "viewport" && session.supportsViewport !== true) {
            throw new FlowWorkerError("UNSUPPORTED_WASM_PACKAGE", "indexed viewport unavailable");
          }
          const page = await session[pageMethod](normalized[0]);
          value = snapshotDeltas.encode(page, args[1]);
        } else {
          value = method === "getSource" ? session.source : await session[method](...normalized);
        }
      }
      const state = { token: session.token, layout: session.layoutOptions };
      let transfer = [];
      if (method === "exportDocument") {
        validateFlowExportResult(value, state.token, normalized[1].maxOutputBytes);
        if (
          value.format !== normalized[0] ||
          value.revision !== normalized[2].revision ||
          value.layoutRevision !== normalized[2].layoutRevision
        )
          throw new FlowWorkerError(
            "INVALID_WASM_RESPONSE",
            "export result mismatched the request",
          );
        // Copy a nested result buffer too: never detach alternative backend storage.
        value = { ...value, bytes: new Uint8Array(value.bytes) };
        transfer = [value.bytes.buffer];
      } else if (value instanceof Uint8Array) {
        // The facade returns owned bytes. Copy again at this ownership boundary
        // so an alternative backend cannot accidentally detach its own storage.
        value = new Uint8Array(value);
        transfer = [value.buffer];
      }
      return { value, state, transfer };
    } catch (error) {
      const converted = convert(error);
      if (
        method === "create" ||
        ["WASM_ERROR", "INVALID_WASM_RESPONSE", "WORKER_OPERATION_FAILED", "WORKER_PROTOCOL_ERROR"].includes(converted.code)
      ) {
        converted.fatal = true;
      }
      throw converted;
    }
  });
  return () => {
    stop();
    snapshotDeltas.clear();
    session?.dispose();
    session = null;
  };
}
