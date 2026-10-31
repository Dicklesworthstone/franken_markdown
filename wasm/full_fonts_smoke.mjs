// Execute the matching generated WASM and production worker entries, without
// renderer doubles. This proves font-byte retention and API wiring, not visual
// browser shaping quality or a release size/performance qualification.
// Usage: node wasm/full_fonts_smoke.mjs <assembled-package> <generated.wasm> [font.otf]
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { inflateRawSync } from "node:zlib";

if (!isMainThread) {
  const listeners = new Set(), pending = [];
  parentPort.on("message", data => {
    if (!listeners.size) pending.push(data);
    else for (const listener of listeners) listener({ data });
  });
  globalThis.self = globalThis;
  globalThis.addEventListener = (kind, listener) => {
    assert.equal(kind, "message");
    listeners.add(listener);
    for (const data of pending.splice(0)) queueMicrotask(() => listener({ data }));
  };
  globalThis.removeEventListener = (_kind, listener) => listeners.delete(listener);
  globalThis.postMessage = (data, transfer) => parentPort.postMessage(data, transfer);
  const core = await import(pathToFileURL(resolve(workerData.packageDir, "franken_markdown.js")));
  await core.init(new Uint8Array(await readFile(workerData.wasmPath)));
  await import(pathToFileURL(resolve(workerData.packageDir, workerData.entry)));
} else {
  const [packageArg, wasmArg, fontArg, ...extra] = process.argv.slice(2);
  if (!packageArg || !wasmArg || extra.length)
    throw new Error("usage: full_fonts_smoke.mjs <assembled-package> <generated.wasm> [font.otf]");
  const packageDir = resolve(packageArg), wasmPath = resolve(wasmArg);
  const font = new Uint8Array(await readFile(fontArg
    ? resolve(fontArg)
    : new URL("../tests/fixtures/fonts/source-sans-3/SourceSans3-Regular.otf", import.meta.url)));
  const wasm = new Uint8Array(await readFile(wasmPath));
  const load = name => import(pathToFileURL(resolve(packageDir, name)));
  const core = await load("franken_markdown.js");
  await core.init(wasm);
  const native = await load("pkg/franken_markdown.js");
  const books = await load("book.js");
  const previews = await load("book_site_preview.mjs");
  const source = "# Typography\n\noffice AVATAR z\u0301\u0307.\n";
  const files = [{ path: "chapter.md", source }];
  const assets = [{ slot: "body-regular", bytes: font }];
  const full = { fontAssets: assets, fontEmbedding: "full" };
  const fontUri = "data:font/otf;base64," + Buffer.from(font).toString("base64");
  const passed = [];
  const pass = name => { passed.push(name); console.log("full-fonts: " + name + " PASS"); };
  const htmlText = value => new TextDecoder().decode(value.bytes);

  // Read the emitter's ZIP local records independently with Node's DEFLATE,
  // not with the renderer's own archive or font decoders.
  function zipEntries(bytes) {
    const zip = Buffer.from(bytes), entries = new Map();
    let at = 0;
    while (at + 4 <= zip.length && zip.readUInt32LE(at) === 0x04034b50) {
      assert.ok(at + 30 <= zip.length);
      assert.equal(zip.readUInt16LE(at + 6) & 9, 0, "no encrypted or data-descriptor entries");
      const method = zip.readUInt16LE(at + 8), compressed = zip.readUInt32LE(at + 18);
      const size = zip.readUInt32LE(at + 22), nameEnd = at + 30 + zip.readUInt16LE(at + 26);
      const start = nameEnd + zip.readUInt16LE(at + 28), end = start + compressed;
      assert.ok(end <= zip.length && nameEnd <= start);
      const name = zip.subarray(at + 30, nameEnd).toString("utf8");
      assert.ok(!entries.has(name), "unique archive entry: " + name);
      assert.ok(method === 0 || method === 8, "supported ZIP method");
      const value = method === 0 ? zip.subarray(start, end) : inflateRawSync(zip.subarray(start, end));
      assert.equal(value.length, size);
      entries.set(name, value);
      at = end;
    }
    assert.ok(entries.size > 0);
    return entries;
  }
  const checkEpub = bytes => assert.deepEqual(
    zipEntries(bytes).get("OEBPS/fonts/font-1.otf"), Buffer.from(font),
    "EPUB retains the exact complete supplied CFF font");
  function checkSite(bytes) {
    const pages = [...zipEntries(bytes)].filter(([name]) => name.endsWith(".html"));
    assert.ok(pages.some(([, bytes]) => bytes.toString("utf8").includes(fontUri)),
      "site HTML retains the exact complete supplied CFF font");
  }

  assert.ok(native.renderHtmlConfiguredAdvanced.length >= 21);
  assert.ok(native.renderEpubConfiguredAdvanced.length >= 19);
  assert.ok(native.renderBookSitePublication.length >= 23);
  pass("generated trailing-argument ABI");

  const compactHtml = await core.renderHtml(source, { fontAssets: assets, htmlFontFormat: "ttf" });
  assert.deepEqual(compactHtml.bytes, (await core.renderHtml(source, {
    fontAssets: assets, htmlFontFormat: "ttf", fontEmbedding: "subset",
  })).bytes);
  const fullHtml = await core.renderHtml(source, { ...full, htmlFontFormat: "ttf" });
  assert.ok(htmlText(fullHtml).includes(fontUri));
  assert.ok(!htmlText(compactHtml).includes(fontUri));
  // Omitted htmlFontFormat must keep its ABI slot when Full is appended.
  const defaultContainer = await core.renderHtml(source, full);
  assert.deepEqual(defaultContainer.bytes, (await core.renderHtml(source, {
    ...full, htmlFontFormat: "woff1",
  })).bytes);
  pass("HTML exact font bytes, subset default and font-container slot");

  const compactEpub = await core.renderEpub(source, { fontAssets: assets });
  assert.deepEqual(compactEpub.bytes, (await core.renderEpub(source, {
    fontAssets: assets, fontEmbedding: "subset",
  })).bytes);
  const fullEpub = await core.renderEpub(source, full);
  checkEpub(fullEpub.bytes);
  assert.notDeepEqual(fullEpub.bytes, compactEpub.bytes);
  const bundled = zipEntries((await core.renderEpub(source, { fontEmbedding: "full" })).bytes);
  assert.ok([...bundled.keys()].some(name => name.startsWith("OEBPS/fonts/")));
  pass("EPUB exact host bytes and complete bundled fonts");

  const fullOptions = { ...full, htmlFontFormat: "ttf" };
  // The canonical one-shot site ABI uses its default font container. Explicit
  // container selection belongs to the reusable BookSession API below.
  const rawSite = await core.renderBookSite(files, full);
  assert.equal(rawSite.fontEmbedding, "full");
  const rawCompact = await core.renderBookSite(files, { fontAssets: assets });
  assert.equal(rawCompact.fontEmbedding, undefined);
  assert.notDeepEqual(rawSite.bytes, rawCompact.bytes, "raw site must consume Full");
  const rawPages = [...zipEntries(rawSite.bytes)]
    .filter(([name]) => name.endsWith(".html")).map(([, bytes]) => bytes.toString("utf8"));
  const preview = await previews.renderBookPreview(core, files, full);
  assert.equal(preview.fontEmbedding, "full");
  const decodedPreview = previews.parseBookPreview(preview.bytes);
  assert.ok(decodedPreview.pages.some(page => rawPages.includes(page.html)),
    "preview retains the native Full site's HTML");
  pass("raw site ABI and real preview codec");

  const compactBook = await books.createBook(files, { fontAssets: assets, htmlFontFormat: "ttf" });
  const fullBook = await books.createBook(files, fullOptions);
  let fullBookPdf, fullBookEpub;
  try {
    const revision = fullBook.sourceRevision, length = fullBook.sourceLength;
    fullBookPdf = fullBook.renderPdf();
    fullBookEpub = fullBook.renderEpub();
    assert.deepEqual(fullBookPdf.bytes, compactBook.renderPdf().bytes);
    assert.equal(fullBookPdf.fontEmbedding, undefined);
    assert.equal(fullBookEpub.fontEmbedding, "full");
    checkEpub(fullBookEpub.bytes);
    checkSite(fullBook.renderSite().bytes);
    assert.equal(fullBook.sourceRevision, revision);
    assert.equal(fullBook.sourceLength, length);
    const chapter = fullBook.renderChapterPreview(0);
    assert.equal(chapter.fontEmbedding, "full");
    assert.ok(new TextDecoder().decode(chapter.bytes).includes(fontUri));
    fullBook.updateSources([{ path: "chapter.md", source: source + "\nMore text.\n" }]);
    assert.equal(fullBook.sourceRevision, revision + 1);
    checkEpub(fullBook.renderEpub().bytes);
  } finally {
    fullBook.dispose();
    compactBook.dispose();
  }
  pass("native book exports, unchanged PDF, chapter preview and source edits");

  const raw = new native.FmdBook(["chapter.md"], [source]);
  try {
    raw.setFont("body-regular", font);
    raw.setFontEmbedding("full");
    const revision = raw.sourceRevision, output = raw.renderEpub();
    checkEpub(output);
    assert.throws(() => raw.setFontEmbedding("guess"));
    assert.equal(raw.sourceRevision, revision);
    assert.deepEqual(raw.renderEpub(), output);
  } finally { raw.free(); }
  pass("invalid native policy preserves the previous full-font book");

  class Endpoint extends EventTarget {
    constructor(entry) {
      super();
      this.thread = new Worker(new URL(import.meta.url), { workerData: { packageDir, wasmPath, entry } });
      this.thread.on("message", data => this.dispatchEvent(new MessageEvent("message", { data })));
      this.thread.on("error", error => { console.error(error); this.dispatchEvent(new Event("error")); });
      this.thread.on("messageerror", () => this.dispatchEvent(new Event("messageerror")));
    }
    postMessage(data, transfer) { this.thread.postMessage(data, transfer); }
    terminate() { return this.thread.terminate(); }
  }
  const { createWorkerRenderer } = await load("document_worker.mjs");
  const worker = createWorkerRenderer({
    workerFactory: () => new Endpoint("document_worker_entry.js"), timeoutMs: 120000,
  });
  try {
    assert.deepEqual((await worker.renderHtml(source, fullOptions)).bytes, fullHtml.bytes);
    const epub = await worker.renderEpub(source, full);
    assert.deepEqual(epub.bytes, fullEpub.bytes);
    checkEpub(epub.bytes);
    assert.ok(font.byteLength > 0, "caller retains font bytes");
  } finally { worker.dispose(); }
  pass("actual document worker HTML/EPUB parity");

  const { createBookWorker } = await load("book-worker.js");
  const bookWorker = createBookWorker({
    workerFactory: () => new Endpoint("book_worker_entry.js"), timeoutMs: 120000,
    retainBook: true, idleTimeoutMs: 60000,
  });
  try {
    const pdf = await bookWorker.render(files, "pdf", fullOptions);
    assert.deepEqual(pdf.bytes, fullBookPdf.bytes);
    assert.equal(pdf.fontEmbedding, undefined);
    const epub = await bookWorker.renderSourceUpdate([], "epub", {
      expectedRevision: pdf.retainedInputRevision,
    });
    assert.equal(epub.fontEmbedding, "full");
    assert.deepEqual(epub.bytes, fullBookEpub.bytes);
    checkEpub(epub.bytes);
    const site = await bookWorker.renderSourceUpdate([], "site", {
      expectedRevision: epub.retainedInputRevision,
    });
    assert.equal(site.fontEmbedding, "full");
    checkSite(site.bytes);
  } finally { bookWorker.dispose(); }
  pass("actual retained book worker PDF-first then EPUB/site");
  console.log(JSON.stringify({
    schema: "fmd-full-font-generated-smoke-v1",
    wasmSha256: createHash("sha256").update(wasm).digest("hex"),
    fontSha256: createHash("sha256").update(font).digest("hex"),
    passed,
  }));
}
