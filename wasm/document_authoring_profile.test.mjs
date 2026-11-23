// Actual document/Flow worker transports; the native renderers are explicit
// boundary doubles. These tests do not claim parser or generated-WASM parity.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { createWorkerRenderer, installDocumentWorker } from "./document_worker.mjs";
import { createFlowAdapter } from "./flow_session.mjs";
import { withFlowExports } from "./flow_export.mjs";
import { createWorkerFlowSessionWith, installFlowWorker } from "./flow_worker_session.mjs";

const encode = value => new TextEncoder().encode(value);
const profiles = ["commonmark-gfm", "gfm-plus"];
const formats = {
  html: ["renderHtml", "text/html; charset=utf-8"],
  pdf: ["renderPdf", "application/pdf"],
  svg: ["renderSvg", "image/svg+xml"],
  epub: ["renderEpub", "application/epub+zip"],
};
const source = "[Term\r\n: café 😀](/target)\r\n";
const opposite = profile => profile === "commonmark-gfm" ? "gfm-plus" : "commonmark-gfm";

function renderers(data = {}, calls = []) {
  return Object.fromEntries(Object.entries(formats).map(([format, [method, mimeType]]) => [method,
    async (markdown, options) => {
      calls.push({ format, markdown, options });
      const output = { format, mimeType, extension: format, sourceLength: encode(markdown).length,
        bytes: encode(JSON.stringify({ markdown, options }, (_key, value) => value instanceof Uint8Array ? [...value] : value)),
        diagnostics: [{ severity: "warning", start: 0, end: 0, scope: "document", code: "fixture", message: "boundary double" }],
      };
      const mode = !data.target || data.target === format ? data.nativeMode : undefined;
      if (options.profile !== undefined && mode !== "omit") output.profile = options.profile;
      if (mode === "wrong") output.profile = opposite(options.profile);
      if (mode === "invalid") output.profile = "gfm";
      if (mode === "unexpected") output.profile = "gfm-plus";
      if (mode === "accessor") Object.defineProperty(output, "profile", {
        get() { throw new Error("receipt accessors must not execute"); },
      });
      if (options.pdfA === "2b") Object.assign(output, { pdfA: "2b", pdfAStrict: options.pdfAStrict });
      return output;
    },
  ]));
}

class NativeFlowDouble {
  constructor(markdown, profile) {
    this.source = markdown; this.profile = profile; this.revision = "1"; this.layoutRevision = "1";
  }
  header() {
    return { schemaVersion: 1, revision: this.revision, layoutRevision: this.layoutRevision,
      ...(this.profile === undefined ? {} : { profile: this.profile }) };
  }
  snapshotJson(_revision, _layout, offset) {
    return JSON.stringify({ ...this.header(), offset, total: 0, nextOffset: null, items: [] });
  }
  free() {}
}
function flowAdapter(profile, data, calls) {
  const engine = renderers(data, calls);
  return withFlowExports(createFlowAdapter(new NativeFlowDouble(source, profile), undefined, profile),
    Object.fromEntries(Object.entries(formats).map(([format, [method]]) => [format, engine[method]])), "sans");
}

if (!isMainThread) {
  const listeners = new Map();
  const endpoint = {
    addEventListener(type, listener) {
      const wrapped = data => listener({ data }); listeners.set(listener, wrapped); parentPort.on(type, wrapped);
    },
    removeEventListener(type, listener) { parentPort.off(type, listeners.get(listener)); listeners.delete(listener); },
    postMessage(message, transfer) {
      if (message.ok && workerData.wireMode) {
        const outputs = message.value?.outputs ?? (Array.isArray(message.value) ? message.value : [message.value]);
        for (const output of outputs) if (output?.format && (!workerData.target || output.format === workerData.target)) {
          if (workerData.wireMode === "omit") delete output.profile;
          else output.profile = workerData.wireMode === "unexpected" ? "gfm-plus" : opposite(output.profile);
        }
      }
      parentPort.postMessage(message, transfer);
    },
  };
  if (workerData.kind === "flow") installFlowWorker(endpoint, async (_source, options) =>
    flowAdapter(options.profile, workerData));
  else installDocumentWorker(endpoint, async () => renderers(workerData));
} else {
  function endpoint(data) {
    const worker = new Worker(new URL(import.meta.url), { workerData: data }), listeners = new Map();
    return {
      addEventListener(type, listener) {
        const wrapped = data => listener(type === "message" ? { data } : data);
        listeners.set(listener, wrapped); worker.on(type, wrapped);
      },
      removeEventListener(type, listener) { worker.off(type, listeners.get(listener)); listeners.delete(listener); },
      postMessage(data, transfer) { worker.postMessage(data, transfer); },
      terminate() { return worker.terminate(); },
    };
  }
  function document(t, data = {}) {
    const api = createWorkerRenderer({ workerFactory: () => endpoint(data), timeoutMs: 5000 });
    t.after(() => api.dispose()); return api;
  }
  async function flow(t, profile, data = {}) {
    const api = await createWorkerFlowSessionWith(() => endpoint({ kind: "flow", ...data }),
      source, profile === undefined ? {} : { profile }, { timeoutMs: 5000 });
    t.after(() => api.dispose()); return api;
  }
  const targets = profile => Object.keys(formats).map(format => ({ format,
    options: profile === undefined ? {} : { profile } }));

  for (const profile of profiles) test(`document outputs preserve the executed ${profile} grammar and exact resources`, async t => {
    const api = document(t), backing = Uint8Array.of(90, 1, 2, 91);
    const options = { profile, pdfImages: [{ destination: "diagram", bytes: backing.subarray(1, 3) }] };
    const pending = Object.keys(formats).map(format => api.render(format, source, options));
    options.profile = opposite(profile); backing.fill(8);
    for (const output of await Promise.all(pending)) {
      assert.equal(output.profile, profile); assert.equal(Object.isFrozen(output), true);
      assert.equal(output.sourceLength, encode(source).length);
      const captured = JSON.parse(output.text());
      assert.equal(captured.markdown, source); assert.equal(captured.options.profile, profile);
      assert.deepEqual(captured.options.pdfImages, [{ destination: "diagram", bytes: [1, 2] }]);
      assert.equal(output.diagnostics[0].code, "fixture");
      assert.equal(output.blob().size, output.bytes.length);
      assert.equal(output.filename("chapter"), `chapter.${output.format}`);
    }
    assert.equal(api.pendingBytes, 0); assert.equal(backing.byteLength, 4);
  });

  test("complete publications retain each target's captured dialect and PDF archival receipt", async t => {
    const api = document(t), requested = targets();
    requested[0].options.profile = "commonmark-gfm";
    requested[1].options = { profile: "gfm-plus", pdfA: "2b", pdfAStrict: true };
    requested[3].options.profile = "gfm-plus";
    const pending = api.publish(source, requested);
    for (const target of requested) target.options.profile = "later mutation";
    const edition = await pending;
    assert.deepEqual(edition.outputs.map(output => output.profile), ["commonmark-gfm", "gfm-plus", undefined, "gfm-plus"]);
    assert.equal(Object.hasOwn(edition.outputs[2], "profile"), false);
    assert.equal(edition.outputs[1].pdfA, "2b"); assert.equal(edition.outputs[1].pdfAStrict, true);
    assert.equal(edition.totalBytes, edition.outputs.reduce((sum, output) => sum + output.bytes.length, 0));
    assert.equal(api.pendingBytes, 0);
  });

  for (const mode of ["omit", "wrong", "invalid", "accessor", "unexpected"])
    for (const publication of [false, true])
      test(`document ${publication ? "publication" : "render"} refuses a native ${mode} dialect receipt`, async t => {
        const api = document(t, { nativeMode: mode, target: "epub" });
        const profile = mode === "unexpected" ? undefined : "commonmark-gfm";
        const refused = assert.rejects(publication ? api.publish(source, targets(profile))
          : api.renderEpub(source, profile === undefined ? {} : { profile }), { code: "UNSUPPORTED_WASM_PACKAGE" });
        const queued = assert.rejects(api.renderHtml("must not replay"), { code: "SESSION_LOST" });
        await Promise.all([refused, queued]);
        assert.equal(api.disposed, true); assert.equal(api.pendingBytes, 0); assert.equal(api.pendingOperations, 0);
      });

  for (const mode of ["omit", "wrong", "unexpected"]) for (const publication of [false, true])
    test(`document client rejects a ${mode} dialect receipt altered after worker validation: publication=${publication}`, async t => {
      const api = document(t, { wireMode: mode, target: "pdf" });
      const profile = mode === "unexpected" ? undefined : "commonmark-gfm";
      await assert.rejects(publication ? api.publish(source, targets(profile))
        : api.renderPdf(source, profile === undefined ? {} : { profile }), { code: "WORKER_PROTOCOL_ERROR" });
      assert.equal(api.disposed, true); assert.equal(api.pendingBytes, 0);
    });

  test("legacy document renderers without grammar receipts remain compatible when no profile is selected", async t => {
    const api = document(t, { nativeMode: "omit" });
    for (const format of Object.keys(formats)) {
      const output = await api.render(format, source);
      assert.equal(Object.hasOwn(output, "profile"), false);
    }
    for (const output of (await api.publish(source, targets())).outputs)
      assert.equal(Object.hasOwn(output, "profile"), false);
    assert.equal(api.disposed, false);
  });

  for (const mode of ["omit", "wrong", "invalid", "accessor", "unexpected"])
    for (const publication of [false, true])
      test(`direct Flow ${publication ? "publication" : "export"} refuses a native ${mode} dialect receipt`, async t => {
        const profile = mode === "unexpected" ? undefined : "commonmark-gfm", calls = [];
        const api = flowAdapter(profile, { nativeMode: mode, target: "epub" }, calls);
        t.after(() => api.dispose());
        await assert.rejects(publication ? api.exportDocuments(Object.keys(formats).map(format => ({ format })), api.token)
          : api.exportDocument("epub", {}, api.token), { code: "UNSUPPORTED_WASM_PACKAGE" });
        assert.equal(api.source, source); assert.equal(api.profile, profile);
        assert.equal(calls.length, publication ? 4 : 1, "no legacy retry or fallback render");
      });

  for (const mode of ["omit", "wrong"]) for (const publication of [false, true])
    test(`Flow worker refuses native ${mode} dialect receipt: publication=${publication}`, async t => {
      const api = await flow(t, "commonmark-gfm", { nativeMode: mode, target: "epub" });
      await assert.rejects(publication ? api.exportDocuments([{ format: "html" }, { format: "epub" }], api.token)
        : api.exportDocument("epub", {}, api.token), { code: "UNSUPPORTED_WASM_PACKAGE" });
      assert.equal(api.pendingBytes, 0);
    });

  for (const profile of [undefined, ...profiles]) test(`Flow publication preserves acknowledged ${profile ?? "legacy"} dialect`, async t => {
    const api = await flow(t, profile);
    const outputs = await api.exportDocuments(Object.keys(formats).map(format => ({ format })), api.token);
    assert.deepEqual(outputs.map(output => output.profile), Array(4).fill(profile));
    for (const output of outputs) {
      assert.equal(Object.hasOwn(output, "profile"), profile !== undefined);
      assert.equal(JSON.parse(new TextDecoder().decode(output.bytes)).options.profile, profile);
    }
  });
}
