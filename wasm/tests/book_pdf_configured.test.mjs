// Exercise the real public root wrapper with explicitly substituted generated
// bindings. No WASM engine or real PDF rendering is claimed by these tests.
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const wrapper = await readFile(new URL("../franken_markdown.js", import.meta.url), "utf8");
const pageModule = new URL("../pdf_page.mjs", import.meta.url).href;
let serial = 0;
const dataModule = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;

async function fixture(advanced = true) {
  const bindingUrl = dataModule(`// isolated generated-binding double ${++serial}
    export const state = { calls: [], loads: 0, freed: 0, wait: null, fail: null, badDiagnostics: false };
    export default async function init() { state.loads++; await state.wait; }
    function output(kind, args) {
      state.calls.push({ kind, args });
      if (state.fail) throw state.fail;
      return {
        bytes: new TextEncoder().encode(kind), format: kind.startsWith('book') ? 'book-pdf' : kind,
        mimeType: 'application/pdf', extension: 'pdf', sourceLength: 10,
        diagnosticsJson() { return state.badDiagnostics ? 'invalid' : '[]'; },
        free() { state.freed++; },
      };
    }
    export const renderBookPdfConfiguredPage = ${advanced ? "(...args) => output('book-advanced', args)" : "undefined"};
    export const renderBookPdf = (...args) => output('book-legacy', args);
    export const renderPdfConfiguredPage = (...args) => output('pdf-page', args);
    export const renderPdfConfiguredMulti = (...args) => output('pdf', args);
    ${["renderEpubConfigured", "renderHtmlConfiguredAdvanced", "renderInteractiveHtmlConfigured",
      "renderSvgConfigured", "accessibilityAudit", "capabilities", "documentStats", "renderBookSite",
      "renderSemanticDiffHtml", "searchIndex", "semanticDiff"].map(name =>
      `export const ${name} = (...args) => output(${JSON.stringify(name)}, args);`).join("\n")}
  `);
  const { state } = await import(bindingUrl);
  const source = wrapper.replaceAll('"./pkg/franken_markdown.js"', JSON.stringify(bindingUrl))
    .replaceAll('"./pdf_page.mjs"', JSON.stringify(pageModule));
  return { api: await import(dataModule(source)), state };
}

const chapters = () => [{ path: "guide/one.md", source: "# One" }, { path: "two.md", source: "# Two" }];
const page = () => ({ size: { widthPt: 720, heightPt: 540 }, margins: { topPt: 18, rightPt: 24, bottomPt: 30, leftPt: 36 } });

test("root basic exports preserve eight-argument legacy ABI, defaults and output ownership", async () => {
  for (const available of [false, true]) {
    const { api, state } = await fixture(available);
    const output = await api.renderBookPdf(chapters(), {
      title: "  Manual  ", author: " Author ", font: " serif ", darkMode: "system", fontScale: "lg",
    });
    assert.equal(state.calls[0].kind, "book-legacy");
    assert.deepEqual(state.calls[0].args, [["guide/one.md", "two.md"], ["# One", "# Two"],
      "  Manual  ", " Author ", "serif", "auto", 1.125, true]);
    assert.equal(output.text(), "book-legacy");
    assert.equal(output.filename(" manual "), "manual.pdf");
    assert.equal(output.filename(" "), "document.pdf");
    assert.equal(output.format, "book-pdf");
    assert.deepEqual(output.diagnostics, []);
    assert.equal(state.freed, 1);
    assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
  }
});

test("root configured books carry all 29 ABI arguments including exact asset views", async () => {
  const { api, state } = await fixture();
  const bytes = new Uint8Array([99, 1, 2, 3, 88]);
  const fonts = new Uint8Array([77, 4, 5, 66]);
  await api.renderBookPdf(chapters(), {
    page: page(), font: "serif", darkMode: "light", title: "Title", author: "Author",
    metadataEpochSeconds: 0, allowRawHtml: true, codeLineNumbers: true,
    pdfImages: [{ destination: "guide/a.svg", bytes: new DataView(bytes.buffer, 1, 3) }],
    fontAssets: [{ slot: "body-regular", bytes: fonts.subarray(1, 3), weight: 550 }],
    baseFontSize: 12, headingScale: 1.3, tableFontSize: 9, pageNumbers: false,
    fontScale: "112.5%", lang: "de", toc: false, tocDepth: 2, fitToPages: 7,
    microtype: "protrusion",
  });
  const { kind, args } = state.calls[0];
  assert.equal(kind, "book-advanced");
  assert.equal(args.length, 29);
  assert.deepEqual(args.slice(0, 10), [["guide/one.md", "two.md"], ["# One", "# Two"],
    "serif", "disabled", "Title", "Author", 0, true, true, ["guide/a.svg"]]);
  assert.deepEqual(Array.from(args[10]), [1, 2, 3]);
  assert.deepEqual(Array.from(args[11]), [3]);
  assert.deepEqual(Array.from(args[12]), [4, 5]);
  for (const index of [13, 14, 15, 16]) assert.equal(args[index].byteLength, 0);
  assert.deepEqual(Array.from(args[17]), [550, 0, 0, 0, 0]);
  assert.deepEqual(args.slice(18, 28), [12, 1.3, 9, false, 1.125, "de", false, 2, 7, true]);
  assert.deepEqual(Array.from(args[28]), [720, 540, 18, 24, 30, 36]);
  assert.ok(args[28] instanceof Float64Array);
  assert.equal(state.freed, 1);
});

test("advanced defaults preserve book TOC/page numbers and do not double-apply font scale", async () => {
  const { api, state } = await fixture();
  const renderer = await api.createRenderer();
  await renderer.renderBookPdf(chapters(), { page: page(), fontScale: "lg" });
  const args = state.calls[0].args;
  assert.equal(args[18], undefined, "book scale must not inject a second base-size override");
  assert.equal(args[21], true);
  assert.equal(args[22], 1.125);
  assert.equal(args[24], true);
  await renderer.renderBookPdf(chapters(), { pageNumbers: false });
  assert.equal(state.calls[1].kind, "book-legacy");
  assert.equal(state.calls[1].args[7], false);
});

test("root snapshots chapters, primitive options, geometry and assets before awaiting initialization", async () => {
  const { api, state } = await fixture();
  let release;
  state.wait = new Promise(resolve => { release = resolve; });
  const files = chapters(), paper = page(), bytes = new Uint8Array([1, 2]);
  const options = { page: paper, title: "Before", fontScale: 1.125,
    pdfImages: [{ destination: "guide/a.svg", bytes }],
    fontAssets: [{ slot: "mono-regular", bytes }],
  };
  const pending = api.renderBookPdf(files, options);
  files[0].source = "Changed";
  files[0].path = "wrong.md";
  options.title = "After";
  options.fontScale = 2;
  paper.size.widthPt = 900;
  paper.margins.leftPt = 200;
  bytes.fill(9);
  release();
  await pending;
  const args = state.calls[0].args;
  assert.deepEqual(args[0], ["guide/one.md", "two.md"]);
  assert.deepEqual(args[1], ["# One", "# Two"]);
  assert.equal(args[4], "Before");
  assert.equal(args[22], 1.125);
  assert.deepEqual(Array.from(args[10]), [1, 2]);
  assert.deepEqual(Array.from(args[16]), [1, 2]);
  assert.deepEqual(Array.from(args[28]), [720, 540, 18, 24, 30, 36]);
});

test("each advanced option gates old WASM instead of silently discarding requested behavior", async () => {
  const { api, state } = await fixture(false);
  for (const options of [
    { page: {} }, { metadataEpochSeconds: 0 }, { allowRawHtml: true }, { codeLineNumbers: true },
    { pdfImages: [{ destination: "a.svg", bytes: new Uint8Array([1]) }] },
    { fontAssets: [{ slot: "body-bold", bytes: new Uint8Array([1]) }] },
    { baseFontSize: 12 }, { headingScale: 1.2 }, { tableFontSize: 8 }, { lang: "fr" },
    { toc: false }, { tocDepth: 2 }, { fitToPages: 3 }, { microtype: "protrusion" },
  ]) {
    await assert.rejects(api.renderBookPdf(chapters(), options), { code: "UNSUPPORTED_WASM_PACKAGE" });
  }
  assert.equal(state.loads, 0);
  assert.equal(state.calls.length, 0);
  await api.renderBookPdf(chapters(), { toc: true });
  assert.equal(state.calls[0].kind, "book-legacy");
});

test("assets and navigation work without explicit geometry", async () => {
  const { api, state } = await fixture();
  await api.renderBookPdf(chapters(), { toc: false });
  assert.equal(state.calls[0].kind, "book-advanced");
  assert.equal(state.calls[0].args[24], false);
  assert.equal(state.calls[0].args[28].length, 0);
  await api.renderBookPdf(chapters(), { pdfImages: [{ destination: "a.svg", bytes: new Uint8Array([1]) }] });
  assert.equal(state.calls[1].kind, "book-advanced");
});

test("invalid sources, options and assets fail before WASM initialization", async () => {
  const { api, state } = await fixture();
  await assert.rejects(api.renderBookPdf(new Array(4097)), /4096/);
  await assert.rejects(api.renderBookPdf([{ path: "a.md", source: "\ud800" }]), /surrogate/);
  await assert.rejects(api.renderBookPdf([{ path: "a.md", source: "x".repeat(64 * 1024 * 1024) }]), /byte limit/);
  for (const options of [
    { page: { margins: 500 } }, { toc: "false" }, { tocDepth: 7 }, { fitToPages: 2 ** 32 },
    { metadataEpochSeconds: -1 }, { baseFontSize: NaN }, { pdfImages: new Array(4097) },
    { fontAssets: new Array(6) },
    { pdfImages: [{ destination: "a.svg", bytes: new Uint8Array() }] },
    { pdfImages: [{ destination: "a.svg", bytes: new Uint8Array(new SharedArrayBuffer(1)) }] },
    { pdfImages: [{ destination: "a.svg", bytes: new Uint8Array([1]) },
      { destination: "a.svg", bytes: new Uint8Array([2]) }] },
  ]) await assert.rejects(api.renderBookPdf(chapters(), options));
  assert.equal(state.loads, 0);
  assert.equal(state.calls.length, 0);
});

test("root snapshots top-level getters once and does not consult them again for ABI selection", async () => {
  const { api, state } = await fixture();
  const reads = new Map();
  const values = { page: page(), toc: false, title: "A", metadataEpochSeconds: 0 };
  const options = Object.fromEntries([]);
  for (const [key, value] of Object.entries(values)) {
    Object.defineProperty(options, key, { get() {
      reads.set(key, (reads.get(key) ?? 0) + 1);
      if (reads.get(key) !== 1) throw new Error("read twice");
      return value;
    } });
  }
  await api.renderBookPdf(chapters(), options);
  assert.equal(state.calls[0].kind, "book-advanced");
  assert.deepEqual([...reads.values()], [1, 1, 1, 1]);
});

test("renderer failures do not leak result wrappers and failed diagnostics still free them", async () => {
  const { api, state } = await fixture();
  state.fail = new Error("native rendering failed");
  await assert.rejects(api.renderBookPdf(chapters(), { page: page() }), /native rendering failed/);
  assert.equal(state.freed, 0);
  state.fail = null;
  state.badDiagnostics = true;
  await assert.rejects(api.renderBookPdf(chapters(), { page: page() }), /Invalid diagnostics/);
  assert.equal(state.freed, 1);
  state.badDiagnostics = false;
  await api.renderBookPdf(chapters(), { page: page() });
  assert.equal(state.freed, 2);
});

test("root single-document PDF calls still use their existing ABI and geometry order", async () => {
  const { api, state } = await fixture();
  await api.renderPdf("# Solo");
  assert.equal(state.calls[0].kind, "pdf");
  assert.equal(state.calls[0].args.length, 27);
  await api.renderPdf("# Solo", { page: page() });
  assert.equal(state.calls[1].kind, "pdf-page");
  assert.equal(state.calls[1].args.length, 28);
  assert.deepEqual(Array.from(state.calls[1].args[27]), [720, 540, 18, 24, 30, 36]);
});
