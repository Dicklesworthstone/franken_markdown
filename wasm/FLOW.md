# Persistent flow/editor sessions

The `./flow` entry point connects JavaScript to the Rust transactional editor,
measured display lists, font-run cache, hit testing and asset lifecycle. It uses
the same initialized WASM instance as the HTML/PDF APIs. Rebuild the generated
`pkg/` alongside these wrappers: an older binary is rejected with
`UNSUPPORTED_WASM_PACKAGE`, not silently substituted with estimated layout.

```js
import { createFlowSession } from "@franken-suite/franken-markdown/flow";

const editor = await createFlowSession(
  "# Guide\n\n**Bold** and [a link](#guide).",
  { font: "sans", viewportWidth: 360 }
);
try {
  const before = editor.token;
  // These offsets are UTF-16 source indices, just like textarea.selectionStart.
  editor.edit(0, 0, "Introduction\n\n", {
    expectedRevision: before.revision
  });
  editor.reflow({ viewportWidth: 720 }, editor.token);

  // A page contains real glyph/cluster data, not HTML or a screenshot.
  const page = editor.snapshot({ limit: 512, glyphs: true });
  for (const item of page.items) {
    if (item.kind === "text" && item.fontRun) {
      // Install precisely the face identified by each actual glyph's fontId.
      const fontBytes = editor.fontBytes(item.fontRun.fontId);
      // Host drawing code consumes item.bounds and item.fontRun.glyphs.
    }
  }
  const hit = editor.hitTest(20, 30, page);
  // hit.linkTarget is data for the host's authorization/navigation layer.
  console.log(hit);
} finally {
  editor.dispose();
}
```

## Coordinates and optimistic transactions

`revision`, `layoutRevision`, request IDs and font IDs are **decimal strings**;
input identities also accept `bigint`. JavaScript `Number` identities are
rejected so values above 2^53 cannot round to another document or resource.

`edit` uses UTF-16 source indices. `editBytes` uses UTF-8 source bytes. Both
require `expectedRevision`; invalid boundaries, unpaired JS surrogates and
mid-surrogate selections are rejected. Failed edits keep the previous source,
asset state, display and revisions. Identical source replacements are no-ops.

`hitTest` and `selectText` require the token of the **display actually shown**.
A resize or asset completion changes `layoutRevision`, so an old click cannot
be interpreted against new geometry. Their text offsets are local to one
rendered text fragment. `enclosingSourceSpan` is a top-level Markdown envelope,
not an invented exact inline mapping. `copySource(startByte, endByte, revision)`
and `selectText(...).text` are deliberately separate copy operations.

## Paged drawing and accessibility output

`snapshot`, `readingOrder` and `pendingAssets` return schema-version-one pages
with `offset`, `total`, `nextOffset`, `revision` and `layoutRevision`. Drawing
items include text/glyph runs, image references, vector decorations, filled or
stroked paths and heading or link anchors. Reading nodes retain table row/cell roles and hierarchy.

Display and inline math are typeset by the shared `fmd_math` engine into bounded `path`
primitives, including real font outlines, fraction bars, roots and formula
colors. Commands are absolute M/L/Q/C/Z contours with nonzero fill; `fill` is
either null (use `colorRole`) or explicit RGBA in 0..1. The primitive carries
the original enclosing source span. Each path is limited to 65,536 commands,
and the shared math/diagram display to 262,144 path commands. Canvas details are in `CANVAS.md`.
Text lines with measured font metrics carry `baselineY` on text items, including
mixed text/math, optional CJK lines and diagram labels. Canvas validates and uses that native
coordinate rather than inferring placement from the expanded line height.
Absent/null baselines retain the older text presentation policy.
Inline fallback text exposes a bounded, nonempty `mathFallback` reason and keeps
the complete literal source. The `math` reading leaf retains block TeX, while
`inlineRuns[].math` identifies exact TeX ranges as `"inline"` or `"display"`
within continuous prose. Search and copying preserve those source characters
without inventing selectable formula glyph clusters. Rebuild the generated
package and update the consumer together: schema 1's additive math capabilities
require both sides. Older consumers may ignore inline metadata and explicit
baselines; they do not provide mixed-line math fidelity.

Pages default to 512 entries, accept at most 2048, and serialize at most 16 MiB.
Use a smaller page or `glyphs: false` when the serializer reports
`SNAPSHOT_TOO_LARGE`. `editor.pages({ limit, glyphs })` iterates drawing pages
under one captured token; an intervening edit or resize makes it throw instead
of mixing snapshots. Do not collect the entire iterator for a virtualized view.

## Explicit image ownership

The renderer never fetches images or follows links. Hosts validate destinations,
permissions, content and decoding before submitting a pending request:

```js
const pending = editor.pendingAssets({ limit: 2048 });
for (const request of pending.requests) {
  const decoded = await loadAuthorizedImage(request.url); // application policy
  editor.provideAsset({
    requestId: request.id,
    generation: request.generation,
    width: decoded.width,
    height: decoded.height,
    bytes: decoded.bytes // optional Uint8Array; omit for dimension-only results
  });
}
```

A source edit during that `await` makes the old result stale. Resizing does not
invalidate other outstanding requests. Invalid results remain pending. Reload
external resources with `reloadAssets(editor.revision)` when bytes, base URI or
authorization changes. Source edits discard assets by default; `reuseAssets:
true` explicitly attests that the host has verified the resource context and
bytes are unchanged. Retrieval through `assetBytes(id, revision)` is generation
checked; `null` means a known dimension-only result. Bytes returned to JS are
owned copies. Stop presenting old snapshots immediately when authorization is
revoked, including when a refresh transaction fails.

## Native Mermaid diagrams

Every `wasm-flow` build renders supported flowcharts, sequence diagrams and pie
charts as native paths with measured bundled-font labels. Dashed and thick
strokes, arrowheads, curves, node shapes and label colors survive the shared
compiler. The complete measured scene fits the available lane and refits after
reflow. It keeps the compiler's light colors on an explicit white backdrop.
Canvas consumes native paths and glyph outlines without browser text shaping,
SVG execution, images or resource fetching.

Path items may now carry an optional `stroke` record with RGBA color, width,
dash lengths/offset, cap, join and miter limit. All seven fields are required
when a stroke exists. With a stroke, `fill: null` means stroke-only and admits
nonempty open subpaths; explicit RGBA fill paints both, with closed contours.
Absent/null stroke retains the existing closed-fill contract. Optional text
`fill` supplies actual native label RGBA; omit it for the semantic palette.
Bounds include conservative stroke extents. Update the consumer alongside the
producer to preserve these additive schema-one capabilities.

The `diagram` reading leaf retains complete code source for supported and
unsupported diagrams. The reader presents a labeled region with literal
`pre`/`code` text, preserving indentation and final-newline selection/Copy.
Unsupported syntax or scene features show a visible fallback reason and source;
ASCII/ditaa keeps the ordinary code-block role. Source/scene limits and active
font coverage still apply, and failed transactions retain the previous state.
See [the complete Flow diagram contract](../docs/FLOW_DIAGRAMS.md) for examples,
the supported syntax, limits and actual native/WASM browser checks.

## Optional horizontal CJK text

Build the native/WASM producer with `cjk-face` to include the existing bundled
Simplified Chinese fallback font in Flow. For a dedicated browser Flow build,
use `--no-default-features --features wasm-flow,cjk-face` when building the
`wasm32-unknown-unknown` library, then generate matching wasm-bindgen bindings.
This is a compile-time capability; there is no JavaScript option that downloads
or activates a missing font. The default lean build keeps its smaller font set
and returns `LAYOUT_ERROR` for CJK text it cannot shape. A failed edit retains
the last successful source, revisions and display.

The enabled profile admits mapped BMP Han, precomposed hiragana/katakana and
horizontal CJK punctuation. Latin text keeps its selected body, emphasis or code
face; each CJK cluster records the actual bundled fallback font ID and measured
advance. Canvas requests that font's real outlines through `glyphOutlines`, with
no browser font selection or network request. The shared layout uses real font
extents for line heights, baselines, wrapping and list-marker placement. CJK
text works in headings, paragraphs, emphasis, links, lists, quotes, table cells,
code and beside inline mathematics. Source editing, fragment UTF-16 selections,
reading search and native Copy retain the original characters; source-byte Copy
also retains a BOM and original CRLF line endings.

The fallback is one regular Simplified Chinese face, including when used inside
bold, italic or code spans. It does not synthesize CJK weights, language-specific
glyph variants or vertical text. Missing glyphs, supplementary Han, Hangul,
decomposed kana/unsupported combining sequences, CJK variation selectors, bidi and
joined emoji still require a suitable host shaper and are refused rather than replaced
with missing-glyph boxes or silently dropped characters.

## Optional monochrome emoji

Build with `--no-default-features --features wasm-flow,emoji-face` to draw mapped
standalone emoji such as `🚀`, `✅`, `📦` and `🎉` with the existing bundled Noto
Emoji outlines. The feature also admits standardized presentation sequences
such as `⚠️` and `©️`: VS16 selects the emoji face, while VS15 requires an actual
glyph in the chosen text/symbol face. A base and its selector share one complete
UTF-8/UTF-16 cluster, so wrapping and hit testing cannot split them. The source
and semantic reader retain both characters when copying or editing.

Emoji works in ordinary styled text, headings, lists, quotes, tables, code and
beside mathematics. Measured font extents determine line height and baseline.
Canvas paints actual glyph paths without downloading images or using a system
emoji font. Add `cjk-face` independently when both repertoires are needed.
The regular monochrome face does not synthesize colored, bold or italic emoji.

The lean profile still refuses uncovered emoji. ZWJ sequences, skin tones,
flags, keycaps, tag sequences, arbitrary presentation selectors and missing
glyphs remain `LAYOUT_ERROR` failures that preserve the previous document.
See [the precise font and source contract](../docs/FLOW_EMOJI_TEXT.md).

## Limits and scope

The JS facade checks a 4 MiB UTF-8 source limit and an 8 MiB individual asset
limit before WASM ingress. Rust additionally bounds source lines, AST nodes,
projection strings, request counts, 32 MiB aggregate assets, and shaping work.
The JavaScript facade rejects shared asset buffers; copy them to owned bytes.

After asynchronous creation, all operations are synchronous. Use a Web Worker
for off-main-thread editing; this API does not silently create one. Edits still
reparse complete bounded snapshots. The bundled adapter is the existing simple
left-to-right profile, not full multilingual/bidirectional shaping. Unsupported
scripts or sequences produce a structured `FlowError` while preserving the
last successful document. Use Rust `FlowSession` with a host shaper for broader
coverage. This entry point exposes drawing data, not a built-in Canvas/GPU UI.

`dispose()` is idempotent and releases the native session and retained cache.
All later operations except checking `disposed` fail with `SESSION_DISPOSED`.
Snapshot mutation in JavaScript cannot modify the Rust session.

## Executable checks

`node --test wasm/flow_session.test.mjs` exercises the JS transport contract with
an explicit test double; it is not proof of the Rust renderer. The separate
`wasm/flow_smoke.mjs` loads the **generated** class and checks rich layout, edits,
stale selections, image reflow, font data and fresh-session equivalence. It is
invoked by both `scripts/check-wasm-package.sh` and the DSR quality route
`scripts/dsr-wasm-package.sh`.
Type declarations are checked with `tsc --noEmit --strict --target ES2022
--module NodeNext --moduleResolution NodeNext wasm/flow_types_test.mts`.

`tests/flow_cjk.rs` checks the optional font's actual cmap, outlines, advances,
metrics, source/caret ranges, wrapping and bounded-layout continuation, with a
separate feature-off refusal test. Set `FMD_FLOW_CJK_FIXTURE` to emit unchanged
native browser pages for `wasm/tests/run_flow_cjk.py`. That Chromium gate accepts
actual enabled and lean-off generated packages and an actual pre-change package
for ordinary Latin pixel comparison. It checks native/WASM geometry and text,
real CJK ink, exact selection/source bytes, edits/reflow, safe links and refused
unsupported sequences without a substitute renderer or browser shaping engine.

`tests/flow_diagram.rs` exports an actual native fixture when
`FMD_FLOW_DIAGRAM_FIXTURE` is set. The
`wasm/tests/flow_diagram.browser.mjs` suite runs through
`wasm/tests/run_flow_generated.mjs` against matching generated bindings and real
Chromium. It checks native/WASM scene and pixel parity, actual strokes and
dashes, bundled label outlines, exact source Copy, fallback and edit/reflow
transactions. It refuses browser text shaping and resource fetching; see
[reproduction commands](../docs/FLOW_DIAGRAMS.md#executable-checks).
