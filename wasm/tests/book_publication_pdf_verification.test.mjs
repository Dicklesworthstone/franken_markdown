import assert from "node:assert/strict";
import test from "node:test";
import { createBookPublicationClient } from "../book_publication.mjs";
import { BOOK_PDF_VERIFICATION_MAX_BYTES } from "../book_pdf_verification.mjs";

const files = [{ path: "./guide.md", source: "# Café" }];
const options = { includeSources: [{ path: "intro.md", source: "文" }],
  page: { size: "a4" }, pdfA: "2b", pdfAStrict: true, profile: "gfm-plus" };
const bytes = value => new TextEncoder().encode(JSON.stringify(value));
function report() {
  return {
    schema: "fmd-book-pdf-verification-v1", scope: "assembled-pdf-publication",
    layout: { schema_version: "1", target: "pdf", verdict: "clean", page_count: 1,
      digest: "fnv1a64:0123456789abcdef", anchors: { resolved: 0, unresolved: [] },
      findings: [], pages: [{ number: 1, runs: [] }] },
    navigation: { schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
      chapters: [{ path: "guide.md", checked: 0, external: 0, unchecked: 0, findings: [] }],
      summary: { chapters: 1, checked: 0, external: 0, unchecked: 0, findings: 0 } },
    destinations: [], pagination: { requestedOptimal: false, strategy: "greedy", fallback: null,
      paragraphVariants: { status: "not_requested", attemptedParagraphs: 0, limitedParagraphs: 0,
        offeredParagraphs: 0, selectedAlternatives: 0, paragraphItemLimit: false,
        retainedLineLimit: false, measurementWorkLimit: false, candidateStateLimit: false, failure: null } },
  };
}
function fixture({ change = () => {}, failures = 0 } = {}) {
  const calls = [], receives = [];
  let disposed = 0;
  const publisher = createBookPublicationClient({
    captureInput: (files, options) => structuredClone({ files, options }),
    createClient(config) {
      receives.push(config);
      let revision = 0;
      const output = format => {
        const result = { format: `book-${format}`, retainedInputRevision: ++revision,
          sourceLength: 10, bytes: Uint8Array.of(receives.length), profile: options.profile };
        if (format === "pdf" || format === "pdf-verification")
          Object.assign(result, { pdfA: "2b", pdfAStrict: true });
        if (format === "pdf-verification") {
          const pdfReport = report();
          Object.assign(result, { mimeType: "application/json", extension: "json" });
          if (failures-- > 0) throw Object.assign(new Error("Worker expired"), { code: "BOOK_CAPTURE_EXPIRED" });
          change(result, pdfReport);
          result.bytes = bytes(pdfReport);
        }
        return result;
      };
      return {
        render(captured, format, settings) {
          calls.push(format); assert.deepEqual(captured, files); assert.deepEqual(settings, options);
          return output(format);
        },
        renderSourceUpdate(delta, format, settings) {
          calls.push(format);
          assert.deepEqual(delta, []); assert.deepEqual(settings, { expectedRevision: revision });
          return output(format);
        },
        dispose() { disposed++; },
      };
    },
    checkLinks() { calls.push("preflight"); return { sourceLength: 10, report: report().navigation }; },
    checkRetainedLinks(client, {expectedRevision}) {
      assert.equal(expectedRevision, 4); calls.push("retained-links"); return report().navigation;
    },
    maxOutputBytes: 3,
  });
  return { publisher, calls, receives, disposed: () => disposed };
}

test("strict PDF waits for the retained writer and composes with the navigation gate", async () => {
  const h = fixture();
  const progress = [];
  const edition = await h.publisher.render(files, ["pdf", "epub", "site"], options,
    { strictPdf: true, strictLinks: true, onProgress: event => progress.push(event.phase) });
  assert.deepEqual(h.calls, ["preflight", "pdf", "epub", "site", "pdf-verification", "retained-links"]);
  assert.equal(h.receives[0].pdfReportLimit, BOOK_PDF_VERIFICATION_MAX_BYTES);
  assert.ok(progress.includes("verifying-pdf"));
  assert.equal(edition.byteLength, 3); assert.equal(edition.pdfReport.layout.page_count, 1);
  assert.equal(edition.linkReport.summary.findings, 0); assert.ok(Object.isFrozen(edition.pdfReport.layout.pages));
  assert.equal(h.disposed(), 1);
});

test("PDF-only findings refuse the entire edition and preserve the actual report", async () => {
  const h = fixture({ change(_result, value) {
    value.layout.verdict = "findings";
    value.layout.findings.push({ code: "overflow", detail: "Text extends outside the page." });
  } });
  await assert.rejects(h.publisher.render(files, ["epub", "pdf"], options, { strictPdf: true }),
    cause => cause.code === "BOOK_PDF_FINDINGS" && cause.pdfReport.layout.findings[0].code === "overflow"
      && cause.pdfReport.navigation.summary.findings === 0 && Object.isFrozen(cause.pdfReport));
  assert.equal(h.disposed(), 1); assert.equal(h.publisher.busy, false);
});

test("PDF gate rejects stale captures, incompatible settings and incomplete reports", async () => {
  for (const [expected, change] of [
    ["INVALID_BOOK_PDF_VERIFICATION", output => { output.retainedInputRevision = 1; }],
    ["INVALID_BOOK_PDF_VERIFICATION", output => { output.sourceLength--; }],
    ["UNSUPPORTED_BOOK_PDFA", output => { delete output.pdfAStrict; }],
    ["UNSUPPORTED_BOOK_PROFILE", output => { output.profile = "commonmark-gfm"; }],
    ["INVALID_BOOK_PDF_VERIFICATION", (_output, value) => { value.layout.profile = "commonmark-gfm"; }],
    ["INVALID_BOOK_PDF_VERIFICATION", (_output, value) => { value.navigation.chapters[0].path = "other.md"; }],
    ["INVALID_BOOK_PDF_VERIFICATION", (_output, value) => { value.layout.pages = []; value.layout.page_count = 0; }],
  ]) {
    const h = fixture({ change });
    await assert.rejects(h.publisher.render(files, ["pdf"], options, { strictPdf: true }), { code: expected });
    assert.equal(h.receives.length, 1); assert.equal(h.disposed(), 1);
  }
});

test("endpoint recovery re-renders every privately held format before accepting the report", async () => {
  const h = fixture({ failures: 1 });
  const edition = await h.publisher.render(files, ["pdf", "epub"], options, { strictPdf: true });
  assert.deepEqual(h.calls, ["pdf", "epub", "pdf-verification", "pdf", "epub", "pdf-verification"]);
  assert.ok(edition.outputs.every(output => output.bytes[0] === 2));
  assert.equal(h.disposed(), 2);
});

test("the default stays unchecked and an invalid strict request opens no worker", async () => {
  const h = fixture();
  await assert.rejects(h.publisher.render(files, ["site"], options, { strictPdf: true }), { code: "INVALID_OPTIONS" });
  await assert.rejects(h.publisher.render(files, ["pdf"], options, { strictPdf: 1 }), { code: "INVALID_OPTIONS" });
  assert.deepEqual(h.calls, []);
  const edition = await h.publisher.render(files, ["pdf"], options);
  assert.equal(edition.pdfReport, undefined); assert.deepEqual(h.calls, ["pdf"]);
});
