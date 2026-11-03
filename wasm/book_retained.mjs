// Worker-private session reuse. Inputs are owned, normalized snapshots from the
// transport; only the native BookSession parses Markdown or resolves includes.
import { bookFontEmbeddingReceipt } from "./book_session.mjs";
const error = (code, message) => Object.assign(new Error(message), { code });
const methods = Object.freeze({ pdf: "renderPdf", epub: "renderEpub", site: "renderSite", links: "validateLinks" });
const fontFormats = new Set(["epub", "site", "preview", "chapter-preview"]);

function samePaths(left, right) {
  return left.length === right.length && left.every((file, i) => file.path === right[i].path);
}

function sameOptions(a, b) {
  for (const key of ["title", "author", "lang", "customCss", "toc", "pageNumbers", "font",
    "darkMode", "fontScale", "expandIncludes", "typography", "baseFontSize", "headingScale",
    "tableFontSize", "tocDepth", "fitToPages", "baselineGridPt", "codeLineNumbers", "metadataEpochSeconds",
    "htmlFontFormat", "fontEmbedding", "pdfA", "pdfAStrict"]) {
    if (a[key] !== b[key]) return false;
  }
  if (JSON.stringify(a.page) !== JSON.stringify(b.page)
      || JSON.stringify(a.running) !== JSON.stringify(b.running)) return false;
  // Source-delta captures share worker-owned resource arrays, not host views.
  // Identity proves unchanged bytes here and avoids rescanning every asset.
  const sameAssets = (left, right, keys) => left === right || (left.length === right.length && left.every((asset, i) => {
    const other = right[i];
    if (keys.some(key => asset[key] !== other[key]) || asset.bytes.length !== other.bytes.length)
      return false;
    for (let j = 0; j < asset.bytes.length; j++) if (asset.bytes[j] !== other.bytes[j]) return false;
    return true;
  }));
  return sameAssets(a.images, b.images, ["destination"])
    && sameAssets(a.fontAssets, b.fontAssets, ["slot", "weight"]);
}

// A facade can expose a method even when its older WASM binary does not. Only
// explicit capability errors permit rebuilding; failed transactions and invalid
// receipts must propagate, never turn into successful fallback publications.
function synchronize(session, before, input) {
  const sameSet = samePaths(before.files, input.files)
    && samePaths(before.options.includeSources, input.options.includeSources);
  const changes = [];
  if (sameSet) {
    for (const [old, next] of [[before.files, input.files],
      [before.options.includeSources, input.options.includeSources]]) {
      for (let i = 0; i < next.length; i++) if (old[i].source !== next[i].source) changes.push(next[i]);
    }
    if (!changes.length) return true;
  }
  const method = sameSet ? "updateSources" : "replaceSources";
  if (typeof session[method] !== "function") return false;
  try {
    const expectedRevision = session.sourceRevision;
    if (!Number.isInteger(expectedRevision) || expectedRevision < 0 || expectedRevision > 0xffffffff)
      throw error("INVALID_BOOK_UPDATE_REPORT", "Invalid retained book source revision.");
    if (sameSet) session.updateSources(changes, { expectedRevision });
    else session.replaceSources(input.files, { includeSources: input.options.includeSources, expectedRevision });
    return true;
  } catch (cause) {
    if (cause?.code === "UNSUPPORTED_BOOK_UPDATE"
        || (!sameSet && cause?.code === "UNSUPPORTED_BOOK_SOURCE_SET")) return false;
    throw cause;
  }
}

/** One retained native book shared by publications, HTML previews and link checks.
 * Each call receives the complete next source capture and presentation profile.
 * Source-only edits reuse native transactions. Settings or asset changes rebuild
 * explicitly, including revocation. Old edit APIs reconstruct from current input.
 * Nothing is queued or persisted; clear() retires both idle and pending captures.
 */
export function createRetainedBook(engine, renderPreview, renderChapterPreview) {
  if (typeof engine?.createBook !== "function" || typeof renderPreview !== "function")
    throw new TypeError("Retained books require the native book factory and existing preview codec.");
  let cached = null, busy = false, generation = 0;
  function clear() {
    generation++;
    const previous = cached;
    cached = null;
    previous?.session.dispose();
  }
  async function render(files, options, format = "preview", selected = 0) {
    if (busy) throw error("BOOK_BUSY", "A retained book is already rendering.");
    if (format !== "preview" && format !== "chapter-preview" && !Object.hasOwn(methods, format))
      throw error("INVALID_FORMAT", "Retained books support pdf, epub, site, preview, chapter-preview or links.");
    if (format === "chapter-preview") {
      if (typeof renderChapterPreview !== "function")
        throw error("UNSUPPORTED_BOOK_CHAPTER_PREVIEW", "Rebuild the matching worker for chapter previews.");
      if (!Number.isInteger(selected) || selected < 0 || selected >= files.length)
        throw error("INVALID_CHAPTER_INDEX", "Choose a chapter in the current book.");
    }
    busy = true;
    let ticket = generation;
    const input = { files, options };
    try {
      if (cached && (!sameOptions(cached.input.options, options)
          || !synchronize(cached.session, cached.input, input))) {
        clear();
        ticket = generation;
      }
      if (!cached) {
        const session = await engine.createBook(files, options);
        if (ticket !== generation) {
          session.dispose();
          throw error("EXPORT_CANCELLED", "Retained book was released during initialization.");
        }
        cached = { session, input };
      }
      const session = cached.session;
      if (format === "links" && typeof session.validateLinks !== "function")
        throw error("UNSUPPORTED_BOOK_LINKS", "Rebuild the matching native book for retained navigation checks.");
      // Native exporters still own every byte. Legacy preview keeps its full
      // ZIP contract; selected previews render one chapter on capable packages.
      // This raw adapter retains no output; the worker cache below owns wire bytes.
      const result = format === "chapter-preview"
        ? await renderChapterPreview(session, selected)
        : format === "preview"
          ? await renderPreview({ renderBookSite: () => session.renderSite() }, files, options)
          : await session[methods[format]]();
      if (ticket !== generation)
        throw error("EXPORT_CANCELLED", "Retained book was released during export.");
      if (format === "pdf" && options.pdfA === "2b"
          && (result.pdfA !== "2b" || result.pdfAStrict !== options.pdfAStrict))
        throw error("UNSUPPORTED_BOOK_PDFA", "The book renderer did not acknowledge the requested PDF/A emission policy.");
      bookFontEmbeddingReceipt(result,
        fontFormats.has(format) && options.fontEmbedding === "full" ? "full" : "subset");
      cached.input = input;
      return result;
    } catch (cause) {
      // Mutation may already have committed before rendering/protocol failure.
      // Retire the handle rather than reuse a mismatched source capture.
      try { clear(); } catch { /* Preserve the primary failure. */ }
      throw cause;
    } finally { busy = false; }
  }
  return Object.freeze({ render, clear });
}

// Additional retained OUTPUT payload, not a quota on the WASM heap, current
// input/resources, a caller's output copies, or transient exporter allocations.
export const BOOK_OUTPUT_CACHE_LIMITS = Object.freeze({
  maxBytes: 32 * 1024 * 1024,
  maxEntryBytes: 16 * 1024 * 1024,
  maxEntries: 64,
  maxSnapshots: 8,
  maxHistoryUnits: 8 * 1024 * 1024,
});
const optionKeys = new Set([
  "title", "author", "lang", "customCss", "toc", "pageNumbers", "font", "darkMode",
  "fontScale", "expandIncludes", "typography", "baseFontSize", "headingScale",
  "tableFontSize", "tocDepth", "fitToPages", "baselineGridPt", "codeLineNumbers", "metadataEpochSeconds",
  "htmlFontFormat", "fontEmbedding", "page", "running", "images", "fontAssets", "includeSources",
  "pdfA", "pdfAStrict",
]);
const sameSources = (a, b) => samePaths(a, b) && a.every((file, i) => file.source === b[i].source);
const copyBytes = bytes => {
  const owned = new Uint8Array(bytes.byteLength);
  owned.set(bytes);
  return owned;
};
function outputCacheLimits(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    throw error("INVALID_OPTIONS", "Output cache limits must be a plain data object.");
  const limits = { ...BOOK_OUTPUT_CACHE_LIMITS };
  for (const key of Reflect.ownKeys(value)) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!Object.hasOwn(limits, key) || !field || !Object.hasOwn(field, "value")
        || !Number.isSafeInteger(field.value) || field.value < 0
        || field.value > BOOK_OUTPUT_CACHE_LIMITS[key])
      throw error("INVALID_OPTIONS", "Output cache limits may only lower the documented ceilings.");
    limits[key] = field.value;
  }
  return limits;
}
function workerOutput(result, format, embedding) {
  const bytes = result?.bytes, sourceLength = result?.sourceLength;
  if (!(bytes instanceof Uint8Array)
      || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
      || !Number.isSafeInteger(sourceLength) || sourceLength < 0 || sourceLength > 64 * 1024 * 1024)
    throw error("WORKER_PROTOCOL_ERROR", "Invalid book output envelope.");
  if (!bytes.byteLength || bytes.byteLength > (format === "links" ? 4 : 128) * 1024 * 1024)
    throw error("OUTPUT_LIMIT", "Book output is empty or exceeds the output byte limit.");
  const previewMode = format === "chapter-preview" ? result.previewMode : undefined;
  if (format === "chapter-preview" && !["native-chapter", "site-fallback"].includes(previewMode))
    throw error("WORKER_PROTOCOL_ERROR", "Invalid chapter-preview capability receipt.");
  // BookOutput helpers close over their buffers. Do not retain the object or
  // its closures, and never return them with an unrelated cloned byte view.
  const archive = {};
  if (result.pdfA !== undefined || result.pdfAStrict !== undefined) {
    if (format !== "pdf" || result.pdfA !== "2b" || typeof result.pdfAStrict !== "boolean")
      throw error("WORKER_PROTOCOL_ERROR", "Invalid book PDF/A emission receipt.");
    archive.pdfA = result.pdfA; archive.pdfAStrict = result.pdfAStrict;
  }
  const fontReceipt = bookFontEmbeddingReceipt(result,
    fontFormats.has(format) && embedding === "full" ? "full" : "subset");
  return Object.freeze({ bytes, sourceLength, ...archive, ...fontReceipt,
    ...(previewMode === undefined ? {} : { previewMode }) });
}

/** Worker-only wire-output adapter over a retained renderer. The public native
 * BookOutput API is unchanged. Inputs MUST be complete, normalized, worker-owned
 * captures, as supplied by installBookWorker, not mutable caller asset views.
 * Only exact full-book equality permits reuse: includes/order/settings/resources
 * all participate. A bounded source-state history also reuses exact undo/redo
 * revisits. No Markdown, include graph, SVG or PDF interpretation occurs.
 * The existing full-preview codec can share the same cached native site export.
 */
export function createCachedBookRenderer(renderer, renderPreview, requestedLimits = {}) {
  if (typeof renderer?.render !== "function" || typeof renderer?.clear !== "function"
      || typeof renderPreview !== "function")
    throw new TypeError("The output cache requires an owned renderer and the existing preview codec.");
  const limits = outputCacheLimits(requestedLimits), outputs = new Map(), history = new Map();
  let input = null, current = null, bytes = 0, historyUnits = 0, busy = false, generation = 0;
  let hits = 0, misses = 0, evictions = 0;
  function discard() {
    outputs.clear(); history.clear(); current = null; input = null; bytes = 0; historyUnits = 0;
  }
  function clear() { generation++; discard(); renderer.clear(); }
  function evict(entry) {
    if (!outputs.delete(entry)) return;
    bytes -= entry.value.bytes.byteLength; entry.owner.entries.delete(entry.key); evictions++;
    if (!entry.owner.entries.size && history.delete(entry.owner)) historyUnits -= entry.owner.units;
  }
  function retire(bucket) {
    if (history.delete(bucket)) historyUnits -= bucket.units;
    for (const entry of bucket.entries.values()) evict(entry);
  }
  function sourceKey(files, includes) {
    const copy = list => Object.freeze(list.map(({ path, source }) => Object.freeze({ path, source })));
    const key = { files: copy(files), includes: copy(includes), entries: new Map(), units: 0 };
    for (const list of [key.files, key.includes]) for (const file of list)
      key.units += file.path.length + file.source.length;
    return key;
  }
  function selectSources(files, includes) {
    const matches = key => sameSources(key.files, files) && sameSources(key.includes, includes);
    if (current && matches(current)) return current;
    let found;
    for (const key of history.keys()) if (matches(key)) { found = key; break; }
    // Own small key containers before retiring anything. Immutable strings are
    // shared; no concatenated mega-key, probabilistic hash, or asset copy exists.
    const next = found ?? sourceKey(files, includes), previous = current;
    if (found) { history.delete(found); historyUnits -= found.units; }
    current = next;
    if (previous?.entries.size) {
      if (limits.maxSnapshots > 1 && limits.maxHistoryUnits && previous.units <= limits.maxHistoryUnits) {
        history.set(previous, true); historyUnits += previous.units;
      } else retire(previous);
    }
    while (history.size >= limits.maxSnapshots || historyUnits > limits.maxHistoryUnits)
      retire(history.keys().next().value);
    return next;
  }
  async function render(files, options, format = "preview", selected = 0) {
    if (busy) throw error("BOOK_BUSY", "A cached book is already rendering.");
    if (format !== "preview" && format !== "chapter-preview" && !Object.hasOwn(methods, format))
      throw error("INVALID_FORMAT", "Unsupported retained book format.");
    if (format === "chapter-preview" && (!Number.isInteger(selected) || selected < 0 || selected >= files.length))
      throw error("INVALID_CHAPTER_INDEX", "Choose a chapter in the current book.");
    busy = true;
    const ticket = generation;
    const fence = () => {
      if (ticket !== generation) throw error("EXPORT_CANCELLED", "Cached book was released during export.");
    };
    try {
      // History spans source edits only. Any presentation/resource change,
      // revocation or unknown future dependency retires ALL previous states.
      let cacheable = limits.maxSnapshots && limits.maxEntries && limits.maxBytes && limits.maxEntryBytes
        && Reflect.ownKeys(options).every(key => optionKeys.has(key));
      if (!cacheable || (input && !sameOptions(input.options, options))) discard();
      let bucket = null;
      if (cacheable) {
        try { bucket = selectSources(files, options.includeSources); }
        catch { discard(); cacheable = false; } // Key allocation is optional too.
      }
      async function load(kind, chapter, produce) {
        const key = kind === "chapter-preview" ? `${kind}:${chapter}` : kind;
        const old = bucket?.entries.get(key);
        if (old) {
          let owned;
          try { owned = copyBytes(old.value.bytes); } catch { evict(old); }
          if (owned) {
            outputs.delete(old); outputs.set(old, true); hits++;
            // Cache hits retain the same asynchronous cancellation boundary.
            await Promise.resolve(); fence();
            return Object.freeze({ ...old.value, bytes: owned });
          }
        }
        misses++;
        const output = workerOutput(await produce(), kind, options.fontEmbedding);
        fence();
        if (cacheable && output.bytes.byteLength <= limits.maxEntryBytes
            && output.bytes.byteLength <= limits.maxBytes) {
          let owned;
          // Admission is optional: allocation failure must not discard an
          // otherwise successful native export. Never retain a WASM-backed view.
          try { owned = copyBytes(output.bytes); } catch { /* Leave this result uncached. */ }
          if (owned) {
            while (outputs.size >= limits.maxEntries || owned.byteLength > limits.maxBytes - bytes)
              evict(outputs.keys().next().value);
            const entry = { owner: bucket, key, value: Object.freeze({ ...output, bytes: owned }) };
            bucket.entries.set(key, entry); outputs.set(entry, true); bytes += owned.byteLength;
          }
        }
        return output;
      }
      // A history hit need not mutate the native book back to the old state.
      // Every miss passes a COMPLETE capture to createRetainedBook, whose own
      // baseline tracks the actual native state, not transport request IDs.
      const result = await load(format, selected, () => format === "preview"
        ? renderPreview({ renderBookSite: () => load("site", 0,
          () => renderer.render(files, options, "site")) }, files, options)
        : renderer.render(files, options, format, selected));
      fence();
      input = { files, options };
      return result;
    } catch (cause) {
      try { clear(); } catch { /* Preserve the export/validation failure. */ }
      throw cause;
    } finally { busy = false; }
  }
  return Object.freeze({ render, clear,
    get cacheStats() { return Object.freeze({ entries: outputs.size, bytes, hits, misses, evictions,
      snapshots: history.size + Number(Boolean(current?.entries.size)), historyUnits }); },
  });
}
