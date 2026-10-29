// Real worker threads and production publication/Flow transports. The native
// renderer is an explicit observation double, not a PDF or conformance oracle.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { createWorkerRenderer, installDocumentWorker } from "./document_worker.mjs";
import { createFlowAdapter } from "./flow_session.mjs";
import { withFlowExports, normalizeFlowExport } from "./flow_export.mjs";
import { createWorkerFlowSessionWith, installFlowWorker } from "./flow_worker_session.mjs";

const utf8 = value => new TextEncoder().encode(value);
const decode = output => JSON.parse(new TextDecoder().decode(output.bytes));
const mime = { pdf: "application/pdf", html: "text/html; charset=utf-8", epub: "application/epub+zip" };

function nativeRendererDouble(mode) {
  return format => async (source, options) => {
    if (options.pdfA === "2b" && mode === "unsupported")
      throw Object.assign(new Error("archival binding unavailable"), { code: "UNSUPPORTED_WASM_PACKAGE" });
    if (options.pdfA === "2b" && mode === "refuse") throw new Error("pdf_a_file_uri: strict refusal");
    const output = { format, mimeType: mime[format], extension: format,
      sourceLength: utf8(source).length, diagnostics: [],
      bytes: utf8(JSON.stringify({ source, options }, (_key, value) => value instanceof Uint8Array ? [...value] : value)) };
    if (format === "pdf" && options.pdfA === "2b" && mode !== "ignore")
      Object.assign(output, { pdfA: "2b", pdfAStrict: mode === "downgrade" ? false : options.pdfAStrict });
    if (mode === "unexpected") Object.assign(output, { pdfA: "2b", pdfAStrict: false });
    return output;
  };
}
class NativeFlowDouble {
  revision = "1";
  layoutRevision = "1";
  constructor(source) { this.source = source; }
  snapshotJson(revision, layoutRevision, offset) {
    return JSON.stringify({ schemaVersion: 1, revision, layoutRevision, offset, total: 0, nextOffset: null, items: [] });
  }
  free() {}
}

if (!isMainThread) {
  const listeners = new Map();
  const endpoint = {
    addEventListener(type, listener) {
      const wrapped = data => listener({ data }); listeners.set(listener, wrapped); parentPort.on(type, wrapped);
    },
    removeEventListener(type, listener) { parentPort.off(type, listeners.get(listener)); },
    postMessage(message, transfer) {
      if (message.ok && workerData.corrupt) {
        const outputs = message.value?.outputs ?? (Array.isArray(message.value) ? message.value : [message.value]);
        for (const output of outputs) if (output?.format === "pdf") {
          if (workerData.corrupt === "omit") { delete output.pdfA; delete output.pdfAStrict; }
          else output.pdfAStrict = !output.pdfAStrict;
        }
      }
      parentPort.postMessage(message, transfer);
    },
  };
  const render = nativeRendererDouble(workerData.mode);
  if (workerData.kind === "flow") installFlowWorker(endpoint, async (source, options) => {
    const { font, ...layout } = options;
    return withFlowExports(createFlowAdapter(new NativeFlowDouble(source), layout),
      { html: render("html"), pdf: render("pdf"), epub: render("epub") }, font);
  });
  else installDocumentWorker(endpoint, async () => ({
    renderHtml: render("html"), renderPdf: render("pdf"), renderEpub: render("epub"),
  }));
} else {
  function endpoint(data) {
    const worker = new Worker(new URL(import.meta.url), { workerData: data });
    const listeners = new Map();
    return {
      addEventListener(type, listener) {
        const wrapped = data => listener(type === "message" ? { data } : data);
        listeners.set(listener, wrapped); worker.on(type, wrapped);
      },
      removeEventListener(type, listener) { worker.off(type, listeners.get(listener)); listeners.delete(listener); },
      postMessage(value, transfer) { worker.postMessage(value, transfer); },
      terminate() { return worker.terminate(); },
    };
  }
  function document(t, data = {}) {
    const api = createWorkerRenderer({ workerFactory: () => endpoint(data), timeoutMs: 5000 });
    t.after(() => api.dispose()); return api;
  }
  async function flow(t, data = {}) {
    const api = await createWorkerFlowSessionWith(() => endpoint({ kind: "flow", ...data }), "# Original 😀",
      { font: "serif" }, { timeoutMs: 5000 });
    t.after(() => api.dispose()); return api;
  }
  const publication = (api, options) => api.publish("# Original 😀", [
    { format: "html" }, { format: "pdf", options }, { format: "epub" },
  ]);
  const flowPublication = (api, options) => api.exportDocuments([
    { format: "html" }, { format: "pdf", options }, { format: "epub" },
  ], api.token);

  for (const strict of [false, true]) test(`document workers preserve archival receipts and complete options: strict=${strict}`, async t => {
    const api = document(t), bytes = new Uint8Array([90, 1, 2, 91]);
    const options = { pdfA: "2b", pdfAStrict: strict, title: "Before", page: { size: "a4", margins: 36 },
      typography: { pareto: true }, strictPagination: true, baselineGridPt: 13.125, microtype: "expansion",
      running: { footer: { center: "{page}" } }, pdfImages: [{ destination: "logo", bytes: bytes.subarray(1, 3) }] };
    const pending = api.renderPdf("# Original 😀", options);
    options.pdfA = "off"; options.pdfAStrict = !strict; options.title = "After";
    options.typography.pareto = false; options.page.margins = 0; bytes.fill(9);
    const output = await pending, captured = decode(output);
    assert.equal(output.pdfA, "2b"); assert.equal(output.pdfAStrict, strict);
    assert.equal(output.filename("report"), "report.pdf"); assert.equal(output.blob().size, output.bytes.length);
    assert.equal(captured.options.pdfA, "2b"); assert.equal(captured.options.pdfAStrict, strict);
    assert.equal(captured.options.title, "Before"); assert.equal(captured.options.typography.pareto, true);
    assert.equal(captured.options.page.margins.topPt, 36); assert.equal(captured.options.baselineGridPt, 13.125);
    assert.deepEqual(captured.options.pdfImages, [{ destination: "logo", bytes: [1, 2] }]);
    assert.equal(bytes.byteLength, 4); assert.equal(api.pendingBytes, 0);
  });

  test("ordinary document exports keep their envelope even with explicit off", async t => {
    const api = document(t);
    for (const options of [{}, { pdfA: "off", pdfAStrict: false }, { pdfAStrict: false }]) {
      const output = await api.renderPdf("source", options);
      assert.equal(Object.hasOwn(output, "pdfA"), false); assert.equal(Object.hasOwn(output, "pdfAStrict"), false);
      assert.equal(Object.hasOwn(decode(output).options, "pdfA"), false);
    }
  });

  test("all publication outputs retain their own successful emission policy", async t => {
    const api = document(t), options = { pdfA: "2b", pdfAStrict: true };
    const pending = publication(api, options); options.pdfA = "off"; options.pdfAStrict = false;
    const edition = await pending;
    assert.deepEqual(edition.outputs.map(output => output.format), ["html", "pdf", "epub"]);
    assert.deepEqual(edition.outputs.map(output => [output.pdfA, output.pdfAStrict]),
      [[undefined, undefined], ["2b", true], [undefined, undefined]]);
    assert.equal(edition.totalBytes, edition.outputs.reduce((sum, output) => sum + output.bytes.length, 0));
    assert.equal(decode(edition.outputs[1]).options.pdfAStrict, true);
  });

  for (const mode of ["ignore", "downgrade", "unexpected"]) for (const batch of [false, true])
    test(`document publication rejects unacknowledged policy: ${mode}, batch=${batch}`, async t => {
      const api = document(t, { mode });
      const options = mode === "unexpected" ? {} : { pdfA: "2b", pdfAStrict: true };
      await assert.rejects(batch ? publication(api, options) : api.renderPdf("source", options), { code: "WORKER_PROTOCOL_ERROR" });
      assert.equal(api.disposed, true); assert.equal(api.pendingBytes, 0);
    });

  for (const corrupt of ["omit", "downgrade"]) for (const batch of [false, true])
    test(`document client rejects policy altered after worker validation: ${corrupt}, batch=${batch}`, async t => {
      const api = document(t, { corrupt }), options = { pdfA: "2b", pdfAStrict: true };
      await assert.rejects(batch ? publication(api, options) : api.renderPdf("source", options), { code: "WORKER_PROTOCOL_ERROR" });
      assert.equal(api.disposed, true);
    });

  for (const [mode, code] of [["unsupported", "UNSUPPORTED_WASM_PACKAGE"], ["refuse", "RENDER_FAILED"]])
    test(`native archival refusal is terminal for an atomic document edition: ${mode}`, async t => {
      const api = document(t, { mode });
      await assert.rejects(publication(api, { pdfA: "2b", pdfAStrict: true }), { code });
      assert.equal(api.disposed, true);
    });

  test("invalid policy, misplaced keys and accessors reject before worker or assets", async () => {
    let factories = 0, getters = 0;
    const api = createWorkerRenderer({ workerFactory() { factories++; throw Error("unexpected start"); } });
    const asset = { destination: "image", get bytes() { getters++; return new Uint8Array([1]); } };
    for (const options of [{ pdfA: "3b" }, { pdfAStrict: true }, { pdfA: "off", pdfAStrict: true },
      { pdfA: "2b", pdfAStrict: "true" }, { pdfA: "2b", baselineGridPt: 12, fitToPages: 2 }])
      await assert.rejects(api.renderPdf("x", { ...options, pdfImages: [asset] }), { code: "INVALID_OPTIONS" });
    for (const format of ["html", "epub", "svg", "interactive-html"])
      await assert.rejects(api.render(format, "x", { pdfA: "2b" }), { code: "INVALID_OPTIONS" });
    await assert.rejects(api.renderPdf("x", { get pdfA() { getters++; return "2b"; } }), { code: "INVALID_OPTIONS" });
    assert.equal(factories, 0); assert.equal(getters, 0); api.dispose();
  });

  for (const strict of [false, true]) test(`Flow workers retain policy in single and atomic exports: strict=${strict}`, async t => {
    const api = await flow(t), options = { pdfA: "2b", pdfAStrict: strict, typography: { pareto: true }, baselineGridPt: 12 };
    const pending = api.exportDocument("pdf", options, api.token);
    options.pdfA = "off"; options.pdfAStrict = !strict; options.typography.pareto = false;
    const output = await pending;
    assert.equal(output.pdfA, "2b"); assert.equal(output.pdfAStrict, strict);
    assert.equal(decode(output).options.typography.pareto, true);
    assert.equal(output.revision, api.token.revision);
    const edition = await flowPublication(api, { pdfA: "2b", pdfAStrict: strict });
    assert.deepEqual(edition.map(output => [output.pdfA, output.pdfAStrict]),
      [[undefined, undefined], ["2b", strict], [undefined, undefined]]);
    const ordinary = await api.exportDocument("pdf", { pdfA: "off" }, api.token);
    assert.equal(Object.hasOwn(ordinary, "pdfA"), false); assert.equal(api.pendingBytes, 0);
  });

  for (const mode of ["ignore", "downgrade", "unexpected"]) for (const batch of [false, true])
    test(`Flow refuses an incorrect backend receipt: ${mode}, batch=${batch}`, async t => {
      const api = await flow(t, { mode }), options = mode === "unexpected" ? {} : { pdfA: "2b", pdfAStrict: true };
      await assert.rejects(batch ? flowPublication(api, options) : api.exportDocument("pdf", options, api.token),
        { code: "INVALID_WASM_RESPONSE" });
      assert.equal(api.disposed, true);
    });

  for (const corrupt of ["omit", "downgrade"]) for (const batch of [false, true])
    test(`Flow client refuses altered archival acknowledgment: ${corrupt}, batch=${batch}`, async t => {
      const api = await flow(t, { corrupt }), options = { pdfA: "2b", pdfAStrict: true };
      await assert.rejects(batch ? flowPublication(api, options) : api.exportDocument("pdf", options, api.token),
        { code: "WORKER_PROTOCOL_ERROR" });
      assert.equal(api.disposed, true);
    });

  for (const [mode, code] of [["unsupported", "UNSUPPORTED_WASM_PACKAGE"], ["refuse", "EXPORT_FAILED"]])
    test(`Flow native archival failure cannot downgrade a publication: ${mode}`, async t => {
      const api = await flow(t, { mode });
      await assert.rejects(flowPublication(api, { pdfA: "2b", pdfAStrict: true }), { code });
      assert.equal(api.disposed, false);
      assert.equal((await api.exportDocument("pdf", {}, api.token)).format, "pdf");
    });

  test("Flow policy admission is immutable and refuses keys on non-PDF formats", () => {
    const token = { revision: "1", layoutRevision: "1" };
    const normalized = normalizeFlowExport("pdf", { pdfA: "2b" }, token)[1];
    assert.equal(normalized.pdfA, "2b"); assert.equal(normalized.pdfAStrict, false); assert(Object.isFrozen(normalized));
    for (const format of ["html", "epub", "svg"])
      assert.throws(() => normalizeFlowExport(format, { pdfA: "off" }, token), { code: "INVALID_OPTIONS" });
    for (const options of [{ pdfA: "3b" }, { pdfAStrict: true }, { pdfA: "2b", pdfAStrict: 1 }])
      assert.throws(() => normalizeFlowExport("pdf", options, token), { code: "INVALID_OPTIONS" });
  });
}
