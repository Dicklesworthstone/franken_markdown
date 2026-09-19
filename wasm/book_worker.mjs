// Worker transport only. The entrypoint injects the real Rust/WASM book API.
import { prepareBookInput } from "./book_session.mjs";
const LIMIT = 128 * 1024 * 1024;
const formats = Object.freeze({
  pdf: ["renderBookPdf", "application/pdf", "pdf"],
  epub: ["renderBookEpub", "application/epub+zip", "epub"],
  site: ["renderBookSite", "application/zip", "zip"],
  preview: ["renderBookPreview", "application/json", "json"]
});
export const bookError = (code, message) => Object.assign(new Error(message), { code });
function formatInfo(format) {
  if (!Object.hasOwn(formats, format)) throw bookError("INVALID_FORMAT", "Choose pdf, epub, site or preview.");
  return formats[format];
}
function checkedOutput(bytes, sourceLength, maximum) {
  if (!(bytes instanceof Uint8Array) || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
      || !Number.isSafeInteger(sourceLength) || sourceLength < 0 || sourceLength > 64 * 1024 * 1024) {
    throw bookError("WORKER_PROTOCOL_ERROR", "Invalid book output envelope.");
  }
  if (bytes.byteLength === 0 || bytes.byteLength > maximum) throw bookError("OUTPUT_LIMIT", "Book output is empty or exceeds the output byte limit.");
}
function diagnostic(error) {
  // Rust bindings may throw structured JSON strings; preserve the reason code,
  // not an unbounded engine dump or a raw source document.
  if (typeof error === "string" && error.length <= 4096) {
    try { error = JSON.parse(error); } catch { error = null; }
  }
  return { code: typeof error?.code === "string" ? error.code.slice(0, 80) : "BOOK_ERROR",
    message: typeof error?.message === "string" ? error.message.slice(0, 2048) : "Book rendering failed." };
}

/** One worker per export. Cancellation terminates synchronous Rust work, not
 * merely its Promise. Nothing is queued and no engine instance survives a job.
 */
export function createBookWorkerClient({ workerFactory, timeoutMs = 120000, maxOutputBytes = LIMIT } = {}) {
  if (typeof workerFactory !== "function" || !Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 600000
      || !Number.isInteger(maxOutputBytes) || maxOutputBytes < 1 || maxOutputBytes > LIMIT) {
    throw bookError("INVALID_OPTIONS", "Invalid worker factory, timeout or output limit.");
  }
  let disposed = false, active = null, serial = 0;
  const alive = () => { if (disposed) throw bookError("SESSION_DISPOSED", "Book worker is disposed."); };
  function cancel() {
    if (active) active.finish(bookError("EXPORT_CANCELLED", "Book export cancelled; source is unchanged."));
  }
  async function render(files, format, options = {}, { signal } = {}) {
    alive();
    if (active) throw bookError("BOOK_BUSY", "A book export is already running.");
    const [, mimeType, extension] = formatInfo(format);
    if (signal && (typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function")) {
      throw bookError("INVALID_OPTIONS", "signal must be an AbortSignal.");
    }
    if (signal?.aborted) throw bookError("EXPORT_CANCELLED", "Book export cancelled before starting.");
    const input = prepareBookInput(files, options);
    // Only these private copies are transferred. The caller can edit/revoke its
    // assets immediately, without mutating the captured export or losing bytes.
    const transfer = [...input.options.images, ...input.options.fontAssets].map(asset => asset.bytes.buffer);
    const id = ++serial;
    return new Promise((resolve, reject) => {
      let worker = null, timer = null, settled = false;
      const onAbort = () => finish(bookError("EXPORT_CANCELLED", "Book export cancelled; source is unchanged."));
      function finish(error, result) {
        if (settled) return;
        settled = true; clearTimeout(timer); signal?.removeEventListener("abort", onAbort);
        if (active?.id === id) active = null;
        if (worker) {
          try {
            worker.removeEventListener("message", onMessage);
            worker.removeEventListener("error", onError);
            worker.removeEventListener("messageerror", onError);
          } catch { /* A failed factory must still settle the export. */ }
          try { worker.terminate(); } catch { /* Promise still settles on a dead worker. */ }
        }
        if (error) reject(error); else resolve(result);
      }
      function onError(event) {
        event.preventDefault?.();
        finish(bookError("WORKER_FAILED", "Book worker could not load or execute. Rebuild the matching WASM package and retry."));
      }
      function onMessage(event) {
        if (settled) return;
        const data = event.data;
        try {
          if (!data || data.schemaVersion !== 1 || data.id !== id || data.format !== format) {
            throw bookError("WORKER_PROTOCOL_ERROR", "Book worker replied for a different export.");
          }
          if (data.error) { const detail = diagnostic(data.error); throw bookError(detail.code, detail.message); }
          checkedOutput(data.bytes, data.sourceLength, maxOutputBytes);
          const bytes = data.bytes;
          finish(null, Object.freeze({ format: `book-${format}`, bytes, sourceLength: data.sourceLength, mimeType, extension,
            blob: () => new Blob([bytes], { type: mimeType }) }));
        } catch (error) { finish(error); }
      }
      active = { id, finish };
      try {
        worker = workerFactory();
        worker.addEventListener("message", onMessage); worker.addEventListener("error", onError);
        worker.addEventListener("messageerror", onError);
        signal?.addEventListener("abort", onAbort, { once: true });
        if (signal?.aborted) { onAbort(); return; }
        timer = setTimeout(() => finish(bookError("EXPORT_TIMEOUT", "Book export exceeded its time budget; the worker was terminated.")), timeoutMs);
        worker.postMessage({ schemaVersion: 1, id, format, ...input, maxOutputBytes }, transfer);
      } catch (error) { finish(bookError("WORKER_FAILED", diagnostic(error).message)); }
    });
  }
  return Object.freeze({ render, cancel, get busy() { return active !== null; },
    dispose() { disposed = true; cancel(); } });
}

/** DedicatedWorkerGlobalScope-compatible handler; tests inject transport and
 * engine doubles explicitly. It never fetches a chapter, image or font URL.
 */
export function installBookWorker(scope, engine) {
  let busy = false;
  scope.addEventListener("message", async event => {
    const data = event.data;
    const envelope = { schemaVersion: 1, id: data?.id, format: data?.format };
    let owned = false;
    try {
      if (!data || data.schemaVersion !== 1 || !Number.isSafeInteger(data.id) || data.id < 1) {
        throw bookError("WORKER_PROTOCOL_ERROR", "Invalid book request envelope.");
      }
      const [method] = formatInfo(data.format);
      if (busy) throw bookError("BOOK_BUSY", "A book export is already running.");
      if (!Number.isInteger(data.maxOutputBytes) || data.maxOutputBytes < 1 || data.maxOutputBytes > LIMIT) {
        throw bookError("INVALID_OPTIONS", "Invalid output limit.");
      }
      busy = true; owned = true;
      const input = prepareBookInput(data.files, data.options);
      const result = await engine[method](input.files, input.options);
      checkedOutput(result.bytes, result.sourceLength, data.maxOutputBytes);
      // Transfer only the returned view, never a larger backing WASM memory.
      const bytes = result.bytes.slice();
      scope.postMessage({ ...envelope, bytes, sourceLength: result.sourceLength }, [bytes.buffer]);
    } catch (error) { scope.postMessage({ ...envelope, error: diagnostic(error) }); }
    finally { if (owned) busy = false; }
  });
}
