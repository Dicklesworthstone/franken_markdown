# Replace and remove images in the book publisher

The rebuilt `demo/book.html` publisher has **Replace or remove book images** in
**Arrange and edit**. Select an existing book-relative binding, choose a local
replacement, and select **Replace selected image**. Every Markdown reference to
that path can use the replacement without changing its destination or alt text.
Other images, source files, fonts, reading order and page settings remain intact.

The picker accepts PNG, JPEG and SVG, up to 8 MiB per file. Replacements keep the
binding's format; `.jpg` and `.jpeg` are equivalent. To add a different format or
a new destination, use the ordinary file/folder imports and update references
explicitly. A filename extension is not proof of valid image contents.

This feature authorizes bytes using the existing import policy. It does not load
untrusted images into the host DOM, parse Markdown again, fetch URLs or invoke a
second renderer. It does not perform image decoding or a native rendering
preflight. Rebuild the HTML preview or PDF proof after replacement and inspect
renderer fallbacks as needed. File admission is not cross-format conformance.

**Remove selected image** requires confirmation. Removal leaves source references
in place, so those references may subsequently produce missing-image fallbacks.
The original local file is never modified or deleted. Source undo cannot restore
resource bytes; download an explicit portable backup before removing resources
that may be needed again. No resource is inferred to be unused or pruned silently.

## Atomicity, limits and output invalidation

`collection.changeImages(changes, expectedRevision)` accepts one through 128
changes to existing bindings. Each plain data record contains `destination` and
either `bytes: Uint8Array` or `remove: true`. Batch paths must be unique; unknown
paths, ambiguous records and property accessors are rejected. The method does not
add or rename images, and the original append-only import API remains unchanged.

The entire final image set must fit 32 MiB. Admission is independent of batch
ordering: removing or shrinking one binding can fund another replacement, and
same-size replacements work at capacity. No-op copies keep both revision counters
and emit no change notification. A changed batch is installed in one observable
revision and advances the render-configuration revision as well. Existing preview,
proof, export and other collection subscribers therefore retire stale resources.

Caller-owned fixed, non-shared Uint8Array ranges are copied before installation.
Shared, detached, resizable or spoofed buffers are rejected. Custom array
iterators, byte getters, slice methods and typed-array species are not used.
The collection rechecks the expected revision after descriptor admission so
reentrant host code cannot overwrite newer collection state. These are logical
admission/retention limits, not a whole-process heap bound.

The controller captures pending source edits through the existing source owner,
then fences file reads and asynchronous confirmations against the collection
revision, editor checkpoint, selected destination and File identity. Silent UI
changes without input events are checked too. Cancellation, import/composition,
source changes, resource changes, suspension and disposal retire pending work.
File/confirmation waits share a 30-second deadline. Native file reads already
started cannot be interrupted, but their late success or failure is observed and
cannot install after cancellation. Native confirmation dialogs are browser-owned.

Successful changes feed existing PDF/EPUB/site captures and portable backups.
Source-only projects and local-library saves continue to exclude resources.
Suspension clears selected Files; the publisher's existing lifecycle separately
revokes installed image and font access. Nothing is saved or uploaded automatically.

## Verification

Run `node --test wasm/tests/book_image_*.test.mjs` for collection, controller and
bootstrap/package regressions. Run
`CHROMIUM=/usr/bin/chromium python wasm/book_images_browser.py` for browser
interactions, real file selection, native confirmation and portable downloads.
The browser harness requires Playwright and retains a fresh temporary directory.

The tests cover source/settings/font preservation, exact byte ownership,
final-set budgeting, atomic rejection, stale/cancelled operations, no-op behavior,
portable round trips and startup/lifecycle wiring. Node DOM and source-host
adapters are explicit. The browser uses production collection/controls with an
explicit source-host adapter, not the full workbench bootstrap. Neither test
route validates native image decoding, generated WASM, or PDF visual quality.
The existing native and package gates are unchanged except for adding these
regressions and shipping the new controller/documentation.
