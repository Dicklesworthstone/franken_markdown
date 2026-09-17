import { init } from "./franken_markdown.js";
import { createFlowAdapter, FlowError, normalizeFlowError, validateCreation } from "./flow_session.mjs";

export { init, FlowError };

/** Create a persistent, synchronous-after-init, revision-fenced editor session.
 * No images are fetched and no links are opened; hosts authorize those actions.
 * Call dispose() when finished to release the native session and font-run cache.
 */
export async function createFlowSession(markdown, options = {}) {
  const prepared = validateCreation(markdown, options);
  let raw;
  try {
    await init();
    const bindings = await import("./pkg/franken_markdown.js");
    if (typeof bindings.FmdFlowSession !== "function") {
      throw new FlowError("UNSUPPORTED_WASM_PACKAGE", "This WASM artifact lacks FmdFlowSession; rebuild the package from matching source.");
    }
    const p = prepared.layout;
    raw = new bindings.FmdFlowSession(prepared.source, prepared.font, p.viewportWidth, p.bodySize, p.codeSize, p.lineHeight);
    return createFlowAdapter(raw, p);
  } catch (error) {
    if (raw) { try { raw.free(); } catch { /* Preserve the original creation failure. */ } }
    throw normalizeFlowError(error);
  }
}
