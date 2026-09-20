// Physical Node worker running the production protocol/transport and JS facade.
// The native boundary is an explicit wire double, not a generated WASM build.
import { parentPort, workerData } from "node:worker_threads";
import { installFlowWorker } from "../flow_worker_session.mjs";
import { createFlowAdapter } from "../flow_session.mjs";

const listeners = new Map();
const endpoint = {
  addEventListener(kind, fn) { const wrapped = data => fn({ data }); listeners.set(fn, wrapped); parentPort.on(kind, wrapped); },
  removeEventListener(kind, fn) { parentPort.off(kind, listeners.get(fn)); listeners.delete(fn); },
  postMessage(message, transfer) {
    if (workerData === "legacy" && message.id === 1 && message.ok) message.value = null;
    if (workerData === "bad-query" && message.value?.queryKind === "viewport-v1") {
      message.value.viewport = { ...message.value.viewport, y: message.value.viewport.y + 1 };
    }
    parentPort.postMessage(message, transfer);
  }
};
class NativeWireDouble {
  revision = "1"; layoutRevision = "1"; source = "source";
  constructor() { if (workerData === "unsupported") this.viewportJson = undefined; }
  free() {}
  snapshotJson(revision, layoutRevision, offset) {
    return JSON.stringify({ schemaVersion: 1, revision, layoutRevision, offset, total: 0, nextOffset: null, items: [] });
  }
  reflow(revision, layoutRevision) {
    if (revision !== this.revision || layoutRevision !== this.layoutRevision) throw new Error("unexpected test reflow token");
    this.layoutRevision = (BigInt(this.layoutRevision) + 1n).toString();
  }
  viewportJson(revision, layoutRevision, x, y, width, height, afterIndex) {
    const viewport = { x, y, width, height };
    return JSON.stringify({ schemaVersion: 1, revision, layoutRevision,
      queryKind: "viewport-v1", shapingProfile: "bundled-simple-ltr", viewport,
      afterIndex, total: 100000, totalBounds: { x: 0, y: 0, width: 800, height: 2000000 },
      visitedEntries: 32, nextIndex: null, items: afterIndex > 95000 ? [] : [{ index: 95000,
        kind: "text", bounds: viewport, effectiveClip: viewport, text: "é🙂", fontRun: null,
        fontSize: 14, colorRole: "text", enclosingSourceSpan: { startByte: 0, endByte: 6 } }] });
  }
}
installFlowWorker(endpoint, async () => {
  const session = createFlowAdapter(new NativeWireDouble());
  if (workerData !== "delayed") return session;
  const descriptors = Object.getOwnPropertyDescriptors(session);
  descriptors.viewport.value = async query => {
    await new Promise(resolve => setTimeout(resolve, 60));
    return session.viewport(query);
  };
  return Object.create(Object.getPrototypeOf(session), descriptors);
});
