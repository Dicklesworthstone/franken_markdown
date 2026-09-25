# Combining text in SVG exports

SVG text now uses supported decomposed Latin accents in the active font. Common
complete base/mark chains use an existing canonical composite glyph, sharing the
reader's fixed 497-pair Unicode table rather than maintaining another mapping.
The original UTF-8 source and emergency-wrapping boundaries remain unchanged;
no compatibility normalization, accent stripping or arbitrary mark reordering
is performed. A composite must exist in the requested face.

When a sequence cannot compose completely, SVG uses the existing strict Latin
OpenType shaper. Base and marks must be covered by one actual active face. GPOS
mark-to-base and mark-to-mark offsets, zero advances, ligatures and source-cluster
ranges survive into the emitted vector glyph instances. Fonts are explicit
caller resources or existing bundled faces, never discovered or downloaded.
Optional ligatures and pair kerning stay disabled in code, while supported marks
are still positioned. Unsupported font mechanisms and sequences retain the raw
glyph fallback with an `svg_text_shaping_unsupported` diagnostic; missing glyphs
also retain the existing missing-character count. Inspect diagnostics before
claiming faithful export of text outside the supported profile.

## One prepared run for layout and paint

Wrapped text retains the exact prepared glyph data used to measure it. Painting
consumes those data, rather than independently shaping again. Actual-face glyph
bounding boxes and positioning offsets determine additional ascent/descent.
Paragraphs, headings, list content, definitions, table rows and code panels grow
when those bounds exceed their nominal line box. Heading rules sit below the
last line's deepest ink, including descenders. The geometry relies on the font's
glyph bounds; this is not outline recomputation or a promise to repair false
bounds in malformed host fonts. Ordinary lines that fit their nominal metrics
retain them; previously intersecting heading rules and overflowing ink change.

Emergency breaks keep base/mark and ligature clusters intact. Simple pair runs
remove the outgoing pair adjustment. Strictly positioned runs reshape each
actual isolated substring at the proposed cut, since a pair can affect both
neighboring glyphs. A fit check can shorten the cut at complete cluster boundaries.
Both the repaired measure and the painted result come from the retained isolated
run. This remains greedy wrapping, not paragraph-optimal or bidirectional layout.
An indivisible overwide cluster remains visible rather than being truncated.

Code expands tabs using four-column source stops and preserves literal spaces
and blank rows. Its panels sum actual row heights, so stacked marks do not escape
the panel or collide merely because a fixed nominal leading was assumed.

## Limits and unresolved scope

The strict positioning route admits at most 64 KiB / 4,096 scalars per run and
keeps the font engine's 64-consecutive-mark limit. Refusals produce
`svg_text_shaping_limit` or `svg_text_shaping_unsupported`, not silent success.
Boundary backtracking has a separate 64-repair ceiling per word; exhaustion
preserves the entire remaining source as one potentially overwide run and emits
`svg_text_wrap_limit`. These are bounds on the added positioning/repair work, not
whole-document memory or time limits. Hosts must still bound their input.

Cross-style combining attachment, arbitrary-script shaping, bidi paragraphs and
universal normalization remain outside this SVG profile. The PDF ragged/final-
line overflow issue is independent and is not changed by this implementation.

## Verification

Five composition regressions and twelve positioning/layout regressions were
added. They exercise exact source ranges, active font identity, retained widths,
HarfBuzz fixture offsets, stacked accents, code spacing, emergency cuts, table
rows, heading rules, diagnostic fallback and public SVG resource export.

The authoring environment ran Python's independent Unicode normalization check
for all 497 shared composition pairs, six native HarfBuzz fixture checks, and an
independent wrapping/line-box model over native glyph output: 1,120 scenarios,
23,145 rows, and 49,770 glyph bounds. The model is NOT Rust execution. Fixture
and changed-file Git blob hashes were checked. No Rust compiler, Cargo, rustfmt,
DSR or generated WASM was available; the Rust tests, full suite and actual SVG
visual acceptance remain unexecuted.

```sh
cargo test --no-default-features --lib svg
cargo test --no-default-features --lib fonts::flow
node --test scripts/font-oracles/flow_composition_data.test.mjs
```

Rebuild the matching native/WASM package before expecting browser or CLI exports
to include these source changes. The added public, font-independent
`BundledFlowFonts::canonical_latin_composite(base, mark)` helper performs only a
single fixed-table lookup; other backends must enforce complete-sequence policy,
font coverage and source mapping themselves.
