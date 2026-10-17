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
and the other tab's stored version and disables autosave. Reconcile their source
changes with **Review saved source changes**, reopen the latest stored book, or
save the current editor as a separate copy. Refreshing metadata alone never
advances the current save token.

Reopening a downloaded project detaches it from the previous library save target.
Even a late save receipt from the old document cannot reattach the imported one.
Starting a new empty book leaves saved library entries untouched. Deletion requires
confirmation and the selected entry's observed revision; it deletes that saved
book and its retained history, not the current editor or downloaded files.

## Reconcile concurrent source edits

After saving or opening a named book, select **Review saved source changes**.
This always reads the **current save target**, not another selected library row.
It compares the last observed saved source tree, the current editor (including
uncaptured source/path/role input), and the latest saved source tree. Review shows
every path action, proposed order, and any choices needed. Select **Apply reviewed
source merge** only after resolving every conflict, or cancel to keep both sides
and the existing save token unchanged. No choice is selected by default.

Independent changes to text, chapter/include roles, additions and removals are
combined. Source conflicts allow the complete editor version, the complete saved
version, a complete manual replacement, or individual conflicting regions.
Region choices keep all independent edits elsewhere in that source. An untouched
manual textarea preserves the original BOM and line endings despite the browser's
normalized display. An empty manual replacement empties a source; it does not
remove it. Excerpts are inert text, at most 1,024 Unicode scalars each, and are
labeled as excerpts rather than complete diffs. Resolution uses complete strings.

Deleting a source on one side while editing, changing its role, or reordering it
on the other requires a membership choice. Removing a source here removes it from
the editor only, never from disk or stored history. Renames are explicit
removal/addition identities: no link rewriting or guessed rename matching occurs.

Reading order merges relative precedence rather than absolute indices. Independent
moves and anchored additions survive. Concurrent unrelated insertions at the same
position use exact path order for a deterministic tie. Incompatible precedence or
a cycle requires an explicit editor-order or saved-order choice; that order is
used first, with other-side-only surviving sources appended in their own order.
An order choice cannot discard a source. Membership decisions may make a previously
required order choice unnecessary; the review conservatively still requires it.

**This is a source-only merge, not a full-project merge or a save.** Current
publishing settings, unapplied drafts, local library name, images, fonts and
resource permissions stay current. Incoming settings are explicitly not imported.
The existing source-tree host applies the complete result as one transaction.
Reconciliation does not call project capture/recovery, request permission, start a
renderer itself, or write to the store. Existing source subscribers can react
normally; an independently enabled automatic preview policy may render the changed
book. A subsequent **Save this book locally** saves the full current project,
including the current settings, against the newly observed
saved revision. Even an unchanged-source approval leaves a conservative unsaved
indicator. Autosave remains off until explicitly enabled again.

### Stale reviews and common ancestors

The session rechecks the full editor source tree, checkpoint, project identity,
local name and lifecycle around asynchronous reads and approval. A final read
must still have the reviewed revision, name and exact source bytes/roles/order.
Newer input, imports, project replacement (even identical source), suspension or
disposal cannot give an old review permission to overwrite the editor. Source
composition cancels the review; composition inside manual conflict text blocks
approval without discarding those choices. Cancellation releases the visible
review. No storage transaction is held open during human review. A later write
from another tab is still caught by the next save's existing revision check.

The optional source-capable session retains one bounded, immutable common source
tree at the exact acknowledged save revision. It records the captured saved text,
not newer edits typed while that save was pending. After reconciliation, the
ancestor is the **incoming saved tree**, not the still-unsaved merged result.
This distinction allows subsequent reconciliation to preserve retained local edits.
Separate-copy saves and project detachment change or retire that ancestor together
with their save binding.

Opening a historical snapshot keeps the observed head as the next save token.
Its common ancestor is loaded lazily from that exact observed head revision,
not invented from the older displayed snapshot. If it has been pruned before
capture, merging refuses and a separate copy remains available. An already
captured ancestor can survive ordinary stored-history pruning while the session
is open. It contains source only, not renewed asset authority or a persistent
backup. Missing/corrupt snapshots cannot become guessed merge bases.

A host must install sources synchronously and return a verifiable new checkpoint.
A rejected, unchanged transaction remains retryable. Partial installation, an
asynchronous host, changed identity or an unverifiable result reports
`LIBRARY_RECONCILE_UNCERTAIN` and retires the old save binding. It never reports a
successful save or attempts a destructive rollback.

### Reconciliation API and bounds

The optional `sources` argument to `createBookLibrarySession` supplies
`read() -> {files, stamp, identity}`, `apply(files, stamp) -> newStamp` synchronously,
and an optional `isBlocked()`. Reads must not mutate the editor. Identity identifies
the source project; apply must preserve it and all current settings/resources.
`session.reconcile(approve)` passes frozen review data to the callback and expects
an array with exactly one decision per conflict, or `false` to cancel. Legacy
hosts without this capability retain their original library functionality.

`book_source_merge.mjs` exports `captureBookSourceTreeForMerge` and
`planBookSourceTreeMerge(base, local, incoming)`. The latter provides frozen
`changes`, `conflicts`, `order`, and a reusable `resolve(choices)` method. Choices
are `{index, choice: "local" | "incoming"}`. Source conflicts additionally accept
`{index, choice: "edited", source}` or `{index, choice: "regions", regions}`;
regional entries use the existing text planner's `local`, `disk`, or `edited`
choices (`disk` denotes incoming saved text). Incomplete, duplicate, accessor-backed,
invalid-Unicode or oversized decisions are rejected before any source installation.

Each input and complete result is bounded by 128 chapters, 128 include-only files,
4 MiB per source, 1 KiB per path, and 16 MiB combined UTF-8 source/path bytes. The
existing two-million-unit diff allowance and 100,000-line allowance are shared
across the tree's text merges, with at most 256 exposed conflict regions in total.
Exhausted text/region budgets retain explicit whole-source/manual choices, not a
truncated or guessed merge. The order graph has at most 768 candidate paths from
the three bounded inputs. These are logical content/work limits, not a promise
about wall time or physical browser heap usage. No runtime dependency, production
package path, database schema, build command, or renderer was changed.

## Stored data and limits

Storage contains validated source projects: chapter/include paths, exact source
strings, roles, ordering, and publishing settings. It preserves original BOM and line
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

The source reconciliation addition was checked with:

```sh
node --test wasm/tests/book_directory_merge_tree.test.mjs \
  wasm/tests/book_library_reconcile.test.mjs
```

**81 new focused tests passed:** 26 source-tree planner cases and 55 library
session/workbench cases. The planner includes 500 seeded structural trials in
both directions and a 10,000-line sparse-edit case. Session/controller tests cover
repeated merges, historical/pruned ancestors, cross-tab revision races, exact
readback, unchanged and uncertain host transactions, empty books, manual and
regional choices, draft preservation, cancellation and lifecycle fencing.

The integration harness evaluates the complete unchanged production ESM modules
for the controller, library session and shared merger in one VM realm. Its error
factory, DOM, source host and revisioned storage are explicit adapters. This run
does **not** exercise native IndexedDB, the production collection/source-host/undo
controller, real-browser IME/focus/accessibility, or Rust/WASM rendering. The older
library suites, native storage harness, assembled-package gates and full repository
suite were not rerun in this authoring environment. Syntax and whitespace checks
passed; the unchanged renderer, database and build policies are not qualified by
those checks.
