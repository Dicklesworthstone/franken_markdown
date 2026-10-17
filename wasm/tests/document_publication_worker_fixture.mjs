// Real worker transport/dispatch fixture. The deterministic facade below is a
// test double, NOT evidence of native rendering or generated-WASM parity.
import { parentPort, workerData } from "node:worker_threads";
import { installDocumentWorker } from "../document_worker.mjs";

const formats = {
  html: ["renderHtml", "text/html; charset=utf-8", "html"],
  pdf: ["renderPdf", "application/pdf", "pdf"],
  svg: ["renderSvg", "image/svg+xml", "svg"],
  epub: ["renderEpub", "application/epub+zip", "epub"],
  "interactive-html": ["renderInteractiveHtml", "text/html; charset=utf-8", "html"],
};
const encoder = new TextEncoder();
const storage = new Uint8Array(2048);
const listeners = new Map();
installDocumentWorker({
  addEventListener(type, listener) {
    const wrapped = data => listener({ data });
    listeners.set(listener, wrapped);
    parentPort.on(type, wrapped);
  },
  removeEventListener(type, listener) { parentPort.off(type, listeners.get(listener)); },
  postMessage(value, transfer) {
    parentPort.postMessage(value, transfer);
    parentPort.postMessage({ testEvent: "retained-storage", bytes: storage.byteLength });
  },
}, async () => {
  parentPort.postMessage({ testEvent: "loaded" });
  return Object.fromEntries(Object.entries(formats)
    .filter(([format]) => format !== workerData?.missing)
    .map(([format, [method, mimeType, extension]]) => [method, async (source, options) => {
      const captured = JSON.parse(JSON.stringify(options, (_key, value) =>
        value instanceof Uint8Array ? [...value] : value));
      parentPort.postMessage({ testEvent: "render", format, source, options: captured });
      if (format === workerData?.hang) await new Promise(() => {});
      if (format === workerData?.fail || (source === "fail-pdf" && format === "pdf"))
        throw new Error("fixture renderer failure");
      let bytes = encoder.encode(JSON.stringify({ format, source, options: captured }));
      if (workerData?.outputSize) bytes = new Uint8Array(workerData.outputSize).fill(format.charCodeAt(0));
      if (workerData?.reuseStorage) {
        storage.fill(format.charCodeAt(0));
        bytes = storage.subarray(7, 23);
      }
      const diagnostics = workerData?.diagnostics ? Array.from({ length: workerData.diagnostics }, () => ({
        severity: "warning", start: 0, end: 0, scope: "document", code: "fixture_warning",
        message: "x".repeat(workerData.diagnosticSize ?? 1),
      })) : [];
      return { format, mimeType, extension, sourceLength: encoder.encode(source).length,
        bytes, diagnostics, text: () => "helpers must never be cloned" };
    }]));
});
