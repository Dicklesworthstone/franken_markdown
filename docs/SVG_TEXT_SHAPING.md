# Shaped text in SVG exports

SVG paragraphs, headings, list content, definition text and table cells now use
the existing font engine's GSUB ligatures and GPOS pair adjustments. The same
shaped-glyph contract measures text and places its vector outline instances.
This replaces raw per-character advances for those text flows; SVG remains a
greedy poster layout, not the PDF Knuth-Plass or pagination engine.

Adjacent AST text fragments with identical styles are combined before shaping.
A split between `f` and `i` therefore cannot disable their ligature accidentally.
Actual style changes remain separate runs; they do not introduce an artificial
space or discretionary line break. Spaces, nonbreaking spaces and explicit
hard breaks retain their existing behavior. Code remains unligated and unkerned,
including preserved inline spaces and fenced-code tab stops.

Font layout tables come from each actual active face, including explicitly
supplied and instanced resource fonts. They are cached within each wrapping or
drawing pass. No system font lookup, font download, browser text shaping or new
runtime dependency is introduced. Fallback glyphs retain their producing slot,
and missing characters retain the existing report count.

## Long content and consistent geometry

Emergency wrapping walks shaped source clusters instead of adding raw scalar
widths. Ligatures and zero-advance marks are not split. Kerning to a glyph moved
to the next line is removed from the preceding line's width. Painting the new
line uses that same isolated-fragment geometry. The original UTF-8 text remains
in the wrapped words; no characters are discarded to force a fit.

An indivisible cluster can still exceed an extremely narrow measure. The export
preserves that cluster rather than truncating it. Negative optional kerning that
would reverse the pen is bounded, and invalid optional substitutions fall back
to source glyphs; `svg_shaping_adjusted` reports these repairs during painting.
This is not full contextual, bidirectional or mark-positioning support.

Glyph definitions are still deduplicated by actual face slot and shaped glyph
ID. A ligature can therefore use an outline that has no direct Unicode cmap
entry. Strikethrough endpoints follow the actual shaped advance. Images, math,
endnotes, themes and existing caller-owned resource policy remain unchanged.

## Regression checks and evidence limits

Eleven Rust tests exercise the actual wrapping/painter and public resource
exporter, using the existing original OFL shaping fixture. They pin the saved
HarfBuzz values for pair spacing and ligatures, equal-style joins, custom-face
identity, long-token line edges, literal code, missing glyphs, decorations,
zero-advance clusters and deterministic outline deduplication.

The authoring environment executed the installed native HarfBuzz against the
exact repository fixture (verified by Git blob hash), checking five independent
metric/cluster cases. That is a font oracle, not execution of the Rust changes.
Cargo, rustc, rustfmt and DSR were unavailable. These Rust tests, the full native
suite, SVG visual regressions and generated-WASM integration remain unexecuted;
no build, rendering-quality acceptance or performance pass is claimed.
