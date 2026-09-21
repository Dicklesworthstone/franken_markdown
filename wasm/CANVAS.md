# Measured flow to Canvas

`./flow-canvas` draws the flow engine's actual glyph IDs through the existing
Rust TrueType outline decoder. It does not use `fillText`, `measureText`, browser
font selection, another Markdown parser, or another shaping engine. Kerning,
ligatures, source clusters, inline styles, and fallback font identities come
from the existing display list. Both `./flow` and `./flow-worker` expose the
same `glyphOutlines(fontId, glyphIds)` contract; the worker form is asynchronous.

Rebuild the matching generated WASM package before using this API. An older
binary without `glyphOutlinesJson` is rejected, not silently approximated.

```js
import { createWorkerFlowSession } from "@franken-suite/franken-markdown/flow-worker";
import { FlowCanvasRenderer } from "@franken-suite/franken-markdown/flow-canvas";

const editor = await createWorkerFlowSession(
  "# Guide\n\n**Bold**, *italic*, `code`, and [a link](#guide).",
  { viewportWidth: 640 }
);
const canvas = document.querySelector("canvas");
canvas.style.width = "640px";
canvas.style.height = "480px";
const painter = new FlowCanvasRenderer(canvas);
try {
  const frame = await painter.render(editor, {
    width: 640, height: 480, scrollY: 0, pixelRatio: devicePixelRatio,
    token: editor.token
  });
  // Pointer coordinates are logical viewport pixels, not device pixels.
  const box = canvas.getBoundingClientRect();
  const localX = (pointer.clientX - box.left) * frame.width / box.width;
  const localY = (pointer.clientY - box.top) * frame.height / box.height;
  const hit = await painter.hitTest(localX, localY);
  // The application decides whether/how to activate hit.linkTarget.
} finally {
  painter.dispose(); // clears pixels and paths; does not own the editor
  editor.dispose(); // terminates the worker and its native session
}
```

The example's pointer variables belong in a pointer event handler. The renderer
never installs global listeners or changes CSS dimensions. HTMLCanvasElement
and OffscreenCanvas are supported. `canvasFactory(width, height)` can supply a
fresh staging surface; it must not return the target or a previously painted
surface. This is a presentation backend, not a complete editor widget.

## Geometry and text fidelity

Font outlines are M/L/Q/Z contours in baseline-relative, y-up font design units
with nonzero winding. Glyph positions use the display run's cluster geometry,
advances and offsets, never an outline's nominal advance. Every actual glyph
font ID is resolved independently, so a fallback face cannot be drawn with the
primary face's metrics. No fonts are installed in the DOM.

The current flow contract supplies line boxes but no explicit baseline. The
Canvas backend therefore applies an explicit presentation policy: fragments
sharing the same line top and height share a baseline derived from the largest
actual-face ascent and descent, with remaining leading split above and below.
Fragments across snapshot-page boundaries share that baseline. Changing the
horizontal scroll offset does not change the baseline. Rasterization quality
and pixel antialiasing remain browser/platform dependent; this is not a claim
of identical pixels across all operating systems.

The target's backing store is the viewport only, not the entire document. The
renderer reads 256-item pages under one captured source/layout token, retains
visible-Y drawing items, and applies scroll and explicit device-pixel transforms.
When `session.supportsViewport === true`, it uses the existing native spatial
index through `viewport({ viewport, afterIndex, limit: 256, glyphs: true, token })`.
Offscreen drawing pages are not serialized or transferred to JavaScript. The
native index is built lazily once per display list, invalidated by edits/reflow,
and can still visit many entries for heavily overlapping geometry. There is no
constant-time scrolling claim and no duplicate JavaScript document index.

Spatial pages retain original item IDs and include effective clips for omitted
ancestors. Their text-line context includes horizontally offscreen and fully
clipped text at visible Y, preserving shared baselines across pages and horizontal
scrolls. The f32 query rectangle rounds outward; actual ink and hit testing retain
the original logical viewport. All pages must agree on token, total inventory,
bounds and query. A sparse cursor is never interpreted as a visible-item count.

Older/custom sessions that do not advertise viewport support keep the bounded
full-snapshot path. An advertised but missing/failing viewport implementation is
an error, never a silent full-scan retry. No new option or opt-in is needed in
the existing live preview. To change wrapping, call/await `editor.reflow(...)`
first; changing paint width alone only changes the viewport.

Clipping covers exactly the declared number of subsequent primitives and
survives page boundaries, including nested clips. Tables, blockquote accents,
checkboxes, rules, and strikethrough decorations are drawn. Heading/link anchors
are interaction data, not ink. Diagram primitives with only classification and
bounds are refused rather than rendered as invented path geometry. The backend
currently accepts the bundled simple-LTR profile, not RTL or vertical text.

## Atomic preparation and cancellation

All pages, paths and optional images are prepared before a fresh staging surface
is drawn. Only after a final revision check is that surface copied to the target.
Invalid data, stale responses, budget exhaustion, and asynchronous image/outline
failures leave the last successfully published frame and pixels intact. A browser
allocation or final target-blit failure cannot be rolled back; its frame token is
invalidated rather than falsely identifying partially updated pixels as valid.

A newer paint supersedes an older pending paint. An AbortSignal cancels waiting
promptly, but is deliberately NOT forwarded to the worker operation: canceling
an in-flight worker request would destroy the editing session. Late replies are
observed and discarded, not published and not leaked as unhandled rejections.
The method's returned frame identifies precisely the painted source and layout.
`hitTest` maps local logical coordinates using that frame and rejects stale
geometry. Call `clear()` immediately to stop presenting revoked resources;
re-render failure must not be treated as authorization to keep showing them.

## Explicit image and accessibility ownership

Pending or unresolved images get a geometric placeholder. For resolved image
descriptors, `resolveImage(info, token)` may return an already-authorized decoded
CanvasImageSource, asynchronously if needed. No URL is fetched and no asset bytes
are decoded automatically. The host validates destination, credentials, resource
limits and decoded content before supplying it. The renderer does not cache image
resources or call `close()` on caller-owned ImageBitmaps. A cross-origin image can
taint the canvas under normal browser rules; resource/CORS policy stays with the
host. Make any cache key include the document/resource authorization context.

For a ready-to-use bounded PNG/JPEG ownership pipeline, use `./flow-assets`
with an explicit host loading callback; see `ASSETS.md`. It feeds actual image
dimensions back into layout and lends authorized bitmaps through `resolveImage`.
The Canvas renderer itself still never fetches or decodes an image implicitly.

A `selection: { revision, layoutRevision, rectangles }` overlay must match the
painted layout. Rectangles are painted behind text, not converted into guessed
source ranges. Text copying remains a separate session selection operation.
Canvas pixels are not an accessibility tree. Use the session's `readingOrder`
pages to build a host accessibility/semantic surface; use its text-selection API
for logical copying. This backend does not advertise native DOM text selection
or install keyboard navigation on its own.

## Resource bounds

The default pixel cap is 16,777,216 per surface (target and staging coexist).
Logical dimensions are at most 16,384; device-pixel ratio is explicitly supplied
in (0, 8], with each resulting pixel dimension also at most 16,384. Default work
caps are 500,000 scanned items, 10,000 retained visible-Y items, 50,000 glyphs,
1,048,576 distinct per-frame path commands and 2,000,000 executed commands.

For indexed paints, `maxScannedItems` charges the sum of native `visitedEntries`
across pages, rather than rejecting a large document for its offscreen inventory.
Legacy paints still admit/scan the complete inventory under that same limit.
Frames expose `queryMode`, `totalItems`, `scannedItems` and `receivedItems` to
separate full document size, native candidate visits and transferred primitives.
Native visits exclude initial index construction and tree traversal overhead;
these counts are not timing benchmarks or a bound on native index-build work.

The LRU outline cache retains at most 2,048 glyphs and 262,144 path commands;
font metrics have a separate 32-entry cap. Cached paths contain no source spans,
absolute coordinates, document content, link targets or image authorizations.
Consumers can lower every exposed limit, including disabling cache retention.
These are structural/work limits, not exact browser/GPU allocation accounting.
The Rust decoder also limits batch size to 256, decoded commands to 65,536 and
JSON output to 16 MiB. Empty glyph batches return font metrics only.

## Tests

`node --test wasm/flow_outlines.test.mjs` checks the native-wire boundary and a
real worker round trip over an explicitly synthetic native provider.

`python wasm/tests/run_flow_canvas.py --chromium /path/to/chromium` uses installed
Python Playwright and Chromium to exercise the production Canvas renderer with
actual pixel assertions. It loads local module bytes as Blob modules, without a
server or network. The glyph fixtures are synthetic: these checks prove Canvas
geometry, paths, baselines, clips and publication behavior, not Rust decoding.
The HTML entry `wasm/tests/flow_canvas.browser.html` runs the same suite when the
repository is served normally. No browser dependency is added to the renderer.

`wasm/flow_outlines_smoke.mjs` instead loads a real generated WASM package,
requests paths for actual shaped glyphs and compares synchronous and worker
responses. Both package assembly gates invoke it. This test must execute before
claiming end-to-end generated-WASM proof; source checks are not a substitute.

Types: `tsc --noEmit --strict --target ES2022 --module NodeNext
--moduleResolution NodeNext wasm/flow_canvas_types_test.mts`.

## Live editing example

Serve the assembled package and open `demo/flow-canvas.html`. It connects the
production worker session, outline API and Canvas backend: source edits are
coalesced, resizing changes measured wrapping, and scrolling repaints only a
viewport-sized surface. Matching packages use indexed viewport pages; legacy
packages retain the bounded full scan described above. A selectable reading-text panel provides a logical-text counterpart,
not a claimed full accessibility widget. Link clicks report targets rather than
navigating. Images remain placeholders until the user explicitly selects local
files. The picker, reference insertion and revocation controls use `./flow-assets`;
no image URLs are fetched and no files are uploaded. Text paints before image I/O,
and completed images trigger a coalesced refresh. See `ASSETS.md` for limits and
ownership. Your textarea remains authoritative after a refused edit or worker
loss. Restart is explicit, with no automatic replay of uncertain mutations.

`node --test wasm/tests/flow_preview_controller.test.mjs` executes eight
controller tests using explicit session/painter doubles, separate from the
Chromium pixel and generated-WASM tests. The demo itself needs the matching
built `pkg/` and a server allowing module workers; it does not install a fallback
renderer when that artifact is missing.

## Indexed rendering regressions

`node --test wasm/flow_canvas_viewport.test.mjs` checks the production renderer
and actual direct/outline facades with explicit native-wire and 2D recording
doubles. It includes sparse pagination, a million-item inventory without prefix
reads, nested clips, cross-page baselines, f32 coverage, work budgets, legacy
negotiation, image ownership, cancellation and 30 seeded mixed-scene comparisons
against the dense path. These prove JS behavior, not native performance or a
built WASM artifact. Native spatial-index tests remain a separate build gate.
