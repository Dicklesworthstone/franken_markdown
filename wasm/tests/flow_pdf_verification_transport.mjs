import { Worker } from "node:worker_threads";

export function verificationEndpoint(data) {
  const thread = new Worker(new URL("./flow_pdf_verification_worker_fixture.mjs", import.meta.url), { workerData: data });
  const listeners = new Map(), sent = [];
  return {
    sent,
    postMessage(message, transfer) { sent.push(message.method); thread.postMessage(message, transfer); },
    terminate() { return thread.terminate(); },
    addEventListener(kind, callback) {
      const listener = value => callback(kind === "message" ? { data: value } : value);
      listeners.set(callback, listener); thread.on(kind, listener);
    },
    removeEventListener(kind, callback) {
      thread.off(kind, listeners.get(callback)); listeners.delete(callback);
    },
  };
}
