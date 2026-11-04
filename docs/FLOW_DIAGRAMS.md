# Native diagrams in browser Flow

Flow sessions render supported Mermaid flowcharts, state diagrams, sequence diagrams and pie
charts as native vector paths with measured, bundled-font labels. This is part
of every `wasm-flow` build; it needs no additional feature flag. Rebuild the
generated WASM package and use the matching in-tree Canvas and reader modules.

The same first-party diagram compiler serves the publication outputs. Flow
uses its complete vector scene, including node shapes, arrowheads, return
routes, dashed strokes, sequence actors and activation bars, and pie slices.
The Canvas consumer draws the supplied paths and actual font outlines. It
does not parse Mermaid, execute generated SVG, shape labels with browser
fonts, fetch images, or download a diagram runtime.

## Use it

````js
import { createFlowSession } from "@franken-suite/franken-markdown/flow";
import { FlowCanvasRenderer } from "@franken-suite/franken-markdown/flow-canvas";
import { FlowReaderView, readFlowDocument }
  from "@franken-suite/franken-markdown/flow-reader";

const source = [
  "```mermaid",
  "flowchart LR",
  "  A[Write] -. Review .-> B{Ready?}",
  "  B ==> C([Publish])",
  "```",
].join("\n");

const session = await createFlowSession(source, {
  font: "sans",
  viewportWidth: 480,
});
const canvasView = new FlowCanvasRenderer(document.querySelector("canvas"));
const readerView = new FlowReaderView(document.querySelector("article"));

await canvasView.render(session, { width: 480, height: 640 });
await readerView.renderAsync(await readFlowDocument(session));

// Resize preserves the original source and recomputes the complete figure.
session.reflow({ viewportWidth: 320 }, session.token);
await canvasView.render(session, { width: 320, height: 640 });

// When these views are no longer needed:
// readerView.dispose();
// canvasView.dispose();
// session.dispose();
````

The public session, Canvas and reader entry points retain their existing
transaction and disposal contracts. See [persistent sessions](../wasm/FLOW.md)
and [Canvas presentation](../wasm/CANVAS.md).

## Supported figures and fitting

| Input | Flow output |
|---|---|
| `graph` / `flowchart` | Native rectangles, rounded nodes, circles, diamonds, forward and return routes, labeled solid/dotted/thick links, and arrowheads |
| `stateDiagram` / `stateDiagram-v2` | Flat state machines with descriptions, initial/final markers, choices, fork/join bars, guarded transitions and separate feedback/parallel routes |
| `sequenceDiagram` | Participants and actors, solid/dashed message lines, arrowheads, activations, self-messages, frames, notes and labels |
| `pie` | Native curved slices, title, percentages, legend colors and optional original values |
| Unsupported or malformed recognized diagram DSL | A visible explanation followed by the complete source |
| ASCII art and `ditaa` | The existing ordinary code presentation, preserving source rows |

The compiler grammar and its source limits are documented in
[Native Mermaid diagrams](DIAGRAMS.md). Recognized fenced diagrams also work
inside Flow lists and quotations. The complete scene is prepared before it is
published, so an unsupported primitive does not leave behind a partial chart.

Labels use the active Flow shaper, real ascent/descent, actual glyph advances,
native baselines, and explicit label colors. The figure envelope includes all
labels, control points, arrowheads and conservative stroke extents. The whole
figure scales uniformly into its available lane; body-size changes scale it
consistently, while narrow viewports reduce the figure to fit. Following blocks
start below that measured envelope. Diagram colors retain the compiler's light
palette on an explicit white backdrop, including on a dark host canvas.

The normal bundled left-to-right shaping profile still applies. Optional
`cjk-face` and `emoji-face` builds extend its covered label repertoire.
A label outside that profile causes source fallback; if the source itself
cannot be shaped by the active profile, the normal `LAYOUT_ERROR` transaction
failure preserves the previous successful document.

This is a diagram-source API. It does not add a general SVG document, remote
image, external font, Mermaid configuration, script, or arbitrary CSS input
surface.

## Drawing contract

Schema version 1 gains optional `stroke` on path items and optional `fill` on
text items. Consumers should update alongside the generated producer. Older
consumers cannot faithfully paint the new stroked figures.

A path still carries absolute `M`, `L`, `Q`, `C` and `Z` commands and its
original enclosing Markdown source span. An example stroke is:

```js
{
  color: [0.15, 0.2, 0.3, 1],
  width: 1.5,
  dash: [4, 4],
  dashOffset: 0,
  cap: "butt",
  join: "miter",
  miterLimit: 4
}
```

All seven fields are required when a stroke exists.

| Field | Admitted values |
|---|---|
| `color` | Four finite RGBA components in 0..1 |
| `width` | Finite, greater than zero, at most 1,000,000 |
| `dash` | At most 16 finite lengths in 0..1,000,000; empty means solid, a nonempty array must contain a positive length |
| `dashOffset` | Finite, absolute value at most 1,000,000,000,000 |
| `cap` | `"butt"`, `"round"` or `"square"` |
| `join` | `"miter"`, `"round"` or `"bevel"` |
| `miterLimit` | Finite, from 1 through 1,000,000 |

With a stroke, `fill: null` means stroke-only and allows nonempty open
subpaths. Explicit RGBA fill plus stroke paints both. Any filled contour must
be explicitly closed. Without a stroke, the existing path contract remains:
`fill: null` uses the semantic palette color, and the path is closed and filled.

Bounds contain every endpoint and curve control point plus a conservative
stroke halo: half the width multiplied by the larger of the miter limit for a
miter join and the square-root-of-two factor for a square cap. Other cap/join
factors are one. This keeps indexed culling and hit envelopes from excluding
thick corners or line ends.

Text `fill` is an optional four-component RGBA value. Omit it for the usual
semantic palette; explicit null is not a text color. Native `baselineY` and
glyph-run data remain authoritative. Diagram label selections are local to
those label runs and their enclosing source span; they are not fabricated
Markdown source caret offsets.

The Canvas boundary validates and owns color arrays, dashes, stroke records
and path command tuples before yielding. It rejects missing/unknown fields,
accessors, inherited stroke fields, sparse arrays, nonfinite coordinates and
out-of-bounds geometry. Later mutation cannot alter an admitted frame.

## Accessible source and editing

Each recognized diagram contributes one `diagram` reading leaf containing
the complete original code source, including when visual rendering falls back.
The semantic reader renders it as:

```html
<section role="region" aria-label="Diagram source">
  <pre><code>original diagram source</code></pre>
</section>
```

The source remains literal text. Search, native browser selection and Copy
retain indentation, Unicode and the final newline; labels resembling markup
do not become HTML or links. Whole-document `copySource` separately preserves
the original Markdown bytes, including BOM and CRLF boundaries. Reading nodes
retain the truthful enclosing block span.

A successful edit replaces paths, labels, reading data and revision together.
Reflow changes the layout revision and refits the original scene. Old display
tokens fail the normal stale-revision/layout checks. A rejected source edit,
invalid reflow or exhausted transaction budget retains the last successful
source, layout options, revisions and display.

## Limits

The scene adapter admits at most 64 KiB of diagram source, 4 MiB of generated
vector markup, 4,096 scene items and 256 KiB of label text. The compiler also
enforces the per-diagram grammar limits described in the syntax guide. It
refuses omitted or unsupported vector primitives, paint resources and viewports
as a whole rather than silently dropping part of a figure.

Flow retains its shared limits for display items, lines, total shaping bytes,
source size and page serialization. Each path is bounded to 65,536 commands;
the shared math/diagram display budget is 262,144 path commands. The complete
prepared figure is charged against those budgets before publication. Recoverable
scene refusals show source; transaction-budget failures retain the previous
successful session.

## Executable checks

Focused JavaScript admission, ownership and reader checks:

```sh
node --test wasm/flow_canvas_diagram.test.mjs wasm/flow_reading_diagram.test.mjs
```

The native integration tests in `tests/flow_diagram.rs` exercise actual compiler
scenes and transactional Flow behavior. Setting `FMD_FLOW_DIAGRAM_FIXTURE`
exports unchanged native snapshots, indexed viewports, reading pages, hits and
bundled glyph outlines:

```sh
FMD_FLOW_DIAGRAM_FIXTURE=/tmp/fmd-flow-diagram-native.json \
  cargo test --no-default-features --test flow_diagram
```

Run the real Chromium suite against a matching, actually generated WASM package:

```sh
node wasm/tests/run_flow_generated.mjs \
  --suite flow_diagram.browser.mjs \
  --fixture /tmp/fmd-flow-diagram-native.json \
  --generated-package /path/to/generated/pkg \
  --chromium /path/to/chromium
```

The runner uses an installed Playwright and Chromium; it does not download test
dependencies. `FMD_PLAYWRIGHT_MODULE` can name an existing Playwright module.
The suite requires actual generated bindings and compares all supported scene
types, fallback and ASCII code against native output. It checks real Canvas
strokes/dashes and label outlines, native-versus-WASM pixels, exact source Copy,
source replacement, fresh-session equivalence and edit/reflow rollback. Browser
text shaping and resource fetching fail the checks. An explicit solid-line
negative control confirms that dropping native dashes changes the actual raster.

The runner records fixture/module/generated-WASM hashes, browser version,
network attempts and page errors. A Node transport test or syntax check alone
does not establish native/WASM or real-browser parity.
