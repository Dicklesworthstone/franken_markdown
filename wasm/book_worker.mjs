// Worker transport only. The entrypoint injects the real Rust/WASM book API.
import { bookLinkOptions, prepareBookInput } from "./book_session.mjs";

const LIMIT = 128 * 1024 * 1024;
const formats = Object.freeze({
  pdf: ["renderBookPdf", "application/pdf", "pdf"],
  epub: ["renderBookEpub", "application/epub+zip", "epub"],
  site: ["renderBookSite", "application/zip", "zip"],
  preview: ["renderBookPreview", "application/json", "json"],
  inspection: ["inspectBook", "application/json", "json"],
  links: ["checkBookLinks", "application/json", "json"],
});
export const bookError = (code, message) => Object.assign(new Error(message), { code });
function formatInfo(format) {
  if (!Object.hasOwn(formats, format))
    throw bookError("INVALID_FORMAT", "Choose pdf, epub, site, preview, inspection or links.");
  return formats[format];
}
function checkedOutput(bytes, sourceLength, maximum) {
  if (
    !(bytes instanceof Uint8Array) ||
    Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]" ||
    !Number.isSafeInteger(sourceLength) ||
    sourceLength < 0 ||
    sourceLength > 64 * 1024 * 1024
  ) {
    throw bookError("WORKER_PROTOCOL_ERROR", "Invalid book output envelope.");
  }
  if (bytes.byteLength === 0 || bytes.byteLength > maximum)
    throw bookError("OUTPUT_LIMIT", "Book output is empty or exceeds the output byte limit.");
}
function diagnostic(error) {
  // Rust bindings may throw structured JSON strings; preserve the reason code,
  // not an unbounded engine dump or a raw source document.
  if (typeof error === "string" && error.length <= 4096) {
    try {
      error = JSON.parse(error);
    } catch {
      error = null;
    }
  }
  try {
    const code = error?.code,
      message = error?.message;
    return {
      code: typeof code === "string" ? code.slice(0, 80) : "BOOK_ERROR",
      message: typeof message === "string" ? message.slice(0, 2048) : "Book rendering failed.",
    };
  } catch {
    return { code: "BOOK_ERROR", message: "Book rendering failed." };
  }
}

/** One worker per export. Cancellation terminates synchronous Rust work, not
 * merely its Promise. Optional preview retention keeps only one idle worker;
 * publication exports keep the original one-job lifetime. Nothing is queued.
 */
export function createBookWorkerClient({
  workerFactory,
  timeoutMs = 120000,
  maxOutputBytes = LIMIT,
  retainPreview = false,
  idleTimeoutMs = 30000,
} = {}) {
  if (
    typeof workerFactory !== "function" ||
    !Number.isInteger(timeoutMs) ||
    timeoutMs < 1 ||
    timeoutMs > 600000 ||
    !Number.isInteger(maxOutputBytes) ||
    maxOutputBytes < 1 ||
    maxOutputBytes > LIMIT ||
    typeof retainPreview !== "boolean" ||
    !Number.isInteger(idleTimeoutMs) || idleTimeoutMs < 1 || idleTimeoutMs > 600000
  ) {
    throw bookError("INVALID_OPTIONS", "Invalid worker factory, timeout or output limit.");
  }
  let disposed = false,
    active = null,
    idle = null,
    serial = 0;
  const alive = () => {
    if (disposed) throw bookError("SESSION_DISPOSED", "Book worker is disposed.");
  };
  function terminate(worker) {
    try {
      const result = worker?.terminate();
      if (result && typeof result.catch === "function") result.catch(() => {});
    } catch { /* Cleanup cannot prevent settlement. */ }
  }
  function takeIdle() {
    const entry = idle;
    if (!entry) return null;
    idle = null;
    clearTimeout(entry.timer);
    for (const kind of ["message", "error", "messageerror"]) {
      try { entry.worker.removeEventListener(kind, entry.failed); }
      catch { terminate(entry.worker); return null; }
    }
    return entry.worker;
  }
  function releaseIdle() { terminate(takeIdle()); }
  function keepIdle(worker) {
    if (disposed || active || idle) { terminate(worker); return; }
    const entry = { worker, timer: null, failed: null };
    entry.failed = () => { if (idle === entry) releaseIdle(); };
    idle = entry;
    try {
      entry.timer = setTimeout(entry.failed, idleTimeoutMs);
      // Do not keep a Node embedding alive solely to expire a warm cache.
      entry.timer?.unref?.();
      for (const kind of ["message", "error", "messageerror"]) {
        worker.addEventListener(kind, entry.failed);
        if (idle !== entry) return;
      }
    } catch { if (idle === entry) releaseIdle(); }
  }
  function cancelPending() {
    if (active)
      active.finish(bookError("EXPORT_CANCELLED", "Book export cancelled; source is unchanged."));
  }
  function cancel() {
    cancelPending();
    releaseIdle();
  }
  async function render(files, format, options = {}, { signal } = {}) {
    alive();
    if (active) throw bookError("BOOK_BUSY", "A book export is already running.");
    const [, mimeType, extension] = formatInfo(format);
    const retaining = retainPreview && format === "preview";
    if (
      signal !== undefined &&
      (signal === null ||
        typeof signal.aborted !== "boolean" ||
        typeof signal.addEventListener !== "function" ||
        typeof signal.removeEventListener !== "function")
    ) {
      throw bookError("INVALID_OPTIONS", "signal must be an AbortSignal.");
    }
    if (signal?.aborted)
      throw bookError("EXPORT_CANCELLED", "Book export cancelled before starting.");
    const id = ++serial;
    return new Promise((resolve, reject) => {
      let worker = null,
        timer = null,
        settled = false,
        retired = false;
      const onAbort = () =>
        finish(bookError("EXPORT_CANCELLED", "Book export cancelled; source is unchanged."));
      function retireWorker(keep = false) {
        if (!worker || retired) return;
        retired = true;
        let detached = true;
        for (const [kind, listener] of [
          ["message", onMessage],
          ["error", onError],
          ["messageerror", onError],
        ]) {
          try {
            worker.removeEventListener(kind, listener);
          } catch {
            detached = false;
            /* Attempt every detach even when a host adapter fails. */
          }
        }
        if (keep && detached && !disposed) keepIdle(worker);
        else terminate(worker);
      }
      function finish(error, result, failed = error !== null) {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        if (active?.id === id) active = null;
        try {
          signal?.removeEventListener("abort", onAbort);
        } catch {
          /* A signal adapter cannot retain the active export slot. */
        }
        retireWorker(!failed && retaining);
        if (failed) reject(error);
        else resolve(result);
      }
      function onError(event) {
        try {
          event.preventDefault?.();
        } catch {
          /* Still report the failure. */
        }
        finish(
          bookError(
            "WORKER_FAILED",
            "Book worker could not load or execute. Rebuild the matching WASM package and retry.",
          ),
        );
      }
      function onMessage(event) {
        if (settled) return;
        const data = event.data;
        try {
          if (!data || data.schemaVersion !== 1 || data.id !== id || data.format !== format) {
            throw bookError("WORKER_PROTOCOL_ERROR", "Book worker replied for a different export.");
          }
          if (data.error) {
            const detail = diagnostic(data.error);
            throw bookError(detail.code, detail.message);
          }
          if (retaining && data.retainedPreview !== true)
            throw bookError("WORKER_PROTOCOL_ERROR", "Rebuild the matching book worker for retained previews.");
          checkedOutput(data.bytes, data.sourceLength, maxOutputBytes);
          const bytes = data.bytes;
          finish(
            null,
            Object.freeze({
              format: `book-${format}`,
              bytes,
              sourceLength: data.sourceLength,
              mimeType,
              extension,
              blob: () => new Blob([bytes], { type: mimeType }),
            }),
          );
        } catch (error) {
          finish(error, undefined, true);
        }
      }
      // Reserve the slot before option/chapter getters, signal adapters, or
      // the factory can reenter render(), cancel(), or dispose().
      active = { id, finish };
      let input, transfer;
      try {
        // Claim any idle worker before input/signal adapters can reenter. A
        // failed or cancelled admission releases only this request's endpoint.
        if (!retaining) releaseIdle();
        else worker = takeIdle();
        if (settled) { retireWorker(); return; }
        signal?.addEventListener("abort", onAbort, { once: true });
        if (settled) return;
        if (signal?.aborted) {
          onAbort();
          return;
        }
        input = prepareBookInput(
          files,
          format === "inspection" ? {} : format === "links" ? bookLinkOptions(options) : options,
        );
        if (settled) return;
        if (signal?.aborted) {
          onAbort();
          return;
        }
        // Transfer only private copies, never detach caller-owned assets.
        transfer = [...input.options.images, ...input.options.fontAssets].map(
          (asset) => asset.bytes.buffer,
        );
      } catch (error) {
        finish(error, undefined, true);
        return;
      }
      try {
        worker ??= workerFactory();
        // The factory may cancel before it returns the worker we now own.
        if (settled) {
          retireWorker();
          return;
        }
        if (
          !worker ||
          ["postMessage", "terminate", "addEventListener", "removeEventListener"].some(
            (method) => typeof worker[method] !== "function",
          )
        ) {
          throw bookError(
            "WORKER_FAILED",
            "workerFactory must return an owned Worker-compatible endpoint.",
          );
        }
        for (const [kind, listener] of [
          ["message", onMessage],
          ["error", onError],
          ["messageerror", onError],
        ]) {
          worker.addEventListener(kind, listener);
          if (settled) return;
        }
        timer = setTimeout(
          () =>
            finish(
              bookError(
                "EXPORT_TIMEOUT",
                "Book export exceeded its time budget; the worker was terminated.",
              ),
            ),
          timeoutMs,
        );
        worker.postMessage({ schemaVersion: 1, id, format, ...input, maxOutputBytes,
          ...(retaining ? { retainPreview: true } : {}) }, transfer);
      } catch (error) {
        finish(bookError("WORKER_FAILED", diagnostic(error).message));
      }
    });
  }
  return Object.freeze({
    render,
    cancel,
    cancelPending,
    get hasRetainedPreview() { return idle !== null; },
    get busy() {
      return active !== null;
    },
    dispose() {
      disposed = true;
      cancel();
    },
  });
}

/** DedicatedWorkerGlobalScope-compatible handler; tests inject transport and
 * engine doubles explicitly. It never fetches a chapter, image or font URL.
 */
export function installBookWorker(scope, engine) {
  let busy = false;
  scope.addEventListener("message", async (event) => {
    const data = event.data;
    const envelope = { schemaVersion: 1, id: data?.id, format: data?.format };
    let owned = false;
    try {
      if (!data || data.schemaVersion !== 1 || !Number.isSafeInteger(data.id) || data.id < 1) {
        throw bookError("WORKER_PROTOCOL_ERROR", "Invalid book request envelope.");
      }
      let [method] = formatInfo(data.format);
      if (data.retainPreview !== undefined && typeof data.retainPreview !== "boolean")
        throw bookError("WORKER_PROTOCOL_ERROR", "Invalid preview retention request.");
      if (data.retainPreview) {
        if (data.format !== "preview" || typeof engine.renderRetainedBookPreview !== "function")
          throw bookError("UNSUPPORTED_BOOK_PREVIEW", "Rebuild the matching worker for retained book previews.");
        method = "renderRetainedBookPreview";
        envelope.retainedPreview = true;
      }
      if (busy) throw bookError("BOOK_BUSY", "A book export is already running.");
      if (
        !Number.isInteger(data.maxOutputBytes) ||
        data.maxOutputBytes < 1 ||
        data.maxOutputBytes > LIMIT
      ) {
        throw bookError("INVALID_OPTIONS", "Invalid output limit.");
      }
      busy = true;
      owned = true;
      const input = prepareBookInput(
        data.files,
        data.format === "inspection"
          ? {}
          : data.format === "links"
            ? bookLinkOptions(data.options)
            : data.options,
      );
      const result = await engine[method](input.files, input.options);
      checkedOutput(result.bytes, result.sourceLength, data.maxOutputBytes);
      // Transfer only the returned view, never a larger backing WASM memory.
      const bytes = result.bytes.slice();
      scope.postMessage({ ...envelope, bytes, sourceLength: result.sourceLength }, [bytes.buffer]);
    } catch (error) {
      if (owned && data.retainPreview) {
        try { engine.clearRetainedBookPreview?.(); } catch { /* Report the original failure. */ }
      }
      scope.postMessage({ ...envelope, error: diagnostic(error) });
    } finally {
      if (owned) busy = false;
    }
  });
}

/** Worker-private reuse of one native book. Inputs are the privately owned,
 * normalized snapshots admitted by installBookWorker, never host buffers.
 * Every output still uses the complete native site exporter and preview codec.
 * JavaScript compares captures; only Rust resolves includes or parses Markdown.
 */
export function createRetainedBookPreview(engine, renderPreview) {
  if (typeof engine?.createBook !== "function" || typeof renderPreview !== "function")
    throw new TypeError("Retained previews require the native book factory and existing preview codec.");
  let cached = null, busy = false, generation = 0;
  function clear() {
    generation++;
    const previous = cached;
    cached = null;
    previous?.session.dispose();
  }
  function compatible(input) {
    if (!cached) return false;
    const before = cached.input, a = before.options, b = input.options;
    const pathsMatch = (left, right) => left.length === right.length
      && left.every((file, index) => file.path === right[index].path);
    if (!pathsMatch(before.files, input.files) || !pathsMatch(a.includeSources, b.includeSources))
      return false;
    for (const key of ["title", "author", "lang", "customCss", "toc", "pageNumbers", "font",
      "darkMode", "fontScale", "expandIncludes"]) {
      if (a[key] !== b[key]) return false;
    }
    if (JSON.stringify(a.page) !== JSON.stringify(b.page)) return false;
    const sameAssets = (left, right, keys) => left.length === right.length && left.every((asset, i) => {
      const other = right[i];
      if (keys.some(key => asset[key] !== other[key]) || asset.bytes.length !== other.bytes.length)
        return false;
      for (let j = 0; j < asset.bytes.length; j++) if (asset.bytes[j] !== other.bytes[j]) return false;
      return true;
    });
    return sameAssets(a.images, b.images, ["destination"])
      && sameAssets(a.fontAssets, b.fontAssets, ["slot", "weight"]);
  }
  async function render(files, options) {
    if (busy) throw bookError("BOOK_BUSY", "A retained preview is already rendering.");
    busy = true;
    let ticket = generation;
    const input = { files, options };
    try {
      if (!compatible(input)) { clear(); ticket = generation; }
      if (cached) {
        const changes = [];
        for (const [old, next] of [[cached.input.files, files],
          [cached.input.options.includeSources, options.includeSources]]) {
          for (let i = 0; i < next.length; i++) if (old[i].source !== next[i].source) changes.push(next[i]);
        }
        if (changes.length) {
          let supported = typeof cached.session.updateSources === "function", revision;
          if (supported) {
            try { revision = cached.session.sourceRevision; }
            catch (error) {
              if (error?.code !== "UNSUPPORTED_BOOK_UPDATE") throw error;
              supported = false;
            }
          }
          if (supported) cached.session.updateSources(changes, { expectedRevision: revision });
          else { clear(); ticket = generation; } // Older native builds reconstruct, never show old source.
        }
      }
      if (!cached) {
        const session = await engine.createBook(files, options);
        if (ticket !== generation) {
          session.dispose();
          throw bookError("EXPORT_CANCELLED", "Retained book was released during initialization.");
        }
        cached = { session, input };
      }
      const session = cached.session;
      // Feed the native session's complete site through the unchanged ZIP /
      // chapter-map validator, not a second HTML preview implementation.
      const result = await renderPreview({ renderBookSite: () => session.renderSite() }, files, options);
      if (ticket !== generation)
        throw bookError("EXPORT_CANCELLED", "Retained book was released during preview generation.");
      cached.input = input;
      return result;
    } catch (error) {
      // An update or export can fail after native mutation. Never keep a capture
      // that describes a different session, or label protocol failure rollback.
      try { clear(); } catch { /* Preserve the primary failure. */ }
      throw error;
    } finally { busy = false; }
  }
  return Object.freeze({ render, clear });
}
