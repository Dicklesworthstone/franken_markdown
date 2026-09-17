import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, copyFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { createFlowAdapter, FlowError, identity, sourceText, validateCreation, FLOW_SOURCE_LIMIT, FLOW_ASSET_LIMIT } from "./flow_session.mjs";

// A transport double, NOT a Markdown renderer. It verifies JS/ABI argument
// contracts and error/lifecycle handling. flow_smoke.mjs exercises real WASM.
function transport() {
  const b = { revision: "1", layoutRevision: "1", source: "café", calls: [], frees: 0, count: 3 };
  const log = (method, args) => b.calls.push({ method, args });
  const tick = () => { b.layoutRevision = (BigInt(b.layoutRevision) + 1n).toString(); };
  const rev = r => { if (r !== b.revision) throw '{"code":"STALE_REVISION","message":"stale"}'; };
  const envelope = () => ({ schemaVersion: 1, revision: b.revision, layoutRevision: b.layoutRevision });
  const page = (offset, limit, key, count) => JSON.stringify({ ...envelope(), offset, total: count,
    nextOffset: offset + Math.min(limit, count - offset) < count ? offset + limit : null,
    [key]: Array.from({ length: Math.min(limit, count - offset) }, (_, i) => ({ index: offset + i })) });
  b.snapshotJson = (...args) => { log("snapshotJson", args); return page(args[2], args[3], "items", b.count); };
  b.readingJson = (...args) => { log("readingJson", args); return page(args[2], args[3], "nodes", 1); };
  b.pendingAssetsJson = (...args) => { log("pendingAssetsJson", args); return page(args[1], args[2], "requests", 1); };
  for (const name of ["editUtf16", "editBytes", "replaceSource"]) b[name] = (...args) => {
    log(name, args); rev(args[0]);
    if (b.reject) throw '{"code":"LAYOUT_ERROR","message":"unsupported glyph"}';
    b.revision = (BigInt(b.revision) + 1n).toString(); tick();
  };
  b.reflow = (...args) => { log("reflow", args); rev(args[0]); if (b.reject) throw '{"code":"LAYOUT_ERROR","message":"failed"}'; tick(); };
  b.provideAsset = (...args) => { log("provideAsset", args); rev(args[1]); tick(); };
  b.reloadAssets = r => { log("reloadAssets", [r]); rev(r); b.revision = (BigInt(b.revision) + 1n).toString(); tick(); };
  b.hitTestJson = (...args) => { log("hitTestJson", args); return JSON.stringify({ ...envelope(), hit: null, linkTarget: 'https://example.test/"quoted"' }); };
  b.selectItemJson = (...args) => { log("selectItemJson", args); return JSON.stringify({ ...envelope(), text: "café", rectangles: [] }); };
  b.copySource = (...args) => { log("copySource", args); return "**bold**"; };
  b.fontBytes = (...args) => { log("fontBytes", args); return b.payload; };
  b.assetBytes = (...args) => { log("assetBytes", args); return b.payload; };
  b.payload = new Uint8Array([1, 2, 3]);
  b.free = () => { b.frees++; };
  return b;
}
const errorCode = code => error => error instanceof FlowError && error.code === code;

test("creation validates source, options and positive f32 layout before initialization", () => {
  assert.deepEqual(validateCreation("hello").layout, { viewportWidth: 800, bodySize: 14, codeSize: 13, lineHeight: 20 });
  for (const value of [{ font: "other" }, { viewportWidth: 0 }, { viewportWidth: NaN }, { bodySize: 30 }, { typo: 1 }, null]) {
    assert.throws(() => validateCreation("hello", value), FlowError);
  }
  assert.throws(() => validateCreation({ toString() { throw Error("must not coerce"); } }), errorCode("INVALID_ARGUMENT"));
});
test("Unicode ingress rejects lone surrogates without replacing valid scalar pairs", () => {
  assert.equal(sourceText("a😀éb"), "a😀éb");
  for (const text of ["\ud800", "\udfff", "a\ud800x", "\ud800\ud800"]) assert.throws(() => sourceText(text), errorCode("INVALID_UNICODE"));
  const raw = transport(), api = createFlowAdapter(raw);
  assert.throws(() => api.edit(0, 0, "\ud800", { expectedRevision: "1" }), errorCode("INVALID_UNICODE"));
  assert.equal(raw.calls.length, 0);
});
test("source budget counts UTF-8 bytes rather than UTF-16 units", () => {
  assert.equal(sourceText("é".repeat(FLOW_SOURCE_LIMIT / 2)).length, FLOW_SOURCE_LIMIT / 2);
  assert.throws(() => sourceText("é".repeat(FLOW_SOURCE_LIMIT / 2 + 1)), errorCode("BUDGET_EXCEEDED"));
  assert.throws(() => sourceText("x".repeat(FLOW_SOURCE_LIMIT + 1)), errorCode("BUDGET_EXCEEDED"));
});
test("u64 revisions preserve all bits and reject Number or coercible strings", () => {
  for (const value of ["0", "9007199254740993", "18446744073709551615", 18446744073709551615n]) assert.equal(identity(value), String(value));
  for (const value of [1, 9007199254740992, -1n, "01", " 1", "1e3", "18446744073709551616", "", "+1"]) assert.throws(() => identity(value), errorCode("INVALID_IDENTITY"));
});
test("UTF-16 edits and byte edits use distinct ABI methods and explicit reuse", () => {
  const raw = transport(); raw.revision = "9007199254740993";
  const api = createFlowAdapter(raw);
  api.edit(1, 3, "é", { expectedRevision: 9007199254740993n });
  assert.deepEqual(raw.calls[0], { method: "editUtf16", args: ["9007199254740993", 1, 3, "é", false] });
  api.editBytes(2, 4, "ok", { expectedRevision: api.revision, reuseAssets: true });
  assert.equal(raw.calls[1].method, "editBytes"); assert.equal(raw.calls[1].args[4], true);
  assert.throws(() => api.edit(0, 0, "x", { expectedRevision: 1 }), errorCode("INVALID_IDENTITY"));
  assert.throws(() => api.edit(0, 0, "x", { expectedRevision: api.revision, reuseAssets: "yes" }), errorCode("INVALID_ARGUMENT"));
});
test("invalid offset domains never reach wasm integer coercion", () => {
  const raw = transport(), api = createFlowAdapter(raw);
  for (const [start, end] of [[-1, 2], [0.5, 2], [3, 2], [0, 4294967296], ["0", 1]]) {
    assert.throws(() => api.edit(start, end, "x", { expectedRevision: "1" }), FlowError);
  }
  assert.equal(raw.calls.length, 0);
});
test("failed reflow leaves JS layout and native revision unchanged", () => {
  const raw = transport(), api = createFlowAdapter(raw); raw.reject = true;
  const before = api.layoutOptions;
  assert.throws(() => api.reflow({ viewportWidth: 100 }, api.token), errorCode("LAYOUT_ERROR"));
  assert.deepEqual(api.layoutOptions, before); assert.equal(api.layoutRevision, "1");
  raw.reject = false;
  api.reflow({ viewportWidth: 100 }, api.token);
  assert.equal(api.layoutOptions.viewportWidth, 100); assert.equal(api.revision, "1");
});
test("pagination carries one frozen snapshot token and refuses mixed generations", () => {
  const raw = transport(), api = createFlowAdapter(raw);
  assert.deepEqual([...api.pages({ limit: 2 })].flatMap(p => p.items.map(i => i.index)), [0, 1, 2]);
  const pages = api.pages({ limit: 1 }); pages.next();
  api.replaceSource("new", { expectedRevision: api.revision });
  assert.throws(() => pages.next(), errorCode("STALE_REVISION"));
  const notStarted = api.pages();
  api.reflow({ viewportWidth: 300 }, api.token);
  assert.throws(() => notStarted.next(), errorCode("STALE_LAYOUT"));
});
test("empty documents yield one terminating page and invalid limits are rejected", () => {
  const raw = transport(); raw.count = 0; const api = createFlowAdapter(raw);
  assert.equal([...api.pages()].length, 1);
  assert.equal(api.snapshot().nextOffset, null);
  for (const limit of [0, -1, 2049, 1.5, "1"]) assert.throws(() => api.snapshot({ limit }), FlowError);
});
test("malformed, stale and non-advancing wire responses fail rather than loop", () => {
  const raw = transport(), api = createFlowAdapter(raw);
  raw.snapshotJson = () => "not json";
  assert.throws(() => api.snapshot(), errorCode("INVALID_WASM_RESPONSE"));
  raw.snapshotJson = () => JSON.stringify({ schemaVersion: 1, revision: "2", layoutRevision: "1" });
  assert.throws(() => api.snapshot(), errorCode("STALE_LAYOUT"));
  raw.snapshotJson = () => JSON.stringify({ schemaVersion: 1, revision: "1", layoutRevision: "1", offset: 0, total: 2, nextOffset: 0, items: [{}] });
  assert.throws(() => api.snapshot({ limit: 1 }), errorCode("INVALID_WASM_RESPONSE"));
});
test("hit testing and selection require the displayed source/layout token", () => {
  const raw = transport(), api = createFlowAdapter(raw), shown = api.token;
  assert.equal(api.hitTest(10, 20, shown).linkTarget, 'https://example.test/"quoted"');
  assert.equal(api.selectText(0, 0, 4, shown).text, "café");
  api.reflow({ viewportWidth: 300 }, shown);
  const count = raw.calls.length;
  assert.throws(() => api.hitTest(1, 1, shown), errorCode("STALE_LAYOUT"));
  assert.throws(() => api.selectText(0, 0, 1, shown), errorCode("STALE_LAYOUT"));
  assert.throws(() => api.hitTest(Infinity, 0, api.token), errorCode("INVALID_ARGUMENT"));
  assert.equal(raw.calls.length, count);
});
test("asset completion retains generation and rejects invalid payloads before WASM", () => {
  const raw = transport(), api = createFlowAdapter(raw);
  api.provideAsset({ requestId: "1", generation: "1", width: 200, height: 100 });
  assert.deepEqual(raw.calls[0].args, ["1", "1", 200, 100, undefined]);
  const count = raw.calls.length;
  for (const bytes of [null, [1], new Uint16Array([1]), new Uint8Array(FLOW_ASSET_LIMIT + 1)]) {
    assert.throws(() => api.provideAsset({ requestId: "1", generation: "1", width: 1, height: 1, bytes }), FlowError);
  }
  if (typeof SharedArrayBuffer === "function") {
    assert.throws(() => api.provideAsset({ requestId: "1", generation: "1", width: 1, height: 1, bytes: new Uint8Array(new SharedArrayBuffer(1)) }), FlowError);
  }
  assert.equal(raw.calls.length, count);
});
test("retrieved font and asset arrays are independently owned", () => {
  const raw = transport(), api = createFlowAdapter(raw);
  const font = api.fontBytes("18446744073709551615"); font[0] = 9;
  const asset = api.assetBytes("1", "1"); asset[1] = 9;
  assert.deepEqual([...raw.payload], [1, 2, 3]);
  raw.payload = undefined;
  assert.equal(api.assetBytes("1", "1"), null);
});
test("reading and pending asset pages use explicit schema and snapshot fences", () => {
  const raw = transport(), api = createFlowAdapter(raw);
  const shown = api.token;
  assert.equal(api.readingOrder({ token: shown }).nodes.length, 1);
  assert.equal(api.pendingAssets({ token: shown }).requests.length, 1);
  api.reloadAssets("1");
  assert.throws(() => api.pendingAssets({ token: shown }), errorCode("STALE_REVISION"));
});
test("dispose is idempotent and all future operations reject closed handles", () => {
  const raw = transport(), api = createFlowAdapter(raw);
  api.dispose(); api.dispose(); assert.equal(raw.frees, 1); assert.equal(api.disposed, true);
  for (const operation of [() => api.source, () => api.token, () => api.snapshot(), () => api.fontBytes("1"), () => api.pages()]) {
    assert.throws(operation, errorCode("SESSION_DISPOSED"));
  }
});
test("WASM errors retain stable codes and unexpected traps retain their cause", () => {
  const raw = transport(), api = createFlowAdapter(raw);
  raw.replaceSource = () => { throw '{"code":"STALE_REVISION","message":"stale source"}'; };
  assert.throws(() => api.replaceSource("x", { expectedRevision: "1" }), errorCode("STALE_REVISION"));
  const trap = new Error("trap"); raw.replaceSource = () => { throw trap; };
  assert.throws(() => api.replaceSource("x", { expectedRevision: "1" }), e => e.code === "WASM_ERROR" && e.cause === trap);
});

async function entrypointFixture(classSource) {
  const dir = await mkdtemp(join(tmpdir(), "fmd-flow-transport-"));
  await mkdir(join(dir, "pkg"));
  for (const file of ["flow.js", "flow_session.mjs"]) await copyFile(new URL(file, import.meta.url), join(dir, file));
  await writeFile(join(dir, "package.json"), '{"type":"module"}');
  await writeFile(join(dir, "franken_markdown.js"), 'export async function init() {}');
  await writeFile(join(dir, "pkg", "franken_markdown.js"), classSource);
  return import(pathToFileURL(join(dir, "flow.js")));
}
test("public entrypoint constructs the generated persistent class", async () => {
  const { createFlowSession } = await entrypointFixture('export class FmdFlowSession { constructor(source) { this.source = source; this.revision = "1"; this.layoutRevision = "1"; } free() {} }');
  const api = await createFlowSession("hello");
  assert.equal(api.source, "hello"); api.dispose();
});
test("public entrypoint rejects mismatched old artifacts with an actionable code", async () => {
  const { createFlowSession } = await entrypointFixture('export const oldVersion = true;');
  await assert.rejects(createFlowSession("hello"), e => e.name === "FlowError" && e.code === "UNSUPPORTED_WASM_PACKAGE");
});
