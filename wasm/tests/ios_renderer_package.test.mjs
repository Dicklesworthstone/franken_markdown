import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { Worker, isMainThread, parentPort } from "node:worker_threads";
import vm from "node:vm";
import test from "node:test";

// Run the actual Apple-bundled wrapper/glue/WASM through the actual bridge.
// Only WebKit's DOM/IPC endpoints are doubles: these are API/format/schema
// compatibility checks, not browser rendering or PDF conformance evidence.
const packageRoot = new URL("../../ios/Renderer/", import.meta.url);
const caseNames = ["HTML", "PDF", "SVG", "EPUB", "interactive HTML", "statistics",
  "accessibility audit", "search index", "semantic-diff HTML", "semantic-diff metrics",
  "book site", "book PDF"];

async function probeInWorker() {
  const worker = new Worker(new URL(import.meta.url));
  let deadline;
  try {
    return await new Promise((resolve, reject) => {
      // A timer in a separate thread remains enforceable during synchronous
      // WASM calls; an in-process test timeout alone cannot stop a stuck core.
      deadline = setTimeout(() => reject(new Error("Bundled iOS API probe exceeded its 40 second deadline")), 40_000);
      worker.once("message", resolve);
      worker.once("error", reject);
      worker.once("exit", code => reject(new Error(`API worker exited without a result (code ${code})`)));
    });
  } finally {
    clearTimeout(deadline);
    await worker.terminate();
  }
}

if (isMainThread) {
  let sharedProbe;
  for (const name of caseNames) {
    test(`bundled iOS bridge retains ${name} compatibility`, {timeout: 45_000}, async t => {
      sharedProbe ??= probeInWorker();
      const report = await sharedProbe;
      const result = report.results.find(entry => entry.name === name);
      assert.ok(result, `missing ${name} result`);
      assert.equal(result.ok, true, result.error);
      t.diagnostic(`Actual bundled package ${report.version}; ${result.elapsedMS.toFixed(1)} ms on this Node host; not a device benchmark.`);
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
  const preview = {srcdoc: ""};
  const window = {webkit: {messageHandlers: {frankenBridge: {
    postMessage(message) { messages.push(message); },
  }}}};
  await vm.runInNewContext(`(async () => {${script}\n})()`, {
    window, document: {querySelector: () => preview}, performance, TextEncoder,
    btoa: value => Buffer.from(value, "binary").toString("base64"), unescape,
    __engine: engine,
    fetch: async path => {
      assert.equal(path, "./pkg/franken_markdown_bg.wasm");
      return {arrayBuffer: async () => wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength)};
    },
  });
  assert.equal(messages[0]?.type, "ready", messages[0]?.message);

  const source = "# Native API compatibility\n\nHello **friend**.\n\n"
    + "#### Skipped heading level\n\n![](missing.png)\n\n"
    + "[click here](https://example.test)\n\nNATIVE_END_MARKER.\n";
  const options = {font: "sans", darkMode: "auto", allowRawHtml: false, fontScale: 1,
    lang: "en", toc: false, tocDepth: 3, title: "Native smoke", author: "Ada Lovelace"};
  const files = [{path: "01-first.md", source: "# First\n\nNative first chapter."},
    {path: "02-last.md", source: "# Last\n\nNative last chapter."}];
  let analysis, diff;
  const analyze = () => analysis ??= window.frankenAnalyze({markdown: source}).then(JSON.parse);
  const compare = () => diff ??= window.frankenDiff({oldMarkdown: "# Before\n\nOld.",
    newMarkdown: source, options: {oldName: "Before", newName: "Current"}}).then(JSON.parse);
  const findings = values => {
    assert.ok(Array.isArray(values));
    for (const finding of values) assert.equal(typeof finding.code, "string");
  };
  const decodeArtifact = (json, extension, signature) => {
    const result = JSON.parse(json);
    assert.equal(result.extension, extension);
    assert.equal(typeof result.mimeType, "string");
    assert.ok(Array.isArray(result.diagnostics));
    const bytes = Buffer.from(result.base64, "base64");
    assert.equal(bytes.length, result.byteLength);
    assert.ok(bytes.length > 0);
    if (signature) assert.deepEqual([...bytes.subarray(0, signature.length)], signature);
    return bytes;
  };
  const checks = [
    ["HTML", async () => {
      assert.equal(await window.frankenRender({requestID: 1, markdown: source, options}), true);
      assert.equal(preview.srcdoc, "");
      assert.equal(window.frankenCommitPreview(1), true);
      assert.match(preview.srcdoc, /^<!doctype html>/i);
      assert.match(preview.srcdoc, /NATIVE_END_MARKER/);
      assert.equal(await window.frankenExportHtml({requestID: 2, markdown: source, options}), true);
      const result = messages.find(value => value.type === "exportHtmlResult" && value.requestID === 2);
      assert.ok(result);
      assert.equal(Buffer.byteLength(result.htmlText), result.byteLength);
      assert.match(result.htmlText, /NATIVE_END_MARKER/);
    }],
    ["PDF", async () => {
      assert.equal(await window.frankenExportPdf({requestID: 3, markdown: source,
        options: {...options, pageNumbers: true, codeLineNumbers: true, microtype: "disabled"}}), true);
      const result = messages.find(value => value.type === "exportPdfResult" && value.requestID === 3);
      assert.ok(result);
      const bytes = Buffer.from(result.base64, "base64");
      assert.equal(bytes.length, result.byteLength);
      assert.equal(bytes.subarray(0, 5).toString(), "%PDF-");
    }],
    ["SVG", async () => {
      const bytes = decodeArtifact(await window.frankenExportArtifact({format: "svg", markdown: source, options}), "svg");
      assert.match(bytes.toString(), /<svg\b/);
      assert.match(bytes.toString(), /<(?:path|text|use)\b/);
    }],
    ["EPUB", async () => {
      decodeArtifact(await window.frankenExportArtifact({format: "epub", markdown: source, options}), "epub", [80, 75, 3, 4]);
    }],
    ["interactive HTML", async () => {
      const bytes = decodeArtifact(await window.frankenExportArtifact({format: "interactive-html", markdown: source, options}), "html");
      assert.match(bytes.toString(), /<!doctype html>/i);
    }],
    ["statistics", async () => {
      const {stats} = await analyze();
      for (const key of ["bytes", "lines", "words", "characters", "sentences", "reading_time_secs",
        "speaking_time_secs", "flesch_reading_ease", "flesch_kincaid_grade", "coleman_liau_index",
        "automated_readability_index"]) assert.equal(typeof stats[key], "number", key);
      assert.equal(typeof stats.reading_ease_label, "string");
      for (const key of ["headings_total", "paragraphs", "code_blocks", "tables", "lists", "images",
        "links_total", "math_blocks", "math_inlines"]) assert.equal(typeof stats.structure[key], "number", key);
      assert.equal(stats.bytes, Buffer.byteLength(source));
      assert.ok(stats.findings.length > 0);
      findings(stats.findings);
    }],
    ["accessibility audit", async () => {
      const {audit} = await analyze();
      assert.equal(typeof audit.verdict, "string");
      assert.ok(audit.findings.length > 0);
      findings(audit.findings);
    }],
    ["search index", async () => {
      const {search} = await analyze();
      assert.ok(search.entries.some(entry => entry.text.includes("NATIVE_END_MARKER")));
      for (const entry of search.entries) {
        for (const key of ["kind", "anchor", "text"]) assert.equal(typeof entry[key], "string", key);
        if (entry.level !== undefined) assert.equal(typeof entry.level, "number");
      }
      const bytes = decodeArtifact(await window.frankenExportArtifact({format: "search-index", markdown: source}), "json");
      assert.deepEqual(JSON.parse(bytes.toString()), search);
    }],
    ["semantic-diff HTML", async () => { assert.match((await compare()).html, /<!doctype html>/i); }],
    ["semantic-diff metrics", async () => {
      const {metrics} = await compare();
      assert.equal(metrics.old_name, "Before"); assert.equal(metrics.new_name, "Current");
      for (const key of ["unchanged_blocks", "inserted_blocks", "deleted_blocks", "modified_blocks",
        "words_inserted", "words_deleted", "similarity_ratio"]) assert.equal(typeof metrics.stats[key], "number", key);
    }],
    ["book site", async () => {
      // Native shared settings include PDF metadata and document raw-HTML trust.
      // The bridge must adapt them to the strict, safe-only site API.
      decodeArtifact(await window.frankenBook({format: "site", files,
        options: {...options, pageNumbers: false, allowRawHtml: true}}), "zip", [80, 75, 3, 4]);
    }],
    ["book PDF", async () => {
      decodeArtifact(await window.frankenBook({format: "pdf", files,
        options: {...options, pageNumbers: true}}), "pdf", [...Buffer.from("%PDF-")]);
    }],
  ];
  const results = [];
  for (const [name, run] of checks) {
    const started = performance.now();
    try { await run(); results.push({name, ok: true, elapsedMS: performance.now() - started}); }
    catch (error) { results.push({name, ok: false, elapsedMS: performance.now() - started, error: String(error).slice(0, 4000)}); }
  }
  parentPort.postMessage({version: packageInfo.version, results});
  parentPort.close();
}
