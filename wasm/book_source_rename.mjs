// Source-only native refactoring. The Rust parser/expander owns every edit;
// this adapter admits data, checks the complete receipt and never parses Markdown.
import { bookLinkOptions, bookTextBytes, prepareBookInput } from "./book_session.mjs";

export const BOOK_SOURCE_RENAME_MAX_BYTES = 128 * 1024 * 1024;
const fail = (code, message) => Object.assign(new Error(message), { code });
const invalid = () => fail("INVALID_BOOK_SOURCE_RENAME", "The native source rename returned an invalid or mismatched source transaction.");

function path(value) {
  if (typeof value !== "string" || !value || value.startsWith("/") || value.includes("\\")
      || /^[A-Za-z][A-Za-z0-9+.-]*:/.test(value) || /[\x00-\x1f\x7f-\x9f]/.test(value)
      || value.split("/").some(part => !part || part === "." || part === "..")) {
    throw fail("INVALID_BOOK_RENAME_PATH", "Use a canonical relative book path without dot segments, backslashes or URL schemes.");
  }
  bookTextBytes(value, 4096);
  return value;
}

function imageKeys(value = []) {
  const count = Array.isArray(value) ? value.length : -1;
  if (!Number.isSafeInteger(count) || count < 0 || count > 4096) {
    throw fail("INVALID_BOOK_RENAME_IMAGES", "imageDestinations must contain at most 4096 supplied image keys.");
  }
  const result = [], seen = new Set(); let bytes = 0;
  // These are renderer keys, not source paths. Literal, root-relative and
  // encoded keys remain meaningful; only setImage's outer trim is applied.
  for (let index = 0; index < count; index++) {
    const key = value[index];
    if (typeof key !== "string" || !key.trim()) {
      throw fail("INVALID_BOOK_RENAME_IMAGES", "Each supplied image key must be a nonempty string.");
    }
    bytes += bookTextBytes(key, 4096);
    if (bytes > 16 * 1024 * 1024) throw fail("INVALID_BOOK_RENAME_IMAGES", "Supplied image keys exceed 16 MiB.");
    const destination = key.trim();
    if (seen.has(destination)) throw fail("INVALID_BOOK_RENAME_IMAGES", "Supplied image keys must be unique after trimming.");
    seen.add(destination); result.push(destination);
  }
  return Object.freeze(result);
}

/** Capture the entire graph and supplied image-key inventory before loading.
 * Image/font bytes, CSS and presentation getters remain outside this operation. */
export function prepareBookSourceRename(files, oldPath, newPath, options = {}) {
  oldPath = path(oldPath); newPath = path(newPath);
  const settings = bookLinkOptions(options);
  if (settings.expandIncludes === false) {
    throw fail("UNSUPPORTED_BOOK_RENAME_POLICY", "Source rename requires include expansion; enable it before refactoring the book.");
  }
  const imageDestinations = imageKeys(options.imageDestinations);
  const input = prepareBookInput(files, settings), known = new Set();
  for (const file of [...input.files, ...input.options.includeSources]) {
    path(file.path);
    if (known.has(file.path)) throw fail("DUPLICATE_BOOK_SOURCE", "Source rename requires unique chapter and include paths.");
    known.add(file.path);
  }
  if (!known.has(oldPath)) throw fail("UNKNOWN_BOOK_SOURCE", "The source to rename is not in this book.");
  if (newPath !== oldPath && known.has(newPath)) {
    throw fail("DUPLICATE_BOOK_SOURCE", "The new path already belongs to another source.");
  }
  return { ...input, options: Object.freeze({ ...input.options, imageDestinations }), oldPath, newPath };
}

function keys(value, expected) {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).length === expected.length && expected.every(key => Object.hasOwn(value, key));
}

/** Validate complete ordered chapter/include identities against the owned
 * request. No returned options, resource grants or arbitrary new files exist. */
export function parseBookSourceRename(bytes, input) {
  try {
    if (!(bytes instanceof Uint8Array) || !bytes.byteLength
        || bytes.byteLength > BOOK_SOURCE_RENAME_MAX_BYTES
        || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]") throw invalid();
    const report = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    if (!keys(report, ["schema", "oldPath", "newPath", "files", "includeSources", "changedSourceCount", "editCount"])
        || report.schema !== "fmd-book-source-rename-v1"
        || report.oldPath !== input.oldPath || report.newPath !== input.newPath
        || !Number.isSafeInteger(report.editCount) || report.editCount < 0 || report.editCount > 16384) throw invalid();
    let changed = 0, textChanges = 0, size = 0;
    const read = (values, before) => {
      if (!Array.isArray(values) || values.length !== before.length) throw invalid();
      return Object.freeze(values.map((file, i) => {
        const previous = before[i], target = previous.path === input.oldPath ? input.newPath : previous.path;
        if (!keys(file, ["path", "source"]) || file.path !== target || typeof file.source !== "string") throw invalid();
        for (const text of [file.path, file.source]) size += bookTextBytes(text, 64 * 1024 * 1024 - size);
        if (file.path !== previous.path || file.source !== previous.source) changed++;
        if (file.source !== previous.source) textChanges++;
        return Object.freeze({ path: file.path, source: file.source });
      }));
    };
    const files = read(report.files, input.files), includeSources = read(report.includeSources, input.options.includeSources);
    if (report.changedSourceCount !== changed || report.editCount < textChanges
        || (report.editCount === 0) !== (textChanges === 0)
        || (input.oldPath === input.newPath && (changed !== 0 || report.editCount !== 0))) throw invalid();
    return Object.freeze({ schema: report.schema, oldPath: report.oldPath, newPath: report.newPath,
      files, includeSources, changedSourceCount: changed, editCount: report.editCount });
  } catch { throw invalid(); }
}

/** loadRename returns the additive native function, never a compatibility
 * rewriter. Older packages fail before any engine refactoring or source change. */
export function createBookSourceRename(loadRename) {
  return async function renameBookSource(files, oldPath, newPath, options = {}) {
    const input = prepareBookSourceRename(files, oldPath, newPath, options);
    const rename = await loadRename();
    if (typeof rename !== "function" || rename.length < 8) {
      throw fail("UNSUPPORTED_BOOK_SOURCE_RENAME", "Rebuild the matching WASM package to rename sources and update their references.");
    }
    let json;
    try {
      json = rename(input.files.map(file => file.path), input.files.map(file => file.source),
        input.options.includeSources.map(file => file.path), input.options.includeSources.map(file => file.source),
        input.oldPath, input.newPath, input.options.profile, input.options.imageDestinations);
    } catch (error) {
      if (typeof error === "string") throw fail("BOOK_SOURCE_RENAME_FAILED", error.slice(0, 2048));
      throw error;
    }
    if (typeof json !== "string") throw invalid();
    bookTextBytes(json, BOOK_SOURCE_RENAME_MAX_BYTES);
    const bytes = new TextEncoder().encode(json), report = parseBookSourceRename(bytes, input);
    const sourceLength = [...input.files, ...input.options.includeSources]
      .reduce((size, file) => size + bookTextBytes(file.source), 0);
    return Object.freeze({ format: "book-source-rename", mimeType: "application/json", extension: "json",
      bytes, sourceLength, report, blob: () => new Blob([bytes], { type: "application/json" }) });
  };
}
