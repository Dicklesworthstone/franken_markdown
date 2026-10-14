// Workbench ownership of one captured publication. The injected publisher owns
// native rendering; this session owns only cancellation, receipts and Blob URLs.
const EDITION_LIMIT = 128 * 1024 * 1024;
const FORMATS = Object.freeze({
  pdf: Object.freeze({ mimeType: "application/pdf", filename: "book.pdf" }),
  epub: Object.freeze({ mimeType: "application/epub+zip", filename: "book.epub" }),
  site: Object.freeze({ mimeType: "application/zip", filename: "book-site.zip" }),
});
const fail = (code, message) => Object.assign(new Error(message), { code });
function selectedFormats(value) {
  if (!Array.isArray(value) || value.length > 3)
    throw fail("INVALID_FORMAT", "Select PDF, EPUB and/or HTML site.");
  const result = [];
  for (let i = 0; i < value.length; i++) {
    const field = Object.getOwnPropertyDescriptor(value, String(i));
    if (!field || !Object.hasOwn(field, "value") || typeof field.value !== "string"
        || !Object.hasOwn(FORMATS, field.value) || result.includes(field.value))
      throw fail("INVALID_FORMAT", "Select each supported publication format at most once.");
    result.push(field.value);
  }
  return Object.freeze(result);
}
function waitFor(pending, signal) {
  return new Promise((resolve, reject) => {
    const cleanup = () => signal.removeEventListener("abort", abort);
    const abort = () => { cleanup(); reject(signal.reason); };
    signal.addEventListener("abort", abort, { once: true });
    // Also observe late failures from a cancelled/uncooperative endpoint.
    Promise.resolve(pending).then(value => { cleanup(); resolve(value); },
      cause => { cleanup(); reject(cause); });
    if (signal.aborted) abort();
  });
}

/** capture() synchronously flushes the source host and returns collection.snapshot().
 * checkpoint() covers raw editor input as well as collection/resource revision.
 * changed() tolerates notifications produced by that initial synchronous capture;
 * invalidate() always retires work (including uncommitted input and composition).
 * There is no renderer, persistence, permission request or automatic download here.
 */
export function createBookEditionSession({
  publisher, capture, checkpoint, isBlocked = () => false, urls = URL, onState = () => {},
} = {}) {
  if (!publisher || ["render", "cancel", "dispose"].some(key => typeof publisher[key] !== "function")
      || [capture, checkpoint, isBlocked, onState].some(fn => typeof fn !== "function")
      || !urls || ["createObjectURL", "revokeObjectURL"].some(key => typeof urls[key] !== "function"))
    throw fail("INVALID_OPTIONS", "Edition sessions require a publisher, source host and URL owner.");
  let disposed = false, suspended = false, capturing = false;
  let active = null, ready = null, formats = selectedFormats(["pdf", "epub", "site"]);
  let phase = "idle", errorCode = null;
  let message = "Select formats and prepare one edition. No files are saved automatically.";
  const blocked = () => {
    try { return !!isBlocked(); } catch { return true; }
  };
  function state() {
    return Object.freeze({
      phase, busy: active !== null,
      canPrepare: !disposed && !suspended && !active && formats.length > 0 && !blocked(),
      formats,
      downloads: Object.freeze(ready ? ready.downloads.slice() : []),
      byteLength: ready?.byteLength ?? 0, sourceLength: ready?.sourceLength ?? 0,
      errorCode, message,
    });
  }
  function announce() {
    try { onState(state()); } catch { /* View observers cannot own a publication outcome. */ }
  }
  function release(record) {
    if (!record) return;
    const owned = record.urls.splice(0);
    record.downloads = [];
    for (const url of owned) {
      try { urls.revokeObjectURL(url); } catch { /* Retire all URLs even if one adapter fails. */ }
    }
  }
  function retire(code, text, nextPhase = "idle") {
    const job = active, previous = ready;
    active = ready = null;
    phase = nextPhase; message = text; errorCode = null;
    release(previous);
    release(job);
    if (job) {
      job.abort.abort(fail(code, text));
      try { publisher.cancel(); } catch { /* Source and URLs are still retired. */ }
    }
    announce();
  }
  function alive() {
    if (disposed) throw fail("SESSION_DISPOSED", "Edition session is disposed.");
    if (suspended) throw fail("WORKBENCH_SUSPENDED", "Resume the workbench before preparing an edition.");
  }
  function owned(job) {
    alive();
    if (active !== job || job.abort.signal.aborted)
      throw job.abort.signal.reason ?? fail("STALE_SOURCE", "This edition was retired.");
    if (blocked()) throw fail("BOOK_BUSY", "Finish importing or composing text before publishing.");
  }
  function fence(job) {
    owned(job);
    if (checkpoint() !== job.checkpoint)
      throw fail("STALE_SOURCE", "The book or editor changed; prepare a new edition.");
  }
  function current(record) {
    if (disposed || suspended || blocked()) return false;
    try { return checkpoint() === record.checkpoint; } catch { return false; }
  }
  async function prepare() {
    alive();
    if (active) throw fail("BOOK_BUSY", "An edition is already being prepared.");
    if (blocked()) throw fail("BOOK_BUSY", "Finish importing or composing text before publishing.");
    if (!formats.length) throw fail("INVALID_FORMAT", "Select at least one publication format.");
    release(ready); ready = null;
    const job = { abort: new AbortController(), urls: [], downloads: [], checkpoint: null };
    const selected = formats;
    active = job; phase = "busy"; errorCode = null;
    message = `Preparing ${selected.map(format => format.toUpperCase()).join(" + ")} from one captured book. No downloads are ready yet.`;
    announce();
    try {
      owned(job);
      let input;
      capturing = true;
      try { input = capture(); } finally { capturing = false; }
      owned(job);
      job.checkpoint = checkpoint();
      const pending = publisher.render(input.files, selected, input.options, { signal: job.abort.signal });
      input = null; // Do not retain an extra resource snapshot while awaiting the worker.
      const result = await waitFor(pending, job.abort.signal);
      fence(job);
      if (result?.format !== "book-publication" || !Array.isArray(result.outputs)
          || result.outputs.length !== selected.length || !Number.isSafeInteger(result.byteLength)
          || result.byteLength < 1 || result.byteLength > EDITION_LIMIT
          || !Number.isSafeInteger(result.sourceLength) || result.sourceLength < 0
          || result.sourceLength > 64 * 1024 * 1024)
        throw fail("WORKER_PROTOCOL_ERROR", "The publisher did not return a complete captured edition.");
      // Admit every receipt/Blob before creating even the first download URL.
      let total = 0;
      const blobs = result.outputs.map((output, index) => {
        const format = selected[index], expected = FORMATS[format];
        if (output?.format !== `book-${format}` || output.sourceLength !== result.sourceLength
            || output.mimeType !== expected.mimeType || typeof output.blob !== "function")
          throw fail("WORKER_PROTOCOL_ERROR", "An edition output does not match its requested format.");
        const blob = output.blob();
        if (!(blob instanceof Blob) || blob.size < 1 || blob.type !== expected.mimeType)
          throw fail("WORKER_PROTOCOL_ERROR", "An edition output is not a valid publication Blob.");
        total += blob.size;
        if (total > EDITION_LIMIT)
          throw fail("OUTPUT_LIMIT", "Combined edition output exceeds 128 MiB.");
        return { format, blob, ...expected };
      });
      if (total !== result.byteLength)
        throw fail("WORKER_PROTOCOL_ERROR", "Edition byte totals do not match the captured outputs.");
      fence(job);
      for (const { format, blob, mimeType, filename } of blobs) {
        const href = urls.createObjectURL(blob);
        job.urls.push(href);
        fence(job);
        job.downloads.push(Object.freeze({ format, filename, href, mimeType, byteLength: blob.size }));
      }
      fence(job);
      job.byteLength = total; job.sourceLength = result.sourceLength;
      ready = job; active = null; phase = "ready";
      message = `Edition ready: ${selected.length} files, ${total} bytes. Use each download link to save it; preparation alone saves nothing.`;
      announce();
      return state();
    } catch (cause) {
      release(job);
      if (active === job) {
        active = null; phase = "error";
        errorCode = typeof cause?.code === "string" ? cause.code : "EDITION_FAILED";
        message = `${errorCode}: ${cause?.message ?? "Edition preparation failed."} No edition downloads were published. Source remains editable.`;
        job.abort.abort(cause);
        try { publisher.cancel(); } catch { /* Preserve the original failure. */ }
        announce();
      }
      throw cause;
    }
  }
  return Object.freeze({
    get state() { return state(); },
    prepare,
    setFormats(value) {
      alive();
      const next = selectedFormats(value);
      if (next.length === formats.length && next.every((format, index) => format === formats[index])) return;
      formats = next;
      retire("EXPORT_CANCELLED", "Publication selection changed. Prepare a new edition.");
    },
    changed() {
      if (disposed || suspended || capturing) return;
      if (active || ready) retire("STALE_SOURCE", "The book changed. Prepare a new edition from the current sources, settings and resources.");
      else announce();
    },
    invalidate() {
      if (disposed || suspended) return;
      if (active || ready) retire("STALE_SOURCE", "The editor changed or is busy. Prepare a new edition when editing is complete.");
      else announce();
    },
    /** Used immediately before both normal and auxiliary-link activation.
     * An old detached link cannot revoke an unrelated newer edition. */
    authorizeDownload(href) {
      if (!ready || !ready.downloads.some(item => item.href === href)) return false;
      if (current(ready)) return true;
      retire("STALE_SOURCE", "Edition downloads are stale. Prepare them again from the current book.");
      return false;
    },
    cancel() {
      if (!disposed && !suspended)
        retire("EXPORT_CANCELLED", "Edition cancelled or discarded. Source and other exports are unchanged.");
    },
    suspend() {
      if (disposed || suspended) return;
      suspended = true;
      retire("WORKBENCH_SUSPENDED", "Edition downloads were discarded while the workbench was suspended.", "suspended");
    },
    resume() {
      if (disposed || !suspended) return;
      suspended = false; phase = "idle";
      message = "Workbench resumed. Explicitly prepare a new edition when ready.";
      announce();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      retire("SESSION_DISPOSED", "Edition session is disposed.", "disposed");
      try { publisher.dispose(); } finally { onState = () => {}; }
    },
  });
}
