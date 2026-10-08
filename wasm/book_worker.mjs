// Worker transport only. The entrypoint injects the real Rust/WASM book API.
import { bookLinkOptions, prepareBookInput, parseBookChapterPreview } from "./book_session.mjs";
import { createRetainedBook } from "./book_retained.mjs";
export { createRetainedBook } from "./book_retained.mjs";

const LIMIT = 128 * 1024 * 1024;
const retainedFormats = new Set(["pdf", "epub", "site", "preview", "chapter-preview"]);
const formats = Object.freeze({
  pdf: ["renderBookPdf", "application/pdf", "pdf"],
  epub: ["renderBookEpub", "application/epub+zip", "epub"],
  site: ["renderBookSite", "application/zip", "zip"],
  preview: ["renderBookPreview", "application/json", "json"],
  "chapter-preview": ["renderBookSelectedPreview", "application/json", "json"],
  inspection: ["inspectBook", "application/json", "json"],
  links: ["checkBookLinks", "application/json", "json"],
});
export const bookError = (code, message) => Object.assign(new Error(message), { code });
function formatInfo(format) {
  if (!Object.hasOwn(formats, format))
    throw bookError("INVALID_FORMAT", "Choose pdf, epub, site, preview, chapter-preview, inspection or links.");
  return formats[format];
}

// Selection is an operation argument, not a presentation option or retained
// source identity. Validate it independently on both sides of the worker.
function chapterSelection(format, value, count = 4096) {
  if (format !== "chapter-preview") {
    if (value !== undefined) throw bookError("INVALID_OPTIONS", "selectedChapter requires chapter-preview format.");
    return undefined;
  }
  const selected = value === undefined ? 0 : value;
  if (!Number.isInteger(selected) || selected < 0 || selected >= count)
    throw bookError("INVALID_CHAPTER_INDEX", "Choose a chapter in the current book.");
  return selected === 0 ? 0 : selected;
}
function checkedChapterPreview(result, selected, count) {
  if (!["native-chapter", "site-fallback"].includes(result.previewMode))
    throw bookError("WORKER_PROTOCOL_ERROR", "Invalid chapter-preview capability receipt.");
  parseBookChapterPreview(result.bytes, selected, count);
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

// Source-only worker transport admission. This revision names a successful
// worker input capture, NOT the native BookSession's u32 source revision.
function deltaRecord(value, keys) {
  const invalid = () => bookError("INVALID_BOOK_SOURCE_DELTA",
    "Source deltas need plain data records with only the documented fields.");
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) throw invalid();
  const own = Reflect.ownKeys(value);
  if (own.length !== keys.length || own.some(key => !keys.includes(key))) throw invalid();
  const result = {};
  for (const key of keys) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!field || !Object.hasOwn(field, "value")) throw invalid();
    result[key] = field.value;
  }
  return result;
}

/** Snapshot only changed chapter/include strings, with no asset/options access.
 * Empty changes re-export the captured book in another supported format.
 * Membership, order, settings and resource authority cannot change here.
 */
function prepareBookSourceUpdate(files, options) {
  const { expectedRevision } = deltaRecord(options, ["expectedRevision"]);
  if (!Number.isSafeInteger(expectedRevision) || expectedRevision < 1) {
    throw bookError("INVALID_BOOK_SOURCE_DELTA", "Use a positive retainedInputRevision from this worker.");
  }
  const count = Array.isArray(files) ? Object.getOwnPropertyDescriptor(files, "length")?.value : -1;
  if (!Number.isInteger(count) || count < 0 || count > 4096) {
    throw bookError("INVALID_BOOK_SOURCE_DELTA", "Use at most 4096 source changes.");
  }
  const owned = [], seen = new Set();
  for (let index = 0; index < count; index++) {
    const field = Object.getOwnPropertyDescriptor(files, String(index));
    if (!field || !Object.hasOwn(field, "value")) {
      throw bookError("INVALID_BOOK_SOURCE_DELTA", "Source changes must not contain holes or accessors.");
    }
    const file = deltaRecord(field.value, ["path", "source"]);
    if (typeof file.path !== "string" || typeof file.source !== "string") {
      throw bookError("INVALID_BOOK_SOURCE_DELTA", "Source changes need string path and source fields.");
    }
    if (seen.has(file.path)) throw bookError("DUPLICATE_BOOK_SOURCE", "Change each source path at most once.");
    seen.add(file.path);
    owned.push(file);
  }
  // Reuse the same UTF-8/count/aggregate policy as full book admission.
  if (owned.length) prepareBookInput(owned);
  return { expectedRevision, files: owned };
}

/** Worker-private reconstruction from an already admitted successful capture.
 * Validate the COMPLETE resulting source budget before invoking native code.
 * Resource arrays are owned by this worker and reused, never sent by the patch.
 * The old source capture is not mutated, even if validation or rendering fails.
 */
export function applyBookSourceUpdate(input, value) {
  const update = deltaRecord(value, ["expectedRevision", "files"]);
  const prepared = prepareBookSourceUpdate(update.files, { expectedRevision: update.expectedRevision });
  const known = new Set([...input.files, ...input.options.includeSources].map(file => file.path));
  for (const file of prepared.files) {
    if (!known.has(file.path)) {
      throw bookError("UNKNOWN_BOOK_SOURCE", "Source deltas must use exact paths from the retained capture; use render() to replace the source set.");
    }
  }
  const changes = new Map(prepared.files.map(file => [file.path, file.source]));
  const replace = files => files.map(file => ({ path: file.path,
    source: changes.has(file.path) ? changes.get(file.path) : file.source }));
  // Already-admitted assets need neither reauthorization nor a second copy.
  // Presentation options still pass the shared normalizer; only bytes are reused.
  const next = prepareBookInput(replace(input.files), { ...input.options,
    includeSources: replace(input.options.includeSources), images: [], fontAssets: [] });
  next.options.images = input.options.images;
  next.options.fontAssets = input.options.fontAssets;
  return next;
}


// Immutable strings may be shared, but mutable input containers must not escape
// into the idle cache through a Worker-compatible embedding's postMessage hook.
function captureSources(input) {
  const copy = files => files.map(({ path, source }) => ({ path, source }));
  return { files: copy(input.files), includeSources: copy(input.options.includeSources) };
}
function updateCapturedSources(before, changes) {
  const values = new Map(changes.map(file => [file.path, file.source]));
  const apply = files => files.map(file => ({ path: file.path,
    source: values.has(file.path) ? values.get(file.path) : file.source }));
  return { files: apply(before.files), includeSources: apply(before.includeSources) };
}
function prepareSourceCapture(files, options) {
  const keys = options && Object.hasOwn(options, "includeSources")
    ? ["expectedRevision", "includeSources"] : ["expectedRevision"];
  const value = deltaRecord(options, keys);
  const { expectedRevision } = prepareBookSourceUpdate([], { expectedRevision: value.expectedRevision });
  // The complete next source set is admitted before comparing to the idle
  // baseline. This path has no image, font or presentation-option access.
  const input = prepareBookInput(files, { includeSources: value.includeSources ?? [] });
  return { expectedRevision, sources: captureSources(input) };
}
function sourceChanges(before, after) {
  const changes = [];
  for (const key of ["files", "includeSources"]) {
    const old = before[key], next = after[key];
    if (old.length !== next.length || next.some((file, i) => file.path !== old[i].path)) {
      throw bookError("BOOK_SOURCE_SET_CHANGED", "Source membership or order changed; render the complete snapshot instead.");
    }
    for (let i = 0; i < next.length; i++) if (next[i].source !== old[i].source) changes.push(next[i]);
  }
  return changes;
}

/** One worker per export. Cancellation terminates synchronous Rust work, not
 * merely its Promise. Optional retention shares one idle native book across
 * publishing formats; the default remains one-shot. Nothing is queued.
 */
export function createBookWorkerClient({
  workerFactory,
  timeoutMs = 120000,
  maxOutputBytes = LIMIT,
  retainPreview = false,
  retainBook = false,
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
    typeof retainPreview !== "boolean" || typeof retainBook !== "boolean" ||
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
  function keepIdle(worker, format, inputRevision, sources) {
    if (disposed || active || idle) { terminate(worker); return; }
    const entry = { worker, format, inputRevision, sources, timer: null, failed: null };
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
  async function run(files, format, options = {}, { signal, selectedChapter } = {}, sourceOnly = false) {
    alive();
    if (active) throw bookError("BOOK_BUSY", "A book export is already running.");
    const [, mimeType, extension] = formatInfo(format);
    const selected = chapterSelection(format, selectedChapter);
    const retainingBook = retainBook && retainedFormats.has(format);
    const retainingPreview = !retainingBook && retainPreview && ["preview", "chapter-preview"].includes(format);
    const retaining = retainingBook || retainingPreview;
    if (sourceOnly && !retaining)
      throw bookError("UNSUPPORTED_BOOK_SOURCE_DELTA", "Source deltas require a retained book or retained preview in this format.");
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
        retired = false,
        inputRevision = null,
        sources = null,
        chapterCount = null;
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
        if (keep && detached && !disposed) keepIdle(worker, format, inputRevision, sources);
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
          if (retainingBook && data.retainedBook !== true)
            throw bookError("WORKER_PROTOCOL_ERROR", "Rebuild the matching book worker for retained publications.");
          if (retainingPreview && data.retainedPreview !== true)
            throw bookError("WORKER_PROTOCOL_ERROR", "Rebuild the matching book worker for retained previews.");
          if (data.retainedInputRevision !== undefined) {
            if (!retaining || data.retainedInputRevision !== id)
              throw bookError("WORKER_PROTOCOL_ERROR", "Book worker acknowledged a different input capture.");
            inputRevision = id;
          }
          if (sourceOnly && inputRevision === null)
            throw bookError("WORKER_PROTOCOL_ERROR", "Book worker did not acknowledge the source delta.");
          checkedOutput(data.bytes, data.sourceLength, maxOutputBytes);
          if (selected !== undefined) {
            if (data.selectedChapter !== selected)
              throw bookError("WORKER_PROTOCOL_ERROR", "Book worker replied for a different chapter.");
            checkedChapterPreview(data, selected, chapterCount);
          }
          const bytes = data.bytes;
          finish(
            null,
            Object.freeze({
              format: `book-${format}`,
              bytes,
              sourceLength: data.sourceLength,
              ...(selected === undefined ? {} : { selectedChapter: selected, previewMode: data.previewMode }),
              ...(inputRevision === null ? {} : { retainedInputRevision: inputRevision }),
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
        if (sourceOnly) {
          // Validate before claiming the idle endpoint. A stale caller does not
          // destroy the newer capture, and Proxy reentry cannot steal a job.
          const capture = sourceOnly === 2 ? prepareSourceCapture(files, options) : null;
          const sourceUpdate = capture ? { expectedRevision: capture.expectedRevision }
            : prepareBookSourceUpdate(files, options);
          if (settled) return;
          if (!idle)
            throw bookError("BOOK_CAPTURE_EXPIRED", "The retained book was released; render the complete current snapshot first.");
          if (idle.inputRevision === null)
            throw bookError("UNSUPPORTED_BOOK_SOURCE_DELTA", "This worker does not support source deltas; use render() or rebuild the matching worker.");
          if (sourceUpdate.expectedRevision !== idle.inputRevision)
            throw bookError("STALE_BOOK_CAPTURE", "The retained input changed; refresh its revision before applying source changes.");
          if (capture) {
            sourceUpdate.files = sourceChanges(idle.sources, capture.sources);
            sources = capture.sources;
          } else sources = updateCapturedSources(idle.sources, sourceUpdate.files);
          if (selected !== undefined) chapterSelection(format, selected, sources.files.length);
          input = { sourceUpdate };
          worker = takeIdle();
          if (!worker)
            throw bookError("BOOK_CAPTURE_EXPIRED", "The retained endpoint could not be reclaimed; render the complete snapshot again.");
        } else if (!retaining) releaseIdle();
        else worker = takeIdle();
        if (settled) { retireWorker(); return; }
        signal?.addEventListener("abort", onAbort, { once: true });
        if (settled) return;
        if (signal?.aborted) {
          onAbort();
          return;
        }
        if (!sourceOnly) input = prepareBookInput(
          files,
          format === "inspection" ? {} : format === "links" ? bookLinkOptions(options) : options,
        );
        if (settled) return;
        if (signal?.aborted) {
          onAbort();
          return;
        }
        if (!sourceOnly && retaining) sources = captureSources(input);
        if (selected !== undefined) {
          chapterCount = (sourceOnly ? sources.files : input.files).length;
          chapterSelection(format, selected, chapterCount);
        }
        // Transfer only private copies, never detach caller-owned assets.
        transfer = sourceOnly ? [] : [...input.options.images, ...input.options.fontAssets].map(
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
          ...(selected === undefined ? {} : { selectedChapter: selected }),
          ...(retainingBook ? { retainBook: true } : retainingPreview ? { retainPreview: true } : {}) }, transfer);
      } catch (error) {
        finish(bookError("WORKER_FAILED", diagnostic(error).message));
      }
    });
  }
  return Object.freeze({
    // Protocol support, not a claim about the loaded native package.
    supportsChapterPreview: true,
    render: (files, format, options, request) => run(files, format, options, request),
    renderSourceUpdate: (files, format, options, request) => run(files, format, options, request, true),
    renderSources: (files, format, options, request) => run(files, format, options, request, 2),
    cancel,
    cancelPending,
    get hasRetainedPreview() { return ["preview", "chapter-preview"].includes(idle?.format); },
    get hasRetainedBook() { return idle !== null; },
    get retainedInputRevision() { return idle?.inputRevision ?? null; },
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
  let busy = false, capture = null;
  function releaseCapture() {
    const previous = capture;
    capture = null;
    if (previous?.mode === "book") engine.clearRetainedBook?.();
    else if (previous?.mode === "preview") engine.clearRetainedBookPreview?.();
  }
  scope.addEventListener("message", async (event) => {
    const data = event.data;
    const envelope = { schemaVersion: 1, id: data?.id, format: data?.format };
    let owned = false;
    try {
      if (!data || data.schemaVersion !== 1 || !Number.isSafeInteger(data.id) || data.id < 1) {
        throw bookError("WORKER_PROTOCOL_ERROR", "Invalid book request envelope.");
      }
      let [method] = formatInfo(data.format);
      const selected = chapterSelection(data.format, data.selectedChapter);
      if (data.retainPreview !== undefined && typeof data.retainPreview !== "boolean")
        throw bookError("WORKER_PROTOCOL_ERROR", "Invalid preview retention request.");
      if (data.retainBook !== undefined && typeof data.retainBook !== "boolean")
        throw bookError("WORKER_PROTOCOL_ERROR", "Invalid book retention request.");
      if (data.retainBook && data.retainPreview)
        throw bookError("WORKER_PROTOCOL_ERROR", "Choose one book retention mode.");
      if (data.retainBook) {
        if (!retainedFormats.has(data.format) || typeof engine.renderRetainedBook !== "function")
          throw bookError("UNSUPPORTED_RETAINED_BOOK", "Rebuild the matching worker for retained publications.");
        method = "renderRetainedBook";
        envelope.retainedBook = true;
      } else if (data.retainPreview) {
        if (!["preview", "chapter-preview"].includes(data.format) || typeof engine.renderRetainedBookPreview !== "function")
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
      const mode = data.retainBook ? "book" : data.retainPreview ? "preview" : null;
      let input;
      if (Object.hasOwn(data, "sourceUpdate")) {
        if (!mode || Object.hasOwn(data, "files") || Object.hasOwn(data, "options"))
          throw bookError("WORKER_PROTOCOL_ERROR", "A source delta must not include a full snapshot or change resource authority.");
        if (!capture || capture.mode !== mode || data.sourceUpdate?.expectedRevision !== capture.id)
          throw bookError("STALE_BOOK_CAPTURE", "Source delta does not match the retained worker input.");
        if (data.id <= capture.id)
          throw bookError("STALE_BOOK_CAPTURE", "Source delta request IDs must advance the retained capture.");
        input = applyBookSourceUpdate(capture.input, data.sourceUpdate);
      } else {
        if (capture && capture.mode !== mode) releaseCapture();
        input = prepareBookInput(
          data.files,
          data.format === "inspection" ? {} : data.format === "links"
            ? bookLinkOptions(data.options) : data.options,
        );
      }
      if (selected !== undefined) chapterSelection(data.format, selected, input.files.length);
      const args = [input.files, input.options];
      if (data.retainBook) args.push(data.format);
      if (selected !== undefined) args.push(selected);
      const result = await engine[method](...args);
      checkedOutput(result.bytes, result.sourceLength, data.maxOutputBytes);
      if (selected !== undefined) {
        checkedChapterPreview(result, selected, input.files.length);
        envelope.selectedChapter = selected;
        envelope.previewMode = result.previewMode;
      }
      // Transfer only the returned view, never a larger backing WASM memory.
      const bytes = result.bytes.slice();
      if (mode) {
        // Publish the revision only after source admission, native export and
        // output validation all succeed. Keep one input, sharing its assets.
        capture = { id: data.id, mode, input };
        envelope.retainedInputRevision = data.id;
      }
      scope.postMessage({ ...envelope, bytes, sourceLength: result.sourceLength }, [bytes.buffer]);
    } catch (error) {
      const previousMode = owned ? capture?.mode : null;
      if (owned) {
        try { releaseCapture(); } catch { /* Report the primary failure. */ }
      }
      if (owned && data.retainBook && previousMode !== "book") {
        try { engine.clearRetainedBook?.(); } catch { /* Report the original failure. */ }
      } else if (owned && data.retainPreview && previousMode !== "preview") {
        try { engine.clearRetainedBookPreview?.(); } catch { /* Report the original failure. */ }
      }
      scope.postMessage({ ...envelope, error: diagnostic(error) });
    } finally {
      if (owned) busy = false;
    }
  });
}

/** Compatibility adapter for existing preview-only embeddings. */
export function createRetainedBookPreview(engine, renderPreview, renderChapterPreview) {
  const retained = createRetainedBook(engine, renderPreview, renderChapterPreview);
  return Object.freeze({
    render: (files, options, selected) => retained.render(files, options,
      selected === undefined ? "preview" : "chapter-preview", selected),
    clear: retained.clear,
  });
}

/** Workbench capture selection. Only scalar ownership/configuration fences live
 * here; source baselines belong to the worker client's expiring idle entry.
 * The host must advance renderConfigurationRevision for any setting, asset or
 * source membership/order change. Legacy adapters retain full snapshot behavior.
 * Returns a one-use task so all existing editor/output checkpoint fences remain
 * at their call sites, before and after the asynchronous renderer boundary.
 */
export function createBookCollectionCapture(collection, worker) {
  let previous = null, serial = 0;
  return function capture() {
    const revision = collection.revision, configuration = collection.renderConfigurationRevision;
    const expectedRevision = worker.retainedInputRevision;
    const sourceOnly = Number.isSafeInteger(configuration) && configuration >= 0
      && Number.isSafeInteger(expectedRevision) && expectedRevision > 0
      && previous?.configuration === configuration && previous?.transport === expectedRevision
      && typeof worker.renderSources === "function" && typeof collection.project === "function";
    const fence = () => {
      if (collection.revision !== revision || collection.renderConfigurationRevision !== configuration)
        throw bookError("STALE_SOURCE", "The book changed during publication capture; capture it again.");
    };
    let input;
    if (sourceOnly) {
      // project() is the collection's validated source-only serialization, not
      // snapshot(): no image/font store is read or cloned for an ordinary edit.
      const project = collection.project();
      if (!Array.isArray(project?.files) || project.files.length > 4096
          || project.files.some(file => !file || (file.role !== undefined
            && file.role !== "chapter" && file.role !== "include")))
        throw bookError("INVALID_PROJECT", "Publication needs a valid source-only project.");
      const files = [], includeSources = [];
      for (const file of project.files) {
        (file.role === "include" ? includeSources : files).push({ path: file.path, source: file.source });
      }
      input = prepareBookInput(files, { ...project.options, includeSources, images: [], fontAssets: [] });
    } else input = collection.snapshot();
    fence();
    let used = false;
    return {
      files: input.files, options: input.options,
      async render(format, request) {
        if (used) throw bookError("BOOK_CAPTURE_USED", "Capture the current book again before another publication.");
        used = true;
        const ticket = ++serial;
        fence();
        try {
          const result = sourceOnly
            ? await worker.renderSources(input.files, format, {
              includeSources: input.options.includeSources, expectedRevision }, request)
            : await worker.render(input.files, format, input.options, request);
          if (ticket === serial) {
            const transport = result.retainedInputRevision;
            previous = Number.isSafeInteger(transport) && transport > 0
              && worker.retainedInputRevision === transport
              && collection.revision === revision && collection.renderConfigurationRevision === configuration
              ? { configuration, transport } : null;
          }
          return result;
        } catch (error) {
          if (ticket === serial) previous = null;
          throw error;
        }
      },
    };
  };
}
