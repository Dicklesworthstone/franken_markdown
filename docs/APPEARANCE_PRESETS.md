# Publication appearance presets

Native commands and MCP publication tools select the same built-in palettes as
the Rust renderers. The accepted names are `auto`, `light`, `dark`, `charcoal`,
`high-contrast-light`, and `high-contrast-dark`; spelling is case-sensitive.

```sh
fmd report.md --to both --appearance charcoal --out report.html
fmd report.md --to svg --appearance high-contrast-dark --out report.svg
fmd book chapters --to all --appearance high-contrast-light --out-dir dist
fmd config set appearance high-contrast-light
fmd config get appearance
```

`--appearance` is available in render, watch, batch, book and PDF verification.
An explicit flag overrides the persistent `appearance` setting; `--no-config`
ignores persistent settings. Watch rebuilds when that configuration changes.
Selecting a palette preserves the configured body font, type scale, page size,
margins and other publication settings.

`auto` is the unchanged default. HTML and EPUB retain their automatic host
appearance behavior; static PDF and SVG use the default light palette. Explicit
appearances remain fixed regardless of the viewer's preferred color scheme.
High-contrast appearances use black/white foreground and background colors and
the renderer's high-contrast syntax colors. `dark_mode` controls automatic theme
switching and does not override a fixed appearance. A replacement stylesheet
supplied with `--css` or MCP `customCss` owns HTML/EPUB styling itself.

Verification keeps presentation settings explicit, even when rendering normally
inherits native configuration:

```sh
fmd verify report.md --appearance high-contrast-dark --font serif --json
```

Pass the same explicit layout and resource options used for publication when
verifying that publication. Appearance selection does not change the verifier's
existing accessibility or conformance scope.

## MCP

`appearance` is an optional string in `fmd.render_html`, `fmd.render_pdf`,
`fmd.render_svg`, `fmd.render_epub`, `fmd.render_file`, `fmd.render_book`, and
`fmd.verify`. Discovery lists the same six names on every consuming tool.

```json
{
  "name": "fmd.render_book",
  "arguments": {
    "chapters": [{"path": "chapter.md", "markdown": "# Accessible edition\n\nBody text."}],
    "to": "all",
    "appearance": "high-contrast-dark",
    "font": "serif",
    "fontScale": "125%"
  }
}
```

Paired file exports and complete book editions share the selected palette across
their outputs. Invalid names are rejected with `invalid_appearance` before source
reads or resource decoding. Wrong JSON types are protocol argument errors.
Default `fmd.verify_book` checks navigation without rendering and accepts no
appearance option. With explicit `target: "pdf"`, it accepts the same appearance
and typography options as PDF book publication.

## Rust

Use `Theme::for_appearance(appearance)` to choose the built-in palette, then apply
font, spacing or page overrides. `SystemAppearance` implements `FromStr` for the
same six canonical names. `Theme::with_appearance` retains its existing behavior:
it selects an appearance without replacing caller-supplied color palettes.
