import assert from "node:assert/strict";
import test from "node:test";
import { FlowError } from "./flow_session.mjs";
import { normalizeFlowExport, validateFlowExportResult, withFlowExports } from "./flow_export.mjs";

// Native snapshots and render bytes are doubles. The production export
// coordinator, shared worker normalizer and result validator are executed.
const MIME = { html: "text/html; charset=utf-8", pdf: "application/pdf", epub: "application/epub+zip" };
const code = (name) => (error) => error instanceof FlowError && error.code === name;
const token = () => ({ revision: "9007199254740993", layoutRevision: "9" });
const image = (id, destination, isResolved = true) => ({ kind: "image", requestId: String(id), destination, isResolved });
function fixture(items = [], payloads = new Map()) {
  const state = { source: "# Café\n\n![plot](plot.svg)", token: token(), closed: false, reads: [], calls: [] };
  const session = {
    get disposed() { return state.closed; },
    get source() { return state.source; },
    get token() { return { ...state.token }; },
    snapshot({ offset, limit, token: expected }) {
      assert.deepEqual(expected, state.token);
      state.reads.push(offset);
      const page = items.slice(offset, offset + limit);
      return { schemaVersion: 1, ...state.token, total: items.length, offset,
        nextOffset: offset + page.length < items.length ? offset + page.length : null, items: page };
    },
    assetBytes(id, revision) { assert.equal(revision, state.token.revision); return payloads.get(id); },
    dispose() { state.closed = true; },
  };
  const output = (format) => ({ format, mimeType: MIME[format], bytes: new Uint8Array([80, 75, 3, 4]),
    diagnostics: [{ severity: "warning", start: 0, end: 2, message: "example diagnostic" }] });
  const renderers = Object.fromEntries(Object.keys(MIME).map((format) => [format, async (source, options) => {
    state.calls.push({ format, source, options });
    return output(format);
  }]));
  return { state, session, renderers, output, wrap: () => withFlowExports(session, renderers, "serif") };
}

test("EPUB options use the shared worker normalizer without acquiring PDF-only defaults", () => {
  const settings = { title: "  Book  ", lang: "fr", toc: true, tocDepth: 3, customCss: "", maxOutputBytes: 4096 };
  const [format, normalized, identity] = normalizeFlowExport("epub", settings, token());
  assert.equal(format, "epub");
  assert.deepEqual(normalized, { ...settings, darkMode: "auto" });
  assert.deepEqual(identity, token());
  assert.ok(Object.isFrozen(normalized));
  settings.title = "later";
  assert.equal(normalized.title, "  Book  ");
  for (const options of [{ author: "x" }, { pageNumbers: true }, { metadataEpochSeconds: 0 },
    { allowRawHtml: true }, { fontAssets: [] }, { pdfImages: [] }, { customCss: "\ud800" },
    { customCss: "é".repeat(2 * 1024 * 1024 + 1) }, { tocDepth: 7 }]) {
    assert.throws(() => normalizeFlowExport("epub", options, token()), FlowError);
  }
  assert.throws(() => normalizeFlowExport("html", { customCss: "" }, token()), code("INVALID_OPTIONS"));
});

test("EPUB captures all image pages, deduplicates equal payloads and owns output", async () => {
  const payload = new Uint8Array([1, 2, 3]);
  const f = fixture([...Array.from({ length: 256 }, () => ({ kind: "text" })),
    image(1, "plot.svg"), image(2, "plot.svg"), image(3, "other.png")],
    new Map([["1", payload], ["2", payload.slice()], ["3", new Uint8Array([4])]]));
  const native = f.output("epub");
  f.renderers.epub = async (source, options) => { f.state.calls.push({ source, options }); return native; };
  const before = f.session.token;
  const api = f.wrap();
  const result = await api.exportDocument("epub", { title: "  Guide  ", customCss: ".fmd{}", toc: true }, before);
  assert.deepEqual(f.state.reads, [0, 256]);
  assert.equal(f.state.calls.length, 1);
  assert.deepEqual(f.state.calls[0].options.pdfImages.map((a) => [a.destination, [...a.bytes]]),
    [["plot.svg", [1, 2, 3]], ["other.png", [4]]]);
  assert.equal(f.state.calls[0].options.allowRawHtml, false);
  assert.equal(f.state.calls[0].options.customCss, ".fmd{}");
  assert.equal(f.state.calls[0].options.font, "serif");
  assert.equal(f.state.calls[0].source, f.state.source);
  assert.equal(result.assetCount, 2);
  assert.equal(result.assetBytes, 4);
  assert.equal(result.sourceLengthBytes, new TextEncoder().encode(f.state.source).length);
  assert.equal(result.mimeType, MIME.epub);
  assert.equal(result.format, "epub");
  assert.equal(validateFlowExportResult(result, before), result);
  native.bytes.fill(9); native.diagnostics[0].message = "changed";
  assert.deepEqual([...result.bytes], [80, 75, 3, 4]);
  assert.equal(result.diagnostics[0].message, "example diagnostic");
  assert.ok(Object.isFrozen(result));
  assert.ok(Object.isFrozen(result.diagnostics[0]));
  assert.deepEqual(f.session.token, before);
});

test("unresolved, dimension-only and ambiguous occurrence assets never reach EPUB rendering", async () => {
  for (const [items, payloads, expected] of [
    [[image(1, "plot.svg", false)], new Map(), "UNRESOLVED_EXPORT_ASSET"],
    [[image(1, "plot.svg")], new Map(), "UNRESOLVED_EXPORT_ASSET"],
    [[image(1, "plot.svg")], new Map([["1", new Uint8Array()]]), "UNRESOLVED_EXPORT_ASSET"],
    [[image(1, "plot.svg"), image(2, "plot.svg")],
      new Map([["1", new Uint8Array([1])], ["2", new Uint8Array([2])]]), "AMBIGUOUS_EXPORT_ASSET"],
  ]) {
    const f = fixture(items, payloads);
    await assert.rejects(f.wrap().exportDocument("epub", {}, f.session.token), code(expected));
    assert.equal(f.state.calls.length, 0);
    assert.deepEqual(f.session.token, token());
  }
});

test("source or asset-layout changes during an awaited EPUB export invalidate its result", async () => {
  for (const field of ["revision", "layoutRevision"]) {
    const payload = new Uint8Array([1, 2]);
    const f = fixture([image(1, "plot.svg")], new Map([["1", payload]]));
    let release, captured;
    f.renderers.epub = (source, options) => { captured = { source, options }; return new Promise((resolve) => { release = resolve; }); };
    const api = f.wrap(), before = api.token;
    const pending = api.exportDocument("epub", { customCss: "" }, before);
    payload.fill(9); f.state.source = "new source";
    f.state.token[field] = String(BigInt(f.state.token[field]) + 1n);
    assert.deepEqual([...captured.options.pdfImages[0].bytes], [1, 2]);
    assert.notEqual(captured.source, f.state.source);
    release(f.output("epub"));
    await assert.rejects(pending, code(field === "revision" ? "STALE_REVISION" : "STALE_LAYOUT"));
  }
});

test("one physical export is shared across formats and the gate releases after failure", async () => {
  const f = fixture(); let reject;
  f.renderers.epub = () => new Promise((_, no) => { reject = no; });
  const api = f.wrap(), pending = api.exportDocument("epub", {}, api.token);
  await assert.rejects(api.exportDocument("pdf", {}, api.token), code("EXPORT_BUSY"));
  reject(new Error("deliberate native failure"));
  await assert.rejects(pending, code("EXPORT_FAILED"));
  const pdf = await api.exportDocument("pdf", {}, api.token);
  assert.equal(pdf.format, "pdf");
  assert.equal(f.state.calls.length, 1);
});

test("output budgets, format and diagnostic checks also apply to EPUB", async () => {
  for (const [mutate, expected, options] of [
    [(out) => { out.bytes = new Uint8Array(9); }, "BUDGET_EXCEEDED", { maxOutputBytes: 8 }],
    [(out) => { out.bytes = new Uint8Array(); }, "INVALID_WASM_RESPONSE", {}],
    [(out) => { out.mimeType = MIME.pdf; }, "INVALID_WASM_RESPONSE", {}],
    [(out) => { out.format = "pdf"; }, "INVALID_WASM_RESPONSE", {}],
    [(out) => { out.diagnostics[0].end = 9999; }, "INVALID_WASM_RESPONSE", {}],
  ]) {
    const f = fixture(), output = f.output("epub"); mutate(output);
    f.renderers.epub = async () => output;
    await assert.rejects(f.wrap().exportDocument("epub", options, f.session.token), code(expected));
    assert.deepEqual(f.session.token, token());
  }
});

test("legacy renderers fail explicitly without substituting HTML or PDF", async () => {
  const f = fixture(); delete f.renderers.epub;
  const api = f.wrap();
  await assert.rejects(api.exportDocument("epub", {}, api.token), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(f.state.reads.length, 0);
  assert.equal(f.state.calls.length, 0);
  const nativeError = Object.assign(new Error("rebuild the native package"), { code: "UNSUPPORTED_WASM_PACKAGE" });
  f.renderers.epub = async () => { throw nativeError; };
  await assert.rejects(api.exportDocument("epub", {}, api.token),
    (error) => code("UNSUPPORTED_WASM_PACKAGE")(error) && error.cause === nativeError);
});

test("existing HTML and PDF export options and results retain their defaults", async () => {
  const f = fixture(), api = f.wrap();
  for (const format of ["html", "pdf"]) {
    const result = await api.exportDocument(format, {}, api.token);
    assert.equal(result.format, format);
    assert.equal(result.mimeType, MIME[format]);
    assert.deepEqual([...result.bytes], [80, 75, 3, 4]);
  }
  assert.equal(f.state.calls[0].options.darkMode, "auto");
  assert.equal(f.state.calls[1].options.metadataEpochSeconds, 0);
  assert.equal(f.state.calls[0].options.metadataEpochSeconds, undefined);
});

test("disposed and stale sessions refuse EPUB before invoking a renderer", async () => {
  const f = fixture(), api = f.wrap(), before = api.token;
  await assert.rejects(api.exportDocument("epub", {}, { ...before, revision: "1" }), code("STALE_REVISION"));
  api.dispose();
  await assert.rejects(api.exportDocument("epub", {}, before), code("SESSION_DISPOSED"));
  assert.equal(f.state.calls.length, 0);
  assert.equal(f.state.reads.length, 0);
});

test("EPUB acknowledgments require the captured token and exact MIME type", async () => {
  const f = fixture(), api = f.wrap(), before = api.token;
  const result = await api.exportDocument("epub", {}, before);
  for (const patch of [{ revision: "1" }, { layoutRevision: "1" }, { mimeType: "application/zip" },
    { format: "constructor" }, { bytes: new Uint16Array([1]) }, { assetCount: 1025 }]) {
    assert.throws(() => validateFlowExportResult({ ...result, ...patch }, before), FlowError);
  }
});
