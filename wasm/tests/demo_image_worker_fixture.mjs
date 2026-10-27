// Physical document worker with an explicit native-renderer double. Its JSON
// receipts prove resource ownership/transport, not HTML appearance or PDF ink.
import { createHash } from "node:crypto";
import { parentPort } from "node:worker_threads";
import { installDocumentWorker } from "../document_worker.mjs";

const listeners = new Map();
const endpoint = {
  addEventListener(type, listener) {
    const wrapped = data => listener({ data });
    listeners.set(listener, wrapped);
    parentPort.on(type, wrapped);
  },
  removeEventListener(type, listener) {
    parentPort.off(type, listeners.get(listener));
    listeners.delete(listener);
  },
  postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
};
installDocumentWorker(endpoint, () => Object.fromEntries([
  ["html", "renderHtml", "text/html; charset=utf-8"], ["pdf", "renderPdf", "application/pdf"],
].map(([format, method, mimeType]) => [method, (source, options) => ({
  format, mimeType, extension: format,
  sourceLength: new TextEncoder().encode(source).length,
  bytes: new TextEncoder().encode(JSON.stringify({
    source,
    images: (options.pdfImages ?? []).map(({ destination, bytes }) => ({
      destination, size: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    })),
  })),
  diagnostics: [],
})])));
