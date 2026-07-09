//! Coverage-focused structural tests for the PDF writer's less-traveled paths:
//! alpha-channel PNG soft masks, synthesized SVG arrowheads, link hitboxes for
//! non-rect SVG shapes, curved-path markers, gradient/pattern layer ordering,
//! embedded-vector viewport mapping, and table column-width rebalancing.
//! Like tests/pdf_test.rs these are intentionally byte-level: they pin
//! deterministic writer invariants without a third-party PDF parser.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PageMargins, PageSize, PdfImageAsset, PdfOptions, RenderWarning, Theme, parse_markdown,
    render_pdf, render_warnings,
};

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    // The renderer does not validate CRCs; the chunk envelope is enough.
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

/// A one-row 8-bit RGBA PNG (color type 6), the alpha-carrying variant of
/// pdf_test.rs's `tiny_rgb_png`.
fn tiny_rgba_png(pixels: &[[u8; 4]]) -> Vec<u8> {
    let width = pixels.len() as u32;
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, deflate, PNG filters, no interlace.

    let mut rows = Vec::with_capacity(1 + pixels.len() * 4);
    rows.push(0); // filter type 0 for the single row.
    for pixel in pixels {
        rows.extend_from_slice(pixel);
    }
    let idat = franken_markdown::compress::zlib_compress(&rows);

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

fn rgba_png_from_filtered_rows(width: u32, rows: &[Vec<u8>]) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&(rows.len() as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA.

    let row_bytes = width as usize * 4;
    let mut raw = Vec::with_capacity(rows.len() * (row_bytes + 1));
    for row in rows {
        assert_eq!(
            row.len(),
            row_bytes + 1,
            "row must include filter byte plus RGBA samples"
        );
        raw.extend_from_slice(row);
    }

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(
        b"IDAT",
        &franken_markdown::compress::zlib_compress(&raw),
    ));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        out.push(TABLE[(chunk[0] >> 2) as usize] as char);
        out.push(TABLE[(((chunk[0] & 0x03) << 4) | (chunk[1] >> 4)) as usize] as char);
        out.push(TABLE[(((chunk[1] & 0x0f) << 2) | (chunk[2] >> 6)) as usize] as char);
        out.push(TABLE[(chunk[2] & 0x3f) as usize] as char);
    }
    match chunks.remainder() {
        [a] => {
            out.push(TABLE[(a >> 2) as usize] as char);
            out.push(TABLE[((a & 0x03) << 4) as usize] as char);
            out.push('=');
            out.push('=');
        }
        [a, b] => {
            out.push(TABLE[(a >> 2) as usize] as char);
            out.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            out.push(TABLE[((b & 0x0f) << 2) as usize] as char);
            out.push('=');
        }
        [] => {}
        _ => unreachable!(),
    }
    out
}

fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn svg_opts(name: &str, svg: impl Into<Vec<u8>>) -> PdfOptions {
    PdfOptions {
        image_assets: vec![PdfImageAsset::new(name, svg)],
        ..PdfOptions::default()
    }
}

fn small_page_opts(width_pt: f32, height_pt: f32) -> PdfOptions {
    let mut theme = Theme::default();
    theme.page.size = PageSize {
        name: "test-small",
        width_pt,
        height_pt,
    };
    theme.page.margins = PageMargins {
        top_pt: 20.0,
        right_pt: 20.0,
        bottom_pt: 20.0,
        left_pt: 20.0,
    };
    PdfOptions {
        theme,
        ..PdfOptions::default()
    }
}

fn text_x_positions(bytes: &[u8], font_size: &str) -> Vec<f32> {
    let needle = format!("{font_size} Tf 1 0 0 1 ");
    let text = as_text(bytes);
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(pos) = rest.find(&needle) {
        let tail = &rest[pos + needle.len()..];
        if let Some(end) = tail.find(' ')
            && let Ok(x) = tail[..end].parse::<f32>()
        {
            out.push(x);
        }
        rest = &rest[pos + needle.len()..];
    }
    out
}

// ---------------------------------------------------------------------------
// PNG alpha channel -> /SMask soft mask objects
// ---------------------------------------------------------------------------

#[test]
fn pdf_rgba_png_embeds_devicergb_xobject_with_soft_mask() {
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "images/alpha.png",
            tiny_rgba_png(&[[0xD0, 0x22, 0x40, 0xFF], [0x20, 0x64, 0xC8, 0x80]]),
        )],
        ..PdfOptions::default()
    };
    let doc = parse_markdown("![Alpha chart](images/alpha.png)");
    assert!(
        render_warnings(&doc, &opts).is_empty(),
        "an 8-bit RGBA PNG is a supported first-class asset and must not warn"
    );

    let pdf = render_pdf("![Alpha chart](images/alpha.png)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Subtype /Image"),
        "RGBA PNG should still become an image XObject: {text}"
    );
    assert!(
        text.contains("/ColorSpace /DeviceRGB"),
        "color samples stay RGB; alpha moves to the soft mask: {text}"
    );
    let smask_ref_pos = text
        .find(" /SMask ")
        .expect("image dictionary should reference a soft-mask object");
    let smask_tail = &text[smask_ref_pos + " /SMask ".len()..];
    let smask_obj: usize = smask_tail[..smask_tail.find(' ').unwrap()]
        .parse()
        .expect("/SMask must reference an object number");
    assert!(
        text.contains(&format!(
            "{smask_obj} 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 \
             /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode"
        )),
        "the referenced soft mask must be an 8-bit DeviceGray image XObject: {text}"
    );
    assert!(
        !text.contains("/Predictor 15"),
        "the full-decode alpha path re-compresses unfiltered samples, so no PNG predictor: {text}"
    );
}

#[test]
fn pdf_rgba_png_full_decode_handles_all_filter_rows_and_soft_mask() {
    let png = rgba_png_from_filtered_rows(
        1,
        &[
            vec![0, 0x10, 0x20, 0x30, 0xff],
            vec![1, 0x05, 0x06, 0x07, 0x80],
            vec![2, 0x00, 0x00, 0x00, 0x00],
            vec![3, 0x08, 0x08, 0x08, 0x40],
            vec![4, 0x00, 0x00, 0x00, 0x00],
        ],
    );
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("images/filters.png", png)],
        ..PdfOptions::default()
    };
    let doc = parse_markdown("![Filtered](images/filters.png)");
    assert!(
        render_warnings(&doc, &opts).is_empty(),
        "RGBA rows using PNG filters 0..4 should decode without degradation"
    );

    let pdf = render_pdf("![Filtered](images/filters.png)", &opts).unwrap();
    let text = as_text(&pdf);
    assert!(
        text.contains("/ColorSpace /DeviceRGB") && text.contains(" /SMask "),
        "full-decoded RGBA PNG should keep RGB samples and emit a soft mask: {text}"
    );
    assert!(
        !text.contains("/Predictor 15"),
        "full decode must re-encode unfiltered rows rather than claiming PNG predictor bytes: {text}"
    );
}

// ---------------------------------------------------------------------------
// Empty documents still produce a valid single-page PDF
// ---------------------------------------------------------------------------

#[test]
fn pdf_empty_document_emits_single_blank_page() {
    let first = render_pdf("", &PdfOptions::default()).unwrap();
    let second = render_pdf("", &PdfOptions::default()).unwrap();
    assert_eq!(first, second, "empty renders must be byte-deterministic");

    let text = as_text(&first);
    assert!(text.starts_with("%PDF-1.7"), "PDF header must be present");
    assert!(
        text.contains("/Type /Pages") && text.contains("/Count 1"),
        "an empty document synthesizes exactly one blank page: {text}"
    );
    assert!(text.contains("/Type /Page"), "page object must exist");
    assert!(text.trim_end().ends_with("%%EOF"), "trailer must terminate");
}

// ---------------------------------------------------------------------------
// Dangling marker references synthesize stroke-colored arrowheads
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_dangling_marker_reference_synthesizes_stroke_arrowheads() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 24">
  <line x1="10" y1="10" x2="40" y2="10" stroke="#ff0000" stroke-width="2"
        marker-start="url(#missing-start)" marker-end="url(#missing-end)"/>
</svg>
"##;
    let pdf = render_pdf("![Arrows](arrows.svg)", &svg_opts("arrows.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("1.000 0.000 0.000 RG 2 w 0 J 0 j 4 M 10 10 m 40 10 l S"),
        "the line itself still strokes normally: {text}"
    );
    // End arrowhead: size = 2 * 4.6 = 9.2, half-width 3.864, tip at (40,10).
    assert!(
        text.contains("1.000 0.000 0.000 rg 30.8 13.86 m 40 10 l 30.8 6.14 l h f"),
        "a dangling marker-end must fall back to a filled arrowhead at the line tip: {text}"
    );
    // Start arrowhead points the other way, tip at (10,10).
    assert!(
        text.contains("1.000 0.000 0.000 rg 19.2 6.14 m 10 10 l 19.2 13.86 l h f"),
        "a dangling marker-start must fall back to a reversed arrowhead: {text}"
    );
}

// ---------------------------------------------------------------------------
// Link hitboxes for every SVG shape kind (not just rect/text)
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_links_annotate_ellipse_line_poly_and_path_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 100">
  <a href="https://example.com/ellipse">
    <ellipse cx="20" cy="12" rx="10" ry="6" fill="#22c55e"/>
  </a>
  <a href="https://example.com/line">
    <line x1="4" y1="30" x2="44" y2="30" stroke="#111111" stroke-width="2"/>
  </a>
  <a href="https://example.com/polyline">
    <polyline points="4,44 24,52 44,44" fill="none" stroke="#111111" stroke-width="2"/>
  </a>
  <a href="https://example.com/polygon">
    <polygon points="4,64 24,72 44,64" fill="#3b82f6"/>
  </a>
  <a href="https://example.com/path">
    <path d="M4 84 L44 84 L24 94 Z" fill="#f97316"/>
  </a>
</svg>
"##;
    let pdf = render_pdf("![Shapes](links.svg)", &svg_opts("links.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches("/Subtype /Link").count(),
        5,
        "each visible linked shape kind should get its own hitbox: {text}"
    );
    for target in ["ellipse", "line", "polyline", "polygon", "path"] {
        assert!(
            text.contains(&format!("/URI (https://example.com/{target})")),
            "linked {target} should carry its URI action: {text}"
        );
    }
}

// ---------------------------------------------------------------------------
// Path markers on curved segments, closes, and marker-start tangents
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_path_markers_follow_cubic_quadratic_and_close_segments() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 90 40">
  <defs>
    <marker id="arrow" markerWidth="8" markerHeight="8" refX="8" refY="4" orient="auto" markerUnits="strokeWidth">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff0000"/>
    </marker>
  </defs>
  <path d="M10 10 C20 0 30 0 40 10 Q50 20 60 10 L70 10 Z" fill="none" stroke="#0000ff"
        marker-start="url(#arrow)" marker-mid="url(#arrow)" marker-end="url(#arrow)"/>
  <path d="M10 30 Q20 38 30 30" fill="none" stroke="#0000ff" marker-end="url(#arrow)"/>
  <path d="M40 30 L60 30" fill="none" stroke="#0000ff" marker-start="url(#arrow)"/>
  <path d="M70 30 Q80 38 88 30" fill="none" stroke="#0000ff" marker-start="url(#arrow)"/>
  <path d="M84 8 Z" fill="none" stroke="#0000ff" marker-start="url(#arrow)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Curves](curve-markers.svg)",
        &svg_opts("curve-markers.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    let marker_paints = text
        .matches("1.000 0.000 0.000 rg 0 0 m 8 4 l 0 8 l h f")
        .count();
    assert_eq!(
        marker_paints, 8,
        "expected start(1) + mids at C/Q, Q/L, L/Z joints (3) + close end (1) on the first \
         path, a quadratic end (1) on the second, line/quad start tangents (2) on the \
         third and fourth, and NO marker for the tangent-less M+Z path; saw \
         {marker_paints}\n{text}"
    );
}

// ---------------------------------------------------------------------------
// Poly paint-order marker layering and 180-degree mid-marker reversal
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_polyline_paint_order_layers_markers_before_stroke() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40">
  <defs>
    <marker id="dot" markerWidth="4" markerHeight="4" refX="2" refY="2">
      <path d="M0 0 L4 0 L4 4 L0 4 Z" fill="#ff0000"/>
    </marker>
  </defs>
  <polyline points="10,20 40,20 10,20" fill="none" stroke="#00aa00" stroke-width="2"
            paint-order="markers stroke" marker-mid="url(#dot)"/>
</svg>
"##;
    let pdf = render_pdf("![Order](order.svg)", &svg_opts("order.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    let marker = text
        .find("1.000 0.000 0.000 rg 0 0 m 4 0 l 4 4 l 0 4 l h f")
        .expect("the reversal vertex still places a mid marker");
    let stroke = text
        .find("0.000 0.667 0.000 RG 2 w")
        .expect("polyline stroke must still paint");
    assert!(
        marker < stroke,
        "paint-order=\"markers stroke\" must emit the marker before the stroke: \
         marker at {marker}, stroke at {stroke}\n{text}"
    );
}

// ---------------------------------------------------------------------------
// Gradient and pattern fills routed through explicit paint-order layers
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_paint_order_fill_layer_supports_gradients_and_patterns() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <defs>
    <linearGradient id="warm">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <pattern id="dots" width="4" height="4" patternUnits="userSpaceOnUse">
      <rect x="0" y="0" width="2" height="2" fill="#00ff00"/>
    </pattern>
  </defs>
  <rect x="2" y="2" width="10" height="10" fill="url(#warm)" stroke="#111111" paint-order="stroke"/>
  <rect x="20" y="2" width="8" height="8" fill="url(#dots)" stroke="#111111" paint-order="stroke"/>
</svg>
"##;
    let pdf = render_pdf("![Layers](layers.svg)", &svg_opts("layers.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    // Gradient rect: the stroke layer paints first, then the shading fill layer.
    let grad_stroke = text
        .find("2 2 10 10 re S")
        .expect("gradient rect should stroke its outline first");
    let grad_fill = text
        .find("q 2 2 10 10 re W n /SG")
        .expect("gradient fill layer should clip and paint a native shading");
    assert!(
        grad_stroke < grad_fill,
        "paint-order=stroke must put the stroke before the gradient fill: {text}"
    );

    // Pattern rect: the fill layer tiles the pattern content under a clip.
    let pattern_fill = text
        .find("q 20 2 8 8 re W n ")
        .expect("pattern fill layer should clip to the shape");
    let pattern_stroke = text
        .find("20 2 8 8 re S")
        .expect("pattern rect should also stroke its outline");
    assert!(
        pattern_stroke < pattern_fill,
        "paint-order=stroke must put the stroke before the pattern tiles: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 rg 0 0 2 2 re f"),
        "pattern tile content should paint inside the fill layer: {text}"
    );
}

#[test]
fn pdf_svg_gradient_fill_with_stroke_paints_stroke_after_shading() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <defs>
    <linearGradient id="warm">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <rect x="2" y="2" width="10" height="10" fill="url(#warm)" stroke="#111111" stroke-width="2"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Both](grad-stroke.svg)",
        &svg_opts("grad-stroke.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    let fill = text
        .find("q 2 2 10 10 re W n /SG1 sh\nQ\n")
        .expect("gradient fill should clip and shade the rect");
    let stroke_tail = &text[fill..];
    assert!(
        stroke_tail.contains("2 w 0 J 0 j 4 M 2 2 10 10 re S"),
        "the solid stroke must still paint after the gradient fill in normal paint order: {text}"
    );
}

// ---------------------------------------------------------------------------
// Radial gradients in userSpaceOnUse units (absolute and percent forms)
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_radial_gradient_user_space_units_map_to_native_shading() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
  <defs>
    <radialGradient id="abs" gradientUnits="userSpaceOnUse" cx="30" cy="30" r="25">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
    <radialGradient id="pct" gradientUnits="userSpaceOnUse" cx="50%" cy="50%" r="25%">
      <stop offset="0%" stop-color="#00ff00"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
  </defs>
  <circle cx="30" cy="30" r="20" fill="url(#abs)"/>
  <circle cx="90" cy="30" r="20" fill="url(#pct)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Radial](radial-user.svg)",
        &svg_opts("radial-user.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/ShadingType 3 /ColorSpace /DeviceRGB /Coords [30 30 0 30 30 25]"),
        "absolute userSpaceOnUse radial coordinates should map straight into /Coords: {text}"
    );
    assert_eq!(
        text.matches("/ShadingType 3").count(),
        1,
        "percentage userSpaceOnUse radial coordinates are not representable and must not \
         register a bogus native shading: {text}"
    );
    assert!(
        text.contains("0.000 0.750 0.250 rg") || text.contains("0.000 0.500 0.500 rg"),
        "the percent-radius circle should keep a deterministic representative flat fill: {text}"
    );
}

// ---------------------------------------------------------------------------
// Pattern fill edge cases: transform, even-odd, empty and oversized tilings
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_pattern_transform_and_evenodd_clip_apply_to_tiles() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <defs>
    <pattern id="grid" width="4" height="4" patternUnits="userSpaceOnUse" patternTransform="translate(3 4)">
      <rect x="0" y="0" width="2" height="2" fill="#ff0000"/>
    </pattern>
  </defs>
  <rect x="2" y="2" width="8" height="8" fill="url(#grid)" fill-rule="evenodd"/>
</svg>
"##;
    let pdf = render_pdf("![Grid](pattern-tx.svg)", &svg_opts("pattern-tx.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("2 2 8 8 re W* n"),
        "an even-odd pattern fill must clip with W*: {text}"
    );
    assert!(
        text.contains(" cm 1 0 0 1 3 4 cm"),
        "patternTransform should be applied inside each tile after the tile translation: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 rg 0 0 2 2 re f"),
        "tile content should still paint: {text}"
    );
}

#[test]
fn pdf_svg_degenerate_patterns_fall_back_to_flat_fill() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 120">
  <defs>
    <pattern id="empty" width="4" height="4" patternUnits="userSpaceOnUse"></pattern>
    <pattern id="micro" width="1" height="1" patternUnits="userSpaceOnUse">
      <rect x="0" y="0" width="1" height="1" fill="#ff0000"/>
    </pattern>
  </defs>
  <rect x="2" y="2" width="20" height="20" fill="url(#empty) #12c0aa"/>
  <rect x="40" y="2" width="100" height="100" fill="url(#micro)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Degenerate](pattern-bad.svg)",
        &svg_opts("pattern-bad.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("rg 2 2 20 20 re f"),
        "an empty pattern must fall back to the explicit fallback color as a flat fill: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 rg 40 2 100 100 re f"),
        "a >512-tile pattern must fall back to the representative flat fill instead of \
         exploding the page stream: {text}"
    );
    assert!(
        !text.contains("q 40 2 100 100 re W n q 1 0 0 1"),
        "no tiling should be attempted for the oversized pattern: {text}"
    );
}

// ---------------------------------------------------------------------------
// Gradient-stroked <line> edge cases
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_line_gradient_stroke_opacity_dashes_and_degenerate_lengths() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <defs>
    <linearGradient id="g" gradientUnits="userSpaceOnUse" x1="2" y1="8" x2="42" y2="8">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <line x1="2" y1="8" x2="42" y2="8" stroke="url(#g)" stroke-width="2" stroke-opacity="0.5"/>
  <line x1="2" y1="18" x2="42" y2="18" stroke="url(#g)" stroke-width="2" stroke-dasharray="3 1"/>
  <line x1="2" y1="28" x2="2" y2="28" stroke="url(#g)" stroke-width="2"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Lines](grad-lines.svg)",
        &svg_opts("grad-lines.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/GSa05000500 gs"),
        "a translucent gradient stroke should shade under an ExtGState alpha: {text}"
    );
    assert!(
        text.contains("q /GSa05000500 gs 2 9 m 42 9 l 42 7 l 2 7 l h W n /SG"),
        "the translucent gradient stroke still clips the stroke outline and shades: {text}"
    );
    assert!(
        text.contains("[3 1] 0 d"),
        "a dashed gradient stroke falls back to a dashed vector stroke: {text}"
    );
    assert!(
        text.contains("2 18 m 42 18 l S"),
        "the dashed fallback still strokes the line geometry: {text}"
    );
    assert!(
        text.contains("2 28 m 2 28 l S"),
        "a zero-length gradient line cannot build a stroke outline and falls back to a \
         plain stroke: {text}"
    );
}

// ---------------------------------------------------------------------------
// Embedded vector <image> viewport mapping (none / slice) and element state
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_embedded_vector_image_honors_inner_preserve_aspect_ratio() {
    let stretch_inner = base64_encode(
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" preserveAspectRatio="none"><rect x="1" y="1" width="8" height="8" fill="#22c55e"/></svg>"##,
    );
    let slice_inner = base64_encode(
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" preserveAspectRatio="xMidYMid slice"><rect x="1" y="1" width="8" height="8" fill="#3b82f6"/></svg>"##,
    );
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
  <image x="2" y="2" width="20" height="10" preserveAspectRatio="none" href="data:image/svg+xml;base64,{stretch_inner}"/>
  <image x="30" y="2" width="20" height="10" preserveAspectRatio="none" href="data:image/svg+xml;base64,{slice_inner}"/>
</svg>
"##
    );
    let pdf = render_pdf(
        "![Nested aspect](nested-aspect.svg)",
        &svg_opts("nested-aspect.svg", svg.into_bytes()),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("2 0 0 1 2 2 cm"),
        "preserveAspectRatio=none on the inner svg should scale non-uniformly to the \
         embedded viewport: {text}"
    );
    assert!(
        text.contains("30 2 20 10 re W n 2 0 0 2 30 -3 cm"),
        "slice on the inner svg should clip to the embedded viewport, scale by the larger \
         axis, and center the overflow: {text}"
    );
}

#[test]
fn pdf_svg_embedded_image_transform_and_hidden_images() {
    let png_data = base64_encode(&tiny_rgba_png(&[
        [0x0B, 0x61, 0xA4, 0xFF],
        [0xF5, 0x9E, 0x0B, 0xFF],
    ]));
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <image x="4" y="4" width="10" height="5" preserveAspectRatio="none" transform="translate(3 2)" href="data:image/png;base64,{png_data}"/>
  <image x="20" y="4" width="10" height="5" visibility="hidden" href="data:image/png;base64,{png_data}"/>
</svg>
"##
    );
    let pdf = render_pdf(
        "![Transformed raster](raster-tx.svg)",
        &svg_opts("raster-tx.svg", svg.into_bytes()),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("1 0 0 1 3 2 cm 10 0 0 -5 4 9 cm /Im1 Do"),
        "an embedded image transform should prefix the draw matrix: {text}"
    );
    assert_eq!(
        text.matches(" Do").count(),
        1,
        "a visibility:hidden embedded image must not be drawn: {text}"
    );
}

// ---------------------------------------------------------------------------
// Root slice viewports clip vector content and selectable text
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_root_slice_viewport_clips_shapes_and_text() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="60" height="20" viewBox="0 0 30 30" preserveAspectRatio="xMidYMid slice">
  <rect x="2" y="2" width="26" height="26" fill="#22c55e"/>
  <text x="4" y="16" font-size="6" fill="#111111">Sliced</text>
</svg>
"##;
    let pdf = render_pdf("![Sliced](sliced.svg)", &svg_opts("sliced.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    let clip = text
        .find(" re W n ")
        .expect("root slice viewport should install a clip rectangle");
    let cm = text.find(" cm\n").expect("root transform must follow");
    assert!(
        clip < cm,
        "the slice clip must be installed before the root user-unit transform: {text}"
    );
    let clip_count = text.matches(" re W n").count();
    assert!(
        clip_count >= 2,
        "the selectable text run repeats the viewport clip inside its own text state; \
         saw {clip_count} clips\n{text}"
    );
    assert!(
        text.contains("BT /F1"),
        "sliced SVG text must stay selectable: {text}"
    );
}

// ---------------------------------------------------------------------------
// SVG text painted with fill AND stroke in normal paint order (2 Tr)
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_text_fill_and_stroke_use_fill_stroke_render_mode() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 30">
  <text x="6" y="20" font-size="12" fill="#ff0000" stroke="#0000ff" stroke-width="1">Both</text>
</svg>
"##;
    let pdf = render_pdf("![Both](text-both.svg)", &svg_opts("text-both.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    let start = text
        .find("1.000 0.000 0.000 rg\n0.000 0.000 1.000 RG ")
        .expect("fill and stroke colors should both be set before the text object");
    let object = &text[start..start + 200];
    assert!(
        object.contains(" Tf 2 Tr "),
        "fill+stroke text in normal paint order must draw once with render mode 2: {object}"
    );
    assert!(
        object.contains(" w 0 J 0 j 4 M [] 0 d"),
        "the stroke pen state should be configured for the dual-mode text: {object}"
    );
    let text_objects = text.matches("BT /F1").count();
    assert_eq!(
        text_objects, 1,
        "normal-order fill+stroke text renders in a single pass, not two layers: {text}"
    );
}

// ---------------------------------------------------------------------------
// Text clip paths in objectBoundingBox units with cubic segments
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_text_object_bounding_box_clip_maps_cubic_ops() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 30">
  <defs>
    <clipPath id="wave" clipPathUnits="objectBoundingBox">
      <path d="M0 0 C0.3 0.2 0.7 0.2 1 0 L1 1 L0 1 Z"/>
    </clipPath>
  </defs>
  <text x="6" y="20" font-size="12" fill="#111111" clip-path="url(#wave)">Clipped</text>
</svg>
"##;
    let pdf = render_pdf("![Clip](text-clip.svg)", &svg_opts("text-clip.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    let bt = text.find("BT /F1").expect("clipped text still renders");
    let before = &text[..bt];
    let clip_start = before
        .rfind("q\n")
        .map(|pos| &before[pos..])
        .expect("text draws inside an isolated graphics state");
    assert!(
        clip_start.contains(" c ") && clip_start.contains("W n"),
        "the objectBoundingBox clip path must emit mapped cubic segments and install a \
         clip before the text object: {clip_start}"
    );
}

// ---------------------------------------------------------------------------
// Root CSS background gradients under partial root opacity
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_root_background_gradient_respects_root_opacity() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20" opacity="0.5"
     style="background: linear-gradient(#ffffff, #000000)">
  <rect x="2" y="2" width="10" height="10" fill="#22c55e"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Backdrop](bg-opacity.svg)",
        &svg_opts("bg-opacity.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/ShadingType 2"),
        "the root CSS background should still register a native axial shading: {text}"
    );
    let shading_use = text
        .find(" sh\nQ\n")
        .expect("the background layer should paint its shading");
    let before = &text[..shading_use];
    assert!(
        before.contains("/GSa05000500 gs"),
        "a half-opaque root must wrap its background shading in an ExtGState alpha: {text}"
    );
}

// ---------------------------------------------------------------------------
// Adversarial SVG definition bodies and text subtrees
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_css_comments_clip_masks_and_style_transforms_render_visible_content() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 90 50">
  <style>
  <![CDATA[
    /* exporter wrapper with braces { } that must not affect block matching */
    :root { --mask-white: #ffffff; --ink: #111111; }
    .panel { fill: #22c55e; /* ignored { declaration } */ stroke: var(--ink); stroke-width: 2; }
    g > rect.outline { fill: none; stroke: #0000ff; }
    @media screen { rect { fill: #ff0000; } }
  ]]>
  </style>
  <defs>
    <clipPath id="clip" clipPathUnits="objectBoundingBox" clip-rule="evenodd"
              transform="translate(0 0)">
      <!-- comments and declarations inside definition bodies are skipped -->
      <?ignored?>
      <!ignored>
      <rect x="0" y="0" width="1" height="1" fill-rule="evenodd"
            style="transform: scale(0.9)"/>
      <circle cx="0.5" cy="0.5" r="0.25"/>
      <ellipse cx="0.5" cy="0.5" rx="0.12" ry="0.2"/>
      <polygon points="0,0 1,0 0.5,1"/>
      <path d="M0.15 0.15 L0.85 0.15 L0.5 0.85 Z"/>
    </clipPath>
    <mask id="mask" maskContentUnits="objectBoundingBox" style="transform: translate(0 0)">
      <!-- only the bright, non-transparent shape should reveal content -->
      <rect x="0" y="0" width="1" height="1"
            style="fill: var(--mask-white); fill-opacity: 0.75"/>
      <rect x="0" y="0" width="1" height="1" fill="#000000"/>
      <rect x="0" y="0" width="1" height="1" fill="none"/>
    </mask>
  </defs>
  <g>
    <rect class="panel" x="6" y="6" width="36" height="20"
          clip-path="url(#clip)" mask="url(#mask)"/>
    <rect class="outline" x="48" y="6" width="28" height="20"/>
  </g>
</svg>
"##;
    let pdf = render_pdf(
        "![Definitions](adversarial-defs.svg)",
        &svg_opts("adversarial-defs.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.133 0.773 0.369 rg"),
        "the CSS class fill must survive comments, CDATA wrappers, and ignored @media: {text}"
    );
    assert!(
        text.contains("W* n"),
        "the even-odd clipPath should install an even-odd clipping path: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 RG"),
        "the child-combinator CSS rule should stroke the outline rectangle blue: {text}"
    );
    assert!(
        !text.contains("1.000 0.000 0.000 rg 6 6 36 20 re f"),
        "the ignored @media rule must not turn the panel red: {text}"
    );
}

#[test]
fn pdf_svg_nested_text_tspans_text_paths_and_position_lists_all_render() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 42">
  <defs>
    <path id="curve" d="M4 28 C24 4 48 4 68 28"/>
  </defs>
  <text x="4" y="14" font-size="8" fill="#111111" textLength="64"
        lengthAdjust="spacingAndGlyphs">
    A
    <!-- a comment between text children must be ignored -->
    <tspan dx="2" dy="1" fill="#ff0000" textLength="24" lengthAdjust="spacing">red</tspan>
    <tspan x="12 24 36" y="32 30 28" dx="1 2" dy="0 0" fill="#0000ff">xyz</tspan>
    <textPath href="#curve" startOffset="25%" fill="#22c55e">path</textPath>
    <tspan display="none">hidden</tspan>
    <metadata><tspan fill="#ff00ff">ignored</tspan></metadata>
  </text>
</svg>
"##;
    let pdf = render_pdf(
        "![Nested text](nested-text.svg)",
        &svg_opts("nested-text.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.matches("BT /F1").count() >= 5,
        "plain text, a tspan, positioned characters, and a textPath should all draw: {text}"
    );
    for color in [
        "0.067 0.067 0.067 rg",
        "1.000 0.000 0.000 rg",
        "0.000 0.000 1.000 rg",
        "0.133 0.773 0.369 rg",
    ] {
        assert!(
            text.contains(color),
            "expected nested SVG text color operator {color}: {text}"
        );
    }
    assert!(
        !text.contains("1.000 0.000 1.000 rg"),
        "metadata children and display:none tspans must not paint: {text}"
    );
}

#[test]
fn pdf_svg_percent_encoded_inner_svg_and_path_command_edges_render() {
    let inner_svg = "%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20viewBox%3D%220%200%2010%2010%22%3E%3Crect%20x%3D%221%22%20y%3D%221%22%20width%3D%228%22%20height%3D%228%22%20fill%3D%22%23dd3377%22%2F%3E%3C%2Fsvg%3E";
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 140 60">
  <image x="4" y="4" width="18" height="18" preserveAspectRatio="none"
         href="data:image/svg+xml,{inner_svg}"/>
  <image x="28" y="4" width="18" height="18"
         href="data:image/svg+xml;base64,QUJD=QUJD"/>
  <path fill="none" stroke="#111111" stroke-width="1.5"
        d="M50 20 h10 v-10 h-10 z
           m18 0 c5 -10 15 -10 20 0 s15 10 20 0
           q5 -10 10 0 t10 0
           a5 3 30 0 1 15 5
           a0 4 0 0 1 10 0
           A5 5 0 1 0 136 20"/>
</svg>
"##
    );
    let pdf = render_pdf(
        "![Nested data uri](data-uri-paths.svg)",
        &svg_opts("data-uri-paths.svg", svg.into_bytes()),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.867 0.200 0.467 rg"),
        "the percent-encoded nested SVG should decode and paint its inner rect: {text}"
    );
    assert!(
        text.contains("0.067 0.067 0.067 RG 1.5 w"),
        "the path command corpus should keep its stroke style: {text}"
    );
    assert!(
        text.contains(" c "),
        "cubic, smooth cubic, quadratic, smooth quadratic, and arc commands should lower \
         to Bezier curve operators: {text}"
    );
    assert!(
        !text.contains("/Subtype /Image"),
        "the malformed base64 image should be ignored instead of becoming a raster XObject: {text}"
    );
}

#[test]
fn pdf_svg_accessible_and_nested_text_feed_missing_glyph_warnings() {
    let nested = base64_encode(
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><text x="1" y="8" font-size="6">&#x10FFFF;</text></svg>"##,
    );
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"
     aria-label="Outer chart">
  <title>Outer chart</title>
  <desc>Nested labels should contribute to diagnostics.</desc>
  <style>text {{ fill: #111111; }}</style>
  <script>ignored()</script>
  <text x="4" y="16" font-size="10">&#x10FFFF;</text>
  <image x="4" y="20" width="24" height="12"
         href="data:image/svg+xml;base64,{nested}"/>
</svg>
"##
    );
    let opts = svg_opts("accessible.svg", svg.into_bytes());
    let doc = parse_markdown("![](accessible.svg)");
    let warnings = render_warnings(&doc, &opts);
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            RenderWarning::MissingGlyphs { count, sample }
                if *count >= 2 && sample.contains('\u{10ffff}')
        )),
        "missing glyph diagnostics should include root and nested SVG text: {warnings:?}"
    );

    let pdf = render_pdf("![](accessible.svg)", &opts).unwrap();
    let text = as_text(&pdf);
    assert!(
        text.contains("/Alt (Outer chart - Nested labels should contribute to diagnostics.)")
            || text.contains("/Alt <FEFF"),
        "empty markdown alt text should fall back to the SVG accessible name: {text}"
    );
}

#[test]
fn pdf_svg_background_colors_text_decoration_and_spacing_variants_render() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 140 64"
     style="background-color: color-mix(in srgb, #ffffff 75%, #000000);
            background-image: linear-gradient(to right bottom, rgba(255 0 0 / 50%) 0%, transparent, #0000ff 100%),
                              radial-gradient(circle at right bottom, #00ff00, rgba(0,0,255,0.25));">
  <defs>
    <linearGradient id="reflect" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="24" y2="0"
                    spreadMethod="reflect">
      <stop offset="0%" stop-color="color-mix(in srgb, red 25%, blue)"/>
      <stop offset="50%" stop-color="rgb(0 128 255 / 75%)"/>
      <stop offset="100%" stop-color="#0f08"/>
    </linearGradient>
  </defs>
  <rect x="4" y="4" width="72" height="18" fill="url('#reflect') #123456"/>
  <text x="6" y="44" font-size="12" fill="currentColor" color="#111111"
        stroke="rgba(0,0,255,0.5)" stroke-width="0.75"
        letter-spacing="0.1em" word-spacing="25%" baseline-shift="super"
        dominant-baseline="hanging"
        text-decoration="underline overline line-through"
        paint-order="stroke fill">Decorated text</text>
</svg>
"##;
    let pdf = render_pdf(
        "![Decorated](decorated.svg)",
        &svg_opts("decorated.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.matches("/ShadingType").count() >= 3,
        "root background layers plus reflected gradient fill should register native shadings: {text}"
    );
    assert!(
        text.contains("0.067 0.067 0.067 rg"),
        "currentColor fill should resolve from the text color attribute: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 RG 0.56 w"),
        "rgba stroke and explicit stroke width should configure the scaled text stroke: {text}"
    );
    assert!(
        text.matches("S\n").count() >= 3,
        "underline, overline, and line-through should draw decoration strokes: {text}"
    );
}

#[test]
fn pdf_svg_symbol_use_filter_primitives_and_xml_noise_render() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 130 58">
  <style>
    symbol .leaf { fill: #22c55e; }
    #blue rect { fill: #0000ff; }
  </style>
  <defs>
    <?defs ignored?>
    <!defs ignored>
    <symbol id="badge" viewBox="0 0 20 10" preserveAspectRatio="xMaxYMin slice">
      <>
      <?inside-symbol?>
      <!inside-symbol>
      <g class="leaf">
        <rect x="1" y="1" width="8" height="6"/>
        <text x="11" y="7" font-size="5" fill="#111111">Hi</text>
      </g>
    </symbol>
    <g id="blue">
      <rect x="0" y="0" width="10" height="8"/>
    </g>
    <filter id="primitive-shadow">
      <>
      <?inside-filter?>
      <!inside-filter>
      <feOffset in="SourceAlpha" dx="2" dy="3" result="off"/>
      <feGaussianBlur in="off" stdDeviation="0" result="blur"/>
      <feFlood flood-color="#333333" flood-opacity="0.5" result="flood"/>
      <feComposite in="flood" in2="blur" operator="in" result="shadow"/>
      <feMerge>
        <feMergeNode in="shadow"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
  </defs>
  <a href="https://example.com/badge">
    <use href="#badge" x="4" y="6" width="50" height="24"
         filter="url(#primitive-shadow)"/>
  </a>
  <use xlink:href="#blue" x="70" y="6"/>
  <use href="url('#missing')" x="92" y="6"/>
</svg>
"##;
    let pdf = render_pdf("![Use](use-filter.svg)", &svg_opts("use-filter.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.133 0.773 0.369 rg"),
        "a CSS-styled symbol referenced by <use> should paint its green rect: {text}"
    );
    assert!(
        text.contains("0.200 0.200 0.200 rg"),
        "the feOffset/feGaussianBlur/feFlood/feComposite/feMerge chain should \
         lower to the existing shadow layer: {text}"
    );
    assert!(
        text.contains("BT /F1"),
        "text inside a referenced symbol should be parsed and emitted: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg"),
        "xlink:href <use> references should still resolve reusable groups: {text}"
    );
    assert!(
        text.contains("/URI (https://example.com/badge)"),
        "links wrapping <use> should propagate to the referenced elements: {text}"
    );
}

#[test]
fn pdf_svg_marker_bodies_skip_xml_noise_and_accept_shape_variants() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 70">
  <defs>
    <marker id="multi" markerWidth="12" markerHeight="12" refX="6" refY="6"
            markerUnits="userSpaceOnUse" orient="0.25turn">
      <>
      <?inside-marker?>
      <!inside-marker>
      <line x1="0" y1="1" x2="12" y2="1" stroke="#111111" stroke-width="1"/>
      <polyline points="0,4 6,0 12,4" fill="none" stroke="#0000ff" stroke-width="1"/>
      <polygon points="0,8 6,12 12,8" fill="#22c55e"/>
      <rect x="1" y="1" width="3" height="3" fill="#ff0000"/>
      <circle cx="8" cy="3" r="2" fill="#ffaa00"/>
      <ellipse cx="8" cy="9" rx="3" ry="2" fill="#dd3377"/>
    </marker>
    <marker id="rad" markerWidth="8" markerHeight="8" refX="4" refY="4" orient="3.14159265rad">
      <path d="M0 0 L8 4 L0 8 Z" fill="#00ffff"/>
    </marker>
    <marker id="grad" markerWidth="8" markerHeight="8" refX="4" refY="4" orient="100grad">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff00ff"/>
    </marker>
  </defs>
  <line x1="10" y1="14" x2="70" y2="14" stroke="#000000" marker-end="url(#multi)"/>
  <line x1="10" y1="34" x2="70" y2="34" stroke="#000000" marker-end="url(#rad)"/>
  <line x1="10" y1="54" x2="70" y2="54" stroke="#000000" marker-end="url(#grad)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Marker variants](marker-variants.svg)",
        &svg_opts("marker-variants.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    for color in [
        "0.067 0.067 0.067 RG",
        "0.000 0.000 1.000 RG",
        "0.133 0.773 0.369 rg",
        "1.000 0.000 0.000 rg",
        "1.000 0.667 0.000 rg",
        "0.867 0.200 0.467 rg",
        "0.000 1.000 1.000 rg",
        "1.000 0.000 1.000 rg",
    ] {
        assert!(
            text.contains(color),
            "marker body shape/color variant {color} should render: {text}"
        );
    }
}

#[test]
fn pdf_svg_embedded_raster_images_honor_meet_slice_and_xlink_href() {
    let png_data = base64_encode(&tiny_rgba_png(&[
        [0x10, 0x80, 0xF0, 0xFF],
        [0xF0, 0x80, 0x10, 0xFF],
    ]));
    let png_with_ws = format!("{}\n {}", &png_data[..8], &png_data[8..]);
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 28">
  <image x="4" y="4" width="16" height="16"
         preserveAspectRatio="xMidYMid slice"
         href="data:image/png;base64,{png_data}"/>
  <image x="28" y="4" width="16" height="16"
         preserveAspectRatio="xMinYMax meet"
         xlink:href="data:image/png;base64,{png_with_ws}"/>
  <image x="48" y="4" width="8" height="8"
         href="data:image/pngbad;base64,{png_data}"/>
</svg>
"##
    );
    let pdf = render_pdf(
        "![Embedded rasters](embedded-raster-aspect.svg)",
        &svg_opts("embedded-raster-aspect.svg", svg.into_bytes()),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches(" Do").count(),
        2,
        "the valid href and xlink:href images draw; malformed metadata is ignored: {text}"
    );
    assert!(
        text.contains("4 4 16 16 re W n"),
        "slice preserveAspectRatio should clip overflowing raster content to its viewport: {text}"
    );
    assert!(
        text.contains("16 0 0 -8 28 20 cm"),
        "meet preserveAspectRatio with xMinYMax should letterbox a 2:1 image at the bottom: {text}"
    );
}

#[test]
fn pdf_svg_defensive_edge_corpus_skips_invalid_inputs_without_losing_valid_paint() {
    let png_data = base64_encode(&tiny_rgba_png(&[[0x20, 0x90, 0xE0, 0xFF]]));
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 90" aria-label="Edge corpus">
  <title>First title</title>
  <title>Ignored second title</title>
  <desc>First desc</desc>
  <desc>Ignored second desc</desc>
  <defs>
    <linearGradient id="quoted" x1="0" y1="0" x2="40" y2="0">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <pattern id="zero-w" width="0" height="8"><rect width="8" height="8" fill="#ff0000"/></pattern>
    <pattern id="zero-h" width="8" height="0"><rect width="8" height="8" fill="#ff0000"/></pattern>
    <clipPath id="bbox"><rect x="0" y="0" width="1" height="1"/></clipPath>
  </defs>
  <rect x="1" y="1" width="0" height="8" fill="#ff0000"/>
  <rect x="4" y="4" width="18" height="10" fill="url(&quot;#quoted&quot;) #00ff00"/>
  <rect x="28" y="4" width="18" height="10" fill="transparent" stroke="none"/>
  <rect x="52" y="4" width="18" height="10"
        fill="color-mix(in display-p3, red, blue)" stroke="none"/>
  <rect x="76" y="4" width="18" height="10" fill="url(#zero-w) #22c55e"/>
  <rect x="100" y="4" width="18" height="10" fill="url(#zero-h) #22c55e"/>
  <path d="M4 28 A5 5 0 0 1 4 28 M12 28 A0 5 0 0 1 20 28 M24 28 A5 5 0 0 0 34 28"
        fill="none" stroke="#111111"/>
  <a href="https://example.com/invisible-image">
    <image x="4" y="40" width="8" height="8" opacity="0"
           href="data:image/png;base64,{png_data}"/>
  </a>
  <image x="18" y="40" width="8" height="8" href="data:image/png;base64,=AAA"/>
  <image x="32" y="40" width="8" height="8" href="data:image/png;base64,A=AA"/>
  <image x="46" y="40" width="8" height="8" href="data:image/png;base64,AA=A"/>
  <image x="60" y="40" width="8" height="8" href="data:image/svg+xml,%"/>
  <text x="4" y="70" font-size="10" fill="none" stroke="none"
        text-decoration="underline">No decoration color</text>
  <text x="4" y="82" font-size="10" fill="#111111" textLength="0"
        lengthAdjust="spacing">A</text>
</svg>
"##
    );
    let pdf = render_pdf(
        "![Edge corpus](svg-edge-corpus.svg)",
        &svg_opts("svg-edge-corpus.svg", svg.into_bytes()),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/ShadingType 2"),
        "a double-quoted paint URL should resolve the gradient id and register a shading: {text}"
    );
    assert!(
        text.contains("0.133 0.773 0.369 rg 76 4 18 10 re f")
            && text.contains("0.133 0.773 0.369 rg 100 4 18 10 re f"),
        "degenerate patterns should fall back to their explicit green fallback paint: {text}"
    );
    assert!(
        text.contains("0.067 0.067 0.067 RG"),
        "valid path stroke must still paint after degenerate arc commands: {text}"
    );
    assert!(
        !text.contains("/URI (https://example.com/invisible-image)"),
        "an opacity-zero image inside a link must not create a hitbox: {text}"
    );
    assert_eq!(
        text.matches(" Do").count(),
        0,
        "opacity-zero and malformed embedded images should not draw raster XObjects: {text}"
    );
}

#[test]
fn pdf_image_asset_branch_corpus_exercises_svg_and_png_rejection_edges() {
    let mut empty_idat_png = Vec::new();
    empty_idat_png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    empty_idat_png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    empty_idat_png.extend_from_slice(&png_chunk(b"IEND", &[]));

    let mut missing_iend_png = Vec::new();
    missing_iend_png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    missing_iend_png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    missing_iend_png.extend_from_slice(&png_chunk(
        b"IDAT",
        &franken_markdown::compress::zlib_compress(&[0, 0, 0, 0]),
    ));

    let valid_single_quote_svg = br##"
<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 40 20'>
  <style>/* unterminated exporter comment</style>
  <rect x='2' y='2' width='12' height='8' fill='red'/>
  <a href='https://example.com/stroke-text'>
    <text x='2' y='17' font-size='6' fill='none' stroke='blue'>Stroke link</text>
  </a>
</svg>
"##;
    let invalid_zero_viewbox = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 0 10">
  <rect x="0" y="0" width="10" height="10" fill="#22c55e"/>
</svg>
"##;
    let empty_self_closing_svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>"##;
    let bad_dimension_svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="0" height="10">
  <rect x="0" y="0" width="10" height="10" fill="#22c55e"/>
</svg>
"##;

    let opts = PdfOptions {
        image_assets: vec![
            PdfImageAsset::new("single-quote.svg", *valid_single_quote_svg),
            PdfImageAsset::new("zero-viewbox.svg", *invalid_zero_viewbox),
            PdfImageAsset::new("empty-root.svg", *empty_self_closing_svg),
            PdfImageAsset::new("bad-dim.svg", *bad_dimension_svg),
            PdfImageAsset::new("empty-idat.png", empty_idat_png),
            PdfImageAsset::new("missing-iend.png", missing_iend_png),
        ],
        ..PdfOptions::default()
    };
    let md = "\
![valid](single-quote.svg)\n\n\
![zero viewbox](zero-viewbox.svg)\n\n\
![empty root](empty-root.svg)\n\n\
![bad dimensions](bad-dim.svg)\n\n\
![empty idat](empty-idat.png)\n\n\
![missing iend](missing-iend.png)\n";
    let doc = parse_markdown(md);
    let warnings = render_warnings(&doc, &opts);
    assert!(
        warnings.len() >= 5,
        "malformed SVG/PNG assets should be reported as degraded content: {warnings:?}"
    );

    let pdf = render_pdf(md, &opts).unwrap();
    let text = as_text(&pdf);
    assert!(
        text.contains("1.000 0.000 0.000 rg"),
        "the valid single-quoted SVG should still paint red: {text}"
    );
    assert!(
        text.contains("/URI (https://example.com/stroke-text)"),
        "stroke-only linked SVG text should still create a usable annotation: {text}"
    );
    assert!(
        !text.contains("0.133 0.773 0.369 rg"),
        "invalid SVG roots must not leak their green rects into the PDF: {text}"
    );
}

// ---------------------------------------------------------------------------
// Table column-width rebalancing (allocator residue and minimum-width floors)
// ---------------------------------------------------------------------------

#[test]
fn pdf_table_minimum_width_floor_positions_columns_on_tiny_pages() {
    // content_w - margins = 60pt < the 72pt table floor, so avail clamps to 72;
    // 72 - 3*14 gutters = 30 <= 3*18 min widths -> every column sits at the
    // 18pt floor with no extra units to allocate.
    let pdf = render_pdf(
        "| a | b | c |\n|---|---|---|\n| d | e | f |\n",
        &small_page_opts(100.0, 220.0),
    )
    .unwrap();
    let xs = text_x_positions(&pdf, "10.00");
    for expected in [27.0, 59.0, 91.0] {
        assert!(
            xs.iter().any(|x| (x - expected).abs() < 0.01),
            "min-width columns should sit at x=27/59/91 (left + gutter halves + 18pt \
             floors); saw {xs:?}"
        );
    }
}

#[test]
fn pdf_table_allocator_rebalances_sub_unit_width_residue() {
    // A single right-aligned column makes the final column width directly
    // observable through the cell's x position. Page width 112.2pt leaves a
    // +0.2pt residue after 0.5pt-unit allocation (grow rebalance); 112.3pt
    // leaves -0.2pt (shrink rebalance). The right edges must therefore differ
    // by ~0.1pt, not by the raw 0.5pt quantization step.
    let md = "| head |\n|---:|\n| xx |\n";
    let grow = render_pdf(md, &small_page_opts(112.2, 220.0)).unwrap();
    let shrink = render_pdf(md, &small_page_opts(112.3, 220.0)).unwrap();

    let mut grow_xs = text_x_positions(&grow, "10.00");
    let mut shrink_xs = text_x_positions(&shrink, "10.00");
    grow_xs.sort_by(f32::total_cmp);
    shrink_xs.sort_by(f32::total_cmp);
    assert_eq!(
        grow_xs.len(),
        shrink_xs.len(),
        "both renders should place the same cells"
    );
    assert!(!grow_xs.is_empty(), "table cells should render text");
    for (g, s) in grow_xs.iter().zip(shrink_xs.iter()) {
        let diff = s - g;
        assert!(
            (0.05..=0.15).contains(&diff),
            "right-aligned cell x must track the rebalanced column width \
             (expected ~0.1pt shift, got {diff}; grow={grow_xs:?} shrink={shrink_xs:?})"
        );
    }
}

// ---------------------------------------------------------------------------
// Visibility, no-paint, and drop-shadow paths for line/ellipse/poly
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_hidden_or_paintless_shapes_skip_ops_while_shadows_still_paint() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 60">
  <line x1="2" y1="4" x2="40" y2="4" stroke="#ff0000" stroke-width="2" visibility="hidden"/>
  <line x1="2" y1="10" x2="40" y2="10" stroke="none"/>
  <ellipse cx="20" cy="20" rx="8" ry="4" fill="#ff0000" visibility="hidden"/>
  <polyline points="2,30 40,30" fill="none" stroke="none"/>
  <line x1="2" y1="40" x2="40" y2="40" stroke="#0000ff" stroke-width="2"
        filter="drop-shadow(1 2 0 #000000)"/>
  <ellipse cx="20" cy="52" rx="8" ry="4" fill="#22c55e" filter="drop-shadow(1 1 0 #333333)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Mixed](hidden-shadow.svg)",
        &svg_opts("hidden-shadow.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("2 4 m 40 4 l") && !text.contains("2 10 m 40 10 l"),
        "hidden and paintless lines must not emit path operators: {text}"
    );
    assert!(
        !text.contains("28 20 m "),
        "a hidden ellipse must not emit its bezier outline: {text}"
    );
    // The shadow layer re-draws the line path under a (1,2) translation and
    // STROKES it (a stroke-only shape shadows with S, not f).
    assert!(
        text.contains("q 1 0 0 1 1 2 cm 0.000 0.000 0.000 RG 2 w 0 J 0 j 4 M 2 40 m 40 40 l S Q"),
        "the line drop-shadow should stroke an offset copy of the path first: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 RG 2 w 0 J 0 j 4 M 2 40 m 40 40 l S"),
        "the real line stroke still paints after its shadow: {text}"
    );
    assert!(
        text.contains("q 1 0 0 1 1 1 cm 0.200 0.200 0.200 rg 12 52 m "),
        "the ellipse drop-shadow should fill an offset outline first: {text}"
    );
}

// ---------------------------------------------------------------------------
// textLength edge cases: negative values and single-glyph spacing
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Marker fallbacks: empty markers, paintless shapes, degenerate viewBoxes
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_unusable_markers_fall_back_or_skip_deterministically() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 50">
  <defs>
    <marker id="empty" markerWidth="8" markerHeight="8"></marker>
    <marker id="nopaint" markerWidth="8" markerHeight="8" orient="auto">
      <path d="M0 0 L8 4 L0 8 Z" fill="none"/>
    </marker>
    <marker id="degenerate-vb" markerWidth="8" markerHeight="8" refX="1" refY="1" orient="auto" viewBox="0 0 0 8">
      <path d="M0 0 L8 4 L0 8 Z" fill="#00ff00"/>
    </marker>
    <marker id="slice-vb" markerWidth="4" markerHeight="2" orient="auto" viewBox="0 0 8 8"
            preserveAspectRatio="xMidYMid slice">
      <path d="M0 0 L8 4 L0 8 Z" fill="#0000ff"/>
    </marker>
    <marker id="real" markerWidth="8" markerHeight="8" orient="auto">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff00ff"/>
    </marker>
    <marker id="ctx" markerWidth="8" markerHeight="8" orient="auto">
      <path d="M0 0 L8 4 L0 8 Z" fill="context-fill"/>
    </marker>
  </defs>
  <line x1="4" y1="8" x2="40" y2="8" stroke="#ff0000" stroke-width="2" marker-end="url(#empty)"/>
  <line x1="4" y1="18" x2="40" y2="18" stroke="#ff0000" stroke-width="2" marker-end="url(#nopaint)"/>
  <line x1="4" y1="28" x2="40" y2="28" stroke="#ff0000" stroke-width="2" marker-end="url(#degenerate-vb)"/>
  <line x1="4" y1="38" x2="40" y2="38" stroke="#ff0000" stroke-width="2" marker-end="url(#slice-vb)"/>
  <line x1="20" y1="46" x2="20" y2="46" stroke="#ff0000" stroke-width="2" marker-end="url(#real)"/>
  <line x1="4" y1="48" x2="40" y2="48" stroke="#ff0000" stroke-width="2" marker-end="url(#ctx)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Markers](marker-edge.svg)",
        &svg_opts("marker-edge.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    // Marker with no shapes: same fallback arrowhead as a dangling reference.
    assert!(
        text.contains("1.000 0.000 0.000 rg 30.8 11.86 m 40 8 l 30.8 4.14 l h f"),
        "a shape-less marker must fall back to the synthesized arrowhead: {text}"
    );
    // A marker whose only shape is paintless parses down to no usable shapes,
    // so it takes the same arrowhead fallback.
    assert!(
        text.contains("1.000 0.000 0.000 rg 30.8 21.86 m 40 18 l 30.8 14.14 l h f"),
        "a paintless marker also falls back to the synthesized arrowhead: {text}"
    );
    // Degenerate zero-width viewBox: refX/refY are used unmapped and the shape
    // still paints.
    assert!(
        text.contains("0.000 1.000 0.000 rg 0 0 m 8 4 l 0 8 l h f"),
        "a marker with a degenerate viewBox still paints its shape unscaled: {text}"
    );
    // Slice viewBox: scale = max(4/8, 2/8) = 0.5 with centered overflow.
    assert!(
        text.contains("0.5 0 0 0.5 0 -1 cm"),
        "a slice marker viewBox should scale by the larger axis and center overflow: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg 0 0 m 8 4 l 0 8 l h f"),
        "the slice-viewBox marker shape still paints: {text}"
    );
    // A zero-length line has no tangent to orient an auto marker along, and the
    // synthesized arrowhead is equally impossible, so nothing may paint.
    assert!(
        !text.contains("1.000 0.000 1.000 rg"),
        "a real marker on a zero-length line has no orientation and must not paint: {text}"
    );
    // context-fill on a marker shape resolves against the referencing line,
    // which has no fill, so the marker shape is skipped without an arrowhead.
    assert!(
        !text.contains("m 40 48 l 30.8"),
        "a context-fill marker without context fill paints neither shape nor arrowhead: {text}"
    );
}

// ---------------------------------------------------------------------------
// Radial gradient strokes on lines use native radial shadings
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_line_radial_gradient_stroke_uses_native_radial_shading() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
  <defs>
    <radialGradient id="glow" gradientUnits="userSpaceOnUse" cx="22" cy="8" r="30">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
  </defs>
  <line x1="2" y1="8" x2="42" y2="8" stroke="url(#glow)" stroke-width="2"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Glow](radial-line.svg)",
        &svg_opts("radial-line.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/ShadingType 3 /ColorSpace /DeviceRGB /Coords [22 8 0 22 8 30]"),
        "a radial gradient stroke should register a native radial shading: {text}"
    );
    assert!(
        text.contains("q 2 9 m 42 9 l 42 7 l 2 7 l h W n /SG"),
        "the line's stroke outline should clip the radial shading: {text}"
    );
}

// ---------------------------------------------------------------------------
// Repeated (repeat/reflect) gradients with unusable periods fall back flat
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_repeat_gradients_with_degenerate_periods_fall_back_to_flat_fill() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 120">
  <defs>
    <linearGradient id="zero-span" gradientUnits="userSpaceOnUse" x1="10" y1="10" x2="10" y2="10" spreadMethod="repeat">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <linearGradient id="too-many" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="1" y2="0" spreadMethod="repeat">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <linearGradient id="way-too-many" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0.05" y2="0" spreadMethod="repeat">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <radialGradient id="offset-focus" gradientUnits="userSpaceOnUse" cx="30" cy="30" r="10" fx="35" fy="30" spreadMethod="repeat">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
    <radialGradient id="tiny-rings" gradientUnits="userSpaceOnUse" cx="30" cy="30" r="0.5" spreadMethod="repeat">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
    <radialGradient id="huge-focus" gradientUnits="userSpaceOnUse" cx="30" cy="30" r="10" fr="30">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
    <radialGradient id="pct-focus" gradientUnits="userSpaceOnUse" cx="30" cy="30" r="10" fx="50%">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
  </defs>
  <rect x="0" y="0" width="80" height="16" fill="url(#zero-span)"/>
  <rect x="0" y="20" width="80" height="16" fill="url(#too-many)"/>
  <rect x="0" y="40" width="80" height="16" fill="url(#way-too-many)"/>
  <rect x="0" y="60" width="80" height="16" fill="url(#offset-focus)"/>
  <rect x="0" y="80" width="80" height="16" fill="url(#tiny-rings)"/>
  <rect x="100" y="0" width="80" height="16" fill="url(#huge-focus)"/>
  <rect x="100" y="20" width="80" height="16" fill="url(#pct-focus)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Repeat](repeat-edge.svg)",
        &svg_opts("repeat-edge.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("/ShadingType"),
        "none of the degenerate repeat gradients may register native shadings: {text}"
    );
    // Every rect keeps the deterministic representative flat fill (stop average).
    for y in [0, 20, 40, 60, 80] {
        assert!(
            text.contains(&format!("0.500 0.000 0.500 rg 0 {y} 80 16 re f")),
            "rect at y={y} should fall back to the averaged stop color: {text}"
        );
    }
    // A focal radius larger than the outer radius and a percent focal center in
    // user-space units are both unrepresentable and fall back flat too.
    for y in [0, 20] {
        assert!(
            text.contains(&format!("0.500 0.000 0.500 rg 100 {y} 80 16 re f")),
            "unusable radial focal geometry at y={y} should fall back flat: {text}"
        );
    }
}

#[test]
fn pdf_svg_identical_gradients_reuse_one_registered_shading() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <defs>
    <linearGradient id="shared" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="80" y2="0">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <rect x="2" y="2" width="20" height="10" fill="url(#shared)"/>
  <rect x="30" y="2" width="20" height="10" fill="url(#shared)"/>
</svg>
"##;
    let pdf = render_pdf("![Shared](shared.svg)", &svg_opts("shared.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches("/SG1 sh").count(),
        2,
        "both rects should paint the same registered shading: {text}"
    );
    assert!(
        !text.contains("/SG2"),
        "an identical user-space gradient must be registered exactly once: {text}"
    );
}

// ---------------------------------------------------------------------------
// Embedded raster <image> with clip paths, masks, and unusable data
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_embedded_image_clip_mask_and_invalid_data() {
    let png_data = base64_encode(&tiny_rgba_png(&[
        [0x0B, 0x61, 0xA4, 0xFF],
        [0xF5, 0x9E, 0x0B, 0xFF],
    ]));
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <defs>
    <clipPath id="c"><rect x="5" y="5" width="6" height="3"/></clipPath>
    <mask id="m"><rect x="24" y="5" width="6" height="3" fill="#ffffff"/></mask>
  </defs>
  <image x="4" y="4" width="10" height="5" preserveAspectRatio="none" clip-path="url(#c)" href="data:image/png;base64,{png_data}"/>
  <image x="23" y="4" width="10" height="5" preserveAspectRatio="none" mask="url(#m)" href="data:image/png;base64,{png_data}"/>
  <image x="42" y="4" width="10" height="5" href="data:image/png;base64,AAAA"/>
  <image x="61" y="4" width="10" height="0" href="data:image/png;base64,{png_data}"/>
</svg>
"##
    );
    let pdf = render_pdf(
        "![Clipped rasters](raster-clip.svg)",
        &svg_opts("raster-clip.svg", svg.into_bytes()),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches(" Do").count(),
        2,
        "only the two valid, positively-sized images may draw: {text}"
    );
    let clipped = text
        .find("5 5 m 11 5 l 11 8 l 5 8 l h W n")
        .expect("clip-path geometry should clip the first image");
    let first_do = text.find(" Do").expect("first image draws");
    assert!(
        clipped < first_do,
        "the clip must be installed before the image draw: {text}"
    );
    assert!(
        text.contains("24 5 m 30 5 l 30 8 l 24 8 l h W n"),
        "a hard mask should clip the second image like a clip path: {text}"
    );
}

// ---------------------------------------------------------------------------
// Links on <image> and <text> elements, zero-area links, slice clamping
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_image_and_text_links_annotate_while_zero_area_links_drop() {
    let png_data = base64_encode(&tiny_rgba_png(&[[0x0B, 0x61, 0xA4, 0xFF]]));
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <a href="https://example.com/image">
    <image x="4" y="4" width="10" height="5" preserveAspectRatio="none" href="data:image/png;base64,{png_data}"/>
  </a>
  <a href="https://example.com/text">
    <text x="4" y="24" font-size="8" fill="#111111">Linked</text>
  </a>
  <a href="https://example.com/zero">
    <ellipse cx="40" cy="30" rx="0" ry="4" fill="#ff0000"/>
  </a>
  <a href="https://example.com/dot">
    <path d="M60 30 L60 30" fill="#ff0000"/>
  </a>
</svg>
"##
    );
    let pdf = render_pdf(
        "![Linked media](media-links.svg)",
        &svg_opts("media-links.svg", svg.into_bytes()),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches("/Subtype /Link").count(),
        2,
        "image and text links annotate; zero-area shapes must not: {text}"
    );
    assert!(text.contains("/URI (https://example.com/image)"));
    assert!(text.contains("/URI (https://example.com/text)"));
    assert!(!text.contains("/URI (https://example.com/zero)"));
    assert!(
        !text.contains("/URI (https://example.com/dot)"),
        "a zero-area path bbox cannot produce a usable hitbox: {text}"
    );
}

#[test]
fn pdf_svg_root_slice_clamps_link_hitboxes_to_the_viewport() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="60" height="20" viewBox="0 0 30 30" preserveAspectRatio="xMidYMid slice">
  <a href="https://example.com/clamped">
    <rect x="0" y="0" width="30" height="30" fill="#22c55e"/>
  </a>
</svg>
"##;
    let pdf = render_pdf("![Clamped](clamped.svg)", &svg_opts("clamped.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    let rect_pos = text.find("/Rect [").expect("link annotation with a rect");
    let rect_tail = &text[rect_pos + "/Rect [".len()..];
    let rect_end = rect_tail.find(']').unwrap();
    let coords: Vec<f32> = rect_tail[..rect_end]
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    assert_eq!(coords.len(), 4, "rect must have four coordinates: {text}");
    let (w, h) = (coords[2] - coords[0], coords[3] - coords[1]);
    assert!(
        (w - 45.0).abs() < 0.6 && (h - 15.0).abs() < 0.6,
        "the slice viewport must clamp the oversized link hitbox to the placed \
         60x20px (45x15pt) image box; got {w}x{h}\n{text}"
    );
}

// ---------------------------------------------------------------------------
// Drop shadows on polygons and paths
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_polygon_and_path_drop_shadows_paint_offset_layers() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 60">
  <polygon points="4,4 24,4 14,20" fill="#22c55e" filter="drop-shadow(2 1 0 #444444)"/>
  <path d="M40 4 L60 4 L50 20 Z" fill="#3b82f6" filter="drop-shadow(1 2 0 #555555)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Shadowed](shape-shadows.svg)",
        &svg_opts("shape-shadows.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 2 1 cm 0.267 0.267 0.267 rg 4 4 m 24 4 l 14 20 l h f Q"),
        "the polygon drop-shadow should fill an offset copy of the poly path: {text}"
    );
    assert!(
        text.contains("q 1 0 0 1 1 2 cm 0.333 0.333 0.333 rg 40 4 m 60 4 l 50 20 l h f Q"),
        "the path drop-shadow should fill an offset copy of the path ops: {text}"
    );
    assert!(
        text.contains("0.133 0.773 0.369 rg 4 4 m 24 4 l 14 20 l h f"),
        "the real polygon still paints after its shadow: {text}"
    );
}

#[test]
fn pdf_svg_evenodd_and_paintless_shadow_variants() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 60">
  <defs>
    <marker id="dot" markerWidth="4" markerHeight="4" refX="2" refY="2">
      <path d="M0 0 L4 0 L4 4 L0 4 Z" fill="#ff0000"/>
    </marker>
  </defs>
  <polygon points="4,4 24,4 14,20" fill="#22c55e" fill-rule="evenodd"
           filter="drop-shadow(2 1 0 #444444)"/>
  <polyline points="40,10 60,10 60,30" fill="none" stroke="none"
            marker-mid="url(#dot)" filter="drop-shadow(3 3 0 #000000)"/>
</svg>
"##;
    let pdf = render_pdf(
        "![Shadow modes](shadow-modes.svg)",
        &svg_opts("shadow-modes.svg", *svg),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("4 4 m 24 4 l 14 20 l h f* Q"),
        "an even-odd shape's drop-shadow must fill with f*: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 rg 0 0 m 4 0 l 4 4 l 0 4 l h f"),
        "the paintless polyline still places its mid marker: {text}"
    );
    assert!(
        !text.contains("q 1 0 0 1 3 3 cm"),
        "a shape with neither fill nor stroke has nothing to shadow: {text}"
    );
}

// ---------------------------------------------------------------------------
// Text stroke-width 0 disables the stroke pass entirely
// ---------------------------------------------------------------------------

#[test]
fn pdf_svg_text_zero_stroke_width_renders_fill_only() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
  <text x="4" y="14" font-size="10" fill="#111111" stroke="#0000ff" stroke-width="0">Thin</text>
</svg>
"##;
    let pdf = render_pdf("![Thin](thin.svg)", &svg_opts("thin.svg", *svg)).unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches("BT /F1").count(),
        1,
        "zero-width stroked text renders exactly one fill pass: {text}"
    );
    assert!(
        !text.contains(" Tr ") && !text.contains("0.000 0.000 1.000 RG"),
        "no stroke pen or dual render mode may be configured for stroke-width 0: {text}"
    );
}

// ---------------------------------------------------------------------------
// Unusable markdown link destinations never become annotations
// ---------------------------------------------------------------------------

#[test]
fn pdf_markdown_links_with_empty_or_suspicious_destinations_stay_plain() {
    let pdf = render_pdf(
        "[empty]() and [weird](:colon-first) stay plain, [real](https://example.com/ok) links.",
        &PdfOptions::default(),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches("/Subtype /Link").count(),
        1,
        "only the well-formed destination may annotate: {text}"
    );
    assert!(text.contains("/URI (https://example.com/ok)"));
    assert!(!text.contains("/URI ()"));
    assert!(!text.contains(":colon-first"));
}

// ---------------------------------------------------------------------------
// Oversized tables bypass the layout cache without dropping rows
// ---------------------------------------------------------------------------

#[test]
fn pdf_oversized_table_bypasses_layout_cache_and_keeps_every_row() {
    // 130 rows x 2 columns = 262 cells, past the 256-cell layout-cache cap, so
    // this table takes the uncached path every time it is laid out.
    let mut md = String::from("| k | v |\n|---|---|\n");
    for i in 0..130 {
        md.push_str(&format!("| k{i} | v{i} |\n"));
    }
    let pdf = render_pdf(&md, &PdfOptions::default()).unwrap();
    let text = as_text(&pdf);

    let pages_pos = text.find("/Type /Pages /Count ").expect("pages dict");
    let pages_tail = &text[pages_pos + "/Type /Pages /Count ".len()..];
    let page_count: usize = pages_tail[..pages_tail.find(' ').unwrap()]
        .parse()
        .expect("page count");
    assert!(page_count > 1, "130 rows must paginate: {page_count}");
    // 1 header + 130 body rows, plus one repeated header row per continuation page.
    assert_eq!(
        text.matches("/S /TR").count(),
        131 + (page_count - 1),
        "all 130 body rows plus the (repeated) headers must survive the uncached \
         table path across {page_count} pages"
    );
    let again = render_pdf(&md, &PdfOptions::default()).unwrap();
    assert_eq!(pdf, again, "uncached table layout must stay deterministic");
}

#[test]
fn pdf_svg_text_length_ignores_negative_and_single_glyph_spacing() {
    let control = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
  <text x="4" y="14" font-size="10" fill="#111111">A</text>
</svg>
"##;
    let negative = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
  <text x="4" y="14" font-size="10" fill="#111111" textLength="-12">A</text>
</svg>
"##;
    let single = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
  <text x="4" y="14" font-size="10" fill="#111111" textLength="30">A</text>
</svg>
"##;

    let control_pdf = render_pdf("![t](t.svg)", &svg_opts("t.svg", *control)).unwrap();
    let negative_pdf = render_pdf("![t](t.svg)", &svg_opts("t.svg", *negative)).unwrap();
    let single_pdf = render_pdf("![t](t.svg)", &svg_opts("t.svg", *single)).unwrap();

    assert_eq!(
        control_pdf, negative_pdf,
        "a negative textLength is invalid and must render exactly like no textLength"
    );
    assert_eq!(
        control_pdf, single_pdf,
        "spacing-mode textLength on a single glyph has no gaps to stretch and must \
         render exactly like no textLength"
    );
}
