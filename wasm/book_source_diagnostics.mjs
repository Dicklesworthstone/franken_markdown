// Decode the Rust parser's retained diagnostics. This module never parses
// Markdown or guesses original include locations from expanded source offsets.
export const BOOK_SOURCE_DIAGNOSTICS_MAX_BYTES = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES = 64 * 1024 * 1024;
const MAX_ITEMS = 4096;
const invalid = () => Object.assign(new Error(
  "The book parser returned an invalid or mismatched source diagnostic report.",
), { code: "INVALID_BOOK_SOURCE_DIAGNOSTICS" });

function integer(value, maximum) {
  if (!Number.isSafeInteger(value) || value < 0 || value > maximum) throw invalid();
  return value;
}
function text(value, maximum) {
  if (typeof value !== "string" || value.length > maximum) throw invalid();
  let bytes = 0;
  for (const scalar of value) {
    const code = scalar.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) throw invalid();
    bytes += code < 128 ? 1 : code < 2048 ? 2 : code < 65536 ? 3 : 4;
    if (bytes > maximum) throw invalid();
  }
  return value;
}

/** Validate a complete report and optionally bind it to canonical chapter paths
 * in reading order. Bounds are UTF-8 offsets in the expanded/rebased chapter,
 * not editor offsets in an original include file. Every returned record is
 * immutable; totals are verified against the actual diagnostic entries. */
export function parseBookSourceDiagnosticsReport(bytes, expectedPaths) {
  if (!(bytes instanceof Uint8Array)
      || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
      || !bytes.byteLength || bytes.byteLength > BOOK_SOURCE_DIAGNOSTICS_MAX_BYTES)
    throw invalid();
  if (expectedPaths !== undefined && (!Array.isArray(expectedPaths)
      || !expectedPaths.length || expectedPaths.length > MAX_ITEMS)) throw invalid();
  let value;
  try {
    value = JSON.parse(new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes));
  } catch { throw invalid(); }
  if (!value || value.schema !== "fmd-book-source-diagnostics-v1"
      || value.coordinateSpace !== "expanded_chapter"
      || !Array.isArray(value.chapters) || !value.chapters.length
      || value.chapters.length > MAX_ITEMS
      || (expectedPaths !== undefined && value.chapters.length !== expectedPaths.length))
    throw invalid();
  const seen = new Set();
  let count = 0, sourceBytesTotal = 0;
  const chapters = value.chapters.map((chapter, index) => {
    if (!chapter || typeof chapter !== "object" || Array.isArray(chapter)) throw invalid();
    const path = text(chapter.path, BOOK_SOURCE_DIAGNOSTICS_MAX_BYTES);
    if (!path || seen.has(path) || (expectedPaths !== undefined && path !== expectedPaths[index]))
      throw invalid();
    seen.add(path);
    const sourceBytes = integer(chapter.sourceBytes, MAX_SOURCE_BYTES);
    sourceBytesTotal = integer(sourceBytesTotal + sourceBytes, MAX_SOURCE_BYTES);
    if (!Array.isArray(chapter.diagnostics)) throw invalid();
    count = integer(count + chapter.diagnostics.length, MAX_ITEMS);
    const diagnostics = chapter.diagnostics.map(item => {
      if (!item || typeof item !== "object" || Array.isArray(item)
          || typeof item.code !== "string" || !/^[A-Za-z0-9_]{1,64}$/.test(item.code)
          || !["warning", "error"].includes(item.severity)) throw invalid();
      const start = integer(item.start, sourceBytes), end = integer(item.end, sourceBytes);
      const message = text(item.message, 2048);
      if (start > end || !message) throw invalid();
      return Object.freeze({ code: item.code, severity: item.severity, start, end, message });
    });
    return Object.freeze({ path, sourceBytes, diagnostics: Object.freeze(diagnostics) });
  });
  if (!value.summary || value.summary.chapters !== chapters.length
      || value.summary.diagnostics !== count) throw invalid();
  return Object.freeze({ schema: value.schema, coordinateSpace: value.coordinateSpace,
    chapters: Object.freeze(chapters),
    summary: Object.freeze({ chapters: chapters.length, diagnostics: count }) });
}
