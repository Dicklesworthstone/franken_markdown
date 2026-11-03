// Explicit-runtime standalone export; does not fetch or implicitly initialize
// the main renderer package. The exported file carries its own matching engine.
export {createNativeWorkspaceExporter, NATIVE_WORKSPACE_LIMITS} from './interactive_export.mjs';
