# Blank lines in listed code samples

An indented code sample may contain several chunks separated by blank lines.
When the next chunk still has four code-indentation columns after its list
prefix, those blanks belong to the same code block. They do not make the list
loose or add paragraph spacing to its other items. This follows CommonMark's
[indented-code](https://spec.commonmark.org/0.31.2/#indented-code-blocks) and
[list](https://spec.commonmark.org/0.31.2/#lists) rules.

For example, this remains a tight list with one code block in the first item:

```markdown
-     first chunk

      second chunk
- Next item
```

Whitespace beyond the list and code indentation remains part of the sample,
including spaces and tabs on internal blank lines. Copying the rendered code
therefore retains that whitespace. A tab whose columns are partly consumed by
indentation leaves spaces; a wholly unconsumed tab remains a tab, using the
original physical column rules described in [container tabs](PARSER_CONTAINER_TABS.md).

The same ownership applies within nested lists and explicitly marked quotes.
A missing quote marker closes quoted code. Blanks that separate actual item
blocks or sibling items still make their owning list loose, and trailing blank
lines remain outside an indented code block.

List collection borrows the stripped blank-line slices instead of replacing
them with empty strings. Rendering, reference collection, and source editing
then see the same content and original UTF-8 positions. The indented-code extent
scanner visits each blank run once rather than repeatedly scanning its suffix.

`tests/parser_list_code_blanks.rs` covers literal spaces/tabs, partial tab
consumption, nested mixed containers, real separators, reference lookalikes,
Unicode with all supported line endings, both authoring profiles, and a long
internal blank run.
