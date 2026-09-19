# Typography and publishing in the live editor

The **Typography and publishing** panel in `demo/flow-canvas.html` configures
measured Canvas preview and document exports. It uses the existing worker and
HTML/PDF renderer, not CSS scaling, a screenshot, or a second Markdown engine.

## Applying settings

Edit the fields, then choose **Apply settings**. Until Apply, fields are a draft:
they do not change the engine. Preparing an export requires applying or discarding
those fields. Field edits invalidate prepared downloads; an in-flight export is
checked again before publication, and a download click rechecks its settings.
**Discard unapplied changes** restores the applied values. **Reset all settings**
returns to the original preview and export defaults. Invalid input keeps both the
last applied settings and all editable field text, with a visible diagnostic.

The three preview presets load draft values only. Standard uses sans/14/13/20;
Reading uses serif/18/15/27; Compact uses sans/12/11/17. The numbers are body size,
code size and line height. Presets preserve publishing fields and require Apply.

## Measured preview

Choose the bundled sans or serif font, body size (8–48), code size (6–40), and line
height (8–72). Line height must accommodate both text sizes. Values are admitted
using the same f32 validation as the worker, so repeated fractional values cannot
cause an endless reflow loop.

Body, code and line-height changes reflow the current native session; they do not
replace source or rebuild the worker. A font-family change rebuilds the native
session from the current source, clearing old glyph ownership first. The existing
host image grant is retained and its assets can be loaded into the new session;
there remains at most one physical image-loading batch across rebuilds.

Scrolling, viewport height and device pixel ratio remain paint-only. Export and
reading navigation reject a newly requested typography state until it matches
the displayed session. Export also rejects a settings change-and-change-back
race, even if it happened before a physical reflow.

These controls do not change the selectable reading surface's browser typography
or extend the bundled shaping profile's supported scripts. Canvas is a flowing
preview, not a paginated PDF proof. Font family is shared with HTML/PDF exports;
Canvas body size, code size and line height are not PDF point-size settings.

## Publishing

Shared HTML/PDF options are title, language tag, table of contents and its depth.
A language tag is metadata/render configuration, not translation or a promise of
support for an otherwise unsupported script. Optional blank fields use the
renderer default instead of guessing an application default.

PDF additionally exposes author, page numbers, code line numbers, body size
(6–24 pt), heading scale (1.05–2), table size (5–24 pt), target page count (1–1000),
and optical margin alignment (disabled or protrusion). The page count is a fit
request, not a guarantee that arbitrary content can fit. The engine's diagnostics
remain visible in export status. PDF timestamps remain explicitly deterministic
at epoch zero; the panel does not substitute the current wall clock.

HTML appearance follows the reader's light/dark preference by default, with a
light-only alternative. PDF-only settings are not sent to HTML. The font used by
exports comes from the current native session, not an arbitrary option overriding
the preview's font identity. A publishing-only Apply invalidates prepared exports
without changing Markdown or requiring a different preview layout.

## Ownership and lifecycle

Settings are page-session memory only. They are not written into Markdown,
frontmatter, file handles, browser drafts, local storage, or a server. Applying a
setting never changes selection, undo/redo, file-save identity, or source dirty
state. It does not save a file. Opening/importing/restoring another document
resets all publishing settings and pending fields so an old title, author or
language cannot leak into the next document; applied preview preferences remain.

Page suspension removes listeners and disables the old controls. A back/forward
return can retain applied primitive settings in memory, but does not retain file
or image authority through this module. The controls are remounted once without
duplicate labels/listeners. An ordinary reload starts with defaults.

Settings cannot enable raw HTML, load network images, supply custom font bytes,
override workers, or smuggle asset payloads through export options. Existing
source, image-authorization, worker, byte-budget and stale-revision checks remain
in force. Source import/save/download runs through its separate entrypoint, so a
rendering or settings failure does not take away original Markdown recovery.

## Verification

```sh
node --test wasm/tests/flow_preview_controller.test.mjs \
  wasm/tests/flow_export_controls.test.mjs \
  wasm/tests/flow_render_settings.test.mjs
```

The suites execute production preview orchestration, settings normalization and
controls, reading/export admission, adapter validation and Blob creation. Their
session/painter/renderer/DOM/URL doubles are explicit. They do not prove actual
Rust glyph output, PDF appearance or native browser accessibility. Package checks
cover the manifest and both assemblers; they are not a generated-WASM build.
