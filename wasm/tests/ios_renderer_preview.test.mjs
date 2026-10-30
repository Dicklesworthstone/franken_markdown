import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const bridge = await readFile(new URL("../../ios/Renderer/bridge.html", import.meta.url), "utf8");
const script = bridge.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
  .replace("import { createRenderer } from './franken_markdown.js';", "const {createRenderer} = __engine;");
const encoder = new TextEncoder();

function output(html) {
  return {bytes: encoder.encode(html), diagnostics: [], text: () => html};
}

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}

async function harness(renderer, initializationError) {
  const messages = [];
  const installs = [];
  let srcdoc = "previous preview";
  const preview = {
    get srcdoc() { return srcdoc; },
    set srcdoc(value) { srcdoc = value; installs.push(value); },
  };
  const window = {webkit: {messageHandlers: {frankenBridge: {
    postMessage(message) { messages.push(message); },
  }}}};
  const context = {
    window,
    document: {querySelector(selector) { assert.equal(selector, "#preview"); return preview; }},
    performance: {now: () => 25},
    TextEncoder,
    fetch: async () => ({arrayBuffer: async () => new ArrayBuffer(0)}),
    btoa: value => Buffer.from(value, "binary").toString("base64"),
    unescape,
    __engine: {createRenderer: async () => {
      if (initializationError) throw initializationError;
      return renderer;
    }},
  };
  await vm.runInNewContext(`(async () => {${script}\n})()`, context);
  return {window, preview, installs, messages};
}

function results(h) { return h.messages.filter(message => message.type === "result"); }
function failures(h) { return h.messages.filter(message => message.type === "failure"); }

test("750 KB source is rendered whole and remains staged until native ownership is acknowledged", async () => {
  const markdown = "# Imported\n\n" + "word ".repeat(150_000) + "\nEND OF DOCUMENT";
  const html = `<article>${markdown}</article>`;
  let received;
  const h = await harness({renderHtml: async (source, options) => {
    received = {source, options};
    return output(html);
  }});
  assert.equal(await h.window.frankenRender({requestID: 1, markdown, options: {font: "serif"}}), true);
  assert.equal(received.source, markdown);
  assert.equal(received.options.font, "serif");
  assert.ok(encoder.encode(markdown).byteLength > 750_000);
  assert.equal(h.preview.srcdoc, "previous preview");
  assert.equal(results(h).length, 0);
  assert.equal(h.window.frankenCommitPreview(2), false);
  assert.equal(h.window.frankenCommitPreview(1), true);
  assert.equal(h.preview.srcdoc, html);
  assert.equal(results(h)[0].outputBytes, encoder.encode(html).byteLength);
  assert.equal(h.window.frankenCommitPreview(1), false);
  assert.equal(h.installs.length, 1);
});

test("out-of-order completions can publish only the newest prepared preview", async () => {
  const jobs = new Map();
  const h = await harness({renderHtml: source => {
    const job = deferred(); jobs.set(source, job); return job.promise;
  }});
  const old = h.window.frankenRender({requestID: 1, markdown: "old"});
  const current = h.window.frankenRender({requestID: 2, markdown: "current"});
  jobs.get("current").resolve(output("CURRENT"));
  assert.equal(await current, true);
  jobs.get("old").resolve(output("OLD"));
  assert.equal(await old, false);
  assert.equal(h.window.frankenCommitPreview(1), false);
  assert.equal(h.window.frankenCommitPreview(2), true);
  assert.deepEqual(h.installs, ["CURRENT"]);
  assert.equal(results(h)[0].requestID, 2);
});

test("discarding an in-flight preview prevents its late completion from becoming publishable", async () => {
  const job = deferred();
  let calls = 0;
  const h = await harness({renderHtml: async () => ++calls === 1 ? job.promise : output("NEW")});
  const old = h.window.frankenRender({requestID: 1, markdown: "old"});
  assert.equal(h.window.frankenDiscardPreview(1), true);
  job.resolve(output("OLD"));
  assert.equal(await old, false);
  assert.equal(h.window.frankenCommitPreview(1), false);
  assert.equal(await h.window.frankenRender({requestID: 2, markdown: "new"}), true);
  assert.equal(h.window.frankenCommitPreview(2), true);
  assert.deepEqual(h.installs, ["NEW"]);
});

test("discarding staged work releases it and refuses a replay without parsing again", async () => {
  let calls = 0;
  const h = await harness({renderHtml: async () => { calls += 1; return output("HTML"); }});
  assert.equal(await h.window.frankenRender({requestID: 5, markdown: "old"}), true);
  h.window.frankenDiscardPreview(5);
  assert.equal(h.window.frankenCommitPreview(5), false);
  assert.equal(await h.window.frankenRender({requestID: 5, markdown: "replay"}), false);
  assert.equal(calls, 1);
  assert.equal(h.installs.length, 0);
  assert.equal(await h.window.frankenRender({requestID: 6, markdown: "new"}), true);
  assert.equal(h.window.frankenCommitPreview(6), true);
});

test("an obsolete render failure cannot replace the newest preview status", async () => {
  const oldJob = deferred();
  const h = await harness({renderHtml: source => source === "old" ? oldJob.promise : Promise.resolve(output("NEW"))});
  const old = h.window.frankenRender({requestID: 1, markdown: "old"});
  await h.window.frankenRender({requestID: 2, markdown: "new"});
  oldJob.reject(new Error("obsolete failure"));
  assert.equal(await old, false);
  assert.equal(failures(h).length, 0);
  assert.equal(h.window.frankenCommitPreview(2), true);
});

test("the current render reports failure without replacing the last good preview and can retry", async () => {
  const h = await harness({renderHtml: async source => {
    if (source === "bad") throw new Error("render failed");
    return output("RECOVERED");
  }});
  assert.equal(await h.window.frankenRender({requestID: 1, markdown: "bad"}), false);
  assert.match(failures(h)[0].message, /render failed/);
  assert.equal(h.preview.srcdoc, "previous preview");
  assert.equal(await h.window.frankenRender({requestID: 2, markdown: "good"}), true);
  assert.equal(h.window.frankenCommitPreview(2), true);
  assert.equal(h.preview.srcdoc, "RECOVERED");
});

test("full HTML and binary exports remain independent from staged preview publication", async () => {
  const markdown = "Complete source ".repeat(50_000) + "LAST";
  const html = `<main>${markdown}</main>`;
  const pdfBytes = new Uint8Array(100_003);
  for (let i = 0; i < pdfBytes.length; i++) pdfBytes[i] = i % 251;
  const sources = [];
  const h = await harness({
    renderHtml: async source => { sources.push(source); return output(html); },
    renderPdf: async source => { sources.push(source); return {bytes: pdfBytes, diagnostics: []}; },
  });
  await h.window.frankenRender({requestID: 1, markdown});
  await h.window.frankenExportHtml({requestID: 2, markdown});
  await h.window.frankenExportPdf({requestID: 3, markdown});
  assert.deepEqual(sources, [markdown, markdown, markdown]);
  const htmlResult = h.messages.find(message => message.type === "exportHtmlResult");
  const pdfResult = h.messages.find(message => message.type === "exportPdfResult");
  assert.equal(htmlResult.htmlText, html);
  assert.equal(htmlResult.byteLength, encoder.encode(html).byteLength);
  assert.deepEqual(Buffer.from(pdfResult.base64, "base64"), Buffer.from(pdfBytes));
  assert.equal(pdfResult.byteLength, pdfBytes.length);
  assert.equal(h.installs.length, 0);
  assert.equal(h.window.frankenCommitPreview(1), true);
});

test("initialization failure is reported without a fabricated preview request", async () => {
  const h = await harness({}, new Error("local WASM unavailable"));
  assert.equal(h.messages.length, 1);
  assert.equal(failures(h)[0].requestID, undefined);
  assert.match(failures(h)[0].message, /initialization failed.*local WASM unavailable/);
  assert.equal(h.installs.length, 0);
});

test("book exports keep complete chapters and separate PDF metadata from safe site options", async () => {
  const files = Object.freeze([
    Object.freeze({path: "first.md", source: "# First\n\n" + "Complete chapter. ".repeat(50_000) + "FINAL_CHAPTER_MARKER"}),
    Object.freeze({path: "second.md", source: "# Second\n\nLast chapter."}),
  ]);
  const options = Object.freeze({font: "serif", darkMode: "auto", fontScale: 1, lang: "en",
    title: "Collected Notes", author: "Ada Lovelace", pageNumbers: false, allowRawHtml: true,
    toc: true, tocDepth: 3, customCss: "body { color: #123456; }"});
  const captures = [];
  const artifact = {bytes: new Uint8Array([80, 75, 3, 4]), diagnostics: [],
    extension: "zip", mimeType: "application/zip"};
  const h = await harness({
    renderBookPdf: async (chapters, received) => {
      captures.push({kind: "pdf", chapters, received}); return artifact;
    },
    renderBookSite: async (chapters, received) => {
      // The current package refuses these options rather than silently ignoring
      // PDF metadata, and book sites have always used safe-only parsing.
      assert.equal(Object.hasOwn(received, "author"), false);
      assert.equal(Object.hasOwn(received, "pageNumbers"), false);
      assert.equal(received.allowRawHtml, false);
      captures.push({kind: "site", chapters, received}); return artifact;
    },
  });
  await h.window.frankenBook({format: "pdf", files, options});
  await h.window.frankenBook({format: "site", files, options});
  await h.window.frankenBook({format: "site", files, options: {pageNumbers: false, allowRawHtml: false}});
  assert.equal(captures[0].received.author, "Ada Lovelace");
  assert.equal(captures[0].received.pageNumbers, false);
  assert.equal(captures[0].received.allowRawHtml, true);
  for (const capture of captures) assert.deepEqual(capture.chapters, files);
  for (const key of ["font", "darkMode", "fontScale", "lang", "title", "toc", "tocDepth", "customCss"]) {
    assert.equal(captures[1].received[key], options[key], key);
  }
  assert.equal(options.author, "Ada Lovelace");
  assert.equal(options.pageNumbers, false);
  assert.equal(options.allowRawHtml, true, "site policy must not mutate document trust or PDF options");
  assert.equal(h.installs.length, 0);
});
