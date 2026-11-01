# Display mathematics in Flow

Flow renders display equations with the same bundled `fmd-math` typesetter used by the publication renderers. Standalone `$$` blocks and the parser's recognized `math` fences become `DisplayBlock::MathBlock { source }`. The block retains the exact formula source supplied by the shared AST.

````markdown
$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2}
$$

```math
\left(\begin{matrix}a & b \\ c & d\end{matrix}\right)
```
````

This capability applies to display math. Inline equations and table-cell mathematics retain their existing Flow text behavior. Ordinary prose, styled links, tables and literal code fences keep their existing layout paths.

## Geometry and rendering

Both shaped Flow output and the estimate-based `to_display_list` API use real mathematical outlines. The renderer creates a lazy, local bundled math engine, typesets in `Style::Display`, and resolves its glyph identities through the engine's own fonts. Hosts do not need system fonts, a separate formula image fetch, or an external TeX service.

Each glyph, fraction rule and constructed path becomes its own `DisplayItem::Path` in the engine's exact `Layout.order`. A glyph's contours remain together under nonzero filling, preserving holes. Explicit primitive colors remain attached to their paths as RGBA; uncolored primitives use the host's `math` palette role. Bezier controls are retained rather than replacing curved glyphs with plain characters or straight approximations.

The formula's enclosing geometry includes its layout advance, height and depth, every outline endpoint and control point, and 0.10 em of padding on each side. This preserves italic overhangs, large delimiters, accents and intentionally invisible phantom space. Flow converts the math engine's upward-positive coordinates to downward-positive display coordinates. It uses the configured body size and uniformly reduces the entire formula only when needed to fit the available column, then centers the resulting box. Nested lists and quotes retain their measured indentation; quotes keep their accent bar.

Measured layout bounds are retained separately from painting items. A phantom-only equation therefore occupies its real space without a visible placeholder, artificial link anchor or clipping item. Code highlighting preserves these bounds when it reconstructs a display list.

## Reading, navigation and fallback

Each math block contributes exactly one `AccessibleReadingRole::Math` node. Its text is the exact formula source, its span is the enclosing original AST source range, and it has no children. The same contract applies to successful equations and fallbacks. Paths also carry that enclosing span. A hit can navigate to the formula's source without inventing per-glyph Markdown or UTF-16 caret positions.

If the engine cannot parse or typeset a formula, or the formula exceeds an individual complexity limit, Flow displays a short `Math source fallback:` explanation followed by the complete source in the normal code-text lane. It publishes no partial mathematical geometry. The explanation is a visible text item; the single reading node still contains only the original source. Source text is neither truncated nor rewritten to make it render.

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
