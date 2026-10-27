# Resource-aware SVG export

SVG poster export accepts PNG, JPEG and SVG image bytes, and the same five
caller-supplied TrueType font slots used by HTML/PDF. The rendering core never
opens a file or fetches a URL. The caller decides which resources to authorize.

```rust
use franken_markdown::{FontAssets, PdfImageAsset, parse_markdown};
use franken_markdown::svg::{SvgOptions, render_svg_with_resources};

let document = parse_markdown("# Results\n\n![Measured result](plot.svg)");
let images = vec![PdfImageAsset::new("plot.svg", br#"<svg
    xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
    <path d="M0 100L100 20L200 60" fill="none" stroke="blue"/>
</svg>"#.to_vec())];
let (bytes, report, warnings) = render_svg_with_resources(
    &document, &SvgOptions::default(), &FontAssets::default(), &images,
)?;
# let _ = (bytes, report, warnings);
# Ok::<(), franken_markdown::RenderError>(())
```

Existing `render_svg`, `render_svg_with_report` and
`render_svg_with_diagnostics` signatures are unchanged. They now render inline
PNG/JPEG/SVG data URIs too. The resource-aware API additionally accepts explicit
image keys and font bytes. Keys are trimmed; the first matching supplied entry
wins, even when malformed. An explicit entry takes precedence over a data URI.
An image key is only an identifier: naming an HTTP URL does not fetch it.

## Layout and reuse

Images are indivisible inline boxes. They retain their intrinsic aspect ratio,
shrink to the available paragraph/table-cell width, and never upscale beyond
their natural size. Lines and table rows reserve the full image height. Images
inside headings, nested lists, quotes and definition lists use the same flow.
The image is bottom-aligned to its line's baseline. A mixed text/image line is
not CSS float layout; very wide images may move to their own line.

Image dimensions use CSS pixels converted to poster points (96 px = 72 pt).
SVG absolute width/height units and viewBox aspect ratios are recognized;
relative dimensions use viewBox geometry where present. Without intrinsic
size or viewBox, the default is 300 by 150 CSS pixels. The renderer does not
interpret a stylesheet to infer image dimensions or apply EXIF orientation.

The same canonical image payload is emitted once in a `symbol` definition,
then referenced by sized `use` elements. Repeated occurrences may have distinct
alt labels. Nonempty alt text is escaped into an accessible image label; empty
alt text marks the image occurrence decorative. `SvgReport.glyphs_drawn` still
counts glyph uses only and `paths_emitted` counts glyph definitions only;
neither includes image definitions/uses.

## Containment, validation and limits

SVG assets are base64-encoded image subresources, **not inserted as markup**
in the outer SVG. Conforming SVG viewers process such resources in a secure
image mode: scripts and external resource loading are disabled. This is
containment, not a general SVG sanitizer; extracting the original bytes and
opening them as a document is a different security context. Asset source bytes
are preserved, including raster alpha and SVG paths/text. Text within a supplied
SVG may retain its own font dependencies; renderer-generated prose remains
outlined and requires no view-time font.

Reference contracts:
- SVG image embedding: https://www.w3.org/TR/SVG2/embedded.html
- Secure image processing: https://www.w3.org/TR/SVG2/conform.html
- Intrinsic dimensions: https://www.w3.org/TR/SVG2/coords.html#IntrinsicSizing

PNG containers require valid chunk bounds/CRCs, IHDR, IDAT and IEND; JPEG
metadata is scanned through its frame and scan headers; SVG receives bounded,
DTD-free structural XML and intrinsic-dimension checks. Raster entropy/pixel
decoding is performed by the viewer, not a new production decoder. These checks
are not a claim that every possible corrupted image is rejected before a viewer
sees it. SVG external DTDs and XML stylesheet processing instructions are refused.

Limits apply before retained-resource growth: 32 MiB per image, 4096 images,
128 MiB aggregate supplied image payload, 128 MiB image-cache strings, 4096
bytes per explicit key, 16384 pixels per intrinsic side, and 100 million pixels.
SVG asset structure is bounded to 128 levels and 65536 elements. Data URI
percent/base64 decoding is bounded and padding is checked. Encoded cache limits
include source keys, so a data URI may consume more budget than an explicit key.
Fonts use the shared per-face validation and a separate 128 MiB aggregate limit.

Missing or malformed images preserve visible alt-text placeholders, with one
warning per painted occurrence. Table measurement does not emit extra warnings.
Stable image codes are `svg_image_missing`, `svg_image_unsupported`,
`svg_image_invalid`, `svg_image_dimensions`, and `svg_image_limit`. Invalid font
resources and oversized supplied collections return `RenderError::InvalidInput`
before producing a poster. Variable `wght` fonts are instanced at their requested
slot weight; instancing failure is an error, not a silently substituted face.

## Regression coverage and current validation

`src/svg/image_tests.rs` is included in both the library and existing standalone
SVG harness. It covers valid PNG fixtures, CRC/truncation, JPEG metadata, SVG
units/viewBox/XML, data URI decoding, payload reuse, geometry, table heights,
containment/escaping, first-match behavior, missing assets, resource admission,
custom font outlines, and deterministic resource-aware output.

The implementation environment had no Cargo/rustc/rustfmt or DSR runner. Source
reconstruction hashes and diff checks were verified, but these Rust tests and
browser rendering of the actual compiled renderer were not executed. Run the
normal DSR-backed Rust gates, including `cargo test --test svg_test`, before
claiming runtime validation. No Actions workflow was added or used.
