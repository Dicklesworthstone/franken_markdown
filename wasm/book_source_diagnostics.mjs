// Decode the Rust parser's retained diagnostics. This module never parses
// Markdown or guesses original include locations from expanded source offsets.
export const BOOK_SOURCE_DIAGNOSTICS_MAX_BYTES = 6 * 1024 * 1024;
const MAX_BASE_REPORT_BYTES = 4 * 1024 * 1024;
const MAX_PROVENANCE_BYTES = 2 * 1024 * 1024;
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

function location(value, budget) {
  // Older generated packages still provide useful expanded diagnostics.
  // Absence is preserved so a host can explicitly explain why source jumps
  // are unavailable instead of guessing that chapter offsets are editor offsets.
  if (value === undefined) return undefined;
  if (!value || typeof value !== "object" || Array.isArray(value)) throw invalid();
  if (value.status === "unavailable") {
    if (typeof value.reason !== "string" || !/^[a-z_]{1,64}$/.test(value.reason)
        || value.ranges !== undefined) throw invalid();
    return Object.freeze({ status: "unavailable", reason: value.reason });
  }
  if (value.status !== "mapped" || !Array.isArray(value.ranges)
      || !value.ranges.length || value.ranges.length > 64) throw invalid();
  budget.ranges = integer(budget.ranges + value.ranges.length, 16384);
  const ranges = value.ranges.map(range => {
    if (!range || typeof range !== "object" || Array.isArray(range)) throw invalid();
    const path = text(range.path, 4096);
    if (!path || /[\\\u0000-\u001f\u007f-\u009f]/.test(path)
        || /^[A-Za-z][A-Za-z0-9+.-]*:/.test(path)
        || path.split("/").some(part => !part || part === "." || part === "..")) throw invalid();
    budget.paths = integer(budget.paths + new TextEncoder().encode(path).length, 256 * 1024);
    const start = integer(range.start, MAX_SOURCE_BYTES), end = integer(range.end, MAX_SOURCE_BYTES);
    if (start > end || !["exact", "rewritten", "generated"].includes(range.kind)
        || (range.kind === "generated" && start !== end)) throw invalid();
    return Object.freeze({ path, start, end, kind: range.kind });
  });
  return Object.freeze({ status: "mapped", ranges: Object.freeze(ranges) });
}

/** Validate a complete report and optionally bind it to canonical chapter paths
 * in reading order. Bounds are UTF-8 offsets in the expanded/rebased chapter,
 * not editor offsets in an original include file. Additive source metadata
 * carries separately verified original ranges; older builds omit it.
 * Every returned record is
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
  const locationBudget = { ranges: 0, paths: 0 };
  let provenanceBytes = 0;
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
      const source = location(item.source, locationBudget);
      if (source !== undefined) {
        // Subtract only the canonical minimum encoding of the actual source
        // property. Extra whitespace/escaping stays charged to the base wire
        // allowance; provenance never grants extra space for base diagnostics.
        provenanceBytes = integer(provenanceBytes + 10
          + new TextEncoder().encode(JSON.stringify(item.source)).length, MAX_PROVENANCE_BYTES);
      }
      return Object.freeze({ code: item.code, severity: item.severity, start, end, message,
        ...(source === undefined ? {} : { source }) });
    });
    return Object.freeze({ path, sourceBytes, diagnostics: Object.freeze(diagnostics) });
  });
  if (!value.summary || value.summary.chapters !== chapters.length
      || value.summary.diagnostics !== count
      || bytes.byteLength - provenanceBytes > MAX_BASE_REPORT_BYTES) throw invalid();
  return Object.freeze({ schema: value.schema, coordinateSpace: value.coordinateSpace,
    chapters: Object.freeze(chapters),
    summary: Object.freeze({ chapters: chapters.length, diagnostics: count }) });
}

/** Bind original byte ranges to an immutable editor capture. Each used source
 * is walked at most once; no Markdown is parsed and no per-finding source scan
 * is performed. Returned offsets are UTF-16 in the original source. The editor
 * remains responsible for its ordinary CRLF-to-textarea normalization. */
export function bookDiagnosticSelections(report, files) {
  const known = new Map(files.map(file => [file.path, file]));
  if (known.size !== files.length) throw invalid();
  const groups = new Map(), selections = new Map();
  for (const chapter of report.chapters) {
    for (const finding of chapter.diagnostics) {
      if (finding.source?.status !== "mapped") continue;
      for (const range of finding.source.ranges) {
        const file = known.get(range.path);
        if (!file || typeof file.source !== "string") throw invalid();
        let group = groups.get(range.path);
        if (!group) groups.set(range.path, group = { file, ranges: [], points: new Set() });
        group.ranges.push(range);
        group.points.add(range.start);
        group.points.add(range.end);
      }
    }
  }
  for (const { file, ranges, points } of groups.values()) {
    const wanted = [...points].sort((a, b) => a - b), offsets = new Map();
    let bytes = 0, units = 0, next = 0;
    function boundary() {
      if (wanted[next] < bytes) throw invalid();
      if (wanted[next] === bytes) offsets.set(wanted[next++], units);
    }
    for (const scalar of file.source) {
      boundary();
      if (next === wanted.length) break;
      const code = scalar.codePointAt(0);
      if (code >= 0xd800 && code <= 0xdfff) throw invalid();
      bytes += code < 128 ? 1 : code < 2048 ? 2 : code < 65536 ? 3 : 4;
      units += scalar.length;
    }
    boundary();
    if (next !== wanted.length) throw invalid();
    for (const range of ranges) selections.set(range, Object.freeze({
      path: file.path, source: file.source, start: offsets.get(range.start), end: offsets.get(range.end),
    }));
  }
  return selections;
}
