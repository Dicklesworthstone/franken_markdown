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

async function fixture({ metadata = () => DEFAULTS, advancedEpub = true, advancedInteractive = true, hold } = {}) {
  const calls = [], sources = [];
  let frees = 0;
  const render = (name, format) => {
    const binding = (...args) => {
      calls.push({ name, args });
      return {
        format, extension: format === "interactive-html" ? "html" : format, sourceLength: utf8(args[0]).length,
        mimeType: { html: "text/html; charset=utf-8", "interactive-html": "text/html; charset=utf-8", pdf: "application/pdf", epub: "application/epub+zip" }[format],
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
    renderInteractiveHtmlConfigured: render("interactive-legacy", "interactive-html"),
    renderSvgConfigured() {}, accessibilityAudit() {}, capabilities() {},
    documentStats() {}, renderSemanticDiffHtml() {}, searchIndex() {}, semanticDiff() {},
  };
  if (advancedEpub) {
    bindings.renderEpubConfiguredAdvanced = render("epub", "epub");
    bindings.renderEpubConfiguredMetadata = render("epub", "epub");
  }
  if (advancedInteractive)
    bindings.renderInteractiveHtmlConfiguredAdvanced = render("interactive", "interactive-html");
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

test("interactive exports inherit source title, language and navigation without rewriting Markdown", async () => {
  const f = await fixture();
  const result = await f.api.renderInteractiveHtml(SOURCE);
  assert.equal(f.calls.length, 1);
  const { name, args } = f.calls[0];
  assert.equal(name, "interactive");
  assert.equal(args.length, 20);
  assert.equal(args[0], SOURCE);
  assert.equal(args[3], DEFAULTS.title);
  assert.equal(args[5], false, "source metadata cannot enable raw HTML");
  assert.equal(args[16], DEFAULTS.lang);
  assert.equal(args[17], true);
  assert.equal(args[18], 2);
  assert.deepEqual(f.sources, [SOURCE]);
  assert.equal(result.sourceLength, utf8(SOURCE).length);
  assert.equal(result.filename("workspace"), "workspace.html");
  assert.equal(f.frees, 1);
});

test("interactive explicit metadata including false takes precedence over source defaults", async () => {
  const f = await fixture();
  await f.api.renderInteractiveHtml(SOURCE, {
    title: "  Host title  ", lang: "fr", toc: false, tocDepth: 4,
  });
  const { args } = f.calls[0];
  assert.equal(args[3], "  Host title  ");
  assert.equal(args[16], "fr");
  assert.equal(args[17], false);
  assert.equal(args[18], 4);
  assert.equal(f.sources.length, 0, "fully resolved settings need no source metadata query");
});

test("interactive empty titles inherit source defaults while explicit false stays disabled", async () => {
  const f = await fixture();
  await f.api.renderInteractiveHtml(SOURCE, { title: "", toc: false });
  const { args } = f.calls[0];
  assert.equal(args[3], DEFAULTS.title);
  assert.equal(args[16], "de");
  assert.equal(args[17], false);
  assert.equal(args[18], 2);
});

for (const warm of [false, true]) for (const resources of [false, true])
  test("interactive captures source, settings and resources before " + (warm ? "warm" : "cold")
      + " initialization with " + (resources ? "configured" : "inherited") + " navigation", async () => {
    const hold = warm ? undefined : gate();
    const f = await fixture({ hold });
    if (warm) await f.api.init();
    let sourceText = SOURCE, reads = 0;
    const source = { toString() { reads++; return sourceText; } };
    const image = new Uint8Array([9, 1, 2, 3, 8]);
    const font = new Uint8Array([7, 4, 5, 6]);
    const options = { toc: false };
    if (resources) Object.assign(options, {
      customCss: ".fmd { color: #234; }",
      pdfImages: [{ destination: "plot.png", bytes: new DataView(image.buffer, 1, 3) }],
      fontAssets: [{ slot: "body-regular", bytes: new DataView(font.buffer, 1, 2), weight: 650 }],
    });
    const pending = f.api.renderInteractiveHtml(source, options);
    sourceText = "# Later";
    Object.assign(options, { title: "Later", lang: "es", toc: true, tocDepth: 6, customCss: ".fmd { color: red; }" });
    if (resources) {
      image.fill(0); font.fill(0);
      options.pdfImages[0].destination = "later.png";
      options.fontAssets[0].weight = 100;
    }
    assert.equal(f.calls.length, 0);
    hold?.resolve();
    await pending;
    const { name, args } = f.calls[0];
    assert.equal(name, "interactive");
    assert.equal(reads, 1);
    assert.equal(args[0], SOURCE);
    assert.equal(args[3], DEFAULTS.title);
    assert.equal(args[16], DEFAULTS.lang);
    assert.equal(args[17], false);
    assert.equal(args[18], DEFAULTS.tocDepth);
    assert.deepEqual(f.sources, [SOURCE]);
    if (resources) {
      assert.equal(args[4], ".fmd { color: #234; }");
      assert.deepEqual(args[13], ["plot.png"]);
      assert.deepEqual([...args[14]], [1, 2, 3]);
      assert.deepEqual([...args[15]], [3]);
      assert.deepEqual([...args[7]], [4, 5]);
      assert.equal(args[12][0], 650);
    }
  });

test("interactive title and language defaults retain the original narrow binding shape", async () => {
  for (const metadata of [{ title: "  Manual  ", lang: "de" }, { toc: false }, {}]) {
    const f = await fixture({ advancedInteractive: false, metadata: () => metadata });
    await f.api.renderInteractiveHtml(SOURCE);
    const { name, args } = f.calls[0];
    assert.equal(name, "interactive-legacy");
    assert.equal(args.length, 6);
    assert.equal(args[0], SOURCE);
    assert.equal(args[3], metadata.title);
    assert.equal(args[4], metadata.lang);
  }
});

test("legacy interactive packages without source metadata keep their existing source behavior", async () => {
  const f = await fixture({ metadata: null, advancedInteractive: false });
  for (const source of [SOURCE, "---\n\nAn ordinary paragraph."]) {
    await f.api.renderInteractiveHtml(source);
    assert.equal(f.calls.at(-1).name, "interactive-legacy");
    assert.deepEqual(f.calls.at(-1).args, [source, undefined, undefined, undefined, undefined, undefined]);
  }
  assert.equal(f.sources.length, 0);
});

test("interactive inherited navigation requires a capable package instead of silently disappearing", async () => {
  for (const metadata of [{ toc: true }, { tocDepth: 2 }, DEFAULTS]) {
    const f = await fixture({ advancedInteractive: false, metadata: () => metadata });
    await assert.rejects(f.api.renderInteractiveHtml(SOURCE), { code: "UNSUPPORTED_WASM_PACKAGE" });
    assert.equal(f.calls.length, 0);
  }
  const disabled = await fixture({ advancedInteractive: false, metadata: () => ({ toc: true }) });
  await disabled.api.renderInteractiveHtml(SOURCE, { toc: false });
  assert.equal(disabled.calls[0].name, "interactive-legacy");
});

test("interactive invalid source metadata cannot reach either render binding", async () => {
  for (const metadata of [null, [], { title: 12 }, { lang: false }, { toc: "true" }, { tocDepth: 0 }, { tocDepth: 7 }]) {
    const f = await fixture({ metadata: () => metadata });
    await assert.rejects(f.api.renderInteractiveHtml(SOURCE), /Invalid source metadata/);
    assert.equal(f.calls.length, 0);
  }
});

test("interactive inherited metadata retains explicit UTF-8 and scalar limits", async () => {
  for (const [metadata, label] of [
    [{ title: "é".repeat(32769) }, /title/],
    [{ lang: "é".repeat(513) }, /lang/],
    [{ title: "broken\ud800" }, /title/],
    [{ lang: "broken\udfff" }, /lang/],
  ]) {
    const f = await fixture({ metadata: () => metadata });
    await assert.rejects(f.api.renderInteractiveHtml(SOURCE), label);
    assert.equal(f.calls.length, 0);
  }
});

test("concurrent interactive exports resolve metadata from their captured sources", async () => {
  const hold = gate();
  const f = await fixture({ hold, metadata: source => source === SOURCE ? DEFAULTS : { title: "Second", lang: "fr" } });
  const first = f.api.renderInteractiveHtml(SOURCE);
  const second = f.api.renderInteractiveHtml("# Other document");
  hold.resolve();
  await Promise.all([first, second]);
  assert.equal(f.calls[0].name, "interactive");
  assert.equal(f.calls[0].args[0], SOURCE);
  assert.equal(f.calls[0].args[3], DEFAULTS.title);
  assert.equal(f.calls[1].name, "interactive-legacy");
  assert.equal(f.calls[1].args[0], "# Other document");
  assert.equal(f.calls[1].args[3], "Second");
  assert.equal(f.calls[1].args[4], "fr");
});

test("interactive metadata inheritance retains data-only option admission", async () => {
  let reads = 0;
  const options = {};
  Object.defineProperty(options, "toc", { get() { reads++; return true; } });
  const f = await fixture();
  await assert.rejects(f.api.renderInteractiveHtml(SOURCE, options), /accessor/);
  assert.equal(reads, 0);
  assert.equal(f.sources.length, 0);
  assert.equal(f.calls.length, 0);
});
