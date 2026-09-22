# MCP render resources

`fmd.render_html`, `fmd.render_pdf`, and `fmd.render_file` accept optional
`images` and `fonts` arrays. Each payload is caller-supplied canonical padded
base64, not a URL or path to read. The file tool reads only its explicitly
requested Markdown file. Markdown image destinations do not authorize asset
reads or network requests.

```json
{
  "name": "fmd.render_file",
  "arguments": {
    "path": "report.md",
    "to": "svg",
    "out": "report.svg",
    "images": [
      { "destination": "plot.svg", "base64": "PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxMCIgaGVpZ2h0PSIxMCIvPg==" }
    ]
  }
}
```

The example embeds a small empty SVG canvas. Real callers encode their image
bytes and retain the exact destination used by the Markdown image reference.
PNG/JPEG/SVG are the portable cross-export image formats; each backend keeps
its own validation and unsupported-format behavior. This does not add a new
image decoder to HTML/PDF/EPUB. SVG resource semantics and containment are
specified in `SVG_RESOURCES.md`.

A font entry has `slot`, `base64`, and optional integer `weight` (1..1000).
Canonical slots are `body-regular`, `body-bold`, `body-italic`,
`body-bold-italic`, and `mono-regular`. Duplicate font slots are errors. Font
bytes pass through the shared TrueType validation; weight pins retain the
existing variable-font semantics. Missing slots use the normal bundled faces.

The same supplied resources reach HTML, PDF, EPUB, SVG, and both artifacts of
paired HTML/PDF file exports. Resource-free requests retain their existing
response payloads. Binary file exports remain raw bytes on disk; inline PDF
and EPUB remain base64. The paired inline response retains its outputs array.

## Admission and failure behavior

Resource limits are 32 MiB per decoded asset, 64 MiB combined encoded payloads
and identifiers, a separate 64 MiB combined decoded budget, 4096 images, and
five fonts. Keys must contain 1..4096 bytes without control characters. JSON
frame and Markdown input limits still apply independently. All metadata and
projected sizes are checked before decoding. Unknown nested fields, wrong
types, invalid padding, noncanonical font slots, and bad font bytes are errors.
A malformed resource fails before output replacement. Existing source-alias
checks and staged multi-output writes remain in force. No asset is fetched.

The shared renderers decide whether an admitted image format is supported.
SVG preserves missing/malformed image alt text and unsupported mathematics as
visible fallback, and now returns those recoverable diagnostics in an additive
`warnings` array beside `content` and `isError`. Each entry has `code` and
`message`; warnings are returned for saved and inline SVG exports. A warning
is not a claim that the export failed. HTML/PDF/EPUB keep their pre-existing
backend fallback behavior; this change does not add their warning APIs to MCP.

## Verification scope

`tests/mcp_resources_test.rs` compares real dispatcher output to directly
configured renderers for every format and covers paired exports, source/output
preservation, diagnostics, and discovery. Resource-module tests cover base64,
nested contracts, admission and font pins. These Rust tests were added but not
executed in the implementation environment: Cargo/rustc/rustfmt and DSR were
unavailable. Source hashes and `git diff --check` were verified. Run the normal
DSR gates with `--features mcp`, including both existing MCP suites and the new
resource suite, before reporting compilation or runtime acceptance.
