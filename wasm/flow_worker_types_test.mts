import { createWorkerFlowSession, type FlowToken, type FlowSnapshot, type FlowWorkerEndpoint } from "@franken-suite/franken-markdown/flow-worker";
const worker: FlowWorkerEndpoint = new Worker(new URL("./flow_worker.js", import.meta.url), { type: "module" });
worker.terminate();
const session = await createWorkerFlowSession("# Guide", { viewportWidth: 360 }, { maxPendingOperations: 8 });
const control = { signal: new AbortController().signal, timeoutMs: 10000 };
const token: FlowToken = await session.edit(0, 0, "Before\n\n", { expectedRevision: session.revision }, control);
await session.reflow({ viewportWidth: 720 }, token);
const page: FlowSnapshot = await session.snapshot({ glyphs: true });
await session.hitTest(10, 20, page);
for await (const next of session.pages({ limit: 16 }, control)) { next.items.forEach(item => item.bounds); }
const source: string = await session.getSource();
void source;
// @ts-expect-error Worker operations are asynchronous, not synchronous snapshots.
const wrongPage: FlowSnapshot = session.snapshot();
// @ts-expect-error Numbers are not lossless revision identities.
await session.edit(0, 0, "x", { expectedRevision: 1 });
// @ts-expect-error There is no speculative synchronous source property.
void session.source;
// @ts-expect-error A stale-default token is never implicit for mutations.
await session.reflow({ viewportWidth: 400 });
session.dispose();
