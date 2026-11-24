// Production facade with generated-binding doubles. These tests exercise
// dispatch, options and ownership; Rust tests separately prove native layout.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { SourceTextModule, SyntheticModule } from "node:vm";
import { normalizePdfVerificationOptions, PDF_VERIFICATION_REPORT_LIMIT,
  validatePdfVerificationReport } from "./render_typography.mjs";

const report = () => ({ schema_version: "1", target: "pdf", verdict: "findings", page_count: 1,
  digest: "fnv1a64:0123456789abcdef", anchors: { resolved: 2, unresolved: ["missing"] },
  findings: [{ code: "unresolved_anchor", detail: "Missing #missing" }],
  pages: [{ number: 1, runs: [{ text: "A complete report β", x: 64, y: 72, size: 13, kind: "paragraph" }] }],
});
function gate() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
async function fixture({ arity = 38, hold, output, failure, metadata = {} } = {}) {
  const calls = [];
  let inits = 0;
  const bindings = {
    default() { inits++; return hold?.promise ?? Promise.resolve(); },
    renderHtmlConfiguredAdvanced() {}, renderPdfConfiguredMulti() {}, renderSvgConfigured() {}, renderEpubConfigured() {},
    accessibilityAudit() { assert.fail("full verification must not fall back to the accessibility subset"); },
    capabilities() {}, documentStats() {}, searchIndex() {}, renderSemanticDiffHtml() {}, semanticDiff() {},
    sourceMetadata() { return JSON.stringify(metadata); },
    renderPdfConfiguredProfiled(...args) {
      calls.push({ method: "render", args });
      return { format: "pdf", mimeType: "application/pdf", extension: "pdf", sourceLength: new TextEncoder().encode(args[0]).length,
        bytes: new Uint8Array([37, 80, 68, 70]), diagnosticsJson: () => "[]", free() {} };
    },
  };
  Object.defineProperty(bindings.renderPdfConfiguredProfiled, "length", { value: 40 });
  if (arity !== null) {
    bindings.verifyPdfConfigured = (...args) => {
      calls.push({ method: "verify", args });
      if (failure) throw failure;
      return output === undefined ? JSON.stringify(report()) : typeof output === "function" ? output(args) : output;
    };
    Object.defineProperty(bindings.verifyPdfConfigured, "length", { value: arity });
  }
  const native = new SyntheticModule(Object.keys(bindings), function() {
    for (const [key, value] of Object.entries(bindings)) this.setExport(key, value);
  });
  const helpers = new Map();
  for (const file of ["pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"])
    helpers.set("./" + file, new SourceTextModule(await readFile(new URL(file, import.meta.url), "utf8")));
  const module = new SourceTextModule(await readFile(new URL("franken_markdown.js", import.meta.url), "utf8"));
  await module.link(name => name === "./pkg/franken_markdown.js" ? native : helpers.get(name));
  await module.evaluate();
  return { api: module.namespace, calls, get inits() { return inits; } };
}

test("full verification uses the exact configured PDF export argument conversion", async () => {
  const f = await fixture();
  const settings = { profile: "commonmark-gfm", font: "serif", darkMode: "disabled", title: "Title", author: "A β",
    metadataEpochSeconds: 1700000000, allowRawHtml: true, codeLineNumbers: true, pageNumbers: true,
    fontScale: "125%", baseFontSize: 13, headingScale: 1.4, tableFontSize: 10, lang: "fr", toc: true, tocDepth: 4,
    microtype: "expansion", typography: { homogeneous: true, themeSpacing: true, lineHeight: 1.4 },
    strictPagination: true, footnoteBacklinks: true, baselineGridPt: 12,
    page: { size: "a5", orientation: "landscape", margins: { topPt: 60, leftPt: 64 } },
    running: { header: { left: "{title}", rule: true, image: { dest: "logo", heightPt: 18 } },
      footer: { right: "{page}/{pages}" }, skipFirstPage: true },
    pdfImages: [{ destination: "logo", bytes: new Uint8Array([1, 2, 3]) }],
    fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([4, 5, 6]), weight: 600 }],
  };
  const source = "# A source\n\nA note[^a]\n\n[^a]: Detail\n";
  const checked = await f.api.verifyPdf(source, settings);
  await f.api.renderPdf(source, settings);
  const verified = f.calls.find(call => call.method === "verify").args;
  const exported = f.calls.find(call => call.method === "render").args;
  assert.equal(verified.length, 38);
  assert.deepEqual(verified, [...exported.slice(0, 36), ...exported.slice(38)],
    "verification must audit export's selected geometry, typography, metadata, profile and assets");
  assert.deepEqual(checked, { ...report(), profile: "commonmark-gfm" });
  assert.ok(Object.isFrozen(checked));
});

test("frontmatter inheritance and explicit overrides match export while fitting is forwarded", async () => {
  const f = await fixture({ metadata: { title: "Frontmatter", author: "Writer", lang: "fr", toc: true, tocDepth: 4 } });
  const options = { profile: "gfm-plus", title: "Override", toc: false, fitToPages: 1, splitFootnotes: true };
  await f.api.verifyPdf("source", options);
  await f.api.renderPdf("source", options);
  const [verify, render] = f.calls;
  assert.deepEqual(verify.args, [...render.args.slice(0, 36), ...render.args.slice(38)]);
  assert.equal(verify.args[3], "Override"); assert.equal(verify.args[4], "Writer");
  assert.equal(verify.args[22], "fr"); assert.equal(verify.args[23], false);
  assert.equal(verify.args[24], 4); assert.equal(verify.args[25], 1);
});

for (const warm of [false, true]) test(`verification captures source, options and exact asset ranges before ${warm ? "warm" : "cold"} init yields`, async () => {
  const hold = warm ? undefined : gate(), f = await fixture({ hold });
  if (warm) await f.api.init();
  const image = new Uint8Array([99, 1, 2, 99]), font = new Uint8Array([99, 3, 4, 99]);
  let source = "# Original β", reads = 0;
  const options = { profile: "commonmark-gfm", title: "Original", page: { size: "a4", margins: { leftPt: 64 } },
    running: { footer: { center: "{page}" } }, typography: { antiriver: true },
    pdfImages: [{ destination: "image", bytes: new DataView(image.buffer, 1, 2) }],
    fontAssets: [{ slot: "body-regular", bytes: font.subarray(1, 3) }] };
  const pending = f.api.verifyPdf({ toString() { reads++; return source; } }, options);
  source = "changed"; options.profile = "gfm-plus"; options.title = "Changed";
  options.page.margins.leftPt = 500; options.running.footer.center = "changed";
  options.typography.antiriver = false; options.pdfImages.length = 0; options.fontAssets.length = 0;
  image.fill(9); font.fill(9); hold?.resolve();
  const result = await pending, args = f.calls[0].args;
  assert.equal(reads, 1); assert.equal(args[0], "# Original β"); assert.equal(args[3], "Original");
  assert.deepEqual([...args[9]], [1, 2]); assert.deepEqual([...args[11]], [3, 4]);
  assert.ok(args[27].includes(64)); assert.ok(args[28].includes("{page}"));
  assert.equal(args[35], "antiriver"); assert.equal(result.profile, "commonmark-gfm");
  assert.equal(image.byteLength, 4); assert.equal(font.byteLength, 4);
});

test("verification refuses missing or truncated generated bindings before reading source/assets", async () => {
  for (const arity of [null, 37]) {
    const f = await fixture({ arity });
    const bytes = { get byteLength() { assert.fail("payload read"); } };
    await assert.rejects(f.api.verifyPdf({ toString() { assert.fail("source read"); } },
      { pdfImages: [{ destination: "image", bytes }] }), { code: "UNSUPPORTED_WASM_PACKAGE" });
    assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
  }
});

test("verification rejects cross-format/archival grants, malformed values and accessors before initialization", async () => {
  const f = await fixture();
  for (const options of [null, [], { pdfA: "off" }, { pdfA: "2b" }, { pdfAStrict: false }, { customCss: "body{}" },
    { fontEmbedding: "full" }, { profile: "gfm" }, { toc: "true" }, { optimalPagination: 1 }, { baseFontSize: NaN },
    { fitToPages: 0 }, { tocDepth: 7 }, { metadataEpochSeconds: -1 },
    { get profile() { assert.fail("profile getter"); } },
    { pdfImages: [{ destination: "image", get bytes() { assert.fail("bytes getter"); } }] },
    { pdfImages: [{ destination: { toString() { assert.fail("destination coercion"); } }, bytes: new Uint8Array([1]) }] }]) {
    await assert.rejects(f.api.verifyPdf("source", options), { code: "INVALID_OPTIONS" });
  }
  await assert.rejects(f.api.verifyPdf("source", { fitToPages: 1, baselineGridPt: 12 }), { code: "INVALID_OPTIONS" });
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test("verification enforces UTF-8, duplicate-resource and non-shared exact-view admission", async () => {
  const f = await fixture();
  await assert.rejects(f.api.verifyPdf("\ud800"), /unpaired surrogate/);
  await assert.rejects(f.api.verifyPdf("source", { author: "β".repeat(3000) }), /UTF-8 byte limit/);
  await assert.rejects(f.api.verifyPdf("source", { pdfImages: [
    { destination: "same", bytes: new Uint8Array([1]) }, { destination: "same", bytes: new Uint8Array([2]) },
  ] }), /unique/);
  await assert.rejects(f.api.verifyPdf("source", { pdfImages: [{ destination: "shared", bytes: new Uint8Array(new SharedArrayBuffer(4)) }] }));
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
  const inherited = await fixture({ metadata: { author: "β".repeat(3000) } });
  await assert.rejects(inherited.api.verifyPdf("source"), /UTF-8 byte limit/);
  assert.equal(inherited.calls.length, 0);
});

test("full verification preserves typed native strict-pagination refusal without a fallback", async () => {
  const f = await fixture({ failure: JSON.stringify({ code: "PAGINATION_INFEASIBLE", reason: "infeasible", message: "No exact plan" }) });
  await assert.rejects(f.api.verifyPdf("source", { strictPagination: true }),
    { code: "PAGINATION_INFEASIBLE", reason: "infeasible", message: "No exact plan" });
  assert.deepEqual(f.calls.map(call => call.method), ["verify"]);
});

test("verification rejects incomplete, contradictory and oversized reports", async () => {
  const bad = [null, [], { findings: [] }, { ...report(), verdict: "clean" }, { ...report(), page_count: 2 },
    { ...report(), digest: "other" }, { ...report(), anchors: { resolved: -1, unresolved: [] } },
    { ...report(), pages: [{ number: 2, runs: [] }] },
    { ...report(), pages: [{ number: 1, runs: [{ text: "text", x: null, y: 1, size: 13, kind: "paragraph" }] }] },
    { ...report(), profile: "commonmark-gfm" }];
  for (const value of bad) {
    const f = await fixture({ output: JSON.stringify(value) });
    await assert.rejects(f.api.verifyPdf("source", { profile: "gfm-plus" }), { code: "INVALID_WASM_RESPONSE" });
  }
  for (const output of ["{", 7]) {
    const f = await fixture({ output });
    await assert.rejects(f.api.verifyPdf("source"), { code: "INVALID_WASM_RESPONSE" });
  }
  const f = await fixture({ output: " ".repeat(PDF_VERIFICATION_REPORT_LIMIT + 1) });
  await assert.rejects(f.api.verifyPdf("source"), { code: "BUDGET_EXCEEDED" });
});

test("renderer instances expose full verification without changing legacy audit options", async () => {
  const f = await fixture(), renderer = await f.api.createRenderer();
  assert.deepEqual(await renderer.verifyPdf("source"), report());
  assert.equal(Object.hasOwn(await renderer.verifyPdf("source"), "profile"), false);
  assert.equal(normalizePdfVerificationOptions({}).profile, undefined);
  assert.deepEqual(validatePdfVerificationReport(report()), report());
});
