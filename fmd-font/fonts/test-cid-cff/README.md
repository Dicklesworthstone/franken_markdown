# CID-keyed CFF1 fixtures

These original geometric test fonts are distributed under the package
[LICENSE](../../LICENSE). They contain no third-party glyph artwork and are
not design fonts or bundled renderer faces.

| Fixture | Bytes | FDSelect |
| --- | ---: | --- |
| `FmdCidFormat0.otf` | 816 | Direct per-glyph selectors |
| `FmdCidFormat3.otf` | 824 | Ordered selector ranges |

Both fonts contain four glyphs and two font dictionaries:

- Glyph 0 is an empty `.notdef` in FD 0.
- `A` is a 200-by-300 rectangle in FD 0.
- `B` is a 400-by-500 rectangle in FD 1, with an explicit nominal-width delta.
- U+65E5 is a test cubic arch in FD 0, not a drawing of the Japanese character.

The two rectangles call the same global subroutine. That subroutine calls
local subroutine zero, whose drawing differs between the dictionaries. A
decoder that loses the active FD when entering a global subroutine produces
the wrong rectangle. The arch uses local subroutine one and has a tight
150-unit ink height although its control points reach 200 units.

The source CIDs are 0, 25, 1000 and 7. This nonidentity, unsorted mapping checks
that sfnt character mapping, FD selection and sparse subsetting use glyph
indices independently of CID labels. Both selector formats must normalize to
identical subset bytes.

`generate.py` writes the font bytes directly from the CFF and sfnt formats,
using Python's standard library. It then uses FontTools 4.61.1 independently
to decode each emitted font and records advance, left side bearing, a 64-bit
FNV-1a outline fingerprint, and outward-rounded cubic ink bounds in
`fonttools-reference.txt`. The fingerprint format matches the existing
Bravura reference: move/line/curve/close opcodes plus little-endian f64 point
coordinates, with negative zero normalized. The generator requires FontTools;
building and running the Rust tests does not.

Regenerate from the repository root:

```sh
python fmd-font/fonts/test-cid-cff/generate.py
```

`tests/cid_cff_test.rs` compares both fonts with the independent reference,
checks nested subroutine identity and Unicode subsets, and exercises parallel
consumers of one parsed font. Directory unit tests additionally reject missing
coverage, invalid dictionary indices, duplicate/out-of-range CIDs, broken
INDEX offsets, malformed private subroutines and unsupported per-FD matrices.

Format reference: Adobe Technical Note 5176, particularly sections 5, 13, 18
and 19, [The Compact Font Format Specification](https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf).
