# Footnote links in PDF books

PDF books support explicit links to referenced footnotes using the same source
anchors as HTML and EPUB:

```markdown
The result follows from the experiment.[^experiment]

[Read the citation](#fn-experiment)
[Read the next chapter's citation](methods.md#fn-experiment)

[^experiment]: The experiment and its supporting observations.
```

Each chapter owns its footnote labels. Two chapters can both define
`[^experiment]`; the local link reaches the current chapter's note, and the
cross-chapter link reaches the note in `methods.md`. Definitions introduced
through includes belong to their containing chapter. Encoded fragments and
chapter paths use the existing book URL resolver.

This applies to the shared Rust book renderer, native `fmd book --to pdf` and
`--to both`, MCP book publication, and retained browser book PDF exports. It
also applies with printed contents and PDF/A settings; no new option is needed.

## How destinations stay correct

Book navigation first resolves the source anchor against the same emitted
footnote inventory used by HTML navigation. The PDF footnote preparation then
maps the original definition ID to its numbered note. The final link uses the
destination recorded while that note is actually painted. It therefore follows
first-citation numbering, chapter isolation, page-bottom placement, and the
endnote fallback for oversized or note-only citations. Definition order and
estimated page numbers are never used to choose a destination.

An explicit Markdown link does not itself count as a `[^reference]`. A
definition without such a reference has no HTML `fn-label` anchor, so a link to
that spelling remains unresolved even if the PDF retains the unused definition
in its Notes section. A heading and referenced note with the same emitted ID
also remain unresolved; neither silently wins the collision. Ordinary headings
whose IDs begin with `fn-` continue to work when no note shares their ID.

The generated superscript citation links retain their existing behavior. This
extension addresses links to note bodies; HTML's `fnref-N` backreference anchors
are not added as PDF destinations.

## Regression coverage

`tests/book_pdf_footnote_links.rs` reads the emitted PDF cross-reference table
and compares explicit note-link destinations with the actual superscript
citation destinations. It covers chapter-local duplicate labels, Unicode and
query fragments, first-use numbering, cycles and note-only references,
oversized notes spanning pages, included definitions, and invalid or ambiguous
anchors. The chapter-scope case runs with and without printed contents and
strict PDF/A. The shared book navigation unit suite also checks identical
output from chunked and monolithic PDF writers.
