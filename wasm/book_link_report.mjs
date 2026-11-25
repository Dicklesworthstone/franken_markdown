// Shared bounded book-text and navigation-report admission. Independent of
// retained sessions and PDF verification so browser module imports stay acyclic.
import { createBookSourceLocationReader } from "./book_source_diagnostics.mjs";

const MAX_CHAPTERS = 4096;
const MAX_SOURCE_BYTES = 64 * 1024 * 1024;
export const BOOK_LINK_REPORT_MAX_BYTES = 6 * 1024 * 1024;

// Count without a lossy TextEncoder allocation: unpaired surrogates must never
// turn into U+FFFD at the WASM boundary. Kept independent of the 4 MiB flow API.
export function bookTextBytes(value, maximum = MAX_SOURCE_BYTES) {
  if (typeof value !== "string") throw new TypeError("book text must be a string");
  if (value.length > maximum) throw new RangeError("book text exceeds its UTF-8 budget");
  let bytes = 0;
  for (let i = 0; i < value.length; i++) {
    const c = value.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = value.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff))
        throw new TypeError("book text contains malformed Unicode");
      bytes += 4;
    } else if (c >= 0xdc00 && c <= 0xdfff)
      throw new TypeError("book text contains malformed Unicode");
    else bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : 3;
    if (bytes > maximum) throw new RangeError("book text exceeds its UTF-8 budget");
  }
  return bytes;
}

/** Decode a link report against canonical chapter paths from a captured host
 * collection. This validates the wire contract, not Markdown. Summary totals
 * are recomputed; no engine-supplied optimistic verdict reaches the reader. */
export function parseBookLinkReport(bytes, expectedPaths) {
  if (!Array.isArray(expectedPaths) || !expectedPaths.length || expectedPaths.length > MAX_CHAPTERS)
    throw invalidLinkReport();
  return validateBookLinkReportBytes(bytes, expectedPaths);
}

function invalidLinkReport() {
  return Object.assign(new Error("The book link checker returned an invalid or mismatched report."), {
    code: "INVALID_LINK_REPORT",
  });
}

/** Internal native/worker wire admission. The public report reader additionally
 * requires the host's chapter identities. Separate provenance cannot enlarge
 * the existing 4 MiB base-report budget, on either worker endpoint. */
export function validateBookLinkReportBytes(bytes, expectedPaths) {
  const invalid = invalidLinkReport;
  const maximum = BOOK_LINK_REPORT_MAX_BYTES;
  if (
    !(bytes instanceof Uint8Array) ||
    Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]" ||
    bytes.byteLength === 0 ||
    bytes.byteLength > maximum
  )
    throw invalid();
  const integer = (value) => {
    if (!Number.isSafeInteger(value) || value < 0 || value > 250000) throw invalid();
    return value;
  };
  const text = (value, limit) => {
    try {
      bookTextBytes(value, limit);
    } catch {
      throw invalid();
    }
    return value;
  };
  const codes = new Set([
    "missing_chapter",
    "missing_anchor",
    "ambiguous_anchor",
    "invalid_fragment",
    "invalid_local_destination",
    "missing_footnote",
  ]);
  let value;
  try {
    value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw invalid();
  }
  if (
    !value ||
    value.schema !== "fmd-book-link-report-v1" ||
    value.scope !== "expanded-html-navigation" ||
    !Array.isArray(value.chapters) || !value.chapters.length || value.chapters.length > MAX_CHAPTERS ||
    (expectedPaths !== undefined && value.chapters.length !== expectedPaths.length)
  )
    throw invalid();
  const seen = new Set(),
    summary = {
      chapters: value.chapters.length,
      checked: 0,
      external: 0,
      unchecked: 0,
      findings: 0,
    };
  const sourceReader = createBookSourceLocationReader(invalid, { link: true });
  const chapters = Array.from(value.chapters, (chapter, index) => {
    if (!chapter || typeof chapter !== "object") throw invalid();
    const path = text(chapter.path, 4096);
    if (!path || (expectedPaths !== undefined && path !== expectedPaths[index]) || seen.has(path)) throw invalid();
    seen.add(path);
    const checked = integer(chapter.checked),
      external = integer(chapter.external),
      unchecked = integer(chapter.unchecked);
    if (
      !Array.isArray(chapter.findings) ||
      chapter.findings.length > checked ||
      chapter.findings.length > 4096
    )
      throw invalid();
    const findings = Array.from(chapter.findings, (finding) => {
      if (!finding || !codes.has(finding.code)) throw invalid();
      const destination = text(finding.destination, 8192),
        message = text(finding.message, 1024);
      if (!message) throw invalid();
      const source = sourceReader.read(finding.source);
      return Object.freeze({ code: finding.code, destination, message,
        ...(source === undefined ? {} : { source }) });
    });
    for (const [key, count] of Object.entries({
      checked,
      external,
      unchecked,
      findings: findings.length,
    })) {
      summary[key] = integer(summary[key] + count);
    }
    if (summary.findings > 4096 || summary.checked + summary.external + summary.unchecked > 250000)
      throw invalid();
    return Object.freeze({ path, checked, external, unchecked, findings: Object.freeze(findings) });
  });
  if (bytes.byteLength - sourceReader.bytes > 4 * 1024 * 1024) throw invalid();
  return Object.freeze({
    schema: value.schema,
    scope: value.scope,
    chapters: Object.freeze(chapters),
    summary: Object.freeze(summary),
  });
}
