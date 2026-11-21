# MCP source publications

`fmd.render_svg` and `fmd.render_epub` publish one caller-supplied Markdown
string without a temporary source file. Alongside `fmd.render_html` and
`fmd.render_pdf`, all four single-document output formats are available from
explicit source and optional resource bytes.

## SVG poster

```json
{
  "name": "fmd.render_svg",
  "arguments": {
    "markdown": "# Overview\n\nA **measured** SVG poster.",
    "profile": "commonmark-gfm",
    "font": "serif",
    "fontScale": "125%",
    "maxWidthPt": 612,
    "toc": true,
    "tocDepth": 2
  }
}
```

The MCP result contains one text content block holding the complete SVG.
SVG is a single-page poster using native font measurements and outlines, not a
raster screenshot. Its accepted options are `markdown`, `profile`, `font`,
`fontScale`, `maxWidthPt`, `toc`, `tocDepth`, `images`, and `fonts`.
The width is 144–14400 finite points and defaults to 612.

Recoverable native diagnostics, such as `svg_image_missing`, remain visible in
an additive top-level `warnings` array of `{ "code", "message" }` records.
`isError` remains `false` when the native renderer returns a fallback artifact.
Requests without warnings retain the ordinary `content`/`isError` response.

## EPUB e-book

```json
{
  "name": "fmd.render_epub",
  "arguments": {
    "markdown": "# Field Guide\n\nThe complete document.",
    "title": "Field Guide",
    "author": "Research team",
    "lang": "en",
    "font": "serif",
    "toc": true,
    "fontEmbedding": "full"
  }
}
```

The text content block holds canonical base64 of the native EPUB archive.
Decode it to bytes and save it with an `.epub` extension. The tool itself does
not write a file. Fixed source, resources and options produce deterministic
archive bytes.

Accepted options are `markdown`, `profile`, `font`, `fontScale`, `darkMode`,
`title`, `author`, `lang`, `customCss`, `fontEmbedding`, `toc`, `tocDepth`,
`images`, and `fonts`. `author` is preserved in escaped EPUB creator metadata
and contributes to publication identity; it is also supported by
`fmd.render_file` with `to: "epub"`. Explicit metadata whitespace is retained.
Invalid XML metadata is rejected by the native EPUB renderer.

`customCss` replaces the normal EPUB stylesheet; an empty string is an explicit
empty replacement. `fontEmbedding` selects compact subsets by default or full
font programs. EPUB uses its own font container and safe XHTML policy, so
`htmlFontFormat`, `allowRawHtml`, and `interactiveHtml` are rejected, including
otherwise innocuous values such as `false`.

## Shared source and resource contract

Both tools select `profile: "commonmark-gfm"` or `profile: "gfm-plus"` before
parsing. Output transforms use the same selection. Omission preserves the
existing extended grammar; unknown names and wrong types do not fall back.

Images and fonts use the existing [explicit resource schema](MCP_RESOURCES.md).
Image destinations are lookup keys, never permission to read a path or URL.
Fonts retain their validated slots and optional variable-font weights.
An uninstantiable requested font fails publication rather than falling back
to the default outlines. No renderer dependency or image codec is introduced.

The server's Markdown byte limit and JSON frame limit still apply. Resource
admission retains the 32 MiB per decoded asset ceiling, 64 MiB combined
encoded/identifier and decoded budgets, 4096-image limit, and five font slots.
Options and resource metadata are checked before rendering.

Source tools reject `path`, `out`, `to`, `expandIncludes`, and caller-supplied
book/include collections. They never read include directives from the server's
filesystem. Use `fmd.render_book` for explicit multi-chapter and include-source
collections, or `fmd.render_file` with its documented include permission for
local files. Existing HTML/PDF tools and paired file-result shapes are unchanged.

`tools/list` and execution derive the SVG/EPUB field sets from the same
applicability rules as file publication. Unsupported format options therefore
fail instead of appearing to affect an output that does not consume them.

## Focused regression coverage

`tests/mcp_source_exports_test.rs` compares dispatcher output to checked native
SVG/EPUB renders, including explicit resources, grammar, theme/geometry, font
embedding, deterministic output, diagnostic records, admission limits and
failed variable-font requests. The MCP-gated
`epub_metadata::mcp_source_and_file_epub_preserve_creator_and_publication_identity`
case in `tests/epub/metadata.rs` uses the existing independent ZIP/XML test
helpers to inspect the byte-equal OPF metadata and checks unchanged paired
HTML/PDF author behavior.

```bash
cargo test --locked --features mcp --test mcp_source_exports_test
cargo test --locked --features mcp --test epub_test mcp_source_and_file_epub_preserve_creator_and_publication_identity
```
