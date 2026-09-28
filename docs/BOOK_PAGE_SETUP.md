# Page setup for the book publisher

The rebuilt `wasm/demo/book.html` publisher includes **PDF page setup** in
**Prepare and download**. Select US Letter, A4, a 6 × 9 inch book, or custom
paper dimensions. Choose orientation and independent top, right, bottom and
left margins, then **Apply page setup**. The controls accept points, inches or
millimetres. They use the existing `normalizePdfPage` contract, not a second
layout engine or a final scaling transform.

Applied geometry is included in the complete book snapshot sent to ordinary
PDF export and **Generate book PDF proof**. It reaches the existing book API's
`renderPdfWithPage` path. Page setup does not change HTML/EPUB reading layout,
font scale, chapter order, source paths, supplied fonts or image permissions.
A matching WASM build with page support is required for nondefault PDF export;
the API rejects an older binary rather than silently using different paper.

## Drafts, precision and validation

Page fields are explicit drafts. Typing or choosing a preset does not change
exports, retained proofs or saved projects until Apply succeeds. The panel shows
both current geometry and the draft's page/content dimensions. **Discard page
draft**, reopening a project or page suspension restores the committed values.
Ordinary source edits do not erase a page draft. Unapplied drafts are not saved.

Dimensions are 144 through 14,400 PDF points (72 points per inch). Margins are
nonnegative and must leave at least 72 points of content in each direction,
including after the native f32 conversions. Orientation rotates the paper, not
the named margin sides. Custom **As entered** dimensions retain their order.
Numbers use a decimal point, not a locale-dependent comma.

The model stores canonical dimensions and margins in points. Unit changes are
presentation only; unchanged formatted fields retain their exact original point
values rather than rounding A4 or imported custom geometry on each conversion.
Unit conversion admits every field first. An incomplete field prevents the unit
switch without deleting its text. Invalid geometry leaves committed settings
and resources untouched. Equivalent geometry is a no-op with no revision change.

A successful Apply captures current editor text/settings through the normal
publisher path, then changes only page geometry in a revision-fenced transaction.
Imports and composition block application. Intervening changes during capture
reject the page transaction. Existing collection notifications invalidate exports,
HTML previews, retained PDF proofs and pending resource operations. The PDF proof
session aborts its owned worker; obsolete results cannot restore an old proof.

## Save, restore and compatibility

Applied page geometry survives source-project downloads, local-library source
serialization and explicit portable backups. Source-only restores still revoke
image/font permissions. Portable restores transfer admitted geometry and resources
in the same collection revision. The existing local-library transaction format
is unchanged; it continues to serialize and validate the complete source project.

Source projects with explicit geometry use **schema version 3**, including books
with include-only files. Older readers reject that version rather than silently
reopening a book with different pagination. The portable envelope remains version
1; only its nested source-project version changes. No geometry means the original
version 1 (chapters only) or version 2 (include-only sources) representation.

**Renderer default** explicitly removes the page override. Default books retain
their original serialized settings and native default-page call. This reset does
not remove resources, rewrite source or change unrelated typography settings.
The ordinary publisher capture preserves the committed page even when the optional
page panel is absent in an embedding host.

## Workbench API and integration

`collection.setPage(page, expectedRevision)` accepts the same page object as the
book API. It validates and owns deeply frozen canonical geometry before mutation,
checks the expected revision again after admission, and returns the installed
revision. `undefined` resets the override. `collection.options.page`, `project()`
and `snapshot()` expose immutable page values, not caller-owned nested objects.

The optional page controller lives inside the already-shipped
`wasm/demo/book_controls.mjs`. It uses the already-shipped `pdf_page.mjs`; neither
package assembler needs a new runtime entry. Its lifetime is owned by the existing
publisher controller, and the ordinary source-capture path retains applied page
settings during metadata edits and save/export actions.

## Verification

```sh
node --test wasm/tests/book_pdf_page_setup.test.mjs
CHROMIUM=/usr/bin/chromium python wasm/book_page_browser.py
```

The small test entry above joins the existing `book_pdf*.test.mjs` gate in both
WASM assembly scripts. Tests exercise production collection, publisher, page
validation and PDF-proof orchestration with explicitly scoped DOM and rendering
adapters. They cover atomic updates, exact round trips, defaults, frozen ownership,
invalid and stale input, unit stability, source capture, resource preservation,
PDF worker forwarding, cancellation and stale-proof invalidation. The original
publisher fails the regression that an imported page survives source capture.

The Chromium harness uses the actual publisher HTML/CSS and production controls,
collection and proof session, with real file inputs, Blob downloads and recording
Workers. It does not initialize the unrelated `book.js` controllers or render
Markdown with WASM. Worker response bytes are labelled protocol fixtures, not PDFs
for visual assessment. It requires Playwright, retains artifacts in a fresh temp
directory and loads local source modules without a network renderer.

Authoring-host execution used unchanged input-validation/error helper extracts
for the collection's larger dependencies and an unexercised source-navigation
adapter. The proof module, page validator, font store, collection and publisher
were complete production files. These checks do not establish native PDF page
counts, visual fidelity, IndexedDB execution or generated-WASM interoperability.
Run the existing compiled native/WASM gates separately; no Rust implementation
or size/parity threshold was changed by this feature.
