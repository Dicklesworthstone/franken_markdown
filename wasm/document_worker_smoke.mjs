// Generated-artifact gate: actual worker entry, actual WASM, no renderer doubles.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

if (!isMainThread) {
  // Browser-compatible endpoint; retain messages while loading real WASM. The
  // worker paths are fixed by this test harness, never received over RPC.
  const listeners = new Set(), pending = [];
  parentPort.on("message", data => {
    if (!listeners.size) pending.push(data);
    else for (const listener of listeners) listener({ data });
  });
  globalThis.addEventListener = (kind, listener) => {
    assert.equal(kind, "message");
    listeners.add(listener);
    for (const data of pending.splice(0)) queueMicrotask(() => listener({ data }));
  };
  globalThis.removeEventListener = (_kind, listener) => listeners.delete(listener);
  globalThis.postMessage = (data, transfer) => parentPort.postMessage(data, transfer);
  const core = await import(pathToFileURL(resolve(workerData.packageDir, "franken_markdown.js")));
  await core.init(new Uint8Array(await readFile(workerData.wasmPath)));
  await import(pathToFileURL(resolve(workerData.packageDir, "document_worker_entry.js")));
} else {
  const [packageArg, wasmArg] = process.argv.slice(2);
  if (!packageArg || !wasmArg) throw new Error("usage: document_worker_smoke.mjs <package> <generated.wasm>");
  const packageDir = resolve(packageArg), wasmPath = resolve(wasmArg);
  class Endpoint extends EventTarget {
    constructor() {
      super();
      this.thread = new Worker(new URL(import.meta.url), { workerData: { packageDir, wasmPath } });
      this.thread.on("message", data => this.dispatchEvent(new MessageEvent("message", { data })));
      this.thread.on("error", error => { console.error(error); this.dispatchEvent(new Event("error")); });
      this.thread.on("messageerror", () => this.dispatchEvent(new Event("messageerror")));
    }
    postMessage(data, transfer) { this.thread.postMessage(data, transfer); }
    terminate() { return this.thread.terminate(); }
  }
  const core = await import(pathToFileURL(resolve(packageDir, "franken_markdown.js")));
  const direct = await core.createRenderer(new Uint8Array(await readFile(wasmPath)));
  const { createWorkerRenderer } = await import(pathToFileURL(resolve(packageDir, "document_worker.mjs")));
  const worker = createWorkerRenderer({ workerFactory: () => new Endpoint(), timeoutMs: 60000 });
  const source = "# Worker publication\n\nOriginal **Markdown** with a [link](https://example.com).\n\n- First\n- Second\n";
  try {
    for (const [format, method] of [
      ["html", "renderHtml"], ["pdf", "renderPdf"], ["svg", "renderSvg"],
      ["epub", "renderEpub"], ["interactive-html", "renderInteractiveHtml"],
    ]) {
      const options = { font: "serif", ...(format === "pdf" ? { metadataEpochSeconds: 0, pageNumbers: true } : {}) };
      const expected = await direct[method](source, options);
      const actual = await worker[method](source, options);
      for (const key of ["format", "mimeType", "extension", "sourceLength", "diagnostics", "bytes"])
        assert.deepEqual(actual[key], expected[key], `${format} ${key}`);
      console.log(`document-worker: ${format} generated-WASM parity PASS (${actual.bytes.length} bytes)`);
    }
    // A deterministic, 1x1 RGB PNG; both paths receive precisely the same bytes.
    const png = new Uint8Array(Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC", "base64"));
    for (const method of ["renderHtml", "renderPdf", "renderSvg", "renderEpub"]) {
      const options = { pdfImages: [{ destination: "pixel.png", bytes: png }],
        ...(method === "renderPdf" ? { metadataEpochSeconds: 0 } : {}) };
      const source = "# Assets\n\n![Pixel](pixel.png)\n";
      const expected = await direct[method](source, options);
      const actual = await worker[method](source, options);
      assert.deepEqual(actual.bytes, expected.bytes, `${method} image parity`);
      assert.deepEqual(actual.diagnostics, expected.diagnostics);
      assert.ok(png.length > 0, "caller image bytes remain owned");
      console.log(`document-worker: ${method} explicit-image parity PASS`);
    }
    const defaultPdf = await direct.renderPdf(source, { metadataEpochSeconds: 0 });
    for (const page of [
      { size: "a4", orientation: "landscape", margins: 36 },
      { size: { widthPt: 480, heightPt: 720 },
        margins: { topPt: 24, rightPt: 18, bottomPt: 36, leftPt: 30 } },
    ]) {
      const options = { metadataEpochSeconds: 0, page };
      const expected = await direct.renderPdf(source, options);
      const actual = await worker.renderPdf(source, options);
      assert.deepEqual(actual.bytes, expected.bytes, "PDF page geometry parity");
      assert.deepEqual(actual.diagnostics, expected.diagnostics);
      assert.notDeepEqual(actual.bytes, defaultPdf.bytes, "explicit paper geometry must affect the PDF");
      console.log("document-worker: configured PDF paper/margins parity PASS");
    }
    const publication = "# Field guide\n\n![Pixel](pixel.png)\n\n## Chapter\n\nBook text.\n\n### Detail\n";
    const epubOptions = { title: "Field guide", lang: "en", customCss: "p { color: navy; }",
      toc: true, tocDepth: 2, pdfImages: [{ destination: "pixel.png", bytes: png }] };
    const expectedEpub = await direct.renderEpub(publication, epubOptions);
    const actualEpub = await worker.renderEpub(publication, epubOptions);
    assert.deepEqual(actualEpub.bytes, expectedEpub.bytes, "EPUB publication options parity");
    assert.deepEqual(actualEpub.diagnostics, expectedEpub.diagnostics);
    // Hold all other settings fixed so CSS and navigation cannot both be ignored.
    const plainEpub = await direct.renderEpub(publication, {
      title: epubOptions.title, lang: epubOptions.lang, pdfImages: epubOptions.pdfImages,
    });
    assert.notDeepEqual(actualEpub.bytes, plainEpub.bytes, "EPUB stylesheet/navigation must affect the archive");
    console.log("document-worker: EPUB stylesheet/navigation/image parity PASS");

    const missingImage = "# Missing resource\n\n![Missing](not-supplied.png)\n";
    const expectedSvg = await direct.renderSvg(missingImage);
    const actualSvg = await worker.renderSvg(missingImage);
    assert.deepEqual(actualSvg.bytes, expectedSvg.bytes, "SVG missing-resource output parity");
    assert.deepEqual(actualSvg.diagnostics, expectedSvg.diagnostics, "SVG structured diagnostic parity");
    assert.ok(expectedSvg.diagnostics.some(item => item.scope === "document" && typeof item.code === "string"),
      "missing SVG resources must produce a document-scoped reason code");
    console.log("document-worker: SVG structured export diagnostics parity PASS");
  } finally { worker.dispose(); }
}
