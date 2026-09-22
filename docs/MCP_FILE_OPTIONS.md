# Configured MCP file exports

`fmd.render_file` accepts flat rendering options alongside `path`, `to`, `out`,
`images`, and `fonts`. The HTML/PDF option builders are shared with
`fmd.render_html` and `fmd.render_pdf`, not reimplemented in the file loader.

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
| `pdf` | The `fmd.render_pdf` options, including author, page numbers, microtype, fit-to-pages and PDF/A |
| `both` | The union of HTML and PDF options; each renderer consumes its own fields |
| `epub` | HTML theme, font scale, title, language, CSS, TOC and TOC depth; no raw-HTML, interactive-HTML or HTML-font-container switches |
| `svg` | `font`, `fontScale`, `maxWidthPt` (144..14400 points), and explicit resources |

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

The default target remains HTML. Omitted titles retain the filename-stem default;
explicit empty titles and surrounding whitespace are preserved. Source parsing is
shared by the artifacts of one paired export. Interactive HTML uses the same
source, AST and options as the in-memory interactive tool.
