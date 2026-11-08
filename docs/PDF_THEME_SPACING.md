# Shared theme spacing in PDF

PDF can use the same body leading and table padding tokens as HTML, EPUB, and
SVG. Enable `PdfOptions::use_theme_spacing` to apply those tokens to measured
PDF layout. The option defaults to `false`, preserving the existing print
layout and output bytes.

## Rust

```rust
use franken_markdown::{PdfOptions, render_pdf};

let mut options = PdfOptions {
    use_theme_spacing: true,
    ..PdfOptions::default()
};
options.theme.spacing.line_height = 2.0;
options.theme.spacing.table_cell_padding_x_em = 1.25;
options.theme.spacing.table_cell_padding_y_em = 0.75;

let pdf = render_pdf("A paragraph.\n\n| Item |\n| --- |\n| Value |", &options)?;
# Ok::<(), franken_markdown::RenderError>(())
```

The values are literal multipliers of the resolved font size. At 11pt body
type, `line_height = 2.0` requests 22pt leading. At 10pt table type,
`table_cell_padding_x_em = 1.25` reserves 12.5pt on each side of a cell, and
`table_cell_padding_y_em = 0.75` reserves 7.5pt above and below a logical row.
Adaptive table font sizing and smaller page-footnote type use their actual
resolved sizes when converting em values to points.

The default theme values, when explicitly enabled, are 1.7 leading, 0.85em
horizontal padding, and 0.55em vertical padding. Enabling the option therefore
changes the density of a default PDF; it is an explicit choice, not a no-op
unless individual tokens are also changed.

## CLI, configuration, and browser APIs

Use `--pdf-theme-spacing` to enable shared spacing for native PDF output, or
set `pdf_theme_spacing = true` in configuration. The explicit CLI value
`--pdf-theme-spacing=false` disables it. Programmatic Rust callers can set
individual spacing values as shown above.

The JavaScript rendering API accepts:

```javascript
const options = {
  typography: {
    themeSpacing: true,
    lineHeight: 2,
    tableCellPaddingXEm: 1.25,
    tableCellPaddingYEm: 0.75,
  },
};
```

A numeric spacing option also enables the policy. Combining numeric values
with `themeSpacing: false` is rejected, so the API cannot silently ignore a
requested geometry change.

The retained book rendering options expose the same settings in
`BookRenderOptions.pdf.typography`:

```text
theme-spacing,line-height=2,table-padding-x=1.25,table-padding-y=0.75
```

## Layout behavior

Body paragraphs, list item text, blockquote text, and table header/body text
use the requested leading. Headings, code blocks, standalone figures, display
equations, rules, and running page bands retain their own typography and box
geometry. Body line height does not change their font sizes or inter-block
spacing.

Leading is a nominal line box. Actual glyphs, positioned combining marks,
inline formulas, and inline images can enlarge it to keep their ink inside
the measured row. A baseline grid can round the required space up to complete
grid intervals. These protections take precedence over a tighter requested
line height.

Table padding participates in column allocation and horizontal panel
capacity. A table that no longer fits across a page can be split into more
panels; all source columns remain present. Padding uses the same resolved
table font size as the cell text.

Vertical padding surrounds a logical row. A row that wraps into several
physical lines receives top padding on its first fragment and bottom padding
on its last fragment. Its interior lines use the requested leading without
repeating the padding. Repeated headers carry the original header padding.
As with the existing table layout, a continuation omits its repeated header
if that header and the next body line cannot fit together; the original source
header remains in the document.
Financial table footing rules sit below the padded content and reserve their
own additional depth. Table separator rules retain their own spacing.

The same measured boxes drive ordinary and strict pagination, paragraph
alternatives, page-footnote admission, baseline grids, zebra backgrounds,
and final placement. Increasing padding or leading can increase page count,
move a note to the trailing notes section, or change the selected paragraph
layout. Verification and PDF output use the same policy. Streaming and
monolithic emission retain identical document layout.

## Admission and compatibility

Explicit spacing accepts the following finite values:

| Token | Inclusive range |
| --- | --- |
| `line_height` | 1.0 to 4.0 |
| `table_cell_padding_x_em` | 0.0 to 8.0 |
| `table_cell_padding_y_em` | 0.0 to 8.0 |

Invalid explicit settings return `RenderError::InvalidInput` with a
`pdf_theme_spacing` diagnostic. They are not clamped. When a document contains
a table, its page body must fit at least one 20pt readable column plus both
horizontal padding sides at the nominal table font size. If it cannot, the
renderer returns the same typed diagnostic; reduce the padding, reduce the
nominal table font size, or increase the usable page width. This admission
check also covers tables nested in lists, quotes, and notes.

After layout resolves font sizes, wrapping, note placement and optional grid
rounding, each row using explicit spacing must fit within the usable page
height. An oversized row returns a `pdf_theme_spacing` diagnostic before
either strict or fallback pagination. The renderer does not discard padding
or force a row beyond the page boundary to make progress.

When `use_theme_spacing` is `false`, these three tokens remain outside the PDF
layout policy. Changing them, including values that would fail explicit
admission, does not alter historical PDF output. Other renderers continue to
apply their existing theme rules.

## Regression coverage

`tests/pdf_theme_spacing_test.rs` decodes real emitted page streams and font
Unicode maps. It checks literal baseline distances and font sizes, per-side
cell coordinates, padding once per wrapped logical row, page-note type
scaling, panel count/source preservation, strict and gridded header repetition,
verification/emission parity, invalid input, and unchanged output with the
option disabled.
