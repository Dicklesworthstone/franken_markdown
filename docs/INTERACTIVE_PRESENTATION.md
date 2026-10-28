# Configured editable HTML

The browser facade's `renderInteractiveHtml(markdown, options)` accepts
`FmdInteractiveRenderOptions`. It preserves HTML document settings in the
generated, editable file:

- Body family, type size, dark-mode policy, title and language.
- Custom CSS, table of contents and heading depth.
- Explicit image resources and all five font resource slots, including weight
  pins for supported variable fonts.
- TTF, WOFF1 or WOFF2 font embedding.

The document appears in a sandboxed frame. Its CSS and heading IDs cannot style
or impersonate editor controls. Custom CSS replaces the default document
stylesheet; complete embedded faces remain available as **FMD Body** and
**FMD Mono**. It does not style the surrounding application.

```js
const document = await renderer.renderInteractiveHtml(markdown, {
  title: "Field notes",
  lang: "en",
  font: "serif",
  typeSize: "lg",
  toc: true,
  tocDepth: 3,
  htmlFontFormat: "woff2",
  pdfImages: [{destination: "chart.png", bytes: chartBytes}],
  fontAssets: [{slot: "body-regular", bytes: bodyFontBytes}],
});
```

The historically named `pdfImages` field supplies images to HTML too.
Resources are explicit byte inputs, never permission to read a local path or
fetch a URL. The frame blocks external image, font and stylesheet requests.
Provided resources remain in the saved file even after the last reference is
temporarily removed.

**Save HTML** retains the presentation, resources, current source, zoom and
theme. New headings update the table of contents. Fonts retain their complete
admitted glyph maps and regular, bold, italic and code faces, so later edits
do not depend on the characters present when the file was created. This makes
configured workspaces larger than ordinary HTML with font subsets. The PDF
button and Ctrl/Cmd+P wait for the current configured preview and its fonts
before invoking the browser's print operation.

This is the lightweight editor: its first preview comes from Rust and its live
edits use the documented JavaScript Markdown subset. Exact undo restores the
native initial content. For native rendering of every edit and native PDF
publication, use the separate embedded WASM workspace API described in
[the interactive workspace guide](../wasm/INTERACTIVE.md).

Raw HTML passthrough is unsupported: `allowRawHtml` may only be `false`.
PDF pagination, running-header and other PDF-only settings do not belong to
this API. The document worker accepts the same HTML options for the
`interactive-html` format.

Nondefault presentation and asset options require the additive
`renderInteractiveHtmlConfiguredAdvanced` binding from a matching
`wasm-workspace` build. An older generated package reports
`UNSUPPORTED_WASM_PACKAGE` before initialization instead of dropping these
settings. Default appearance and title/language-only calls keep the existing
six-argument binding for compatibility.
