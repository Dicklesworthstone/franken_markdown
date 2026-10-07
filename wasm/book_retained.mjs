// Worker-private session reuse. Inputs are owned, normalized snapshots from the
// transport; only the native BookSession parses Markdown or resolves includes.
const error = (code, message) => Object.assign(new Error(message), { code });
const methods = Object.freeze({ pdf: "renderPdf", epub: "renderEpub", site: "renderSite" });

function samePaths(left, right) {
  return left.length === right.length && left.every((file, i) => file.path === right[i].path);
}

function sameOptions(a, b) {
  for (const key of ["title", "author", "lang", "customCss", "toc", "pageNumbers", "font",
    "darkMode", "fontScale", "expandIncludes", "typography", "baseFontSize", "headingScale",
    "tableFontSize", "tocDepth", "fitToPages", "codeLineNumbers", "metadataEpochSeconds",
    "htmlFontFormat"]) {
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

/** One retained native book shared by PDF, EPUB, site and HTML-preview exports.
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
      throw error("INVALID_FORMAT", "Retained books support pdf, epub, site, preview or chapter-preview.");
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
      // Native exporters still own every byte. Legacy preview keeps its full
      // ZIP contract; selected previews render one chapter on capable packages.
      // No rendered HTML is retained between requests.
      const result = format === "chapter-preview"
        ? await renderChapterPreview(session, selected)
        : format === "preview"
          ? await renderPreview({ renderBookSite: () => session.renderSite() }, files, options)
          : await session[methods[format]]();
      if (ticket !== generation)
        throw error("EXPORT_CANCELLED", "Retained book was released during export.");
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
