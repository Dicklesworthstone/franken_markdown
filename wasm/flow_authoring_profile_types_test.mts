import { createFlowSession, type FlowAuthoringProfile } from "./flow.js";
import { createWorkerFlowSession } from "./flow-worker.js";

const direct = await createFlowSession("# Guide", { profile: "commonmark-gfm" });
const worker = await createWorkerFlowSession("# Guide", { profile: "gfm-plus" });
const profile: FlowAuthoringProfile | undefined = direct.profile;
const workerProfile: FlowAuthoringProfile | undefined = worker.profile;
const pageProfile: FlowAuthoringProfile | undefined = (await worker.snapshot()).profile;
const outputProfile: FlowAuthoringProfile | undefined =
  (await direct.exportDocument("epub", {}, direct.token)).profile;
void [profile, workerProfile, pageProfile, outputProfile];
// @ts-expect-error Profiles are exact canonical names.
createFlowSession("text", { profile: "gfm" });
// @ts-expect-error Changing dialect requires a new session.
direct.profile = "gfm-plus";
// @ts-expect-error A publication cannot override the preview's dialect.
worker.exportDocument("pdf", { profile: "commonmark-gfm" }, worker.token);
direct.dispose(); worker.dispose();
