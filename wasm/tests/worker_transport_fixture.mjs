// Deliberately synthetic operations in a REAL worker thread. This exercises
// transport isolation/cancellation, not the Markdown engine or WASM rendering.
import { parentPort } from "node:worker_threads";
import { setTimeout as sleep } from "node:timers/promises";
import { FlowWorkerError, serveOwnedWorker, WORKER_PROTOCOL } from "../worker_transport.mjs";
const listeners = new Map();
const endpoint = {
  postMessage: (message, transfer) => parentPort.postMessage(message, transfer),
  addEventListener(kind, listener) {
    const wrapped = data => listener({ data });
    listeners.set(listener, wrapped);
    parentPort.on(kind, wrapped);
  },
  removeEventListener(kind, listener) { parentPort.off(kind, listeners.get(listener)); }
};
let count = 0;
serveOwnedWorker(endpoint, async (method, args) => {
  let value;
  let transfer = [];
  switch (method) {
    case "increment": count += args[0] ?? 1; value = count; break;
    case "count": value = count; break;
    case "delay": await sleep(args[0]); value = count; break;
    case "block": Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, args[0]); count++; value = count; break;
    case "bytes": value = args[0]; value[0] = 99; transfer = [value.buffer]; break;
    case "knownError": throw new FlowWorkerError("STALE_REVISION", "synthetic revision conflict");
    case "crash": setTimeout(() => { throw new Error("synthetic worker crash"); }, 0); await sleep(10000); break;
    case "badReply": parentPort.postMessage({ protocol: WORKER_PROTOCOL, id: 999, ok: true }); await sleep(10000); break;
    case "badValue": value = () => {}; break;
    case "untypedError": throw new Error("synthetic unknown operation failure");
    default: throw new FlowWorkerError("UNKNOWN_METHOD", "synthetic method is not supported");
  }
  return { value, state: { count }, transfer };
});
