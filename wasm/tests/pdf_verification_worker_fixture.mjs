// Real worker transport and dispatch with an explicitly named renderer double.
// These cases do not execute generated WASM or prove native PDF/layout fidelity.
import { parentPort, workerData } from "node:worker_threads";
import { installDocumentWorker } from "../document_worker.mjs";

const data = workerData ?? {};
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const listeners = new Map();
const latch = data.latch && new Int32Array(data.latch);
const capture = value => JSON.parse(JSON.stringify(value, (_key, item) =>
  item instanceof Uint8Array ? [...item] : item));

function corruptReply(message) {
  if (!data.wireMode || !message.ok || message.value?.format !== "pdf-verification") return;
  const value = message.value;
  switch (data.wireMode) {
    case "source": value.sourceLength++; break;
    case "format": value.format = "pdf"; break;
    case "missing-profile": delete value.profile; break;
    case "wrong-profile": value.profile = "gfm-plus"; break;
    case "invalid-json": value.json = encoder.encode("{"); break;
    case "invalid-utf8": value.json = new Uint8Array([255]); break;
    case "padded-buffer": {
      const bytes = new Uint8Array(value.json.length + 2);
      bytes.set(value.json, 1);
      value.json = bytes.subarray(1, bytes.length - 1);
      break;
    }
    case "report-profile":
    case "report-subset":
    case "report-page-count": {
      const report = JSON.parse(decoder.decode(value.json));
      if (data.wireMode === "report-profile") report.profile = "gfm-plus";
      if (data.wireMode === "report-subset") delete report.pages;
      if (data.wireMode === "report-page-count") report.page_count++;
      value.json = encoder.encode(JSON.stringify(report));
      break;
    }
    default: throw new Error("Unknown fixture wire mode");
  }
}

installDocumentWorker({
  addEventListener(type, callback) {
    const wrapped = value => callback({ data: value });
    listeners.set(callback, wrapped);
    parentPort.on(type, wrapped);
  },
  removeEventListener(type, callback) {
    parentPort.off(type, listeners.get(callback));
    listeners.delete(callback);
  },
  postMessage(message, transfer) {
    corruptReply(message);
    if (message.ok && message.value?.json instanceof Uint8Array)
      transfer = [message.value.json.buffer];
    parentPort.postMessage(message, transfer);
  },
}, async () => {
  parentPort.postMessage({ testEvent: "loaded" });
  if (data.missingBinding) return {};
  let calls = 0;
  const nativeVerificationRendererDouble = {
    async verifyPdf(source, options) {
      const call = ++calls;
      parentPort.postMessage({ testEvent: "verify", source, options: capture(options), call });
      if (source === "block") {
        if (!latch) throw new Error("Blocking fixture needs a latch");
        Atomics.store(latch, 0, 1);
        Atomics.wait(latch, 1, 0);
      }
      if (options.strictPagination && data.paginationFailure) {
        throw JSON.stringify({
          code: "PAGINATION_" + data.paginationFailure.toUpperCase(),
          message: "fixture exact pagination refused: " + data.paginationFailure,
          reason: data.paginationFailure,
        });
      }
      const report = {
        schema_version: "1",
        target: "pdf",
        verdict: "findings",
        page_count: 2,
        digest: "fnv1a64:0123456789abcdef",
        anchors: { resolved: 3, unresolved: ["lost-section"] },
        findings: [
          { code: "overflow", detail: "fixture measured horizontal overflow" },
          { code: "unresolved_anchor", detail: "fixture missing internal destination" },
        ],
        pages: [
          { number: 1, runs: [{ text: source, x: 36, y: 720.5, size: 11, kind: "body" }] },
          { number: 2, runs: [{ text: "Fixture second page", x: 40, y: 690, size: 10, kind: "heading" }] },
        ],
        nativeExtension: { producer: "explicit renderer double", call },
        ...(options.profile === undefined ? {} : { profile: options.profile }),
      };
      switch (data.reportMode) {
        case "missing-profile": delete report.profile; break;
        case "wrong-profile": report.profile = "gfm-plus"; break;
        case "invented-profile": report.profile = "commonmark-gfm"; break;
        case "profile-accessor":
          Object.defineProperty(report, "profile", {
            get() {
              parentPort.postMessage({ testEvent: "profile-getter" });
              throw new Error("Profile receipt getter must never run");
            },
          });
          break;
        case "subset": delete report.pages; break;
        case "page-count": report.page_count++; break;
        case "invalid-run": report.pages[0].runs[0].x = Number.NaN; break;
        case "invalid-digest": report.digest = "fixture-not-native"; break;
      }
      return report;
    },
  };
  return nativeVerificationRendererDouble;
});

