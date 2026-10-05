// Production flow facade + exporter, with explicit native-session and renderer
// doubles. These tests do not establish generated-WASM or browser raster parity.
import assert from "node:assert/strict";
import test from "node:test";
import { createFlowAdapter } from "./flow_session.mjs";
import { normalizeFlowExport, validateFlowExportResult, withFlowExports } from "./flow_export.mjs";

const utf8 = text => new TextEncoder().encode(text);
const rejects = code => error => error.code === code;
const mime = { html: "text/html; charset=utf-8", pdf: "application/pdf",
  epub: "application/epub+zip", svg: "image/svg+xml" };
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  // A regression may refuse before the renderer ever awaits this test gate.
  promise.catch(() => {});
  return { promise, resolve, reject };
};
function fixture() {
  const calls = [], reads = [], assets = new Map();
  const raw = {
    revision: "9007199254740993", layoutRevision: "9007199254740997",
    source: "# Original 😀\n\n![Plot](plot.svg)", items: [],
    snapshotJson(revision, layoutRevision, offset, limit, glyphs) {
      reads.push({ revision, layoutRevision, offset, limit, glyphs });
      const end = Math.min(this.items.length, offset + limit);
      return JSON.stringify({ schemaVersion: 1, revision, layoutRevision,
        offset, total: this.items.length, nextOffset: end < this.items.length ? end : null,
        items: this.items.slice(offset, end) });
    },
    assetBytes(id, revision) {
      assert.equal(revision, this.revision);
      return assets.get(id);
    },
    free() { this.freed = true; },
  };
  const session = createFlowAdapter(raw);
  let hold = null, output = null;
  const renderers = Object.fromEntries(Object.keys(mime).map(format => [format,
    async (source, options) => {
      calls.push({ format, source, options });
      if (hold) await hold.promise;
      return output ?? { format, mimeType: mime[format], bytes: utf8(`native-${format}-double`),
        diagnostics: [] };
    }]));
  const api = withFlowExports(session, renderers, "serif");
  return { raw, session, api, calls, reads, assets, renderers,
    hold(value) { hold = value; }, output(value) { output = value; } };
}
const image = (id, destination = "plot.svg", isResolved = true) => ({
  kind: "image", requestId: String(id), destination, isResolved,
});

test("SVG dispatches original source and exact resolved assets without Canvas or HTML fallback", async () => {
  const f = fixture(), backing = new Uint8Array([99, 1, 2, 3, 99]);
  f.raw.items = [...Array.from({ length: 256 }, () => ({ kind: "text" })), image(1), image(2)];
  f.assets.set("1", backing.subarray(1, 4));
  f.assets.set("2", new Uint8Array([1, 2, 3]));
  const result = await f.api.exportDocument("svg", { maxWidthPt: 360 }, f.api.token);
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].format, "svg");
  assert.equal(f.calls[0].source, f.raw.source);
  assert.deepEqual(f.calls[0].options, {
    maxWidthPt: 360, font: "serif", pdfImages: [{ destination: "plot.svg", bytes: new Uint8Array([1, 2, 3]) }],
  });
  assert.deepEqual(f.reads.map(read => read.offset), [0, 256]);
  assert(f.reads.every(read => read.glyphs === false));
  assert.equal(result.assetCount, 1);
  assert.equal(result.assetBytes, 3);
  assert.equal(result.sourceLengthBytes, utf8(f.raw.source).length);
  assert.equal(result.mimeType, "image/svg+xml");
  assert.equal(result.revision, "9007199254740993");
  assert.equal(result.layoutRevision, "9007199254740997");
  assert.deepEqual(structuredClone(result), result);
  assert(Object.isFrozen(result));
  assert.notEqual(f.calls[0].options.pdfImages[0].bytes.buffer, backing.buffer);
});

test("SVG options are bounded, captured and stable through structured-clone normalization", () => {
  const token = { revision: 18446744073709551615n, layoutRevision: 0n };
  assert.equal(normalizeFlowExport("svg", undefined, token)[1].maxWidthPt, 612);
  for (const width of [144, 360.5, 14400]) {
    const options = { maxWidthPt: width, maxOutputBytes: 1024 };
    const normalized = normalizeFlowExport("svg", options, token);
    options.maxWidthPt = 900;
    assert.equal(normalized[1].maxWidthPt, width);
    assert.equal(normalized[2].revision, "18446744073709551615");
    assert(Object.isFrozen(normalized[1]));
    assert.deepEqual(normalizeFlowExport(...structuredClone(normalized)), normalized);
  }
});

test("SVG rejects coercions, foreign format settings and injected assets before native reads", async () => {
  const f = fixture();
  for (const options of [
    ...[0, 143.9, 14400.1, NaN, Infinity, null, "612"].map(maxWidthPt => ({ maxWidthPt })),
    { title: "ignored" }, { toc: true }, { darkMode: "auto" }, { lang: "en" },
    { pageNumbers: true }, { page: {} }, { allowRawHtml: true },
    { pdfImages: [] }, { fontAssets: [] }, { font: "sans" },
    { maxOutputBytes: 0 }, { maxOutputBytes: 64 * 1024 * 1024 + 1 },
  ]) await assert.rejects(f.api.exportDocument("svg", options, f.api.token), rejects("INVALID_OPTIONS"));
  assert.equal(f.calls.length, 0);
  assert.equal(f.reads.length, 0);
});

test("missing SVG renderer refuses explicitly without reading assets or invoking another format", async () => {
  const f = fixture();
  const api = withFlowExports(f.session, { html: f.renderers.html, pdf: f.renderers.pdf });
  await assert.rejects(api.exportDocument("svg", {}, api.token), rejects("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(f.calls.length, 0);
  assert.equal(f.reads.length, 0);
  await api.exportDocument("pdf", {}, api.token);
  assert.equal(f.calls[0].format, "pdf");
});

test("SVG refuses unresolved, dimensions-only and conflicting destination payloads", async () => {
  for (const mode of ["pending", "dimensions", "empty", "ambiguous"]) {
    const f = fixture();
    f.raw.items = [image(1, "plot.svg", mode !== "pending")];
    if (mode === "empty") f.assets.set("1", new Uint8Array());
    if (mode === "ambiguous") {
      f.raw.items.push(image(2));
      f.assets.set("1", new Uint8Array([1]));
      f.assets.set("2", new Uint8Array([2]));
    }
    await assert.rejects(f.api.exportDocument("svg", {}, f.api.token),
      rejects(mode === "ambiguous" ? "AMBIGUOUS_EXPORT_ASSET" : "UNRESOLVED_EXPORT_ASSET"));
    assert.equal(f.calls.length, 0);
  }
});

test("SVG owns settings and bytes during await, and shares the physical export exclusion", async () => {
  const f = fixture(), hold = deferred(), payload = new Uint8Array([1, 2]);
  f.hold(hold); f.raw.items = [image(1)]; f.assets.set("1", payload);
  const options = { maxWidthPt: 360, maxOutputBytes: 100 };
  const pending = f.api.exportDocument("svg", options, f.api.token);
  options.maxWidthPt = 900; options.maxOutputBytes = 1; payload.fill(9);
  await assert.rejects(f.api.exportDocument("pdf", {}, f.api.token), rejects("EXPORT_BUSY"));
  assert.equal(f.calls[0].options.maxWidthPt, 360);
  assert.deepEqual([...f.calls[0].options.pdfImages[0].bytes], [1, 2]);
  hold.resolve();
  await pending;
  await f.api.exportDocument("html", {}, f.api.token);
  assert.equal(f.calls.length, 2);
});

for (const change of ["source", "layout", "dispose"]) {
  test(`SVG refuses delayed publication after ${change} changes`, async () => {
    const f = fixture(), hold = deferred(); f.hold(hold);
    const pending = f.api.exportDocument("svg", {}, f.api.token);
    if (change === "source") { f.raw.source = "new"; f.raw.revision = "9007199254740994"; }
    else if (change === "layout") f.raw.layoutRevision = "9007199254740998";
    else f.api.dispose();
    hold.resolve();
    await assert.rejects(pending, rejects(change === "source" ? "STALE_REVISION"
      : change === "layout" ? "STALE_LAYOUT" : "SESSION_DISPOSED"));
  });
}

test("SVG errors release the export slot and preserve unsupported-native diagnostics", async () => {
  const f = fixture(), hold = deferred(); f.hold(hold);
  const pending = f.api.exportDocument("svg", {}, f.api.token);
  hold.reject(Object.assign(new Error("matching native package required"), { code: "UNSUPPORTED_WASM_PACKAGE" }));
  await assert.rejects(pending, rejects("UNSUPPORTED_WASM_PACKAGE"));
  f.hold(null);
  await f.api.exportDocument("svg", {}, f.api.token);
  assert.equal(f.calls.length, 2);
  assert.equal(f.api.disposed, false);
});

test("SVG preserves immutable structured renderer diagnostics and independent output bytes", async () => {
  const f = fixture();
  const diagnostics = [{ severity: "warning", start: 0, end: 0, message: "missing glyph",
    code: "svg_missing_glyphs", scope: "document" },
  { severity: "warning", start: 2, end: 4, message: "parser note" }];
  const output = { format: "svg", mimeType: mime.svg, bytes: utf8("<svg/>"), diagnostics };
  f.output(output);
  const result = await f.api.exportDocument("svg", {}, f.api.token);
  assert.deepEqual(result.diagnostics, diagnostics);
  assert(Object.isFrozen(result.diagnostics[0]));
  assert.notEqual(result.bytes.buffer, output.bytes.buffer);
  output.bytes.fill(0); diagnostics[0].code = "changed";
  assert.equal(new TextDecoder().decode(result.bytes), "<svg/>");
  assert.equal(result.diagnostics[0].code, "svg_missing_glyphs");
  assert.deepEqual(validateFlowExportResult(structuredClone(result), f.api.token), result);
});

test("diagnostic metadata is checked on renderer output and worker-style acknowledgments", async () => {
  const f = fixture();
  const output = { format: "svg", mimeType: mime.svg, bytes: utf8("<svg/>"), diagnostics: [] };
  f.output(output);
  const valid = await f.api.exportDocument("svg", {}, f.api.token);
  for (const extra of [{ code: 1 }, { code: "" }, { code: "x".repeat(129) },
    { scope: "source" }, { scope: "document", start: 1 }]) {
    const diagnostics = [{ severity: "warning", start: 0, end: 0, message: "finding", ...extra }];
    output.diagnostics = diagnostics;
    await assert.rejects(f.api.exportDocument("svg", {}, f.api.token), rejects("INVALID_WASM_RESPONSE"));
    assert.throws(() => validateFlowExportResult({ ...valid, diagnostics }, f.api.token), rejects("INVALID_WASM_RESPONSE"));
  }
  output.diagnostics = Array.from({ length: 513 }, () => ({ severity: "warning", start: 0, end: 0,
    message: "", code: "x".repeat(128), scope: "document" }));
  await assert.rejects(f.api.exportDocument("svg", {}, f.api.token), rejects("BUDGET_EXCEEDED"));
});

test("SVG checks output shape and output byte budget without publishing partial data", async () => {
  const f = fixture(), good = { format: "svg", mimeType: mime.svg, bytes: utf8("<svg/>"), diagnostics: [] };
  for (const output of [{ ...good, format: "html" }, { ...good, mimeType: mime.pdf },
    { ...good, bytes: new Uint8Array() }, { ...good, bytes: new Uint8Array(new SharedArrayBuffer(1)) }]) {
    f.output(output);
    await assert.rejects(f.api.exportDocument("svg", {}, f.api.token), rejects("INVALID_WASM_RESPONSE"));
  }
  f.output(good);
  await assert.rejects(f.api.exportDocument("svg", { maxOutputBytes: 5 }, f.api.token), rejects("BUDGET_EXCEEDED"));
  assert.equal((await f.api.exportDocument("svg", { maxOutputBytes: 6 }, f.api.token)).bytes.length, 6);
});

test("HTML, PDF and EPUB keep their option dispatch and now retain renderer reason codes too", async () => {
  const f = fixture();
  for (const format of ["html", "pdf", "epub"]) {
    f.output({ format, mimeType: mime[format], bytes: utf8("native-double"), diagnostics: [
      { severity: "warning", start: 0, end: 0, code: "image_missing", scope: "document", message: "missing" },
    ] });
    const options = format === "pdf" ? { title: "Title", pageNumbers: true }
      : format === "epub" ? { title: "Title", customCss: "" } : { title: "Title", toc: true };
    const result = await f.api.exportDocument(format, options, f.api.token);
    assert.equal(result.diagnostics[0].code, "image_missing");
    const call = f.calls.at(-1);
    assert.equal(call.format, format);
    assert.equal(call.options.allowRawHtml, false);
    assert.equal(call.options.title, "Title");
    if (format === "pdf") assert.equal(call.options.metadataEpochSeconds, 0);
    else assert.equal(call.options.darkMode, "auto");
    if (format === "epub") assert.equal(call.options.customCss, "");
  }
});
