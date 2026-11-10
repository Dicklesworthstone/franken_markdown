# MCP PDF layout and verification

`fmd.render_pdf`, `fmd.render_file` with `to: "pdf"` or `"both"`,
`fmd.render_book` with `to: "pdf"` or `"all"`, and `fmd.verify` accept a
`page` object and explicit typographic sizes. They populate
the native `PdfOptions` and theme before layout; output is not rescaled or patched.

```json
{
  "name": "fmd.render_pdf",
  "arguments": {
    "markdown": "# A landscape report\n\nContent.",
    "page": {
      "size": "a4",
      "orientation": "landscape",
      "margins": {"topPt": 36, "rightPt": 30, "bottomPt": 36, "leftPt": 30}
    },
    "baseFontSize": 12,
    "headingScale": 1.25,
    "tableFontSize": 9,
    "pageNumbers": true
  }
}
```

`page.size` is `"letter"`, `"a4"`, `"a5"`, `"legal"`, `"tabloid"`, or an object
containing both `widthPt` and `heightPt`. These presets match the native CLI
and browser APIs. Custom dimensions preserve their order unless an explicit
`"portrait"`/`"landscape"` orientation rotates the paper. Margin sides never
rotate. `page.margins` is a uniform number or an object with optional `topPt`,
`rightPt`, `bottomPt`, and `leftPt`. Missing sides default to 72 points. Omitted
size defaults to Letter. Omitting `page` entirely retains the historical theme.

All dimensions are points. Paper must be 144..14400 points per dimension;
margins must be finite, nonnegative, and leave at least 72 points of content in
both directions. Admission checks both JSON-number precision and the renderer's
actual f32 subtraction. Nested unknown fields, wrong types and null values are
errors, not requests for defaults. The shape matches the browser page contract.

The numeric typography fields are `baseFontSize` (6..24 points), `headingScale`
(1.05..2), and `tableFontSize` (5..24 points). The native renderer additionally
caps nominal table size to body size. Existing `fontScale`, `microtype`, font
family, navigation, metadata and PDF/A settings remain available for rendering.
Invalid numeric input is refused rather than silently narrowed or clamped by
this adapter. In paired exports, PDF paper/size fields do not change HTML.

## Baseline grids and strict pagination

All PDF routes above, including `fmd.verify`, accept these two controls:

| Option | Meaning |
|---|---|
| `baselineGridPt` | Body baseline spacing in points: `0` disables the grid; otherwise use a finite number from `1` through `144`, rounded to 0.001 point. Omission preserves the default layout. |
| `strictPagination` | Require an exact final page plan. Implies `optimalPagination: true`, even when `optimalPagination: false` was supplied. Omission or `false` preserves best-effort rendering. |

For example, add the following to a PDF, paired-file, book, or verification
request:

```json
{
  "baselineGridPt": 13.125,
  "strictPagination": true
}
```

An enabled grid cannot be combined with `fitToPages`. Remove the fitting
target or set `baselineGridPt: 0`. MCP validates the numeric range and this
combination before file reads, book-source processing, or resource decoding.
The grid controls body baselines; running headers, footers, and figure internals
have their own geometry.

Strict pagination refuses a fallback when the bounded exact planner cannot
satisfy its constraints over the final measured lines and admitted paragraph
variants. It does not promise an unconstrained global optimum over every possible
line shape. It is independent of `pdfAStrict`, which controls PDF/A admission.

A strict refusal is a JSON-RPC render error (`-32003`) whose
`error.data.reason` preserves the core's actionable code:
`pagination_infeasible`, `pagination_budget_exceeded`,
`pagination_invalid_input`, `pagination_cost_overflow`, or
`pagination_invalid_plan`. The message retains the underlying planner detail.
These codes propagate through direct PDF, file, book, archival, and verification
requests. A failed paired-file render leaves both existing output files intact.

HTML, EPUB, SVG, and site-only requests reject these PDF-only fields, including
an explicit `false` or `0`. In `both` and `all` exports, HTML, EPUB, and
site artifacts keep their own layout settings.

## Running header and footer

`fmd.render_pdf` and `fmd.render_file` (`pdf`/`both`) accept a `running` object
that draws text in the page margins:

```json
{
  "running": {
    "header": {"left": "{title}", "right": "{date}", "rule": true},
    "footer": {"left": "Confidential", "center": "{page} / {pages}", "rule": true},
    "skipFirstPage": true
  },
  "metadataEpochSeconds": 1700000000
}
```

Each band has optional `left`, `center` and `right` string templates and a
`rule` boolean. Tokens are `{page}`, `{pages}`, `{title}` (the `title` argument,
else the first heading), `{chapter}`, `{section}`, `{author}` and `{date}`
(`metadataEpochSeconds` as `YYYY-MM-DD` UTC, empty when absent; never the clock).
Unknown tokens stay literal. Chapter/section use the final PDF's first body
heading on each page and its owning H1, carrying the latest pair onto continuation
pages. Generated contents/Notes headings and footnote headings do not change this
context. Text is one line in the body face at 9/11 of the body size; an overlong
slot is shortened with an ellipsis. A band that does not fit its margin fails
the call with a message naming the margin to enlarge (`page.margins.topPt` or
`bottomPt`). `pageNumbers: true` is sugar for `footer.center: "{page}"` and
yields to an explicit `footer.center`. Unknown fields, wrong types and templates
over 4096 bytes are refused. Chrome is tagged as pagination artifacts and never
changes layout or pagination. Images in bands are not supported yet.

## Audit the configuration being rendered

`fmd.verify` now accepts the PDF layout options and the same explicit `images`
and `fonts` byte payloads as rendering. A verifier request can therefore use the
same `markdown`, `page`, sizes, fonts, image bytes, baseline grid, and strict
pagination policy. It uses the production page planner and running-band admission;
a strict failure returns an error instead of a report from fallback pagination.
Its JSON report and `a11y` filtering retain the existing native verification contract.

Verification is a source-layout audit, not an inspection of serialized PDF
bytes and not a PDF/A conformance validator. `pdfA` and `pdfAStrict` are deliberately
absent from its schema and rejected. Invalid options or resources fail the tool;
they cannot become a misleading successful report. Rendering without new options
and verification without new options retain their previous defaults.
