// Exercise the production public facade against explicit generated-binding
// doubles. These tests prove async ownership, bounds and compatibility; native
// tests/wasm_font_collections.rs and font_collections_smoke.mjs prove extraction.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const collection = new Uint8Array(await readFile(new URL("../fmd-font/fonts/test-collection/FmdMixedFacesV1.ttc", import.meta.url)));
const standalone = new Uint8Array(await readFile(new URL("../fmd-font/fonts/test-collection/FmdCollectionTrueType.ttf", import.meta.url)));

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

async function fixture({ missing = [], init, invoke } = {}) {
  const calls = [];
  let initializations = 0;
  const bindings = {
    default() { initializations++; return init?.(initializations) ?? Promise.resolve(); },
    accessibilityAudit() {}, capabilities() { return "{}"; }, documentStats() {},
    searchIndex() {}, renderSemanticDiffHtml() {}, semanticDiff() {},
    renderEpubConfigured() {}, renderPdfConfiguredMulti() {}, renderSvgConfigured() {},
    renderHtmlConfiguredAdvanced() {
      return { bytes: new TextEncoder().encode("<p>legacy</p>"), format: "html", extension: "html",
        mimeType: "text/html", sourceLength: 6, diagnosticsJson: () => "[]", free() {} };
    },
    fontCollectionFaceCount(bytes) {
      calls.push({ name: "count", bytes });
      return invoke ? invoke("count", bytes) : 2;
    },
    extractFontCollectionFace(bytes, faceIndex) {
      calls.push({ name: "extract", bytes, faceIndex });
      return invoke ? invoke("extract", bytes, faceIndex) : standalone.slice();
    },
  };
  for (const key of missing) delete bindings[key];
  const native = new vm.SyntheticModule(Object.keys(bindings), function() {
    for (const [name, value] of Object.entries(bindings)) this.setExport(name, value);
  });
  const helpers = new Map();
  for (const name of ["pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"])
    helpers.set(`./${name}`, new vm.SourceTextModule(await readFile(new URL(name, import.meta.url), "utf8")));
  const module = new vm.SourceTextModule(await readFile(new URL("franken_markdown.js", import.meta.url), "utf8"));
  await module.link(name => name === "./pkg/franken_markdown.js" ? native : helpers.get(name));
  await module.evaluate();
  return { api: module.namespace, calls, get initializations() { return initializations; } };
}

for (const method of ["fontCollectionFaceCount", "extractFontCollectionFace"]) {
  for (const warm of [false, true]) test(`${method} captures exactly the view before ${warm ? "warm" : "cold"} initialization`, async () => {
    const gate = deferred();
    const f = await fixture({ init: () => warm ? Promise.resolve() : gate.promise });
    if (warm) await f.api.init();
    const backing = new Uint8Array(collection.length + 8).fill(0xee);
    backing.set(collection, 4);
    const input = new DataView(backing.buffer, 4, collection.length);
    const pending = f.api[method](input, 1);
    assert.equal(f.calls.length, 0);
    backing.fill(0);
    structuredClone(backing.buffer, { transfer: [backing.buffer] });
    gate.resolve();
    const result = await pending;
    assert.equal(f.initializations, 1);
    assert.deepEqual(f.calls[0].bytes, collection);
    if (method === "extractFontCollectionFace") {
      assert.equal(f.calls[0].faceIndex, 1);
      assert.deepEqual(result, standalone);
    } else assert.equal(result, 2);
  });
}

test("buffer, typed array, DataView and cross-realm inputs preserve byte ranges without numeric conversion", async () => {
  const f = await fixture();
  const backing = new Uint8Array(collection.length + 8).fill(0xee);
  backing.set(collection, 4);
  const typed = new Uint16Array(backing.buffer, 4, collection.length / 2);
  Object.defineProperty(typed, "byteLength", { get() { assert.fail("shadowed size was read"); } });
  Object.defineProperty(typed, "buffer", { get() { assert.fail("shadowed buffer was read"); } });
  const foreign = vm.runInNewContext("new Uint8Array([9, 1, 2, 3, 8]).subarray(1, 4)");
  for (const input of [collection.slice().buffer, backing.subarray(4, 4 + collection.length),
    typed, new DataView(backing.buffer, 4, collection.length), Buffer.from(collection)]) {
    await f.api.fontCollectionFaceCount(input);
    assert.deepEqual(f.calls.at(-1).bytes, collection);
  }
  await f.api.fontCollectionFaceCount(foreign);
  assert.deepEqual([...f.calls.at(-1).bytes], [1, 2, 3]);
  assert.deepEqual(backing.subarray(4, 4 + collection.length), collection);
});

test("invalid indices are rejected without coercion, byte access or WASM initialization", async () => {
  const f = await fixture();
  const bytes = new Proxy({}, { get() { assert.fail("bytes read before index admission"); } });
  for (const index of [-1, 0.5, 1024, 2 ** 32, NaN, Infinity, -Infinity, "0", null, true,
    new Number(0), { valueOf() { assert.fail("index was coerced"); } }]) {
    await assert.rejects(f.api.extractFontCollectionFace(bytes, index), error =>
      error instanceof f.api.FontCollectionError && error.code === "FONT_COLLECTION_FACE_INDEX");
  }
  assert.equal(f.initializations, 0);
  assert.equal(f.calls.length, 0);
  await f.api.extractFontCollectionFace(collection);
  assert.equal(f.calls.at(-1).faceIndex, 0);
  await f.api.extractFontCollectionFace(collection, 1023);
  assert.equal(f.calls.at(-1).faceIndex, 1023);
});

test("unusable, shared, detached and oversized buffers fail before initialization", async () => {
  const f = await fixture();
  const detached = new Uint8Array(12), detachedView = new DataView(detached.buffer);
  structuredClone(detached.buffer, { transfer: [detached.buffer] });
  const bad = [undefined, null, "ttcf", [1, 2], {}, new ArrayBuffer(0), detached, detachedView,
    { [Symbol.toStringTag]: "ArrayBuffer", byteLength: 12 }];
  if (typeof SharedArrayBuffer !== "undefined") {
    const shared = new SharedArrayBuffer(12);
    bad.push(shared, new Uint8Array(shared), new DataView(shared));
  }
  for (const input of bad) for (const method of ["fontCollectionFaceCount", "extractFontCollectionFace"])
    await assert.rejects(f.api[method](input), { code: "INVALID_FONT_BYTES" });
  const large = new Uint8Array(32 * 1024 * 1024 + 1);
  for (const method of ["fontCollectionFaceCount", "extractFontCollectionFace"])
    await assert.rejects(f.api[method](large), { code: "FONT_COLLECTION_LIMIT" });
  assert.equal(f.initializations, 0);
  assert.equal(f.calls.length, 0);
  large.set(collection, 16);
  await f.api.fontCollectionFaceCount(large.subarray(16, 16 + collection.length));
  assert.deepEqual(f.calls[0].bytes, collection);
});

test("extracted output never aliases input snapshots or a generated binding's reusable memory", async () => {
  const returned = standalone.slice();
  const f = await fixture({ invoke: () => returned });
  const source = collection.slice();
  const first = await f.api.extractFontCollectionFace(source);
  first.fill(0);
  assert.deepEqual(returned, standalone);
  const second = await f.api.extractFontCollectionFace(source);
  assert.deepEqual(second, standalone);
  assert.deepEqual(source, collection);
  assert.notEqual(second.buffer, returned.buffer);
  assert.notEqual(f.calls[0].bytes.buffer, source.buffer);
});

test("native collection failures preserve stable codes and bounded diagnostics", async () => {
  for (const code of ["NOT_COLLECTION", "UNSUPPORTED_VERSION", "UNSUPPORTED_FACE_FORMAT",
    "FACE_INDEX", "MALFORMED", "DUPLICATE_TABLE", "MISSING_TABLE", "LIMIT"]) {
    const f = await fixture({ invoke() { throw `FONT_COLLECTION_${code}: face Some(1), offset Some(24)`; } });
    for (const method of ["fontCollectionFaceCount", "extractFontCollectionFace"])
      await assert.rejects(f.api[method](collection, 1), error =>
        error instanceof f.api.FontCollectionError && error.code === `FONT_COLLECTION_${code}`
        && error.message.includes("face Some(1)"));
  }
  const f = await fixture({ invoke() { throw "x".repeat(10000); } });
  await assert.rejects(f.api.fontCollectionFaceCount(collection), error =>
    error.code === "FONT_COLLECTION_FAILED" && error.message.length === 2048);
});

test("old generated packages remain loadable but cannot silently choose a collection face", async () => {
  for (const missing of [["fontCollectionFaceCount"], ["extractFontCollectionFace"],
    ["fontCollectionFaceCount", "extractFontCollectionFace"]]) {
    const f = await fixture({ missing });
    for (const method of missing) await assert.rejects(f.api[method](collection), error =>
      error instanceof f.api.FontCollectionError && error.code === "UNSUPPORTED_WASM_PACKAGE"
      && error.message.includes("rebuilt from matching source"));
    assert.equal(f.initializations, 0);
    assert.equal((await f.api.renderHtml("legacy")).text(), "<p>legacy</p>");
  }
});

test("concurrent extraction requests share initialization but retain independent source snapshots", async () => {
  const gate = deferred(), f = await fixture({ init: () => gate.promise });
  const source = Uint8Array.of(1, 2, 3, 4);
  const first = f.api.extractFontCollectionFace(source, 0);
  source[0] = 9;
  const second = f.api.extractFontCollectionFace(source, 1);
  source.fill(0);
  gate.resolve();
  await Promise.all([first, second]);
  assert.equal(f.initializations, 1);
  assert.deepEqual(f.calls.map(call => [...call.bytes]), [[1, 2, 3, 4], [9, 2, 3, 4]]);
  assert.deepEqual(f.calls.map(call => call.faceIndex), [0, 1]);
});

test("initialization failures remain retryable and renderer objects expose both helpers", async () => {
  const failure = new Error("initialization failed");
  const f = await fixture({ init: count => count === 1 ? Promise.reject(failure) : Promise.resolve() });
  await assert.rejects(f.api.fontCollectionFaceCount(collection), error => error === failure);
  assert.equal(f.calls.length, 0);
  const renderer = await f.api.createRenderer();
  assert.equal(await renderer.fontCollectionFaceCount(collection), 2);
  assert.deepEqual(await renderer.extractFontCollectionFace(collection), standalone);
  assert.equal(f.initializations, 2);
});
