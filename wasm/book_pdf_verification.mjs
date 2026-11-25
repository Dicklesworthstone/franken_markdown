// Admission for the complete native assembled-PDF report. Markdown, layout,
// destination binding and source provenance remain owned by the Rust renderer.
import { bookTextBytes, validateBookLinkReportBytes } from "./book_link_report.mjs";
import { validatePdfVerificationReport } from "./render_typography.mjs";

export const BOOK_PDF_VERIFICATION_MAX_BYTES = 8 * 1024 * 1024;
const invalid = () => Object.assign(new Error(
  "The book PDF verifier returned an invalid or mismatched publication report.",
), { code: "INVALID_BOOK_PDF_VERIFICATION" });

function text(value, maximum) {
  try { bookTextBytes(value, maximum); } catch { throw invalid(); }
  return value;
}

/** Match the native logical source-path identity, without decoding URLs or
 * accessing files. Host captures can contain dot segments or backslashes;
 * report chapter paths must be the canonical names produced by Rust. */
export function canonicalBookPdfVerificationPaths(paths) {
  if (!Array.isArray(paths) || !paths.length || paths.length > 4096) throw invalid();
  const seen = new Set();
  let sourceBytes = 0;
  return Object.freeze(paths.map(value => {
    text(value, 64 * 1024 * 1024);
    sourceBytes += bookTextBytes(value);
    if (sourceBytes > 64 * 1024 * 1024) throw invalid();
    if (!value || /^[\\/]/.test(value) || /^[A-Za-z][A-Za-z0-9+.-]*:/.test(value)
        || /[\x00-\x1f\x7f-\x9f]/.test(value)) throw invalid();
    const path = value.replaceAll("\\", "/"), parts = [];
    if (path.endsWith("/")) throw invalid();
    for (const part of path.split("/")) {
      if (part === "..") { if (!parts.length) throw invalid(); parts.pop(); }
      else if (part && part !== ".") parts.push(part);
    }
    const canonical = parts.join("/");
    text(canonical, 4096);
    if (!canonical || seen.has(canonical)) throw invalid();
    seen.add(canonical);
    return canonical;
  }));
}

function failure(value) {
  if (value === null) return null;
  if (!value || !["budget_exceeded", "infeasible", "invalid_input", "cost_overflow", "invalid_plan"].includes(value.code)
      || !text(value.detail, BOOK_PDF_VERIFICATION_MAX_BYTES)) throw invalid();
  return Object.freeze({ code: value.code, detail: value.detail });
}

function pagination(value) {
  if (!value || typeof value.requestedOptimal !== "boolean"
      || !["greedy", "exact_height", "legacy"].includes(value.strategy)) throw invalid();
  const fallback = failure(value.fallback), variants = value.paragraphVariants;
  if (fallback !== null && (!value.requestedOptimal || value.strategy === "exact_height")) throw invalid();
  if (!variants || !["not_requested", "unsupported_typography", "unprepared_footnotes",
    "continued_footnotes", "no_alternatives", "planned", "rejected"].includes(variants.status)) throw invalid();
  const checked = { status: variants.status };
  for (const key of ["attemptedParagraphs", "limitedParagraphs", "offeredParagraphs", "selectedAlternatives"]) {
    if (!Number.isSafeInteger(variants[key]) || variants[key] < 0) throw invalid();
    checked[key] = variants[key];
  }
  for (const key of ["paragraphItemLimit", "retainedLineLimit", "measurementWorkLimit", "candidateStateLimit"]) {
    if (typeof variants[key] !== "boolean") throw invalid();
    checked[key] = variants[key];
  }
  checked.failure = failure(variants.failure);
  return Object.freeze({ requestedOptimal: value.requestedOptimal, strategy: value.strategy,
    fallback, paragraphVariants: Object.freeze(checked) });
}

function layout(value) {
  validatePdfVerificationReport(value, "INVALID_BOOK_PDF_VERIFICATION");
  // Rebuild the documented native shape rather than recursively walking
  // arbitrary additive JSON supplied by an incompatible worker or package.
  const findings = value.findings.map(finding => Object.freeze({
    code: text(finding.code, BOOK_PDF_VERIFICATION_MAX_BYTES),
    detail: text(finding.detail, BOOK_PDF_VERIFICATION_MAX_BYTES),
  }));
  const pages = value.pages.map(page => Object.freeze({ number: page.number,
    runs: Object.freeze(page.runs.map(run => Object.freeze({
      text: text(run.text, BOOK_PDF_VERIFICATION_MAX_BYTES), x: run.x, y: run.y, size: run.size,
      kind: text(run.kind, BOOK_PDF_VERIFICATION_MAX_BYTES),
    }))) }));
  return Object.freeze({ schema_version: value.schema_version, target: value.target,
    verdict: value.verdict, page_count: value.page_count, digest: value.digest,
    anchors: Object.freeze({ resolved: value.anchors.resolved,
      unresolved: Object.freeze(value.anchors.unresolved.map(value => text(value, BOOK_PDF_VERIFICATION_MAX_BYTES))) }),
    findings: Object.freeze(findings), pages: Object.freeze(pages),
    ...(value.profile === undefined ? {} : { profile: value.profile }) });
}

function destinationSource(value, budget) {
  if (!value || !["destination", "block"].includes(value.precision)) throw invalid();
  const precision = value.precision;
  if (value.status === "unavailable") {
    if (typeof value.reason !== "string" || !/^[a-z_]{1,64}$/.test(value.reason)
        || value.ranges !== undefined) throw invalid();
    return Object.freeze({ status: value.status, precision, reason: value.reason });
  }
  if (value.status !== "mapped" || !Array.isArray(value.ranges)
      || !value.ranges.length || value.ranges.length > 64) throw invalid();
  const ranges = value.ranges.map(range => {
    if (!range || typeof range !== "object") throw invalid();
    const path = text(range.path, 4096);
    if (canonicalBookPdfVerificationPaths([path])[0] !== path) throw invalid();
    for (const coordinate of [range.start, range.end]) {
      if (!Number.isSafeInteger(coordinate) || coordinate < 0 || coordinate > 64 * 1024 * 1024) throw invalid();
    }
    if (range.start > range.end || !["exact", "rewritten", "generated"].includes(range.kind)
        || (range.kind === "generated" && range.start !== range.end)) throw invalid();
    // Match pdf_links::verification_references, whose retained source budget
    // counts path bytes plus 32 per range. The source-only HTML report has
    // different aggregate range/path/JSON allowances and cannot be reused.
    budget.provenance += bookTextBytes(path, 4096) + 32;
    if (budget.provenance > 2 * 1024 * 1024) throw invalid();
    return Object.freeze({ path, start: range.start, end: range.end, kind: range.kind });
  });
  return Object.freeze({ status: value.status, precision, ranges: Object.freeze(ranges) });
}

/** Decode a complete, bounded UTF-8 report. Optional expected paths bind it to
 * the caller's captured chapter order, after native-compatible normalization.
 * Returns immutable data and retains the native digest. A missing destination
 * stays a finding; malformed replies never turn into a clean/partial report.
 * This validates the wire contract, not PDF/A or PDF/UA conformance. */
export function parseBookPdfVerificationReport(bytes, expectedPaths) {
  try {
    if (!(bytes instanceof Uint8Array)
        || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
        || !bytes.byteLength || bytes.byteLength > BOOK_PDF_VERIFICATION_MAX_BYTES) throw invalid();
    const expected = expectedPaths === undefined ? undefined : canonicalBookPdfVerificationPaths(expectedPaths);
    const value = JSON.parse(new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes));
    if (!value || value.schema !== "fmd-book-pdf-verification-v1"
        || value.scope !== "assembled-pdf-publication") throw invalid();
    const checkedLayout = layout(value.layout);
    const navigation = validateBookLinkReportBytes(
      new TextEncoder().encode(JSON.stringify(value.navigation)), expected);
    const paths = navigation.chapters.map(chapter => chapter.path);
    if (canonicalBookPdfVerificationPaths(paths).some((path, index) => path !== paths[index])) throw invalid();
    const chapters = new Set(paths), budget = { references: 0, parsed: 0, provenance: 0 };
    if (!Array.isArray(value.destinations) || value.destinations.length > 16384) throw invalid();
    const destinations = value.destinations.map(item => {
      if (!item || !chapters.has(item.chapter)
          || !["link", "footnote", "footnote_backreference"].includes(item.kind)
          || !["resolved", "unresolved", "unchecked"].includes(item.status)) throw invalid();
      const destination = text(item.destination, 8192);
      const parsedDestination = item.parsedDestination === null ? null : text(item.parsedDestination, 8192);
      budget.references += bookTextBytes(item.chapter, 4096) + bookTextBytes(destination, 8192);
      budget.parsed += parsedDestination === null ? 0 : bookTextBytes(parsedDestination, 8192);
      if (budget.references > 1024 * 1024 || budget.parsed > 1024 * 1024) throw invalid();
      const source = destinationSource(item.source, budget);
      if (item.status === "resolved") {
        if (!Number.isSafeInteger(item.page) || item.page < 1 || item.page > checkedLayout.page_count
            || !Number.isFinite(item.y)) throw invalid();
      } else if (item.page !== null || item.y !== null) throw invalid();
      return Object.freeze({ chapter: item.chapter, destination, parsedDestination, kind: item.kind,
        source, status: item.status, page: item.page, y: item.y });
    });
    const resolved = destinations.filter(item => item.status === "resolved").length;
    const unresolved = [...new Set(destinations.filter(item => item.status === "unresolved")
      .map(item => `${item.chapter}: ${item.destination}`))];
    if (checkedLayout.anchors.resolved !== resolved
        || checkedLayout.anchors.unresolved.length !== unresolved.length
        || unresolved.some((target, index) => target !== checkedLayout.anchors.unresolved[index])
        || (checkedLayout.verdict === "clean" && (unresolved.length || navigation.summary.findings))) throw invalid();
    return Object.freeze({ schema: value.schema, scope: value.scope, layout: checkedLayout,
      navigation, destinations: Object.freeze(destinations), pagination: pagination(value.pagination) });
  } catch { throw invalid(); }
}
