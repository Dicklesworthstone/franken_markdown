# Decimal columns and total rules

Financial statements can opt into decimal alignment with `---.` in a pipe
table's delimiter row. The dot follows at least three hyphens; ordinary GFM
left, center and right alignment keep their existing meaning.

```markdown
| Item | Current year | Prior year |
| --- | ---. | ---. |
| Revenue | 1,234.5 | 950.00 |
| Costs | (234.50) | (175.000) |
| | --- | --- |
| Net income | **1,000.000** | **775.00** |
| | === | === |
```

`---` draws a single rule beneath the preceding data cell; `===` draws a double
rule. A rule row contains only these plain markers and empty cells. Empty cells
leave that column unruled. Consecutive rule rows combine, with a double rule
taking precedence. Rules can also span label columns by putting markers in
those cells.

Marker rows are interpreted only when the table has a decimal column. A leading
marker row with no preceding data row remains literal. Code spans, styled text,
links and rows containing any other content remain data, so `` `---` `` can be
used to display a literal marker in a financial table.

## Numeric spelling

The renderer preserves the source text exactly. It does not round, pad with
zeros, parse amounts into floating-point values, or change their separators.
Decimal columns accept:

- ASCII digits and an optional decimal dot, including integers and `.125`;
- consistent three-digit grouping with commas, apostrophes, ordinary spaces,
  nonbreaking spaces or narrow nonbreaking spaces;
- an optional sign and currency symbol (`$`, `€`, `£`, `¥`, `₹`, `₩`, `₽`);
- accounting parentheses and percent or per-mille suffixes;
- emphasis, bold, strike-through and links around numeric text.

Integers align where their decimal point would be. Decimal commas, scientific
notation, arbitrary units and other nonnumeric content use right alignment.
Code, mathematics, images and footnote references also use the ordinary cell
layout. The original content remains visible in every case.

## Rendering behavior

PDF and SVG measure the actual styled font advances on both sides of the
decimal point. The widest integer part and widest fraction part can belong to
different rows; the column allocator reserves room for both. A cell that must
wrap in a narrow column falls back to right alignment. PDF total rules belong
to the last physical line of their data row, include clearance for deep inline
content, and reserve space during pagination. Repeated headers never repeat a
subtotal rule.

HTML uses adjacent styled spans and a shared measured suffix track in each
decimal column. Its ordinary scrollable table wrapper handles wide statements.
Total rules are borders on the preceding data cells, preserving table reading
order. EPUB uses the same table content and inline styles; the reading system
controls its support for the CSS layout.

The shaped Flow preview uses the host's measured text clusters for decimal
positions and draws rules on their data rows. The estimate-only preview draws
rules without claiming font-measured alignment. The separate constrained
plain-text table viewport treats `Align::Decimal` as right alignment.

The AST retains all authored rows for source mapping and editing. Marker rows
do not become rendered data rows, PDF table tags or Flow reading-order nodes.
PDF rules are decorative artifacts. Existing links, values and source row
identities remain attached to their data cells.

## Native and library use

```sh
SOURCE_DATE_EPOCH=1700000000 fmd statement.md --to both --out statement.html
fmd statement.md --to svg --out statement.svg
fmd statement.md --to epub --out statement.epub
```

No extra renderer option is required. Library callers can parse the syntax or
construct an AST table with `Align::Decimal`. All paths use the existing theme,
font assets and deterministic metadata options. Decimal formatting itself adds
no clock, locale, filesystem or network dependency.

The checked-in [statement fixture](../tests/fixtures/financial_statement.md)
exercises mixed precision, accounting parentheses, bold totals and a linked
amount. PDF regressions inspect actual content streams, table structure, links,
page breaks and PDF/A output; renderer tests check measured geometry and
ordinary-table compatibility.
