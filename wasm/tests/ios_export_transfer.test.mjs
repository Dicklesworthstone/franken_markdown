import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const bridge = await readFile(new URL("../../ios/Renderer/bridge.html", import.meta.url), "utf8");
const script = bridge.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
  .replace("import { createRenderer } from './franken_markdown.js';", "const {createRenderer} = __engine;");
const chunkSize = 65_536;

function artifact(bytes, extension = "pdf", mimeType = "application/pdf") {
  return {bytes, extension, mimeType, diagnostics: [{code: "retained-diagnostic"}],
    text() { throw new Error("Export must not decode the complete output as text"); }};
}

async function harness(renderer) {
  const messages = [];
  const encodedSizes = [];
  const window = {webkit: {messageHandlers: {frankenBridge: {
    postMessage(message) { messages.push(message); },
  }}}};
  await vm.runInNewContext(`(async () => {${script}\n})()`, {
    window, document: {querySelector: () => ({srcdoc: ""})},
    performance: {now: () => 1}, TextEncoder,
    fetch: async () => ({arrayBuffer: async () => new ArrayBuffer(0)}),
    btoa(value) {
      encodedSizes.push(value.length);
      assert.ok(value.length <= chunkSize, "only one bounded raw chunk may be encoded");
      return Buffer.from(value, "binary").toString("base64");
    },
    __engine: {createRenderer: async () => renderer},
  });
  return {window, messages, encodedSizes};
}

// Bit-at-a-time reference, independent of the production lookup table. The
// canonical check value below also fixes the polynomial, initialization and XOR.
function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value ^= byte;
    for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
  }
  return ((value ^ 0xffffffff) >>> 0).toString(16).padStart(8, "0");
}

function read(h, requestID, index) {
  const chunk = JSON.parse(h.window.frankenReadExportChunk({requestID, index}));
  const bytes = Buffer.from(chunk.base64, "base64");
  assert.equal(chunk.requestID, requestID);
  assert.equal(chunk.index, index);
  assert.equal(chunk.byteLength, bytes.length);
  assert.ok(bytes.length <= chunkSize);
  assert.equal(chunk.checksum, crc32(bytes));
  return bytes;
}

function finish(h, manifest) {
  return h.window.frankenFinishExport({requestID: manifest.requestID,
    chunkCount: Math.ceil(manifest.byteLength / manifest.chunkSize), byteLength: manifest.byteLength});
}

for (const size of [0, 1, 32_768, 65_535, 65_536, 65_537, 2 * 1024 * 1024 + 7]) {
  test(`pull export preserves all ${size} bytes with bounded conversion and no eager messages`, async () => {
    // Nonzero byteOffset catches accidental transfer of the backing allocation.
    const allocation = new Uint8Array(size + 29);
    allocation.fill(252);
    const bytes = allocation.subarray(13, 13 + size);
    for (let index = 0; index < size; index++) bytes[index] = (index * 53 + 17) % 256;
    const h = await harness({renderPdf: async () => artifact(bytes)});
    const manifest = JSON.parse(await h.window.frankenExportPdf({requestID: 7, markdown: "# Complete"}));
    assert.deepEqual(manifest, {requestID: 7, byteLength: size, extension: "pdf",
      mimeType: "application/pdf", diagnosticCount: 1, chunkSize});
    assert.deepEqual(h.encodedSizes, [], "preparing output must not encode or enqueue its chunks");
    const chunks = [];
    for (let index = 0; index < Math.ceil(size / chunkSize); index++) {
      const previousCalls = h.encodedSizes.length;
      chunks.push(read(h, 7, index));
      assert.equal(h.encodedSizes.length, previousCalls + 1, "one pull produces exactly one chunk");
    }
    assert.deepEqual(Buffer.concat(chunks), Buffer.from(bytes));
    assert.equal(finish(h, manifest), true);
    assert.throws(() => h.window.frankenReadExportChunk({requestID: 7, index: 0}), /no longer available/);
    assert.equal(h.window.frankenCancelExport(7), false, "finished output was released");
    assert.deepEqual(h.messages.map(message => message.type), ["ready"], "export data never enters a push queue");
  });
}

test("CRC32 matches the independent standard check value", async () => {
  const h = await harness({renderPdf: async () => artifact(new TextEncoder().encode("123456789"))});
  const manifest = JSON.parse(await h.window.frankenExportPdf({requestID: 1, markdown: ""}));
  const chunk = JSON.parse(h.window.frankenReadExportChunk({requestID: 1, index: 0}));
  assert.equal(chunk.checksum, "cbf43926");
  assert.equal(finish(h, manifest), true);
});

test("CRC32 covers every short tail and unaligned view at transport boundaries", async () => {
  let bytes;
  const h = await harness({renderPdf: async () => artifact(bytes)});
  const sizes = [...Array.from({length: 33}, (_, index) => index),
    63, 64, 65, 32_767, 32_768, 32_769, 65_535, 65_536, 65_537];
  let requestID = 0;
  for (const size of sizes) {
    for (let offset = 0; offset < 8; offset++) {
      const allocation = new Uint8Array(size + offset + 8).fill(252);
      bytes = allocation.subarray(offset, offset + size);
      let seed = size ^ (offset << 16) ^ 0x7f9c9e11;
      for (let index = 0; index < size; index++) {
        seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
        bytes[index] = seed & 255;
      }
      const manifest = JSON.parse(await h.window.frankenExportPdf({requestID: ++requestID, markdown: ""}));
      for (let index = 0; index < Math.ceil(size / chunkSize); index++) {
        // read() checks CRC against the independent scalar oracle above.
        assert.deepEqual(read(h, requestID, index),
          Buffer.from(bytes.subarray(index * chunkSize, (index + 1) * chunkSize)),
          `size ${size}, byte offset ${offset}, chunk ${index}`);
      }
      assert.equal(finish(h, manifest), true);
    }
  }
});

test("HTML uses original UTF-8 bytes and round-trips a code point split across chunks", async () => {
  const html = "a".repeat(chunkSize - 1) + "🙂café漢字";
  const bytes = new TextEncoder().encode(html);
  const h = await harness({renderHtml: async () => artifact(bytes, "html", "text/html")});
  const manifest = JSON.parse(await h.window.frankenExportHtml({requestID: 1, markdown: html}));
  const first = read(h, 1, 0);
  assert.equal(first[first.length - 1], 0xf0);
  const second = read(h, 1, 1);
  assert.equal(Buffer.concat([first, second]).toString("utf8"), html);
  assert.equal(finish(h, manifest), true);
});

test("chunk order, ownership and final byte totals are enforced before release", async () => {
  const bytes = new Uint8Array(chunkSize + 1);
  const h = await harness({renderPdf: async () => artifact(bytes)});
  const manifest = JSON.parse(await h.window.frankenExportPdf({requestID: 3, markdown: ""}));
  for (const index of [-1, 0.5, 1, Number.MAX_SAFE_INTEGER, "0"]) {
    assert.throws(() => h.window.frankenReadExportChunk({requestID: 3, index}), /sequence/);
  }
  assert.throws(() => h.window.frankenReadExportChunk({requestID: 4, index: 0}), /no longer available/);
  assert.equal(h.window.frankenCancelExport(2), false);
  assert.throws(() => finish(h, manifest), /incomplete/);
  read(h, 3, 0);
  assert.throws(() => h.window.frankenReadExportChunk({requestID: 3, index: 0}), /sequence/);
  assert.throws(() => finish(h, manifest), /incomplete/);
  read(h, 3, 1);
  assert.throws(() => h.window.frankenReadExportChunk({requestID: 3, index: 2}), /no remaining chunks/);
  assert.throws(() => h.window.frankenFinishExport({requestID: 3, chunkCount: 2, byteLength: 1}), /incomplete/);
  assert.equal(finish(h, manifest), true);
});

test("only one export can retain bytes and identifiers cannot be replayed", async () => {
  let renderCalls = 0;
  const h = await harness({renderPdf: async () => { renderCalls++; return artifact(new Uint8Array([1])); }});
  for (const requestID of [0, -1, 1.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, "1", null]) {
    await assert.rejects(h.window.frankenExportPdf({requestID, markdown: ""}), /identifiers/);
  }
  const first = JSON.parse(await h.window.frankenExportPdf({requestID: 1, markdown: ""}));
  await assert.rejects(h.window.frankenExportPdf({requestID: 2, markdown: ""}), /still in progress/);
  assert.equal(renderCalls, 1);
  read(h, 1, 0); finish(h, first);
  await assert.rejects(h.window.frankenExportPdf({requestID: 1, markdown: ""}), /identifiers/);
  const second = JSON.parse(await h.window.frankenExportPdf({requestID: 2, markdown: ""}));
  assert.equal(second.requestID, 2);
  assert.equal(h.window.frankenCancelExport(2), true);
  assert.throws(() => h.window.frankenReadExportChunk({requestID: 2, index: 0}), /no longer available/);
  assert.equal(renderCalls, 2);
});

test("cancellation discards a late render without clearing its replacement", async () => {
  let resolveOld;
  const old = new Promise(resolve => { resolveOld = resolve; });
  const h = await harness({renderPdf: source => source === "old" ? old : Promise.resolve(artifact(new Uint8Array([8, 9])))});
  const oldStart = h.window.frankenExportPdf({requestID: 1, markdown: "old"});
  assert.equal(h.window.frankenCancelExport(1), true);
  const current = JSON.parse(await h.window.frankenExportPdf({requestID: 2, markdown: "current"}));
  resolveOld(artifact(new Uint8Array([7])));
  await assert.rejects(oldStart, /cancelled/);
  assert.equal(h.window.frankenCancelExport(1), false);
  assert.deepEqual(read(h, 2, 0), Buffer.from([8, 9]));
  assert.equal(finish(h, current), true);
});

test("render failures release ownership and allow the next export", async () => {
  const h = await harness({renderPdf: async source => {
    if (source === "bad") throw new Error("render rejected");
    return artifact(new Uint8Array([1]));
  }});
  await assert.rejects(h.window.frankenExportPdf({requestID: 1, markdown: "bad"}), /render rejected/);
  await assert.rejects(h.window.frankenExportArtifact({requestID: 2, format: "unknown"}), /Unsupported artifact/);
  const manifest = JSON.parse(await h.window.frankenExportPdf({requestID: 3, markdown: "good"}));
  read(h, 3, 0); assert.equal(finish(h, manifest), true);
});

test("search-index export keeps non-ASCII JSON intact without URI-encoding the document", async () => {
  const search = {entries: [{text: "🙂漢字 café\n".repeat(20_000), anchor: "#last"}]};
  const h = await harness({searchIndex: async () => search});
  const manifest = JSON.parse(await h.window.frankenExportArtifact({requestID: 1, format: "search-index", markdown: ""}));
  assert.equal(manifest.extension, "json");
  assert.equal(manifest.mimeType, "application/json");
  const chunks = [];
  for (let index = 0; index < Math.ceil(manifest.byteLength / chunkSize); index++) chunks.push(read(h, 1, index));
  assert.deepEqual(JSON.parse(Buffer.concat(chunks).toString()), search);
  assert.equal(finish(h, manifest), true);
});


test("over-budget output is rejected before encoding and releases the transfer slot", async () => {
  // Override the length report to exercise the post-render budget guard without
  // requiring this regression test to allocate half a gigabyte.
  class OversizedBytes extends Uint8Array {
    get byteLength() { return 512 * 1024 * 1024 + 1; }
  }
  const h = await harness({renderPdf: async source => artifact(source === "huge"
    ? new OversizedBytes(1) : new Uint8Array([9]))});
  await assert.rejects(h.window.frankenExportPdf({requestID: 1, markdown: "huge"}), /512 MiB/);
  assert.deepEqual(h.encodedSizes, []);
  const manifest = JSON.parse(await h.window.frankenExportPdf({requestID: 2, markdown: "small"}));
  read(h, 2, 0); assert.equal(finish(h, manifest), true);
});
