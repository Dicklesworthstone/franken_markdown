# Tabs in nested Markdown containers

Tabs used for indentation follow four-column tab stops on the original physical
line. Quote markers, list markers, and preceding spaces count toward that column.
Removing a container does not restart the next tab at column zero. This follows
the [CommonMark tab rules](https://spec.commonmark.org/0.31.2/#tabs).

For example, the following lines contain a space and then a tab after their
first marker. Both produce a heading inside their container:

```markdown
> 	# Quoted heading
- 	# Listed heading
```

The same rule determines continuation ownership. A tab immediately after `-`
places the item content at physical column four. After a blank line, a line
indented only two spaces belongs outside that item; four columns keep it inside.
Nested quotes and lists use their full physical prefix when interpreting tabs,
so correctly aligned sibling items remain separate items.

Reference definitions and ordinary inline syntax use these same boundaries.
A definition after valid tabbed container padding can resolve links throughout
the document. Four columns of actual code indentation still make a definition
literal text. Source spans and editable destinations continue to refer to the
original UTF-8 input, including its tabs, Unicode, BOM, and line endings.

Literal tabs remain literal. Fenced code, indented code, inline code, and raw
HTML recover every tab whose complete width remains after structural indentation
is removed. If indentation consumes only part of a tab, the remaining columns
are spaces. Marker-shaped code such as `-` followed by a tab is therefore copied
without changing the tab or activating links in that code.

The parser normalizes potential container-prefix whitespace once, before
splitting containers, and records each expansion separately from ordinary
spaces. Both rendering and the include scanner use this representation. Block
recognition remains with the ordinary parser; the prefix scan does not turn
literal marker-shaped text into a new block.

`tests/parser_container_tabs.rs` covers heading/list recognition, continuation
boundaries, nested siblings, partially consumed code indentation, literal-code
preservation, Unicode destination ranges, and reference-definition ownership.
