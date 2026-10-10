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
  line and its note are never split across pages.
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

## Endnotes instead

Hosts that want every note in a trailing Notes section prepare the document
first: `render_pdf_document(&doc.with_endnotes(), &opts)`. The SVG renderer
always uses endnotes, and the book pipeline keeps its endnote layout.

## Limits

- Under `--pdf-optimal-pagination` the exact page planner reserves each
  note's space with the line citing it, charging the separator once per citing
  line, so planned pages never under-reserve but can leave a little extra room
  when one page carries several notes. If the exact planner cannot plan a
  document, it paginates greedily (the legacy fallback planner does not model
  note space).
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
