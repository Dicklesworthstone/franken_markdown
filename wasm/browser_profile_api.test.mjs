// Execute the complete production root facade with explicit generated-binding
// doubles. V8 linking detects absent named exports; no Rust rendering is faked
// as native evidence. Run: node --experimental-vm-modules --test this-file.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { SourceTextModule, SyntheticModule } from "node:vm";

const source = await readFile(new URL("./franken_markdown.js", import.meta.url), "utf8");
const paper = await readFile(new URL("./pdf_page.mjs", import.meta.url), "utf8");
const utf8 = text => new TextEncoder().encode(text);
const files = [{ path: "guide.md", source: "# Guide" }];
const unsupported = feature => error => error.code === "UNSUPPORTED_WASM_PACKAGE"
  && error.message.includes(feature);
const gate = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  promise.catch(() => {});
  return { promise, resolve, reject };
};

async function fixture(optional = {}, initialization = undefined) {
  const calls = [];
  let inits = 0, frees = 0;
  const output = (format, args) => {
    const mimeType = { html: "text/html; charset=utf-8", pdf: "application/pdf", svg: "image/svg+xml",
      epub: "application/epub+zip", "book-pdf": "application/pdf", "book-site": "application/zip",
      "interactive-html": "text/html; charset=utf-8" }[format];
    return { format, mimeType, extension: format === "book-pdf" ? "pdf" : format === "book-site" ? "zip"
      : format === "interactive-html" ? "html" : format,
      sourceLength: utf8(String(args[0] ?? "")).length,
      bytes: utf8(`native-double:${format}`), diagnosticsJson: () => "[]", free() { frees++; } };
  };
  const render = (name, format) => (...args) => { calls.push({ name, args }); return output(format, args); };
  const bindings = {
    default: () => { inits++; return initialization ?? Promise.resolve(); },
    capabilities: () => JSON.stringify({ outputs: ["html", "pdf"] }),
    renderHtmlConfiguredAdvanced: render("html", "html"),
    renderPdfConfiguredMulti: render("pdf", "pdf"),
    renderEpubConfigured: render("epub", "epub"),
    renderSvgConfigured: render("svg", "svg"),
    accessibilityAudit: () => "{}", documentStats: () => "{}", searchIndex: () => "{}",
    semanticDiff: () => "{}", renderSemanticDiffHtml: render("diff", "html"),
  };
  for (const [name, value] of Object.entries(optional)) {
    const formats = { renderBookPdf: "book-pdf", renderBookPdfConfiguredPage: "book-pdf",
      renderBookSite: "book-site", renderInteractiveHtmlConfigured: "interactive-html" };
    bindings[name] = value === true ? render(name, formats[name]) : value;
  }
  const native = new SyntheticModule(Object.keys(bindings), function() {
    for (const [name, value] of Object.entries(bindings)) this.setExport(name, value);
  });
  const module = new SourceTextModule(source, { identifier: "production-root" });
  const geometry = new SourceTextModule(paper, { identifier: "production-paper" });
  await module.link(specifier => {
    if (specifier === "./pkg/franken_markdown.js") return native;
    if (specifier === "./pdf_page.mjs") return geometry;
    throw new Error(`unexpected import: ${specifier}`);
  });
  await module.evaluate();
  return { api: module.namespace, calls, bindings,
    get inits() { return inits; }, get frees() { return frees; } };
}

// There are deliberately NO book or workspace names in the synthetic module.
// The previous root facade fails module linking even for plain HTML/PDF.
test("render-only bindings load and keep all four single-document formats working", async () => {
  const f = await fixture();
  assert.equal(f.inits, 0);
  for (const [method, format] of [["renderHtml", "html"], ["renderPdf", "pdf"],
    ["renderSvg", "svg"], ["renderEpub", "epub"]]) {
    const result = await f.api[method]("# Hello", { font: "serif" });
    assert.equal(result.format, format);
    assert.equal(result.text(), `native-double:${format}`);
    assert.equal(f.calls.at(-1).args[0], "# Hello");
    assert.equal(f.calls.at(-1).args[1], "serif");
  }
  assert.equal(f.frees, 4);
  assert.equal(f.inits, 1);
  assert.deepEqual((await f.api.capabilities()).outputs, ["html", "pdf"]);
});

for (const [method, feature] of [["renderBookPdf", "wasm-book"], ["renderBookSite", "wasm-book"],
  ["renderInteractiveHtml", "wasm-workspace"]]) {
  test(`${method} refuses missing features before input access or WASM initialization`, async () => {
    const f = await fixture();
    const poison = new Proxy({}, { get() { throw new Error("input must not be read"); } });
    await assert.rejects(f.api[method](poison, poison), unsupported(feature));
    assert.equal(f.inits, 0);
    assert.equal(f.calls.length, 0);
  });
}

test("createRenderer keeps optional methods callable with actionable refusal", async () => {
  const f = await fixture();
  const renderer = await f.api.createRenderer();
  assert(Object.isFrozen(renderer));
  assert.equal((await renderer.renderHtml("hello")).format, "html");
  await assert.rejects(renderer.renderBookPdf(files), unsupported("wasm-book"));
  await assert.rejects(renderer.renderInteractiveHtml("hello"), unsupported("wasm-workspace"));
  assert.equal(f.calls.length, 1);
});

test("legacy book PDF keeps its original ABI and defaults without workspace exports", async () => {
  const f = await fixture({ renderBookPdf: true });
  const result = await f.api.renderBookPdf(files, { title: "  Manual  ", author: "Writer", font: "serif" });
  assert.equal(result.format, "book-pdf");
  assert.deepEqual(f.calls[0].args, [["guide.md"], ["# Guide"], "  Manual  ", "Writer", "serif", undefined, undefined, true]);
  assert.equal(f.frees, 1);
});

test("configured book PDF keeps asset packing, geometry and ownership across initialization", async () => {
  const hold = gate(), f = await fixture({ renderBookPdfConfiguredPage: true }, hold.promise);
  const payload = new Uint8Array([99, 1, 2, 99]);
  const input = [{ path: "one.md", source: "# One" }];
  const options = { page: { size: "a4", margins: 36 }, title: "before",
    pdfImages: [{ destination: "plot.svg", bytes: payload.subarray(1, 3) }] };
  const pending = f.api.renderBookPdf(input, options);
  input[0].source = "# Changed"; options.title = "after"; payload.fill(9);
  hold.resolve();
  assert.equal((await pending).format, "book-pdf");
  const args = f.calls[0].args;
  assert.equal(f.calls[0].name, "renderBookPdfConfiguredPage");
  assert.deepEqual(args[1], ["# One"]);
  assert.equal(args[4], "before");
  assert.deepEqual([...args[10]], [1, 2]);
  assert.deepEqual([...args.at(-1)], [210 * 72 / 25.4, 297 * 72 / 25.4, 36, 36, 36, 36]);
});

test("a configured-only package never calls its different ABI with legacy arguments", async () => {
  const f = await fixture({ renderBookPdfConfiguredPage: true });
  await assert.rejects(f.api.renderBookPdf(files), unsupported("wasm-book"));
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test("legacy-only book PDF refuses advanced options without falling back or initializing", async () => {
  const f = await fixture({ renderBookPdf: true });
  await assert.rejects(f.api.renderBookPdf(files, { page: { size: "a4" } }),
    error => error.code === "UNSUPPORTED_WASM_PACKAGE");
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test("legacy book site keeps its warning and narrow ABI without unrelated features", async () => {
  const f = await fixture({ renderBookSite: true });
  const result = await f.api.renderBookSite(files, { title: "Title" });
  assert.equal(result.format, "book-site");
  assert.match(result.diagnostics[0].message, /Legacy book site renderer/);
  assert.deepEqual(f.calls[0].args, [["guide.md"], ["# Guide"], "Title", undefined, undefined, undefined]);
});

test("canonical-only site export remains usable without the legacy book export", async () => {
  const calls = [], bytes = new Uint8Array([80, 75, 3, 4, 5]);
  const f = await fixture({ renderBookSitePublication: (...args) => { calls.push(args); return bytes; } });
  const result = await f.api.renderBookSite(files, { toc: true });
  assert.equal(result.format, "book-site");
  assert.deepEqual(result.bytes, bytes);
  assert.equal(result.diagnostics.length, 0);
  assert.equal(calls.length, 1);
});

test("legacy site rejects publication-only features before initialization", async () => {
  const f = await fixture({ renderBookSite: true });
  await assert.rejects(f.api.renderBookSite(files, { toc: true }), error => error.code === "UNSUPPORTED_WASM_PACKAGE");
  assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
});

test("workspace-only build captures source and metadata before initialization yields", async () => {
  const hold = gate(), f = await fixture({ renderInteractiveHtmlConfigured: true }, hold.promise);
  let text = "# Before";
  const input = { toString: () => text }, options = { title: "  Before  ", font: "serif", typeSize: "lg" };
  const pending = f.api.renderInteractiveHtml(input, options);
  text = "# After"; options.title = "After"; options.font = "sans";
  hold.resolve();
  const result = await pending;
  assert.equal(result.format, "interactive-html");
  assert.deepEqual(f.calls[0].args, ["# Before", "serif", undefined, "  Before  ", undefined, 1.125]);
  assert.equal(f.frees, 1);
});

test("full package uses the same optional paths and does not silently catch render failures", async () => {
  const failure = new Error("native failed");
  const f = await fixture({ renderBookPdf: true, renderBookSite: true,
    renderInteractiveHtmlConfigured: () => { throw failure; } });
  await f.api.renderBookPdf(files);
  await f.api.renderBookSite(files);
  await assert.rejects(f.api.renderInteractiveHtml("x"), error => error === failure);
  assert.deepEqual(f.calls.map(call => call.name), ["renderBookPdf", "renderBookSite"]);
});

test("nonfunction optional exports are not treated as supported", async () => {
  const f = await fixture({ renderBookPdf: 1, renderBookSite: {}, renderInteractiveHtmlConfigured: "yes" });
  await assert.rejects(f.api.renderBookPdf(files), unsupported("wasm-book"));
  await assert.rejects(f.api.renderBookSite(files), unsupported("wasm-book"));
  await assert.rejects(f.api.renderInteractiveHtml("x"), unsupported("wasm-workspace"));
  assert.equal(f.inits, 0);
});
