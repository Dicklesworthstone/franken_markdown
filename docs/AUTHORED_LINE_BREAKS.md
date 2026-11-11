# Authored line breaks in PDF and SVG prose

PDF and SVG rendering recognize two Unicode characters as optional line-break
hints in running text. They work through the shared render APIs without a new
CLI flag or library option.

| Character | Markdown spelling | When the line continues | When a break is selected |
| --- | --- | --- | --- |
| Soft hyphen, U+00AD | `&shy;` or the literal character | No visible glyph or advance | A visible `-` at the end of the line |
| Zero-width space, U+200B | `&#x200B;` or the literal character | No visible glyph or advance | A line break with no added glyph or space |

For example, this Markdown gives the renderer an authored hyphenation point in
a long word and unhyphenated break opportunities in an identifier:

```markdown
An inter&shy;nationalization report.

customer&#x200B;Account&#x200B;Identifier
```

Render the file normally:

```bash
fmd report.md --to pdf --out report.pdf
```

## Layout and text behavior

The hints participate in measured paragraph and table-cell wrapping. Unused
hints do not produce missing-glyph boxes or warnings, consume horizontal space,
or insert spaces into extracted text. The surrounding text still uses its
ordinary font shaping, including ligatures and kerning across an unused hint.
A selected soft hyphen uses the hint's text style and reserves room for its
visible hyphen.

Adding a hint can change the chosen lines even when that particular hint stays
unused. Multiline optimization measures the intervening fragments; final
painting shapes the assembled line. A paragraph that fits on one line uses a
bounded whole-paragraph fit check to preserve its contextual shaping metrics.

In PDF, an authored hint takes precedence over dictionary hyphenation within
that source word. SVG uses the authored opportunities without adding dictionary
hyphenation. A hint is an opportunity rather than a forced break: the line
breaker can leave it unused. Existing measured emergency breaks remain available
when a fragment is wider than its containing line, including in narrow lists,
quotations and table cells.

Leading and trailing hints do not create empty lines. Consecutive hints at one
boundary collapse into one opportunity; a soft hyphen takes precedence over a
zero-width space. A hint between a base character and its following supported
combining marks (U+0300–U+036F) is ignored so that it cannot separate the cluster.

## Scope

This behavior applies to PDF and SVG prose, including styled text and link labels.
Inline code, fenced code and literal TeX fallback text retain their source
characters. A zero-width character in literal code may therefore still produce
the existing missing-glyph diagnostic if the selected code font lacks it.

The Markdown AST retains these Unicode characters. HTML and EPUB retain their existing
Unicode output and rely on the reader's layout engine. The SVG poster renderer
retains the fully joined word for unused hints and reshapes each selected
physical fragment before checking its actual width. Authored candidate and
repair limits fall back to complete visible text through the existing
cluster-safe wrapper; see [SVG text flow](SVG_RENDERING.md#text-flow).

## Regression coverage

Run `cargo test --test pdf_discretionary_breaks_test` for emitted-PDF checks of
unbroken byte parity, tight ligature and kerning fits, chosen hyphens and
zero-width breaks, page bounds, tables, styled/link text, combining marks,
literal-code ownership and formula-adjacent hints.

Run `cargo test --test svg_test` for SVG checks of authored break selection,
exact fitting widths, ligatures, kerning, styles, links, combining marks,
literal code, nested blocks and bounded fallback diagnostics.
