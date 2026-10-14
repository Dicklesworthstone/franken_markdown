# Book publishing profiles and authoring

The publishing workbench now exposes the detailed settings already supported by
the native book facade. Open `demo/book.html` in a matching assembled package and
use **Typography and publishing profile**. This is a data/authoring surface, not
a second renderer, font loader or image-authorization mechanism.

## Author, apply, proof and save

The panel includes body/heading/table sizes, TOC depth, an adaptive PDF page-count
target, code line numbers, metadata epoch, HTML-site font format, and the six
native typography switches. Running header/footer bands have left/center/right
templates, rules, first-page suppression, and repeated logos selected from the
book's authorized image paths. Logo position and height remain native PDF options.

All inputs are **drafts until Apply publishing profile**. Invalid input preserves
both the draft and the previously applied settings. Discard reloads the applied
profile; Use native defaults in draft stages a reset that still requires Apply.
Applying updates only the publishing profile, never captures or rewrites the
source editor, resets paper geometry, or starts a render. PDF proofs, exports,
source projects and local-library saves consume only the applied profile.

Changed profiles advance render configuration, so existing collection observers
clear stale previews, PDF proofs and prepared downloads. Source/include text
edits preserve a profile draft. Settings, resource, structure or project changes
refresh a pristine panel; a dirty draft is retained but blocked until explicitly
reloaded. Silent DOM edits are detected by actual control values, not just input
events. Import/composition blocks Apply; the active composition field stays
editable. Suspension discards drafts, and disposal releases all subscriptions.

Blank numeric fields select native defaults; zero is valid for the metadata
epoch. Invalid numeric text never becomes an empty default through browser
number-input sanitization. Unchanged textarea views preserve the saved template's
exact CRLF/CR bytes, including when another field is edited. Typography token
spelling is retained on no-op captures. Source-only edits do not rebuild unchanged
logo selector options. No resource payload is copied merely to edit a profile.

After applying, generate an **actual book PDF proof** and inspect it before
publishing. Authoring does not prove that templates, logo dimensions or margins
fit a finished PDF. Rendering retains the existing capability checks: older WASM
builds reject explicitly requested unsupported features instead of silently
ignoring them. Source editing and saving remain independent of those errors.

## Collection API

```js
collection.setPublication({
  typography: "homogeneous,antiriver,pareto,optimal-pagination,protrusion,expansion",
  baseFontSize: 12,
  headingScale: 1.25,
  tableFontSize: 9,
  tocDepth: 3,
  codeLineNumbers: true,
  metadataEpochSeconds: 0,
  htmlFontFormat: "woff2",
  running: {
    header: { left: "{title}", right: "{author}", rule: true,
      image: { dest: "assets/logo.png", position: "left", heightPt: 18 } },
    footer: { center: "{page} / {pages}" },
    skipFirstPage: true,
  },
}, collection.revision);
```

`setPublication` replaces only these publishing choices, atomically. It preserves
chapter/include bytes, roles and order, metadata, paper size, images and fonts.
Invalid or stale submissions cannot partially install a profile. A no-op leaves
both revision counters alone; a changed profile invalidates render configuration.
`setPublication({}, revision)` resets the profile without resetting geometry or
metadata. No source/font/image store is read to construct a replacement profile.

The accepted canonical fields are exported as `BOOK_PUBLICATION_FIELDS`.
`normalizeBookPublication` delegates numeric, typography and running-band
admission to the existing book facade; it does not parse templates or Markdown.
Use comma-separated `typography` tokens rather than the facade's convenience
aliases in stored profiles. `fitToPages` is an adaptive target, not a guaranteed
page count. Body size, heading scale, table size, TOC depth, code line numbers,
metadata epoch and HTML font container retain existing limits and semantics.
See `BOOK_PUBLISHING_OPTIONS.md` for the underlying native contract.

## Saving, restoring and resource authority

Non-default profiles use **source-project schema 4**. Source downloads, the local
library's existing source serializer, and the project inside a portable bundle
retain the same profile. Old/default projects continue to use schemas 1–3 as
appropriate; older readers reject schema 4 rather than silently losing settings.
Profiles concealed inside earlier schema versions are rejected too.

Running-logo destinations must be safe book-relative PNG/JPEG/SVG paths. They
remain references when a source-only project is reopened, but image and font
bytes/grants are still removed. The panel labels missing paths rather than
silently removing them. Reauthorize those image paths before PDF rendering.
Portable bundles keep their separate, explicit resource-authorization step.
No reference causes a network fetch, filesystem read, or font installation.

The authoring factory lives in the already-shipped `book_pdf_controls.mjs`, but
has no PDF-proof session or worker ownership. `book.js` initializes it separately
and handles its suspension, resumption, disposal and initialization failure.
The original PDF-proof controller is unchanged. No new runtime package entry,
assembler path, dependency, workflow or native API is introduced.

## Verification

```sh
node --test wasm/tests/book_publication_*.test.mjs
python wasm/book_publication_browser.py --chromium /usr/bin/chromium \
  --output /tmp/fmd-publication-browser
```

The authoring run passes **39 Node tests**: 19 model/serialization tests, 18
controller tests and two application-orchestration tests. It also passes **27
Chromium checks**, with zero page errors and zero observed HTTP attempts. These
cover profile application, no-ops, invalid input, Unicode, real textarea newline
behavior, source/portable round trips, missing logo references, conflicts,
composition events, suspension and reset. The browser runner creates the panel
through the production factory and writes a JSON report, input module Git-blob
hashes and a screenshot. It requires an already-installed browser, never a
download. HTTP attempts are recorded and blocked.

Scope is explicit: DOM/source-host adapters are named fixtures; no renderer is
substituted to claim PDF output. On the authoring host, the unchanged option
normalization portion of `book_session.mjs` and the `bookError` export were
isolated from unused native/transport exports; the uncalled PDF-proof dependency
throws on use. Collection, font store, page contract and profile/PDF UI module
were complete. The committed tests and browser runner use normal repository
imports; the orchestration test executes the unchanged application body with
factory doubles. Original changed files were verified against their Git blob
hashes, and the existing PDF-proof body remains byte-for-byte unchanged.

These results do not establish native Rust/WASM execution, worker integration,
IndexedDB transactions, operating-system IME behavior, PDF/font rendering, the
normal HTTP/module-worker startup, or a full package build. Rust/Cargo and the
prescribed remote build tools were unavailable. Run the repository's separate
native, generated-WASM and package gates before claiming those results.
