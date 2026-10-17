import assert from "node:assert/strict";
import test from "node:test";
import { bookLinkOptions, prepareBookInput } from "../book_session.mjs";
import { createRetainedBook, createCachedBookRenderer } from "../book_retained.mjs";
import { nativeBookFixture, chapters, included, geometry, decode, code } from "./book_pdfa_fixture.mjs";
const preview = async (engine, files, options) => engine.renderBookSite(files, options);

for (const strict of [false, true]) test(`archival one-shot uses the chapter-aware ABI with assets, metadata and paper: strict=${strict}`, async () => {
  const { api, state } = nativeBookFixture();
  const output = await api.renderBookPdf(chapters(), { pdfA: "2b", pdfAStrict: strict, page: geometry(),
    includeSources: included(), title: "Manual", author: "Writer", lang: "fr", toc: true,
    metadataEpochSeconds: 0, baseFontSize: 12, running: { footer: { center: "{page}" } },
    images: [{ destination: "guide/figure.png", bytes: new Uint8Array([1, 2]) }],
    fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([3, 4]), weight: 550 }] });
  const got = decode(output);
  assert.equal(got.pdfA, "2b"); assert.equal(got.strict, strict);
  assert.deepEqual(got.files, chapters()); assert.deepEqual(got.includes, included());
  assert.deepEqual(got.page, [720, 540, 18, 24, 30, 36]);
  assert.deepEqual(got.images, [["guide/figure.png", [1, 2]]]);
  assert.deepEqual(got.fonts, [["body-regular", [3, 4]]]);
  assert.deepEqual(got.settings.metadata, ["Manual", "Writer", "fr"]);
  assert.equal(got.settings.pdf[7], 0); assert.equal(got.settings.pdf[1], 12);
  assert.equal(output.pdfA, "2b"); assert.equal(output.pdfAStrict, strict);
  assert.equal(output.format, "book-pdf"); assert.equal(output.filename("manual"), "manual.pdf");
  assert.equal(output.mimeType, "application/pdf"); assert.equal(state.freed, 1);
  assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
  assert.equal(state.calls.length, 1); assert.equal(state.created, 1);
});

for (const options of [{}, { pdfA: "off" }, { pdfAStrict: false }, { pdfA: "off", pdfAStrict: false }]) {
  test(`ordinary defaults retain the old ABI and envelope: ${JSON.stringify(options)}`, async () => {
    const { api, state } = nativeBookFixture({ old: true });
    const output = await api.renderBookPdf(chapters(), options);
    assert.equal(decode(output).pdfA, "off"); assert.equal(Object.hasOwn(output, "pdfA"), false);
    assert.equal(Object.hasOwn(output, "pdfAStrict"), false); assert.equal(state.freed, 1);
    assert.deepEqual(prepareBookInput(chapters(), options), prepareBookInput(chapters()));
  });
}

for (const options of [
  { pdfA: null }, { pdfA: "" }, { pdfA: "2B" }, { pdfA: "3b" }, { pdfA: true },
  { pdfA: {} }, { pdfAStrict: true }, { pdfA: "off", pdfAStrict: true },
  { pdfA: "2b", pdfAStrict: "true" }, { pdfA: "2b", pdfAStrict: 1 }, { pdfAStrict: null },
]) test(`invalid policy refuses before native initialization and asset access: ${JSON.stringify(options)}`, async () => {
  const { api, state } = nativeBookFixture();
  let reads = 0;
  await assert.rejects(api.createBook(chapters(), { ...options,
    get images() { reads++; throw Error("must not read"); } }), code("INVALID_OPTIONS"));
  assert.equal(state.loads, 0); assert.equal(state.created, 0); assert.equal(reads, 0);
});

test("policy normalizes idempotently across complete captures and structured-clone boundaries", () => {
  const first = prepareBookInput(chapters(), { pdfA: "2b", includeSources: included() });
  assert.equal(first.options.pdfAStrict, false);
  const copy = structuredClone(first), second = prepareBookInput(copy.files, copy.options);
  assert.deepEqual(second, first);
});

test("async creation captures policy and geometry once, without retaining host containers", async () => {
  let release, reads = 0;
  const { api, state } = nativeBookFixture({ load: () => new Promise(r => { release = r; }) });
  const page = geometry(), files = chapters(), bytes = new Uint8Array([1, 2]);
  const options = { pdfA: "2b", get pdfAStrict() { reads++; return true; }, page,
    images: [{ destination: "image.png", bytes }], includeSources: included() };
  const pending = api.createBook(files, options);
  options.pdfA = "off"; page.size.widthPt = 900; files[0].source = "changed"; bytes.fill(0);
  release(); const session = await pending;
  try {
    const got = decode(session.renderPdf());
    assert.equal(reads, 1); assert.equal(got.pdfA, "2b"); assert.equal(got.strict, true);
    assert.equal(got.page[0], 720); assert.deepEqual(got.files, chapters());
    assert.deepEqual(got.images, [["image.png", [1, 2]]]); assert.equal(state.created, 1);
  } finally { session.dispose(); }
});

test("export-local policy overrides preserve session defaults, page, sources and native revision", async () => {
  const { api, state } = nativeBookFixture();
  const session = await api.createBook(chapters(), { pdfA: "2b", pdfAStrict: true, page: geometry() });
  try {
    const before = session.sourceRevision;
    assert.equal(decode(session.renderPdf()).strict, true);
    assert.equal(decode(session.renderPdf({ pdfAStrict: false })).strict, false);
    const ordinary = session.renderPdf({ pdfA: "off" });
    assert.equal(decode(ordinary).pdfA, "off"); assert.equal(ordinary.pdfA, undefined);
    assert.deepEqual(decode(ordinary).page, [720, 540, 18, 24, 30, 36]);
    const rotated = decode(session.renderPdf({ page: { orientation: "landscape" } }));
    assert.deepEqual(rotated.page, [792, 612, 72, 72, 72, 72]); assert.equal(rotated.strict, true);
    assert.equal(decode(session.renderPdf()).strict, true);
    assert.deepEqual(decode(session.renderPdf()).page, [720, 540, 18, 24, 30, 36]);
    assert.equal(session.sourceRevision, before); assert.equal(state.created, 1); assert.equal(state.updates, 0);
  } finally { session.dispose(); }
});

test("one-off archive on an ordinary session returns to the original ordinary exporter", async () => {
  const { api } = nativeBookFixture(); const session = await api.createBook(chapters());
  try {
    const before = session.renderPdf();
    assert.equal(session.renderPdf({ pdfA: "2b" }).pdfAStrict, false);
    const after = session.renderPdf();
    assert.deepEqual(after.bytes, before.bytes); assert.equal(after.pdfA, undefined);
  } finally { session.dispose(); }
});

test("export policy admission rejects accessors, unknown keys and contradictory overrides", async () => {
  const { api, state } = nativeBookFixture(); const session = await api.createBook(chapters(), { pdfA: "2b" });
  let reads = 0;
  try {
    for (const value of [null, [], { pdfa: "2b" }, { pdfA: "2b", title: "hidden edit" },
      { pdfA: "off", pdfAStrict: true }, { get pdfA() { reads++; return "2b"; } },
      { [Symbol("pdfA")]: "2b" }, { pdfA: null }])
      assert.throws(() => session.renderPdf(value), code("INVALID_OPTIONS"));
    assert.equal(reads, 0); assert.equal(state.calls.length, 0);
    assert.equal(session.renderPdf().pdfA, "2b");
  } finally { session.dispose(); }
});

test("reentrant disposal during policy admission never invokes a freed native renderer", async () => {
  const { api, state } = nativeBookFixture(); const session = await api.createBook(chapters());
  const request = new Proxy({ pdfA: "2b" }, { ownKeys(target) { session.dispose(); return Reflect.ownKeys(target); } });
  assert.throws(() => session.renderPdf(request), /disposed/);
  assert.equal(state.calls.length, 0); assert.equal(state.freed, 1);
});

test("old native API refuses archival export, without disabling ordinary PDF, EPUB or site", async () => {
  const { api, state } = nativeBookFixture({ old: true });
  await assert.rejects(api.renderBookPdf(chapters(), { pdfA: "2b" }), code("UNSUPPORTED_BOOK_PDFA"));
  assert.equal(state.freed, 1); assert.equal(state.calls.length, 0);
  const session = await api.createBook(chapters(), { pdfA: "2b", pdfAStrict: true });
  try {
    assert.throws(() => session.renderPdf(), code("UNSUPPORTED_BOOK_PDFA"));
    assert.equal(decode(session.renderPdf({ pdfA: "off" })).pdfA, "off");
    assert.equal(session.renderEpub().pdfA, undefined); assert.equal(session.renderSite().pdfA, undefined);
  } finally { session.dispose(); }
});

test("native strict failure is terminal, bounded, does not fall back and leaves a direct session reusable", async () => {
  const { api, state } = nativeBookFixture(); const session = await api.createBook(chapters(), { pdfA: "2b", pdfAStrict: true });
  try {
    state.failure = "native refusal " + "x".repeat(3000);
    assert.throws(() => session.renderPdf(), e => e.message.length === 2048);
    assert.equal(state.calls.length, 1); assert.equal(state.calls[0].pdfA, "2b");
    state.failure = null; assert.equal(session.renderPdf().pdfAStrict, true);
    assert.equal(session.sourceRevision, 0);
  } finally { session.dispose(); }
  state.failure = Object.assign(Error("native policy failed"), { code: "NATIVE_ARCHIVAL_ERROR" });
  await assert.rejects(api.renderBookPdf(chapters(), { pdfA: "2b" }), e => e === state.failure);
  assert.equal(state.freed, 2);
});

test("archival defaults survive selective source updates and structural source replacement", async () => {
  const { api, state } = nativeBookFixture(); const session = await api.createBook(chapters(), {
    pdfA: "2b", pdfAStrict: true, includeSources: included(), page: geometry() });
  try {
    session.updateSources([{ path: "shared.txt", source: "Revised 🙂" }], { expectedRevision: 0 });
    const updated = decode(session.renderPdf()); assert.equal(updated.strict, true);
    assert.equal(updated.includes[0].source, "Revised 🙂"); assert.equal(session.sourceRevision, 1);
    const next = chapters().reverse(); next.push({ path: "appendix.md", source: "# Appendix" });
    session.replaceSources(next, { includeSources: [], expectedRevision: 1 });
    const replaced = decode(session.renderPdf()); assert.equal(replaced.strict, true);
    assert.deepEqual(replaced.files, next); assert.deepEqual(replaced.includes, []);
    assert.deepEqual(replaced.page, [720, 540, 18, 24, 30, 36]); assert.equal(state.created, 1);
  } finally { session.dispose(); }
});

test("PDF policy never changes EPUB/site bytes or source-only navigation admission", async () => {
  const { api } = nativeBookFixture();
  for (const method of ["renderBookEpub", "renderBookSite"]) {
    const a = await api[method](chapters()), b = await api[method](chapters(), { pdfA: "2b", pdfAStrict: true });
    assert.deepEqual(a.bytes, b.bytes); assert.equal(b.pdfA, undefined);
  }
  const options = { get pdfA() { throw Error("not source"); }, get pdfAStrict() { throw Error("not source"); } };
  assert.deepEqual(prepareBookInput(chapters(), bookLinkOptions(options)), prepareBookInput(chapters()));
  assert.equal((await api.checkBookLinks(chapters(), options)).format, "book-links");
});

test("retained output/history cache separates ordinary, archival, and strict policies", async t => {
  const { api, state } = nativeBookFixture();
  const retained = createRetainedBook(api, preview), cache = createCachedBookRenderer(retained, preview);
  t.after(() => cache.clear());
  const render = async (options, files = chapters()) => {
    const input = prepareBookInput(files, { includeSources: included(), ...options });
    return cache.render(input.files, input.options, "pdf");
  };
  for (const options of [{}, { pdfA: "2b" }, { pdfA: "2b", pdfAStrict: true }, {}]) {
    const before = state.calls.length;
    const a = await render(options), b = await render(options);
    assert.equal(state.calls.length, before + 1); assert.deepEqual(a.bytes, b.bytes);
    assert.equal(b.pdfA, options.pdfA); assert.equal(b.pdfAStrict, options.pdfA ? options.pdfAStrict === true : undefined);
    assert.notEqual(a.bytes.buffer, b.bytes.buffer);
  }
  assert.equal(state.created, 4); assert.equal(cache.cacheStats.hits, 4);
  assert.equal(cache.cacheStats.snapshots, 1);
});

test("archival history hits skip native mutations but the next miss synchronizes the actual native state", async t => {
  const { api, state } = nativeBookFixture(); const retained = createRetainedBook(api, preview);
  const cache = createCachedBookRenderer(retained, preview); t.after(() => cache.clear());
  const render = async (files, kind = "pdf") => {
    const input = prepareBookInput(files, { pdfA: "2b", pdfAStrict: true, includeSources: included() });
    return cache.render(input.files, input.options, kind);
  };
  const first = await render(chapters()); const changed = chapters(); changed[0].source += "\nChanged";
  await render(changed); const updates = state.updates;
  const restored = await render(chapters()); assert.deepEqual(restored.bytes, first.bytes);
  assert.equal(restored.pdfAStrict, true); assert.equal(state.updates, updates);
  const epub = await render(chapters(), "epub"); assert.deepEqual(decode(epub).files, chapters());
  assert.equal(state.updates, updates + 1); assert.equal(state.created, 1);
});

test("retained archival failures retire native state and clear old outputs rather than falling back", async t => {
  const { api, state } = nativeBookFixture(); const retained = createRetainedBook(api, preview);
  const cache = createCachedBookRenderer(retained, preview); t.after(() => cache.clear());
  const render = async options => { const i = prepareBookInput(chapters(), options); return cache.render(i.files, i.options, "pdf"); };
  await render({}); state.failure = Object.assign(Error("strict rejected"), { code: "STRICT_REFUSAL" });
  await assert.rejects(render({ pdfA: "2b", pdfAStrict: true }), code("STRICT_REFUSAL"));
  assert.equal(cache.cacheStats.entries, 0); assert.equal(state.freed, 2);
  assert.equal(state.calls.length, 2); assert.equal(state.calls.at(-1).pdfA, "2b");
  state.failure = null; assert.equal((await render({ pdfA: "2b", pdfAStrict: true })).pdfAStrict, true);
  assert.equal(state.created, 3);
});

test("a legacy renderer facade cannot silently ignore the archival policy in a retained book", async t => {
  let disposed = 0;
  const retained = createRetainedBook({ async createBook() { return {
    renderPdf: () => ({ bytes: Uint8Array.of(1), sourceLength: 1 }), dispose: () => disposed++,
  }; } }, preview); t.after(() => retained.clear());
  const input = prepareBookInput(chapters(), { pdfA: "2b", pdfAStrict: true });
  await assert.rejects(retained.render(input.files, input.options, "pdf"), code("UNSUPPORTED_BOOK_PDFA"));
  assert.equal(disposed, 1);
});
