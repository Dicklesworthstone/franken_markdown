# PDF mathematics

PDF output typesets display equations with the shared, first-party `fmd-math`
engine. Single-line and multiline `$$` blocks and fenced `math` blocks produce
native vector outlines, including fractions, radicals, scripts, operator limits,
matrices and supported stretchy delimiters. No browser or external typesetter
is required.

```markdown
$$
\frac{a+b}{c+d} = \sqrt{x^2+y^2}
$$
```

Use the normal PDF command:

```sh
fmd equations.md --to pdf --out equations.pdf
```

The library, native book publisher and browser PDF adapter consume the same
display-math layout. Equations inherit body-size settings and the theme's
foreground color, center within their current column, and scale down when
necessary to fit the page. Their measured height participates in pagination,
including equations nested in blockquotes, lists and footnotes.

Explicit `\color` and `\textcolor` declarations override that inherited ink
within their TeX scope, for glyphs, fraction rules and constructed paths alike.
The shared engine supports xcolor's base names, mixes such as `red!30!blue`,
and the `[rgb]`, `[RGB]`, `[HTML]` and `[gray]` models. The same resolved colours
appear in PDF and SVG output. For example, `\textcolor{red}{\frac{a}{b}}`
colours both letters and their fraction bar without changing the formula's
geometry or the ink of surrounding content.

## Text and accessibility

The visible equation uses outlines from the math engine's bundled fonts.
PDF structure identifies it as `/Formula`, retains its TeX source as `/Alt`,
and supplies `/ActualText` through an invisible text anchor. Copying or
extracting a formula therefore returns its source rather than an unrelated
placeholder glyph. Surrounding prose remains normal selectable text.

## Limits and fallback

A display formula accepts at most 64 KiB of source, 4,096 layout primitives
and 262,144 outline segments. Geometry must be finite and bounded. Unsupported
commands or exceeded limits preserve the original TeX as readable text and
produce a `math_fallback` render warning. Successful formulas use math-font
coverage and do not incorrectly report missing glyphs in the prose fonts.

## Inline formulas

Inline `$…$` in prose, headings, list items, definition lists and table cells
goes through the same engine, in text style at the line's font size. Each
formula is one unbreakable box in the paragraph's line breaking. It never
splits, and the words around it do not hyphenate against it. Its advance is the
formula's layout width. Its outlines are filled on the text baseline in the
run's colour unless explicitly overridden by a scoped colour declaration.
A formula inside a link keeps the link colour for its uncoloured parts, together
with the link's underline and annotation.

Inline `$$…$$` retains display style in PDF as it does in HTML and SVG.
Large operators use their display-size glyphs and limits, and fractions use
the display layout even inside a sentence, heading or table cell. The result
remains one inline box; its actual display-style dimensions drive wrapping,
line height, baseline-grid placement and tagged formula bounds. Single-dollar
`$…$` formulas retain their existing text-style layout.

A formula taller than the text grows its line. Ink above 1.0 em or below 0.3 em
of the line's size adds that much leading, and the baseline rises by the extra
depth, the way TeX's box heights and depths space lines. A line holding a
formula takes its justification as spacing rather than `Tz` glyph expansion,
because outlines do not scale with `Tz`.

Each inline formula is its own `/Formula` structure element inside its
paragraph, with its TeX source as `/Alt` and a bounding box. Like display
equations, it carries `/ActualText` through an invisible anchor glyph.
Unsupported sources keep their visible TeX in the monospace face with a
`math_fallback` warning. A formula wider than the text measure at body size
overflows its line whole, with a `math_overflow` warning.

## Verification

`cargo test --test pdf_math_test` exercises display formula geometry, native/browser
adapter byte parity, deterministic emission, inherited and scoped explicit
colors (including graphics-state restoration), fallbacks, nested
containers, narrow pages, and Unicode coverage. When installed, independent
`pdftotext` checks confirm TeX and Unicode extraction. The TOC layout has
separate measured-column tests in `src/pdf/toc_tests.rs`; long titles wrap
without overlapping their page numbers and retain their heading links.
`cargo test --test pdf_inline_math_test` covers inline outlines and tagging, line
growth against the formula's own metrics, fallback and overflow warnings, and
headings and table cells.
