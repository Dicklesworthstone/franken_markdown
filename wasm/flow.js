import { withAssetBatches } from "./flow_asset_batch.mjs";
import { withFlowExports } from "./flow_export.mjs";

import { withGlyphOutlines } from "./flow_outlines.mjs";
import {
  createFlowAdapter,
  FlowError,
  normalizeFlowError,
  validateCreation,
} from "./flow_session.mjs";
import { init, renderHtml, renderPdf } from "./franken_markdown.js";

export { FlowError, init };

/** Create a persistent, revision-fenced editor session. Editing is synchronous;
 * document export returns a Promise. Use a Worker to keep both off the UI thread.
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
      throw new FlowError(
        "UNSUPPORTED_WASM_PACKAGE",
        "This WASM artifact lacks FmdFlowSession; rebuild the package from matching source.",
      );
    }
    const p = prepared.layout;
    raw = new bindings.FmdFlowSession(
      prepared.source,
      prepared.font,
      p.viewportWidth,
      p.bodySize,
      p.codeSize,
      p.lineHeight,
    );
    const session = withGlyphOutlines(createFlowAdapter(raw, p), (id, glyphIds) => {
      if (typeof raw.glyphOutlinesJson !== "function") {
        throw new FlowError(
          "UNSUPPORTED_WASM_PACKAGE",
          "This WASM artifact lacks glyph outlines; rebuild from matching source.",
        );
      }
      return raw.glyphOutlinesJson(id, glyphIds);
    });
    return withFlowExports(
      withAssetBatches(session, () => raw),
      { html: renderHtml, pdf: renderPdf },
      prepared.font,
    );
  } catch (error) {
    if (raw) {
      try {
        raw.free();
      } catch {
        /* Preserve the original creation failure. */
      }
    }
    throw normalizeFlowError(error);
  }
}
