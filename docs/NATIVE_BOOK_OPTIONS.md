# Native book typography and publication options

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
The same flags are available for single-document `render`, `watch`, and `batch`.
Use `fmd config set page_size a4` (or custom dimensions) to persist a default
paper size for every native PDF route. An explicit `--page-size` overrides it.

`--pdf-line-numbers` numbers fenced code. Paper, margin, and code-numbering
flags require `--to pdf` or `--to both`. HTML-only and EPUB requests reject
those flags with `unsupported_target_option` (exit 64).

`--toc-depth` accepts levels 1–6 and limits generated contents for every
target: PDF book contents or the contents inside each HTML/EPUB chapter.
Use `--toc` to enable them. Source headings, PDF outline bookmarks, and
EPUB's publication navigation remain complete.

## Typography, pagination, and persistent defaults

The native book command carries the shared renderer's advanced typography
through the complete assembled book, including chapter destinations and notes:

```sh
fmd book ./manual --to pdf --out ./manual.pdf \
  --microtype protrusion --typography-homogeneous \
  --typography-antiriver --typography-pareto --pdf-optimal-pagination

fmd book ./manual --to pdf --out ./compact.pdf \
  --toc=false --fit-to-pages 12 --pdf-base-font-size 12 \
  --pdf-heading-scale 1.2 --pdf-table-font-size 10
```

`--microtype` accepts `off`, `protrusion`, and `expansion`, with the same
policies as ordinary rendering. The three typography switches and
`--pdf-optimal-pagination` select the existing shared algorithms.
`--fit-to-pages` (alias `--target-pages`) accepts a positive page budget for
the entire publication, including generated contents and chapter breaks.
The command checks the actual emitted page count after the shared solver
finishes. An infeasible budget fails with `page_budget_unmet` before any
HTML or PDF is replaced; the error states the achieved count.

Explicit PDF sizes must be finite: body size is 6–24 points, heading ratio
is 1.05–2, and nominal table size is 5–24 points. The renderer caps table
size by the effective body size. `--pdf-base-font-size` overrides the PDF
body size derived from `--font-scale`; HTML keeps the uniform font scale.

Persistent `font_scale`, `lang`, `html_font_format`, `toc`, `toc_depth`,
`microtype`, `pdf_page_numbers`, `pdf_optimal_pagination`, and the three
typography defaults now apply to books. Explicit flags win over config.
For contents and depth, source frontmatter ranks between flags and config.
For book language, the existing order remains flag, manifest, first
chapter frontmatter, then config. Individual HTML/EPUB chapter language
frontmatter continues to override the book language.

PDF books keep generated contents and running page numbers enabled when no
flag, relevant source metadata, or config key changes them. HTML/EPUB
contents remain off by default. `--toc` enables contents; `--toc=false`
disables them. The typography switches, `--pdf-optimal-pagination`, and
`--pdf-page-numbers` likewise accept `=false`, so a single command can
override a configured true value. `--no-config` retains the normal
configuration-free defaults.

`--html-font-format woff1|woff2|ttf` is available for HTML and combined
publications. Configured font formats are harmless for PDF/EPUB output;
an explicit format flag on those targets is rejected because EPUB uses
its own TrueType subset container. Explicit PDF-only typography,
pagination, running-content, and archival flags require PDF or combined
output and fail before reading chapter or asset files.

## Running titles, page counters, and logos

```sh
fmd book ./manual --to both --out-dir ./publication \
  --title 'Reference manual' --author 'Documentation team' \
  --pdf-header-left '{title}' --pdf-header-right '{author}' \
  --pdf-footer-center '{page} / {pages}' --pdf-footer-rule \
  --pdf-image brand=./logo.svg --pdf-header-image brand \
  --pdf-header-image-position right --pdf-header-image-height-pt 18 \
  --pdf-running-skip-first
```

All six text slots use the shared `{page}`, `{pages}`, `{title}`, `{author}`,
and `{date}` tokens. The date comes only from `SOURCE_DATE_EPOCH`; it does
not read the clock. `--pdf-header-rule` and `--pdf-footer-rule` draw rules,
and `--pdf-running-skip-first` omits both bands and page numbers on the
first physical page. An explicit footer center overrides automatic page
numbers. These decorations remain outside the document's tagged reading
order and use the existing page margins.

Header and footer images use `--pdf-header-image`/`--pdf-footer-image`,
optional `--pdf-*-image-position left|right`, and positive integer
`--pdf-*-image-height-pt`. Their bytes can come from repeated
`--pdf-image DEST=PATH` mappings, including logos absent from chapter
Markdown. Paths resolve from the working directory. A body-image override
uses the resolved book-root destination, such as
`--pdf-image /guide/chart.svg=./replacement.svg`; URL query/fragment
suffixes remain part of the exact key. External URL keys are also exact,
and no image is fetched from the network.

Later mappings replace earlier values for the same destination, while
every read file remains protected against output overwrite. Each image
must be a nonempty regular file within `--max-pdf-image-bytes` and the
32 MiB ceiling. Publication image payloads and explicit image reads are
bounded to 128 MiB, with at most 4,096 retained resources or mappings.
Unusable or missing running logos and insufficient margin bands are
render errors rather than silently missing branding.

## Archival PDF emission

```sh
SOURCE_DATE_EPOCH=1700000000 fmd book ./manual --to pdf \
  --out ./manual-archive.pdf --pdf-a 2b --pdf-a-strict --json
```

`--pdf-a 2b` uses the existing PDF/A-2b writer with its XMP identification
and sRGB output intent. `--pdf-a off` preserves ordinary bytes. Optional
`--pdf-a-strict` requires the `2b` profile and rejects unsupported URI
actions even inside unreferenced note definitions. The chapter-aware
navigation pipeline still binds links to actual emitted destinations.
This selects the writer's archival mode; it does not claim independent
PDF/A certification.

Successful JSON receipts identify the selected archival profile and
strictness in a `pdf_a` object, and `pages` always counts actual emitted
pages. PDF render warnings continue through the ordinary warnings array.
If archival rendering fails in a combined publication, no HTML or PDF
destination is replaced.

## Diagnostics and input preservation

Successful JSON publication receipts retain the existing `warnings` array.
PDF render diagnostics are included as strings beginning with their stable
code and a colon, such as `missing_glyphs:`, `unsupported_image:`,
`unresolved_image:`, or `font_weight_ignored_static:`. They describe the
actual assembled document, including chapter-relative assets, chapter
landings, and prepared footnotes. Without `--json`, these warnings go to
stderr. Recoverable rendering warnings do not change the success exit code.

Every explicitly loaded font, mapped image and loaded config is protected
from output overwrite, including an earlier asset replaced by a later
assignment to the same slot or destination. Font read,
size, or validation failures exit 66 before publication writes; invalid
option values exit 64. All publication outputs are still rendered and staged
before replacement. `--deny-broken-links` checks navigation before font-file
loading, and `--check-links` rejects these publication options.

Native HTML links resolve against exact chapter source paths, as in the
core site exporter. An unknown source is never redirected merely because
its flattened output name resembles an existing chapter.
