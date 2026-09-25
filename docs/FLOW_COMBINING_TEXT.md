# Combining text in continuous-flow output

`BundledFlowFonts::shape` now admits supported decomposed Latin accents instead
of rejecting every U+0300–U+036F sequence. This is the adapter used by the
browser/editor flow session; no new JavaScript option or external font service
is required. Rebuild the Rust/WASM artifact alongside its wrapper.

The adapter first tries complete canonical Latin composition with a real glyph
in the requested face. For example, `e` followed by U+0301 can use the face's
`é` glyph. The temporary glyph text never replaces the document: the owned run
retains the original source, UTF-8 bytes, UTF-16 offsets, and base-plus-mark
selection boundaries. Whole-run composition preserves ordinary ligatures,
kerning, font styles and code-tab behavior. No compatibility normalization,
accent removal, guessed glyph, or cross-face mark attachment occurs.

The checked-in table contains 497 canonical pairs from Unicode 15.1.0, restricted
to Latin U+00C0–U+024F and U+1E00–U+1EFF with marks U+0300–U+036F. Composition
exclusions are respected. This is a bounded glyph-selection path, **not a
complete NFC implementation**: it does not reorder arbitrary marks or normalize
other scripts. A sequence must compose completely or proceed unchanged to the
positioned path.

Sequences needing real positioning use the existing strict OpenType shaper.
The selected face must cover the entire sequence. Its actual glyph advances,
mark-to-base/mark-to-mark offsets, and shared source clusters survive into the
owned run and existing Canvas glyph painter. Missing glyphs, unsupported font
lookups, unattached marks, and script/joining sequences outside the Latin
profile still fail explicitly. Support depends on the selected face; this is
not universal multilingual shaping. Arabic/Hebrew bidi, emoji sequences, and
cross-style combining clusters still need broader host shaping support.

Run ingress remains bounded by 64 KiB and 16,384 scalars. Strict positioned
segments additionally retain the font engine's 4,096-scalar and 64-consecutive-
mark limits. A failed operation returns no partial owned run; existing session
transaction semantics continue to preserve the last successful layout.

## Checks

```sh
node --test scripts/font-oracles/flow_composition_data.test.mjs
cargo test --lib fonts::flow
```

The Node test checks the actual production data against ICU NFC/NFD, including
all supported two-step composition chains. It does not execute Rust. Rust tests
cover the existing independent HarfBuzz fixture, stacked mark geometry, mixed
font rebasing, exact source domains, bundled styles/code, and real narrow
heading/table reflow. A generated WASM build and browser/platform acceptance
remain separate requirements; adding these tests is not a passing build claim.
