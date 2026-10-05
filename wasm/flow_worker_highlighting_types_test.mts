import type { WorkerFlowSession, FlowToken, FlowWorkerOptions } from "./flow-worker.js";

declare const session: WorkerFlowSession;
const supported: boolean = session.supportsCodeHighlighting;
const enabled: boolean = session.codeHighlighting;
const changed: Promise<FlowToken> = session.setCodeHighlighting(true, session.token, {
  signal: new AbortController().signal,
  timeoutMs: 5000,
});
// @ts-expect-error syntax mode is a boolean, not an option string
session.setCodeHighlighting("true", session.token);
// @ts-expect-error a source/layout token is mandatory, even for a same-mode request
session.setCodeHighlighting(false);
// @ts-expect-error mode reflects acknowledgments and cannot be assigned locally
session.codeHighlighting = true;
// @ts-expect-error controls use the same typed cancellation contract as other operations
session.setCodeHighlighting(false, session.token, { signal: true });
void [supported, enabled, changed];

const startup: FlowWorkerOptions = { initialCodeHighlighting: true, startupTimeoutMs: 10000 };
// @ts-expect-error the startup mode is also strictly boolean
const invalidStartup: FlowWorkerOptions = { initialCodeHighlighting: "true" };
void [startup, invalidStartup];
