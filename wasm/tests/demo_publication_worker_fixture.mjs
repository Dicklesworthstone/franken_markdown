// Actual document-worker protocol in a physical Node worker. The renderer is an
// explicit receipt double: this proves transport/settings/resources, not native
// EPUB/SVG/PDF rendering or browser download behavior.
import { parentPort } from "node:worker_threads";
import { createHash } from "node:crypto";
import { installDocumentWorker } from "../document_worker.mjs";

const listeners = new Map();
const endpoint = {
  addEventListener(type, listener) {
    const callback = data => listener({ data });
    listeners.set(listener, callback); parentPort.on(type, callback);
  },
  removeEventListener(type, listener) {
    parentPort.off(type, listeners.get(listener)); listeners.delete(listener);
  },
  postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
};
installDocumentWorker(endpoint, () => Object.fromEntries([
  ["html", "renderHtml", "text/html; charset=utf-8"], ["pdf", "renderPdf", "application/pdf"],
  ["epub", "renderEpub", "application/epub+zip"], ["svg", "renderSvg", "image/svg+xml"],
].map(([format, method, mimeType]) => [method, (source, options) => {
  const { pdfImages = [], ...settings } = options;
  return {
    format, mimeType, extension: format, sourceLength: new TextEncoder().encode(source).length,
    ...(options.profile === undefined ? {} : { profile: options.profile }),
    bytes: new TextEncoder().encode(JSON.stringify({ source, settings,
      images: pdfImages.map(({ destination, bytes }) => ({ destination, size: bytes.length,
        sha256: createHash("sha256").update(bytes).digest("hex") })),
    })), diagnostics: [],
  };
}])));
