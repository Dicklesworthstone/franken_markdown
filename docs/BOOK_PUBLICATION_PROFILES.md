# Persistent book publishing profiles

The workbench collection now retains the detailed settings already supported by
the native book facade, alongside source, metadata and page geometry. A profile
is data, not a new renderer or an image/font authorization mechanism.

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
both revision counters alone; a changed profile advances render configuration,
which invalidates existing previews, PDF proofs and prepared downloads through
their existing collection subscriptions. No resource payload is copied just to
change settings. `setPublication({}, revision)` resets the profile without
resetting page geometry or metadata.

The accepted canonical fields are exported as `BOOK_PUBLICATION_FIELDS`.
`normalizeBookPublication` delegates numeric, typography and running-band
admission to the existing book facade; there is no independent template parser.
Use comma-separated `typography` tokens rather than the facade's convenience
aliases in stored profiles. `fitToPages` is also accepted, as an adaptive target,
not a guaranteed page count. Body size, heading scale, table size, TOC depth,
code line numbers, metadata epoch and HTML font container retain the facade's
existing limits and semantics. See `BOOK_PUBLISHING_OPTIONS.md` for details.

## Saving and restoring

Non-default profiles use **source-project schema 4**. Source downloads, the local
library's existing source serializer, and the project inside a portable bundle
retain the same profile. Old/default projects continue to use schemas 1–3 as
appropriate; older readers reject schema 4 instead of silently losing settings.
Profiles concealed inside earlier schema versions are rejected as well.

Running-logo destinations must be safe book-relative PNG/JPEG/SVG paths. They
remain references when a source-only project is reopened, but image and font
bytes/grants are still removed. Reauthorize images with the same paths before
rendering. Portable bundles keep their separate, explicit resource-authorization
step. No path causes a network fetch, filesystem read or font installation.

Rendering uses the ordinary captured book options, including native capability
checks for older WASM builds. Unsupported requested settings fail explicitly;
source remains available for editing/saving. Authoring a profile does not prove
that its templates, logo dimensions or margins fit a finished PDF. Inspect an
actual native PDF proof before publishing.

## Verification

```sh
node --test wasm/tests/book_publication_project.test.mjs
```

The 19 transaction tests cover native option admission, source and portable
round trips, resource preservation, schema compatibility, limits, Unicode,
accessors, revision fences and source-only editing. They do not render a PDF.
On the authoring host, the unchanged option-normalization portion of
`book_session.mjs` and the `bookError` export were isolated from unused native and
transport exports; collection, font-store and page modules were complete. The
committed tests import the normal repository modules. No Rust/WASM compilation,
worker integration, IndexedDB transaction or visual-rendering result is claimed
by these model tests. No dependency or package assembly change is required.
