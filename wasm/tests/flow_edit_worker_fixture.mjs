// Real Node worker, real RPC/protocol/facade; only the native renderer is a
// source-transaction double. This is transport coverage, not WASM layout proof.
import { parentPort, workerData } from "node:worker_threads";
import { createFlowAdapter } from "../flow_session.mjs";
import { installFlowWorker } from "../flow_worker_session.mjs";

const reject = (code) => { throw JSON.stringify({ code, message: code }); };
class NativeDouble {
  constructor(source) {
    this.source = source;
    this.revision = "9007199254740993";
    this.layoutRevision = "7";
    this.calls = 0;
    this.last = null;
    if (workerData.legacy) this.editManyUtf16Packed = undefined;
  }
  editManyUtf16Packed(revision, ranges, lengths, text, reuse) {
    if (revision !== this.revision) reject("STALE_REVISION");
    this.calls++;
    if (workerData.gate && this.calls === 1) {
      const gate = new Int32Array(workerData.gate);
      Atomics.store(gate, 1, 1);
      Atomics.notify(gate, 1);
      // Deliberately block this thread as synchronous native rendering would.
      Atomics.wait(gate, 0, 0);
    }
    this.last = { ranges: [...ranges], lengths: [...lengths], text, reuse };
    if (text.includes("FAIL_LAYOUT")) reject("LAYOUT_ERROR");
    const payload = new TextEncoder().encode(text);
    const edits = [];
    let offset = 0;
    for (let index = 0; index < lengths.length; index++) {
      const replacement = new TextDecoder("utf-8", { fatal: true }).decode(
        payload.subarray(offset, offset + lengths[index]),
      );
      offset += lengths[index];
      const start = ranges[index * 2], end = ranges[index * 2 + 1];
      for (const point of [start, end]) {
        if (point > this.source.length || (
          point > 0 && point < this.source.length &&
          /[\ud800-\udbff]/u.test(this.source[point - 1]) &&
          /[\udc00-\udfff]/u.test(this.source[point])
        )) reject("INVALID_SELECTION");
      }
      edits.push({ start, end, replacement, index });
    }
    edits.sort((a, b) => a.start - b.start || a.end - b.end || a.index - b.index);
    let cursor = 0, candidate = "";
    for (const edit of edits) {
      if (edit.start < cursor) reject("OVERLAPPING_EDITS");
      candidate += this.source.slice(cursor, edit.start) + edit.replacement;
      cursor = edit.end;
    }
    candidate += this.source.slice(cursor);
    if (candidate !== this.source) {
      this.source = candidate;
      this.revision = String(BigInt(this.revision) + 1n);
      this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
    }
  }
  // A test-only inspection channel through an existing binary read operation.
  fontBytes() { return new TextEncoder().encode(JSON.stringify({ calls: this.calls, last: this.last })); }
  snapshotJson(revision, layoutRevision, offset, limit) {
    if (revision !== this.revision) reject("STALE_REVISION");
    if (layoutRevision !== this.layoutRevision) reject("STALE_LAYOUT");
    const items = offset === 0 && limit > 0 ? [{ index: 0, kind: "text", text: this.source }] : [];
    return JSON.stringify({ schemaVersion: 1, revision, layoutRevision, total: 1, offset, nextOffset: null, items });
  }
  editUtf16() { throw new Error("sequential edit fallback must never run"); }
  replaceSource() { throw new Error("replacement fallback must never run"); }
  free() {}
}

const listeners = new Map();
const endpoint = {
  postMessage(message, transfer) {
    // Exercise strict creation and mutation acknowledgments without touching
    // the production dispatcher's checks or manufacturing successful renders.
    if (message.ok && message.value?.supportsEditBatches !== undefined) {
      if (workerData.capability === "invalid") message.value.supportsEditBatches = "yes";
      if (workerData.capability === "omitted") delete message.value.supportsEditBatches;
    } else if (message.ok && workerData.badAck && message.value?.revision) {
      message.value = { ...message.value, revision: "1" };
    }
    parentPort.postMessage(message, transfer);
  },
  addEventListener(kind, listener) {
    const wrapped = (data) => listener({ data });
    listeners.set(listener, wrapped);
    parentPort.on(kind, wrapped);
  },
  removeEventListener(kind, listener) {
    parentPort.off(kind, listeners.get(listener));
    listeners.delete(listener);
  },
};
installFlowWorker(endpoint, (source, { font: _font, ...layout }) =>
  createFlowAdapter(new NativeDouble(source), layout),
);
