import { createNativeWorkspaceExporter } from "../native_workspace.js";
import { loadPackagedWorkspaceRuntime } from "./demo_workspace_runtime.mjs";
import { installDemoWorkspaceWorker } from "./demo_workspace_worker.mjs";

installDemoWorkspaceWorker(globalThis, async () =>
  createNativeWorkspaceExporter(await loadPackagedWorkspaceRuntime()));
