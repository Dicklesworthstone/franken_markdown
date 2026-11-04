import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { sourceChunkSize, sourceChunk, uploadSource, uploadSourceBytes } from "./support/ios_source_upload.mjs";

const bridge = await readFile(new URL("../../ios/Renderer/bridge.html", import.meta.url), "utf8");
const script = bridge.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
  .replace("import { createRenderer } from './franken_markdown.js';", "const {createRenderer} = __engine;");

function output(text = "rendered") {
  return {bytes: new TextEncoder().encode(text), text: () => text,
    diagnostics: [], extension: "html", mimeType: "text/html"};
}

function deferred() {
  let resolve;
  const promise = new Promise(yes => { resolve = yes; });
  return {promise, resolve};
}

async function harness(renderer) {
  const messages = [], decodedSizes = [], received = [], installs = [];
  const preview = {set srcdoc(text) { installs.push(text); }};
  class BoundedDecoder extends TextDecoder {
    decode(bytes, options) {
      if (bytes) {
        assert.ok(bytes.byteLength <= sourceChunkSize, "decoder input must stay bounded");
        decodedSizes.push(bytes.byteLength);
      }
      return super.decode(bytes, options);
    }
  }
  const window = {webkit: {messageHandlers: {frankenBridge: {
    postMessage(message) { messages.push(message); },
  }}}};
  const actual = renderer ?? {renderHtml: async (markdown, options) => {
    received.push({markdown, options}); return output();
  }};
  const encodedSizes = [];
  await vm.runInNewContext("(async () => {" + script + "\n})()", {
    window, document: {querySelector: () => preview}, performance: {now: () => 1},
    TextEncoder, TextDecoder: BoundedDecoder,
    fetch: async () => ({arrayBuffer: async () => new ArrayBuffer(0)}),
    btoa,
    atob(encoded) {
      encodedSizes.push(encoded.length);
      assert.ok(encoded.length <= Math.ceil(sourceChunkSize / 3) * 4);
      return atob(encoded);
    },
    __engine: {createRenderer: async () => actual},
  });
  assert.equal(messages[0]?.type, "ready", messages[0]?.message);
  return {window, messages, decodedSizes, encodedSizes, received, installs};
}

function begin(h, sourceID, byteLength) {
  return JSON.parse(h.window.frankenBeginSource({sourceID, byteLength, chunkSize: sourceChunkSize}));
}

function complete(h, sourceID, byteLength, chunkCount) {
  return JSON.parse(h.window.frankenFinishSource({sourceID, byteLength, chunkCount}));
}

function exportBytes(h, json) {
  const manifest = JSON.parse(json);
  const chunks = [];
  for (let index = 0; index < Math.ceil(manifest.byteLength / manifest.chunkSize); index++) {
    const chunk = JSON.parse(h.window.frankenReadExportChunk({requestID: manifest.requestID, index}));
    chunks.push(Buffer.from(chunk.base64, "base64"));
  }
  assert.equal(h.window.frankenFinishExport({requestID: manifest.requestID,
    byteLength: manifest.byteLength, chunkCount: chunks.length}), true);
  return Buffer.concat(chunks);
}

for (const size of [0, 1, 65_535, 65_536, 65_537, 2 * 1024 * 1024 + 11]) {
  test("source upload preserves all " + size + " bytes using bounded chunks", async () => {
    const markdown = size ? "a".repeat(size - 1) + "Z" : "";
    const h = await harness();
    const receipt = uploadSource(h.window, markdown, 7);
    assert.equal(receipt.byteLength, size);
    assert.equal(h.decodedSizes.length, Math.ceil(size / sourceChunkSize));
    assert.equal(await h.window.frankenRender({requestID: 1, sourceID: 7,
      get markdown() { throw new Error("handle must not read an inline source"); }}), true);
    assert.equal(h.received[0].markdown, markdown);
    assert.deepEqual(h.messages.map(message => message.type), ["ready"]);
    assert.equal(h.window.frankenCommitPreview(1), true);
  });
}

test("Unicode, BOMs, NUL and CRLF survive every split within multibyte scalars", async () => {
  const h = await harness();
  let sourceID = 0;
  const samples = [];
  for (const scalar of ["é", "漢", "🧪", "\ufeff"]) {
    for (let split = 1; split < Buffer.byteLength(scalar); split++) {
      samples.push("\ufeff" + "a".repeat(sourceChunkSize - 3 - split) + scalar + "e\u0301\0\r\nEND\ufeff");
    }
  }
  for (const markdown of samples) {
    uploadSource(h.window, markdown, ++sourceID);
    assert.equal(await h.window.frankenRender({requestID: sourceID, sourceID}), true);
    assert.equal(h.received.at(-1).markdown, markdown);
  }
});

test("a source is unavailable until every chunk and the final receipt are acknowledged", async () => {
  const h = await harness();
  const bytes = Buffer.alloc(sourceChunkSize + 3, 97);
  begin(h, 1, bytes.length);
  h.window.frankenAppendSourceChunk(sourceChunk(1, 0, bytes.subarray(0, sourceChunkSize)));
  assert.equal(await h.window.frankenRender({requestID: 1, sourceID: 1}), false);
  assert.equal(h.received.length, 0);
  assert.throws(() => complete(h, 1, bytes.length, 1), /incomplete/);
  h.window.frankenAppendSourceChunk(sourceChunk(1, 1, bytes.subarray(sourceChunkSize)));
  assert.throws(() => complete(h, 1, bytes.length - 1, 2), /incomplete/);
  assert.throws(() => complete(h, 1, bytes.length, 3), /incomplete/);
  complete(h, 1, bytes.length, 2);
  assert.equal(await h.window.frankenRender({requestID: 2, sourceID: 1}), true);
  assert.equal(h.received[0].markdown, bytes.toString());
});

test("source ownership, chunk order and integrity are checked before decoder state advances", async () => {
  const h = await harness();
  const bytes = Buffer.alloc(sourceChunkSize + 1, 97);
  begin(h, 7, bytes.length);
  const first = sourceChunk(7, 0, bytes.subarray(0, sourceChunkSize));
  for (const invalid of [
    {...first, sourceID: 6}, {...first, sourceID: 8}, {...first, index: -1},
    {...first, index: 1}, {...first, index: 0.5}, {...first, index: "0"},
    {...first, byteLength: sourceChunkSize - 1}, {...first, base64: first.base64 + "A"},
    {...first, checksum: "bad"}, {...first, checksum: "00000000"},
    {...first, base64: "!".repeat(first.base64.length)},
  ]) {
    assert.throws(() => h.window.frankenAppendSourceChunk(invalid));
    assert.equal(h.decodedSizes.length, 0);
  }
  assert.equal(h.window.frankenCancelSource(8), false);
  h.window.frankenAppendSourceChunk(first);
  assert.throws(() => h.window.frankenAppendSourceChunk(first), /order/);
  h.window.frankenAppendSourceChunk(sourceChunk(7, 1, bytes.subarray(sourceChunkSize)));
  complete(h, 7, bytes.length, 2);
  assert.equal(await h.window.frankenRender({requestID: 1, sourceID: 7}), true);
  assert.equal(h.received[0].markdown, bytes.toString());
});

test("invalid manifests preserve the resident source and allocate no decoder input", async () => {
  const h = await harness();
  uploadSource(h.window, "retained", 1);
  const manifest = {sourceID: 2, byteLength: 0, chunkSize: sourceChunkSize};
  const previousDecodes = h.decodedSizes.length;
  for (const invalid of [
    {...manifest, sourceID: 0}, {...manifest, sourceID: -1}, {...manifest, sourceID: "2"},
    {...manifest, sourceID: 1.5}, {...manifest, sourceID: Number.MAX_SAFE_INTEGER + 1},
    {...manifest, byteLength: -1}, {...manifest, byteLength: 0.5},
    {...manifest, byteLength: 64 * 1024 * 1024 + 1}, {...manifest, chunkSize: 1},
  ]) assert.throws(() => h.window.frankenBeginSource(invalid));
  assert.equal(h.decodedSizes.length, previousDecodes);
  assert.equal(await h.window.frankenRender({requestID: 1, sourceID: 1}), true);
  assert.equal(h.received[0].markdown, "retained");
});

test("malformed or unfinished UTF-8 cannot publish a source and a valid upload can retry", async () => {
  const h = await harness();
  let sourceID = 0;
  for (const bytes of [
    [0x80], [0xc0, 0xaf], [0xed, 0xa0, 0x80], [0xf4, 0x90, 0x80, 0x80],
    [0xf0, 0x9f], [0x61, 0xe2, 0x82],
  ]) {
    sourceID++;
    assert.throws(() => uploadSourceBytes(h.window, Buffer.from(bytes), sourceID));
    assert.equal(h.window.frankenCancelSource(sourceID), false);
    assert.equal(await h.window.frankenRender({requestID: sourceID, sourceID}), false);
  }
  assert.equal(h.received.length, 0);
  uploadSource(h.window, "valid 🧪", ++sourceID);
  assert.equal(await h.window.frankenRender({requestID: sourceID, sourceID}), true);
  assert.equal(h.received[0].markdown, "valid 🧪");
});

test("cancellation and late old chunks cannot contaminate a replacement upload", async () => {
  const h = await harness();
  const old = Buffer.alloc(sourceChunkSize + 1, 120);
  begin(h, 1, old.length);
  assert.throws(() => begin(h, 2, 3), /still in progress/);
  h.window.frankenAppendSourceChunk(sourceChunk(1, 0, old.subarray(0, sourceChunkSize)));
  assert.equal(h.window.frankenCancelSource(1), true);
  assert.throws(() => begin(h, 1, old.length), /increase/);
  begin(h, 2, 3);
  assert.throws(() => h.window.frankenAppendSourceChunk(sourceChunk(1, 1, old.subarray(sourceChunkSize))), /no longer/);
  assert.throws(() => complete(h, 1, old.length, 2), /no longer/);
  assert.equal(h.window.frankenCancelSource(1), false);
  h.window.frankenAppendSourceChunk(sourceChunk(2, 0, Buffer.from("NEW")));
  complete(h, 2, 3, 1);
  assert.equal(await h.window.frankenRender({requestID: 1, sourceID: 2}), true);
  assert.equal(h.received[0].markdown, "NEW");
});

test("replaced and explicitly released handles fail instead of falling back to another document", async () => {
  const h = await harness({renderHtml: async () => { throw new Error("stale handle reached renderer"); },
    renderPdf: async () => { throw new Error("stale handle reached renderer"); }});
  uploadSource(h.window, "old", 1);
  begin(h, 2, 0);
  assert.equal(await h.window.frankenRender({requestID: 1, sourceID: 1, markdown: "fallback"}), false);
  await assert.rejects(h.window.frankenExportPdf({requestID: 1, sourceID: 1, markdown: "fallback"}), /handle/);
  complete(h, 2, 0, 0);
  assert.equal(h.window.frankenCancelSource(1), false);
  assert.equal(h.window.frankenCancelSource(2), true);
  await assert.rejects(h.window.frankenExportHtml({requestID: 2, sourceID: 2}), /handle/);
});

test("settings-only previews, ordinary exports and analysis reuse the uploaded snapshot", async () => {
  const seen = [];
  const renderer = {
    renderHtml: async (text, options) => { seen.push({text, options}); return output(); },
    renderPdf: async text => { seen.push({text}); return output(); },
    renderSvg: async text => { seen.push({text}); return output(); },
    renderEpub: async text => { seen.push({text}); return output(); },
    renderInteractiveHtml: async text => { seen.push({text}); return output(); },
    documentStats: async text => { seen.push({text}); return {bytes: Buffer.byteLength(text)}; },
    accessibilityAudit: async text => { seen.push({text}); return {verdict: "ok"}; },
    searchIndex: async text => { seen.push({text}); return {entries: [{text}]}; },
  };
  const h = await harness(renderer);
  const markdown = "Source 🧪\r\n".repeat(20_000);
  uploadSource(h.window, markdown, 7);
  const decodes = h.decodedSizes.length;
  await h.window.frankenRender({requestID: 1, sourceID: 7, options: {font: "sans"}});
  h.window.frankenCommitPreview(1);
  await h.window.frankenRender({requestID: 2, sourceID: 7, options: {font: "serif"}});
  h.window.frankenCommitPreview(2);
  exportBytes(h, await h.window.frankenExportHtml({requestID: 3, sourceID: 7}));
  exportBytes(h, await h.window.frankenExportPdf({requestID: 4, sourceID: 7}));
  let requestID = 4;
  for (const format of ["svg", "epub", "interactive-html", "search-index"]) {
    exportBytes(h, await h.window.frankenExportArtifact({requestID: ++requestID, sourceID: 7, format}));
  }
  const analysis = JSON.parse(await h.window.frankenAnalyze({sourceID: 7}));
  assert.equal(analysis.stats.bytes, Buffer.byteLength(markdown));
  assert.equal(h.decodedSizes.length, decodes, "reusing source must not resend or decode it");
  assert.equal(seen.length, 11);
  assert.ok(seen.every(entry => entry.text === markdown));
  assert.equal(seen[0].options.font, "sans");
  assert.equal(seen[1].options.font, "serif");
});

test("an export keeps its captured source while later typing replaces the cache", async () => {
  const pending = deferred();
  const seen = [];
  const h = await harness({renderPdf: async text => {
    seen.push(text);
    return text === "OLD 🧪" ? pending.promise : output(text);
  }});
  uploadSource(h.window, "OLD 🧪", 1);
  const exporting = h.window.frankenExportPdf({requestID: 1, sourceID: 1});
  uploadSource(h.window, "NEW 漢字", 2);
  pending.resolve(output("OLD 🧪"));
  assert.equal(exportBytes(h, await exporting).toString(), "OLD 🧪");
  assert.equal(exportBytes(h, await h.window.frankenExportPdf({requestID: 2, sourceID: 2})).toString(), "NEW 漢字");
  assert.deepEqual(seen, ["OLD 🧪", "NEW 漢字"]);
});

test("obsolete preview completion cannot publish after the uploaded source changes", async () => {
  const old = deferred();
  const h = await harness({renderHtml: async text => text === "old" ? old.promise : output("NEW")});
  uploadSource(h.window, "old", 1);
  const rendering = h.window.frankenRender({requestID: 1, sourceID: 1});
  h.window.frankenDiscardPreview(1);
  uploadSource(h.window, "current", 2);
  assert.equal(await h.window.frankenRender({requestID: 2, sourceID: 2}), true);
  assert.equal(h.window.frankenCommitPreview(2), true);
  old.resolve(output("OLD"));
  assert.equal(await rendering, false);
  assert.equal(h.window.frankenCommitPreview(1), false);
  assert.deepEqual(h.installs, ["NEW"]);
  assert.equal(h.window.frankenCancelSource(1), false);
});

test("a recreated WebKit page rejects previous-process handles until a new upload completes", async () => {
  const first = await harness();
  uploadSource(first.window, "source", 10);
  const replacement = await harness();
  assert.equal(await replacement.window.frankenRender({requestID: 2, sourceID: 10}), false);
  assert.equal(replacement.received.length, 0);
  uploadSource(replacement.window, "source", 11);
  assert.equal(await replacement.window.frankenRender({requestID: 3, sourceID: 11}), true);
  assert.equal(replacement.received[0].markdown, "source");
});
