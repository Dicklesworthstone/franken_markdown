// The production facade and decoder, with an explicit generated-binding double.
// Native and generated-WASM tests separately exercise actual PDF layout.
import assert from "node:assert/strict";
import test from "node:test";
import { createBookBindings } from "./book_session.mjs";
import { BOOK_PDF_VERIFICATION_MAX_BYTES, canonicalBookPdfVerificationPaths,
  parseBookPdfVerificationReport } from "./book_pdf_verification.mjs";

const encode = value => new TextEncoder().encode(JSON.stringify(value));
const FILES = [{ path: "guide/one.md", source: "# One\n\n[Next](../two.md#two)." },
  { path: "two.md", source: "# Two" }];
function report(paths = FILES.map(file => file.path)) {
  return {
    schema: "fmd-book-pdf-verification-v1", scope: "assembled-pdf-publication",
    layout: { schema_version: "1", target: "pdf", verdict: "findings", page_count: 2,
      digest: "fnv1a64:0123456789abcdef", anchors: { resolved: 1, unresolved: [`${paths[0]}: #missing`] },
      findings: [{ code: "unresolved_pdf_destination", detail: "guide/one.md: #missing" }],
      pages: [1, 2].map(number => ({ number, runs: [{ text: `Page ${number} β`, x: 64, y: 72, size: 12, kind: "paragraph" }] })) },
    navigation: { schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
      chapters: paths.map(path => ({ path, checked: 0, external: 0, unchecked: 0, findings: [] })),
      summary: { chapters: paths.length, checked: 0, external: 0, unchecked: 0, findings: 0 } },
    destinations: [
      { chapter: paths[0], destination: "/two.md#two", parsedDestination: "../two.md#two", kind: "link",
        source: { status: "mapped", precision: "destination", ranges: [{ path: "parts/body.md", start: 8, end: 21, kind: "rewritten" }] },
        status: "resolved", page: 2, y: 720 },
      { chapter: paths[0], destination: "#missing", parsedDestination: null, kind: "link",
        source: { status: "unavailable", reason: "metadata_limit", precision: "block" },
        status: "unresolved", page: null, y: null },
    ],
    pagination: { requestedOptimal: true, strategy: "exact_height", fallback: null,
      paragraphVariants: { status: "planned", attemptedParagraphs: 2, limitedParagraphs: 0,
        offeredParagraphs: 2, selectedAlternatives: 1, paragraphItemLimit: false,
        retainedLineLimit: false, measurementWorkLimit: false, candidateStateLimit: false, failure: null } },
  };
}

function fixture({ hold, old = false, result, failure } = {}) {
  const instances = [], calls = [];
  class NativeBook {
    constructor(paths, sources) {
      this.paths = [...paths]; this.sources = [...sources]; this.sourceRevision = 0;
      this.settings = {}; this.dead = false; instances.push(this);
    }
    static newWithProfile(paths, sources, profile) { const book = new this(paths, sources); book.settings.profile = profile; return book; }
    static fromSources(paths, sources, resourcePaths, resourceSources) {
      const book = new this(paths, sources); book.resources = resourcePaths.map((path, i) => ({ path, source: resourceSources[i] })); return book;
    }
    static fromSourcesProfiled(paths, sources, resourcePaths, resourceSources, profile) {
      const book = this.fromSources(paths, sources, resourcePaths, resourceSources); book.settings.profile = profile; return book;
    }
    get chapterCount() { return this.paths.length; }
    get sourceLength() { return new TextEncoder().encode([...this.sources, ...(this.resources ?? []).map(file => file.source)].join("")).length; }
    setAuthoringProfile(profile) { this.settings.profile = profile; }
    setMetadata(...values) { this.settings.metadata = values; }
    setCustomCss(css) { this.settings.css = css; }
    setTheme(...values) { this.settings.theme = values; }
    setNavigation(...values) { this.settings.navigation = values; }
    setPdfOptions(...values) { this.settings.pdf = values; }
    setBaselineGridPt(points) { this.settings.grid = points; }
    setImage(destination, bytes) { this.settings.image = [destination, [...bytes]]; }
    setFont(slot, bytes) { this.settings.font = [slot, [...bytes]]; }
    setFontWeight(slot, weight) { this.settings.weight = [slot, weight]; }
    renderPdfWithPage() { assert.fail("verification must not publish a PDF"); }
    renderPdfArchival() { assert.fail("verification must not publish a PDF"); }
    validateLinks() { assert.fail("verification must not substitute source-link validation"); }
    verifyPdf(profile, strict, geometry) {
      assert.equal(this.dead, false);
      calls.push({ profile, strict, geometry, settings: structuredClone(this.settings), sources: [...this.sources], resources: this.resources });
      if (failure) throw failure;
      return typeof result === "function" ? result(this) : result ?? JSON.stringify(report(canonicalBookPdfVerificationPaths(this.paths)));
    }
    updateSources() { assert.fail("unexpected source mutation"); }
    replaceSources(paths, sources, resourcePaths, resourceSources, expected) {
      assert.equal(expected, this.sourceRevision);
      this.paths = [...paths]; this.sources = [...sources];
      this.resources = resourcePaths.map((path, i) => ({ path, source: resourceSources[i] }));
      this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-set-v1", changed: true, revision: this.sourceRevision,
        chapter_count: paths.length, resource_count: resourcePaths.length, source_length: this.sourceLength,
        reparsed_chapter_count: paths.length });
    }
    free() { assert.equal(this.dead, false); this.dead = true; }
  }
  if (old) delete NativeBook.prototype.verifyPdf;
  const api = createBookBindings(async () => { await hold; return NativeBook; });
  return { api, instances, calls };
}

test("complete book PDF report preserves actual layout, source provenance and explicit unavailable locations", () => {
  const original = report(), parsed = parseBookPdfVerificationReport(encode(original), ["./guide/one.md", "a/../two.md"]);
  assert.deepEqual(parsed, original);
  assert.equal(parsed.layout.digest, original.layout.digest);
  assert.equal(parsed.navigation.summary.findings, 0, "HTML scope stays distinct from the PDF's dormant-note finding");
  assert.equal(parsed.layout.verdict, "findings");
  assert.equal(parsed.destinations[1].parsedDestination, null);
  assert.equal(parsed.destinations[1].source.precision, "block");
  assert.ok(Object.isFrozen(parsed.layout.pages[0].runs[0]));
  assert.ok(Object.isFrozen(parsed.destinations[0].source.ranges[0]));
  assert.ok(Object.isFrozen(parsed.pagination.paragraphVariants));
});

test("chapter identity uses the native logical path policy and preserves literal percent escapes", () => {
  assert.deepEqual(canonicalBookPdfVerificationPaths([".\\guide\\one.md", "part/../two%20words.md"]), ["guide/one.md", "two%20words.md"]);
  assert.deepEqual(canonicalBookPdfVerificationPaths([`${"./".repeat(2100)}one.md`]), ["one.md"]);
  for (const paths of [["/one.md"], ["../one.md"], ["a/../../one.md"], ["https:one.md"],
    ["one.md/"], ["one\u0085.md"], ["one.md", "./one.md"], [], ["."]]) {
    assert.throws(() => canonicalBookPdfVerificationPaths(paths), { code: "INVALID_BOOK_PDF_VERIFICATION" });
  }
});

test("incomplete, forged and inconsistent nested publication reports are rejected", () => {
  const mutations = [
    r => { r.scope = "expanded-html-navigation"; }, r => { delete r.layout; },
    r => { r.layout.verdict = "clean"; }, r => { r.layout.page_count = 3; },
    r => { r.layout.digest = "fake"; }, r => { r.layout.pages[0].runs[0].x = Infinity; },
    r => { r.layout.pages[0].runs[0].text = "\ud800"; },
    r => { r.layout.anchors.resolved = 2; }, r => { r.layout.anchors.unresolved = []; },
    r => { r.layout.findings = []; r.layout.verdict = "clean"; },
    r => { r.navigation.chapters.reverse(); }, r => { r.destinations[0].chapter = "other.md"; },
    r => { r.destinations[0].page = 3; }, r => { r.destinations[0].y = null; },
    r => { r.destinations[1].page = 1; }, r => { delete r.destinations[0].parsedDestination; },
    r => { delete r.destinations[0].source; }, r => { r.destinations[1].source.precision = "guessed"; },
    r => { r.destinations[0].source.ranges[0].start = 99; },
    r => { delete r.pagination.paragraphVariants.failure; },
    r => { r.pagination.paragraphVariants.selectedAlternatives = -1; },
    r => { r.pagination.paragraphVariants.candidateStateLimit = "false"; },
    r => { r.pagination.fallback = { code: "infeasible", detail: "cannot be exact with a fallback" }; },
  ];
  for (const mutate of mutations) {
    const value = report(); mutate(value);
    assert.throws(() => parseBookPdfVerificationReport(encode(value), FILES.map(file => file.path)),
      { code: "INVALID_BOOK_PDF_VERIFICATION" });
  }
  const fallback = report();
  fallback.pagination.strategy = "legacy";
  fallback.pagination.fallback = { code: "budget_exceeded", detail: "Retained bounded fallback" };
  assert.equal(parseBookPdfVerificationReport(encode(fallback)).pagination.fallback.code, "budget_exceeded");
});

test("wire admission rejects invalid UTF-8, shared buffers and oversized reports without truncation", () => {
  for (const bytes of [new Uint8Array(), new Uint8Array([0xff]),
    new Uint8Array(new SharedArrayBuffer(1)), new Uint8Array(BOOK_PDF_VERIFICATION_MAX_BYTES + 1)]) {
    assert.throws(() => parseBookPdfVerificationReport(bytes), { code: "INVALID_BOOK_PDF_VERIFICATION" });
  }
});

test("PDF provenance uses its native retained-range budget, not the smaller source-only path budget", () => {
  const path = `${"long".repeat(38)}.md`, value = report([path]);
  const destination = value.destinations[0];
  destination.source.ranges = [{ path, start: 0, end: 10, kind: "exact" }];
  value.destinations = Array.from({ length: 2000 }, () => structuredClone(destination));
  value.layout.anchors = { resolved: 2000, unresolved: [] };
  value.layout.findings = []; value.layout.verdict = "clean";
  const parsed = parseBookPdfVerificationReport(encode(value), [path]);
  assert.equal(parsed.destinations.length, 2000);
  assert.equal(parsed.destinations[1999].source.ranges[0].path, path);
  // More than the native retained-byte allowance must still be refused even
  // when compact JSON fits the enclosing 8 MiB wire cap.
  for (const item of value.destinations) item.source.ranges = Array.from({ length: 6 }, () => ({ path, start: 0, end: 10, kind: "exact" }));
  assert.ok(encode(value).byteLength < BOOK_PDF_VERIFICATION_MAX_BYTES);
  assert.throws(() => parseBookPdfVerificationReport(encode(value)), { code: "INVALID_BOOK_PDF_VERIFICATION" });
});

test("one-shot verification captures sources, dialect, assets and PDF options before initialization", async () => {
  let ready; const hold = new Promise(resolve => { ready = resolve; }), f = fixture({ hold });
  const files = structuredClone(FILES), image = new Uint8Array([99, 1, 2, 99]), font = new Uint8Array([99, 3, 4, 99]);
  const options = { profile: "commonmark-gfm", title: "Original", page: { size: "a4" },
    pdfA: "2b", pdfAStrict: true, baselineGridPt: 12, typography: "optimal-pagination",
    includeSources: [{ path: "parts/body.md", source: "Original include" }],
    images: [{ destination: "logo", bytes: image.subarray(1, 3) }],
    fontAssets: [{ slot: "body-regular", bytes: new DataView(font.buffer, 1, 2), weight: 600 }] };
  const pending = f.api.verifyBookPdf(files, options);
  files[0].source = "changed"; options.title = "changed"; options.page.size = "a5";
  options.includeSources[0].source = "changed"; image.fill(9); font.fill(9);
  ready(); const output = await pending;
  assert.deepEqual(f.calls[0].sources, FILES.map(file => file.source));
  assert.equal(f.calls[0].settings.metadata[0], "Original");
  assert.deepEqual(f.calls[0].settings.image, ["logo", [1, 2]]);
  assert.deepEqual(f.calls[0].settings.font, ["body-regular", [3, 4]]);
  assert.deepEqual(f.calls[0].settings.weight, ["body-regular", 600]);
  assert.equal(f.calls[0].resources[0].source, "Original include");
  assert.equal(f.calls[0].geometry[0], 210 * 72 / 25.4);
  assert.equal(f.calls[0].profile, "2b"); assert.equal(f.calls[0].strict, true);
  assert.equal(output.profile, "commonmark-gfm"); assert.equal(output.pdfA, "2b");
  assert.equal(output.pdfAStrict, true); assert.equal(output.format, "book-pdf-verification");
  assert.equal(output.blob().type, "application/json"); assert.equal(output.filename("proof"), "proof.json");
  assert.equal(f.instances[0].dead, true);
  assert.deepEqual(parseBookPdfVerificationReport(output.bytes), report());
});

test("export-local page and archival overrides preserve retained verification defaults", async () => {
  const f = fixture(), book = await f.api.createBook(FILES, { page: { size: "a4" }, pdfA: "2b", pdfAStrict: true });
  const before = book.verifyPdf();
  const override = book.verifyPdf({ page: {}, pdfA: "off" });
  const after = book.verifyPdf();
  assert.deepEqual([...f.calls[1].geometry], [612, 792, 72, 72, 72, 72]);
  assert.equal(f.calls[1].profile, "off"); assert.equal(f.calls[1].strict, false);
  assert.equal(override.pdfA, undefined);
  assert.deepEqual(f.calls[0], f.calls[2]); assert.deepEqual(before.bytes, after.bytes);
  assert.equal(book.sourceRevision, 0);
  for (const options of [{ profile: "gfm-plus" }, { pdfA: "off", pdfAStrict: true }, { get page() { assert.fail("getter invoked"); } }])
    assert.throws(() => book.verifyPdf(options));
  assert.equal(f.calls.length, 3); book.dispose();
  assert.throws(() => book.verifyPdf(), /disposed/);
});

test("verified source-set replacement updates the private chapter identity capture", async () => {
  const f = fixture(), book = await f.api.createBook(FILES);
  book.replaceSources([{ path: "./renamed.md", source: "# New" }], { expectedRevision: 0 });
  const parsed = parseBookPdfVerificationReport(book.verifyPdf().bytes, ["renamed.md"]);
  assert.equal(parsed.navigation.chapters[0].path, "renamed.md");
  assert.equal(book.sourceRevision, 1); book.dispose();
});

test("long admitted host path spellings are normalized before report identity checks", async () => {
  const f = fixture();
  const output = await f.api.verifyBookPdf([{ path: `${"./".repeat(2100)}one.md`, source: "# One" }]);
  assert.equal(parseBookPdfVerificationReport(output.bytes).navigation.chapters[0].path, "one.md");
});

test("older engines and publication failures never become source-only or default-layout success", async () => {
  const old = fixture({ old: true });
  await assert.rejects(old.api.verifyBookPdf(FILES), { code: "UNSUPPORTED_BOOK_PDF_VERIFICATION" });
  assert.equal(old.instances[0].dead, true); assert.equal(old.calls.length, 0);
  const refused = fixture({ failure: JSON.stringify({ code: "PAGINATION_INFEASIBLE", reason: "infeasible", message: "strict page plan is infeasible" }) });
  await assert.rejects(refused.api.verifyBookPdf(FILES), { code: "PAGINATION_INFEASIBLE" });
  assert.equal(refused.instances[0].dead, true); assert.equal(refused.calls.length, 1);
  for (const result of ["{}", JSON.stringify(report(["wrong.md", "two.md"])), "\ud800",
    "β".repeat(BOOK_PDF_VERIFICATION_MAX_BYTES / 2 + 1)]) {
    const f = fixture({ result });
    await assert.rejects(f.api.verifyBookPdf(FILES), { code: "INVALID_BOOK_PDF_VERIFICATION" });
    assert.equal(f.instances[0].dead, true);
  }
});

test("caller admission cannot invoke verification on a disposed native handle", async () => {
  const f = fixture(), book = await f.api.createBook(FILES);
  const options = new Proxy({}, { ownKeys() { book.dispose(); return []; } });
  assert.throws(() => book.verifyPdf(options), /disposed/);
  assert.equal(f.calls.length, 0);
});
