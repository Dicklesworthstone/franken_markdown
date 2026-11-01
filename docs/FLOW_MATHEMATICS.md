# Mathematics in Flow

Flow renders display equations with the same bundled `fmd-math` typesetter used by the publication renderers. Standalone `$$` blocks and the parser's recognized `math` fences become `DisplayBlock::MathBlock { source }`. The block retains the exact formula source supplied by the shared AST.

````markdown
$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2}
$$

```math
\left(\begin{matrix}a & b \\ c & d\end{matrix}\right)
```
````

Shaped Flow also typesets inline equations in prose, headings, lists, quotations, and table cells. `$...$` uses the engine's text style; inline `$$...$$` uses its display style. Literal inline code and ordinary code fences retain their source text. The estimate-based `to_display_list` API continues to show inline TeX as text because it has no truthful text metrics for mixed-line composition.

```markdown
The ratio $\frac{a^2+b^2}{\sqrt{c}}$ changes with $x$.

| Quantity | Definition |
| --- | --- |
| Mean | $\frac{1}{n}\sum_{i=1}^{n}x_i$ |
```

## Geometry and rendering

For display blocks, both shaped Flow output and the estimate-based `to_display_list` API use real mathematical outlines. The renderer creates a lazy, local bundled math engine, typesets in `Style::Display`, and resolves its glyph identities through the engine's own fonts. Hosts do not need system fonts, a separate formula image fetch, or an external TeX service.

Each glyph, fraction rule and constructed path becomes its own `DisplayItem::Path` in the engine's exact `Layout.order`. A glyph's contours remain together under nonzero filling, preserving holes. Explicit primitive colors remain attached to their paths as RGBA; uncolored primitives use the host's `math` palette role. Bezier controls are retained rather than replacing curved glyphs with plain characters or straight approximations.

The formula's enclosing geometry includes its layout advance, height and depth, every outline endpoint and control point, and 0.10 em of padding on each side. This preserves italic overhangs, large delimiters, accents and intentionally invisible phantom space. Flow converts the math engine's upward-positive coordinates to downward-positive display coordinates. It uses the configured body size and uniformly reduces the entire formula only when needed to fit the available column, then centers the resulting box. Nested lists and quotes retain their measured indentation; quotes keep their accent bar.

Measured layout bounds are retained separately from painting items. A phantom-only equation therefore occupies its real space without a visible placeholder, artificial link anchor or clipping item. Code highlighting preserves these bounds when it reconstructs a display list.

## Inline composition and host fonts

`FlowInlineRun::math` preserves each formula as a separate atomic range in the block's reading text, including internal TeX newlines. Adjacent formulas are never coalesced into one expression. Normal text keeps its styled font shaping; formula contours use the math engine's own faces. Inherited links and strikethrough remain attached to the measured inline object.

An equation participates in line wrapping using its actual width, ascent, and depth. It moves to the next line as one object when the current line lacks space. Only an equation wider than the entire available column is uniformly reduced. Tall fractions and delimiters increase line height; neighboring text uses the same baseline. Table allocation measures mathematical content before choosing column widths and row heights. Decimal alignment does not interpret TeX punctuation as a prose numeric anchor.

The bundled font bridge supplies `OwnedTextRun::vertical_metrics: Some(TextRunMetrics { ascent, descent })` from the actual faces, glyph bounds, and positioning offsets. Custom shapers can supply the same nonnegative, finite layout-unit metrics. A mixed line requiring absent vertical metrics preserves its formulas as complete source with an explicit fallback reason; it does not estimate a font baseline. Invalid supplied metrics are rejected. Existing shaping callback signatures remain unchanged.

Text in a measured line carries `DisplayTextRun::baseline_y`; browser wire data calls it `baselineY`. Canvas uses this native baseline for ordinary text and composed mathematics. Known list-marker metrics join the first body row, including its enlarged height for tall formulas. Text without the optional field keeps its established font-derived baseline policy. The same measured-row contract supports the optional [CJK font profile](FLOW_CJK_TEXT.md).

## Reading, navigation and fallback

Each math block contributes exactly one `AccessibleReadingRole::Math` node. Its text is the exact formula source, its span is the enclosing original AST source range, and it has no children. The same contract applies to successful equations and fallbacks. Paths also carry that enclosing span. A hit can navigate to the formula's source without inventing per-glyph Markdown or UTF-16 caret positions.

Inline formulas stay inside their parent reading paragraph, heading, or table cell. Their reading runs have `math: "inline"` or `math: "display"` and exact UTF-8 ranges in that parent's logical text. The semantic reader presents the TeX as selectable `role="math"` text, preserving surrounding styles and safe links. These ranges are reading-text coordinates, not Markdown offsets, and do not imply a TeX-to-glyph caret map.

If the engine cannot parse or typeset a formula, or the formula exceeds an individual complexity limit, Flow displays a short `Math source fallback:` explanation followed by the complete source in the normal code-text lane. It publishes no partial mathematical geometry. The explanation is a visible text item; the single reading node still contains only the original source. Source text is neither truncated nor rewritten to make it render.

For an inline formula, source fallback remains in the paragraph's code-text lane. Each emitted source fragment carries a bounded `math_fallback` reason (`mathFallback` in browser wire data), while the reading text retains the complete original TeX. This also reports missing host vertical metrics. Formula source and any fallback diagnostics remain subject to the shared work and output limits.

Shaped fallback text uses the host's normal font and wrapping contract. As with other Flow code text, an indivisible cluster that cannot fit or an exhausted global layout budget returns a typed error. `FlowSession` retains the last complete display when such a reflow fails.

## Resource limits

The math lane adds no dependencies or ambient I/O. Its limits complement existing source, AST, nesting, block, line and item limits:

| Resource | Limit and behavior |
| --- | --- |
| Source per formula | 64 KiB; larger formulas use visible source fallback |
| Engine primitives per formula | 4,096 glyphs, rules and constructed paths combined; excess uses fallback |
| Commands per filled primitive | 65,536, including contour starts and closes; excess uses fallback |
| Commands per formula | 262,144; excess uses fallback |
| Retained commands across a shaped display | 262,144; exhaustion returns `FlowLayoutError::BudgetExceeded` |
| Input and retained command payload | Formula source bytes plus `size_of::<DisplayPathCommand>()` per retained command share `max_total_shape_bytes` with host shaping work |
| Engine coordinates | Finite values within ±1,000,000 em before placement; invalid geometry uses fallback |
| Diagnostic explanation | At most 160 characters of the engine reason, after a fixed prefix |

Command limits apply after actual glyph-outline expansion, not merely to the number of source tokens. Remaining global command and byte budgets are checked while resolving primitives. Explicit colors and complete contour grammar are validated before a path enters the display list. Per-formula fallbacks remain subject to ordinary shaped-text budgets; they cannot bypass global resource limits.

The compatibility `to_display_list` method cannot return a layout error. It uses the default math byte ceiling and the same command ceiling, showing source fallback if that fixed allowance is exhausted. Its ordinary text remains estimate-based. Hosts needing typed resource refusal and shaped fallback should use `to_shaped_display_list`, `to_styled_display_list`, or the session APIs.

## Validation scope

`tests/flow_math_display.rs` exercises the public Flow/session APIs using bundled fonts. Its geometry checks compare actual glyph, rule and constructed-path landmarks with the shared math engine, including a drawn arrow, fractions, tall matrices and colors. Other cases cover nested source spans, recognized math fences, unsupported and complex source fallback, phantom geometry through code highlighting, narrow/wide reflow, batch invariance, viewport pagination, global-budget rollback, and browser wire/source-hit behavior.

`tests/flow_inline_math.rs` compares real text-style and display-style engine landmarks against adjacent text baselines. It covers atomic wrapping, mixed fonts and links, expanded line and table geometry, multiline formulas, literal-code controls, source-only path hits, complete fallback, missing metrics, and transactional budget failures. The browser acceptance harness exercises the same public session path through native wire fixtures and generated WASM.
