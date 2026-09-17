// Importing this entrypoint starts no worker and loads no WASM on the UI thread.
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";
import { fields } from "./flow_worker_protocol.mjs";
import { FlowWorkerError } from "./worker_transport.mjs";
export { FlowWorkerError } from "./worker_transport.mjs";

export async function createWorkerFlowSession(source, options = {}, workerOptions = {}) {
  fields(workerOptions, ["workerFactory", "signal", "startupTimeoutMs", "timeoutMs",
    "maxPendingOperations", "maxPendingBytes"], "worker options");
  const { workerFactory, ...runtime } = workerOptions;
  const factory = workerFactory ?? (() => {
    if (typeof Worker !== "function") {
      throw new FlowWorkerError("WORKER_UNAVAILABLE", "this environment needs an explicit workerFactory");
    }
    return new Worker(new URL("./flow_worker.js", import.meta.url), { type: "module", name: "franken-markdown-flow" });
  });
  return createWorkerFlowSessionWith(factory, source, options, runtime);
}
