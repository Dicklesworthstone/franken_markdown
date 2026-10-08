# Transactional book collection editing

The retained `book.js` API supports complete source-set replacement without
losing publishing settings, authorized font/image bytes or PDF page defaults.
This is an in-memory editing operation, not a filesystem mutation or a source
link-rewriting service. It requires a matching WASM build with
`FmdBook.replaceSources`.

```js
import { createBook } from "@franken-suite/franken-markdown/book";

const book = await createBook([
  { path: "intro.md", source: "# Introduction" },
  { path: "guide.md", source: "# Guide" },
], { title: "Manual", pageNumbers: true, expandIncludes: true });

try {
  const expectedRevision = book.sourceRevision;
  const report = book.replaceSources([
    { path: "guide.md", source: "# Guide\n\n{{#include parts/example.md}}" },
    { path: "appendix.md", source: "# Appendix" },
  ], {
    expectedRevision,
    includeSources: [{ path: "parts/example.md", source: "An example." }],
  });
  // intro.md is no longer selected; guide.md is first; appendix.md is new.
  // No file has been deleted or written on the host.
  console.log(report.revision, report.chapterCount, report.resourceCount);
  const publication = book.renderPdf();
  // publication.bytes remain owned after book.dispose().
} finally {
  book.dispose();
}
```

## Transaction semantics

`replaceSources(files, options)` receives the COMPLETE ordered chapter list and
COMPLETE include-only resource list. Omitted or undefined `includeSources`
means an empty resource list, not preservation of previous resources. Empty
chapter lists are rejected. Only `includeSources` and `expectedRevision` are
accepted in replacement options; presentation fields are not silently ignored.

The original expansion policy stays fixed. Parse-only books reject include-only
resources. Expanding books resolve all directives against the final submitted
graph, allowing an included file and the source that references it to be renamed
together. The caller supplies updated references; the API never rewrites text.
Missing or cyclic includes, invalid/duplicate paths, output-name collisions,
source limits and stale revisions reject the entire operation.

Chapters and resources share a 4096-source and 64 MiB raw UTF-8 text/path budget.
The Rust constructor additionally enforces the existing expansion/resolution
budgets. JavaScript captures strings once, refuses malformed Unicode, and uses
bounded indexed traversal rather than caller-supplied array iterators. The raw
WASM revision parameter is f64, validated before integer narrowing.

Complete replacement and selective `updateSources` share an optimistic source
revision. A changed capture advances it once; stale requests fail even when they
would be no-ops. Exact normalized captures preserve the revision and AST. A
changed complete capture deliberately reparses all chapters against the new
chapter map, avoiding stale cross-file bindings after membership changes. Use
`updateSources` for selective reparsing when only existing source text changes.

The frozen receipt contains revision, sourceLength, chapterCount, resourceCount,
changed and reparsedChapterCount. It carries no source text or filenames. The
adapter checks it against the submitted counts/UTF-8 bytes and native post-state.
A rejected native transaction leaves the previous book usable. An unverifiable
SUCCESS receipt instead disposes the session (`INVALID_BOOK_SOURCE_SET_REPORT`):
the engine may already have committed, so this is not reported as rollback.
Old packages reject the operation with `UNSUPPORTED_BOOK_SOURCE_SET` before
inspecting source getters. Source edits share one reentrancy guard and cannot
invoke a handle disposed during input admission.

The API is synchronous after session creation. It does not imply worker queuing,
source persistence, cancellation or a new editor UI. Assets are retained by their
existing keys, never renamed or removed implicitly when chapter paths change.

## Verification

Run the adapter regressions with:

```sh
node --test wasm/tests/book_source_set.test.mjs wasm/tests/book_session.test.mjs
```

The implementation pass executed 35 passing tests: 22 new source-set regressions
and 13 unchanged session regressions. All 22 new tests fail against the preceding
adapter. These tests use explicit stateful engine doubles, not generated WASM.

Ten native core regressions in `src/book/workspace_source_set_tests.rs` cover
full PDF/EPUB/site parity with a freshly constructed book, graph replacement,
asset identity, budgets, no-ops, revisions and rollback. Three additional raw
binding regressions in `src/book/browser_source_set.rs` cover numeric revision
and array admission. These 13 Rust tests are added but were not executed in the
implementation environment, which lacks a Rust toolchain. Rust compilation,
Clippy, rustfmt, generated WASM execution and PDF visual output remain unverified.
