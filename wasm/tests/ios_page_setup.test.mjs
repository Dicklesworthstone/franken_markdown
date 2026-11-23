import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import test from "node:test";
import vm from "node:vm";
import { Worker, isMainThread, parentPort } from "node:worker_threads";
import { inflateSync } from "node:zlib";
import { uploadSource } from "./support/ios_source_upload.mjs";

// Exercise the tracked Apple wrapper, bridge and WASM, not the current source
// package or a renderer double. Inspect the resulting PDF bytes independently.
// This proves the native export contract, not SwiftUI or device qualification.
const packageRoot = new URL("../../ios/Renderer/", import.meta.url);
const caseNames = ["default geometry", "paper presets and orientation", "independent margins",
  "collection PDF geometry", "invalid geometry and transfer recovery"];

async function probeInWorker() {
  const worker = new Worker(new URL(import.meta.url));
  let deadline;
  try {
    return await new Promise((resolve, reject) => {
      deadline = setTimeout(() => reject(new Error("Native page setup probe exceeded 40 seconds")), 40_000);
      worker.once("message", resolve);
      worker.once("error", reject);
      worker.once("exit", code => reject(new Error(`Page setup worker exited without a result (${code})`)));
    });
  } finally {
    clearTimeout(deadline);
    await worker.terminate();
  }
}

// These small PDF readers handle this renderer's direct page/content objects.
// Read stream lengths as bytes; use Node's independent zlib implementation.
function inspectPdf(bytes) {
  assert.equal(bytes.subarray(0, 5).toString(), "%PDF-");
  const source = bytes.toString("latin1");
  const objects = new Map([...source.matchAll(/(?:^|\n)(\d+) 0 obj\b([\s\S]*?)endobj/g)]
    .map(match => [Number(match[1]), match]));
  const pages = [];
  for (const [, match] of objects) {
    if (!/\/Type\s*\/Page\b/.test(match[2])) continue;
    const box = match[2].match(/\/MediaBox\s*\[\s*0\s+0\s+([\d.]+)\s+([\d.]+)\s*\]/);
    assert.ok(box, "page owns a MediaBox");
    const contentID = Number(match[2].match(/\/Contents\s+(\d+)\s+0\s+R/)?.[1]);
    const content = objects.get(contentID);
    assert.ok(content, "page owns a content stream");
    const marker = content[0].match(/stream\r?\n/);
    assert.ok(marker);
    const length = Number(content[2].match(/\/Length\s+(\d+)/)?.[1]);
    assert.ok(Number.isSafeInteger(length));
    const start = content.index + marker.index + marker[0].length;
    const encoded = bytes.subarray(start, start + length);
    const commands = /\/FlateDecode\b/.test(content[2]) ? inflateSync(encoded).toString("latin1") : encoded.toString("latin1");
    const positions = [...commands.matchAll(/1 0 0 1\s+(-?[\d.]+)\s+(-?[\d.]+)\s+Tm\b/g)]
      .map(position => ({x: Number(position[1]), y: Number(position[2])}));
    assert.ok(positions.length, "page contains positioned text");
    pages.push({width: Number(box[1]), height: Number(box[2]), positions});
  }
  assert.ok(pages.length > 0);
  return pages;
}

function approximately(actual, expected, label) {
  assert.ok(Math.abs(actual - expected) <= 0.011, `${label}: ${actual} != ${expected}`);
}

if (isMainThread) {
  let sharedProbe;
  for (const name of caseNames) {
    test(`bundled iOS PDF page setup: ${name}`, {timeout: 45_000}, async t => {
      sharedProbe ??= probeInWorker();
      const report = await sharedProbe;
      const result = report.results.find(entry => entry.name === name);
      assert.ok(result);
      assert.equal(result.ok, true, result.error);
      t.diagnostic(`Actual Apple-bundled package ${report.version}; PDF geometry and content-stream checks on Node, not a device test.`);
    });
  }
} else {
  const engine = await import(new URL("franken_markdown.js", packageRoot));
  const wasm = await readFile(new URL("pkg/franken_markdown_bg.wasm", packageRoot));
  const packageInfo = JSON.parse(await readFile(new URL("package.json", packageRoot), "utf8"));
  const bridge = await readFile(new URL("bridge.html", packageRoot), "utf8");
  const script = bridge.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
    .replace("import { createRenderer } from './franken_markdown.js';", "const {createRenderer} = __engine;");
  const messages = [];
  const window = {webkit: {messageHandlers: {frankenBridge: {postMessage: message => messages.push(message)}}}};
  await vm.runInNewContext(`(async () => {${script}\n})()`, {
    window, document: {querySelector: () => ({srcdoc: ""})}, performance, TextEncoder, TextDecoder, atob,
    btoa: value => Buffer.from(value, "binary").toString("base64"), unescape,
    __engine: engine,
    fetch: async path => {
      assert.equal(path, "./pkg/franken_markdown_bg.wasm");
      return {arrayBuffer: async () => wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength)};
    },
  });
  assert.equal(messages[0]?.type, "ready", messages[0]?.message);
  let nextRequest = 1, nextSource = 1;
  const options = {font: "sans", fontScale: 1, darkMode: "disabled", pageNumbers: false};
  const readExport = (raw, requestID) => {
    const result = JSON.parse(raw);
    assert.equal(result.requestID, requestID);
    assert.equal(result.extension, "pdf");
    assert.equal(result.chunkSize, 65_536);
    const chunks = [];
    for (let index = 0; index < Math.ceil(result.byteLength / result.chunkSize); index++) {
      const chunk = JSON.parse(window.frankenReadExportChunk({requestID, index}));
      assert.equal(chunk.requestID, requestID);
      assert.equal(chunk.index, index);
      const bytes = Buffer.from(chunk.base64, "base64");
      assert.equal(bytes.length, chunk.byteLength);
      assert.ok(bytes.length <= result.chunkSize);
      chunks.push(bytes);
    }
    const bytes = Buffer.concat(chunks);
    assert.equal(bytes.length, result.byteLength);
    assert.equal(window.frankenFinishExport({requestID, byteLength: bytes.length, chunkCount: chunks.length}), true);
    return bytes;
  };
  const documentPdf = async (page, source = "Page setup origin.\n") => {
    const sourceID = uploadSource(window, source, nextSource++).sourceID;
    const requestID = nextRequest++;
    const settings = page === undefined ? options : {...options, page};
    return readExport(await window.frankenExportPdf({requestID, sourceID, options: settings}), requestID);
  };
  const page = (widthPt, heightPt, margins = {topPt: 72, rightPt: 72, bottomPt: 72, leftPt: 72}) =>
    ({size: {widthPt, heightPt}, margins});
  const checks = [
    ["default geometry", async () => {
      const legacy = await documentPdf(undefined);
      const explicit = await documentPdf(page(612, 792));
      assert.deepEqual(explicit, legacy, "unchanged Letter geometry preserves PDF bytes");
      const [first] = inspectPdf(legacy);
      assert.equal(first.width, 612); assert.equal(first.height, 792);
      assert.equal(first.positions[0].x, 72);
    }],
    ["paper presets and orientation", async () => {
      // Canonical dimensions produced by MarkdownPageSetup, including presets
      // whose names are intentionally unavailable in this older bundled API.
      for (const [width, height] of [[612, 792], [210 * 72 / 25.4, 297 * 72 / 25.4],
        [148 * 72 / 25.4, 210 * 72 / 25.4], [612, 1008], [792, 1224], [450.125, 620.375]]) {
        for (const [w, h] of [[width, height], [height, width]]) {
          for (const actual of inspectPdf(await documentPdf(page(w, h)))) {
            approximately(actual.width, w, "sheet width");
            approximately(actual.height, h, "sheet height");
          }
        }
      }
    }],
    ["independent margins", async () => {
      const baseline = inspectPdf(await documentPdf(page(620, 450)))[0];
      const asymmetric = page(620, 450, {topPt: 24, rightPt: 36, bottomPt: 48, leftPt: 60});
      const moved = inspectPdf(await documentPdf(asymmetric))[0];
      assert.equal(moved.positions[0].x, 60, "left margin positions text");
      approximately(moved.positions[0].y - baseline.positions[0].y, 48, "top margin moves the text origin");
      const source = Array.from({length: 40}, (_, i) => `Paragraph ${i + 1}. Page geometry must constrain the actual text layout and pagination, including every independent margin.`).join("\n\n");
      const originalPages = inspectPdf(await documentPdf(asymmetric, source)).length;
      const narrowPages = inspectPdf(await documentPdf({...asymmetric,
        margins: {...asymmetric.margins, rightPt: 330}}, source)).length;
      const shallowPages = inspectPdf(await documentPdf({...asymmetric,
        margins: {...asymmetric.margins, bottomPt: 240}}, source)).length;
      assert.ok(narrowPages > originalPages, "right margin reduces the wrapping width");
      assert.ok(shallowPages > originalPages, "bottom margin reduces the page capacity");
    }],
    ["collection PDF geometry", async () => {
      const files = [{path: "01-first.md", source: "# First chapter\n\nFirst chapter body."},
        {path: "02-last.md", source: "# Last chapter\n\nLast chapter body."}];
      const requestID = nextRequest++;
      const settings = {...options, page: page(620, 450, {topPt: 24, rightPt: 36, bottomPt: 48, leftPt: 60})};
      const bytes = readExport(await window.frankenBook({requestID, format: "pdf", files, options: settings}), requestID);
      const pages = inspectPdf(bytes);
      assert.ok(pages.length >= 2, "both chapters survive the collection export");
      for (const sheet of pages) {
        assert.equal(sheet.width, 620); assert.equal(sheet.height, 450);
        assert.ok(sheet.positions.some(position => position.x === 60), "every chapter uses the selected left margin");
      }
    }],
    ["invalid geometry and transfer recovery", async () => {
      for (const invalid of [page(143, 792), page(612, 792, {leftPt: 600}),
        page(816.4909835206345, 1166.8058596551418,
          {topPt: 72, rightPt: 347.5558721843905, bottomPt: 72, leftPt: 396.93511133624395})]) {
        await assert.rejects(documentPdf(invalid), error => error.code === "INVALID_OPTIONS");
        assert.equal(inspectPdf(await documentPdf(page(612, 792))).length, 1,
          "rejected geometry releases the active transfer for the next export");
      }
    }],
  ];
  const results = [];
  for (const [name, run] of checks) {
    try { await run(); results.push({name, ok: true}); }
    catch (error) { results.push({name, ok: false, error: String(error.stack ?? error).slice(0, 4000)}); }
  }
  parentPort.postMessage({version: packageInfo.version, results});
  parentPort.close();
}
