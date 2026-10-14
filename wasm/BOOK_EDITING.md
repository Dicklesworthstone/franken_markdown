# Revise a book's Markdown source

The assembled `demo/book.html` publisher includes **Find and replace across
chapters**. This is a source-authoring operation, not rendered-text search or a
second Markdown parser. It works without a loaded WASM engine or export worker.

Enter literal text, choose the whole book or the current chapter, then select
**Find in source**. Results appear in reading order with chapter, line and column.
Open a result or use Previous/Next match to select its exact source range in the
editor. The results list shows 50 rows at a time; page controls reach the rest.
Search and navigation do not alter source or revoke images.

## Review, apply, undo

Enter a literal replacement (empty means deletion), then review either the
selected match or every match in the chosen scope. Review shows the actual number
of changes, affected chapters, byte sizes and a before/after excerpt of the first
change in each chapter. Excerpts are not a complete diff. Review does not apply
changes or invalidate a prepared download.

**Apply reviewed changes** asks for confirmation and rechecks the exact editor,
query and replacement afterward. Typing during that confirmation, even without
an input event, cannot be overwritten. Changed or discarded reviews cannot apply.
The complete replacement is validated against all chapter/book limits before any
chapter is changed. Applying it emits one complete source revision. Current image
authorizations, settings, chapter names and reading order are preserved; existing
preview/export subscribers invalidate their stale outputs normally.

**Undo replacement** and **Redo replacement** operate on whole replacement
batches, not individual keystrokes. They retain at most ten batches within a
64 MiB aggregate before/after UTF-8 source budget, pruning oldest undo entries.
An ordinary source/settings/asset edit, import, project recovery or suspension
clears this history. Newer undispatched keystrokes are captured rather than
silently overwritten by undo. Chapter navigation and searching alone retain
history. A replacement after undo drops the redo branch.

This replacement-specific history is session-only, not crash recovery, filesystem
saving, or the textarea's native typing undo manager. It does not intercept
keyboard undo; the separate book-wide history below owns source-editor shortcuts.
Use the optional local library or download a source project to retain work.
History contains source only, never image bytes, handles or renewed permissions.
Returning from page suspension starts with no old results, review or batch history.

## Book-wide source and structure undo

The **Book-wide source and structure undo** toolbar records typing, source
transactions and structural edits in chronological order. Use its Undo/Redo
buttons, or Ctrl/Cmd+Z, Shift+Ctrl/Cmd+Z and Ctrl+Y in the source editor. Adding or
importing chapters and include-only files, removing them, renaming paths,
reordering sources and changing chapter/include roles no longer erase earlier
source recovery. Each observed structural transaction is reversible, including
removing the last chapter or returning to an empty book. Multi-file imports are
one source-tree transaction; resource grants in an import are not part of undo.

Replay installs the complete source tree as one collection revision and updates
the actual chapter selector and editor. A surviving selected path stays selected;
a deleted selection is clamped to the remaining sources. Empty books clear the
editor and disable source-specific controls without making redo unavailable.
Renames do not rewrite links or transclusion references, in either direction.

This history never restores publishing settings, image/font bytes, permissions,
file handles or local-library identity. A revoked resource remains revoked after
undo and redo. Current presentation settings and unapplied control drafts stay
current. Ordinary previews, proofs and exports observe the new source revision
and invalidate stale results through their existing subscriptions. History itself
starts no renderer, resource fetch, save or download. Separately enabled automatic
preview or autosave policies can still react to the source change normally.

Opening/replacing a source project, starting a new book, or restoring a portable
project establishes a new source-project identity **even for byte-identical
source**. Old undo/redo cannot cross that boundary. Suspension clears history;
resume starts at the current book without replaying or acquiring anything.
Pending imports and source/path text composition block recovery. An uncaptured
editor change or stale checkpoint refuses replay rather than overwriting input.
A rejected transaction remains retryable unless another edit/project switch has
already established a different source state.

History retains one current source snapshot plus at most 100 undo/redo entries
within a shared 16 MiB logical payload budget. Text edits retain replacement
deltas. Structural entries refer to unchanged source text by its position in the
opposite tree; a reorder or rename does not retain full copies of every chapter.
Added/deleted text, UTF-16 paths and 16 bytes per structural slot count toward the
budget. Older complete entries are evicted when needed. A transaction larger than
the budget creates an explicit recovery boundary, never a partial undo. These
limits exclude the current snapshot, engine string representation and object
headers, so they are not a browser heap bound or a saved backup.

Draft paths, duplicate/incomplete names and exact source strings remain editable
and recoverable. Source-tree replay bounds source/path bytes and counts but does
not silently normalize draft Unicode or newlines. Export and project-save
validation remain strict; a restored invalid draft must be repaired before
publication or saving. The existing `replaceSources` transaction used by search
still cannot change paths, order, membership or roles.

The history factory's structural option is additive:
`createBookSourceHistory(files, limits, { trackStructure: true })`. Legacy callers
keep source-only reset semantics by default. The workbench controller enables it
only when its host has `applySourceTree` and the collection provides a
`sourceProjectRevision` identity epoch. Neither API serializes resources.

## Literal and Unicode behavior

Search includes code, frontmatter and link destinations. Review carefully:
replacing Markdown syntax may intentionally change rendering or break a link.
There is no regular-expression mode. Regex metacharacters, `$&`, `$1` and
backslash sequences are ordinary text, not patterns or replacement expansions.
Actual multiline text is supported. Matches do not overlap.

Match case is on by default. Turning it off uses ECMAScript Unicode simple case
folding, not locale-sensitive linguistic matching or arbitrary length-changing
case transformations. No normalization merges distinct Unicode spellings.
Original UTF-16 source offsets are converted to the textarea's newline-normalized
view for selection. Displayed columns count Unicode code points, not grapheme
clusters; tabs count as one point. Imported BOMs remain source characters.

A newline in the query matches LF, CRLF or CR. Newly inserted replacement lines
use the first newline convention in each chapter (LF for a single-line chapter).
Unmatched source, including BOMs and mixed line endings, is left exactly intact.
Undo restores the original chapter strings, not a normalized approximation.

## Limits and verification

Search is explicit, synchronous and bounded by the workbench's 128 chapters,
4 MiB per chapter and 16 MiB total source/path budget. Queries are limited to
4 KiB UTF-8, replacement input to 64 KiB, and results to 10,000 matches. A larger
result set is rejected, never truncated and presented as a complete replace-all
set. Query, source and replacement reject malformed surrogate sequences. These
are logical content budgets, not guarantees about physical browser heap usage.

The Node suites exercise the production collection, publisher controller,
search/planning code and search controller. UI tests explicitly substitute the
DOM, confirmation, object-URL registry and export worker. File decoding and Blob
operations use native Node facilities. Package tests check the real manifest,
markup and assembly wiring. No generated Rust/WASM, native browser focus/selection,
IME, visual or screen-reader acceptance is implied by these tests.

```sh
node --test wasm/tests/book_source_search.test.mjs \
  wasm/tests/book_search_controls.test.mjs wasm/tests/book_search_package.test.mjs
```

For the structural-history implementation, **83 focused Node tests passed**:
20 existing source-journal cases, 21 existing source-controller cases, 21 new
structural-journal cases and 21 new collection/source-host integration cases.
The differential cases compare 7,500 mixed structural operations and 4,000 text
operations with full-snapshot oracles. Integration evaluates the complete,
unchanged ESM sources in one VM realm; native-bound option admission, font
storage, transport and the DOM are explicit adapters. Those tests do not qualify
real native/WASM rendering, real-browser IME/focus, or the full package/parity
build. No build commands, dependencies, versions or size budgets were changed.
The new tests use the existing history-suite wildcard in both package gates.

```sh
node --test wasm/tests/book_source_history.test.mjs \
  wasm/tests/book_source_history_controls.test.mjs \
  wasm/tests/book_source_history_structure.test.mjs \
  wasm/tests/book_source_history_tree.test.mjs
```
