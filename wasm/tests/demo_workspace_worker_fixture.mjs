// Physical Node worker, actual demo entry/runtime admission/native publisher.
// The generated binding implementation below is an explicit ABI fixture, not
// a claim that current Rust was compiled or that a browser rendered the result.
import { parentPort, workerData } from "node:worker_threads";

const listeners = new Map();
globalThis.addEventListener = (type, listener) => {
  const wrapped = data => listener({ data }); listeners.set(listener, wrapped); parentPort.on(type, wrapped);
};
globalThis.removeEventListener = (type, listener) => { parentPort.off(type, listeners.get(listener)); listeners.delete(listener); };
globalThis.postMessage = (value, transfer) => parentPort.postMessage(value, transfer);

const binding = String.raw`
const json = value => JSON.stringify(value).replace(/</g, '\\u003c');
const escape = value => value.replace(/[&<>]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;'}[c]));
function result(value, mimeType = 'text/html;charset=utf-8') {
  return {bytes: new TextEncoder().encode(value), mimeType, diagnosticsJson: () => '[]', free() {}};
}
export default async function init({module_or_path}) {
  if (module_or_path[1] !== 97) throw new Error('fixture received wrong runtime');
}
export function renderInteractiveHtmlConfigured(source) {
  return result('<!DOCTYPE html><html><head><title>ABI fixture</title></head><body>'
    + '<header class="fmd-app-header"></header><div class="fmd-app-body" id="fmd-app-body">'
    + '<section id="editor-pane"><textarea id="fmd-editor">' + escape(source) + '</textarea></section>'
    + '<main id="preview-pane"><div class="fmd-content" id="fmd-content"></div>\n  </main>\n</div>\n'
    + '<script type="application/json" id="fmd-raw-source">' + json(source) + '</script>\n'
    + '<script>\nwindow.__fmdNativeRuntime; /* fmd-async-preview-v1 */\n</script></body></html>');
}
export function renderHtmlConfiguredAdvanced(source) {
  return result('<!doctype html><html><head></head><body><p>' + escape(source) + '</p></body></html>');
}
export function renderPdfConfiguredMulti() { return result('%PDF-1.7 fixture', 'application/pdf'); }
`;

globalThis.fetch = async (url, options) => {
  if (options.redirect !== "error" || options.mode !== "same-origin") throw new Error("Unrestricted fixture request");
  workerData?.started?.postMessage("runtime-started");
  if (workerData?.hang) await new Promise(() => {});
  if (url.pathname.endsWith("/pkg/franken_markdown_bg.wasm"))
    return new Response(Uint8Array.from([0, 97, 115, 109, 1, 0, 0, 0]));
  if (url.pathname.endsWith("/pkg/franken_markdown.js"))
    return new Response(workerData?.oldPackage ? "export default async function init() {}" : binding);
  throw new Error(`Unexpected runtime fetch: ${url}`);
};
await import("../demo/demo_workspace_worker_entry.js");
