// Test the real root wrapper with explicit generated-binding doubles. These
// tests prove admission, option delivery and ownership, NOT native HTML/ZIP
// rendering. site_publication_smoke.mjs separately requires a real WASM build.
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const wrapper = await readFile(new URL("../franken_markdown.js", import.meta.url), "utf8");
const pageModule = new URL("../pdf_page.mjs", import.meta.url).href;
const asModule = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const files = () => [{ path: "guide/start.md", source: "# Start" }, { path: "end.md", source: "# End" }];
let serial = 0;
async function fixture(available = true) {
  const url = asModule(`// Explicit generated-binding double ${++serial}
    export const state = { calls: [], loads: 0, freed: 0, wait: null, failure: null,
      badDiagnostics: false, output: new Uint8Array([80, 75, 3, 4, 1, 2]) };
    export default async function init() { state.loads++; await state.wait; }
    export const renderBookSitePublication = ${available ? "(...args) => { state.calls.push({kind:'site',args}); if(state.failure) throw state.failure; return state.output; }" : "undefined"};
    function result(kind,args) {
      state.calls.push({kind,args});
      if(state.failure) throw state.failure;
      return {bytes:state.output,format:kind,mimeType:'application/zip',extension:'zip',sourceLength:12,
        diagnosticsJson:()=>state.badDiagnostics?'broken':'[]',free(){state.freed++;}};
    }
    export const renderBookSite = (...args) => result('legacy-site',args);
    export const renderBookPdf = (...args) => result('legacy-pdf',args);
    export const renderBookPdfConfiguredPage = (...args) => result('pdf-book',args);
    export const renderPdfConfiguredPage = (...args) => result('pdf-page',args);
    export const renderEpubConfiguredAdvanced = (...args) => result('epub',args);
    ${["renderEpubConfigured", "renderHtmlConfiguredAdvanced", "renderInteractiveHtmlConfigured",
      "renderPdfConfiguredMulti", "renderSvgConfigured", "accessibilityAudit", "capabilities",
      "documentStats", "renderSemanticDiffHtml", "searchIndex", "semanticDiff"].map(name =>
      `export const ${name} = (...args) => result(${JSON.stringify(name)},args);`).join("\n")}
  `);
  const { state } = await import(url);
  const source = wrapper.replaceAll('"./pkg/franken_markdown.js"', JSON.stringify(url))
    .replaceAll('"./pdf_page.mjs"', JSON.stringify(pageModule));
  return { api: await import(asModule(source)), state };
}

test("ordinary site exports use the canonical publisher, never the old builder", async () => {
  const { api, state } = await fixture();
  const output = await api.renderBookSite(files());
  const { kind, args } = state.calls[0];
  assert.equal(kind, "site");
  assert.equal(args.length, 22);
  assert.deepEqual(args.slice(0, 13), [["guide/start.md", "end.md"], ["# Start", "# End"],
    [], [], true, undefined, undefined, undefined, undefined, undefined, undefined, false, undefined]);
  assert.equal(output.format, "book-site");
  assert.equal(output.mimeType, "application/zip");
  assert.equal(output.extension, "zip");
  assert.equal(output.filename("  Manual  "), "Manual.zip");
  assert.equal(output.filename(" "), "document.zip");
  assert.equal(output.sourceLength, 12);
  assert.deepEqual(output.diagnostics, []);
  assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
  assert.equal(Object.isFrozen(output), true);
});

test("all site settings and exact image/font views reach the 22-argument native ABI", async () => {
  const { api, state } = await fixture();
  const bytes = new Uint8Array([9, 1, 2, 8]), font = new Uint8Array([7, 3, 4, 6]);
  await api.renderBookSite(files(), {
    title: "  Manual  ", font: "serif", darkMode: "light", fontScale: "lg", lang: "de",
    customCss: "", toc: true, tocDepth: 2,
    includeSources: [{ path: "guide/shared.md", source: "Included" }],
    pdfImages: [{ destination: "guide/figure.svg", bytes: new DataView(bytes.buffer, 1, 2) }],
    fontAssets: [{ slot: "body-bold", bytes: font.subarray(1, 3), weight: 550 }],
  });
  const args = state.calls[0].args;
  assert.deepEqual(args.slice(2, 14), [["guide/shared.md"], ["Included"], true,
    "  Manual  ", "serif", "disabled", 1.125, "de", "", true, 2, ["guide/figure.svg"]]);
  assert.deepEqual(Array.from(args[14]), [1, 2]);
  assert.deepEqual(Array.from(args[15]), [2]);
  assert.deepEqual(Array.from(args[17]), [3, 4]);
  for (const index of [16, 18, 19, 20]) assert.equal(args[index].length, 0);
  assert.deepEqual(Array.from(args[21]), [0, 550, 0, 0, 0]);
  assert.equal(state.loads, 1);
});

test("chapters, resources, settings and asset bytes are captured before initialization", async () => {
  const { api, state } = await fixture();
  let release;
  state.wait = new Promise(resolve => { release = resolve; });
  const chapters = files(), resources = [{ path: "shared.md", source: "Shared" }];
  const bytes = new Uint8Array([1, 2]), font = new Uint8Array([3, 4]);
  const options = { includeSources: resources, customCss: "old", title: "Before",
    pdfImages: [{ destination: "a.svg", bytes }], fontAssets: [{ slot: "mono-regular", bytes: font }] };
  const pending = api.renderBookSite(chapters, options);
  chapters[0].path = "changed.md";
  chapters[0].source = "After";
  resources[0].source = "Replaced";
  options.title = "After";
  options.customCss = "changed";
  bytes.fill(9); font.fill(8);
  release();
  const output = await pending, args = state.calls[0].args;
  assert.equal(args[0][0], "guide/start.md");
  assert.equal(args[1][0], "# Start");
  assert.deepEqual(args[3], ["Shared"]);
  assert.equal(args[5], "Before");
  assert.equal(args[10], "old");
  assert.deepEqual(Array.from(args[14]), [1, 2]);
  assert.deepEqual(Array.from(args[20]), [3, 4]);
  assert.equal(output.sourceLength, 18);
});

test("UTF-8 sourceLength counts source text once, including non-chapter resources", async () => {
  const { api } = await fixture();
  const chapters = [{ path: "中文.md", source: "# α😀" }], resources = [{ path: "x.md", source: "é" }];
  const output = await api.renderBookSite(chapters, { includeSources: resources });
  assert.equal(output.sourceLength, new TextEncoder().encode("# α😀é").length);
});

test("old basic packages retain their six-argument ABI with an explicit limitation warning", async () => {
  const { api, state } = await fixture(false);
  const output = await api.renderBookSite(files(), { title: " A ", font: " serif ", darkMode: "system", typeSize: "lg" });
  assert.equal(state.calls[0].kind, "legacy-site");
  assert.deepEqual(state.calls[0].args, [["guide/start.md", "end.md"], ["# Start", "# End"], " A ", "serif", "auto", 1.125]);
  assert.equal(state.freed, 1);
  assert.equal(output.diagnostics.length, 1);
  assert.match(output.diagnostics[0].message, /Legacy.*rebuild/);
});

test("old WASM cannot silently lose assets, CSS, language, navigation or includes", async () => {
  const { api, state } = await fixture(false);
  for (const options of [{ customCss: "" }, { lang: "de" }, { toc: true }, { tocDepth: 2 },
    { expandIncludes: false }, { includeSources: [{ path: "x.md", source: "x" }] },
    { pdfImages: [{ destination: "a.svg", bytes: new Uint8Array([1]) }] },
    { fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([1]) }] }]) {
    await assert.rejects(api.renderBookSite(files(), options), { code: "UNSUPPORTED_WASM_PACKAGE" });
  }
  assert.equal(state.loads, 0);
  assert.equal(state.calls.length, 0);
});

test("unsupported PDF options and raw-HTML trust requests fail instead of being ignored", async () => {
  const { api, state } = await fixture();
  for (const options of [{ page: {} }, { author: "A" }, { pageNumbers: true },
    { metadataEpochSeconds: 0 }, { allowRawHtml: true }, { allowRawHtml: null }, { unknown: 1 }]) {
    await assert.rejects(api.renderBookSite(files(), options), { code: "UNSUPPORTED_SITE_OPTION" });
  }
  assert.equal(state.loads, 0);
  await api.renderBookSite(files(), { allowRawHtml: false, page: undefined });
  assert.equal(state.calls[0].kind, "site");
});

test("invalid counts are refused before touching asset and chapter entries", async () => {
  const { api, state } = await fixture();
  const large = new Array(4097);
  Object.defineProperty(large, 0, { get() { throw new Error("must not inspect entries"); } });
  await assert.rejects(api.renderBookSite(large), /4096/);
  await assert.rejects(api.renderBookSite(files(), { pdfImages: large }), /4096/);
  await assert.rejects(api.renderBookSite(files(), { includeSources: new Array(4095) }), /4096/);
  await assert.rejects(api.renderBookSite(files(), { fontAssets: new Array(6) }), /5 entries/);
  assert.equal(state.loads, 0);
});

test("Unicode, source, metadata, and include-policy admission share native budgets", async () => {
  const { api, state } = await fixture();
  await assert.rejects(api.renderBookSite([{ path: "a.md", source: "\ud800" }]), /surrogate/);
  await assert.rejects(api.renderBookSite([{ path: "a.md", source: "x".repeat(64 * 1024 * 1024) }]), /byte limit/);
  await assert.rejects(api.renderBookSite(files(), { customCss: "x".repeat(4 * 1024 * 1024), title: "x" }), /byte limit/);
  await assert.rejects(api.renderBookSite(files(), { expandIncludes: false, includeSources: [{ path: "x", source: "x" }] }), /requires/);
  for (const options of [{ toc: "true" }, { expandIncludes: null }, { tocDepth: 0 }, { tocDepth: 7 },
    { fontScale: NaN }, { includeSources: null }]) await assert.rejects(api.renderBookSite(files(), options));
  assert.equal(state.loads, 0);
});

test("shared, detached, empty, duplicate and oversized asset admissions fail", async () => {
  const { api, state } = await fixture();
  const detached = new Uint8Array([1]);
  structuredClone(detached, { transfer: [detached.buffer] });
  for (const bytes of [new Uint8Array(), detached, new Uint8Array(new SharedArrayBuffer(1)), new Uint8Array(32 * 1024 * 1024 + 1)]) {
    await assert.rejects(api.renderBookSite(files(), { pdfImages: [{ destination: "a.svg", bytes }] }));
  }
  await assert.rejects(api.renderBookSite(files(), { pdfImages: [
    { destination: "a.svg", bytes: new Uint8Array([1]) }, { destination: " a.svg ", bytes: new Uint8Array([2]) },
  ] }), /unique/);
  await assert.rejects(api.renderBookSite(files(), { fontAssets: [
    { slot: "body-bold", bytes: new Uint8Array([1]) }, { slot: "body-bold", bytes: new Uint8Array([2]) },
  ] }), /duplicate slot/);
  assert.equal(state.loads, 0);
});

test("each option/source/asset getter is captured exactly once", async () => {
  const { api, state } = await fixture();
  const reads = new Map(), get = (key, value) => () => {
    reads.set(key, (reads.get(key) ?? 0) + 1);
    assert.equal(reads.get(key), 1, `read ${key} twice`);
    return value;
  };
  const source = {}, image = {}, font = {}, options = {};
  Object.defineProperties(source, { path: { get: get("path", "one.md") }, source: { get: get("source", "# One") } });
  Object.defineProperties(image, { destination: { get: get("dest", "a.svg") }, bytes: { get: get("image-bytes", new Uint8Array([1])) } });
  Object.defineProperties(font, { slot: { get: get("slot", "body-bold") }, bytes: { get: get("font-bytes", new Uint8Array([2])) }, weight: { get: get("weight", 600) } });
  for (const [key, value] of Object.entries({ title: "Title", customCss: "", pdfImages: [image], fontAssets: [font], toc: true }))
    Object.defineProperty(options, key, { get: get(key, value) });
  await api.renderBookSite([source], options);
  assert.equal(state.calls[0].args[5], "Title");
  assert.equal(reads.size, 12);
});

test("native failures remain bounded and do not prevent a subsequent successful export", async () => {
  const { api, state } = await fixture();
  state.failure = "include missing: " + "x".repeat(5000);
  await assert.rejects(api.renderBookSite(files()), error => error instanceof Error && error.message.length === 2048);
  state.failure = null;
  await api.renderBookSite(files());
  assert.equal(state.calls.length, 2);
});

test("legacy diagnostic failures release the WASM wrapper", async () => {
  const { api, state } = await fixture(false);
  state.badDiagnostics = true;
  await assert.rejects(api.renderBookSite(files()), /Invalid diagnostics/);
  assert.equal(state.freed, 1);
});

test("output type/header admission rejects incorrect generated-binding results", async () => {
  const { api, state } = await fixture();
  for (const output of [null, [], new Uint8Array(), new Uint8Array([1, 2, 3, 4]), new Uint8Array(new SharedArrayBuffer(4))]) {
    state.output = output;
    await assert.rejects(api.renderBookSite(files()), { code: "INVALID_SITE_OUTPUT" });
  }
});

test("a returned subview does not retain the adapter's unrelated backing allocation", async () => {
  const { api, state } = await fixture();
  const backing = new Uint8Array(100);
  backing.set([80, 75, 3, 4, 5], 10);
  state.output = backing.subarray(10, 15);
  const output = await api.renderBookSite(files());
  assert.equal(output.bytes.buffer.byteLength, 5);
  assert.notEqual(output.bytes.buffer, backing.buffer);
  backing.fill(0);
  assert.deepEqual(Array.from(output.bytes), [80, 75, 3, 4, 5]);
});

test("createRenderer exposes the same canonical site operation", async () => {
  const { api, state } = await fixture();
  const renderer = await api.createRenderer();
  await renderer.renderBookSite(files(), { toc: true });
  assert.equal(state.calls[0].kind, "site");
  assert.equal(state.calls[0].args[11], true);
});

test("existing PDF, book PDF and EPUB argument delivery is unchanged", async () => {
  const { api, state } = await fixture();
  await api.renderPdf("# One");
  await api.renderPdf("# One", { page: { size: "a4" } });
  await api.renderBookPdf(files());
  await api.renderBookPdf(files(), { toc: false });
  await api.renderEpub("# One", { customCss: "" });
  assert.deepEqual(state.calls.map(call => [call.kind, call.args.length]), [
    ["renderPdfConfiguredMulti", 27], ["pdf-page", 28], ["legacy-pdf", 8], ["pdf-book", 29], ["epub", 18],
  ]);
  assert.equal(state.freed, 5);
});
