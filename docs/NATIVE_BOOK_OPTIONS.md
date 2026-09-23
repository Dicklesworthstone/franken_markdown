# Native book typography and PDF paper

`fmd book` accepts the same host font slots, variable-font weight pins, and
uniform font scale as the single-document renderer. These settings apply to
HTML, PDF, and EPUB publications:

```sh
fmd book ./manual --to both --out-dir ./publication \
  --pdf-font body-regular=./fonts/Body.ttf \
  --pdf-font mono-regular=./fonts/Code.ttf \
  --pdf-font-weight 500 --font-scale 125% --json

fmd book ./manual --to epub --out ./manual.epub \
  --pdf-font body-regular=./fonts/Body.ttf --font-scale lg
```

Font paths resolve from the command's working directory. Repeat `--pdf-font`
for `body-regular`, `body-bold`, `body-italic`, `body-bold-italic`, and
`mono-regular`. Missing slots use the normal bundled fonts. As in the shared
renderer, a variable `body-regular` face also supplies bold when no separate
bold face is selected. Each explicit file must be a supported TrueType font
with `glyf` outlines, stored in a regular file of at most 32 MiB.

`--pdf-font-weight WEIGHT` pins body regular; `--pdf-font-weight SLOT=WEIGHT`
pins another slot. Weights must be integers from 1 through 1000. A static
host font remains usable and reports `font_weight_ignored_static` when it
cannot honor a pin. Font scale accepts the shared presets, positive
multipliers, percentages, and CSS `px`/`pt` sizes, with the renderer's existing
scale bounds.

## Paper, margins, and contents

PDF and combined publications accept explicit page geometry and presentation:

```sh
fmd book ./manual --to pdf --out ./manual-a4.pdf \
  --page-size a4 --margin-top-pt 36 --margin-bottom-pt 36 \
  --margin-left-pt 48 --margin-right-pt 48 \
  --pdf-line-numbers --toc-depth 2 --json

fmd book ./manual --to pdf --out ./manual-landscape.pdf \
  --page-size 792x612 --margin-top-pt 36 --margin-bottom-pt 36
```

`--page-size` accepts `letter`, `a4`, `a5`, `legal`, `tabloid`, or custom
`WIDTHxHEIGHT` dimensions in points, retaining their specified orientation.
One point is 1/72 inch. Each dimension must be from 144 through 14,400 points.
Each margin must be finite and nonnegative, and the resulting content area
must be at least 72 points wide and high. An omitted margin retains its
configured value; `--no-config` selects the normal 72-point defaults.

`--pdf-line-numbers` numbers fenced code. `--toc-depth` accepts levels 1–6
and controls the automatically generated PDF contents; source headings and
PDF outline bookmarks remain present. Paper, margin, code-numbering, and
PDF contents-depth flags require `--to pdf` or `--to both`. HTML-only and
EPUB requests reject those flags with `unsupported_target_option` (exit 64).

## Diagnostics and input preservation

Successful JSON publication receipts retain the existing `warnings` array.
PDF render diagnostics are included as strings beginning with their stable
code and a colon, such as `missing_glyphs:`, `unsupported_image:`,
`unresolved_image:`, or `font_weight_ignored_static:`. They describe the
actual assembled document, including chapter-relative assets, chapter
landings, and prepared footnotes. Without `--json`, these warnings go to
stderr. Recoverable rendering warnings do not change the success exit code.

Every explicitly loaded font is protected from output overwrite, including
an earlier font replaced by a later assignment to the same slot. Font read,
size, or validation failures exit 66 before publication writes; invalid
option values exit 64. All publication outputs are still rendered and staged
before replacement. `--deny-broken-links` checks navigation before font-file
loading, and `--check-links` rejects these publication options.
