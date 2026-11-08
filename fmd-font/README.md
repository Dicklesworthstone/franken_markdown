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

Large BMP-only subsets retain their complete mappings: when the historical
format-4 representation no longer fits, the writer compacts adjacent mappings
and can use a Unicode format-12 map. Small-subset bytes stay unchanged.

The optional `cjk-face` feature exposes `bundled::noto_sans_cjk_sc()`, a static
Regular-weight Noto Sans SC subset for 28,215 mapped Han/kana/punctuation
characters. It implies `bundled-faces`; it adds no runtime dependencies. See
[the artifact's repertoire and provenance](fonts/noto-sans-cjk/README.md).

## CFF1 outlines

`Font::cff_outline(gid)` returns move, line, cubic-curve, and close commands,
plus horizontal metrics. It does not approximate cubic curves with quadratics.
The subsetter decodes selected outlines and emits deterministic, unhinted,
subroutine-free CFF1 with ascending glyph renumbering and rebuilt sfnt tables.
Sparse subsets preserve metrics and geometry; unused glyph programs disappear.
The caller must supply every pre-shaped glyph that it needs.

Supported Type 2 mechanics include standard drawing operators, flex operators,
stem/hint masks, and bounded local/global subroutines. Name-keyed and CID-keyed
CFF1 with the standard design-unit top FontMatrix are supported. CID fonts
use FDSelect format 0 or 3 to select each glyph's font dictionary and private
subroutines. Calls from global subroutines retain that same local dictionary.
The sfnt Unicode `cmap` addresses glyph IDs independently of the source CIDs;
normalized subsets remain name-keyed OpenType CFF with fresh ascending glyph
IDs, retaining the existing embedding format.

CFF2, per-font-dictionary matrices, nonstandard top matrices, seac, and
computational/random operators return typed refusals. CID font dictionaries,
selectors, and charsets are validated before any glyph is decoded, including
structures for glyphs outside the requested subset. Charsets support formats
0, 1 and 2 and must contain unique, in-range CIDs. Selectors must cover every
glyph with valid font-dictionary indices; range selectors require ordered
ranges beginning at glyph zero and an exact glyph-count sentinel.

Validated CFF directories are initialized once per font and shared by parsed
clones. CharStrings and subroutine INDEXes retain compact offsets into the
immutable font bytes: decoding a glyph does not allocate or walk the entire
font's glyph array again. Cache initialization and use are thread-safe and
do not depend on host files, font discovery, or a platform shaping engine.
Directory parsing has a shared work limit that also bounds aliased structures.
Charstrings have bounded stacks, recursion, instructions, and output commands;
subsetting also bounds total decoded commands. Font license obligations still
apply to embedding and redistribution.

[`fonts/test-cid-cff`](fonts/test-cid-cff/README.md) contains tiny original
fixtures and independently decoded FontTools references for both selector
formats, multiple private dictionaries, nested global/local subroutines,
nonidentity CIDs, cubic bounds, and sparse Unicode subsets. Font collections
(`.ttc`/`.otc`) use the explicit extraction API below; `Font::parse` continues
to accept standalone faces only.

## Font collections

`collection_face_count(bytes)` validates a TTC/OTC header and its face-directory
pointers and returns the number of faces. `extract_collection_face(bytes,
face_index)` selects a zero-based face and returns standalone sfnt bytes for
the existing font and renderer APIs:

```rust
use fmd_font::{Font, collection_face_count, extract_collection_face};
# fn example(collection: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
let count = collection_face_count(collection)?;
if count > 0 {
    let selected = extract_collection_face(collection, 0)?;
    let font = Font::parse(selected)?;
    let subset = font.try_subset(&['A', '日'])?;
}
# Ok(()) }
```

TTC header versions 1.0 and 2.0 are supported, including mixed TrueType/CFF
collections and tables shared by different faces. Extraction preserves table
bytes, sorts the selected sfnt directory, rebases offsets, and regenerates
table checksums and `head.checksumAdjustment`. Digital signatures from either
the collection header or a selected `DSIG` table are omitted because the
container rewrite invalidates them. Signature metadata is bounds-checked;
this API does not authenticate source signatures.

The source collection and extracted face are each limited to 32 MiB, with at
most 1,024 faces and 4,095 tables per face. Checked output-size accumulation
prevents shared source tables from amplifying into an oversized standalone
font. Duplicate table tags, invalid ranges, and missing or truncated `head`
tables are rejected before output allocation. `CollectionErrorKind` separates
invalid face indices from malformed input, unsupported containers, and limits;
errors carry bounded face/table/offset context without host paths.

Extraction is a container operation. It preserves optional layout and variable
font tables; `Font::parse`, instancing and renderer validation still determine
which font programs are supported. It does not add CFF2 decoding, discover
installed fonts, choose a face implicitly, or modify the supplied collection.
The original mixed-format fixtures and independent FontTools checks are
documented in [`fonts/test-collection`](fonts/test-collection/README.md).

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
