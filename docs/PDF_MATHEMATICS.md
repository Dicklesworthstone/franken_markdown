# PDF display mathematics

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

Inline formulas in running prose still render as literal TeX in PDF. The
display-equation path does not introduce inline image or formula objects into
the paragraph layout model.

## Verification

`cargo test --test pdf_math_test` exercises formula geometry, native/browser
adapter byte parity, deterministic emission, theme colors, fallbacks, nested
containers, narrow pages, and Unicode coverage. When installed, independent
`pdftotext` checks confirm TeX and Unicode extraction. The TOC layout has
separate measured-column tests in `src/pdf/toc_tests.rs`; long titles wrap
without overlapping their page numbers and retain their heading links.
