// Atomic multi-format publication over one private retained worker client.
// Capture and transport are injected by book-worker.js; this module neither
// parses Markdown nor serializes publication formats or creates object URLs.
const LIMIT = 128 * 1024 * 1024;
const TYPES = Object.freeze({
  pdf: ["application/pdf", "pdf"],
  epub: ["application/epub+zip", "epub"],
  site: ["application/zip", "zip"],
});
const DEFAULT_FORMATS = Object.freeze(["pdf", "epub", "site"]);
const error = (code, message) => Object.assign(new Error(message), { code });
const clock = () => typeof performance === "object" && typeof performance.now === "function"
  ? performance.now() : Date.now();

function publicationFormats(value) {
  const count = Array.isArray(value) ? Object.getOwnPropertyDescriptor(value, "length")?.value : -1;
  if (!Number.isInteger(count) || count < 1 || count > 3)
    throw error("INVALID_FORMAT", "Choose one through three unique publication formats: pdf, epub, site.");
  const formats = [];
  for (let i = 0; i < count; i++) {
    const field = Object.getOwnPropertyDescriptor(value, String(i));
    if (!field || !Object.hasOwn(field, "value") || typeof field.value !== "string"
        || !Object.hasOwn(TYPES, field.value) || formats.includes(field.value))
      throw error("INVALID_FORMAT", "Publication formats must be unique pdf, epub or site data entries.");
    formats.push(field.value);
  }
  return formats;
}

function publicationOutput(value, format, remaining, previousRevision, sourceLength) {
  const bytes = value?.bytes, revision = value?.retainedInputRevision;
  if (!Number.isSafeInteger(revision) || revision < 1)
    throw error("UNSUPPORTED_BOOK_PUBLICATION", "Rebuild the matching worker for captured multi-format publication.");
  if (revision <= previousRevision || value.format !== `book-${format}`
      || !Number.isSafeInteger(value.sourceLength) || value.sourceLength < 0
      || value.sourceLength > 64 * 1024 * 1024
      || (sourceLength !== null && value.sourceLength !== sourceLength)
      || !(bytes instanceof Uint8Array)
      || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]")
    throw error("WORKER_PROTOCOL_ERROR", "Publication output does not match the captured book and requested format.");
  if (bytes.byteLength === 0 || bytes.byteLength > remaining)
    throw error("OUTPUT_LIMIT", "Combined publication output is empty or exceeds its byte limit.");
  // Own exactly this view before another export or disposal can recycle its
  // backing storage. The aggregate limit is an output limit, not a heap bound.
  const owned = bytes.slice(), [mimeType, extension] = TYPES[format];
  return {
    revision,
    output: Object.freeze({
      format: `book-${format}`, bytes: owned, sourceLength: value.sourceLength,
      mimeType, extension, blob: () => new Blob([owned], { type: mimeType }),
    }),
  };
}

/** Internal, dependency-injected coordinator used by createBookPublicationWorker.
 * A publication owns a fresh retained client exclusively until all requested
 * outputs are admitted. Later formats use empty source deltas against its exact
 * acknowledged capture; images, fonts and unchanged sources are not resent.
 * No partial output is published on failure, cancellation or timeout.
 */
export function createBookPublicationClient({
  createClient, captureInput, timeoutMs = 120000, maxOutputBytes = LIMIT,
} = {}) {
  if (typeof createClient !== "function" || typeof captureInput !== "function"
      || !Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 600000
      || !Number.isInteger(maxOutputBytes) || maxOutputBytes < 1 || maxOutputBytes > LIMIT)
    throw error("INVALID_OPTIONS", "Invalid publication factory, capture, deadline or combined output limit.");
  let active = null, disposed = false;

  function render(files, formats = DEFAULT_FORMATS, options = {}, request = {}) {
    if (disposed) return Promise.reject(error("SESSION_DISPOSED", "Book publisher is disposed."));
    if (active) return Promise.reject(error("BOOK_BUSY", "A book publication is already running."));
    return new Promise((resolve, reject) => {
      let settled = false, client = null, timer = null, signal, deadline = Infinity;
      let outputs = [], total = 0, sourceLength = null, revision = 0;
      const retire = (owned) => {
        try { owned?.dispose(); } catch { /* Cleanup must not hide the primary result. */ }
      };
      const onAbort = () => finish(error("EXPORT_CANCELLED", "Book publication cancelled; no outputs were published."));
      const operation = { cancel: onAbort };
      const expired = () => finish(error("EXPORT_TIMEOUT", "Book publication exceeded its shared time budget."));
      const stopped = () => {
        // Timers cannot interrupt synchronous capture/copying or a custom
        // endpoint's microtask chain. Refuse late results at phase boundaries.
        if (!settled && clock() >= deadline) expired();
        return settled;
      };
      function finish(cause, result, failed = true) {
        if (settled) return;
        settled = true;
        if (active === operation) active = null;
        clearTimeout(timer);
        try { signal?.removeEventListener("abort", onAbort); } catch { /* Still retire the worker. */ }
        const owned = client;
        client = null;
        outputs = [];
        retire(owned);
        if (failed) reject(cause);
        else resolve(result);
      }
      // Reserve before format proxies, option/source getters, signal adapters
      // or the worker factory can reenter this public operation.
      active = operation;
      let selected, input;
      try {
        selected = publicationFormats(formats);
        signal = request?.signal;
        if (signal !== undefined && (signal === null || typeof signal.aborted !== "boolean"
            || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
          throw error("INVALID_OPTIONS", "signal must be an AbortSignal.");
        if (settled) return;
        if (signal?.aborted) { onAbort(); return; }
        signal?.addEventListener("abort", onAbort, { once: true });
        if (settled) return;
        if (signal?.aborted) { onAbort(); return; }
        // One deadline covers capture, initialization and every format, not a
        // fresh timeout per output. Disposing the client terminates Rust work.
        deadline = clock() + timeoutMs;
        timer = setTimeout(expired, timeoutMs);
        input = captureInput(files, options);
        if (stopped()) return;
        if (signal?.aborted) { onAbort(); return; }
        const created = createClient();
        if (stopped()) { retire(created); return; }
        client = created;
        if (!client || ["render", "renderSourceUpdate", "dispose"].some(key => typeof client[key] !== "function"))
          throw error("WORKER_FAILED", "Publication requires an owned retained source-update client.");
      } catch (cause) {
        finish(cause);
        return;
      }
      void (async () => {
        try {
          for (let index = 0; index < selected.length; index++) {
            if (stopped()) return;
            if (total >= maxOutputBytes)
              throw error("OUTPUT_LIMIT", "No output budget remains for the next publication format.");
            const format = selected[index];
            const result = index === 0
              ? await client.render(input.files, format, input.options, { signal })
              : await client.renderSourceUpdate([], format, { expectedRevision: revision }, { signal });
            if (stopped()) return;
            const admitted = publicationOutput(result, format, maxOutputBytes - total, revision, sourceLength);
            if (stopped()) return;
            revision = admitted.revision;
            sourceLength = admitted.output.sourceLength;
            total += admitted.output.bytes.byteLength;
            outputs.push(admitted.output);
          }
          const result = Object.freeze({
            format: "book-publication", sourceLength, byteLength: total,
            outputs: Object.freeze(outputs),
          });
          finish(null, result, false);
        } catch (cause) { finish(cause); }
      })();
    });
  }
  return Object.freeze({
    render,
    cancel() { active?.cancel(); },
    get busy() { return active !== null; },
    dispose() { disposed = true; active?.cancel(); },
  });
}
