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
| Symbol fallback | Noto Sans Math (curated subset) | `noto-sans-math/NotoSansMathSymbols.ttf` |

`cmunrm` is the classic Computer Modern Roman — the canonical LaTeX body face.

The symbol fallback face backs PDF text runs whose primary face has no glyph for
a character — arrows, math operators, geometric markers, and friends — so `⇒`
or `≠` render as real glyphs instead of `.notdef` boxes. It is a curated
~56 KiB subset of Noto Sans Math produced deterministically by the project's
own subsetter:

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
- **Noto Sans Math** — <https://github.com/google/fonts/tree/main/ofl/notosansmath>
  (© The Noto Project Authors), SIL OFL 1.1. License: `noto-sans-math/OFL.txt`.
  Committed as the curated `NotoSansMathSymbols.ttf` subset described above, not
  the full face.

## Notes

- All files are TrueType (`glyf` outlines, sfnt `0x00010000`), so the clean-room
  reader + subsetter in `src/text.rs` handle them directly (no CFF/woff2 decoding).
- `IBM Plex Mono` is not yet bundled; CM Typewriter currently serves the mono role,
  and a Plex Mono cut can be added later.
- A TTF is only compiled into the binary where an `include_bytes!` references it; the
  subsetter then strips each embed down to the document's glyph set, keeping PDFs small
  while the engine stays self-contained.
