# Native search sidecars

Use `--search-index` to publish a deterministic JSON index with a rendered document:

```sh
fmd guide.md --out guide.html --search-index guide.search.json
fmd guide.md --profile gfm-plus --out guide.html --search-index guide.search.json
```

The sidecar includes headings and prose, fenced/indented code and diagram source,
table rows, definition terms and descriptions, TeX mathematics, and referenced
footnotes. Nested lists and quotations keep document order. Notes follow their
first-reference order after the body, including notes cited from other notes.
Unreferenced definitions are omitted, and cycles do not duplicate entries.

The selected authoring grammar is applied before extraction. A definition list
in `gfm-plus` becomes searchable term/description text; in
`commonmark-gfm` its literal paragraph text remains searchable.

## Anchors and compatibility

The schema remains `fmd-search-index-v1`. Headings use the `heading` kind and
their emitted level. Other content uses the existing `paragraph` kind.
Text is plain and whitespace-normalized. Body entries target their nearest
preceding Markdown heading, or an empty anchor before the first heading.
Note content targets its actual emitted note element until a heading inside
that note takes over.

Note anchors share the renderer's collision allocation. For example, when a
heading already owns `fn-a`, search results for note `a` use that note's
allocated alternative, preserving reserved targets for other notes as well.
The same correction applies to offline book search because it uses the shared
full-content indexer.

Search includes the reader-visible text of supported HTML blocks, including table
cells, details, summaries, and image alternative text. Search results in a raw
HTML heading use the surrounding Markdown section or note. Inline formatting
keeps words together, line breaks separate words, and comments and formatting
attributes stay outside snippets. Blocks containing only unsupported markup
and layout-only breaks remain outside the full index. Hosts should treat index text as data and encode anchors as
fragment identifiers when building links.

The library's `build_search_index` retains its established heading/body-paragraph
contract. Use `search_index::build_full_search_index` for full content. Callers
holding an AST should apply their selected authoring profile first, matching the
document they render.

## Publication behavior

The sidecar participates in the existing staged output transaction. Invalid
destinations, source aliases or failed writes must not publish a partial set of
files or overwrite source inputs. Output is byte-deterministic for a fixed
document and profile.

The focused regressions are in `tests/cli_search_content.rs` and
`tests/search_index_test.rs`. They cover content inclusion, dialect selection,
literal/raw-HTML boundaries, collision-safe note destinations, deterministic
output and failed-sidecar preservation.
