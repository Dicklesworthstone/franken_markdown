# Bundled fonts

These [SIL OFL 1.1](https://openfontlicense.org/) font families are vendored so
franken_markdown can embed document-specific **subsets** into PDFs with zero runtime
font dependencies and full WASM portability (no system-font / fontconfig reliance).
The full TTFs live here; each rendered PDF embeds only the glyphs it actually uses
(via the subsetter in `src/text.rs`), so output stays tiny.

## Families & theme roles

| Role           | Family          | Files |
|----------------|-----------------|-------|
| Sans (default) | IBM Plex Sans   | `ibm-plex-sans/IBMPlexSans-{Regular,Bold,Italic,BoldItalic}.ttf` |
| Serif (LaTeX)  | Computer Modern | `computer-modern/cmun{rm,bx,ti,bi}.ttf` (Roman / Bold / Italic / BoldItalic) |
| Mono (code)    | CM Typewriter   | `computer-modern/cmuntt.ttf` |
| Math (fmd-math) | CM math italic / symbols / extension, roman script sizes | `cm-math/FmdCmMath{Italic,Symbols}{10,7,5}.ttf`, `cm-math/FmdCmMathExtension10.ttf`, `cm-math/FmdCmRoman{7,5}.ttf` |
| AMS math (fmd-math) | AMS-A and AMS-B symbols, including blackboard capitals | `cm-math/FmdAms{A,B}Symbols{10,7,5}.ttf` |
| Fraktur (fmd-math) | Euler Fraktur, `\mathfrak`'s face | `cm-math/FmdEulerFraktur{10,7,5}.ttf` |
| Symbol fallback | Noto Sans Math (curated subset) | `noto-sans-math/NotoSansMathSymbols.ttf` |
| Emoji fallback (native, `emoji-face`) | Noto Emoji, monochrome (curated subset) | `noto-emoji/NotoEmojiCurated.ttf` |
| CJK fallback (`cjk-face`, enabled by the CLI) | Noto Sans SC, Regular (BMP Han/kana subset) | `noto-sans-cjk/NotoSansSC-CJK.ttf` |
| Test-only VF | FmdTestVF (synthetic `wght` axis) | `test-variable/FmdTestVF.ttf` |

`cmunrm` is the classic Computer Modern Roman — the canonical LaTeX body face.

The symbol fallback face backs PDF text runs whose primary face has no glyph for
a character — arrows, math operators, geometric markers, and friends — so `⇒`
or `≠` render as real glyphs instead of `.notdef` boxes. It also carries the
mathematical-alphanumeric script (`\mathcal`) and double-struck (`\mathbb`)
letter repertoire — U+1D49C–U+1D4CF, U+1D538–U+1D56B, plus the double-struck
digits U+1D7D8–U+1D7E1 — that `fmd-math`'s `faces::calligraphic_char` /
and ℬℰℱℋℐℒℳℛℯℊℴ live in the U+2100 block). The result is a curated
72,908-byte (~71 KiB) subset of Noto Sans Math, produced deterministically
by the project's own subsetter:

```bash
cargo run --example gen_symbol_fallback_font -- \
    /path/to/NotoSansMath-Regular.ttf fonts/noto-sans-math/NotoSansMathSymbols.ttf
```

The curated codepoint ranges live in `examples/gen_symbol_fallback_font.rs` and
are the single source of truth for the fallback repertoire.

## Sources

- **IBM Plex Sans** — <https://github.com/IBM/plex> (release `@ibm/plex-sans@1.1.0`),
  SIL OFL 1.1, © IBM Corp. License: `ibm-plex-sans/OFL.txt`.
- **Computer Modern Unicode** — the CMU project, packaged as web fonts
  (<https://checkmyworking.com/cm-web-fonts/>), SIL OFL 1.1. License:
  `computer-modern/OFL.txt`.
- **CM math** — the American Mathematical Society's Type 1 release of Knuth's
  Computer Modern math fonts (cmmi10/7/5, cmsy10/7/5, cmex10, cmr7/5; © 1997, 2009
  AMS), SIL OFL 1.1 with Reserved Font Names. License: `cm-math/OFL.txt`. These
  are the faces TeX sets mathematics in, converted offline to TrueType `glyf`
  outlines by `scripts/font-oracles/convert-cm-math.py` (fonttools 4.65, cu2qu at
  most 1/1000 em) and renamed "FMD CM Math …" to honour the Reserved Font
  Names. `cm-math/PROVENANCE.json` records each input and output SHA-256. Every
  glyph is addressable at U+F000 + its TFM code position; the characters TeX
  takes from a face are also mapped at their Unicode codepoints (fmd-math's
  `faces` module documents the roles). Decided in franken_manim ADR-0028.
- **AMS symbols** — AMSFonts' `msam10/7/5` and `msbm10/7/5`, under the same
  AMS copyright and `cm-math/OFL.txt`. The existing offline converter produces
  renamed "FMD AMS …" derivatives with the same 1/1000 em bound. Source and
  output hashes are in `cm-math/PROVENANCE.json`. Private-use mappings preserve
  every TeX code; reviewed Unicode aliases cover checkmark, circle arrows,
  hbar, not-divides and blackboard capitals. The short minus stays private-use
  only, so ordinary subtraction keeps its CM glyph. `fmd-math` uses the
  corresponding TFM metrics and 7/5 pt optical cuts; Noto remains the fallback
  for alphabets and symbols these faces do not contain.
- **Euler Fraktur** — AMSFonts' `eufm10/7/5`, the faces amssymb's `\mathfrak`
  sets (through eufrak's `ueuf.fd`), under the same AMS copyright and
  `cm-math/OFL.txt`, which names them as Reserved Font Names. The same converter
  produces renamed "FMD Euler Fraktur …" derivatives. Letters map at the
  Mathematical Fraktur codepoints (the five Letterlike capitals included) and
  digits at ASCII; punctuation and the alternate letter forms are code-addressed
  only.
- **Noto Sans Math** — <https://github.com/google/fonts/tree/main/ofl/notosansmath>
  (© The Noto Project Authors), SIL OFL 1.1. License: `noto-sans-math/OFL.txt`.
  Committed as the curated `NotoSansMathSymbols.ttf` subset described above, not
  the full face.
- **Noto Emoji** (monochrome) — <https://github.com/google/fonts/tree/main/ofl/notoemoji>
  (© Google LLC / The Noto Project Authors), SIL OFL 1.1. License:
  `noto-emoji/OFL.txt`. Committed as the curated `NotoEmojiCurated.ttf` subset
  (Regular instance) produced by `examples/gen_emoji_fallback_font.rs`.
- **Noto Sans SC** — <https://github.com/google/fonts/tree/main/ofl/notosanssc>
  (© 2014–2021 Adobe, Reserved Font Name “Source”), SIL OFL 1.1. The optional
  `cjk-face` feature supplies real PDF/SVG fallback glyphs for the source's BMP
  Han, kana, CJK punctuation, and fullwidth forms. It uses SC glyph forms and
  does not include Korean Hangul or supplementary-plane Han. The pinned source,
  exact coverage, deterministic generation command, and hashes are in
  [`noto-sans-cjk/README.md`](noto-sans-cjk/README.md); the license ships there too.
- **FmdTestVF** — project-authored synthetic variable font for `fvar`/`avar`
  parser tests (`gk3v.1`). SIL OFL 1.1, no Reserved Font Name. License:
  `test-variable/OFL.txt`. See `test-variable/README.md`.

## Notes

- All files are TrueType (`glyf` outlines, sfnt `0x00010000`), so the clean-room
  reader + subsetter in `src/text.rs` handle them directly (no CFF/woff2 decoding).
- `IBM Plex Mono` is not yet bundled; CM Typewriter currently serves the mono role,
  and a Plex Mono cut can be added later.
- A TTF is only compiled into the binary where an `include_bytes!` references it; the
  subsetter then strips each embed down to the document's glyph set, keeping PDFs small
  while the engine stays self-contained.
