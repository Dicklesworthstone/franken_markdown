# Typography and publishing in the live editor

The **Typography and publishing** panel in `demo/flow-canvas.html` configures
Markdown dialect, measured Canvas preview and document exports. It uses the
existing worker and HTML/PDF/EPUB/SVG renderers.

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
code size and line height. Presets preserve dialect and publishing fields and
require Apply.

## Markdown dialect

**Markdown dialect (preview and exports)** selects the document's grammar:

| Choice | Behavior |
| --- | --- |
| Renderer default (GFM plus) | Preserves the existing extended grammar and legacy native constructor. |
| CommonMark + GFM | Uses the engine's GitHub-compatible syntax. Definition-list-shaped text remains ordinary paragraph text, and page-break comments remain ordinary comments. |
| GFM plus | Also enables definition lists and standalone `<!-- pagebreak -->` comments for printed page boundaries. |

The dialect is shared by the measured preview, semantic reading surface and
HTML/PDF/EPUB/SVG exports. It belongs to the native session, so an export cannot
override it independently. Applying a dialect change rebuilds the session from
the unchanged editor source. Old pixels and reading navigation are withdrawn
immediately; late paints, transcripts and exports from the old session cannot
be published, even when the new session starts with the same revision numbers.
Returning to **Renderer default** also rebuilds explicitly.

Typing, resizing, typography presets and **Restart preview from current source**
retain the applied dialect. Opening/importing/restoring a different plain
Markdown document resets dialect to **Renderer default**: plain Markdown files
and browser drafts do not carry publishing settings. Font and preview size
preferences are retained across that document change. An editable
`.fmdflow.json` project instead restores its saved dialect and complete applied
profile; see [source-project recovery](SOURCE.md#editable-flow-source-projects).

An explicit dialect requires a matching generated WASM package with the
`FmdFlowSession.withProfile` binding. An older package reports
`UNSUPPORTED_WASM_PACKAGE` and preserves the editor source instead of silently
using a different grammar. Selecting **Renderer default** can recover the older
package's existing behavior. Dialect selection does not enable raw HTML, grant
image access, or change the bundled shaper's script coverage.

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
preview, not a paginated PDF proof. Font family is shared with all four export formats;
Canvas body size, code size and line height are not PDF point-size settings.

## Publishing

Choose **Prepare HTML**, **Prepare PDF**, **Prepare EPUB**, or **Prepare SVG**,
then use the prepared download link. Title and language apply to HTML, PDF and
EPUB. All four formats receive the selected table of contents and its depth.
Title and author controls support multiple lines. Unchanged imported metadata
retains its original CR/LF bytes in project saves and exports; editing the field
uses the textarea's LF convention. Changing other settings preserves that text.
A language tag is metadata/render configuration, not translation or a promise of
support for an otherwise unsupported script. Optional blank fields use the
renderer default instead of guessing an application default.

SVG has an optional poster width from 144 through 14,400 points; leaving it
blank uses the renderer's 612-point default. EPUB retains the applied appearance
settings. Each format uses the current session dialect, font and explicitly
authorized images. Missing native EPUB or SVG bindings produce an explicit
refusal while other supported formats remain available.

PDF additionally exposes author, page numbers, code line numbers, body size
(6–24 pt), heading scale (1.05–2), table size (5–24 pt), target page count (1–1000),
and microtypography (disabled, protrusion or expansion). The page count is a fit
request, not a guarantee that arbitrary content can fit. Expansion enables glyph
expansion without protrusion; protrusion retains the native combined mode.
PDF timestamps remain explicitly deterministic at epoch zero.

**PDF output policy** selects standard PDF, PDF/A-2b archival, or PDF/A-2b
archival with strict writer checks. Strict export refuses forbidden URI actions.
A successful archival export enables the PDF/A-2b download. **Verify PDF layout**
can check the same page geometry, fonts and text before either kind of export.
Independent PDF/A conformance testing requires the emitted file. Flow projects
retain the selected policy, and opening plain Markdown restores standard PDF.

The **PDF headers and footers** group provides independent left, center and right text
slots for both page bands, a rule for each band, and an option to leave the first
physical page without any header, footer or page number. For example, put
`{title}` in Header left, `{section}` in Header right, and `{page} / {pages}` in
Footer center, then enable Header rule and apply. The renderer expands the
templates against the final PDF page layout. Canvas remains a continuous preview
and does not display the page bands.

Templates support `{page}`, `{pages}`, `{title}`, `{author}`, `{date}`, `{section}`
and `{chapter}`; unknown tokens remain literal. The date token uses the export's
deterministic timestamp, currently `1970-01-01`, rather than the current date.
Text is preserved exactly, including leading/trailing spaces and unchanged
imported CR/LF. Each slot renders on one line: the renderer replaces each control
character with a space. Each slot allows at most 4,096 UTF-8 bytes, and all six
together allow at most 16,384 UTF-16 units. The shared validator reports invalid
Unicode or limits before applying the settings.

An empty Footer center lets **PDF page numbers** supply its usual centered
folio. Any nonempty Footer center takes precedence, including whitespace, and
remains explicit content even when automatic page numbers are Off. The controls
do not rewrite templates or add another page number. To suppress page numbers
on the first page, enter `{page}` in Footer center and enable the first-page
option. That option requires a nonempty header/footer or an enabled rule; it
suppresses all first-page bands and automatic page numbers without renumbering
later pages. Explicit bands must fit the selected top/bottom margins; the
renderer reports an error if they do not fit.

All nine header/footer fields are part of the same applied settings capture as
PDF export and configured PDF verification. Editing them invalidates prepared
PDFs and reports. They round-trip in `.fmdflow.json` projects, survive a
same-document remount, and reset when opening plain Markdown or resetting all
settings. Older projects without these fields retain the original PDF defaults.
The text controls do not grant decorative-image access; running-band images
remain available through the explicit asset APIs.

Paper controls select Letter, A4, A5, Legal, Tabloid, or custom width and height,
with an optional portrait/landscape orientation. Custom dimensions are used only
when **Custom dimensions** is selected; incomplete or conflicting fields are
rejected before export. Dimensions and independent top/right/bottom/left margins
use PDF points (72 points per inch). Blank page controls preserve the renderer
default. Once any paper or margin control is selected, omitted size uses Letter
and omitted margins use 72 points. The shared page validator enforces dimensions
from 144 through 14,400 points and at least 72 points of content on each axis,
including the native float-precision boundary. Explicit zero margins are valid.

The PDF baseline grid accepts 1–144 points, or zero to disable it. A fixed grid
cannot be combined with a target page count because fitting changes type sizes
and margins. The panel also exposes the existing homogeneous spacing,
river-reduction and paragraph-alternative refinements, optimal pagination, and
strict pagination. These are optional engine settings, not promises that every
document has a feasible exact layout. Strict pagination refuses planner fallback
with the engine's typed failure; default rendering retains graceful fallback.
The engine's diagnostics remain visible in export status. Blank switches preserve
defaults and explicit Off is retained. Older renderer packages that lack a
requested capability reject it with a rebuild explanation.

For long documents, **PDF note numbers link back to their citations** enables
return navigation, and **Continue long PDF footnotes across page feet** keeps
eligible long notes at page feet instead of moving the entire note to the end.
Continuation supports standard, optimal and strict pagination, including
baseline grids. Optimal pagination plans body boundaries together with pending
note fragments; strict pagination reports the engine's typed refusal if that
exact plan cannot honor the document's constraints. Oversized indivisible note
content retains the engine's documented endnote fallback.

The panel also exposes measured PDF body line height (1–4 times the body font
size) and horizontal/vertical table-cell padding (0–8 em). These values affect
PDF layout, including pagination, rather than the flowing Canvas preview.
Explicit numeric values enable theme spacing; **Use theme spacing in PDF** can
remain at its default or be On. Explicit Off with numeric spacing is rejected.
Zero table padding is valid, and clearing the numbers restores engine defaults.

All paper and typography settings are applied atomically. Invalid margins,
custom dimensions or grid/fit combinations retain the previous applied settings
and every editable field, so the draft can be corrected or discarded.

HTML and EPUB appearance follow the reader's light/dark preference by default,
with a light-only alternative. PDF-only settings remain scoped to PDF. The font used by
exports comes from the current native session, not an arbitrary option overriding
the preview's font identity. A publishing-only Apply invalidates prepared exports
without changing Markdown or requiring a different preview layout.

## Ownership and lifecycle

Source or settings composition immediately invalidates prepared downloads.
Publication waits through the final input turn after composition ends, including
when the browser delivers that final input after `compositionend`.

If a publication deadline or cancellation destroys the retained worker, the
preview withdraws navigation and publication readiness immediately. The editor
keeps its Markdown and the last successfully painted frame. Choose **Restart preview from current source** to create a new session from the latest source. A late export or paint
from the old session cannot restore readiness or replace the new session; at
most one physical export remains active while old work settles.

Settings remain in page memory until explicitly saved with **Prepare Flow source
project**. That project retains source and the complete applied profile together;
unapplied fields must be applied or discarded before saving. Settings are not
written into Markdown, frontmatter, direct Markdown file saves, browser drafts,
local storage, or a server. Applying a setting never changes selection,
undo/redo, file-save identity, or source dirty state, and does not save a file.
Opening or restoring plain Markdown resets dialect, publishing settings and
pending fields while retaining font and preview size preferences. Reopening a
Flow source project restores its own profile and revokes prior image access.

Page suspension removes listeners and disables the old controls. A back/forward
return can retain applied primitive settings in memory, but does not retain file
or image authority through this module. The controls are remounted once without
duplicate labels/listeners. An ordinary reload starts with defaults.

Settings cannot enable raw HTML, load network images, supply custom font bytes,
override workers, or smuggle asset payloads through export options. Existing
source, image-authorization, worker, byte-budget and stale-revision checks remain
in force. Source import/save/download runs through its separate entrypoint, so a
rendering or settings failure does not take away original Markdown recovery.
The source entrypoint also owns the settings panel, so applied-profile project
save/reopen remains available when the generated renderer cannot load. Preview,
publication and PDF verification subscribe to that same applied profile.

## Verification

```sh
node --test wasm/tests/flow_preview_controller.test.mjs \
  wasm/tests/flow_preview_profile.test.mjs \
  wasm/tests/flow_export_controls.test.mjs \
  wasm/tests/flow_render_settings.test.mjs \
  wasm/tests/flow_publication_controls.test.mjs
```

The suites execute production preview orchestration, settings normalization and
controls, reading/export admission, adapter validation and Blob creation. Dialect
integration also exercises the public direct entrypoint and real Node worker
transport through all four exports, including older-binding refusal and recovery.
Their native/session/painter/renderer/DOM/URL doubles are explicit. They do not
prove actual Rust glyph output, PDF appearance or native browser accessibility.
Package checks cover the manifest and both assemblers; they are not a
generated-WASM build.
