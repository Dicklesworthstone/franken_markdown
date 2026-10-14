# Save a book's source files to a local folder

The book workbench's **Connected source folder** panel saves chapters and
include-only sources as ordinary UTF-8 files beneath a directory you explicitly
choose. This closes the import/edit/write-back workflow for multi-file books;
it does not convert a directory into a project database or mirror the filesystem.
The renderer and its native/WASM interfaces are unchanged.

## Review, connect, then save

First import the source folder using the existing picker, reopen a source project,
or author chapters and include-only files in the workbench. Their relative book
paths become the exact candidate disk paths. **Review / reconnect source folder**
opens the directory picker and reads only those paths. It does not enumerate
unrelated files, import disk content into the editor, create files, or write bytes.

An in-panel review lists every path as `unchanged`, `replace` or `create`, with
its outgoing UTF-8 byte count. Existing differing content is an overwrite
candidate, not an implicit merge. Inspect the complete list and choose
**Connect reviewed source paths**, or cancel. Connecting records the approved
baseline without saving anything. A changed disk file during the review refuses
the connection rather than accepting a different baseline silently.

After editing, choose **Save source files to folder**. The browser must grant
write permission, then the workbench presents a second explicit review. Only
**Write reviewed source files** starts writes. Each changed file is staged,
closed, and read back byte-for-byte; unchanged files are not opened for writing.
New sources and missing parent directories are created only during an approved
save. An empty new source still creates its file. BOMs, CRLF, LF, CR and Unicode
remain exactly as captured by the existing source host; malformed Unicode is
rejected rather than silently replacement-encoded.

Folder saving captures the complete source tree plus the selected raw editor
path/text without calling project export or applying publishing drafts. Valid
source can be saved even while unrelated publishing settings are unfinished.
Undispatched editor input is included. An unchanged textarea's LF-normalized view
of imported CRLF/CR text retains the original source bytes, including its BOM.
Actual source edits use the editor string. Invalid source Unicode/paths/budgets
are still rejected before opening a picker or requesting writes.

Connecting and saving do not change the collection, source history, publishing
settings or resource authority. The explicit external refresh described below
uses the host's existing source-only transaction; normal history, preview and
autosave subscribers can react to that source revision. Neither operation starts
a renderer or acquires images/fonts itself.

## What is and is not saved

The files contain source text only. Chapter order, chapter/include roles,
publishing settings and resource bytes are **not** serialized into the folder.
Keep a source-project JSON file for order/roles/settings, or a portable backup for
supplied images and fonts. Connected project-file saving and its Ctrl/Cmd+S
shortcuts remain independent; folder saving is an explicit separate action.

A changed source path set requires fresh folder review. Reordering the same paths
does not. Removing or renaming a source in the workbench never deletes, moves or
renames its old disk file: a renamed source is a new candidate path, and the old
file is left alone. Unlisted files are never enumerated, overwritten or deleted.
There is no automatic filesystem watcher, within-file merge, implicit reload,
or folder autosave. External refresh is always an explicit reviewed action.

Portable path checks reject traversal, URL-like or reserved names, case/Unicode
aliases and file/directory prefix collisions. Actual file-handle identity checks
also reject multiple source paths resolving to one file. Existing disk sources
must be valid UTF-8 and within the byte limits before they can be approved for
replacement. The host still enforces its existing chapter/include role policy.

## Review external edits without replacing the project

Choose **Review external source changes** to compare each connected path's current
disk bytes, the last reviewed/verified disk baseline, and the live editor source.
No directory enumeration, new-path discovery or write-permission request occurs.
The per-file action list is not a text diff:

- `reload`: only the disk changed; that whole source may be imported.
- `keep-editor`: only the editor changed; its text is retained, not overwritten.
- `converged` / `unchanged`: both versions agree; no source rewrite is needed.
- `conflict` / `missing`: divergent edits or a formerly present disk file's
  removal stop the ENTIRE refresh. The list identifies every conflicting path.

Resolve conflicting versions explicitly, then review again. A file absent since
connection remains an unsaved local source; refresh does not create or remove it.
There is no line-level or syntax-aware merge and no automatic choice between
conflicting versions. Source additions/removals still use ordinary import/editing
and a new folder review. Pending path/role drafts must be finished first so an
external text refresh cannot silently discard structural edits.

**Apply reviewed disk-only changes** rechecks every reviewed disk file's bytes
and identity, including unchanged files, immediately before applying one complete
source-only transaction. Atomic-save editors may replace a file's identity;
refresh can adopt that explicitly reviewed replacement only if it is unchanged
at the recheck. Local raw source edits are included in the same transaction.
Chapter order, include roles, current settings and resource authorizations remain
owned by the existing book. A cancelled/stale review leaves source and comparison
baselines untouched. A successful refresh updates the disk baseline, so a later
explicit save writes only the remaining local differences.

Only the in-memory source transaction is atomic. The disk snapshot is not a
filesystem-wide lock: other applications can race its final reads. Refresh never
writes disk bytes and does not claim the filesystem cannot change again.
An uncertain or incompletely aborted earlier disk write must still be resolved
through explicit folder review; refresh cannot silently clear its save receipt.

A failed source transaction with no mutation is retryable. If the host mutates
source then fails, returns an invalid receipt, or loses project/folder ownership,
`DIRECTORY_REFRESH_UNCERTAIN` reports that source may already have changed and
retires the old folder binding. Inspect the current book before reconnecting;
there is no pretend rollback or automatic replay.

## Conflicts, cancellation and partial saves

Before opening writers, the session compares the complete approved source set
with its recorded disk bytes and file identities, including unchanged sources.
After staging all changes it rechecks that entire set before the first close.
Each remaining target is checked again just before its own close. A renamed,
deleted, newly appearing, replaced or externally edited file refuses further
writes. Disk-only edits can be reviewed through external refresh. Divergent edits
need an explicit version choice; uncertain writes require review/reconnection.

**Several file closes are not an atomic transaction or an OS-wide lock.** An
external application can race the last comparison and close. The browser's
staged writers protect individual file updates, not the whole book. Read-back
verifies each captured file at that moment, not permanent ownership of the disk.
Do not treat a successful receipt as protection against later external edits.

A save receipt identifies every intended changed file as `verified`, `not-saved`,
`uncertain`, `created-unverified`, or `abort-unverified`. If a close fails or read-back does not match,
the session reports uncertainty and blocks blind retries. A rejected retry does
not erase that receipt. Already verified files are not rolled back; unclosed
streams are aborted. New empty files or directories can remain after failed or
cancelled creation. Directory-creation attempts are included in the receipt.
An unconfirmed abort is reported explicitly and also requires fresh review.
No cleanup/deletion is attempted because it could destroy another editor's work.

Editing, changing project identity, disconnecting, importing, or entering source
text composition retires pending approval or uncommitted writes. A close already
in progress may finish; its captured bytes are verified and reported without
replacing newer editor input. If a later file is stopped, earlier verified files
remain in the partial-save receipt. When safe to continue, a later explicit save
writes only the remaining differences against the updated verified baselines.

Folder handles and approvals stay in memory only. Reopening any project, including
one with byte-identical source, disconnects through the collection's project
identity epoch. Page suspension/disposal also disconnects and retires approvals;
returning to the page never renews permission or saves automatically. The panel
warns before leaving with a pending operation or unverified folder changes.

## Bounds and verification

The source-folder session accepts at most 256 sources, 4 MiB UTF-8 per source,
1024 UTF-8 bytes per relative path, and 16 MiB combined source/path bytes. The
selected disk baseline independently uses the same per-file and aggregate
budgets. Refresh also validates the combined local/disk result before approval
or source mutation, since two independently valid sets can exceed a merged bound. These are logical content bounds, not browser heap or filesystem quota
guarantees. Source contents are not normalized for comparison.

```sh
node --test wasm/tests/book_directory_*.test.mjs
```

The session and controller execute through normal ESM imports. Tests replace only
the browser handle/permission/writer boundary and, for the UI tests, the DOM and
source host. File/Blob and text encoding execute in Node. Package tests copy the
complete two-module runtime closure, import it unchanged, and check the manifest,
both assembler lists, test gates and bootstrap lifecycle wiring.

These tests do not establish real-browser picker, permission, focus/IME or
filesystem acceptance; generated-WASM rendering; native/WASM parity; or a complete
package build. Run the repository's prescribed DSR/RCH build and browser acceptance
gates before claiming those properties. Browsers without directory-picker support
retain the existing imports, project-file downloads and portable-backup paths.

The API is browser-side orchestration, not a second renderer. Its implementation
is `demo/book_directory_session.mjs`; the workbench uses
`demo/book_directory_controls.mjs`. The session receives only a source snapshot
callback, an editor checkpoint, a busy predicate and an observer. It never receives
a project installer or resource store. Approval callbacks see frozen path/action
metadata, never file handles or resource bytes.

`session.refresh(confirm, apply)` is additive. The synchronous `apply(files,
expectedStamp)` callback must atomically replace only source text and return the
new `read().stamp`. It is invoked once only when disk-only source changes exist.
The session verifies the resulting complete source set and ownership before
adopting it. Promise-returning callbacks are not atomic and are refused as
uncertain; do not pass an async source installer. The workbench supplies its
existing `controls.applySources` transaction and preserves each source's role.
Older source hosts without that method keep saving but cannot refresh.

The capture/refresh change was verified with 117 executed Node tests: 37 existing
session tests, 17 controller tests, 27 source-capture cases, 27 refresh tests
(including 200 seeded three-way comparison scenarios), and 9 refresh UI cases.
These use the complete production session/controller and explicit host/DOM/disk
adapters. They do not establish actual browser filesystem acceptance or native
rendering. Package assembly and the full repository suite were not executed for
this change; the existing two-module package closure and test wildcard remain.

