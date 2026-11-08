# Explicit equation labels

Standalone math blocks accept an explicit label using `\tag{...}` or
`\tag*{...}`. Labels work in HTML, EPUB, PDF, SVG and Flow through the shared
`fmd-math` parser. No render option, external typesetter or JavaScript library
is required.

```markdown
$$
E = mc^2 \tag{1}
$$

$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2} \tag*{Triangular numbers}
$$
```

`\tag{1}` displays `(1)`. The starred form adds no parentheses. A label is
upright text by default; it supports the existing text grammar, including
escaped characters, text styling, colors and explicit `$...$` math islands:

```tex
x = y \tag*{\textbf{Case $n_1$}: positive \& finite}
```

The command labels the whole standalone display. For example, an existing
`aligned` environment can have one label after its closing `\end{aligned}`.
Recognized fenced `math` blocks accept the same syntax. A display may contain
only one tag, and its label must be a complete braced argument.

The engine also accepts bare top-level rows separated by `\\`, with `&` for
alignment. A single top-level tag on that form labels the complete display,
wherever the tag occurs in the source; it does not number one individual row.

## Placement and narrow columns

PDF, SVG and Flow center the equation in its actual containing column and
place the label at the right edge. Placement includes visible ink and
intentional spacing, with a one-em minimum gap between equation and label.
Nested list and quote indentation therefore affects label placement.

If the equation and label cannot fit at the normal size, these renderers fit
the complete pair using their existing uniform scaling policy. They do not
drop the tag, crop its characters, or lay it over the equation. The resulting
height participates in normal block layout and pagination. HTML uses separate
MathML body and label cells in a centered CSS grid; a pair wider than its
column scrolls horizontally as one unit.

PDF retains the complete original TeX, including the tag, in its accessible
formula and extraction text. Flow keeps one math reading node and the original
source span for the complete display. Tagged HTML retains the original formula
in the wrapper's escaped `data-tex` attribute; labels and embedded math are
serialized as MathML rather than inserted as raw HTML.

## Deliberate limits

This feature supplies explicit labels. It does not infer equation numbers or
cross references. Automatic `equation`, `align`, `gather` and `multline`
environments, `\label`/`\ref`, and per-row tags inside an alignment retain their
existing unsupported status. Use a single top-level explicit tag on a supported
display instead.

Tags inside braces, scripts, text islands or nested environment rows are refused. Inline
`$...$` and inline `$$...$$` are single inline boxes and do not have a standalone
label column, so they also refuse tags. Duplicate, unbraced and incomplete tags
produce the renderers' complete-source fallback; no successful partial formula
silently loses the requested label. Existing per-formula source, nesting,
primitive and outline limits apply to equation and label together.

## Library APIs

The existing `Node`, `Layout`, `parse` and `Engine::typeset` contracts remain
unchanged. A host that owns a display column can use the additive APIs:

```rust
use fmd_math::{Engine, parse_display};

let source = r"E = mc^2 \tag{1}";
let formula = parse_display(source)?;
let body = formula.body();
let label_tree = formula.tag();
let label_mathml = formula.tag_mathml();

let engine = Engine::bundled()?;
let layout = engine.typeset_display(source, 40.0)?;
# Ok::<(), fmd_math::MathError>(())
```

The width is in em units at the host's intended formula size. It must be finite,
positive and at most 1,000,000 em. For a tagged formula, `Layout.width` is at
least the requested width and may grow to the pair's minimum fitting width;
the host must then fit or scroll the complete layout. Untagged displays retain
their existing intrinsic layout, independent of the supplied column width.
Glyphs, rules and constructed paths retain their exact source spans and paint
order. Label parentheses refer to the original `\tag` command span.

`tag_mathml()` and [ordinary HTML/EPUB mathematics](HTML_MATHEMATICS.md) share
the same text serializer, preserving nested styling and mathematical islands.
Plain text-only formulas retain their compact single-`mtext` spelling.

## Focused validation

`cargo test -p fmd-math --test display_tags` exercises strict tag parsing,
macro expansion and original spans, upright text, nested MathML text islands,
actual outline separation at multiple widths, negative spacing, deterministic
placement, combined output budgets and unchanged untagged layouts.

`cargo test --test display_equation_tags` exercises HTML cache isolation and
escaping, native/browser adapter parity, deterministic tagged PDF geometry and
independent text extraction when `pdftotext` is installed, vector SVG output,
Flow reflow and reading order, nested columns and complete-source fallbacks.
`cargo test --test epub_test explicit_equation_tags` checks that single-document
and book EPUBs keep both MathML trees, escaping, valid chapter XML and the
MathML manifest declaration with default or caller-supplied stylesheets.
