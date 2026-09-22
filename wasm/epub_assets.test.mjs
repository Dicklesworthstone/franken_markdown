import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

// Execute the real public wrapper with a generated-binding double. This proves
// ABI marshalling/ownership, not native font subsetting or EPUB rendering.
async function fixture({ advanced = true, blocked = false } = {}) {
  const dir = await mkdtemp(join(tmpdir(), "fmd-epub-abi-"));
  await mkdir(join(dir, "pkg"));
  await writeFile(join(dir, "package.json"), '{"type":"module"}');
  await copyFile(new URL("./franken_markdown.js", import.meta.url), join(dir, "franken_markdown.js"));
  const unused = ["renderHtmlConfiguredAdvanced", "renderInteractiveHtmlConfigured", "renderPdfConfiguredMulti", "renderSvgConfigured", "accessibilityAudit", "capabilities", "documentStats", "renderBookPdf", "renderBookSite", "renderSemanticDiffHtml", "searchIndex", "semanticDiff"];
  await writeFile(join(dir, "pkg/franken_markdown.js"), `
    export const calls = [];
    export let initCount = 0, frees = 0;
    let release;
    const ready = new Promise((resolve) => { release = resolve; });
    export const finishInit = () => release();
    export default function init() { initCount++; return ${blocked ? "ready" : "Promise.resolve()"}; }
    function result(kind, args) {
      calls.push({ kind, args });
      return { format: "epub", mimeType: "application/epub+zip", extension: "epub",
        sourceLength: new TextEncoder().encode(args[0]).length, bytes: new Uint8Array([80,75,3,4]),
        diagnosticsJson() { return '[]'; }, free() { frees++; } };
    }
    export const renderEpubConfigured = (...args) => result("legacy", args);
    ${advanced ? 'export const renderEpubConfiguredAdvanced = (...args) => result("advanced", args);' : ""}
    ${unused.map((name) => `export const ${name} = () => { throw new Error("unexpected ${name}"); };`).join("\n")}
  `);
  const bindings = await import(pathToFileURL(join(dir, "pkg/franken_markdown.js")));
  const api = await import(pathToFileURL(join(dir, "franken_markdown.js")));
  return { api, bindings };
}

test("EPUB forwards exact image views, all font slots, weights, CSS and navigation", async () => {
  const { api, bindings } = await fixture();
  const buffer = new Uint8Array([9, 1, 2, 3, 9]);
  const fonts = ["mono-regular", "body-bold-italic", "body-italic", "body-bold", "body-regular"]
    .map((slot, i) => ({ slot, bytes: new Uint8Array([i + 1]), weight: 100 + i }));
  const output = await api.renderEpub("# Café", { font: "serif", darkMode: "off", title: "  Book  ",
    lang: " fr ", typeSize: "lg", customCss: "  .fmd{}  ", toc: true, tocDepth: 2,
    pdfImages: [{ destination: "a.png", bytes: buffer.subarray(1, 3) }, { destination: "b.svg", bytes: new DataView(buffer.buffer, 3, 1) }], fontAssets: fonts });
  const { kind, args } = bindings.calls[0];
  assert.equal(kind, "advanced");
  assert.equal(args.length, 18);
  assert.deepEqual(args.slice(0, 10), ["# Café", "serif", "disabled", "  Book  ", "fr", 1.125, "  .fmd{}  ", true, 2, ["a.png", "b.svg"]]);
  assert.deepEqual([...args[10]], [1, 2, 3]);
  assert.deepEqual([...args[11]], [2, 1]);
  assert.deepEqual(args.slice(12, 17).map((b) => [...b]), [[5], [4], [3], [2], [1]]);
  assert.deepEqual([...args[17]], [104, 103, 102, 101, 100]);
  assert.equal(output.mimeType, "application/epub+zip");
  assert.equal(output.filename("book"), "book.epub");
  assert.equal(bindings.frees, 1);
  assert.deepEqual([...buffer], [9, 1, 2, 3, 9]);
});

test("all inputs are owned before asynchronous WASM initialization", async () => {
  const { api, bindings } = await fixture({ blocked: true });
  const image = new Uint8Array([1, 2]), font = new Uint8Array([3, 4]);
  const options = { title: "before", pdfImages: [{ destination: "before.png", bytes: image }],
    fontAssets: [{ slot: "body-regular", bytes: font, weight: 650 }] };
  const pending = api.renderEpub("source", options);
  image.fill(9); font.fill(8); options.title = "after";
  options.pdfImages[0].destination = "after.png"; options.fontAssets[0].weight = 100;
  options.pdfImages.push({ destination: "extra.png", bytes: image });
  bindings.finishInit(); await pending;
  const args = bindings.calls[0].args;
  assert.equal(args[3], "before");
  assert.deepEqual(args[9], ["before.png"]);
  assert.deepEqual([...args[10]], [1, 2]);
  assert.deepEqual([...args[12]], [3, 4]);
  assert.equal(args[17][0], 650);
});

test("legacy packages work only for their original surface and never drop assets", async () => {
  const { api, bindings } = await fixture({ advanced: false });
  await api.renderEpub("old", { title: "  title  ", lang: "fr", fontScale: 1.25 });
  assert.equal(bindings.calls[0].kind, "legacy");
  assert.equal(bindings.calls[0].args.length, 6);
  for (const options of [{ customCss: "" }, { toc: true }, { tocDepth: 2 },
    { pdfImages: [{ destination: "a.png", bytes: new Uint8Array([1]) }] },
    { fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([1]) }] }]) {
    await assert.rejects(api.renderEpub("x", options), { code: "UNSUPPORTED_WASM_PACKAGE" });
  }
  assert.equal(bindings.calls.length, 1);
});

test("invalid or ambiguous image payloads never initialize or enter native code", async () => {
  const { api, bindings } = await fixture();
  const image = (destination, bytes = new Uint8Array([1])) => ({ destination, bytes });
  const cases = [[image("same"), image(" same ")], [image(" ")], [image("a", new Uint8Array())],
    [image("x".repeat(8193))], [image("\ud800")], [image("a", new Uint8Array(32 * 1024 * 1024 + 1))]];
  if (typeof SharedArrayBuffer === "function") cases.push([image("a", new Uint8Array(new SharedArrayBuffer(1)))]);
  const detached = new Uint8Array(1); structuredClone(detached.buffer, { transfer: [detached.buffer] });
  cases.push([image("a", detached)]);
  for (const pdfImages of cases) await assert.rejects(api.renderEpub("x", { pdfImages }));
  assert.equal(bindings.initCount, 0);
  assert.equal(bindings.calls.length, 0);
});

test("admission checks counts before entries and totals before copying", async () => {
  const { api, bindings } = await fixture();
  const tooMany = new Array(4097);
  Object.defineProperty(tooMany, 0, { get() { assert.fail("entries must not be read"); } });
  await assert.rejects(api.renderEpub("x", { pdfImages: tooMany }), /at most 4096/);
  const font = new Uint8Array(32 * 1024 * 1024);
  const slots = ["body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular"];
  await assert.rejects(api.renderEpub("x", { fontAssets: slots.map((slot) => ({ slot, bytes: font })) }), /128 MiB/);
  await assert.rejects(api.renderEpub("x", { pdfImages: slots.map((destination) => ({ destination, bytes: font })) }), /128 MiB/);
  assert.equal(bindings.initCount, 0);
});

test("font validation and absent slot fallback stay consistent with other formats", async () => {
  const { api, bindings } = await fixture();
  for (const fontAssets of [[{ slot: "missing", bytes: new Uint8Array([1]) }],
    [{ slot: "body-regular", bytes: new Uint8Array() }],
    [{ slot: "body-regular", bytes: new Uint8Array([1]), weight: 1001 }],
    [1, 2].map(() => ({ slot: "body-regular", bytes: new Uint8Array([1]) }))]) {
    await assert.rejects(api.renderEpub("x", { fontAssets }));
  }
  await api.renderEpub("plain");
  assert.deepEqual(bindings.calls[0].args.slice(12, 17).map((b) => b.length), [0, 0, 0, 0, 0]);
  assert.deepEqual([...bindings.calls[0].args[17]], [0, 0, 0, 0, 0]);
});

test("empty CSS is preserved, Unicode is not silently repaired, and depth is bounded", async () => {
  const { api, bindings } = await fixture();
  for (const [source, options] of [["\ud800", {}], ["ok", { customCss: "\udfff" }],
    ["ok", { customCss: "é".repeat(2 * 1024 * 1024 + 1) }],
    ["ok", { toc: "true" }], ["ok", { tocDepth: 7 }], ["ok", { tocDepth: 1.5 }]]) {
    await assert.rejects(api.renderEpub(source, options));
  }
  assert.equal(bindings.initCount, 0);
  await api.renderEpub("é😀", { customCss: "" });
  assert.equal(bindings.calls[0].args[6], "");
  assert.equal(bindings.calls[0].args[0], "é😀");
});
