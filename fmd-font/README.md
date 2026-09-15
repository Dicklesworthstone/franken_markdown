# fmd-font

Dependency-free, safe Rust font mechanics for native and WASM consumers.
Markdown, score engraving, lyric alignment, and document layout stay outside
this crate.

## Strict font subsets

The additive `Font::try_subset`, `try_subset_glyphs`, and
`try_subset_glyphs_with_lookup` APIs return `Result<Subset, SubsetError>`.
`Subset` contains bytes, a dense original-to-subset glyph map, and an explicit
`EmbeddingFormat`. Glyph zero is retained; absent glyphs map to
`MISSING_GLYPH_REMAP`. Input order and duplicates do not change output bytes.

```rust
use fmd_font::{Font, EmbeddingFormat};
# fn example(bytes: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
let font = Font::parse(bytes)?;
let subset = font.try_subset_glyphs(&[font.glyph_index('A')], &['A'])?;
match subset.format {
    EmbeddingFormat::TrueType => { /* sfnt glyf: PDF FontFile2 */ }
    EmbeddingFormat::OpenTypeCff => {
        // OTTO, name-keyed CFF1: FontFile3 with /Subtype /OpenType.
        // Select a compatible simple-font encoding/dictionary. These are not
        // CID-keyed CFF bytes, and must not be labeled FontFile2.
    }
}
# Ok(()) }
```

Errors distinguish unsupported formats/operators, missing tables, invalid
requested glyph IDs, malformed data, resource budgets, and output capacity.
They carry a table tag and, when available, a glyph or local byte offset.
No error contains a host path. Reads propagate their failure where it occurs.

Existing Option-returning APIs retain their TrueType-only behavior and legacy
malformed-composite tolerance. Successful TrueType subset bytes and maps are
unchanged. Use the strict APIs when malformed input must be refused.

## CFF1 outlines

`Font::cff_outline(gid)` returns move, line, cubic-curve, and close commands,
plus horizontal metrics. It does not approximate cubic curves with quadratics.
The subsetter decodes selected outlines and emits deterministic, unhinted,
subroutine-free CFF1 with ascending glyph renumbering and rebuilt sfnt tables.
Sparse subsets preserve metrics and geometry; unused glyph programs disappear.
The caller must supply every pre-shaped glyph that it needs.

Supported Type 2 mechanics include standard drawing operators, flex operators,
stem/hint masks, and bounded local/global subroutines. Name-keyed CFF1 with the
standard design-unit FontMatrix is supported. CID-keyed fonts, CFF2, seac,
nonstandard matrices, and computational/random operators return typed refusals.
Charstrings have bounded stacks, recursion, instructions, and output commands;
subsetting also bounds total decoded commands. Font license obligations still
apply to embedding and redistribution.

## Explicit shaping runs

`Font::shape(text, &ShapeOptions)` accepts an explicitly supplied font, script,
OpenType language tag, direction, and feature switches. It returns glyph IDs,
advances and offsets in font design units, and UTF-8 source-cluster ranges.
`ShapedRun::logical_text()` retains the exact original text. Ligatures span
contributing input; attached marks share their base cluster. Glyphs are returned
in visual order with positive horizontal advances.

```rust
use fmd_font::shaping::{Direction, ShapeOptions};
# fn example(font: &fmd_font::Font) -> Result<(), fmd_font::shaping::ShapeError> {
let options = ShapeOptions {
    script: *b"arab",
    language: *b"dflt",
    direction: Direction::RightToLeft,
    features: &[],
};
let run = font.shape("بَب", &options)?;
assert_eq!(run.logical_text(), "بَب");
# Ok(()) }
```

Coverage is deliberately bounded:

- `latn` LTR: U+0020–U+024F and U+0300–U+036F combining marks.
- `arab` RTL: basic letters U+0620–U+064A, U+064B–U+065F and U+0670
  marks, Arabic-Indic digits, and ASCII punctuation. Unicode 17 joining types
  choose isolated/initial/medial/final forms; required ligatures then apply.
- GSUB single and ligature substitutions, including extension wrappers.
- GPOS single positioning, pair positioning (glyph and class formats),
  mark-to-base, and mark-to-mark, with format-1 anchors.
- Feature order: `ccmp`, `locl`, Arabic forms, `rlig`, `liga`, then `kern`,
  `mark`, `mkmk`. Explicit unknown features, required disabled features, and
  selected unsupported lookup mechanisms are errors.

**The caller performs bidi segmentation, normalization, and font selection.**
This API does not claim universal Arabic/font coverage. Contextual substitutions,
cursive attachment, mark-to-ligature, variable/device positioning, ZWJ/ZWNJ,
other scripts, and other directional combinations are outside this version's
contract. Missing glyphs, unpositioned marks, unsupported mechanisms, and
malformed fonts are distinct errors. Marks are never silently discarded.
There is no implicit host-font fallback. Limits are 4,096 input scalars, 64
consecutive marks, bounded layout reads, and checked positioning arithmetic.

## Independent verification

- Every outline and horizontal metric in licensed Bravura 1.392 (3,693 glyphs)
  is checked against FontTools, plus sparse subset/remap and repeatability tests.
- An original OFL shaping fixture is checked against HarfBuzz for Latin
  ligatures/pairs, combining and stacked marks, Arabic joining, RTL positions,
  lam-alef, clusters, and exact logical-text recovery.
- Malformed/truncated data, invalid glyphs, unsupported operators, recursion,
  resource limits, and deterministic hostile mutations have refusal tests.
- Existing TrueType tests and renderer goldens remain required release gates.

Oracle scripts live in `scripts/font-oracles/` in the source repository.
FontTools and HarfBuzz are development references only; neither is a runtime
or build dependency. See [OpenType CFF](https://learn.microsoft.com/en-us/typography/opentype/spec/cff),
[Type 2 charstrings](https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf),
[GSUB](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub),
[GPOS](https://learn.microsoft.com/en-us/typography/opentype/spec/gpos), and
[Unicode joining data](https://www.unicode.org/Public/17.0.0/ucd/ArabicShaping.txt).
