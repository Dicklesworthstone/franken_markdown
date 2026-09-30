# MCP PDF layout and verification

`fmd.render_pdf`, `fmd.render_file` with `to: "pdf"` or `"both"`, and
`fmd.verify` accept a `page` object and explicit typographic sizes. They populate
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

`page.size` is `"letter"`, `"a4"`, or an object containing both `widthPt` and
`heightPt`. Custom dimensions preserve their order unless an explicit
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
else the first heading), `{author}` and `{date}` (`metadataEpochSeconds` as
`YYYY-MM-DD` UTC, empty when absent; never the clock). Unknown tokens stay
literal. Text is one line in the body face at 9/11 of the body size; an overlong
slot is shortened with an ellipsis. A band that does not fit its margin fails
the call with a message naming the margin to enlarge (`page.margins.topPt` or
`bottomPt`). `pageNumbers: true` is sugar for `footer.center: "{page}"` and
yields to an explicit `footer.center`. Unknown fields, wrong types and templates
over 4096 bytes are refused. Chrome is tagged as pagination artifacts and never
changes layout or pagination. Images in bands are not supported yet.

## Audit the configuration being rendered

`fmd.verify` now accepts the PDF layout options and the same explicit `images`
and `fonts` byte payloads as rendering. A verifier request can therefore use the
same `markdown`, `page`, sizes, fonts and image bytes instead of auditing a
second, default-font/default-paper document with missing resources. Its JSON
report and `a11y` filtering retain the existing native verification contract.

Verification is a source-layout audit, not an inspection of serialized PDF
bytes and not a PDF/A conformance validator. `pdfA` and `pdfAStrict` are deliberately
absent from its schema and rejected. Invalid options or resources fail the tool;
they cannot become a misleading successful report. Rendering without new options
and verification without new options retain their previous defaults.
