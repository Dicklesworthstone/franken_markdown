// Real worker transport with an explicit native verifier/session double.
import { parentPort, workerData } from "node:worker_threads";
import { installFlowWorker } from "../flow_worker_session.mjs";
import { createVerificationFixture } from "./flow_pdf_verification_fixture.mjs";

const listeners = new Map();
const endpoint = {
  addEventListener(kind, callback) {
    const listener = data => callback({ data }); listeners.set(callback, listener);
    parentPort.on(kind, listener);
  },
  removeEventListener(kind, callback) {
    parentPort.off(kind, listeners.get(callback)); listeners.delete(callback);
  },
  postMessage(message, transfer = []) {
    if (message.ok && message.value?.supportsPdfVerification !== undefined && workerData?.omitCapability)
      delete message.value.supportsPdfVerification;
    if (message.ok && message.value?.format === "pdf-verification" && workerData?.corrupt) {
      const value = message.value, mode = workerData.corrupt;
      if (mode === "token") value.revision = message.state.token.revision = "9007199254741993";
      if (mode === "font") value.font = "sans";
      if (mode === "profile") value.profile = "commonmark-gfm";
      if (mode === "json") value.json = new TextEncoder().encode("{");
      if (mode === "budget") value.json = new Uint8Array(4096);
      if (mode === "report") {
        const report = JSON.parse(new TextDecoder().decode(value.json));
        report.page_count++; value.json = new TextEncoder().encode(JSON.stringify(report));
      }
      transfer = [value.json.buffer];
    }
    parentPort.postMessage(message, transfer);
  },
};
installFlowWorker(endpoint, (source, options) => createVerificationFixture(workerData, source, options).api);
