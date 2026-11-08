// Exercise the production facade with generated-binding doubles. Real parser
// and render behavior is covered by tests/wasm_api_test.rs; these checks cover
// precedence, ABI forwarding and ownership across asynchronous initialization.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { SourceTextModule, SyntheticModule } from "node:vm";

const utf8 = value => new TextEncoder().encode(value);
const SOURCE = "---\ntitle: Metadata title\nauthor: Source author\nlang: de\ntoc: true\ntoc_depth: 2\n---\n# Heading\n\n## Child\n";
const DEFAULTS = Object.freeze({ title: "Metadata title", author: "Source author", lang: "de", toc: true, tocDepth: 2 });
const routes = [
  { method: "renderHtml", name: "html", options: {}, slots: { title: 3, lang: 16, toc: 17, tocDepth: 18 } },
  { method: "renderPdf", name: "pdf", options: {}, slots: { title: 3, author: 4, lang: 22, toc: 23, tocDepth: 24 } },
  { method: "renderPdf", name: "archival", options: { pdfA: "2b", pdfAStrict: true }, slots: { title: 3, author: 4, lang: 22, toc: 23, tocDepth: 24 } },
  { method: "renderEpub", name: "epub", options: {}, slots: { title: 3, author: 20, lang: 4, toc: 7, tocDepth: 8 } },
];

function gate() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

async function fixture({ metadata = () => DEFAULTS, advancedEpub = true, hold } = {}) {
  const calls = [], sources = [];
  let frees = 0;
  const render = (name, format) => {
    const binding = (...args) => {
      calls.push({ name, args });
      return {
        format, extension: format, sourceLength: utf8(args[0]).length,
        mimeType: { html: "text/html; charset=utf-8", pdf: "application/pdf", epub: "application/epub+zip" }[format],
        bytes: utf8(`binding-double:${name}`), diagnosticsJson: () => "[]", free() { frees++; },
      };
    };
    Object.defineProperty(binding, "length", { value: 39 });
    return binding;
  };
  const bindings = {
    default: () => hold?.promise ?? Promise.resolve(),
    renderHtmlConfiguredAdvanced: render("html", "html"),
    renderPdfConfiguredMulti: render("pdf", "pdf"),
    renderPdfConfiguredArchival: render("archival", "pdf"),
    renderEpubConfigured: render("epub-legacy", "epub"),
    renderSvgConfigured() {}, accessibilityAudit() {}, capabilities() {},
    documentStats() {}, renderSemanticDiffHtml() {}, searchIndex() {}, semanticDiff() {},
  };
  if (advancedEpub) {
    bindings.renderEpubConfiguredAdvanced = render("epub", "epub");
    bindings.renderEpubConfiguredMetadata = render("epub", "epub");
  }
  if (metadata !== null) bindings.sourceMetadata = source => {
    sources.push(source);
    return JSON.stringify(metadata(source));
  };
  const native = new SyntheticModule(Object.keys(bindings), function() {
    for (const [name, value] of Object.entries(bindings)) this.setExport(name, value);
  });
  const helpers = new Map();
  for (const file of ["pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"])
    helpers.set(`./${file}`, new SourceTextModule(await readFile(new URL(file, import.meta.url), "utf8")));
  const module = new SourceTextModule(await readFile(new URL("franken_markdown.js", import.meta.url), "utf8"));
  await module.link(name => name === "./pkg/franken_markdown.js" ? native : helpers.get(name));
  await module.evaluate();
  return { api: module.namespace, calls, sources, get frees() { return frees; } };
}

for (const route of routes) {
  test(`${route.name} inherits source defaults through its existing render binding`, async () => {
    const f = await fixture();
    const result = await f.api[route.method](SOURCE, route.options);
    assert.equal(f.calls.length, 1);
    assert.equal(f.calls[0].name, route.name);
    for (const [key, index] of Object.entries(route.slots))
      assert.equal(f.calls[0].args[index], DEFAULTS[key], key);
    assert.deepEqual(f.sources, [SOURCE]);
    assert.equal(f.calls[0].args[0], SOURCE, "source and diagnostic offsets remain intact");
    assert.equal(result.sourceLength, utf8(SOURCE).length);
    if (route.name === "archival") assert.equal(result.pdfAStrict, true);
    assert.equal(f.frees, 1);
  });

  test(`${route.name} explicit settings including false override source defaults`, async () => {
    const f = await fixture();
    const explicit = { title: "  Host title  ", author: "Host author", lang: "fr", toc: false, tocDepth: 4 };
    await f.api[route.method](SOURCE, { ...route.options, ...explicit });
    for (const [key, index] of Object.entries(route.slots))
      assert.equal(f.calls[0].args[index], explicit[key], key);
    assert.equal(f.sources.length, 0, "fully explicit settings need no metadata pass");
  });

  test(`${route.name} partial settings keep false while inheriting other fields`, async () => {
    const f = await fixture();
    await f.api[route.method](SOURCE, { ...route.options, title: "Host title", toc: false });
    for (const [key, index] of Object.entries(route.slots))
      assert.equal(f.calls[0].args[index], key === "title" ? "Host title" : key === "toc" ? false : DEFAULTS[key], key);
  });

  for (const warm of [false, true]) test(`${route.name} captures metadata choices once before ${warm ? "warm" : "cold"} initialization yields`, async () => {
    const hold = warm ? undefined : gate();
    const f = await fixture({ hold });
    if (warm) await f.api.init();
    let text = SOURCE, sourceReads = 0;
    const source = { toString() { sourceReads++; return text; } };
    const values = { title: undefined, author: "Host author", lang: "fr", toc: false, tocDepth: undefined };
    const reads = {};
    const options = { ...route.options };
    for (const key of Object.keys(route.slots)) Object.defineProperty(options, key, {
      get() { reads[key] = (reads[key] ?? 0) + 1; return values[key]; },
    });
    const pending = f.api[route.method](source, options);
    text = "# Changed source";
    Object.assign(values, { title: "Changed title", author: "Changed author", lang: "es", toc: true, tocDepth: 6 });
    assert.equal(f.calls.length, 0);
    hold?.resolve();
    await pending;
    assert.deepEqual(f.sources, [SOURCE]);
    assert.equal(sourceReads, 1);
    const expected = { ...DEFAULTS, author: "Host author", lang: "fr", toc: false };
    for (const [key, index] of Object.entries(route.slots)) {
      assert.equal(reads[key], 1, `${key} getter was read once`);
      assert.equal(f.calls[0].args[index], expected[key], key);
    }
  });
}

test("false source defaults and unknown-only metadata keep TOC disabled", async () => {
  for (const metadata of [{ toc: false }, {}]) {
    const f = await fixture({ metadata: () => metadata });
    for (const route of routes) {
      await f.api[route.method](SOURCE, route.options);
      const args = f.calls.at(-1).args;
      assert.equal(args[route.slots.toc], false);
      assert.equal(args[route.slots.title], undefined);
    }
  }
});

test("concurrent source defaults remain scoped to their submitted documents", async () => {
  const hold = gate();
  const f = await fixture({ hold, metadata: source => source === SOURCE ? DEFAULTS : { title: "Second", toc: false } });
  const first = f.api.renderPdf(SOURCE);
  const second = f.api.renderPdf("# Other document");
  hold.resolve();
  await Promise.all([first, second]);
  assert.equal(f.calls[0].args[3], DEFAULTS.title);
  assert.equal(f.calls[0].args[23], true);
  assert.equal(f.calls[1].args[3], "Second");
  assert.equal(f.calls[1].args[23], false);
});

test("legacy generated packages preserve their existing behavior for frontmatter and leading rules", async () => {
  const f = await fixture({ metadata: null, advancedEpub: false });
  for (const source of [SOURCE, "---\n\nAn ordinary paragraph."]) {
    for (const route of routes) {
      await f.api[route.method](source, route.options);
      assert.equal(f.calls.at(-1).args[0], source);
      assert.equal(f.calls.at(-1).args[3], undefined);
    }
  }
  assert.equal(f.sources.length, 0);
});

test("inherited EPUB navigation cannot disappear through the legacy narrow binding", async () => {
  const f = await fixture({ advancedEpub: false });
  await assert.rejects(f.api.renderEpub(SOURCE), error => error.code === "UNSUPPORTED_WASM_PACKAGE");
  assert.equal(f.calls.length, 0);
  const narrow = await fixture({ advancedEpub: false, metadata: () => ({ title: "Manual", lang: "de" }) });
  await narrow.api.renderEpub(SOURCE);
  assert.equal(narrow.calls[0].name, "epub-legacy");
  assert.equal(narrow.calls[0].args.length, 6);
  assert.equal(narrow.calls[0].args[3], "Manual");
  assert.equal(narrow.calls[0].args[4], "de");
});

test("invalid core metadata refuses the render instead of silently changing settings", async () => {
  for (const metadata of [null, [], { toc: "false" }, { tocDepth: 7 }, { title: 12 }]) {
    const f = await fixture({ metadata: () => metadata });
    await assert.rejects(f.api.renderHtml(SOURCE), /Invalid source metadata/);
    assert.equal(f.calls.length, 0);
  }
});
