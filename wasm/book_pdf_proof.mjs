// A complete book PDF retained once for preview and exact-byte download.
// The host owns source/settings/resources; this session owns a dedicated book
// worker client. No Markdown/PDF parser, alternative renderer or network access.
const MAX_BYTES = 128 * 1024 * 1024;
const failure = (code, message) => Object.assign(new Error(message), { code });

function pdfResult(result, maximum, metadata) {
  const bytes = result?.bytes;
  if (result?.format !== "book-pdf" || result.mimeType !== "application/pdf"
      || result.extension !== "pdf" || !(bytes instanceof Uint8Array)
      || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
      || bytes.byteLength < 8 || bytes.byteLength > maximum
      || bytes[0] !== 37 || bytes[1] !== 80 || bytes[2] !== 68 || bytes[3] !== 70
      || bytes[4] !== 45 || ![49, 50].includes(bytes[5]) || bytes[6] !== 46
      || bytes[7] < 48 || bytes[7] > 57
      || !Number.isSafeInteger(result.sourceLength) || result.sourceLength < 0
      || result.sourceLength > 64 * 1024 * 1024) {
    throw failure("INVALID_BOOK_PDF", "The book worker did not return an admitted PDF result.");
  }
  // Do not call result.blob(): a host could close over subsequently mutated
  // bytes. Blob takes ownership of this exact view, including its byte offset.
  const blob = new Blob([bytes], { type: "application/pdf" });
  return Object.freeze({ ...metadata, blob, size: blob.size, sourceLength: result.sourceLength });
}

function filename(title) {
  return ((typeof title === "string" ? title : "")
    .replace(/[^a-z0-9_-]+/gi, "-").replace(/^-+|-+$/g, "").slice(0, 80) || "book") + ".pdf";
}

/** `worker` must be a dedicated createBookWorker() client. Its per-call signal
 * terminates only that operation; retiring an older proof cannot cancel a newer
 * render or any of the publisher's separate HTML/inspection/export workers.
 * The session never persists resources, creates object URLs or saves files.
 */
export function createBookPdfProof({ collection, controls, worker, maxOutputBytes = MAX_BYTES }) {
  for (const [object, methods] of [
    [collection, ["snapshot", "subscribe"]],
    [controls, ["captureProject", "checkpoint", "subscribeSourceState"]],
    [worker, ["render", "dispose"]],
  ]) {
    if (!object || methods.some(method => typeof object[method] !== "function"))
      throw new TypeError("Book PDF proofing requires collection, editor and dedicated worker adapters.");
  }
  if (!Number.isSafeInteger(maxOutputBytes) || maxOutputBytes < 8 || maxOutputBytes > MAX_BYTES)
    throw new TypeError("Invalid book PDF output limit.");

  let pending = null, retained = null, disposed = false, suspended = false;
  const listeners = new Set();
  const notify = () => {
    for (const listener of listeners) {
      try { listener(); } catch { /* Observers cannot keep ownership of a job. */ }
    }
  };
  function ready() {
    if (disposed || suspended) throw failure("PDF_PROOF_CLOSED", "Book PDF proofing is not active.");
    if (controls.sourceBusy) throw failure("BOOK_BUSY", "Finish importing or composing text before proofing the book.");
  }
  function matches(value) {
    return !disposed && !suspended && !controls.sourceBusy
      && collection.revision === value.revision && controls.checkpoint() === value.checkpoint;
  }
  function retire(reason) {
    retained = null;
    const operation = pending;
    pending = null;
    if (operation) {
      operation.removeAbort();
      // Settle our own promise even when a broken host ignores the signal.
      operation.reject(reason);
      operation.controller.abort();
    }
    notify();
  }
  function invalidate(message = "The book changed; generate a new PDF proof.") {
    retire(failure("STALE_SOURCE", message));
  }
  function current() {
    if (!retained) return null;
    try {
      if (matches(retained)) return retained.proof;
    } catch { /* A retired editor cannot authorize old output. */ }
    invalidate();
    return null;
  }
  const unsubscribe = collection.subscribe(() => {
    // captureProject synchronously installs the current raw editor text. It is
    // part of this capture, not a stale edit. Admission is reserved throughout;
    // the final checkpoint and snapshot are validated after capture returns.
    if (pending?.capturing) { retained = null; return; }
    invalidate();
  });
  const unsubscribeSource = controls.subscribeSourceState(() => {
    if (controls.sourceBusy) invalidate("Book import or text composition started; the PDF proof was retired.");
    else notify();
  });

  function render({ signal } = {}) {
    try {
      ready();
      if (pending) throw failure("BOOK_BUSY", "A book PDF proof is already rendering.");
      if (signal !== undefined && (!signal || typeof signal.aborted !== "boolean"
          || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
        throw failure("INVALID_OPTIONS", "Use an AbortSignal for PDF proof cancellation.");
      if (signal?.aborted) throw failure("EXPORT_CANCELLED", "Book PDF proof cancelled before starting.");
      const proof = current();
      if (proof) return Promise.resolve(proof);
    } catch (reason) { return Promise.reject(reason); }

    return new Promise((resolve, reject) => {
      const operation = {
        controller: new AbortController(), capturing: true,
        revision: null, checkpoint: null, reject, removeAbort: () => {},
      };
      pending = operation;
      const owns = () => pending === operation && matches(operation);
      function settle(reason, proof, failed = true) {
        if (pending !== operation) return;
        pending = null;
        operation.removeAbort();
        if (failed) {
          retained = null;
          operation.controller.abort();
          reject(reason);
        } else {
          retained = { revision: operation.revision, checkpoint: operation.checkpoint, proof };
          resolve(proof);
        }
        notify();
      }
      try {
        const abort = () => {
          if (pending === operation) retire(failure("EXPORT_CANCELLED", "Book PDF proof cancelled; source is unchanged."));
        };
        if (signal) {
          operation.removeAbort = () => signal.removeEventListener("abort", abort);
          signal.addEventListener("abort", abort, { once: true });
          if (signal.aborted) abort();
        }
        if (pending !== operation) return;
        controls.captureProject();
        operation.capturing = false;
        if (pending !== operation) return;
        ready();
        operation.revision = collection.revision;
        operation.checkpoint = controls.checkpoint();
        const input = collection.snapshot();
        if (!owns()) throw failure("STALE_SOURCE", "The book changed during PDF capture.");
        const metadata = { filename: filename(input.options.title), chapters: input.files.length,
          revision: operation.revision };
        notify();
        if (pending !== operation) return;
        // The production client copies/transfers the admitted book inputs and
        // kills synchronous WASM on abort. Observe both early and late failures.
        Promise.resolve(worker.render(input.files, "pdf", input.options,
          { signal: operation.controller.signal })).then(result => {
          if (pending !== operation) return;
          if (!owns()) throw failure("STALE_SOURCE", "The book changed while its PDF was rendering.");
          const proof = pdfResult(result, maxOutputBytes, metadata);
          if (!owns()) throw failure("STALE_SOURCE", "The book changed before its PDF was retained.");
          settle(null, proof, false);
        }, reason => { throw reason; }).catch(reason => settle(reason));
      } catch (reason) { settle(reason); }
    });
  }

  return Object.freeze({
    render,
    get pending() { return pending !== null; },
    get current() { return current(); },
    subscribe(listener) {
      if (disposed) throw failure("PDF_PROOF_CLOSED", "Book PDF proofing is closed.");
      if (typeof listener !== "function") throw new TypeError("Use a PDF proof listener function.");
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    invalidate,
    cancel() { retire(failure("EXPORT_CANCELLED", "Book PDF proof cancelled and released; source is unchanged.")); },
    suspend() {
      if (disposed) return;
      suspended = true;
      retire(failure("PDF_PROOF_CLOSED", "Book PDF proofing suspended; generate again after returning."));
    },
    resume() { if (!disposed) { suspended = false; notify(); } },
    dispose() {
      if (disposed) return;
      disposed = true;
      retire(failure("PDF_PROOF_CLOSED", "Book PDF proofing closed."));
      unsubscribe();
      unsubscribeSource();
      listeners.clear();
      worker.dispose();
    },
  });
}
