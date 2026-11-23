import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import vm from "node:vm";
import { sourceCRC32, sourceChunkSize } from "./ios_source_upload.mjs";

export const resourceChunkSize = sourceChunkSize;
export const bridgeURL = new URL("../../../ios/Renderer/bridge.html", import.meta.url);

export function resourceManifest(resourceSetID, entries) {
  return { resourceSetID, byteLength: entries.reduce((n, entry) => n + entry.bytes.byteLength, 0),
    chunkSize: resourceChunkSize,
    entries: entries.map(entry => ({ kind: entry.kind, key: entry.key, byteLength: entry.bytes.byteLength })) };
}
export function resourceChunk(resourceSetID, assetIndex, index, bytes) {
  return { resourceSetID, assetIndex, index, byteLength: bytes.byteLength,
    base64: Buffer.from(bytes).toString("base64"), checksum: sourceCRC32(bytes) };
}

// Whole fixture Buffers are test inputs. Production Swift encodes one bounded
// acknowledged chunk at a time and never puts a whole attachment in base64.
export function uploadResources(window, entries, resourceSetID = 1) {
  const manifest = resourceManifest(resourceSetID, entries), entryCount = entries.length;
  assert.deepEqual(JSON.parse(window.frankenBeginResources(manifest)), {
    resourceSetID, byteLength: manifest.byteLength, chunkSize: resourceChunkSize, entryCount,
  });
  let chunkCount = 0, receivedBytes = 0;
  for (const [assetIndex, entry] of entries.entries()) {
    for (let offset = 0; offset < entry.bytes.byteLength; offset += resourceChunkSize) {
      const part = entry.bytes.subarray(offset, offset + resourceChunkSize);
      receivedBytes += part.byteLength;
      assert.deepEqual(JSON.parse(window.frankenAppendResourceChunk(
        resourceChunk(resourceSetID, assetIndex, chunkCount, part),
      )), { resourceSetID, assetIndex, index: chunkCount, receivedBytes });
      chunkCount++;
    }
  }
  const receipt = { resourceSetID, byteLength: manifest.byteLength, chunkCount, entryCount };
  assert.deepEqual(JSON.parse(window.frankenFinishResources(receipt)), receipt);
  return receipt;
}

export function readResourceExport(window, json) {
  const manifest = JSON.parse(json), chunks = [];
  assert.equal(manifest.chunkSize, resourceChunkSize);
  for (let index = 0; index < Math.ceil(manifest.byteLength / manifest.chunkSize); index++) {
    const chunk = JSON.parse(window.frankenReadExportChunk({ requestID: manifest.requestID, index }));
    const bytes = Buffer.from(chunk.base64, "base64");
    assert.equal(chunk.requestID, manifest.requestID); assert.equal(chunk.index, index);
    assert.equal(bytes.byteLength, chunk.byteLength); assert(bytes.byteLength <= resourceChunkSize);
    assert.equal(sourceCRC32(bytes), chunk.checksum);
    chunks.push(bytes);
  }
  assert.equal(window.frankenFinishExport({ requestID: manifest.requestID,
    byteLength: manifest.byteLength, chunkCount: chunks.length }), true);
  const bytes = Buffer.concat(chunks);
  assert.equal(bytes.byteLength, manifest.byteLength);
  return { ...manifest, bytes };
}

export function resourceOutput(text = "rendered") {
  const bytes = new TextEncoder().encode(text);
  return { bytes, diagnostics: [], text: () => text, mimeType: "text/html", extension: "html" };
}

// Execute the exact production module. WebKit's endpoints are explicit doubles;
// callers may supply either the actual bundled engine or a renderer receipt.
export async function resourceHarness({ renderer, engine, wasm = new Uint8Array(), bridgeSource } = {}) {
  const html = bridgeSource ?? await readFile(bridgeURL, "utf8");
  const module = html.match(/<script type="module">([\s\S]*?)<\/script>/)?.[1];
  assert(module?.includes("import { createRenderer } from './franken_markdown.js';"));
  const script = module.replace("import { createRenderer } from './franken_markdown.js';", "const {createRenderer} = __engine;");
  const messages = [], calls = [], allocations = [], decodedSizes = [];
  const preview = { srcdoc: "" };
  const window = { webkit: { messageHandlers: { frankenBridge: {
    postMessage(message) { messages.push(message); },
  } } } };
  const supplied = renderer ?? Object.fromEntries(["renderHtml", "renderPdf", "renderSvg", "renderEpub",
    "renderInteractiveHtml", "renderBookPdf", "renderBookSite"].map(method => [method, async (source, options) => {
    calls.push({ method, source, options }); return resourceOutput();
  }]));
  const InstrumentedBytes = new Proxy(Uint8Array, { construct(target, args) {
    if (typeof args[0] === "number") allocations.push(args[0]);
    return Reflect.construct(target, args);
  } });
  await vm.runInNewContext(`(async () => {${script}\n})()`, {
    window, document: { querySelector: () => preview }, performance, TextEncoder, TextDecoder,
    ArrayBuffer, Uint8Array: InstrumentedBytes, Uint32Array, DataView,
    atob(value) {
      decodedSizes.push(value.length);
      assert(value.length <= Math.ceil(resourceChunkSize / 3) * 4, "bounded base64 input");
      return atob(value);
    }, btoa, unescape,
    __engine: engine ?? { createRenderer: async () => supplied },
    fetch: async path => {
      assert.equal(path, "./pkg/franken_markdown_bg.wasm", "no ambient resource fetches");
      return { arrayBuffer: async () => wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength) };
    },
  });
  assert.equal(messages[0]?.type, "ready", messages[0]?.message);
  return { window, preview, messages, calls, allocations, decodedSizes };
}
