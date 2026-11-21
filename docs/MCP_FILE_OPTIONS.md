# Configured MCP file exports

`fmd.render_file` accepts flat rendering options alongside `path`, `to`, `out`,
`images`, `fonts`, and `expandIncludes`. The HTML/PDF option builders are shared
with `fmd.render_html` and `fmd.render_pdf`, not reimplemented in the file loader.

All targets accept `profile: "commonmark-gfm"` or `profile: "gfm-plus"`.
The source is parsed under that dialect before any output transforms, including
SVG. Paired HTML/PDF output shares the same profiled AST.
Omitting the option retains the existing extended grammar. Unknown names and
non-string values fail before the source is read or any output is replaced.

Explicit profiles cannot be combined with `interactiveHtml: true`: the
lightweight saved-HTML editor has its own local parser and cannot retain the
requested grammar after editing. This returns `profile_interactive_unsupported`
before source I/O. Omit `interactiveHtml` for a static document, or use the native
Flow/WASM workspace for profiled editing. Unprofiled interactive output keeps its
existing behavior.

```json
{
  "name": "fmd.render_file",
  "arguments": {
    "path": "report.md",
    "to": "both",
    "out": "publication.pdf",
    "font": "serif",
    "fontScale": "125%",
    "title": "Quarterly Report",
    "author": "Research team",
    "lang": "en",
    "toc": true,
    "tocDepth": 2,
    "pageNumbers": true,
    "pdfA": "2b",
    "pdfAStrict": true
  }
}
```

This produces `publication.html` and `publication.pdf`. The PDF goes through the
archival renderer when `pdfA` is enabled. Both renders complete before the staged
writer replaces either destination. Output paths and source-alias protection
retain the existing contract. With no `out`, paired output retains the existing
JSON `outputs` array; single-format output retains its existing text/base64 form.

## Target-specific options

| Target | Accepted rendering options |
|---|---|
| `html` | The `fmd.render_html` options, including CSS, appearance, TOC, font container and interactive HTML |
| `pdf` | The `fmd.render_pdf` options, including author, page numbers, microtype, fit-to-pages, baseline grid, strict pagination and PDF/A |
| `both` | The union of HTML and PDF options; each renderer consumes its own fields |
| `epub` | HTML theme, font scale, title, language, CSS, TOC and TOC depth; no raw-HTML, interactive-HTML or HTML-font-container switches |
| `svg` | `profile`, `font`, `fontScale`, `maxWidthPt` (144..14400 points), `toc`, `tocDepth`, and explicit resources |

`images` and `fonts` work for all targets. They contain explicit bytes; destinations
never authorize ambient file reads or network requests. SVG resolves `fontScale`
through the shared theme before wrapping text and laying out code, tables and
mathematics. Paper width and intrinsic image dimensions do not scale with text.
SVG warnings retain the existing per-occurrence response channel, including
`svg_layout_adjusted` when the core normalizes an out-of-range theme value.

`tools/list` derives the file-option union and each field's target description
from the same definitions used by execution. Unknown fields, wrong types, invalid
ranges, impossible PDF/A settings, and options with no selected-target consumer
fail before source I/O. `unsupported_target_option` identifies the latter case,
including a supplied `false` for a switch that the target does not implement.

See [PDF layout and verification](MCP_PDF_CONFIGURATION.md) for
`baselineGridPt`, `strictPagination`, paper, typography, and running bands.
An enabled baseline grid and `fitToPages` are incompatible and fail before source
I/O. Strict-pagination failures preserve their typed `pagination_*` reason and
leave both paired destinations unchanged.

The default target remains HTML. Omitted titles retain the filename-stem default;
explicit empty titles and surrounding whitespace are preserved. Source parsing is
shared by the artifacts of one paired export. Interactive HTML uses the same
source, AST and options as the in-memory interactive tool.

## Local Markdown includes

Set `expandIncludes: true` to expand `{{#include path}}` in the selected file.
The default is `false`: omission retains the original single-file behavior and
does not read include destinations. The option works for HTML, PDF, paired
HTML/PDF, EPUB, SVG, and unprofiled interactive HTML.

```json
{
  "name": "fmd.render_file",
  "arguments": {
    "path": "guide/report.md",
    "expandIncludes": true,
    "to": "both",
    "out": "publication/report.pdf"
  }
}
```

Expansion uses the same resolver as native rendering: recursive includes, named
anchors and line selectors work; fenced/indented code remains literal; the
selected `profile` applies during discovery and parsing. Relative links and
image destinations retain the included fragment's directory. Supply explicit
image bytes using the resulting root-relative destination, such as
`parts/plot.svg` for `![Plot](plot.svg)` in `parts/body.md`.
Image and font destinations still do not authorize local reads or network
requests.

Include reads stay inside the canonical directory of the requested input path,
including when the input itself is a symlink. Every fragment is a regular UTF-8
file subject to the server's Markdown byte limit; expanded output is capped by
that same limit and the shared 64 MiB ceiling. Missing files, cycles, escaped
paths, invalid UTF-8 and exceeded budgets return `include_error` with the
resolver's diagnostic before publication.

The original source and every consulted fragment are protected from output
replacement, including symlink and hard-link aliases. With `to: "both"`, an
include error or protected destination leaves both previous artifacts intact.
Interactive HTML contains the expanded editable source so subsequent edits
retain the published document.
