# PDF footnotes

GFM footnotes (`claim[^id]` with `[^id]: note text`) print the way a book
prints them: a superscript mark in the text, and the note at the foot of the
same page, under a short rule, at 85% of the body type sizes. Each mark is a
link to its note.

## Rules

- Notes are numbered by first reference in the body; note-to-note references
  are followed in that order (same numbering as HTML and SVG).
- A note sits on the page that carries its **first** body reference. The page
  builder chooses the body break against a page shortened by the notes its
  lines reference, re-measuring until the chosen lines' notes fit, so a body
  line and its note are never split across pages. Reservations retain the
  actual remaining body height, even below 40 points: if only the citing line
  fits, later body text moves to the next page instead of overlapping notes.
- A note prints in a trailing **Notes** section instead when it cannot sit at
  a page foot:
  - no body line references it (an unreferenced definition, or one referenced
    only from another note), or
  - it is taller than 40% of the page body, where a page holding it would
    carry almost no text, or
  - the first reference shares a line with other notes and their combined
    height, separator, and citing line cannot fit on one page. Eligible notes
    are admitted in note order; the remaining notes use the Notes section.
- Note bodies keep their full block structure: paragraphs, lists, code,
  tables, quotes, mathematics and images.
- Note marks link to the note (a `/Link` annotation with a `/Dest`), including
  notes moved to the Notes section and references from one note to another.
  Note destinations are not bookmarks.

## Return to a citation

Enable `--pdf-footnote-backlinks` to make the existing superscript label at the
start of each note return to that note's first linked citation. The label
keeps its normal colour and typography; no arrow, extra text or line is added.
The option is off by default, preserving existing PDF output.

The native flag works with render, watch, batch and book PDF output. Set a
persistent default with `fmd config set pdf_footnote_backlinks true`; pass
`--pdf-footnote-backlinks=false` to override that default for one invocation.
Rust callers set `PdfOptions::footnote_backlinks` or
`WasmRenderOptions::footnote_backlinks` to `true`. JavaScript document, book
and interactive export options use `footnoteBacklinks: true`, as does the MCP
`fmd.render_pdf` tool.

Browser hosts need a matching rebuilt JavaScript/WASM package whose native
typography parser accepts `footnote-backlinks`. Older bindings or token parsers
reject an explicit backlink request; they do not retry without return links.
Omitting the option or setting `footnoteBacklinks: false` preserves the existing
browser ABI. Saved offline workspaces carry the option with their captured
runtime, so their embedded package must support the token too.

Return destinations use the actual final page and line position after
pagination, fitting and TOC insertion. A repeated citation returns to the
first citation in the document body. A note referenced only from another note
returns to its first citation in note-number order. Body citations take
precedence even when an earlier page-bottom note also cites that definition.
Repeated table headers return to the first painted occurrence. The same
rules apply to full notes moved into the trailing Notes section and to PDF/A
output. Unreferenced notes retain their plain labels and have no return link.

PDF book links can also address the HTML-compatible citation anchors, such as
`chapter.md#fnref-1`, without enabling automatic return labels. These numbers
follow each chapter's first-use order; the merged book's global note numbers
do not change the meaning of a source link. Links to `chapter.md#fn-label`
still address the note itself. Both forms bind to actual rendered destinations,
including note-to-note citations and endnote fallback. Missing or ambiguous
anchors remain unresolved, and a heading that merely starts with `fnref-`
remains a heading.

A citation nested inside an ordinary link is printed without a second,
nested link. Return navigation uses the first citation with its own rendered
note link; it does not invent a destination for a missing citation.

## Endnotes instead

Hosts that want every note in a trailing Notes section prepare the document
first: `render_pdf_document(&doc.with_endnotes(), &opts)`. The SVG renderer
always uses endnotes. PDF books use the same page-bottom rules as ordinary
PDF documents, including when chapters contain local navigation links. Each
chapter keeps its own footnote-ID scope; repeated labels in different chapters
cannot replace one another. Chapter and heading links bind to actual emitted
source destinations, so headings inside notes remain addressable after a note
moves to a page foot or to the trailing Notes section.

## Limits

- Under `--pdf-optimal-pagination` the exact page planner reserves each
  note's body with its first citing line and one shared separator per page
  containing notes. Several citing lines on the same page do not reserve extra
  separators. If the exact planner cannot plan a document, it reports that
  outcome and paginates greedily (the legacy fallback planner does not model
  note space). With `--pdf-strict-pagination`, it returns an error instead of
  falling back.
- Notes placed at a page foot are indivisible. Notes that exceed the available
  space print as endnotes, where ordinary block pagination can span pages.
  Their complete text, block structure, numbering, and link destinations are
  retained.

## Tests

`tests/pdf_page_footnotes_test.rs` pins placement (every note on its
reference's page, below the body, smaller than body text), linking without
bookmarks, the endnote fallbacks, emitter determinism and placement under
optimal pagination. `tests/pdf_footnote_fidelity_test.rs` proves no note
block is discarded on any PDF entrypoint.
The page-footnote tests also cover several large notes cited on one line,
fallback destinations in the emitted PDF, note-to-note cycles, nonparagraph
endnotes, and byte parity with the ordinary-note PDF captured before these fixes.

`tests/pdf_navigation_test.rs` also checks emitted PDF books with and without
chapter links, repeated note labels, note headings interleaved with body
headings, nested alerts/lists, PDF/A, TOC and tall-note fallback. It independently
reads xref/page objects, annotation rectangles, font character maps and page
streams to verify note placement and the destination of each visible link.
Its return-navigation fixtures also cover first-use identity when paint order
differs, repeated table headers, cycles, actual multipage endnotes, unchanged
text geometry, emitter determinism and chapter-scoped `fnref-N` targets.
`tests/book_pdf_footnote_links.rs` keeps missing and ambiguous citation anchors
from capturing unrelated destinations.

`tests/book_browser_footnote_profile.rs` drives the production Rust `FmdBook`
API through repeated enable/reset cycles, comparing actual PDF bytes with
direct native book exports and preserving source identity. Run it with
`cargo test --no-default-features --features wasm-book --test book_browser_footnote_profile`.

The browser adapter regressions cover root/book exports, worker and Flow
transport, retained-output caching, and offline/source-project round trips with
explicit native binding doubles and real Node workers. They verify option
ownership and forwarding; the Rust suites above inspect emitted PDF navigation.
These adapter tests do not execute a newly generated WASM engine.
