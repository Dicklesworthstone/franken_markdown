# Supplied fonts in the book publisher

The rebuilt `demo/book.html` has a **Book fonts** section. Assign local TrueType
`.ttf` files to body regular, bold, italic, bold italic and monospace roles. Pick
one or several files, optionally enter integer weight pins from 1 through 1000,
and choose **Validate and assign selected fonts**. Blank pins use the renderer's
role defaults. Reselect a font file to change its pin; changing a draft without
selecting its file does not change an installed font.

The proposed batch is read, admitted and checked through a dedicated instance of
the existing book-PDF worker. A small document exercises all five style roles;
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
rendering engine is involved. TrueType collections, WOFF/WOFF2 and CFF/OTF are
not accepted by this workbench file picker.

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

Each role is limited to 8 MiB and the final set to 32 MiB. The final-set budget is
checked before reading selected files: replacing a role at capacity does not need
extra retained-store headroom. Selecting the same File for several roles reads it
once, while each role keeps its own weight and privately owned installed bytes.
These are admission/retention limits, not a whole-process memory guarantee.

File reads, asynchronous confirmations and validation share a two-minute host
deadline. The existing worker has its own two-minute execution deadline and
128 MiB output ceiling. **Cancel font operation** retires only this operation;
publication, HTML-site preview and PDF-proof workers are independent. Cancellation
settles the host promise even when an adapter ignores abort; the production book
worker terminates its synchronous Rust computation. File.arrayBuffer cannot be
interrupted after starting, so a late read is observed and discarded. Native
browser confirmation dialogs themselves are not programmatically dismissed.

Collection revision, editor checkpoint and selected-file/weight identity are
checked before installation, including silent changes with no input event. Edits,
imports, composition, changing a file/pin, suspension and disposal cancel obsolete
work. Retries are explicit. Suspending the publisher clears uncommitted File
selections; its existing collection lifecycle also revokes installed font/image
authorizations. Returning from the back/forward cache does not reload fonts.

The collection remains the single font owner. PDF/EPUB/site exports and explicit
portable backups already consume its font snapshots. Source-only project files
and local-library saves deliberately exclude font bytes. A portable restore
refreshes the inventory from the restored collection; it does not retain stale
picker selections. Nothing is saved, uploaded or persisted automatically.

## Integration and verification

`createBookFontAuthoring` is in `demo/book_font_authoring.mjs`. It owns a dedicated
book worker, uses the existing collection `setFonts`, `revokeFont` and `revokeFonts`
contracts, and exposes `assign`, confirmed `remove`/`clear`, cancellation,
subscriptions and lifecycle methods. `assign` accepts one through five
`{slot, file, weight}` selections. Its optional operation options are `signal`
and an `isCurrent()` host guard. The UI provides that guard for its raw drafts.
The panel/controller are in `demo/book_font_controls.mjs`; the publisher bootstrap
handles initialization failure without disabling other features. Both package
assemblers and the package manifest include these modules and this guide.

```sh
node --test wasm/tests/book_font_authoring.test.mjs \
  wasm/tests/book_font_controls.test.mjs wasm/tests/book_font_package.test.mjs
CHROMIUM=/usr/bin/chromium python wasm/book_font_browser.py
```

Node tests execute the production authoring session, controls and font store with
explicit collection/editor and transport/DOM adapters. Package checks load the
staged helper graph and execute the unchanged publisher bootstrap against named
constructor/lifecycle adapters. The Chromium harness uses real file inputs,
File.arrayBuffer, Blob module loading and Workers with an explicit protocol-only
font/PDF adapter, not the Rust font parser. It must not be reported as evidence
of native font support, generated-PDF fidelity, glyph coverage or full-workbench
acceptance. Run the generated-WASM and native rendering gates separately.
