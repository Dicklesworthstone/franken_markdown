# Publish a complete edition from the book workbench

The **Publish a complete edition** panel in `demo/book.html` prepares a selected
PDF, EPUB and/or HTML-site ZIP together. Select the formats, choose **Prepare
selected edition**, then use the individual download links after the entire
edition succeeds. The files are named `book.pdf`, `book.epub` and `book-site.zip`.
No automatic download, disk write, permission request or resource fetch occurs.

The panel uses `createBookPublicationWorker`, not three independently captured
single-format exports. The existing source host first captures the active editor
and applied settings. The collection's production snapshot supplies chapter order,
include-only sources, images and supplied font roles. The publisher captures that
input and reuses its retained native book for every selected format. It owns a
separate client: edition cancellation does not cancel ordinary exports, previews,
PDF proofs, inspection or font preflight.

Every selected native output must succeed. Receipts, MIME types, source-length
agreement and aggregate Blob size are checked before any download URL is created.
URL creation is itself staged: failure releases the whole staged set. The panel
shows the complete set together, never successful early formats from an otherwise
failed edition. The publisher has one two-minute deadline and a combined 128 MiB
output ceiling. These are logical limits, not a browser heap bound.

## Source ownership and lifecycle

The full source-host checkpoint and collection revision are checked after
publication, throughout URL staging, and immediately before normal or auxiliary
link activation. Format selection is also part of the checkpoint, including
programmatic checkbox changes with no event. Source, resource and settings edits
invalidate every link and pending edition. Raw input is observed before other
controls can reject a half-edited path or setting, so invalid input cannot leave
an old output presented as current. Unapplied control drafts conservatively
invalidate links too, but only applied settings enter the edition.

Pending imports and text composition block preparation. Composition in any
workbench field is tracked through the final input task, not only the chapter
textarea. Source is never replaced by the edition, and editing stays available
while it runs. Cancellation rejects promptly even if a custom publisher endpoint
ignores its abort signal; late completions cannot publish or overwrite a newer
edition's state.

**Cancel / discard edition** releases ready URLs as well as pending work.
Suspension cancels and discards everything; resume never automatically renders or
reacquires resources. Disposal removes event/subscription/timer ownership and
permanently releases the private publisher. No handles or source snapshots are
persisted by this feature. An old detached link cannot revoke a newer edition.

Published editions are not editable source-project saves or portable backups.
Those existing actions remain separate. Returning all outputs together is an
in-memory transaction; separate browser downloads are not a transactional disk
save, and preparation does not establish that any file reached disk.

## Validation

```sh
node --test wasm/tests/book_edition*.test.mjs
python wasm/tests/book_edition_browser.py --chromium /path/to/chromium
```

The implementation run passed **41 Node tests** and **31 opaque-DOM Chromium
checks**, with zero page errors. The normal HTTP browser test was blocked before
its first check; actual disk downloads are not part of that pass count.

The Node tests execute the complete edition session through normal ESM imports,
including success/failure integration with the production publication coordinator.
The source codec, transport and native format outputs in these tests are explicit
fixtures; they are not replacement renderers. Packaging tests execute both actual
copy loops in isolated source fixtures and import/execute the copied session and
controls modules. A real ESM VM evaluates the unchanged application entry with
explicit component bindings to check integration and lifecycle order.

Chromium policy blocked the normal HTTP fixture navigation in the implementation
environment. It was not bypassed. The additional diagnostic command below runs
in opaque `about:blank`, rebinds only the controls module's static dependency
specifier to an in-memory Blob module, and executes unmodified implementation
bodies. It checks real DOM events, controls, Blob ownership, cancellations and
lifecycle, but **not real-origin startup or actual file downloads**:

```sh
python wasm/tests/book_edition_browser.py --opaque-dom
```

Native/WASM rendering, the real worker entry, the production collection codec in
a complete workbench, real-origin downloads, and full package/parity gates remain
unqualified in this environment. Rust/Cargo, RCH, DSR and br were unavailable.
No Actions job, native build, or Beads mutation is claimed. The existing build,
parity and size-budget gates are unchanged apart from copying the new feature
files and running its Node regression suite.
