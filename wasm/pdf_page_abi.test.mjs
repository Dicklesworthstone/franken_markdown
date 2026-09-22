import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

// Execute the exact public wrapper with only the generated WASM binding doubled.
// This tests ABI selection, argument order and ownership, not native PDF output.
async function fixture({ pageBinding = true, blocked = false } = {}) {
  const dir = await mkdtemp(join(tmpdir(), "fmd-pdf-page-abi-"));
  await mkdir(join(dir, "pkg"));
  await writeFile(join(dir, "package.json"), '{"type":"module"}');
  for (const file of ["franken_markdown.js", "pdf_page.mjs"])
    await copyFile(new URL(`./${file}`, import.meta.url), join(dir, file));
  const unused = ["renderEpubConfigured", "renderHtmlConfiguredAdvanced", "renderInteractiveHtmlConfigured",
    "renderSvgConfigured", "accessibilityAudit", "capabilities", "documentStats", "renderBookPdf",
    "renderBookSite", "renderSemanticDiffHtml", "searchIndex", "semanticDiff"];
  await writeFile(join(dir, "pkg/franken_markdown.js"), `
    export const calls = [];
    export let initCount = 0, frees = 0;
    let release;
    const ready = new Promise(resolve => { release = resolve; });
    export const finishInit = () => release();
    export default function init() { initCount++; return ${blocked ? "ready" : "Promise.resolve()"}; }
    function result(kind, args) {
      calls.push({ kind, args });
      return { format: "pdf", mimeType: "application/pdf", extension: "pdf",
        sourceLength: new TextEncoder().encode(args[0]).length, bytes: new Uint8Array([37, 80, 68, 70]),
        diagnosticsJson() { return '[{"severity":"warning","start":0,"end":0,"message":"fixture"}]'; },
        free() { frees++; } };
    }
    export const renderPdfConfiguredMulti = (...args) => result("legacy", args);
    ${pageBinding ? 'export const renderPdfConfiguredPage = (...args) => result("page", args);' : ""}
    ${unused.map(name => `export const ${name} = () => { throw new Error("unexpected ${name}"); };`).join("\n")}
  `);
  const bindings = await import(pathToFileURL(join(dir, "pkg/franken_markdown.js")));
  const api = await import(pathToFileURL(join(dir, "franken_markdown.js")));
  return { api, bindings };
}

test("no page option uses the unchanged 27-argument legacy ABI even with a new package", async () => {
  const { api, bindings } = await fixture();
  const output = await api.renderPdf("# Café", { font: "serif", title: "  Title  ", author: "Author",
    darkMode: "off", metadataEpochSeconds: 0, codeLineNumbers: true, pageNumbers: true,
    baseFontSize: 12, headingScale: 1.3, tableFontSize: 10, fontScale: 1.125,
    lang: "fr", toc: true, tocDepth: 3, fitToPages: 2, microtype: "protrusion" });
  const { kind, args } = bindings.calls[0];
  assert.equal(kind, "legacy");
  assert.equal(args.length, 27);
  assert.deepEqual(args.slice(0, 8), ["# Café", "serif", "disabled", "  Title  ", "Author", 0, false, true]);
  assert.deepEqual(args.slice(8, 11).map(value => [...value]), [[], [], []]);
  assert.deepEqual(args.slice(11, 16).map(value => [...value]), [[], [], [], [], []]);
  assert.deepEqual([...args[16]], [0, 0, 0, 0, 0]);
  assert.deepEqual(args.slice(17), [12, 1.3, 10, true, 1.125, "fr", true, 3, 2, true]);
  assert.equal(output.filename("report"), "report.pdf");
  assert.equal(output.sourceLength, 7);
  assert.equal(output.diagnostics[0].message, "fixture");
  assert.equal(bindings.frees, 1);
});

test("paper geometry appends exactly six point values without losing images, font slots or weights", async () => {
  const { api, bindings } = await fixture();
  const data = new Uint8Array([99, 1, 2, 3, 88]);
  const options = { pdfImages: [{ destination: "pixel.png", bytes: data.subarray(1, 4) }],
    fontAssets: ["body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular"]
      .map((slot, i) => ({ slot, bytes: new Uint8Array([i + 4]), weight: 100 + i })),
    title: "Geometry", metadataEpochSeconds: 123, pageNumbers: true, toc: true, tocDepth: 2 };
  await api.renderPdf("source", options);
  await api.renderPdf("source", { ...options, page: { size: { widthPt: 720, heightPt: 540 },
    margins: { topPt: 18, rightPt: 24, bottomPt: 30, leftPt: 36 } } });
  const { kind, args } = bindings.calls[1];
  assert.equal(kind, "page"); assert.equal(args.length, 28);
  assert.deepEqual(args.slice(0, 27), bindings.calls[0].args);
  assert.ok(args[27] instanceof Float64Array);
  assert.deepEqual([...args[27]], [720, 540, 18, 24, 30, 36]);
  assert.deepEqual(args[8], ["pixel.png"]);
  assert.deepEqual([...args[9]], [1, 2, 3]);
  assert.deepEqual([...args[10]], [3]);
  assert.deepEqual(args.slice(11, 16).map(value => [...value]), [[4], [5], [6], [7], [8]]);
  assert.deepEqual([...args[16]], [100, 101, 102, 103, 104]);
  assert.equal(data.byteLength, 5);
});

test("page, source, primitive options and binary assets are captured before initialization yields", async () => {
  const { api, bindings } = await fixture({ blocked: true });
  const image = new Uint8Array([1, 2]), font = new Uint8Array([3, 4]);
  const options = { title: "Before", metadataEpochSeconds: 0,
    page: { size: { widthPt: 720, heightPt: 540 }, margins: { leftPt: 24 } },
    pdfImages: [{ destination: "before.png", bytes: image }],
    fontAssets: [{ slot: "body-regular", bytes: font, weight: 650 }] };
  const pending = api.renderPdf("# Original", options);
  options.title = "After"; options.page.size.widthPt = 999; options.page.margins.leftPt = 99;
  options.page = {}; options.pdfImages[0].destination = "after.png";
  options.fontAssets[0].weight = 100; image.fill(9); font.fill(8);
  bindings.finishInit(); await pending;
  const args = bindings.calls[0].args;
  assert.equal(args[0], "# Original"); assert.equal(args[3], "Before");
  assert.deepEqual(args[8], ["before.png"]);
  assert.deepEqual([...args[9]], [1, 2]); assert.deepEqual([...args[11]], [3, 4]);
  assert.equal(args[16][0], 650);
  assert.deepEqual([...args[27]], [720, 540, 72, 72, 72, 24]);
});

test("legacy generated packages render defaults but refuse to silently ignore requested paper", async () => {
  const { api, bindings } = await fixture({ pageBinding: false });
  await api.renderPdf("old");
  assert.equal(bindings.calls[0].kind, "legacy");
  for (const page of [{}, { size: "a4" }, { margins: 36 }])
    await assert.rejects(api.renderPdf("new", { page }), { code: "UNSUPPORTED_WASM_PACKAGE" });
  assert.equal(bindings.calls.length, 1);
});

test("invalid page geometry fails before initializing WASM or inspecting asset entries", async () => {
  const { api, bindings } = await fixture();
  let reads = 0;
  for (const page of [null, { margins: 500 }, { size: "unknown" }, { orientation: "sideways" }]) {
    await assert.rejects(api.renderPdf("x", { page, get pdfImages() { reads++; return []; } }), { code: "INVALID_OPTIONS" });
  }
  assert.equal(reads, 0); assert.equal(bindings.initCount, 0); assert.equal(bindings.calls.length, 0);
});

test("A4 and landscape selections reach the geometry binding without scaling typography", async () => {
  const { api, bindings } = await fixture();
  await api.renderPdf("x", { page: { size: "a4", orientation: "landscape", margins: 36 }, baseFontSize: 12 });
  const args = bindings.calls[0].args;
  assert.equal(args[17], 12);
  assert.deepEqual([...args[27]], [297 * 72 / 25.4, 210 * 72 / 25.4, 36, 36, 36, 36]);
});
