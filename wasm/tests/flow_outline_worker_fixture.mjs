// A real worker running the production facade/transport over a deliberately
// synthetic native outline provider. This is NOT proof of Rust/WASM decoding.
import { parentPort } from "node:worker_threads";
import { installFlowWorker } from "../flow_worker_session.mjs";
import { withGlyphOutlines } from "../flow_outlines.mjs";
const listeners = new Map();
const endpoint = {
  postMessage: (value, transfer) => parentPort.postMessage(value, transfer),
  addEventListener(type, listener) {
    const adapted = data => listener({ data }); listeners.set(listener, adapted); parentPort.on(type, adapted);
  },
  removeEventListener(type, listener) { parentPort.off(type, listeners.get(listener)); listeners.delete(listener); }
};
installFlowWorker(endpoint, async (source, layout) => withGlyphOutlines(Object.freeze({
  disposed: false, token: { revision: "1", layoutRevision: "1" },
  layoutOptions: { viewportWidth: layout.viewportWidth, bodySize: layout.bodySize,
    codeSize: layout.codeSize, lineHeight: layout.lineHeight },
  source, dispose() {}
}), (id, glyphs) => {
  if (id === "0") throw JSON.stringify({ code: "UNKNOWN_FONT", message: "fixture unknown font" });
  return JSON.stringify({ schemaVersion: 1, fontId: id, unitsPerEm: 1000,
    ascent: 800, descent: -200, lineGap: 0,
    glyphs: Array.from(glyphs, glyphId => ({ glyphId, commands: [] })) });
}));
