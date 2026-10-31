# Noto Sans SC CJK fallback

`NotoSansSC-CJK.ttf` is a static Regular (`wght=400`) subset of Noto Sans SC,
licensed under the adjacent [SIL OFL 1.1](OFL.txt). It contains **28,215 mapped
characters** and **28,101 glyphs**, including `.notdef` and shared mappings.

The optional `cjk-face` feature exposes it as
`fmd_font::bundled::noto_sans_cjk_sc()`. The native renderer's `cli` feature enables
it. Lean library and WASM builds can opt in explicitly. The standard browser
package does not include this broad font. PDF/HTML/EPUB embeds a further subset
for each document; ordinary Latin publications do not embed this face. Full
HTML/EPUB embedding and styled editable HTML retain complete fallback bytes
when their policy requires them.

## Repertoire

The generator retains every character mapped by the pinned upstream source
within the ranges below. This is a Unicode-range subset, **not** a frequency
ranking or a claim that every scalar in those blocks is assigned or covered.
There are 441 selected scalars that this upstream version does not map.

| Range | Repertoire |
|---|---|
| U+3000–U+303F | CJK punctuation, iteration marks, ideographic space |
| U+3040–U+30FF | Hiragana and katakana |
| U+31F0–U+31FF | Katakana phonetic extensions |
| U+3400–U+4DBF | CJK Unified Ideographs Extension A |
| U+4E00–U+9FFF | CJK Unified Ideographs |
| U+F900–U+FAFF | Source-mapped CJK compatibility ideographs |
| U+FE10–U+FE1F | Vertical punctuation presentation characters |
| U+FE30–U+FE4F | CJK compatibility punctuation |
| U+FF00–U+FFEF | Source-mapped fullwidth and halfwidth forms |

The face uses **Simplified-Chinese glyph forms**. It supplies horizontal Han
and kana outlines, not locale-specific Japanese GSUB substitutions, vertical
layout, Korean Hangul, supplementary-plane Han, or general complex-script
shaping. Precomposed kana are supported; decomposed combining-kana placement
requires shaping that the PDF/SVG path does not currently implement. Explicit
presentation characters in the font do not imply a vertical writing mode.
Uncovered characters remain visible in PDF/SVG missing-glyph diagnostics.

The primary body/code face remains first in the fallback chain. Emoji uses a
separate face and resource slot. This CJK subset has no ASCII mappings, so it
cannot replace Latin typography in a CJK-only EPUB fallback stylesheet.

## Pinned source

- Repository: <https://github.com/google/fonts>
- Revision: `2894aab31764f10f29c421bdfd2340d3b382d384`
- Path: `ofl/notosanssc/NotoSansSC[wght].ttf`
- Version: `2.004-H2`
- Source SHA-256: `a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da`
- Copyright: 2014–2021 Adobe; Reserved Font Name: “Source”.
- Source, license, and generated-file digests are recorded in
  [PROVENANCE.json](PROVENANCE.json).

## Reproduce and independently verify

Download the source at the exact revision above (the URL-escaped filename is
`NotoSansSC%5Bwght%5D.ttf`) and verify its SHA-256. No network or system-font lookup
occurs in the renderer or generator. From the repository root:

```bash
cargo run --no-default-features --example gen_cjk_fallback_font -- \
    /path/to/NotoSansSC-wght.ttf fmd-font/fonts/noto-sans-cjk/NotoSansSC-CJK.ttf
python3 scripts/font-oracles/check-cjk-fallback.py /path/to/NotoSansSC-wght.ttf --full
```

The Rust generator uses the project's own weight instancer and subsetter. It
checks every retained character after parsing the resulting sfnt. The separate
Python oracle uses FontTools 4.61.1 as a development reference: it checks the
exact coverage against the original source, then compares every retained
character's path operations, coordinates, and horizontal advance at weight 400.
A one-font-unit bound permits integer rounding of interpolated source points;
it does not admit missing paths or scale/position distortions. Without `--full`,
the oracle checks complete cmap coverage and 166 representative glyphs.
FontTools is not a build or runtime dependency.

The checked-in font uses standard short-coordinate and repeated-flag encodings
and is 9,867,212 bytes. The recorded verification chain is explicit:
[fonttools-reference.json](fonttools-reference.json) checks all 28,215 mappings
of the expanded encoding against the pinned source; then
[compact-equivalence.json](compact-equivalence.json) independently checks all
28,101 glyphs and 3,065,603 points of the compact encoding with zero coordinate
differences. Character maps, metrics, contour boundaries, point flags, and
instructions are unchanged. The full source-oracle command above can also
verify the compact artifact directly. The low-memory equivalence checker is
`scripts/font-oracles/check-glyph-equivalence.py`; its `--help` describes the
baseline, candidate, and baseline-report inputs.
