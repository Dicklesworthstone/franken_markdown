# Revise a book's Markdown source

The assembled `demo/book.html` publisher includes **Find and replace across
chapters**. This is a source-authoring operation, not rendered-text search or a
second Markdown parser. It works without a loaded WASM engine or export worker.

## Format the selected source

The source editor also includes Bold, Italic, Strike, Inline code, Code block,
Heading 1–3, Bullets, Numbered list, Task list and Quote buttons. Select source
text, then choose a command; Ctrl/Cmd+B, Ctrl/Cmd+I and Ctrl/Cmd+Shift+X work in
the source editor. Commands apply equally to chapters and include-only files.
Line commands affect complete selected lines. Code commands insert literal
delimiters; they do not infer or remove existing Markdown syntax.

Each accepted command is one ordinary edit in the existing book-wide history.
Undo/Redo below restores it. Preview, autosave and prepared-download invalidation
observe the same source revision as typing; formatting starts no worker or save
of its own and preserves current source roles, images, fonts and settings.
Imports, source/path composition, readonly/disabled editors and empty books block
commands. A chapter or project switch during focus or `beforeinput` refuses the
stale command even when the replacement has identical text. A cancelled
`beforeinput` leaves source and undo history unchanged. Page suspension removes
the toolbar; persisted restoration installs one fresh owner.

The commands reuse the single-document editor's
[source formatting rules](demo/SOURCE_FORMATTING.md), including Unicode and size
limits. Fourteen Node integration/package checks cover this wiring, and the
standalone browser runner exercises the actual unmodified book HTML and modules
with native textarea/button/keyboard behavior. It can run without a generated
WASM artifact; it does not qualify renderer output, real IME sessions, OS file
pickers or actual back/forward-cache restoration. Its lifecycle checks dispatch
page transition events explicitly.

```sh
node --test wasm/tests/book_source_formatting*.test.mjs
# Use an installed Playwright and Chromium; neither is a package dependency.
FMD_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs \
FMD_CHROMIUM=/path/to/chromium \
node wasm/tests/book_source_formatting.browser.mjs
```

## Find source text

Enter literal text, choose the whole book or the current chapter, then select
**Find in source**. Results appear in reading order with chapter, line and column.
Open a result or use Previous/Next match to select its exact source range in the
editor. The results list shows 50 rows at a time; page controls reach the rest.
Search and navigation do not alter source or revoke images.

## Return after leaving the page

Page suspension retires pending source imports and confirmations, clears file
selections, and ends the editor's source/path composition state. Returning allows
new edits and imports immediately, even if an old file read has not settled or
the browser never delivered `compositionend`. Existing image and font access
must still be authorized again.

An old read may finish in the background. Its result cannot replace source,
renew resource access, clear a newer file selection, unlock a newer import, or
replace the current status. Retiring UI ownership does not promise that the
browser interrupted the underlying read. Resource revocation already prevented
late source installation; these lifecycle guards also preserve the resumed
editor's usability and operation ownership.

Nine source-host regressions cover retirement, overlapping source/path
composition, pending confirmations and old/new import cleanup. The browser runner
above also exercises delayed `File.arrayBuffer` results and interrupted
composition through the unmodified app. Its composition and page-transition
events are explicit fault injections, not real IME or actual back/forward-cache
acceptance. All original formatting checks remain in the runner.

Whole-project replacement also rechecks active composition immediately before
changing source. A project read, saved-book read, or replacement confirmation
that began earlier cannot replace the editor while source/path composition is
active, even when its text and checkpoint have not changed yet. Pending chapter
removal follows the same rule. Finish composing, then start the operation again;
the earlier approval is not replayed automatically. Existing source-merge
conflict and stale-review rules remain separate.

The browser runner now also executes the eleven native IndexedDB store checks
and the real workbench's save/open controls. Its recovery races delay delivery
of a completed native storage transaction, a file read, or an embedding host's
confirmation callback. These are explicit scheduling controls around the
unmodified app, not real-IME or operating-system picker acceptance.

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
