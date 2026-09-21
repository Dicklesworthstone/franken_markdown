import type { FlowSession, FlowAssetResult, FlowToken } from "./flow.js";
import type { WorkerFlowSession } from "./flow-worker.js";
declare const direct: FlowSession;
declare const worker: WorkerFlowSession;
const values: readonly FlowAssetResult[] = [
  { requestId: 1n, generation: "1", width: 20, height: 10 },
  { requestId: "2", generation: 1n, width: 40, height: 20, bytes: new Uint8Array([1, 2]) }
];
const directSupport: boolean = direct.supportsAssetBatches;
const remoteSupport: boolean = worker.supportsAssetBatches;
const directToken: FlowToken = direct.provideAssets(values);
const remoteToken: Promise<FlowToken> = worker.provideAssets(values, { signal: new AbortController().signal });
void [directSupport, remoteSupport, directToken, remoteToken];
// @ts-expect-error identities must never pass through Number
direct.provideAssets([{ requestId: 1, generation: "1", width: 20, height: 10 }]);
// @ts-expect-error direct completion is synchronous
const wrong: Promise<FlowToken> = direct.provideAssets([]);
// @ts-expect-error arbitrary transfer control is not exposed
worker.provideAssets(values, { transfer: [] });
