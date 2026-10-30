# Local book library and revision recovery

Open `demo/book.html` in the assembled browser package. **Save this book locally**
creates a named source project in this browser. Editing, publishing, and downloading
source projects still work when IndexedDB is unavailable or a save fails. There
is no implicit document restoration, server upload, or Web Storage fallback.

## Save and continue working

Give the book a local name, then save. The local name is separate from the book's
published title. A successful save displays the acknowledged revision. **Save a
separate copy** creates another independent library entry and makes it the current
save target. Selecting a different saved book in the list does not redirect saves.

**Autosave this named book** is an explicit, per-session opt-in, available after
saving or opening a book. It coalesces edits with a one-second delay and pauses
during source text composition. Edits made while a save is in progress remain
dirty until a later save acknowledges their own snapshot. Validation, conflict,
and storage failures turn autosave off rather than discarding source or silently
saving older valid model settings in place of invalid current input.

Autosave is off again after project replacement, recovery, page suspension, and
reload. A transaction already started before suspension may still complete. The
UI never treats preparation, a request-success callback, or a pending promise as
a completed save. Only the IndexedDB transaction completion supplies the receipt.
On teardown or timeout, refresh recovery before retrying an interrupted operation.

A conditional unsaved-change warning supplements explicit saves; it is not a
mobile close/crash guarantee. Download source projects and keep original images.

## Recover without overwriting another tab

Refresh the saved-book list, choose a book and a retained revision, then select
**Open selected revision**. Confirmation is mandatory. The editor's source,
selection, raw settings, and local name must still match the recovery checkpoint
after reading and confirmation. Otherwise recovery refuses to replace the editor.
Successful recovery revokes all old image authority and prepared downloads.

Opening a historical revision changes the editor, not the stored head. Saving it
creates a new revision, subject to the current head's compare-and-swap check. The
newer snapshots remain available until ordinary retention removes them.

Every update and deletion must provide its last observed revision. Two tabs
saving from the same revision cannot both win. A conflict preserves the editor
and the other tab's stored version, disables autosave, and offers two explicit
paths: reopen the latest stored book, or save the current editor as a separate
copy. Refreshing metadata alone never advances the current save token.

Reopening a downloaded project detaches it from the previous library save target.
Even a late save receipt from the old document cannot reattach the imported one.
Starting a new empty book leaves saved library entries untouched. Deletion requires
confirmation and the selected entry's observed revision; it deletes that saved
book and its retained history, not the current editor or downloaded files.

## Stored data and limits

Storage contains schema-version-1 source projects: chapter paths, exact source
strings, ordering, and publishing settings. It preserves original BOM and line
endings. Images, fonts, file handles, workers, and access grants are not serialized.
Reauthorize image files after recovery. Invalid Unicode, paths, or settings remain
editable but cannot overwrite a valid stored snapshot. Individual chapter downloads
remain the recovery route when a project cannot pass validation.

The local library admits at most **32 books**, at most **five snapshots per book**,
and **128 MiB of aggregate UTF-8 snapshot JSON**. This is an application-level
payload budget, not a promise about physical IndexedDB overhead or browser quota.
A save may prune its own oldest revisions to satisfy count or payload limits; it
never evicts another book automatically. Refused updates do not prune any history.
The browser can refuse quota independently. Local data is unencrypted, shared by
same-origin tabs, and may be cleared or evicted. This is recovery, not a backup.

The database is `franken-markdown-book-library-v1`, schema version 1. Metadata and
source payloads occupy separate stores so listing books does not load all chapter
bodies. Entry revisions, snapshots, pruning, and aggregate accounting change in one
read-write transaction. No renderer or third-party storage dependency was added.

## Verification boundaries

```sh
node --test wasm/tests/book_project.test.mjs \
  wasm/tests/book_library_session.test.mjs \
  wasm/tests/book_library_controls.test.mjs \
  wasm/tests/book_library_package.test.mjs \
  wasm/tests/book_controls.test.mjs
```

Node tests run the production project validation, sessions, and UI controllers
with explicit store, DOM, worker, and timer doubles where needed. File/Blob and
object-URL checks use native Node implementations. Package checks inspect declared
runtime inventory and copy lists. They do not prove native database transactions,
physical durability, real-browser composition, or rendering.

For native storage acceptance, serve the repository's `wasm/` directory over HTTP
and open `tests/book_library_store.browser.html`. This separate harness exercises
real IndexedDB, independent connections, conflicting saves/deletes, exact source
reopen, revision retention, capacity rollback, and interrupted transactions without
storage substitutes. It uses isolated `book-library-test-*` databases, not the
production library. The harness does not exercise Rust/WASM rendering.
