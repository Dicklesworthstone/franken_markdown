import type { FlowSession, FlowViewportOptions, FlowViewportPage } from "./flow.js";
import type { CanvasFlowSession } from "./flow-canvas.js";
import type { WorkerFlowSession } from "./flow-worker.js";

declare const direct: FlowSession;
declare const worker: WorkerFlowSession;
declare const legacy: Pick<
  CanvasFlowSession,
  "disposed" | "token" | "layoutOptions" | "snapshot" | "glyphOutlines" | "hitTest"
>;
const clients: CanvasFlowSession[] = [direct, worker, legacy];
const query: FlowViewportOptions = {
  viewport: { x: 0, y: 0, width: 600, height: 400 },
  token: direct.token,
  glyphs: true,
};
const immediate: FlowViewportPage = direct.viewport(query);
const pending: Promise<FlowViewportPage> = worker.viewport(query, { timeoutMs: 5000 });
const capability: boolean = direct.supportsViewport && worker.supportsViewport;
void [clients, immediate, pending, capability];
for (const item of immediate.items) {
  const x: number = item.effectiveClip.x;
  // @ts-expect-error viewport geometry is immutable consumer data
  item.effectiveClip.x = 5;
  // @ts-expect-error clip commands are folded into effectiveClip, not returned
  if (item.kind === "clip") throw new Error("unreachable");
  if (item.kind === "text") {
    const text: string = item.text;
    // @ts-expect-error source fragments are not mutated through viewport output
    item.text = text + "changed";
  }
  void x;
}
// @ts-expect-error cursors use original numeric item indices, not revision IDs
direct.viewport({ ...query, afterIndex: "9000" });
// @ts-expect-error native u64 revision IDs are not Numbers
direct.viewport({ ...query, token: { revision: 1, layoutRevision: "2" } });
// @ts-expect-error a viewport rectangle is required
worker.viewport({ limit: 256 });
// @ts-expect-error viewport pages do not expose dense snapshot offsets
immediate.nextOffset;
