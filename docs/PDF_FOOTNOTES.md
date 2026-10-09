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
    carry almost no text.
- Note bodies keep their full block structure: paragraphs, lists, code,
  tables, quotes, mathematics and images.
- Note marks link to the note (a `/Link` annotation with a `/Dest`). Note
  destinations are not bookmarks.

## Endnotes instead

Hosts that want every note in a trailing Notes section prepare the document
first: `render_pdf_document(&doc.with_endnotes(), &opts)`. The SVG renderer
always uses endnotes, and the book pipeline keeps its endnote layout.

## Limits

- `--pdf-optimal-pagination` does not model note space; documents with page
  notes keep the greedy page breaks.
- Notes are not split across pages. A line whose notes together exceed the
  page body (several near-limit notes on one line) still places them all, and
  they can run into the bottom margin.

## Tests

`tests/pdf_page_footnotes_test.rs` pins placement (every note on its
reference's page, below the body, smaller than body text), linking without
bookmarks, the endnote fallbacks, emitter determinism and the optimal
pagination fallback. `tests/pdf_footnote_fidelity_test.rs` proves no note
block is discarded on any PDF entrypoint.
