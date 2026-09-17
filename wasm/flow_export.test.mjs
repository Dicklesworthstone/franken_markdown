// Real export adapter and admission checks; native session/renderers below are
// explicit doubles. Generated-WASM parity is a separate flow_export_smoke gate.
import test from "node:test";
import assert from "node:assert/strict";
import { withFlowExports, normalizeFlowExport, validateFlowExportResult, FLOW_EXPORT_LIMIT } from "./flow_export.mjs";
const utf8 = value => new TextEncoder().encode(value);
const gate = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const code = expected => error => error.code === expected;
const image = (id, destination = `image-${id}.png`, resolved = true) => ({ kind: "image", requestId: String(id), destination, isResolved: resolved });
export class ExportSession {
  disposed = false; source = "# Original 😀\n\n**Bold** and [link](https://example.test)";
  revision = "9007199254740993"; layoutRevision = "9007199254740997";
  layoutOptions = { viewportWidth: 640, bodySize: 14, codeSize: 13, lineHeight: 20 };
  items = []; assets = new Map(); reads = [];
  get token() { return { revision: this.revision, layoutRevision: this.layoutRevision }; }
  snapshot(options) {
    this.reads.push(options);
    const { offset, limit } = options, end = Math.min(offset + limit, this.items.length);
    return { schemaVersion: 1, ...this.token, offset, total: this.items.length,
      nextOffset: end < this.items.length ? end : null, items: this.items.slice(offset, end) };
  }
  assetBytes(id, revision) { assert.equal(revision, this.revision); return this.assets.get(id) ?? null; }
  dispose() { this.disposed = true; }
}
function fixture() {
  const session = new ExportSession(), calls = [];
  let result = null, wait = null;
  const render = format => async (source, options) => {
    calls.push({ format, source, options });
    if (wait) await wait.promise;
    return result ?? { format, mimeType: format === "html" ? "text/html; charset=utf-8" : "application/pdf",
      bytes: utf8(format === "html" ? "<!doctype html><p>Native double</p>" : "%PDF-native-double"), diagnostics: [] };
  };
  const api = withFlowExports(session, { html: render("html"), pdf: render("pdf") }, "serif");
  return { session, calls, api, setResult(value) { result = value; }, setGate(value) { wait = value; } };
}

test("exports original Markdown through the chosen core with data-only provenance", async () => {
  const f = fixture(), token = f.session.token;
  const result = await f.api.exportDocument("pdf", { title: "Document", toc: true, pageNumbers: true }, token);
  assert.equal(f.calls.length, 1); assert.equal(f.calls[0].source, f.session.source);
  assert.equal(f.calls[0].options.font, "serif"); assert.equal(f.calls[0].options.allowRawHtml, false);
  assert.equal(f.calls[0].options.metadataEpochSeconds, 0);
  assert.equal(f.calls[0].options.pageNumbers, true); assert.deepEqual(f.calls[0].options.pdfImages, []);
  assert.equal(result.sourceLengthBytes, utf8(f.session.source).length);
  assert.equal(result.revision, token.revision); assert.equal(result.layoutRevision, token.layoutRevision);
  assert.equal(result.assetCount, 0); assert.equal(result.font, "serif");
  assert.equal(result.text, undefined); assert.deepEqual(structuredClone(result), result);
  assert.equal(f.session.revision, token.revision); assert(Object.isFrozen(result));
});

test("rejects unknown/coerced settings, unsafe HTML/font/asset injection, and invalid identities before work", async () => {
  const f = fixture();
  for (const [format, options] of [["svg", {}], ["toString", {}], ["pdf", { author: 2 }], ["pdf", { title: "\ud800" }],
    ["pdf", { toc: 1 }], ["pdf", { title: "a".repeat(4097) }], ["html", { author: "ignored" }],
    ["pdf", { allowRawHtml: true }], ["html", { customCss: "@import url(https://example.test)" }],
    ["pdf", { pdfImages: [] }], ["html", { fontAssets: [] }], ["pdf", { font: "sans" }],
    ["pdf", { metadataEpochSeconds: NaN }], ["pdf", { maxOutputBytes: FLOW_EXPORT_LIMIT + 1 }],
    ["pdf", { baseFontSize: 100 }], ["pdf", { fitToPages: 0 }], ["html", { lang: "en\nX" }]]) {
    await assert.rejects(f.api.exportDocument(format, options, f.session.token));
  }
  for (const revision of [1, "01", "18446744073709551616"]) {
    await assert.rejects(f.api.exportDocument("pdf", {}, { revision, layoutRevision: "1" }), code("INVALID_IDENTITY"));
  }
  assert.equal(f.calls.length, 0); assert.equal(f.session.reads.length, 0);
  const normalized = normalizeFlowExport("pdf", {}, { revision: 18446744073709551615n, layoutRevision: 0n });
  assert.equal(normalized[2].revision, "18446744073709551615");
});

test("collects late-page images, deduplicates equal destinations and copies exact owned payloads", async () => {
  const f = fixture();
  f.session.items = [...Array.from({ length: 257 }, () => ({ kind: "text" })), image(1, "same.png"), image(2, "same.png"), image(3)];
  const backing = new Uint8Array([99, 1, 2, 99]);
  f.session.assets.set("1", backing.subarray(1, 3)); f.session.assets.set("2", new Uint8Array([1, 2]));
  f.session.assets.set("3", new Uint8Array([3]));
  const result = await f.api.exportDocument("html", {}, f.session.token);
  assert.deepEqual(f.session.reads.map(p => p.offset), [0, 256]);
  assert(f.session.reads.every(p => p.glyphs === false && p.token.revision === f.session.revision));
  assert.deepEqual(f.calls[0].options.pdfImages.map(v => [v.destination, [...v.bytes]]), [["same.png", [1, 2]], ["image-3.png", [3]]]);
  assert.equal(result.assetCount, 2); assert.equal(result.assetBytes, 3);
  assert.notEqual(f.calls[0].options.pdfImages[0].bytes.buffer, backing.buffer);
});

test("fails rather than dropping unresolved, dimension-only, empty, or ambiguous images", async () => {
  for (const mode of ["pending", "dimensions", "empty", "ambiguous"]) {
    const f = fixture(); f.session.items = [image(1, "x.png", mode !== "pending")];
    if (mode === "empty") f.session.assets.set("1", new Uint8Array());
    if (mode === "ambiguous") {
      f.session.items.push(image(2, "x.png")); f.session.assets.set("1", new Uint8Array([1])); f.session.assets.set("2", new Uint8Array([2]));
    }
    await assert.rejects(f.api.exportDocument("html", {}, f.session.token), code(mode === "ambiguous" ? "AMBIGUOUS_EXPORT_ASSET" : "UNRESOLVED_EXPORT_ASSET"));
    assert.equal(f.calls.length, 0);
  }
});

test("fences before work and before publishing a delayed export after source/layout changes or disposal", async () => {
  for (const mutation of ["source", "layout", "dispose"]) {
    const f = fixture(), hold = gate(); f.setGate(hold);
    const promise = f.api.exportDocument("pdf", {}, f.session.token);
    assert.equal(f.calls.length, 1);
    if (mutation === "source") { f.session.source = "new"; f.session.revision = "9007199254740994"; }
    else if (mutation === "layout") f.session.layoutRevision = "9007199254740998";
    else f.session.dispose();
    hold.resolve();
    await assert.rejects(promise, code(mutation === "source" ? "STALE_REVISION" : mutation === "layout" ? "STALE_LAYOUT" : "SESSION_DISPOSED"));
  }
  const f = fixture();
  await assert.rejects(f.api.exportDocument("pdf", {}, { revision: "1", layoutRevision: "1" }), code("STALE_REVISION"));
  assert.equal(f.calls.length, 0); assert.equal(f.session.reads.length, 0);
});

test("owns source, options and image bytes across renderer awaits; rejects concurrent physical exports", async () => {
  const f = fixture(), hold = gate(); f.setGate(hold);
  const source = f.session.source, options = { title: "before", maxOutputBytes: 128 };
  f.session.items = [image(1)]; const payload = new Uint8Array([1, 2]); f.session.assets.set("1", payload);
  const promise = f.api.exportDocument("pdf", options, f.session.token);
  options.title = "after"; options.maxOutputBytes = 1; payload.fill(9);
  await assert.rejects(f.api.exportDocument("html", {}, f.session.token), code("EXPORT_BUSY"));
  assert.equal(f.calls[0].source, source); assert.equal(f.calls[0].options.title, "before");
  assert.deepEqual([...f.calls[0].options.pdfImages[0].bytes], [1, 2]);
  hold.resolve(); await promise;
  await f.api.exportDocument("pdf", {}, f.session.token); assert.equal(f.calls.length, 2);
});

test("recoverable renderer failure releases the export slot without changing source", async () => {
  const f = fixture(), hold = gate(); f.setGate(hold);
  const promise = f.api.exportDocument("pdf", {}, f.session.token); hold.reject(new Error("render rejected"));
  await assert.rejects(promise, code("EXPORT_FAILED"));
  f.setGate(null); const result = await f.api.exportDocument("html", {}, f.session.token);
  assert.equal(result.revision, f.session.revision); assert.equal(f.session.disposed, false);
});

test("WebAssembly traps remain fatal-class errors rather than ordinary export failures", async () => {
  const f = fixture(), hold = gate(); f.setGate(hold);
  const promise = f.api.exportDocument("pdf", {}, f.session.token); hold.reject(new WebAssembly.RuntimeError("unreachable"));
  await assert.rejects(promise, code("WASM_ERROR"));
});

test("rejects stalled, inconsistent and oversized display inventories without rendering", async () => {
  for (const change of [p => ({ ...p, nextOffset: 0 }), p => ({ ...p, revision: "1" }),
    p => ({ ...p, items: [] }), p => ({ ...p, total: 500001, nextOffset: 256 })]) {
    const f = fixture(); f.session.items = Array.from({ length: 300 }, () => ({ kind: "text" }));
    const original = f.session.snapshot.bind(f.session); f.session.snapshot = options => change(original(options));
    await assert.rejects(f.api.exportDocument("pdf", {}, f.session.token)); assert.equal(f.calls.length, 0);
  }
  const f = fixture(); f.session.items = Array.from({ length: 1025 }, (_, i) => image(i, "same.png"));
  f.session.assetBytes = () => new Uint8Array([1]);
  await assert.rejects(f.api.exportDocument("pdf", {}, f.session.token), code("BUDGET_EXCEEDED"));
  assert.equal(f.calls.length, 0);
});

test("rejects shared/detached image memory and both per-image and aggregate payload overflow", async () => {
  const shared = new Uint8Array(new SharedArrayBuffer(1));
  const detached = new Uint8Array(1); structuredClone(detached, { transfer: [detached.buffer] });
  for (const payload of [shared, detached, new Uint8Array(8 * 1024 * 1024 + 1)]) {
    const f = fixture(); f.session.items = [image(1)]; f.session.assets.set("1", payload);
    await assert.rejects(f.api.exportDocument("pdf", {}, f.session.token)); assert.equal(f.calls.length, 0);
  }
  const f = fixture(), payload = new Uint8Array(8 * 1024 * 1024);
  f.session.items = Array.from({ length: 5 }, (_, i) => image(i)); f.session.assetBytes = () => payload;
  await assert.rejects(f.api.exportDocument("pdf", {}, f.session.token), code("BUDGET_EXCEEDED"));
  assert.equal(f.calls.length, 0);
});

test("checks output format, size, diagnostics and copies render-owned result bytes", async () => {
  const f = fixture();
  const good = { format: "pdf", mimeType: "application/pdf", bytes: utf8("%PDF-double"),
    diagnostics: [{ severity: "warning", start: 0, end: 2, message: "Example warning" }] };
  for (const bad of [{ ...good, format: "html" }, { ...good, mimeType: "text/html" }, { ...good, bytes: new Uint8Array() },
    { ...good, diagnostics: [{ severity: "warning", start: 0, end: 10000, message: "invalid" }] },
    { ...good, diagnostics: Array(1025).fill(good.diagnostics[0]) }]) {
    f.setResult(bad); await assert.rejects(f.api.exportDocument("pdf", {}, f.session.token));
  }
  f.setResult(good);
  await assert.rejects(f.api.exportDocument("pdf", { maxOutputBytes: 2 }, f.session.token), code("BUDGET_EXCEEDED"));
  const output = await f.api.exportDocument("pdf", {}, f.session.token);
  assert.notEqual(output.bytes.buffer, good.bytes.buffer); good.bytes.fill(0);
  assert.equal(new TextDecoder().decode(output.bytes), "%PDF-double");
  assert.deepEqual(output.diagnostics, good.diagnostics); assert.notEqual(output.diagnostics, good.diagnostics);
  assert.throws(() => validateFlowExportResult({ ...output, layoutRevision: "1" }, f.session.token), code("INVALID_WASM_RESPONSE"));
});

test("preserves live getter descriptors while wrapping a frozen production-style facade", async () => {
  let revision = "1", disposed = false;
  const base = Object.freeze({ get revision() { return revision; }, get disposed() { return disposed; }, dispose() { disposed = true; } });
  const api = withFlowExports(base, { html() {}, pdf() {} });
  revision = "2"; assert.equal(api.revision, "2"); api.dispose(); assert.equal(api.disposed, true);
  await assert.rejects(api.exportDocument("pdf", {}, { revision: "2", layoutRevision: "1" }), code("SESSION_DISPOSED"));
});
