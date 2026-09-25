# Shaped text, source coordinates, and interaction

`fmd_font::text_run::OwnedTextRun::from_shaped_run` is the active bridge from
`Font::shape` into owned glyph runs. It now preserves the shaper's visual glyph
order in both directions. The separate cluster inventory is always in logical
source order; RTL cluster positions therefore descend while glyphs remain in
visual drawing order. Conversion never mirrors already-positioned glyphs.

Every cluster retains its actual glyph interval, font identity, original UTF-8
range, UTF-16 range, and visual advance bounds. Base/mark groups retain shared
source ranges and all individual positioning offsets, including zero-advance
marks that occur before their base in visual order. Conversion counts each
source slice once instead of scanning all earlier text for every cluster.

Admission refuses direction mismatches, missing or overlapping source coverage,
non-monotone directional groups, invalid scalar boundaries, missing glyphs,
negative horizontal advances and non-finite scaled metrics. Size and scale must
be finite and positive. The operation returns a complete owned run or an error;
it never changes the input `ShapedRun` or exposes partially converted geometry.
This does not make the public mutable struct immune to subsequent host changes.

## Atomic caret coordinates

`caret_at_byte` and `caret_at_utf16` resolve to actual cluster edges. Inside a
ligature or base-plus-mark cluster, Leading chooses its logical start and
Trailing its logical end. The returned source offsets identify the chosen edge,
not the originally requested interior position. For example, byte 1 inside the
`fi` ligature resolves to byte 0 or byte 2; it is never paired with an invented
half-glyph advance. Multibyte-scalar and surrogate-pair interiors are refused.

At a shared source boundary, Leading uses the following cluster's leading edge
and Trailing the preceding cluster's trailing edge. At either document endpoint,
only the existing inward cluster edge is available. This also keeps hit-test
results round-trippable through byte and native-coordinate caret lookup.

Hit testing uses actual cluster geometry. Exterior clicks and gaps produce
inexact nearest-edge hits; infinities clamp to the visual extremes. NaN gives an
inexact logical-start caret. Empty runs give an inexact zero caret. Distance
comparisons avoid overflowing a midpoint when valid f32 positions are large.

## Selection rectangles

Selections require in-bounds UTF-8 scalar boundaries and finite coordinates with
positive height. Selecting part of an atomic cluster covers that cluster's full
advance. Returned rectangles are visually ordered. Adjacent and overlapping
intervals are unioned in either direction; genuine visual gaps remain separate.
Malformed selected geometry rejects the whole selection rather than returning
only its valid portion. Hosts still own layout-revision fencing and presentation.

This repairs the shared text-run contract, not automatic paragraph bidi
segmentation, universal shaping, Arabic font bundling, or RTL browser reflow.
The existing bundled flow and Canvas profiles retain their stated LTR limits.
Hosts providing independently shaped directional runs remain responsible for
bidi segmentation, font selection and paragraph-level visual ordering.

## Regression checks

```sh
cargo test -p fmd-font --test text_run_test --test text_run_direction_test --test text_run_interaction_test
cargo test --no-default-features --lib fonts::flow
```

The directional tests use the repository's real shaping font and all 14 saved
HarfBuzz cases at three scales, checking visual glyph data, logical partitions,
RTL endpoints and mark membership. Interaction tests cover ligatures, combining
clusters, Arabic source domains, caret round trips, RTL selection unions,
explicit host-supplied discontiguous geometry and invalid numeric inputs.

These Rust tests were added but could not be executed in the authoring
environment, which lacks cargo, rustc, rustfmt and DSR. Blob identity and
whitespace checks are not compilation, runtime, browser or rendering proof.
