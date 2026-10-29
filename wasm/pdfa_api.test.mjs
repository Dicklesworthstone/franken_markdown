// Execute the production facade with generated-binding doubles. These tests
// prove admission/ABI ownership, not native rendering or PDF/A conformance.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { SourceTextModule, SyntheticModule } from "node:vm";
import { normalizePdfArchival, pdfArchivalReceipt } from "./render_typography.mjs";

const utf8 = value => new TextEncoder().encode(value);
const gate = () => {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
};
async function fixture({ archival = 39, hold, failure } = {}) {
  const calls = [];
  let inits = 0, frees = 0;
  const render = name => (...args) => {
    calls.push({ name, args });
    if (name === "archival" && failure) throw failure;
    return { format: "pdf", mimeType: "application/pdf", extension: "pdf",
      sourceLength: utf8(args[0]).length, bytes: utf8(`native-double:${name}`),
      diagnosticsJson: () => "[]", free() { frees++; } };
  };
  const bindings = {
    default() { inits++; return hold?.promise ?? Promise.resolve(); },
    renderPdfConfiguredMulti: render("ordinary"),
    renderPdfConfiguredPage: render("page"),
    renderPdfConfiguredGrid: render("grid"),
    renderPdfConfiguredRunning: render("running"),
    renderPdfConfiguredRunningImages: render("running-images"),
    renderBookPdf: render("book"),
    renderHtmlConfiguredAdvanced() {}, renderEpubConfigured() {}, renderSvgConfigured() {},
    accessibilityAudit() {}, capabilities() {}, documentStats() {}, searchIndex() {},
    renderSemanticDiffHtml() {}, semanticDiff() {},
  };
  if (archival !== null) {
    bindings.renderPdfConfiguredArchival = render("archival");
    Object.defineProperty(bindings.renderPdfConfiguredArchival, "length", { value: archival });
  }
  const native = new SyntheticModule(Object.keys(bindings), function() {
    for (const [name, value] of Object.entries(bindings)) this.setExport(name, value);
  });
  const helpers = new Map();
  for (const file of ["pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"])
    helpers.set(`./${file}`, new SourceTextModule(await readFile(new URL(file, import.meta.url), "utf8")));
  const module = new SourceTextModule(await readFile(new URL("franken_markdown.js", import.meta.url), "utf8"));
  await module.link(name => name === "./pkg/franken_markdown.js" ? native : helpers.get(name));
  await module.evaluate();
  return { api: module.namespace, calls, get inits() { return inits; }, get frees() { return frees; } };
}

test("single-document archival policy is strict, immutable and idempotent", () => {
  for (const [profile, strict] of [[undefined, undefined], ["off", undefined], ["off", false], [undefined, false]])
    assert.deepEqual(normalizePdfArchival(profile, strict), {});
  for (const strict of [false, true]) {
    const value = normalizePdfArchival("2b", strict);
    assert.deepEqual(value, { pdfA: "2b", pdfAStrict: strict });
    assert(Object.isFrozen(value));
    assert.deepEqual(normalizePdfArchival(value.pdfA, value.pdfAStrict), value);
  }
  assert.deepEqual(normalizePdfArchival("2b"), { pdfA: "2b", pdfAStrict: false });
});

for (const strict of [false, true]) test(`archival selection uses the full native ABI and acknowledges strict=${strict}`, async () => {
  const f = await fixture({ archival: 38 });
  const output = await f.api.renderPdf("# Source", { pdfA: "2b", pdfAStrict: strict });
  assert.deepEqual(f.calls.map(call => call.name), ["archival"]);
  assert.equal(f.calls[0].args.length, 38);
  assert.deepEqual(f.calls[0].args.slice(36), ["2b", strict]);
  assert.deepEqual(pdfArchivalReceipt(output, { pdfA: "2b", pdfAStrict: strict }),
    { pdfA: "2b", pdfAStrict: strict });
  assert.equal(output.text(), "native-double:archival");
  assert.equal(output.filename("report"), "report.pdf");
  assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
  assert(Object.isFrozen(output));
  assert.equal(f.frees, 1); assert.equal(f.inits, 1);
});

for (const options of [{}, { pdfA: "off" }, { pdfAStrict: false }, { pdfA: "off", pdfAStrict: false }])
  test(`ordinary requests preserve the old ABI and envelope: ${JSON.stringify(options)}`, async () => {
    const f = await fixture({ archival: null });
    const output = await f.api.renderPdf("source", options);
    assert.equal(f.calls[0].name, "ordinary"); assert.equal(f.calls[0].args.length, 27);
    assert.equal(Object.hasOwn(output, "pdfA"), false);
    assert.equal(Object.hasOwn(output, "pdfAStrict"), false);
  });

test("archival export captures all source, assets, metadata, typography, paper and running content before init yields", async () => {
  const hold = gate(), f = await fixture({ hold });
  const image = new Uint8Array([90, 1, 2, 91]), font = new Uint8Array([92, 3, 4, 93]);
  const options = { pdfA: "2b", pdfAStrict: true, title: "Before", author: "Author", lang: "fr",
    metadataEpochSeconds: 1700000000, font: "serif", fontScale: 1.2,
    page: { size: { widthPt: 720, heightPt: 540 }, margins: { topPt: 36, rightPt: 24, bottomPt: 30, leftPt: 18 } },
    running: { header: { left: "{title}", rule: true, image: { dest: "logo.png", position: "right", heightPt: 16 } },
      footer: { center: "{page}/{pages}" }, skipFirstPage: true },
    pdfImages: [{ destination: "logo.png", bytes: image.subarray(1, 3) }],
    fontAssets: [{ slot: "body-regular", bytes: font.subarray(1, 3), weight: 550 }],
    baseFontSize: 12, headingScale: 1.3, tableFontSize: 9, pageNumbers: true, codeLineNumbers: true,
    toc: true, tocDepth: 4, typography: { homogeneous: true, antiriver: true, pareto: true },
    optimalPagination: true, strictPagination: true, microtype: "expansion", baselineGridPt: 13.125 };
  let source = "# Before";
  const pending = f.api.renderPdf({ toString: () => source }, options);
  source = "changed"; options.title = "After"; options.pdfA = "off"; options.pdfAStrict = false;
  options.baselineGridPt = 14; options.page.size.widthPt = 900;
  options.typography.pareto = false; options.running.header.left = "changed";
  image.fill(8); font.fill(9); hold.resolve();
  const output = await pending, args = f.calls[0].args;
  assert.equal(output.pdfA, "2b"); assert.equal(output.pdfAStrict, true);
  assert.equal(args.length, 39); assert.equal(args[0], "# Before"); assert.equal(args[1], "serif");
  assert.equal(args[3], "Before"); assert.equal(args[4], "Author"); assert.equal(args[5], 1700000000);
  assert.equal(args[7], true); assert.deepEqual(args[8], ["logo.png"]);
  assert.deepEqual([...args[9]], [1, 2]); assert.deepEqual([...args[10]], [2]);
  assert.deepEqual([...args[11]], [3, 4]); assert.equal(args[16][0], 550);
  assert.deepEqual(args.slice(17, 26), [12, 1.3, 9, true, 1.2, "fr", true, 4, undefined]);
  assert.deepEqual([...args[27]], [720, 540, 36, 24, 30, 18]);
  assert.deepEqual(args[28], ["{title}", "", "", "", "{page}/{pages}", ""]);
  assert.deepEqual(args.slice(29, 32), [true, false, true]);
  assert.deepEqual(args[32], ["logo.png", ""]);
  assert.match(args[35], /homogeneous,antiriver,pareto,optimal-pagination,strict-pagination,expansion/);
  assert.deepEqual(args.slice(36), ["2b", true, 13.125]);
});

for (const options of [
  { pdfA: null }, { pdfA: "" }, { pdfA: "3b" }, { pdfA: "2B" }, { pdfA: true },
  { pdfAStrict: true }, { pdfA: "off", pdfAStrict: true }, { pdfA: "2b", pdfAStrict: "true" },
  { pdfAStrict: null }, { pdfA: "2b", baselineGridPt: 12, fitToPages: 2 },
]) test(`invalid archival policy refuses before assets or init: ${JSON.stringify(options)}`, async () => {
  const f = await fixture(); let reads = 0;
  await assert.rejects(f.api.renderPdf("source", { ...options,
    get pdfImages() { reads++; throw Error("assets must not be read"); } }), { code: "INVALID_OPTIONS" });
  assert.equal(reads, 0); assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

for (const [arity, grid] of [[null, undefined], [37, undefined], [38, 12]])
  test(`missing or old archival bindings refuse without ordinary fallback: arity=${arity}, grid=${grid}`, async () => {
    const f = await fixture({ archival: arity }); let reads = 0;
    await assert.rejects(f.api.renderPdf("source", { pdfA: "2b", baselineGridPt: grid,
      get pdfImages() { reads++; throw Error("assets must not be read"); } }), { code: "UNSUPPORTED_WASM_PACKAGE" });
    assert.equal(reads, 0); assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
  });

test("native archival rejection never falls back and typed pagination errors remain actionable", async () => {
  const failure = new Error("pdf_a_file_uri: native strict refusal");
  const f = await fixture({ failure });
  await assert.rejects(f.api.renderPdf("[local](file:///tmp/private)", { pdfA: "2b", pdfAStrict: true }), error => error === failure);
  assert.deepEqual(f.calls.map(call => call.name), ["archival"]);
  const p = await fixture({ failure: JSON.stringify({ code: "PAGINATION_INFEASIBLE", reason: "infeasible", message: "exact plan refused" }) });
  await assert.rejects(p.api.renderPdf("source", { pdfA: "2b", strictPagination: true }),
    { code: "PAGINATION_INFEASIBLE", reason: "infeasible", message: "exact plan refused" });
});

test("archival resource admission rejects native-limit violations before copying or initializing", async () => {
  const f = await fixture();
  let reads = 0;
  const images = new Array(4097);
  Object.defineProperty(images, "0", { get() { reads++; throw Error("count must reject first"); } });
  await assert.rejects(f.api.renderPdf("source", { pdfA: "2b", pdfImages: images }), /at most 4096/);
  assert.equal(reads, 0);
  for (const options of [
    { pdfImages: [{ destination: "empty", bytes: new Uint8Array() }] },
    { pdfImages: [{ destination: "same", bytes: new Uint8Array([1]) }, { destination: "same", bytes: new Uint8Array([2]) }] },
    { pdfImages: [{ destination: "x".repeat(8193), bytes: new Uint8Array([1]) }] },
    { pdfImages: [{ destination: "shared", bytes: new Uint8Array(new SharedArrayBuffer(1)) }] },
  ]) await assert.rejects(f.api.renderPdf("source", { ...options, pdfA: "2b" }));
  const payload = new Uint8Array(32 * 1024 * 1024);
  await assert.rejects(f.api.renderPdf("source", { pdfA: "2b",
    pdfImages: Array.from({ length: 5 }, (_, index) => ({ destination: String(index), bytes: payload })) }),
  /128 MiB combined/);
  await assert.rejects(f.api.renderPdf("\ud800", { pdfA: "2b" }), /unpaired surrogate/);
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test("legacy root book entry refuses archival keys instead of silently accepting shared PDF options", async () => {
  const f = await fixture();
  await assert.rejects(f.api.renderBookPdf([{ path: "one.md", source: "# One" }], { pdfA: "2b" }), /\.\/book entry/);
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test("receipt validation rejects downgraded, unexpected and non-PDF archival claims", () => {
  const requested = normalizePdfArchival("2b", true);
  for (const output of [{ format: "pdf" }, { format: "pdf", pdfA: "2b", pdfAStrict: false },
    { format: "pdf", pdfA: "off", pdfAStrict: true }, { format: "html", ...requested }])
    assert.throws(() => pdfArchivalReceipt(output, requested), { code: "INVALID_WASM_RESPONSE" });
  assert.throws(() => pdfArchivalReceipt({ format: "pdf", ...requested }, {}), { code: "INVALID_WASM_RESPONSE" });
  assert.deepEqual(pdfArchivalReceipt({ format: "pdf" }, {}), {});
});
