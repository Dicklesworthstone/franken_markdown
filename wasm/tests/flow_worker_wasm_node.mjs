// Test-only Node bridge for the PRODUCTION browser worker entry, not a renderer
// double. Loads the generated package and real WASM bytes supplied by the gate.
import { parentPort, workerData } from "node:worker_threads";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import path from "node:path";
const waiting = [];
const listeners = new Set();
parentPort.on("message", data => {
  if (listeners.size === 0) waiting.push(data);
  else for (const listener of listeners) listener({ data });
});
globalThis.postMessage = (message, transfer) => parentPort.postMessage(message, transfer);
globalThis.addEventListener = (kind, listener) => {
  if (kind !== "message") throw new Error("unexpected worker event registration");
  listeners.add(listener);
  const queued = waiting.splice(0);
  for (const data of queued) queueMicrotask(() => { if (listeners.has(listener)) listener({ data }); });
};
globalThis.removeEventListener = (_kind, listener) => listeners.delete(listener);
const entry = name => pathToFileURL(path.join(workerData.packageDir, name)).href;
const { init } = await import(entry("franken_markdown.js"));
await init(new Uint8Array(await readFile(workerData.wasmPath)));
await import(entry("flow_worker.js"));
