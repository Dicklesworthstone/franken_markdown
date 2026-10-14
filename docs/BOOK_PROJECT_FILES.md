# Connected source-project files

The book publisher now has a **Connected source-project file** panel beside its
publication controls. It can open one local JSON source project, save back to
that file, save to another chosen file, reload explicitly, or disconnect while
retaining the current book. The ordinary file inputs, source downloads, local
library and portable backups remain available independently.

Ctrl/Cmd+S saves the connected source project, or opens Save as when disconnected.
Ctrl/Cmd+Shift+S chooses another destination. Where direct file picking is not
available, a disconnected Ctrl/Cmd+S prepares the ordinary source-project
**download** instead; it does not pretend to save a file. Composition and busy
imports block file operations. File access starts in the initiating user action;
there is no automatic permission request, filesystem scan or background autosave.

## What is saved

A connected file uses the existing source-project serializer: chapter and
include-only text, paths, roles, order, and applied publishing settings. It does
**not** contain image bytes, supplied fonts, permissions, local-library identity,
undo history or unapplied settings drafts. Source text, including Unicode and
line endings, follows the same admission/serialization rules as source downloads.
A connected file's outer UTF-8 BOM is preserved on subsequent saves.

Use a portable backup to retain authorized images and supplied fonts. Attempting
to open that distinct resource-bearing format through source-project Open goes
through the existing source-project validator, not an implicit resource restore.
Open and Reload require confirmation before replacing the book and revoking its
current resources. Save and Save as do not replace source or authorize resources.

## File identity, conflicts and completed saves

Handles remain private to the current page session. The file session retains an
exact disk-byte baseline separately from the canonical source-project identity.
Consequently whitespace-only, BOM-only and same-sized external changes still
refuse an in-place save. Older valid source schemas may normalize to the same
semantic project without making their raw disk bytes interchangeable.

Each write requests an exclusive staged browser stream and compares the disk
again after obtaining the stream, after writing the stage, and before close.
The stage is aborted when source changes before close. The exact written bytes
are read back after close before the captured revision is marked saved. A denied
permission, unavailable file, competing writer or storage failure preserves the
editor and produces a distinct error. Conflicts require an explicit reload or a
different destination; Save as checks file identity so aliases cannot bypass a
conflict. Nonempty new destinations need explicit overwrite consent.

This is **not an OS-wide compare-and-swap or lock guarantee**. Another application
can race the last comparison and close. A failed close/read-back is reported as
uncertain, not saved, and is never silently retried. Edits after close has begun
remain in the editor; only the captured revision can become the disk baseline.
A late completion cannot reconnect a detached project or replace a newer book.

## Workbench integration and lifecycle

The application supplies its actual `normalizeBookProject`, `readBookProject`
and `serializeBookProject` functions. Raw textarea/settings input is captured
through the existing source controller before starting I/O; its full checkpoint
fences subsequent suspended work. A successful file open uses the existing
source-project replacement transaction, detaches unrelated local-library
identity and resets source history. No renderer or worker is created to save.

Later workbench components receive a descriptor-preserving source-controller
facade. Its `replaceProject` route disconnects file ownership on library/new-book
recovery, even for identical source. A host that changes the book and then fails
updating the view also loses the old file authority; an unchanged rejected
replacement does not. Ordinary source/setting edits keep the file connection.
Successful source/portable file-input imports disconnect through their existing
application callback, before other observers run.

The panel distinguishes disconnected, edited, saved, conflict and uncertain
states. Change notifications conservatively mark a connected project edited
until another verified save, including edits later undone or non-source
collection changes; this does not imply resource bytes are in the file. An
unsaved connected project or pending file operation requests a before-unload
warning. Browser policy decides whether to show it. Suspension disconnects the
handle, and resume does not silently reacquire permission. Disposal removes DOM
and collection listeners and retires pending writes without clearing book text.

The source-project JSON limit is 64 MiB, alongside the existing source/schema
limits. This bounds admitted file payloads, **not peak JavaScript memory**:
canonical strings, captured byte arrays, staged data and read-back buffers can
coexist. Project serialization is cached by collection revision, not rerun for
every state-label refresh. Handles, disk paths and source bodies are absent from
the public state object and never enter workers or persistent library records.

## Verification and remaining qualification

```sh
node --test wasm/tests/book_file_*.test.mjs
python wasm/tests/book_file_controls_browser.py
```

The implementation run passed **28 Node tests**: 25 file-session tests and three
packaging checks. File tests use real temporary files, native File/Blob and a
Node filesystem handle adapter that stages, syncs and renames actual bytes.
Controlled hooks exercise external edits, suspended confirmation/picking, stage
aborts, edits during close, denied permission and uncertain read-back. This does
not verify a browser's native file picker or exclusive-writer implementation.

The **20 Chromium DOM checks** passed with zero page errors and zero network
requests (one in-memory Blob module load). Complete production modules execute
without changing their implementation bodies. Explicit project-host/codec
contract fixtures exercise accessible controls, source-download fallback,
shortcuts, composition, live getters, pending-operation cancellation, recovery
boundaries and lifecycle. Pickers only cancel or remain pending; no virtual
writable handle is used to imitate successful browser I/O.

Managed browser policy refused local-page navigation; it was not bypassed.
DOM checks run in opaque `about:blank`, not a manufactured filesystem origin.
Native picker/permission behavior, full workbench startup with the production
codec and generated WASM, native rendering and complete package builds still
need their separate qualification. Cargo, rustc, rch, DSR and br were unavailable
in the implementation environment; no Rust gate, Actions job or Beads mutation
is claimed. Package metadata checks verify both assembly lists, and a copied
feature module closure imports and executes normally; that is not a full WASM
package-build result. Runtime dependencies, versions and size budgets are unchanged.
