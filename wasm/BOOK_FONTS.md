# Supplied fonts in the book publisher

The rebuilt `demo/book.html` has a **Book fonts** section. Assign local TrueType
`.ttf`, OpenType `.otf`, or TTC/OTC collection files to body regular, bold, italic,
bold italic and monospace roles. For collections, enter an explicit face index
from zero through 1,023; leave that field blank for a standalone font. Pick
one or several files, optionally enter integer weight pins from 1 through 1000,
and choose **Validate and assign selected fonts**. Blank pins use the renderer's
role defaults. Reselect a font file to change its pin; changing a draft without
selecting its file does not change an installed font.

The proposed batch is read, admitted and checked through dedicated instances of
the existing book worker. Its shared Rust extractor selects collection faces
before the ordinary book-PDF preflight. A small document exercises all five style roles;
the regular renderer validates and instances supplied font bytes. This allows
font assignment before a book has chapters and avoids making font admission
depend on unrelated missing images or broken transclusions. The probe is never
installed as source or offered as a publication download. Only after successful
preflight does the existing collection install the complete batch in one revision.
A failed file or worker check leaves every previous font assignment intact.

This preflight checks the proposed faces, not the complete current book or every
Unicode character. It does not certify embedding rights or absence of fallback.
Rebuild the HTML-site preview or book PDF proof afterward to inspect the actual
book. No browser FontFace, system font discovery, remote font URL or alternate
rendering engine is involved. The renderer accepts TrueType outlines and
supported CFF1 OpenType outlines, including bounded CID-keyed faces. The filename
admits a local file for validation; it does not establish that its font program
is supported. CFF2 and unsupported Type2
glyph programs are refused. WOFF/WOFF2 remain outside this file picker.

Use a WASM engine rebuilt with CFF1 rendering support. Older engines may reject
a selected OpenType font; that failure leaves the previous assignments intact.
Collection selection additionally requires matching worker scripts and the
`fontCollectionFaceCount` / `extractFontCollectionFace` WASM exports. An old
package reports `UNSUPPORTED_WASM_PACKAGE` and does not silently select a face.

## Replacement, fallback and removal

Selecting another file for an occupied role replaces that role only. Other fonts,
image resources, source paths, chapter order and presentation settings are retained.
The current filename, retained size and weight pin are shown for every assigned
role. Unassigned roles use the engine's existing bundled/inherited fallback.
Changing the Sans/Serif selector does not silently remove explicit assignments.

**Remove supplied font** asks for confirmation for one role; **Revoke all supplied
fonts** asks before clearing all roles. Neither deletes or modifies an original
file or rewrites Markdown. An empty role is a no-op. Removing fonts changes the
collection revision, invalidating its prior previews, PDF proofs and downloads.
Source undo does not restore resources. Save a portable backup before removing
fonts that may be needed later.

## Ownership, cancellation and limits

Each retained role is limited to 8 MiB and the final set to 32 MiB. A collection
input may be up to 32 MiB; distinct selected files may total at most 64 MiB per
operation. Standalone-font size checks precede file reads. Collection face sizes
are checked after extraction, and the exact final-set budget is always checked
before PDF preflight. Replacing a role at capacity does not require extra
retained-store headroom. Selecting the same File for several roles reads it
once and extracts each selected face once, while each role keeps its own weight
and privately owned installed bytes.
These are admission/retention limits, not a whole-process memory guarantee.

File reads, asynchronous confirmations and validation share a two-minute host
deadline. The existing worker has its own two-minute execution deadline and
128 MiB output ceiling. **Cancel font operation** retires only this operation;
publication, HTML-site preview and PDF-proof workers are independent. Cancellation
settles the host promise even when an adapter ignores abort; the production book
worker terminates its synchronous Rust computation. File.arrayBuffer cannot be
interrupted after starting, so a late read is observed and discarded. Native
browser confirmation dialogs themselves are not programmatically dismissed.

Collection revision, editor checkpoint and selected-file/face/weight identity are
checked before installation, including silent changes with no input event. Edits,
imports, composition, changing a file/pin, suspension and disposal cancel obsolete
work. Retries are explicit. Suspending the publisher clears uncommitted File
selections; its existing collection lifecycle also revokes installed font/image
authorizations. Returning from the back/forward cache does not reload fonts.

The collection remains the single font owner. PDF/EPUB/site exports and explicit
portable backups consume its font snapshots. For an imported collection these
contain only the selected standalone face, with a `[face N]` label in the stored
filename; reopening needs neither the original collection nor another selection.
Source-only project files
and local-library saves deliberately exclude font bytes. A portable restore
refreshes the inventory from the restored collection; it does not retain stale
picker selections. Nothing is saved, uploaded or persisted automatically.

## Integration and verification

`createBookFontAuthoring` is in `demo/book_font_authoring.mjs`. It owns a dedicated
book worker, uses the existing collection `setFonts`, `revokeFont` and `revokeFonts`
contracts, and exposes `assign`, confirmed `remove`/`clear`, cancellation,
subscriptions and lifecycle methods. `assign` accepts one through five
`{slot, file, weight, faceIndex}` selections (`weight` and `faceIndex` are optional
for standalone fonts). Its optional operation options are `signal`
and an `isCurrent()` host guard. The UI provides that guard for its raw drafts.
The panel/controller are in `demo/book_font_controls.mjs`; the publisher bootstrap
handles initialization failure without disabling other features. Both package
assemblers and the package manifest include these modules and this guide.

```sh
node --test wasm/tests/book_font_authoring.test.mjs \
  wasm/tests/book_font_controls.test.mjs wasm/tests/book_font_package.test.mjs
CHROMIUM=/usr/bin/chromium python wasm/book_font_browser.py
```

The public book worker also exposes
`extractFontCollectionFace(bytes, faceIndex, {signal})`, returning
`{bytes, faceIndex, faceCount}`. Its input is captured before worker startup,
and the response must identify the requested selection. This operation grants
no book/source authority and does not retain a book. See
[Font collections](../docs/FONT_COLLECTIONS.md) for the lower-level browser API.

Node tests execute the production authoring session, controls and font store with
explicit collection/editor and transport/DOM adapters. Package checks load the
staged helper graph and execute the unchanged publisher bootstrap against named
constructor/lifecycle adapters. The Chromium harness uses real file inputs,
File.arrayBuffer, Blob module loading and Workers with an explicit protocol-only
font/PDF adapter, not the Rust font parser. It must not be reported as evidence
of native font support, generated-PDF fidelity, glyph coverage or full-workbench
acceptance. Run the generated-WASM and native rendering gates separately.


## Complete fonts for browser and EPUB shaping

Pass `fontEmbedding: "full"` to `createBook`, `renderBookSite`, document-worker
HTML/EPUB exports, or Flow HTML/EPUB exports to preserve complete supplied or
bundled OpenType fonts. Complete character maps and substitution/positioning
tables remain available to the browser or EPUB reader. Publications are larger;
this does not change native PDF shaping or PDF font subsetting.

The default `"subset"` retains the existing compact output and ABI calls.
For HTML, `htmlFontFormat` independently selects the raw sfnt, WOFF or WOFF2
container. EPUB uses the source sfnt format, including supported CFF1 OpenType.
Interactive HTML already keeps complete fonts for later edits and has no
separate `fontEmbedding` switch.

The book workbench's publishing profile includes this selection. Applied
profiles survive source and portable project round trips, native book reuse,
source deltas, site/EPUB exports and chapter previews. Selecting Full requires
matching native bindings and worker scripts; older packages fail explicitly.
Full book font-bearing outputs acknowledge `fontEmbedding: "full"`; the worker
also checks its captured policy for PDF-first and source-update operations so
an older worker cannot silently lose the later HTML/EPUB setting.

```js
const book = await createBook(chapters, { fontEmbedding: "full", fontAssets });
try {
  const epub = book.renderEpub();
  const site = book.renderSite();
  // epub.fontEmbedding and site.fontEmbedding are "full".
} finally {
  book.dispose();
}
```
