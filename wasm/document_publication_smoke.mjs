// DSR execution probe: requires the assembled package and GENERATED WASM.
// No renderer doubles or synthetic WASM can establish the byte-parity checks.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

if (!isMainThread) {
  const listeners = new Map();
  const { installDocumentWorker } = await import(pathToFileURL(resolve(workerData.packageDir, "document_worker.mjs")));
  installDocumentWorker({
    addEventListener(type, listener) {
      const wrapped = data => listener({ data });
      listeners.set(listener, wrapped); parentPort.on(type, wrapped);
    },
    removeEventListener(type, listener) { parentPort.off(type, listeners.get(listener)); },
    postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
  }, async () => {
    const { createRenderer } = await import(pathToFileURL(resolve(workerData.packageDir, "franken_markdown.js")));
    return createRenderer(new Uint8Array(await readFile(workerData.wasmPath)));
  });
} else {
  const [packageArg, wasmArg, ...extra] = process.argv.slice(2);
  if (!packageArg || !wasmArg || extra.length)
    throw new Error("Usage: node wasm/document_publication_smoke.mjs PACKAGE GENERATED_WASM");
  const packageDir = resolve(packageArg), wasmPath = resolve(wasmArg), threads = [];
  const workerFactory = () => {
    const thread = new Worker(new URL(import.meta.url), { workerData: { packageDir, wasmPath } });
    threads.push(thread);
    const listeners = new Map();
    return {
      postMessage(value, transfer) { thread.postMessage(value, transfer); },
      terminate() { return thread.terminate(); },
      addEventListener(type, listener) {
        const wrapped = type === "message" ? data => listener({ data }) : listener;
        listeners.set(listener, wrapped); thread.on(type, wrapped);
      },
      removeEventListener(type, listener) { thread.off(type, listeners.get(listener)); },
    };
  };
  const { createRenderer } = await import(pathToFileURL(resolve(packageDir, "franken_markdown.js")));
  const { createWorkerRenderer } = await import(pathToFileURL(resolve(packageDir, "document_worker.mjs")));
  const { renderDocumentBatch } = await import(pathToFileURL(resolve(packageDir, "document-batch.js")));
  const direct = await createRenderer(new Uint8Array(await readFile(wasmPath)));
  const source = "# Publication édition\n\nOne **coherent** revision.\n\n![Chart](chart.svg)\n";
  const image = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><rect width="32" height="16" fill="#369"/></svg>');
  const assets = [{ destination: "chart.svg", bytes: image }];
  const targets = [
    { format: "html", options: { title: "Edition", toc: true, pdfImages: assets } },
    { format: "pdf", options: { title: "Edition", metadataEpochSeconds: 0, pdfImages: assets,
      page: { size: "a4" }, running: { footer: { right: "{page}/{pages}" } } } },
    { format: "svg", options: { maxWidthPt: 720, pdfImages: assets } },
    { format: "epub", options: { title: "Edition", toc: true, pdfImages: assets } },
    { format: "interactive-html", options: { title: "Edition" } },
  ];
  const methods = { html: "renderHtml", pdf: "renderPdf", svg: "renderSvg",
    epub: "renderEpub", "interactive-html": "renderInteractiveHtml" };
  const expected = [];
  for (const target of targets) {
    const output = await direct[methods[target.format]](source, target.options);
    expected.push({ ...output, bytes: new Uint8Array(output.bytes), diagnostics: structuredClone(output.diagnostics) });
  }
  const verify = publication => {
    assert.equal(publication.format, "document-publication");
    assert.equal(publication.sourceLength, new TextEncoder().encode(source).length);
    assert.equal(publication.totalBytes, expected.reduce((sum, output) => sum + output.bytes.length, 0));
    assert.equal(publication.outputs.length, expected.length);
    for (let index = 0; index < expected.length; index++) {
      const actual = publication.outputs[index], reference = expected[index];
      assert.equal(actual.format, reference.format);
      assert.equal(actual.mimeType, reference.mimeType);
      assert.deepEqual(actual.bytes, reference.bytes, `direct/publication parity: ${actual.format}`);
      assert.deepEqual(actual.diagnostics, reference.diagnostics);
      assert.equal(actual.blob().size, actual.bytes.length);
    }
  };
  const renderer = createWorkerRenderer({ workerFactory, timeoutMs: 120000 });
  let stream, limited;
  try {
    verify(await renderer.publish(source, targets));
    verify(await renderer.publish(source, targets));
    assert.ok(image.byteLength > 0, "host resource was not detached");
    console.error("publication-smoke: direct/worker all-format bytes and diagnostics PASS");
    async function* jobs() {
      for (let index = 0; index < 3; index++) yield { source, targets, name: `edition-${index}` };
    }
    stream = renderDocumentBatch(jobs(), { concurrency: 2, order: "completion",
      worker: { workerFactory, timeoutMs: 120000 } });
    const seen = new Set();
    for await (const receipt of stream) {
      assert.equal(receipt.status, "fulfilled");
      assert.equal(receipt.name, `edition-${receipt.index}`);
      assert.equal(seen.has(receipt.index), false);
      seen.add(receipt.index);
      verify(receipt.value);
    }
    assert.deepEqual([...seen].sort(), [0, 1, 2]);
    console.error("publication-smoke: public async batch all-format parity PASS");
    const maxOutputBytes = expected.reduce((sum, output) => sum + output.bytes.length, 0) - 1;
    limited = createWorkerRenderer({ workerFactory, timeoutMs: 120000, maxOutputBytes });
    await assert.rejects(limited.publish(source, targets), { code: "BUDGET_EXCEEDED" });
    assert.equal(limited.disposed, true);
    console.error("publication-smoke: aggregate output rejection PASS");
  } finally {
    renderer.dispose(); limited?.dispose();
    await stream?.return();
    await Promise.all(threads.map(thread => thread.terminate()));
  }
}
