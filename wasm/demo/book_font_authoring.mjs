import { BOOK_FONT_LIMITS, BOOK_FONT_SLOTS, readBookFont } from "./book_font_assets.mjs";

const failure = (code, message) => Object.assign(new Error(message), { code });
// A small native render validates the proposed faces even before a book has a
// chapter. This is a font admission probe, never a replacement for book source
// or a claim that all of the eventual book's glyphs are covered.
const PROBE = Object.freeze({
  path: "font-validation.md",
  source: "# Font validation\n\nRegular Aa **Bold Bb** *Italic Cc* ***Bold italic Dd*** `Monospace Ee`\n",
});

function signalValue(signal) {
  if (signal !== undefined && (!signal || typeof signal.aborted !== "boolean"
      || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function")) {
    throw failure("INVALID_OPTIONS", "Use an AbortSignal for font cancellation.");
  }
}

function selections(values, inventory) {
  if (!Array.isArray(values) || values.length < 1 || values.length > BOOK_FONT_SLOTS.length)
    throw failure("FONT_LIMIT", "Choose one through five font assignments.");
  const next = new Map(inventory.map(font => [font.slot, font.size])), seen = new Set(), selected = [];
  // Capture by index, not a caller-controlled iterator. Admit the FINAL store
  // before any file reads so replacing at capacity does not require headroom.
  for (let i = 0; i < values.length; i++) {
    const { slot, file, weight } = values[i] ?? {};
    if (!BOOK_FONT_SLOTS.includes(slot) || seen.has(slot))
      throw failure("INVALID_FONT_SLOT", "Choose each supported font role at most once.");
    if (weight !== undefined && (!Number.isInteger(weight) || weight < 1 || weight > 1000))
      throw failure("INVALID_FONT_WEIGHT", "Font weight must be blank or an integer from 1 through 1000.");
    if (Object.prototype.toString.call(file) !== "[object File]" || typeof file.arrayBuffer !== "function"
        || typeof file.name !== "string" || !/\.ttf$/i.test(file.name))
      throw failure("INVALID_FONT", "Choose a local TrueType (.ttf) file for each selected role.");
    const size = file.size;
    if (!Number.isSafeInteger(size) || size < 1 || size > BOOK_FONT_LIMITS.faceBytes)
      throw failure("FONT_LIMIT", "Each font must contain 1 byte through 8 MiB.");
    seen.add(slot); next.set(slot, size); selected.push({ slot, file, weight, size });
  }
  if ([...next.values()].reduce((total, size) => total + size, 0) > BOOK_FONT_LIMITS.totalBytes)
    throw failure("FONT_LIMIT", "The resulting supplied font roles exceed 32 MiB.");
  return selected;
}

function checkProbe(result) {
  const bytes = result?.bytes;
  if (result?.format !== "book-pdf" || result.mimeType !== "application/pdf"
      || !(bytes instanceof Uint8Array)
      || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
      || bytes.length < 5 || bytes.length > 128 * 1024 * 1024
      || String.fromCharCode(...bytes.subarray(0, 5)) !== "%PDF-") {
    throw failure("FONT_VALIDATION_FAILED", "The font validation worker did not return a PDF. No fonts were assigned.");
  }
}

/** Owns a dedicated book worker. The existing collection is the only font
 * store; this session never keeps a second resource registry or changes source.
 * No assignment becomes observable before the whole batch passes native PDF
 * preflight and the original collection/editor checkpoint still matches. */
export function createBookFontAuthoring({ collection, controls, worker, timeoutMs = 120000 }) {
  if (!collection || !controls || typeof worker?.render !== "function" || typeof worker.dispose !== "function"
      || !Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 600000)
    throw new TypeError("Font authoring needs a book collection, editor controls and a dedicated worker.");
  let pending = null, admitting = false, disposed = false, suspended = false;
  const listeners = new Set();
  const notify = () => {
    for (const listener of listeners) {
      try { listener(); } catch { /* Observers cannot prevent settlement. */ }
    }
  };
  function ready() {
    if (disposed || suspended) throw failure("FONT_CLOSED", "Book font authoring is not active.");
    if (controls.sourceBusy) throw failure("BOOK_BUSY", "Finish importing or composing source before changing fonts.");
  }
  function current(job) {
    ready();
    if (pending !== job || job.abort.signal.aborted)
      throw failure("FONT_CANCELLED", "Font operation cancelled; previous assignments were kept.");
    if (job.revision !== collection.revision || job.checkpoint !== controls.checkpoint() || job.isCurrent() !== true)
      throw failure("STALE_SOURCE", "The book changed; review the fonts again before assigning them.");
  }
  function finish(job, error, result) {
    if (job.done) return;
    job.done = true;
    clearTimeout(job.timer);
    try { job.signal?.removeEventListener("abort", job.onAbort); } catch { /* Still release ownership. */ }
    if (pending === job) pending = null;
    if (error) { job.abort.abort(); job.reject(error); }
    else job.resolve(result);
    notify();
  }
  function cancel(message = "Font operation cancelled; previous assignments were kept.") {
    if (pending && !pending.installing) finish(pending, failure("FONT_CANCELLED", message));
  }
  function begin(work, { signal, isCurrent = () => true } = {}) {
    let job, ownsAdmission = false;
    try {
      ready(); signalValue(signal);
      if (typeof isCurrent !== "function") throw failure("INVALID_OPTIONS", "Font selection guard must be a function.");
      if (admitting || pending) throw failure("FONT_BUSY", "Another font operation is in progress.");
      if (signal?.aborted) throw failure("FONT_CANCELLED", "Font operation cancelled before starting.");
      admitting = true; ownsAdmission = true;
      // Capture may install typed source and notify collection observers. Do it
      // before reserving this operation's checkpoint, but reject reentrant starts.
      controls.captureProject();
      ready();
      job = { revision: collection.revision, checkpoint: controls.checkpoint(), signal, isCurrent,
        abort: new AbortController(), phase: "preparing", done: false, installing: false, timer: null };
      const promise = new Promise((resolve, reject) => { job.resolve = resolve; job.reject = reject; });
      pending = job;
      job.onAbort = () => {
        // A collection observer can abort while our synchronous commit notifies
        // it. Those bytes are already installed: report success, not rollback.
        if (!job.installing) finish(job, failure("FONT_CANCELLED", "Font operation cancelled; previous assignments were kept."));
      };
      try {
        signal?.addEventListener("abort", job.onAbort, { once: true });
        if (signal?.aborted) job.onAbort();
        if (!job.done) job.timer = setTimeout(() => finish(job,
          failure("FONT_TIMEOUT", "Font operation exceeded its deadline; previous assignments were kept.")), timeoutMs);
      } catch (error) { finish(job, error); }
      // Observe late I/O, dialogs and worker failures even after cancellation.
      // The host promise settles immediately; File.arrayBuffer itself cannot
      // be interrupted. Production worker cancellation does terminate Rust work.
      Promise.resolve().then(() => { current(job); return work(job); }).then(
        result => finish(job, null, result), error => finish(job, error),
      );
      return promise;
    } catch (error) { return Promise.reject(error); }
    finally { if (ownsAdmission) { admitting = false; notify(); } }
  }
  function phase(job, value) {
    current(job); job.phase = value; notify(); current(job);
  }
  function install(job, action) {
    current(job);
    // Only the notification synchronously produced by our own commit is
    // exempt from invalidation. The collection still enforces its revision.
    job.installing = true;
    try { return action(); } finally { job.installing = false; }
  }
  const unCollection = collection.subscribe(() => cancel("Book resources or source changed; the font operation was cancelled."));
  const unSource = controls.subscribeSourceState(() => {
    if (controls.sourceBusy) cancel("Import or text composition started; the font operation was cancelled.");
    notify();
  });
  return Object.freeze({
    get busy() { return admitting || pending !== null; },
    get phase() { return pending?.phase ?? "idle"; },
    subscribe(listener) {
      if (disposed) throw failure("FONT_CLOSED", "Book font authoring is disposed.");
      if (typeof listener !== "function") throw new TypeError("Font observer must be a function.");
      listeners.add(listener); return () => listeners.delete(listener);
    },
    assign(values, options) {
      // Copy roles/File references before asynchronous admission so callers
      // cannot retarget a batch by modifying its array while files are read.
      let selected;
      try { ready(); selected = selections(values, collection.fonts); }
      catch (error) { return Promise.reject(error); }
      return begin(async job => {
        phase(job, "reading");
        const reads = new Map(), assets = [];
        for (const selection of selected) {
          let asset = reads.get(selection.file);
          if (!asset) {
            asset = await readBookFont(selection.file, selection.slot, selection.weight);
            current(job); reads.set(selection.file, asset);
          }
          if (asset.bytes.length !== selection.size)
            throw failure("FONT_READ_FAILED", "A selected font changed size; no fonts were assigned.");
          assets.push({ ...asset, slot: selection.slot, weight: selection.weight });
        }
        phase(job, "validating");
        // Never transfer the private bytes that will be installed. The worker
        // also performs its normal input admission and private transport copies.
        const result = await worker.render([{ ...PROBE }], "pdf", {
          font: "sans", toc: false, pageNumbers: false, expandIncludes: false,
          fontAssets: assets.map(({ slot, weight, bytes }) => ({ slot, weight, bytes: new Uint8Array(bytes) })),
        }, { signal: job.abort.signal });
        current(job); checkProbe(result);
        install(job, () => collection.setFonts(assets, job.revision));
        return Object.freeze({ revision: job.revision + 1,
          assignments: Object.freeze(assets.map(({ slot, name, weight, bytes }) =>
            Object.freeze({ slot, name, weight, size: bytes.length }))) });
      }, options);
    },
    remove(slot, confirm, options) {
      if (!BOOK_FONT_SLOTS.includes(slot) || typeof confirm !== "function")
        return Promise.reject(failure("INVALID_OPTIONS", "Choose a font role and supply explicit removal confirmation."));
      return begin(async job => {
        const font = collection.fonts.find(font => font.slot === slot);
        if (!font) return false;
        phase(job, "confirming");
        const approved = await confirm(`Remove the supplied ${slot} font (${font.name})? The renderer will use its normal fallback. Source and the original file will not change. Save a portable backup first to retain this font.`);
        current(job);
        if (approved !== true) return false;
        install(job, () => collection.revokeFont(slot));
        return true;
      }, options);
    },
    clear(confirm, options) {
      if (typeof confirm !== "function")
        return Promise.reject(failure("INVALID_OPTIONS", "Supply explicit confirmation before revoking fonts."));
      return begin(async job => {
        if (!collection.fonts.length) return false;
        phase(job, "confirming");
        const approved = await confirm("Revoke all supplied book fonts? The renderer will use its normal fallback. Save a portable backup first to retain the font bytes. Source and original files will not change.");
        current(job);
        if (approved !== true) return false;
        install(job, () => collection.revokeFonts());
        return true;
      }, options);
    },
    cancel,
    suspend() { if (!disposed) { suspended = true; cancel("Font authoring suspended; no pending fonts were installed."); notify(); } },
    resume() { if (!disposed) { suspended = false; notify(); } },
    dispose() {
      if (disposed) return;
      disposed = true; cancel("Font authoring disposed; no pending fonts were installed.");
      unCollection(); unSource(); listeners.clear(); worker.dispose();
    },
  });
}
