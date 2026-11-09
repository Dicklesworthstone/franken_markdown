import { BOOK_FONT_LIMITS, BOOK_FONT_SLOTS, captureBookFont, readCapturedBookFont } from "./book_font_assets.mjs";

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
  const files = new Map();
  // Capture by index, not a caller-controlled iterator. Admit the FINAL store
  // before any file reads so replacing at capacity does not require headroom.
  for (let i = 0; i < values.length; i++) {
    const { slot, file, weight, faceIndex } = values[i] ?? {};
    if (!BOOK_FONT_SLOTS.includes(slot) || seen.has(slot))
      throw failure("INVALID_FONT_SLOT", "Choose each supported font role at most once.");
    const selection = captureBookFont(file, slot, weight, faceIndex);
    // Collection size does not predict its selected standalone face size. Admit
    // the known minimum now and check the exact final store before PDF preflight.
    seen.add(slot); next.set(slot, faceIndex === undefined ? selection.size : 1);
    selected.push(selection); files.set(file, Math.max(files.get(file) ?? 0, selection.size));
  }
  if ([...next.values()].reduce((total, size) => total + size, 0) > BOOK_FONT_LIMITS.totalBytes)
    throw failure("FONT_LIMIT", "The resulting supplied font roles exceed 32 MiB.");
  if ([...files.values()].reduce((total, size) => total + size, 0) > BOOK_FONT_LIMITS.selectedFileBytes)
    throw failure("FONT_LIMIT", "Distinct selected font files exceed the 64 MiB read budget.");
  return selected;
}

function extractedFace(result, selection) {
  const { bytes, faceIndex, faceCount } = result ?? {};
  if (faceIndex !== selection.faceIndex || !Number.isInteger(faceCount) || faceCount < 1
      || faceCount > 1024 || faceIndex >= faceCount || !(bytes instanceof Uint8Array)
      || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
      || bytes.length < 12)
    throw failure("FONT_VALIDATION_FAILED", "The font worker did not return the selected collection face.");
  if (bytes.length > BOOK_FONT_LIMITS.faceBytes)
    throw failure("FONT_LIMIT", "The selected standalone face exceeds the 8 MiB per-role limit.");
  const signature = String.fromCharCode(...bytes.subarray(0, 4));
  if (!(bytes[0] === 0 && bytes[1] === 1 && bytes[2] === 0 && bytes[3] === 0)
      && signature !== "OTTO" && signature !== "true")
    throw failure("FONT_VALIDATION_FAILED", "The font worker returned a collection instead of a standalone font face.");
  const suffix = ` [face ${faceIndex}]`;
  const name = selection.name.slice(0, BOOK_FONT_LIMITS.nameUnits - suffix.length).replace(/[\uD800-\uDBFF]$/u, "") + suffix;
  // A custom worker adapter cannot mutate the bytes we will install after its
  // Promise settles, or detach them while the PDF validation request runs.
  return { name, bytes: new Uint8Array(bytes) };
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
      try {
        ready(); selected = selections(values, collection.fonts);
        if (selected.some(selection => selection.faceIndex !== undefined)
            && typeof worker.extractFontCollectionFace !== "function")
          throw failure("UNSUPPORTED_WASM_PACKAGE", "Font collections require the matching renderer and book worker package.");
      }
      catch (error) { return Promise.reject(error); }
      return begin(async job => {
        phase(job, "reading");
        const reads = new Map(), extracted = new Map(), assets = [];
        for (const selection of selected) {
          phase(job, "reading");
          let asset = reads.get(selection.file);
          if (!asset) {
            asset = await readCapturedBookFont(selection);
            current(job); reads.set(selection.file, asset);
          }
          if (asset.bytes.length !== selection.size)
            throw failure("FONT_READ_FAILED", "A selected font changed size; no fonts were assigned.");
          if (selection.faceIndex !== undefined) {
            let faces = extracted.get(selection.file);
            if (!faces) { faces = new Map(); extracted.set(selection.file, faces); }
            let face = faces.get(selection.faceIndex);
            if (!face) {
              phase(job, "extracting");
              const result = await worker.extractFontCollectionFace(new Uint8Array(asset.bytes), selection.faceIndex,
                { signal: job.abort.signal });
              current(job); face = extractedFace(result, selection); faces.set(selection.faceIndex, face);
            }
            assets.push({ ...face, slot: selection.slot, weight: selection.weight });
          } else {
            if (asset.bytes[0] === 0x74 && asset.bytes[1] === 0x74 && asset.bytes[2] === 0x63 && asset.bytes[3] === 0x66)
              throw failure("FONT_COLLECTION_FACE_INDEX", "This file contains a font collection; choose a zero-based face index for every assigned role.");
            assets.push({ slot: selection.slot, name: asset.name, bytes: asset.bytes, weight: selection.weight });
          }
        }
        reads.clear(); extracted.clear();
        const sizes = new Map(collection.fonts.map(font => [font.slot, font.size]));
        for (const asset of assets) sizes.set(asset.slot, asset.bytes.length);
        if ([...sizes.values()].reduce((total, size) => total + size, 0) > BOOK_FONT_LIMITS.totalBytes)
          throw failure("FONT_LIMIT", "The resulting supplied font roles exceed 32 MiB.");
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
