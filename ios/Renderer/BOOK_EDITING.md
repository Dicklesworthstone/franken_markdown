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

This is session-only history, not crash recovery, filesystem saving, or the
textarea's native typing undo manager. No keyboard undo shortcut is intercepted.
Use the optional local library or download a source project to retain work.
History contains source only, never image bytes, handles or renewed permissions.
Returning from page suspension starts with no old results, review or batch history.

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
