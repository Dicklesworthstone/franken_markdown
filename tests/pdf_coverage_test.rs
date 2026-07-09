//! Coverage-focused structural tests for the PDF writer's less-traveled paths:
//! alpha-channel PNG soft masks, synthesized SVG arrowheads, link hitboxes for
//! non-rect SVG shapes, curved-path markers, gradient/pattern layer ordering,
//! embedded-vector viewport mapping, and table column-width rebalancing.
//! Like tests/pdf_test.rs these are intentionally byte-level: they pin
//! deterministic writer invariants without a third-party PDF parser.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PageMargins, PageSize, PdfImageAsset, PdfOptions, Theme, parse_markdown, render_pdf,
    render_warnings,
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
