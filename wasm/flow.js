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
import * as publicationRenderers from "./franken_markdown.js";

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
    const args = [
      prepared.source,
      prepared.font,
      p.viewportWidth,
      p.bodySize,
      p.codeSize,
      p.lineHeight,
    ];
    if (prepared.profile === undefined) {
      raw = new bindings.FmdFlowSession(...args);
    } else {
      if (typeof bindings.FmdFlowSession.withProfile !== "function"
          || bindings.FmdFlowSession.withProfile.length < 7) {
        throw new FlowError("UNSUPPORTED_WASM_PACKAGE",
          "Flow authoring profiles require FmdFlowSession.withProfile from matching source.");
      }
      raw = bindings.FmdFlowSession.withProfile(...args, prepared.profile);
    }
    const session = withGlyphOutlines(createFlowAdapter(raw, p, prepared.profile), (id, glyphIds) => {
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
      {
        html: renderHtml, pdf: renderPdf,
        epub: publicationRenderers.renderEpub, svg: publicationRenderers.renderSvg,
        verifyPdf: publicationRenderers.verifyPdf,
      },
      prepared.font,
      (expected, offset, limit) => {
        if (typeof raw.assetInventoryJson !== "function") {
          throw new FlowError(
            "UNSUPPORTED_WASM_PACKAGE",
            "Complete image export requires assetInventoryJson; rebuild the WASM package from matching source. Editing remains available.",
          );
        }
        let json;
        try {
          json = raw.assetInventoryJson(expected.revision, expected.layoutRevision, offset, limit);
        } catch (error) {
          throw normalizeFlowError(error);
        }
        try {
          if (typeof json !== "string") throw new Error("expected JSON text");
          return JSON.parse(json);
        } catch (error) {
          throw new FlowError("INVALID_WASM_RESPONSE", "invalid native export image inventory JSON", { cause: error });
        }
      },
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
