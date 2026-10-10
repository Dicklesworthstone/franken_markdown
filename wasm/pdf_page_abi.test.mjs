import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

// Execute the exact public wrapper with only the generated WASM binding doubled.
// This tests ABI selection, argument order and ownership, not native PDF output.
async function fixture({ pageBinding = true, blocked = false, runningBinding = true, imageBinding = true } = {}) {
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
    ${runningBinding ? 'export const renderPdfConfiguredRunning = (...args) => result("running", args);' : ""}
    ${imageBinding ? 'export const renderPdfConfiguredRunningImages = (...args) => result("running-images", args);' : ""}
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

test("running logos select the additive ABI and capture exact asset bytes before initialization", async () => {
  const { api, bindings } = await fixture({ blocked: true });
  const payload = new Uint8Array([99, 1, 2, 3, 88]);
  const options = { page: { margins: 48 }, optimalPagination: true,
    pdfImages: [{ destination: "logo.svg", bytes: payload.subarray(1, 4) }],
    running: { header: { image: { dest: " logo.svg " }, right: "{title}" },
      footer: { image: { dest: "logo.svg", position: "right", heightPt: 24 }, center: "{page}" },
      skipFirstPage: true } };
  const pending = api.renderPdf("# Original", options);
  options.running.header.image.dest = "different.svg";
  options.running.footer.image.position = "left";
  options.running.footer.image.heightPt = 42;
  payload.fill(7);
  bindings.finishInit();
  await pending;
  const { kind, args } = bindings.calls[0];
  assert.equal(kind, "running-images");
  assert.equal(args.length, 36);
  assert.deepEqual(args[8], ["logo.svg"]);
  assert.deepEqual([...args[9]], [1, 2, 3]);
  assert.deepEqual(args[28], ["", "", "{title}", "", "{page}", ""]);
  assert.deepEqual(args.slice(29, 32), [false, false, true]);
  assert.deepEqual(args[32], ["logo.svg", "logo.svg"]);
  assert.deepEqual([...args[33]], [0, 1]);
  assert.deepEqual([...args[34]], [0, 24]);
  assert.equal(args[35], "optimal-pagination");
  assert.equal(bindings.frees, 1);
  assert.equal(payload.byteLength, 5);
});

test("image-only bands are retained, malformed images fail early and old packages refuse logos", async () => {
  const { api, bindings } = await fixture();
  for (const image of [null, [], {}, { dest: " " }, { dest: "\ud800" },
    { dest: "é".repeat(2049) }, { dest: "logo", url: "https://example.test/logo" },
    { dest: "logo", position: "center" }, { dest: "logo", heightPt: 0 },
    { dest: "logo", heightPt: 0.5 }, { dest: "logo", heightPt: 65536 },
    { dest: "logo", heightPt: "24" }]) {
    await assert.rejects(api.renderPdf("x", { running: { header: { image } } }));
  }
  let getterCalls = 0;
  const accessor = Object.defineProperty({}, "dest", { get() { getterCalls++; return "logo"; } });
  await assert.rejects(api.renderPdf("x", { running: { header: { image: accessor } } }));
  assert.equal(getterCalls, 0);
  assert.equal(bindings.initCount, 0);
  await api.renderPdf("x", { running: { header: { image: { dest: "logo" } }, skipFirstPage: true } });
  assert.equal(bindings.calls[0].kind, "running-images");
  assert.deepEqual(bindings.calls[0].args[32], ["logo", ""]);
  const old = await fixture({ imageBinding: false });
  await assert.rejects(old.api.renderPdf("x", { running: { header: { image: { dest: "logo" } } } }),
    error => error.code === "UNSUPPORTED_WASM_PACKAGE");
  assert.equal(old.bindings.initCount, 0);
  await old.api.renderPdf("x", { running: { footer: { center: "{page}" } } });
  assert.equal(old.bindings.calls[0].kind, "running");
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

test("running chrome appends geometry, six slots and three flags; empty chrome keeps the old ABIs", async () => {
  const { api, bindings } = await fixture();
  const base = { title: "Spec", metadataEpochSeconds: 1700000000 };
  await api.renderPdf("x", base);
  await api.renderPdf("x", { ...base, running: {} });
  await api.renderPdf("x", { ...base, running: { header: { left: "", rule: false }, skipFirstPage: true } });
  assert.deepEqual(bindings.calls.map(call => call.kind), ["legacy", "legacy", "legacy"]);
  await api.renderPdf("x", { ...base, running: { header: { right: "{title}", rule: true },
    footer: { center: "{page} / {pages}" }, skipFirstPage: true } });
  const { kind, args } = bindings.calls[3];
  assert.equal(kind, "running"); assert.equal(args.length, 32);
  assert.deepEqual(args.slice(0, 27), bindings.calls[0].args);
  assert.deepEqual([...args[27]], []);
  assert.deepEqual(args[28], ["", "", "{title}", "", "{page} / {pages}", ""]);
  assert.deepEqual(args.slice(29), [true, false, true]);
  await api.renderPdf("x", { ...base, page: { margins: 36 }, running: { footer: { rule: true } } });
  const paged = bindings.calls[4].args;
  assert.deepEqual([...paged[27]], [612, 792, 36, 36, 36, 36]);
  assert.deepEqual(paged.slice(28), [["", "", "", "", "", ""], false, true, false]);
});

test("malformed running options fail before initialization; old packages refuse chrome", async () => {
  const { api, bindings } = await fixture();
  for (const running of [[], "x", { top: {} }, { header: [] }, { header: { middle: "x" } },
    { footer: { left: 3 } }, { footer: { rule: "yes" } }, { skipFirstPage: 1 }]) {
    await assert.rejects(api.renderPdf("x", { running }), TypeError);
  }
  assert.equal(bindings.initCount, 0); assert.equal(bindings.calls.length, 0);
  await assert.rejects(api.renderBookPdf([{ path: "a.md", source: "# A" }], { running: {} }), TypeError);
  const old = await fixture({ runningBinding: false });
  await assert.rejects(old.api.renderPdf("x", { running: { footer: { center: "{page}" } } }),
    { code: "UNSUPPORTED_WASM_PACKAGE" });
  await old.api.renderPdf("x", { running: {} });
  assert.equal(old.bindings.calls[0].kind, "legacy");
});
