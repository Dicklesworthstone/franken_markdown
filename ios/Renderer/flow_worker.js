// Dedicated browser module-worker entry. The fixed import loads WASM only here,
// and only when the first session is created. No script URL comes from a message.
import { installFlowWorker } from "./flow_worker_session.mjs";

installFlowWorker(globalThis, async (source, options) => {
  const { createFlowSession } = await import("./flow.js");
  return createFlowSession(source, options);
});
