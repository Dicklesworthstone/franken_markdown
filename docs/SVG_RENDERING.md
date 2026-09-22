# SVG poster rendering

SVG is a standalone, single-page export. Renderer-generated text uses
outlined fonts and deterministic glyph definitions, without `<text>`,
`foreignObject`, JavaScript or a browser math dependency. Explicit PNG/JPEG/SVG
image resources retain their own content; see `SVG_RESOURCES.md`.

## Mathematics

Inline math, display-style inline math, and math blocks use the existing
`fmd-math` TeX engine. The adapter preserves positioned glyph face identities,
script scales, fraction and overline rules, and drawn radical/delimiter paths.
All geometry is converted from y-up em coordinates to the SVG page's y-down
point coordinates. Glyph outlines use the existing deterministic definitions.

The layout box includes both advances and ink bounds, including italic
overshoot. Inline lines and table rows grow to accommodate formula ascent and
descent; display blocks are centered. A formula wider than its available
measure scales uniformly as an indivisible box instead of clipping or breaking
apart. This may make unusually large equations small: the poster is not a
multi-page equation-breaking engine.

Math fonts initialize only when a formula needs them. Plain prose does not
pay for the math face roster. The adapter adds no third-party dependency.

Unsupported mathematics stays visible as monospaced source. To distinguish a
fallback from successfully typeset math, use the additive diagnostic API:

```rust
use franken_markdown::{parse_markdown, svg::{SvgOptions, render_svg_with_diagnostics}};

let doc = parse_markdown("A formula: $x^2 + \\frac{1}{y}$.");
let (bytes, report, warnings) = render_svg_with_diagnostics(&doc, &SvgOptions::default());
for warning in warnings {
    eprintln!("{}: {}", warning.code, warning.message);
}
# let _ = (bytes, report);
```

Stable codes: `svg_math_unsupported` (shared-engine parsing/layout error),
`svg_math_fonts` (unavailable face), `svg_math_limit` (source or geometry count),
and `svg_math_geometry` (invalid numeric geometry). Limits are 64 KiB source,
65,536 primitives, 65,536 drawn-path segments, and finite coordinates of at
most one million em in magnitude. Engine-side parsing limits still apply.
Diagnostics are returned in paint order, once per failed occurrence, not during
table measurement. Existing `render_svg` and `render_svg_with_report` signatures
remain unchanged and discard these additional diagnostics.

`SvgReport.paths_emitted` counts unique **glyph definitions**, not standalone
radical/delimiter paths. `glyphs_drawn` counts emitted glyph uses, including math.

## Text flow

Styling boundaries do not insert spaces or create word breaks. Source spaces
become explicit measured gaps; consecutive hard breaks retain blank lines.
Inline-code spacing and nonbreaking spaces are preserved. Overlong tokens use
emergency scalar-boundary wrapping without dropping characters. Fenced code
wraps with measured advances, preserves spaces, and expands tabs to four-column
source stops. Its panel height includes every wrapped line. Raw HTML is drawn
as inert source text, never executed or silently discarded.

## Endnotes

All SVG entry points prepare complete numbered endnotes using the same AST
pass as PDF. References become `[n]`, and a Notes section follows the document.
Code, lists, quotes, tables, equations, images, headings and inline formatting
inside notes keep their structure instead of being flattened or omitted.

Numbering follows first use in the main body, then references found in those
notes. Cycles do not expand recursively: each definition is emitted once.
Unreferenced definitions follow in source order; duplicate identifiers use the
first definition. Undefined references remain visible as `[^id]`. These are
endnotes, not page-bottom footnotes or clickable superscripts.

`Document::with_endnotes()` exposes the shared preparation for other hosts.
It never mutates the source. Ordinary documents and already-prepared documents
are borrowed, so explicit preparation followed by SVG/PDF rendering neither
clones a note-free AST nor creates a second Notes section. Resources and
recoverable warnings inside notes use the ordinary SVG rendering path.

## Typography and page geometry

The shared theme is resolved before layout. Its integral `spacing.base_px` maps
16 CSS pixels to the existing 11-point body baseline; the shared `TypeScale`
resolves the complete heading/body/code/table ladder with a 6..24-point body
range. All named font-scale presets therefore affect actual glyph sizes, line
breaks and poster height. Custom factors inherit the theme's whole-pixel root
rounding; this is not a final transform of a fixed-size rendering.

`max_width_pt` remains a physical poster width in points. All four page margins
come from `theme.page.margins`. Asymmetric margins are retained when they fit;
if necessary, left/right margins are reduced proportionally to preserve 72
points of content width. Negative, non-finite or over-limit numeric values
receive deterministic defaults and `svg_layout_adjusted` warnings. Supported
width is 144..14400 points, individual margins are 0..14400 points, body leading
is 1..4, and table padding is 0..8 em. A zero-margin empty poster still has a
positive one-point height.

Table padding is measured in ems of the effective table size. This intentionally
changes the historical fixed 6/4-point padding: default theme values now control
table density. Code panel padding and quote/definition spacing scale with the
body. Nested containers stop consuming indentation when one body em remains.
List items share a gutter measured from their actual font and largest marker;
when a usable hanging layout cannot fit, the complete marker wraps above the
item rather than overlapping it. Empty items still receive a line. Ordered
list ordinals continue past a host-supplied `u64::MAX` start without overflow.

Mathematics scales with text; image dimensions retain their intrinsic point
size, subject to fitting the available measure. Explicit theme appearance uses
its selected palette. Auto is deterministic light, with no system appearance
lookup or media-query behavior. The CSS readable-measure/radius tokens, PDF
paper height, and code-ligature policy are not implemented by this poster path.

Default-size prose retains its historical metrics. Tables, invalid geometry,
wide list markers and explicitly scaled or appearance-configured documents have
intentional output changes. Existing SVG option/report structures are unchanged.

## Remaining scope

Prose remains greedily wrapped using advances, without the PDF renderer's
Knuth–Plass optimization or complex-script shaping. Emergency token wrapping
is not a full Unicode grapheme/line-break implementation. Unresolved images
retain alt-text placeholders. The poster is single-page with deterministic
explicit appearance selection.
SVG changes do not change the HTML or PDF rendering algorithms.

## Verification

`src/svg/text_tests.rs` and `src/svg/math_tests.rs` exercise the actual painter,
including direct comparisons to the shared TeX engine's glyph/rule geometry,
vertical clearance, width fitting, fallback diagnostics, and determinism. They
run both in library tests and the existing `tests/svg_test.rs` standalone path.
`tests/svg_endnotes_test.rs` checks complete SVG output against explicitly
prepared documents, resource-bearing notes, cycles and PDF preparation parity.
`src/svg/geometry_tests.rs` checks real painter placement and sizing, and
`tests/mcp_svg_geometry_test.rs` compares configured file responses and saved
artifacts against the native core.

The implementation session checked source hashes and `git diff --check` but
had no Rust toolchain or DSR runner. Compilation, tests, Clippy, rustfmt and
visual raster acceptance must be run in the normal DSR environment; this file
does not represent those gates as passed.
