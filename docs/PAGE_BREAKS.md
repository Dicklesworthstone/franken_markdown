# Page breaks in Markdown

Place this comment on a line of its own to start the following content on a
new PDF page:

```markdown
# Part one

The first part of the report.

<!-- pagebreak -->

# Part two

The next part starts on a new page.
```

The directive works with the default authoring behavior and with
`--profile gfm-plus`. An explicit `--profile commonmark-gfm` keeps the normal
HTML-comment behavior, so the comment does not request a page boundary.
Native library and browser callers select the same behavior with
`Profile::GfmPlus`, `Profile::CommonMarkGfm`, or an omitted profile.

## Exact syntax and source boundaries

The complete marker is `<!-- pagebreak -->`, with that lowercase spelling and
the spaces shown. It can have up to three leading spaces and trailing spaces
or tabs. LF, CRLF, and CR line endings work. No blank line is required around
it: an ordinary Markdown HTML comment already ends the preceding paragraph.

Only a complete standalone comment block becomes a page boundary. These stay
ordinary source content:

- Inline comments inside a paragraph or table cell.
- Comments inside fenced or indented code, code spans, or math.
- Comments inside a larger raw HTML block, such as `<div>...</div>`.
- Multiline comments, additional text on the marker line, and other spellings
  such as `<!--pagebreak-->` or `<!-- PAGEBREAK -->`.

The directive also works as a standalone block inside a quote, list item, or
footnote body. Use the container's usual indentation and blank-line rules.
For example, an explicit break within one list item is:

```markdown
- First part of the item.

  <!-- pagebreak -->

  Continuation of the same item on the next page.
```

Supported includes can contain the same marker. The include expands using the
existing source and resource rules before the authoring pass interprets the
comment. An include containing a page break does not become a separate book
chapter, alter link bases, or introduce a generated heading.

## Output behavior

| Output | Effect |
|---|---|
| PDF, including PDF/A and book exports | The next printable content starts on a new page using the existing explicit-boundary pagination path. |
| HTML, streamed HTML, and HTML fragments | Emits the existing `fmd-pagebreak` element with `break-after: page`; ordinary screen display has no extra text. |
| EPUB | Keeps the same page-break element in chapter XHTML. Reading systems apply its paged-layout CSS; scrolling presentation can ignore the boundary. |
| Continuous Flow preview | Shows the existing page-boundary rule and retains the marker's enclosing source span. |
| Continuous SVG poster | Keeps its existing continuous output: native page boundaries add no ink, and ordinary comments disappear through safe HTML preparation. |

PDF boundaries preserve the current layout constraints and container rules.
They do not create blank leading or trailing pages, and consecutive controls
coalesce when there is no printable content between them. Footnote contents
retain the existing page-note and endnote-fallback policy. A boundary does not
become an outline item or change heading IDs, citations, or chapter-local links.

## Library and source preservation

`parse_markdown` and `parse_markdown_spanned` retain the original comment as an
`HtmlBlock`. The source AST, diagnostics, and UTF-8 byte ranges are unchanged.
The shared `apply_profile` authoring pass promotes an admitted comment to the
existing `Block::PageBreak` before safe HTML lowering can discard it. Keeping
the source comment distinguishes this optional syntax from a programmatic
`Block::PageBreak`, which remains active in every authoring profile.

Ordinary documents without a relevant profile change still use a borrowed AST.
HTML document, writer, and fragment exports share the same block-profile pass.
Book PDF and standalone/book EPUB run it before lowering HTML comments.
SVG keeps its existing continuous-output and resource behavior; hosts that
select an explicit profile continue to apply it before rendering the poster.
Public low-level PDF functions that accept an already prepared layout document
retain that contract; use `render_pdf_document` for ordinary parsed source.

## Validation

`cargo test --no-default-features --test source_page_breaks` exercises exact
marker admission, profile switching, programmatic controls, original parser
spans, literal and raw-HTML contexts, nested containers, include-only sources,
actual PDF page placement, repeated controls, emitter and PDF/A parity, HTML
string/writer/fragment parity, EPUB/book navigation, SVG profile/resource parity,
browser adapter output, and Flow/source-map provenance. Expected PDF and EPUB documents use independently
constructed native page-break nodes rather than the directive recognizer.
