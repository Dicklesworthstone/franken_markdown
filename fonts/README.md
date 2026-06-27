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

`cmunrm` is the classic Computer Modern Roman — the canonical LaTeX body face.

## Sources

- **IBM Plex Sans** — <https://github.com/IBM/plex> (release `@ibm/plex-sans@1.1.0`),
  SIL OFL 1.1, © IBM Corp. License: `ibm-plex-sans/OFL.txt`.
- **Computer Modern Unicode** — the CMU project, packaged as web fonts
  (<https://checkmyworking.com/cm-web-fonts/>), SIL OFL 1.1. License:
  `computer-modern/OFL.txt`.

## Notes

- All files are TrueType (`glyf` outlines, sfnt `0x00010000`), so the clean-room
  reader + subsetter in `src/text.rs` handle them directly (no CFF/woff2 decoding).
- `IBM Plex Mono` is not yet bundled; CM Typewriter currently serves the mono role,
  and a Plex Mono cut can be added later.
- A TTF is only compiled into the binary where an `include_bytes!` references it; the
  subsetter then strips each embed down to the document's glyph set, keeping PDFs small
  while the engine stays self-contained.
