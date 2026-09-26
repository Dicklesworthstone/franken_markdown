# Render-free document navigation analysis

`book::validation::analyze_document_links` validates a single parsed, in-memory
Markdown document without creating a synthetic book, rendering PDF/HTML, loading
fonts, reading files, or requesting URLs.

```rust
use franken_markdown::{parse_markdown, book::validation::analyze_document_links};

let document = parse_markdown("# Target\n\n[go](#target) [broken](#missing)");
let analysis = analyze_document_links(&document)?;
for reference in &analysis.references {
    if let Some(finding) = &reference.finding {
        eprintln!("block {}: {}: {}", reference.block_index, finding.code, finding.destination);
    }
}
# Ok::<(), franken_markdown::RenderError>(())
```

## One publication contract

Standalone analysis and `check_book_links` share AST admission, the emitted
footnote queue, the publication heading-ID algorithm, percent decoding, and
ambiguity detection. Forward references and headings inside lists, quotes and
referenced footnotes participate. Repeated heading IDs use the same collision
suffixes as HTML. A heading colliding with a generated footnote ID is reported
as ambiguous rather than arbitrarily resolved.

The first footnote definition wins. Notes are visited in first-reference order,
including references inside tables, definition lists, and other notes. Cycles
terminate. Unreferenced note bodies, code examples, image destinations and raw
HTML IDs are not mined for document links.

Same-document links include empty destinations, `#fragment`, and
`?query#fragment`. Empty fragments address the document root. Invalid percent
encoding, invalid UTF-8, missing anchors, ambiguous anchors and undefined parsed
footnote references remain distinct findings.

File-addressed links are counted as `unchecked`, not incorrectly diagnosed as
missing merely because an editor supplied one buffer. External links are counted
as `external` without fetching them. Use the explicitly supplied book model and
`check_book_links` for cross-file publication validation.

## Source ownership

Every returned reference and anchor identifies the top-level AST block containing
its source. Pair that index with the corresponding `SpannedDocument.blocks`
entry from the same parse to obtain an authoritative enclosing range. Nested
headings and references retain their container's range; the analyzer does not
claim a precise inline span or search for matching text in code examples.

Footnote definitions identify their definition block; generated backreferences
identify the block containing the first actual reference. This remains true when
notes are emitted out of source order or contain cycles.

Anchor entries are sorted by ID and include kind, title, owner and occurrence
count. References follow HTML emission order. `target_block_index` is absent
for unresolved references and root links; the `finding` field distinguishes
those cases. Results are snapshots of one AST revision, never live source maps.

## Limits and tests

The shared admission limits are 250,000 AST nodes, depth 128, and 64 MiB of AST
text. Findings are limited to 4,096, with at most 8,192 destination bytes each
and 256 KiB of destination text in total. Exceeding a limit returns an error,
not a partial report that looks clean. These are logical work limits, not an
exact heap-memory or wall-clock guarantee.

```sh
cargo test --test document_links_test
cargo test --lib book::validation::tests
```

The regressions compare standalone and book findings, inspect real HTML anchor
IDs, check source-block ownership, cover encoded/root fragments and footnote
cycles, and assert fail-closed admission. They do not certify PDF destinations,
external resources, or raw HTML IDs.
