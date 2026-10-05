# Persistent flow publication

Persistent direct and worker flow sessions can export `svg` in addition to
`html`, `pdf` and `epub`. SVG invokes the existing native resource renderer from
`flow.js`. It is not a Canvas screenshot, HTML conversion, or new Markdown parser.

```js
const output = await session.exportDocument("svg", {
  maxWidthPt: 360,
  maxOutputBytes: 16 * 1024 * 1024,
}, session.token);
const blob = new Blob([output.bytes], { type: output.mimeType });
```

SVG is a light-only, single-page poster. Width is in points, defaults to 612,
and must be finite within 144..14400. Only `maxWidthPt` and `maxOutputBytes` are
accepted for SVG; HTML/PDF settings such as title, TOC, paper or running headers
are rejected rather than silently discarded. The session's bundled font preset
is retained. No fonts, images, arbitrary renderer options or raw-HTML permission
can be injected through this export call.

## Source and resource ownership

Export collects the original Markdown and resolved encoded image payloads under
one source/layout token before awaiting the renderer. All snapshot pages are
checked. Repeated destinations with identical bytes are deduplicated; conflicting
payloads fail. Pending images, dimensions-only assets and empty payloads refuse
export instead of disappearing. Nothing is fetched. The existing limits remain:
4 MiB source, 1,024 image occurrences, 8 MiB per payload, 32 MiB retained images,
and 64 MiB maximum published output. These are not total heap limits.

Source or layout changes and disposal during an asynchronous export reject its
publication. Concurrent direct exports share the existing `EXPORT_BUSY` gate.
The worker uses its existing export operation, queue, transfer and cancellation
policy; callers must update both worker and facade modules together. A missing
SVG renderer returns `UNSUPPORTED_WASM_PACKAGE`, never a different format. The
public native wrapper separately refuses asset requests against an incompatible
generated WASM package; asset-free legacy warnings remain visible.

## Diagnostics

All flow export formats retain optional native `code` and `scope` fields.
Document findings retain `scope: "document"` and `start: 0, end: 0`; the adapter
does not invent a source span. Parser findings keep their original UTF-8 offsets.
Reason codes are nonempty strings of at most 128 UTF-16 units, charged against
the existing 64 KiB diagnostic text budget. Invalid scopes, codes, spans and
oversized inventories reject both direct output and worker acknowledgment.
The returned records are owned and frozen; output bytes are copied.

## PDF paper and running bands

The same persistent export operation now accepts the native PDF print setup:

```js
const output = await session.exportDocument("pdf", {
  title: "Engineering report",
  page: { size: "a4", orientation: "portrait", margins: 48 },
  running: {
    header: { left: "{title}", rule: true },
    footer: { center: "{page} / {pages}", right: "{date}" },
    skipFirstPage: true,
  },
  metadataEpochSeconds: 0,
}, session.token);
```

`page` uses the existing shared `normalizePdfPage` contract: Letter/A4 or
explicit point dimensions, optional orientation, and uniform or per-side margins.
Dimensions must be 144..14400 points; margins must leave at least 72 points of
content in both dimensions, including the native f32 calculation. Orientation
rotates paper only, never margin sides. Omitted settings stay omitted, preserving
the ordinary renderer path; an explicit page requires the matching native ABI.

Headers and footers have left/center/right string templates and an optional rule.
They are copied into bounded, deeply frozen data before queueing or rendering;
accessors, inherited configurations and unknown fields are refused. Each template
is at most 4,096 UTF-16 units, with 16,384 units combined, and must contain valid
Unicode. Template substitution and actual margin-fit checks remain native work,
not JavaScript text measurement or Canvas painting. Date tokens use the supplied
metadata epoch, default zero, not the clock. An incompatible native package
returns `UNSUPPORTED_WASM_PACKAGE`; there is no fallback that loses print setup.

`skipFirstPage: true` requires at least one nonempty template or enabled rule.
Without a drawable band, the existing native wrapper chooses its legacy ABI and
cannot honor this flag, so the flow API refuses that request rather than silently
ignoring it. Empty bands with no skip request remain valid no-ops. Print settings
do not mutate the editor's viewport layout or revisions. They are PDF-only; other
export formats reject them.

The public `FlowPdfPage`, `FlowPdfRunning` and `FlowPdfRunningBand` types alias the
existing renderer types and are also exported by the worker entrypoint.

## Verification

```sh
node --test wasm/flow_publication.test.mjs wasm/flow_pdf_publication.test.mjs
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext wasm/flow_publication_types_test.mts wasm/flow_pdf_publication_types_test.mts
```

Fourteen Node regressions exercise the actual production flow facade and export
adapter with explicit native-session and renderer doubles. They cover paged
assets, ownership across await, revision/disposal fences, format-specific options,
legacy refusal, output bounds, diagnostic admission and existing-format dispatch.
Thirteen fail against the previous exporter. The strict TypeScript fixture checks
both direct and worker declarations without `skipLibCheck`.

Ten additional PDF cases cover canonical geometry, data capture across await,
structured-clone normalization, native defaults, hostile nested options, template
budgets, legacy refusal and stale publication. Eight fail against the exporter
before PDF print setup. All 24 publication tests and both strict TypeScript
fixtures pass together, using the actual shared paper module and renderer types.

These tests are not generated-WASM, real-worker, native rendering, byte-parity or
browser/raster evidence. Native/Rust, full-package and DSR gates were not run in
the authoring environment. A wrapper change does not rebuild the native package.
