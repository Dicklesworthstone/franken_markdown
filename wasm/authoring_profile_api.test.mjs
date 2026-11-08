// Production facade and option admission with explicit generated-binding doubles.
// Native definition-list rendering is exercised by the Rust ABI tests; these
// cases cover routing, compatibility and ownership at the asynchronous boundary.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { SourceTextModule, SyntheticModule } from "node:vm";
import { normalizeAuthoringProfile } from "./render_typography.mjs";

const utf8 = value => new TextEncoder().encode(value);
const routes = [
  { method: "renderHtml", format: "html", binding: "renderHtmlConfiguredProfiled", count: 22 },
  { method: "renderPdf", format: "pdf", binding: "renderPdfConfiguredProfiled", count: 40 },
  { method: "renderSvg", format: "svg", binding: "renderSvgConfiguredProfiled", count: 15 },
  { method: "renderEpub", format: "epub", binding: "renderEpubConfiguredProfiled", count: 20 },
];
function gate() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
async function fixture({ profiled = true, short = false, hold, failure } = {}) {
  const calls = [];
  let inits = 0, frees = 0;
  const render = (name, format, count) => {
    const binding = (...args) => {
      calls.push({ name, args });
      if (name.endsWith("Profiled") && failure) throw failure;
      return {
        format, extension: format === "interactive-html" ? "html" : format,
        mimeType: { html: "text/html; charset=utf-8", pdf: "application/pdf", svg: "image/svg+xml",
          epub: "application/epub+zip", "interactive-html": "text/html; charset=utf-8" }[format],
        sourceLength: utf8(args[0]).length, bytes: utf8(`binding-double:${name}`),
        diagnosticsJson: () => "[]", free() { frees++; },
      };
    };
    Object.defineProperty(binding, "length", { value: count });
    return binding;
  };
  const bindings = { default() { inits++; return hold?.promise ?? Promise.resolve(); },
    accessibilityAudit() {}, capabilities() {}, documentStats() {}, searchIndex() {},
    renderSemanticDiffHtml() {}, semanticDiff() {},
  };
  for (const [name, format, count] of [
    ["renderHtmlConfiguredAdvanced", "html", 21],
    ["renderPdfConfiguredMulti", "pdf", 28],
    ["renderPdfConfiguredPage", "pdf", 29],
    ["renderPdfConfiguredRunning", "pdf", 33],
    ["renderPdfConfiguredRunningImages", "pdf", 36],
    ["renderPdfConfiguredGrid", "pdf", 37],
    ["renderPdfConfiguredArchival", "pdf", 39],
    ["renderSvgConfigured", "svg", 5],
    ["renderSvgConfiguredResources", "svg", 14],
    ["renderEpubConfigured", "epub", 6],
    ["renderEpubConfiguredAdvanced", "epub", 19],
    ["renderInteractiveHtmlConfiguredAdvanced", "interactive-html", 20],
    ["renderBookPdf", "pdf", 5],
  ]) bindings[name] = render(name, format, count);
  if (profiled) for (const route of routes)
    bindings[route.binding] = render(route.binding, route.format, route.count - Number(short));
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

test("authoring profiles accept only canonical names without coercion", () => {
  assert.equal(normalizeAuthoringProfile(undefined), undefined);
  for (const profile of ["commonmark-gfm", "gfm-plus"])
    assert.equal(normalizeAuthoringProfile(profile), profile);
  for (const profile of [null, "", "gfm", "CommonMark-GFM", " gfm-plus ", "plus", true, 1,
    { toString() { assert.fail("profile coercion must not run"); } }])
    assert.throws(() => normalizeAuthoringProfile(profile), { code: "INVALID_OPTIONS" });
});

for (const route of routes) {
  test(`${route.format} uses the profiled native export only for an explicit dialect`, async () => {
    const f = await fixture();
    for (const profile of ["commonmark-gfm", "gfm-plus"]) {
      const output = await f.api[route.method]("Term\n: Definition", { profile });
      const call = f.calls.at(-1);
      assert.equal(call.name, route.binding);
      assert.equal(call.args.length, route.count);
      assert.equal(call.args.at(-1), profile);
      assert.equal(call.args[0], "Term\n: Definition");
      assert.equal(output.format, route.format);
      assert.equal(output.sourceLength, utf8("Term\n: Definition").length);
    }
    assert.equal(f.frees, 2);
  });

  for (const warm of [false, true]) test(`${route.format} captures profile before ${warm ? "warm" : "cold"} initialization yields`, async () => {
    const hold = warm ? undefined : gate(), f = await fixture({ hold });
    if (warm) await f.api.init();
    let profile = "commonmark-gfm", reads = 0, source = "Term\n: Before";
    const options = { get profile() { reads++; return profile; } };
    const pending = f.api[route.method]({ toString: () => source }, options);
    profile = "gfm-plus"; source = "After";
    assert.equal(f.calls.length, 0);
    hold?.resolve();
    await pending;
    assert.equal(reads, 1);
    assert.equal(f.calls[0].args.at(-1), "commonmark-gfm");
    assert.equal(f.calls[0].args[0], "Term\n: Before");
  });

  test(`${route.format} rejects invalid profiles before assets or WASM initialization`, async () => {
    const f = await fixture();
    let assets = 0;
    for (const profile of [null, "", "gfm", "GFM-PLUS", 7, {}, true])
      await assert.rejects(f.api[route.method]("Term\n: Definition", { profile,
        get pdfImages() { assets++; assert.fail("assets read before profile validation"); } }),
      { code: "INVALID_OPTIONS" });
    assert.equal(assets, 0); assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
  });

  test(`${route.format} refuses missing or incomplete profile bindings without fallback`, async () => {
    for (const fixtureOptions of [{ profiled: false }, { short: true }]) {
      const f = await fixture(fixtureOptions);
      for (const profile of ["commonmark-gfm", "gfm-plus"])
        await assert.rejects(f.api[route.method]("Term\n: Definition", { profile,
          get pdfImages() { assert.fail("stale-package refusal must precede asset access"); } }),
        { code: "UNSUPPORTED_WASM_PACKAGE" });
      assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
    }
  });
}

test("omitted and undefined profiles retain every legacy route and exact argument count", async () => {
  const f = await fixture({ profiled: false });
  for (const [method, options, name, count] of [
    ["renderHtml", {}, "renderHtmlConfiguredAdvanced", 19],
    ["renderHtml", { htmlFontFormat: "woff2" }, "renderHtmlConfiguredAdvanced", 20],
    ["renderHtml", { fontEmbedding: "full" }, "renderHtmlConfiguredAdvanced", 21],
    ["renderPdf", {}, "renderPdfConfiguredMulti", 27],
    ["renderPdf", { page: { size: "a4" } }, "renderPdfConfiguredPage", 28],
    ["renderPdf", { running: { footer: { center: "{page}" } } }, "renderPdfConfiguredRunning", 32],
    ["renderPdf", { running: { header: { image: { dest: "logo" } } } }, "renderPdfConfiguredRunningImages", 35],
    ["renderPdf", { baselineGridPt: 12 }, "renderPdfConfiguredGrid", 37],
    ["renderPdf", { pdfA: "2b" }, "renderPdfConfiguredArchival", 38],
    ["renderPdf", { pdfA: "2b", baselineGridPt: 12 }, "renderPdfConfiguredArchival", 39],
    ["renderSvg", {}, "renderSvgConfiguredResources", 14],
    ["renderEpub", {}, "renderEpubConfiguredAdvanced", 18],
    ["renderEpub", { fontEmbedding: "full" }, "renderEpubConfiguredAdvanced", 19],
  ]) {
    await f.api[method]("source", options);
    const omitted = f.calls.at(-1);
    assert.equal(omitted.name, name); assert.equal(omitted.args.length, count);
    await f.api[method]("source", { ...options, profile: undefined });
    assert.deepEqual(f.calls.at(-1), omitted);
  }
});

test("profiled PDF preserves paper, running images, typography, archival mode and baseline grid", async () => {
  const hold = gate(), f = await fixture({ hold });
  const image = Uint8Array.of(91, 1, 2, 92), font = Uint8Array.of(93, 3, 4, 94);
  const options = { profile: "commonmark-gfm", pdfA: "2b", pdfAStrict: true, baselineGridPt: 13.125,
    page: { size: { widthPt: 720, heightPt: 540 }, margins: { topPt: 36, rightPt: 24, bottomPt: 30, leftPt: 18 } },
    running: { header: { left: "{title}", rule: true, image: { dest: "logo", position: "right", heightPt: 16 } },
      footer: { center: "{page}" }, skipFirstPage: true },
    pdfImages: [{ destination: "logo", bytes: image.subarray(1, 3) }],
    fontAssets: [{ slot: "body-regular", bytes: font.subarray(1, 3), weight: 550 }],
    title: "Before", typography: { homogeneous: true, antiriver: true, pareto: true },
    optimalPagination: true, strictPagination: true, footnoteBacklinks: true, microtype: "expansion" };
  const pending = f.api.renderPdf("Term\n: Definition", options);
  options.profile = "gfm-plus"; options.pdfA = "off"; options.pdfAStrict = false;
  options.baselineGridPt = 14; options.page.size.widthPt = 900; options.running.header.left = "Changed";
  options.typography.pareto = false; image.fill(8); font.fill(9);
  hold.resolve();
  const output = await pending, args = f.calls[0].args;
  assert.equal(args.length, 40);
  assert.deepEqual([...args[9]], [1, 2]); assert.deepEqual([...args[11]], [3, 4]);
  assert.equal(args[16][0], 550);
  assert.deepEqual([...args[27]], [720, 540, 36, 24, 30, 18]);
  assert.deepEqual(args[28], ["{title}", "", "", "", "{page}", ""]);
  assert.deepEqual(args.slice(29, 32), [true, false, true]);
  assert.deepEqual(args[32], ["logo", ""]);
  assert.equal(args[35], "homogeneous,antiriver,pareto,optimal-pagination,strict-pagination,footnote-backlinks,expansion");
  assert.deepEqual(args.slice(36), ["2b", true, 13.125, "commonmark-gfm"]);
  assert.equal(output.pdfA, "2b"); assert.equal(output.pdfAStrict, true);
});

test("ordinary profiled PDFs use explicit archival-off defaults without adding an archival receipt", async () => {
  const f = await fixture();
  const output = await f.api.renderPdf("Term\n: Definition", { profile: "gfm-plus" });
  assert.deepEqual(f.calls[0].args.slice(36), ["off", false, undefined, "gfm-plus"]);
  assert.equal(Object.hasOwn(output, "pdfA"), false);
  assert.equal(Object.hasOwn(output, "pdfAStrict"), false);
});

test("profiled HTML and EPUB retain explicit full-font policy in their distinct ABI slots", async () => {
  const f = await fixture();
  await f.api.renderHtml("Term\n: Definition", { profile: "commonmark-gfm", htmlFontFormat: "woff2", fontEmbedding: "full" });
  assert.deepEqual(f.calls.at(-1).args.slice(19), ["woff2", "full", "commonmark-gfm"]);
  await f.api.renderEpub("Term\n: Definition", { profile: "gfm-plus", fontEmbedding: "full" });
  assert.deepEqual(f.calls.at(-1).args.slice(18), ["full", "gfm-plus"]);
});

test("concurrent dialects retain request identity and native failure is never replayed without the profile", async () => {
  const hold = gate(), f = await fixture({ hold });
  const first = f.api.renderHtml("first", { profile: "commonmark-gfm" });
  const second = f.api.renderHtml("second", { profile: "gfm-plus" });
  hold.resolve(); await Promise.all([first, second]);
  assert.deepEqual(f.calls.map(({ args }) => [args[0], args.at(-1)]), [
    ["first", "commonmark-gfm"], ["second", "gfm-plus"],
  ]);
  const failed = await fixture({ failure: new Error("fixture native refusal") });
  for (const route of routes) {
    await assert.rejects(failed.api[route.method]("source", { profile: "commonmark-gfm" }), /fixture native refusal/);
    assert.equal(failed.calls.at(-1).name, route.binding);
  }
  assert.equal(failed.calls.length, routes.length);
});

test("old book bindings and lightweight interactive exports refuse authoring profiles", async () => {
  const f = await fixture();
  await assert.rejects(f.api.renderBookPdf([{ path: "one.md", source: "Term\n: Definition" }], { profile: "gfm-plus" }),
    { code: "UNSUPPORTED_WASM_PACKAGE" });
  await assert.rejects(f.api.renderInteractiveHtml("Term\n: Definition", { profile: "commonmark-gfm" }),
    /Unsupported interactive HTML option/);
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});
