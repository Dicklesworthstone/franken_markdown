# Edit a retained book without reparsing unchanged chapters

The `/book` session API now exposes `sourceRevision` and synchronous
`updateSources`. One selected source set can be revised and exported repeatedly
without reconstructing the renderer, resupplying images/fonts, or reparsing
chapters whose expanded Markdown text is unchanged.

```js
import { createBook } from "@franken-suite/franken-markdown/book";

const book = await createBook([
  { path: "start.md", source: "# Start\n\n{{#include shared.md}}\n" },
  { path: "end.md", source: "# End\n\n{{#include shared.md}}\n" },
], {
  includeSources: [{ path: "shared.md", source: "Original shared paragraph.\n" }],
  title: "Manual",
  page: { size: "a4" },
  pageNumbers: true,
});
try {
  const expectedRevision = book.sourceRevision;
  const change = book.updateSources([
    { path: "shared.md", source: "Revised shared paragraph.\n" },
  ], { expectedRevision });
  // change.revision === 1
  // change.changedSources === 1
  // change.reparsedChapters is [0, 1], in chapter reading order.
  const pdf = book.renderPdf();
  const epub = book.renderEpub();
  const site = book.renderSite();
  // Use the ordinary output.blob() / filename() methods for explicit downloads.
} finally {
  book.dispose();
}
```

## Transaction semantics

A replacement is a complete `{path, source}` string, not a text patch or a
rename. Paths identify existing chapters or include-only resources. The source
set, reading order, and expansion mode are fixed by creation. Construct another
book to add, remove, rename, or reorder files. Include resources remain resources,
not chapter pages or EPUB spine items.

The core first validates the entire batch and its projected source budget, then
expands against the completed new source set. It compares exact expanded chapter
strings, not hashes, and parses only changed text. Newly parsed links bind against
the full book, not just the edited chapter. Only after that preparation succeeds
are source snapshots, AST replacements, source length and revision published.

All ordinary rejected updates leave the previous source capture, AST, revision,
render options and assets usable. Missing/cyclic includes, unknown paths,
duplicate normalized replacements and budget failures are not partial updates.
One source may shrink to make room for another in a single batch; acceptance is
independent of replacement ordering.

An exact no-op batch keeps its revision and reports no changed sources or parser
invocations. Editing an unused resource or text outside a selected snippet can
advance the source revision with an empty `reparsedChapters` list. The revised
resource is still retained for a later include. Existing output byte arrays remain
owned snapshots; they are not silently changed by later edits.

`sourceRevision` counts source changes only. It does not track `setImage`, font
changes, presentation settings or host editor buffers. Hosts must still invalidate
prepared downloads when those inputs change. The revision is scoped to one book
instance and cannot identify a capture from a different session.

## Stale-edit protection and failure handling

Pass `expectedRevision` for a replacement prepared asynchronously. The session
rejects a stale revision before reading replacement source getters, then rechecks
after admission. The raw Rust API also compares the expected revision before
mutating source. Omitting the option captures the current revision at call entry;
it does not prove that an asynchronously prepared edit was based on that version.

Revisions are integers from zero through 4,294,967,295, never coerced strings,
floats, wrapped integers or timestamps. At exhaustion, changed batches require a
new workspace; a true no-op remains possible. Stale no-op requests still fail.
Options must be plain data objects containing only `expectedRevision`.
Reentrant source updates are refused with `BOOK_BUSY`; disposal during admission
cannot cause a call on a freed raw handle.

A native success reply is checked against the actual post-update revision,
chapter count and source byte count, and its sorted parser-invocation indexes
are validated. A malformed/mismatched successful reply produces
`INVALID_BOOK_UPDATE_REPORT` and disposes the session. Native mutation may already
have happened in that situation: this is deliberately not reported as rollback.
Recreate the session from the host's authoritative source capture.

Old generated packages still support their render-only calls. Source revision
and update operations on them fail with `UNSUPPORTED_BOOK_UPDATE`. A wrapper-only
update cannot add native editing support. Rebuild the matching WASM package.

## Parsing, expansion, and memory boundaries

This is chapter-level incremental **parsing**, not incremental PDF layout, partial
ZIP rewriting, a new parser, or a dependency-graph-only include engine. A changed
batch still revalidates/re-expands the entire selected source set through the
existing bounded resolver. Final exports use the normal complete rendering paths.
There is no benchmarked latency or speedup claim.

Editable books retain original source strings and, when expansion is enabled,
expanded chapter strings. Staging an update temporarily holds additional bounded
source copies. Logical byte budgets are not guarantees about peak heap usage;
AST and rendering allocations are additional. Regular native `BookRenderer`
callers do not acquire this source cache: they opt in with `BookWorkspace`.
The browser's `FmdBook` now uses that workspace internally.

The existing limits apply: at most 4096 selected sources and 64 MiB original
text/path bytes; each replacement batch also fits 64 MiB. Expansion keeps the
shared 16-level depth, 4096 whole-book resolver calls, 64 MiB expanded-text/path
budget and original-source-plus-resolver-copy budget. Returned reports contain
only bounded numeric fields and at most 4096 chapter indexes.

JavaScript validates Unicode and captures each path/source string before the
native call. It does not normalize Unicode or line endings, expand includes,
resolve paths, or parse Markdown. Array traversal uses a captured bounded count,
not caller-supplied iterators. `sourceLength` counts original UTF-8 source bytes
once each, including include-only resources but excluding filenames and repeated
expansion copies.

`expandIncludes: true` (default) is retained even when no include directive exists
at creation, so later authored directives use the same mode. Explicit false keeps
include syntax literal throughout the session. As before, URLs inside inserted
snippet text resolve relative to the consuming chapter; this feature does not
change transclusion URL-origin semantics.

## Native API and verification

```rust
use franken_markdown::book::{BookInput, BookWorkspace};

# fn example() -> franken_markdown::Result<()> {
let mut book = BookWorkspace::new(&[
    BookInput { path: "one.md".into(), source: "# One".into() },
])?;
let change = book.update_sources_at_revision(&[
    BookInput { path: "one.md".into(), source: "# Revised".into() },
], 0)?;
assert_eq!(change.reparsed_chapters, vec![0]);
let pdf = book.render_pdf()?;
# let _ = pdf;
# Ok(())
# }
```

Native workspace tests cover real AST/full-rebuild parity, shared and selected
includes, rollback, unchanged AST/image allocation retention, source budgets,
Unicode, revision exhaustion and all publication formats. A seeded test applies
200 source edits. Those Rust tests require a working Rust/DSR environment.

`node --test wasm/tests/book_source_updates.test.mjs` runs the production session
adapter with an explicit stateful engine double. Its seeded 1000-update test is
wire/ownership coverage, not evidence of native parsing. The public TypeScript
fixture is `wasm/book_source_updates_types_test.mts`.

After rebuilding `wasm/pkg`, `node wasm/book_source_updates_smoke.mjs` runs the
actual generated engine with the production session adapter, checks PDF/EPUB/site
byte parity against fresh rebuilds, include updates, rollback, revision boundaries,
asset retention and repeated-render determinism. It fails instead of substituting
an engine when the generated package is missing. No demo UI, persistent worker,
filesystem save, undo history or new release is implied by this session API.
