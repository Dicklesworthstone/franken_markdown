// Real Node worker transport, explicit native double. No Rust/WASM claim.
import { parentPort, workerData } from "node:worker_threads";
import { installFlowWorker } from "../flow_worker_session.mjs";
import { facade, NativeDouble } from "./flow_asset_batch_fixture.mjs";
const listeners = new Map();
const endpoint = {
  postMessage(message, transfer) { parentPort.postMessage(message, transfer); },
  addEventListener(type, listener) {
    const handler = data => listener({ data }); listeners.set(listener, handler);
    parentPort.on(type, handler);
  },
  removeEventListener(type, listener) { parentPort.off(type, listeners.get(listener)); listeners.delete(listener); }
};
installFlowWorker(endpoint, async source => {
  const raw = new NativeDouble(source);
  if (workerData?.legacy) raw.provideAssetsPacked = undefined;
  const { session } = facade(raw);
  // Replace fixture descriptors before constructing the frozen facade.
  const descriptors = Object.getOwnPropertyDescriptors(session);
  if (workerData?.slowSnapshot) descriptors.snapshot = { enumerable: true, async value(options) {
    await new Promise(resolve => setTimeout(resolve, 40)); return session.snapshot(options);
  } };
  if (workerData?.badAck) descriptors.provideAssets = { enumerable: true, value(results) {
    session.provideAssets(results); return { revision: "1", layoutRevision: "999" };
  } };
  return Object.freeze(Object.create(Object.getPrototypeOf(session), descriptors));
});
