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
- When a note starts with an image-only paragraph, its superscript label
  appears in a separate paragraph before the figure. Linked and styled
  images and image galleries keep their figure layout and dimensions; adding
  the note label does not shrink them into inline icons. Images mixed with
  prose retain ordinary inline sizing. A figure that makes its note too tall
  for the page foot keeps its full figure layout in the trailing Notes section.
- Note marks link to the note (a `/Link` annotation with a `/Dest`), including
  notes moved to the Notes section and references from one note to another.
  Note destinations are not bookmarks.

Footnote names are literal source identifiers. Names such as `fmd-fn-1` and
`fmd-fn-evidence` follow the same numbering and preservation rules as any other
name, including unreferenced definitions and text that already starts with a
superscript. Internal preparation state is kept separately from the source.
HTML and EPUB retain those literal names in their anchor IDs and encode them
when generating links: a note named `evidence%41` links to
`#fn-evidence%2541`, keeping it distinct from a note named `evidenceA`.

## Continue long text notes across pages

Enable `--pdf-split-footnotes` to keep long text notes in the page-foot flow
instead of moving them into the trailing Notes section:

```sh
fmd report.md --to pdf --pdf-split-footnotes --out report.pdf
fmd book ./manual --to pdf --pdf-split-footnotes --out manual.pdf
```

The first fragment stays on its first citation's page. Remaining measured lines
continue at later page feet under the repeated separator rule. Pending notes
can create note-only pages after the document body; these pages participate in
running folios, total page counts, destinations, and verification. Every source
line belongs to the same logical note across its fragments, and the citation
still links to the first fragment. `--pdf-footnote-backlinks` can be combined
with this mode.

Continuation supports paragraphs with styled text, inline code, inline
mathematics, images mixed with prose, and ordinary links, as well as ordinary
fenced or indented code, tables, headings, ordered/unordered/task lists, and
blockquotes (including alerts).
Lists and quotations can nest, and code blocks can appear inside them. Fragments
retain syntax highlighting, source line numbers, list labels, indentation,
quotation bars, code backgrounds, links and logical note ownership. Code and
quotation decorations reserve their own space at fragment boundaries. Generated
note labels, list markers on their own row, and alert titles stay with the first
content row they introduce.

Section headings retain their source level, links and logical note ownership.
A wrapped title and its H1/H2 underline stay together with the first following
content row, including a table's complete header and first logical data row.
The association follows source quotation and list-item ownership: a heading at
the end of one item does not capture its sibling, and a heading at the end of a
nested quotation does not capture text outside that quotation. A heading can
introduce nested content in its own scope. The generated note label stays with
a first heading and its subject, so the first fragment begins on the citation's
page. If this mandatory group cannot fit, the complete note keeps its existing
endnote fallback.

Inline formulas and images stay indivisible within their measured text rows.
Continuation retains their fitted dimensions, formula source, image alternative
text, links, and Formula/Figure tags inside the same logical note. Actual object
ascent and descent contribute to fragment boundaries, keeping the first and
last row inside the note region. Missing images and unsupported inline math
retain their ordinary visible fallback and diagnostics. A row too tall for a
note-only page keeps the complete note in the endnote fallback.

Tables continue between complete logical rows. A wrapped cell stays with the
other cells in its row, and each table fragment repeats the original measured
header and separator above its first data row. The initial header stays with
the first complete row; the closing rule stays with the last row. Repeated
header paint is a pagination artifact, so the logical note contains one table
with one set of source-owned header and data cells. Table padding, alternating
row shading, financial footing rules, inline formulas, images and links retain
their measured geometry, including inside lists and quotations.

Admission includes the full repeated header and an atomic data row. If that
unit cannot fit on a note-only page, the complete note retains its ordinary
endnote fallback. The same fallback applies when the first header, row and
note label cannot fit with the citing body line. No rows are clipped, shrunk
to satisfy a page, or partially moved into a separate note.

Notes containing standalone figures, display-math blocks, diagrams, definition
lists, generated contents, raw HTML blocks, thematic breaks or explicit page breaks retain the full-fidelity endnote fallback when they cannot
fit as an ordinary page note. Unreferenced notes remain endnotes. A measured row
or a required label-and-content group that cannot fit on a note-only page also
sends its complete note to that fallback.

Continuation requires **greedy pagination without a baseline grid**. Combining
it with `--pdf-optimal-pagination`, `--pdf-strict-pagination`, or a positive
`--pdf-baseline-grid-pt` fails with `pdf_split_footnotes_incompatible`. Disable
continuation or the incompatible policy; the renderer does not silently change
the requested pagination contract.

The option is off by default. `fmd config set pdf_split_footnotes true` supplies
a persistent default, and `--pdf-split-footnotes=false` overrides it for one
render, watch, batch or book invocation. Rust callers set
`PdfOptions::split_footnotes` or `WasmRenderOptions::split_footnotes`. Browser
PDF options and the MCP PDF routes use `splitFootnotes: true`; retained browser
books also accept the typography token `split-footnotes`. Browser and saved
offline exports require a matching rebuilt WASM package. An older token parser
must reject the request instead of silently omitting continuation.

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
- By default, notes placed at a page foot are indivisible. Notes that exceed
  the available space print as endnotes, where ordinary block pagination can span pages.
  Their complete text, block structure, numbering, and link destinations are
  retained.

## Tests

`tests/pdf_footnote_continuation_test.rs` independently reads emitted text,
geometry, links and page objects to check text-note continuation, source-line
conservation, tail pages, book note scope, running folios, shared browser-core
output, emitter parity, and incompatible-policy refusals. Its table fixtures
check wrapped-row atomicity, repeated header artifacts, complete logical cell
ownership, quoted tables with literal padding, and the unchanged full-note
fallback for oversized rows and headers. Sectioned-note fixtures inspect
wrapped headings and underline paint, first-subject keeps, source-level tags
under the logical Note, late-heading link destinations, sibling-item boundaries,
and complete endnote fallback for an impossible kept group. Their geometry
checks identify body text by fixture content rather than font size, because
note headings can be larger than ordinary body prose.

`tests/pdf_footnote_inline_objects_test.rs` decodes emitted glyph maps, vector
paths, image transforms, annotation rectangles and structure ancestry to check
inline formulas and linked images across continuation boundaries, including
complete table cells. It also preserves the existing indivisible-figure,
oversized-row and unreferenced-note behavior.


`tests/pdf_page_footnotes_test.rs` pins placement (every note on its
reference's page, below the body, smaller than body text), linking without
bookmarks, the endnote fallbacks, emitter determinism and placement under
optimal pagination. `tests/pdf_footnote_fidelity_test.rs` proves no note
block is discarded on any PDF entrypoint.
`tests/pdf_footnote_figures_test.rs` checks first-image notes, linked and styled
figures, galleries, tall-note fallback, explicit endnotes and the unchanged
inline-prose case. It compares emitted Figure bounds with decoded drawing
matrices and checks the note label and both navigation directions.
`tests/pdf_footnote_source_identity.rs` checks author-controlled note names,
rich unreferenced bodies, authored superscripts, strict baseline-grid output,
and book navigation across the source-document APIs.
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
