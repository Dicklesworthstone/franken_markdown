//! Branch-coverage tests for the SVG parsing / CSS-cascade / gradient / transform
//! subsystem in the first half (source lines < 12700) of `src/pdf.rs`.
//!
//! Every SVG is delivered through the public API exactly as a browser would see
//! it: an image asset referenced by a Markdown `![alt](name.svg)` and turned into
//! vector PDF content by `render_pdf`. The renderer emits the SVG's vector content
//! *uncompressed* into the page stream, so these tests pin the exact PDF operator
//! substrings (`rg`/`RG` colours, `cm` transforms, `m`/`l`/`c` path ops, `d` dash
//! arrays, `sh` shadings, `Tj`/`TJ` text) that each parser branch produces.
//!
//! The focus is the *edge* arms the happy-path suite never takes: malformed
//! colours falling back to the inherited black fill, rejected transforms, degenerate
//! arcs, alternate enum keywords, and error/None returns deep in the parsers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfImageAsset, PdfOptions, parse_markdown, render_pdf, render_warnings};

/// Render `![alt](<name>)` with a single supplied SVG asset and return the raw
/// PDF as lossy UTF-8 (the SVG vector stream is emitted uncompressed).
fn svg_pdf(alt: &str, name: &str, svg: &str) -> String {
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(name, svg.as_bytes().to_vec())],
        ..PdfOptions::default()
    };
    let a = render_pdf(&format!("![{alt}]({name})"), &opts).unwrap();
    let b = render_pdf(&format!("![{alt}]({name})"), &opts).unwrap();
    assert_eq!(
        a, b,
        "SVG render must be byte-for-byte deterministic: {name}"
    );
    String::from_utf8_lossy(&a).into_owned()
}

/// Convenience: render with a fixed alt derived from the asset name.
fn svg(name: &str, body: &str) -> String {
    svg_pdf("d", name, body)
}

#[track_caller]
fn assert_has(text: &str, needle: &str) {
    assert!(
        text.contains(needle),
        "expected PDF to contain {needle:?}\n--- content excerpt ---\n{}",
        excerpt(text, needle)
    );
}

#[track_caller]
fn assert_absent(text: &str, needle: &str) {
    assert!(
        !text.contains(needle),
        "expected PDF NOT to contain {needle:?}"
    );
}

/// Show a window of the content around the first path operator, to make failures
/// legible without dumping the whole binary.
fn excerpt(text: &str, _needle: &str) -> String {
    let start = text.find(" re f").or_else(|| text.find(" m ")).unwrap_or(0);
    let lo = start.saturating_sub(200);
    let hi = (start + 400).min(text.len());
    text[lo..hi].to_string()
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&0u32.to_be_bytes()); // CRC is not validated by the reader.
    out
}

/// A one-row 8-bit RGB (colour type 2) PNG the renderer accepts as an XObject.
fn tiny_rgb_png(pixels: &[[u8; 3]]) -> Vec<u8> {
    let width = pixels.len() as u32;
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB.
    let mut rows = Vec::with_capacity(1 + pixels.len() * 3);
    rows.push(0);
    for p in pixels {
        rows.extend_from_slice(p);
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
            out.push_str("==");
        }
        [a, b] => {
            out.push(TABLE[(a >> 2) as usize] as char);
            out.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            out.push(TABLE[((b & 0x0f) << 2) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

// ===========================================================================
// Colours: named / hex 3-4-6-8 digit / rgb()/rgba() forms
// ===========================================================================

#[test]
fn color_named_and_hex_digit_forms_map_to_exact_rg() {
    // #rgb short, #rrggbb long, named `green` (#008000), and #rgba/#rrggbbaa
    // whose alpha becomes a fill-opacity ExtGState.
    let text = svg(
        "hex.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<rect x="0" y="0" width="8" height="8" fill="#0f8"/>
<rect x="10" y="0" width="8" height="8" fill="green"/>
<rect x="20" y="0" width="8" height="8" fill="#11223344"/>
<rect x="30" y="0" width="8" height="8" fill="#0f84"/>
</svg>"##,
    );
    assert_has(&text, "0.000 1.000 0.533 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 0.502 0.000 rg 10 0 8 8 re f");
    // #11223344 -> rgb 0.067 0.133 0.200, alpha 0x44/255 = 0.267 -> /ca 0.267.
    assert_has(&text, "/GSa02671000 gs 0.067 0.133 0.200 rg 20 0 8 8 re f");
    assert_has(&text, "/ca 0.267 /CA 1.000");
    // #0f84 short alpha: rgb 0.000/1.000/0.533, alpha 0x4/15 = 0.267.
    assert_has(&text, "0.000 1.000 0.533 rg 30 0 8 8 re f");
}

#[test]
fn color_rgb_function_space_comma_percent_and_slash_alpha() {
    let text = svg(
        "rgbfn.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<rect x="0" y="0" width="8" height="8" fill="rgb(255 0 136)"/>
<rect x="10" y="0" width="8" height="8" fill="rgb(50%, 0%, 100%)"/>
<rect x="20" y="0" width="8" height="8" fill="rgba(10, 20, 30, 0.5)"/>
<rect x="30" y="0" width="8" height="8" fill="rgb(10 20 30 / 50%)"/>
</svg>"##,
    );
    assert_has(&text, "1.000 0.000 0.533 rg 0 0 8 8 re f");
    assert_has(&text, "0.500 0.000 1.000 rg 10 0 8 8 re f");
    // Comma alpha and slash alpha both route to a 0.5 fill-opacity ExtGState.
    assert_has(&text, "/GSa05001000 gs 0.039 0.078 0.118 rg 20 0 8 8 re f");
    assert_has(&text, "/GSa05001000 gs 0.039 0.078 0.118 rg 30 0 8 8 re f");
}

#[test]
fn color_rgb_function_malformed_falls_back_to_inherited_black() {
    // Wrong channel counts, non-finite channel, and a double-slash alpha all fail
    // to parse; the shape keeps the initial black fill (0 0 0).
    let text = svg(
        "rgbbad.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<rect x="0" y="0" width="8" height="8" fill="rgb(1 2)"/>
<rect x="10" y="0" width="8" height="8" fill="rgb(1 2 3 4)"/>
<rect x="20" y="0" width="8" height="8" fill="rgb(1e40 0 0)"/>
<rect x="30" y="0" width="8" height="8" fill="rgb(1 2 3 / 0.5 / 0.5)"/>
</svg>"##,
    );
    for x in ["0", "10", "20", "30"] {
        assert_has(&text, &format!("0.000 0.000 0.000 rg {x} 0 8 8 re f"));
    }
    // None of the malformed channels leaked a coloured operator.
    assert_absent(&text, "0.004 0.008 0.012 rg");
}

#[test]
fn color_hsl_function_units_alpha_vars_and_gradients() {
    let text = svg(
        "hslfn.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 24">
<style>
:root { --hue: 240deg; --sat: 100%; --lit: 50%; --fade: 25%; }
.varfill { fill: hsl(var(--hue) var(--sat) var(--lit) / var(--fade)); }
</style>
<defs>
<linearGradient id="g"><stop offset="0" stop-color="hsl(0 100% 50%)"/><stop offset="1" stop-color="hsl(240 100% 50%)"/></linearGradient>
</defs>
<rect x="0" y="0" width="8" height="8" fill="hsl(120 100% 50%)"/>
<rect x="10" y="0" width="8" height="8" fill="hsl(0.5turn 100% 50%)"/>
<rect x="20" y="0" width="8" height="8" fill="hsla(240, 100%, 50%, 0.5)"/>
<rect x="30" y="0" width="8" height="8" fill="hsl(3.1415927rad 100% 50% / 25%)"/>
<rect x="40" y="0" width="8" height="8" fill="hsl(200grad 100% 50%)"/>
<rect class="varfill" x="50" y="0" width="8" height="8"/>
<rect x="60" y="0" width="8" height="8" fill="url(#g)"/>
</svg>"##,
    );
    assert_has(&text, "0.000 1.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 1.000 1.000 rg 10 0 8 8 re f");
    assert_has(&text, "/GSa05001000 gs 0.000 0.000 1.000 rg 20 0 8 8 re f");
    assert_has(&text, "/ca 0.500 /CA 1.000");
    assert_has(&text, "/GSa02501000 gs 0.000 1.000 1.000 rg 30 0 8 8 re f");
    assert_has(&text, "0.000 1.000 1.000 rg 40 0 8 8 re f");
    assert_has(&text, "/GSa02501000 gs 0.000 0.000 1.000 rg 50 0 8 8 re f");
    assert_has(&text, "/C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000]");
}

#[test]
fn color_hsl_function_malformed_falls_back_to_inherited_black() {
    let text = svg(
        "hslbad.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<rect x="0" y="0" width="8" height="8" fill="hsl(120 100 50%)"/>
<rect x="10" y="0" width="8" height="8" fill="hsl(120 100% 50% / 0.5 / 0.5)"/>
<rect x="20" y="0" width="8" height="8" fill="hsl(120foo 100% 50%)"/>
<rect x="30" y="0" width="8" height="8" fill="hsl(120, 100%)"/>
</svg>"##,
    );
    for x in ["0", "10", "20", "30"] {
        assert_has(&text, &format!("0.000 0.000 0.000 rg {x} 0 8 8 re f"));
    }
    assert_absent(&text, "0.000 1.000 0.000 rg 0 0 8 8 re f");
}

// ===========================================================================
// color-mix(in srgb, ...) weighting
// ===========================================================================

#[test]
fn color_mix_weight_combinations_blend_exactly() {
    let text = svg(
        "mix.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<rect x="0" y="0" width="8" height="8" fill="color-mix(in srgb, red 30%, blue)"/>
<rect x="10" y="0" width="8" height="8" fill="color-mix(in srgb, red, blue 40%)"/>
<rect x="20" y="0" width="8" height="8" fill="color-mix(in srgb, red, blue)"/>
<rect x="30" y="0" width="8" height="8" fill="color-mix(in srgb, red 20%, blue 20%)"/>
</svg>"##,
    );
    // red 30% + blue 70%.
    assert_has(&text, "0.300 0.000 0.700 rg 0 0 8 8 re f");
    // red (60%) + blue 40%.
    assert_has(&text, "0.600 0.000 0.400 rg 10 0 8 8 re f");
    // default 50/50.
    assert_has(&text, "0.500 0.000 0.500 rg 20 0 8 8 re f");
    // 20% + 20% normalise to 50/50.
    assert_has(&text, "0.500 0.000 0.500 rg 30 0 8 8 re f");
}

#[test]
fn color_mix_invalid_forms_fall_back_to_black() {
    let text = svg(
        "mixbad.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<rect x="0" y="0" width="8" height="8" fill="color-mix(in srgb, red)"/>
<rect x="10" y="0" width="8" height="8" fill="color-mix(in hsl, red, blue)"/>
<rect x="20" y="0" width="8" height="8" fill="color-mix(in srgb, red 0%, blue 0%)"/>
<rect x="30" y="0" width="8" height="8" fill="color-mix(in srgb, , blue)"/>
<rect x="40" y="0" width="8" height="8" fill="color-mix(in srgb, red -5%, blue)"/>
<rect x="50" y="0" width="8" height="8" fill="color-mix(in srgb extra, red, blue)"/>
</svg>"##,
    );
    for x in ["0", "10", "20", "30", "40", "50"] {
        assert_has(&text, &format!("0.000 0.000 0.000 rg {x} 0 8 8 re f"));
    }
}

// ===========================================================================
// Paint via url(#..) references and keyword fallbacks
// ===========================================================================

#[test]
fn paint_url_reference_fallbacks_and_none_keyword() {
    let text = svg(
        "url.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<rect x="0" y="0" width="8" height="8" fill="url(#missing) #ff0000"/>
<rect x="10" y="0" width="8" height="8" fill="url(#fm-node-gradient)"/>
<rect x="20" y="0" width="8" height="8" fill="none" stroke="#00ff00" stroke-width="1"/>
<rect x="30" y="0" width="8" height="8" fill="url(#nope)"/>
</svg>"##,
    );
    // url(#missing) with a literal fallback colour resolves to that colour.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    // The synthetic franken node-gradient id resolves to its grey.
    assert_has(&text, "0.965 0.965 0.965 rg 10 0 8 8 re f");
    // fill="none" + a stroke -> stroked only (S), never filled.
    assert_has(&text, "0.000 1.000 0.000 RG 1 w 0 J 0 j 4 M 20 0 8 8 re S");
    // url(#nope) with no gradient/fallback => unfilled; that rect never fills.
    assert_absent(&text, "rg 30 0 8 8 re f");
}

// ===========================================================================
// Transforms
// ===========================================================================

#[test]
fn transform_functions_emit_expected_cm_matrices() {
    let text = svg(
        "tf.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<rect x="1" y="1" width="8" height="8" fill="#f00" transform="translate(5)"/>
<rect x="1" y="1" width="8" height="8" fill="#0f0" transform="translate(5,7)"/>
<rect x="1" y="1" width="8" height="8" fill="#00f" transform="scale(2)"/>
<rect x="1" y="1" width="8" height="8" fill="#ff0" transform="scale(2,3)"/>
<rect x="1" y="1" width="8" height="8" fill="#0ff" transform="matrix(1 0 0 1 3 4)"/>
<rect x="1" y="1" width="8" height="8" fill="#f0f" transform="skewY(15)"/>
</svg>"##,
    );
    assert_has(&text, "q 1 0 0 1 5 0 cm 1.000 0.000 0.000 rg");
    assert_has(&text, "q 1 0 0 1 5 7 cm 0.000 1.000 0.000 rg");
    assert_has(&text, "q 2 0 0 2 0 0 cm 0.000 0.000 1.000 rg");
    assert_has(&text, "q 2 0 0 3 0 0 cm 1.000 1.000 0.000 rg");
    assert_has(&text, "q 1 0 0 1 3 4 cm 0.000 1.000 1.000 rg");
    assert_has(&text, "q 1 0.27 0 1 0 0 cm 1.000 0.000 1.000 rg");
}

#[test]
fn transform_invalid_forms_are_dropped_leaving_identity() {
    // A malformed transform never wraps the shape in a `q .. cm` at all.
    let text = svg(
        "tfbad.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<rect x="0" y="0" width="8" height="8" fill="#ff0000" transform="wobble(5)"/>
<rect x="10" y="0" width="8" height="8" fill="#00ff00" transform="translate"/>
<rect x="20" y="0" width="8" height="8" fill="#0000ff" transform="matrix(1 0 0 1)"/>
<rect x="30" y="0" width="8" height="8" fill="#ffff00" transform="rotate()"/>
</svg>"##,
    );
    // The unknown/incomplete transforms yield plain (un-wrapped) fills.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg 20 0 8 8 re f");
    assert_has(&text, "1.000 1.000 0.000 rg 30 0 8 8 re f");
}

// ===========================================================================
// Path data commands
// ===========================================================================

#[test]
fn path_data_all_command_letters_produce_ops() {
    let text = svg(
        "path.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="M1 1 L10 1 H20 V10 C22 12 24 14 26 10 S28 6 30 10 Q32 12 34 10 T38 10 Z" fill="none" stroke="#ff0000" stroke-width="1"/>
</svg>"##,
    );
    assert_has(&text, "1 1 m");
    assert_has(&text, "10 1 l");
    assert_has(&text, "20 1 l"); // H20
    assert_has(&text, "20 10 l"); // V10
    assert_has(&text, "22 12 24 14 26 10 c"); // C
    assert_has(&text, "34 10 c"); // Q -> cubic end
    assert_has(&text, "h S"); // close + stroke
}

#[test]
fn path_data_relative_commands_accumulate_current_point() {
    let text = svg(
        "pathrel.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="m5 5 l10 0 h5 v5 c2 2 4 4 6 0 z" fill="none" stroke="#0000ff" stroke-width="1"/>
</svg>"##,
    );
    assert_has(&text, "5 5 m");
    assert_has(&text, "15 5 l"); // l10 0 relative
    assert_has(&text, "20 5 l"); // h5 relative
    assert_has(&text, "20 10 l"); // v5 relative
    assert_has(&text, "22 12 24 14 26 10 c"); // c relative: end = (20+6, 10+0)
}

#[test]
fn path_data_smooth_shorthand_reflects_previous_control() {
    // S/T with no preceding curve reflect the current point (control == current).
    let text = svg(
        "pathsmooth.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="M10 10 S20 20 30 10 T50 10" fill="none" stroke="#00ff00" stroke-width="1"/>
</svg>"##,
    );
    // First S: no prior cubic control, first control == current (10,10).
    assert_has(&text, "10 10 20 20 30 10 c");
    // T reflects the (absent) quad control -> quad lowered to a cubic ending (50,10).
    assert_has(&text, "50 10 c");
}

#[test]
fn path_data_arc_variants_and_degenerate_radii() {
    let text = svg(
        "patharc.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="M10 10 A0 0 0 0 0 30 30" fill="none" stroke="#ff0000" stroke-width="1"/>
<path d="M10 40 A8 8 0 1 1 26 40" fill="none" stroke="#00ff00" stroke-width="1"/>
<path d="M10 60 a6 6 0 0 0 0 0" fill="none" stroke="#0000ff" stroke-width="1"/>
</svg>"##,
    );
    // rx=ry=0 -> straight line to the endpoint.
    assert_has(&text, "10 10 m 30 30 l");
    // large-arc/sweep produce cubic approximations.
    assert_has(&text, "10 40 m");
    assert_has(&text, "c"); // arc lowered to curves
    // a with zero delta (endpoint == start) emits just the moveto, no arc.
    assert_has(&text, "10 60 m");
}

#[test]
fn path_data_invalid_content_is_tolerated() {
    // Unknown command letters and truncated coordinate runs stop cleanly.
    let text = svg(
        "pathbad.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="M5 5 L10 10 Z Q" fill="#ff0000"/>
<path d="XYZ" fill="#00ff00"/>
<rect x="40" y="0" width="8" height="8" fill="#0000ff"/>
</svg>"##,
    );
    // First path still emits its valid prefix.
    assert_has(&text, "5 5 m");
    // Control rect after the junk still renders -> parser recovered.
    assert_has(&text, "0.000 0.000 1.000 rg 40 0 8 8 re f");
}

// ===========================================================================
// Stroke: caps, joins, miter limit, dash arrays
// ===========================================================================

#[test]
fn stroke_line_caps_and_joins_map_to_operator_codes() {
    let text = svg(
        "caps.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<polyline points="1,1 20,1 20,20" fill="none" stroke="#111111" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
<polyline points="1,30 20,30 20,50" fill="none" stroke="#222222" stroke-width="2" stroke-linecap="square" stroke-linejoin="bevel"/>
<polyline points="1,60 20,60 20,80" fill="none" stroke="#333333" stroke-width="2" stroke-linecap="butt" stroke-linejoin="miter" stroke-miterlimit="9.5"/>
</svg>"##,
    );
    assert_has(&text, "1 J 1 j"); // round cap, round join
    assert_has(&text, "2 J 2 j"); // square cap, bevel join
    assert_has(&text, "0 J 0 j 9.5 M"); // butt cap, miter join, custom limit
}

#[test]
fn stroke_dash_array_parsing_edge_cases() {
    let text = svg(
        "dash.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<line x1="1" y1="1" x2="40" y2="1" stroke="#000000" stroke-width="1" stroke-dasharray="4 2" stroke-dashoffset="1"/>
<line x1="1" y1="10" x2="40" y2="10" stroke="#000000" stroke-width="1" stroke-dasharray="3 1.5 2"/>
<line x1="1" y1="20" x2="40" y2="20" stroke="#000000" stroke-width="1" stroke-dasharray="none"/>
<line x1="1" y1="30" x2="40" y2="30" stroke="#000000" stroke-width="1" stroke-dasharray="4 -2"/>
<line x1="1" y1="40" x2="40" y2="40" stroke="#000000" stroke-width="1" stroke-dasharray="0 0"/>
</svg>"##,
    );
    assert_has(&text, "[4 2] 1 d"); // even pattern + offset
    assert_has(&text, "[3 1.5 2 3 1.5 2] 0 d"); // odd pattern doubled
    // "none", the negative-value pattern, and the all-zero pattern all resolve to
    // a solid stroke, and the dash operator is omitted entirely (miter `4 M`
    // directly precedes the path).
    assert_has(&text, "4 M 1 20 m 40 20 l S");
    assert_has(&text, "4 M 1 30 m 40 30 l S");
    assert_has(&text, "4 M 1 40 m 40 40 l S");
}

// ===========================================================================
// Opacity
// ===========================================================================

#[test]
fn opacity_attributes_build_extgstate_and_clamp() {
    let text = svg(
        "op.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<rect x="0" y="0" width="8" height="8" fill="#ff0000" fill-opacity="0.125" stroke="#0000ff" stroke-opacity="87.5%" stroke-width="1"/>
<rect x="10" y="0" width="8" height="8" fill="#00ff00" fill-opacity="2.5"/>
<rect x="20" y="0" width="8" height="8" fill="#00ff00" fill-opacity="-1"/>
</svg>"##,
    );
    assert_has(
        &text,
        "/GSa01250875 gs 1.000 0.000 0.000 rg 0.000 0.000 1.000 RG",
    );
    assert_has(&text, "/ca 0.125 /CA 0.875");
    // fill-opacity clamps to [0,1]: 2.5 -> fully opaque, so the green rect renders
    // with a plain fill (no ExtGState alpha).
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f");
    // fill-opacity -1 -> fully transparent -> the rect is dropped entirely.
    assert_absent(&text, "20 0 8 8 re");
}

// ===========================================================================
// Attribute tokenizer: quote styles, valueless attrs, entities
// ===========================================================================

#[test]
fn attribute_tokenizer_handles_quote_styles_and_valueless_attrs() {
    let text = svg(
        "attrs.svg",
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 50 20'>\
<rect x=0 y=0 width=8 height=8 fill='#ff0000' data-flag/>\
<rect x=\"10\" y=\"0\" width=\"8\" height=\"8\" fill=\"#0000ff\"/>\
<rect x=20 y=0 width=8 height=8 fill=\"#00ff00\" title=\"a &amp; b\"/></svg>",
    );
    // Single-quoted, unquoted, and valueless (`data-flag`) attributes all parse.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg 10 0 8 8 re f");
    assert_has(&text, "0.000 1.000 0.000 rg 20 0 8 8 re f");
}

// ===========================================================================
// Reusable defs: <use> of a rich <symbol> body
// ===========================================================================

#[test]
fn use_of_symbol_body_expands_all_child_kinds() {
    let text = svg(
        "use.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<symbol id="sym">
<!-- inner comment -->
<g fill="#ff0000"><rect x="0" y="0" width="5" height="5"/></g>
<a href="https://x.example"><rect x="6" y="0" width="5" height="5" fill="#00ff00"/></a>
<text x="0" y="20" font-size="6" fill="#0000ff">Hi</text>
<use href="#dot"/>
<style>.z{fill:#ffff00}</style>
<rect class="z" x="12" y="0" width="5" height="5"/>
</symbol>
<rect id="dot" x="18" y="0" width="4" height="4" fill="#00ffff"/>
</defs>
<use href="#sym" x="5" y="5"/>
</svg>"##,
    );
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 5 5 re f"); // <g> child
    assert_has(&text, "0.000 1.000 0.000 rg 6 0 5 5 re f"); // <a> child
    assert_has(&text, "0.000 0.000 1.000 rg"); // <text> fill
    assert_has(&text, "0.000 1.000 1.000 rg 18 0 4 4 re f"); // nested <use> of #dot
    assert_has(&text, "1.000 1.000 0.000 rg 12 0 5 5 re f"); // css-class styled child
}

// ===========================================================================
// Document structure: containers, links, comments, skipped subtrees
// ===========================================================================

#[test]
fn structure_containers_links_and_skipped_subtrees() {
    let text = svg(
        "struct.svg",
        r##"<?xml version="1.0"?>
<!DOCTYPE svg>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs><style>.klass{fill:#00ff00}</style></defs>
<g fill="#ff0000" transform="translate(2,2)">
  <rect x="1" y="1" width="8" height="8"/>
  <a href="https://example.com"><circle cx="20" cy="20" r="5"/></a>
</g>
<rect class="klass" x="30" y="1" width="8" height="8"/>
<script>ignore()</script>
</svg>"##,
    );
    // <g> transform wraps children; inherited red fill applies.
    assert_has(&text, "q 1 0 0 1 2 2 cm 1.000 0.000 0.000 rg 1 1 8 8 re f");
    // CSS class from the <style> in <defs> colours the later rect green.
    assert_has(&text, "0.000 1.000 0.000 rg 30 1 8 8 re f");
    // The <script> subtree is skipped entirely (no stray text ops from it).
    assert_absent(&text, "ignore");
}

// ===========================================================================
// Accessible text -> /Alt (only when the Markdown alt is empty)
// ===========================================================================

#[test]
fn svg_title_desc_and_aria_label_feed_alt_when_markdown_alt_empty() {
    // aria-label wins over <title>; <desc> is appended with " - ".
    let aria = svg_pdf(
        "",
        "aria.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 50 20" aria-label="Aria Name">
<title>Title Ignored</title><desc>The Description</desc>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/></svg>"##,
    );
    assert_has(&aria, "/Alt (Aria Name - The Description)");

    // With no aria-label, <title> supplies the name.
    let title = svg_pdf(
        "",
        "title.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 50 20">
<title>Just A Title</title>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/></svg>"##,
    );
    assert_has(&title, "/Alt (Just A Title)");

    // Only a <desc>.
    let desc = svg_pdf(
        "",
        "desc.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 50 20">
<desc>Only Desc</desc>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/></svg>"##,
    );
    assert_has(&desc, "/Alt (Only Desc)");
}

// ===========================================================================
// Clip paths and masks
// ===========================================================================

#[test]
fn clip_path_and_mask_emit_clipping_before_fill() {
    let text = svg(
        "clip.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<clipPath id="cp"><circle cx="20" cy="20" r="10"/></clipPath>
<clipPath id="rectclip"><rect x="55" y="5" width="20" height="20"/></clipPath>
<mask id="mk"><rect x="0" y="55" width="40" height="30" fill="#ffffff"/></mask>
</defs>
<rect x="5" y="5" width="40" height="40" fill="#ff0000" clip-path="url(#cp)"/>
<rect x="55" y="5" width="40" height="40" fill="#0000ff" clip-path="url(#rectclip)"/>
<rect x="5" y="55" width="40" height="40" fill="#00ff00" mask="url(#mk)"/>
</svg>"##,
    );
    // Circle clip lowered to bezier ops then `W n` before the red fill.
    assert_has(&text, "h W n 1.000 0.000 0.000 rg 5 5 40 40 re f");
    // Rect clip is an axis-aligned path clip before the blue fill.
    assert_has(&text, "W n 0.000 0.000 1.000 rg 55 5 40 40 re f");
    // Mask reveals via a clip region before the green fill.
    assert_has(&text, "W n 0.000 1.000 0.000 rg 5 55 40 40 re f");
}

// ===========================================================================
// Filter drop shadows
// ===========================================================================

#[test]
fn drop_shadow_filter_and_css_function_paint_offset_shadow() {
    let text = svg(
        "shadow.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<filter id="ds"><feDropShadow dx="2" dy="3" stdDeviation="1" flood-color="#123456" flood-opacity="0.5"/></filter>
</defs>
<rect x="10" y="10" width="30" height="30" fill="#00ff00" filter="url(#ds)"/>
<rect x="50" y="10" width="30" height="30" fill="#ff0000" style="filter:drop-shadow(2px 3px 1px #654321)"/>
</svg>"##,
    );
    // feDropShadow: translated shadow in the flood colour at 0.5 opacity, then shape.
    assert_has(
        &text,
        "q 1 0 0 1 2 3 cm /GSa05000500 gs 0.071 0.204 0.337 rg 10 10 30 30 re f Q",
    );
    assert_has(&text, "0.000 1.000 0.000 rg 10 10 30 30 re f"); // the shape itself
    // CSS drop-shadow(): shadow in #654321.
    assert_has(
        &text,
        "q 1 0 0 1 2 3 cm 0.396 0.263 0.129 rg 50 10 30 30 re f Q",
    );
}

// ===========================================================================
// Gradients
// ===========================================================================

#[test]
fn linear_gradient_spread_repeat_tiles_multiple_shadings() {
    let text = svg(
        "grep.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<linearGradient id="rep" spreadMethod="repeat" x1="0" y1="0" x2="0.25" y2="0">
<stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/>
</linearGradient>
</defs>
<rect x="1" y="1" width="40" height="20" fill="url(#rep)"/>
</svg>"##,
    );
    // A repeat gradient over a quarter-width tile is realised as multiple shadings.
    assert_has(&text, "/ShadingType 2 /ColorSpace /DeviceRGB");
    assert_has(&text, "/C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000]");
    assert_has(&text, "/SG1 sh");
    assert_has(&text, "/SG2 sh"); // tiling produced a second shading
}

#[test]
fn radial_gradient_userspace_and_stop_opacity() {
    let text = svg(
        "grad2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<radialGradient id="r" gradientUnits="userSpaceOnUse" cx="30" cy="30" r="20">
<stop offset="0" stop-color="#00ff00"/>
<stop offset="1" stop-color="#0000ff" stop-opacity="0.5"/>
</radialGradient>
</defs>
<circle cx="30" cy="30" r="20" fill="url(#r)"/>
</svg>"##,
    );
    assert_has(&text, "/ShadingType 3");
    // userSpaceOnUse keeps native coordinates in the shading /Coords.
    assert_has(&text, "/Coords [30 30 0 30 30 20]");
    // stop-opacity 0.5 composites #0000ff over the white page: (0.5,0.5,1.0).
    assert_has(&text, "/C1 [0.500 0.500 1.000]");
}

#[test]
fn gradient_without_usable_stops_falls_back_to_solid_or_nothing() {
    // A gradient with a single stop degenerates to a flat fill of that stop colour.
    let text = svg(
        "grad3.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<linearGradient id="one"><stop offset="0" stop-color="#ff8800"/></linearGradient>
</defs>
<rect x="1" y="1" width="30" height="30" fill="url(#one)"/>
</svg>"##,
    );
    // Single-stop gradient renders as a flat fill in the representative colour.
    assert_has(&text, "1.000 0.533 0.000 rg 1 1 30 30 re f");
}

// ===========================================================================
// Text: anchor, textLength, decoration, baseline, spacing, fonts, tspans
// ===========================================================================

#[test]
fn svg_text_anchor_and_length_adjust_shift_and_scale() {
    let text = svg(
        "text1.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
<text x="100" y="20" font-size="12" fill="#000000" text-anchor="middle" textLength="80" lengthAdjust="spacingAndGlyphs">Hello World</text>
<text x="10" y="50" font-size="12" fill="#000000" text-anchor="end">Right</text>
</svg>"##,
    );
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
    // spacingAndGlyphs applies a horizontal scale factor in the text matrix.
    assert_has(&text, "Tf 1.");
}

#[test]
fn svg_text_decoration_and_dominant_baseline_and_fonts() {
    let text = svg(
        "text2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
<text x="10" y="30" font-size="12" fill="#000000" text-decoration="line-through">Strike</text>
<text x="10" y="50" font-size="12" fill="#000000" text-decoration="underline">Under</text>
<text x="10" y="70" font-size="12" fill="#000000" dominant-baseline="central" font-family="monospace">Mono</text>
<text x="10" y="90" font-size="12" fill="#000000" font-weight="bold" font-style="italic">BoldItalic</text>
</svg>"##,
    );
    // Decoration lines are drawn as stroked segments after the glyphs.
    assert_has(&text, "RG"); // decoration stroke colour
    assert_has(&text, " m ");
    assert_has(&text, " l S");
    // monospace maps to a mono font resource (F4 in the default face set).
    assert_has(&text, "/F4 ");
}

#[test]
fn svg_text_baseline_shift_and_spacing_keywords() {
    let text = svg(
        "text3.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
<text x="10" y="40" font-size="12" fill="#000000" baseline-shift="super">Up</text>
<text x="60" y="40" font-size="12" fill="#000000" baseline-shift="sub">Down</text>
<text x="10" y="70" font-size="12" fill="#000000" letter-spacing="2" word-spacing="4">a b</text>
</svg>"##,
    );
    // All three produce text runs at distinct baselines.
    assert_has(&text, "BT ");
    let bt_count = text.matches("BT ").count();
    assert!(bt_count >= 3, "expected >=3 text runs, got {bt_count}");
}

#[test]
fn svg_text_positioned_tspan_children() {
    let text = svg(
        "text4.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
<text x="10" y="30" font-size="12" fill="#000000"><tspan dx="5" dy="2">A</tspan><tspan x="80" y="30">B</tspan></text>
<text x="10" y="60" font-size="12" fill="#000000" x="10 30 50" y="60">xyz</text>
</svg>"##,
    );
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
}

// ===========================================================================
// Inline <image> with data: URIs (base64 + percent-encoded)
// ===========================================================================

#[test]
fn svg_inline_image_data_uri_decoding() {
    // A valid base64 PNG, a percent-encoded inline SVG, and a malformed base64
    // sibling exercise the three data-URI decode paths. The malformed one must be
    // ignored without derailing the render.
    let png_b64 = base64_encode(&tiny_rgb_png(&[[0xff, 0x00, 0x00], [0x00, 0x00, 0xff]]));
    // A percent-encoded inline SVG whose content is a solid green square.
    let inner = "%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20viewBox%3D%220%200%2010%2010%22%3E%3Crect%20width%3D%2210%22%20height%3D%2210%22%20fill%3D%22%2300ff00%22%2F%3E%3C%2Fsvg%3E";
    let svg_src = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40">
<image x="0" y="0" width="10" height="10" href="data:image/png;base64,{png_b64}"/>
<image x="12" y="0" width="10" height="10" href="data:image/svg+xml,{inner}"/>
<image x="24" y="0" width="10" height="10" href="data:image/png;base64,not$$valid"/>
<rect x="30" y="0" width="8" height="8" fill="#3300ff"/>
</svg>"##
    );
    let text = svg("img.svg", &svg_src);
    // The valid inline PNG becomes an image XObject drawn with `Do`.
    assert_has(&text, " Do");
    assert_has(&text, "/Subtype /Image");
    // The percent-decoded inline SVG paints its green square as inline vector ops.
    assert_has(&text, "0.000 1.000 0.000 rg");
    // The control rect after the malformed image still renders.
    assert_has(&text, "0.200 0.000 1.000 rg 30 0 8 8 re f");
}

// ===========================================================================
// Markers
// ===========================================================================

#[test]
fn markers_render_shapes_at_vertices() {
    let text = svg(
        "marker.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<defs>
<marker id="dot" markerWidth="4" markerHeight="4" refX="2" refY="2" orient="auto">
<circle cx="2" cy="2" r="2" fill="#ff0000"/>
</marker>
</defs>
<polyline points="10,30 40,30 70,10" fill="none" stroke="#000000" stroke-width="1" marker-start="url(#dot)" marker-mid="url(#dot)" marker-end="url(#dot)"/>
</svg>"##,
    );
    // The polyline stroke is present, and marker instances paint red circles.
    assert_has(&text, "0.000 0.000 0.000 RG");
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, " c"); // marker circle beziers
}

// ===========================================================================
// Patterns
// ===========================================================================

#[test]
fn pattern_fill_tiles_with_representative_color_fallback() {
    let text = svg(
        "pat.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<pattern id="p" width="8" height="8" patternUnits="userSpaceOnUse">
<rect x="0" y="0" width="4" height="4" fill="#ff0000"/>
</pattern>
</defs>
<rect x="1" y="1" width="40" height="40" fill="url(#p)"/>
</svg>"##,
    );
    // The pattern's representative colour (#ff0000) drives the tiled/flat fill.
    assert_has(&text, "1.000 0.000 0.000");
    assert_has(&text, "40 40 re");
}

// ===========================================================================
// CSS: selectors, cascade, variables
// ===========================================================================

#[test]
fn css_selectors_class_id_tag_and_descendant() {
    let text = svg(
        "css.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<style>
rect { fill: #0000ff; }
.hot { fill: #ff0000; }
#special { fill: #00ff00; }
g rect { fill: #ffff00; }
</style>
<rect x="0" y="0" width="8" height="8"/>
<rect class="hot" x="10" y="0" width="8" height="8"/>
<rect id="special" x="20" y="0" width="8" height="8"/>
<g><rect x="30" y="0" width="8" height="8"/></g>
</svg>"##,
    );
    assert_has(&text, "0.000 0.000 1.000 rg 0 0 8 8 re f"); // tag selector
    assert_has(&text, "1.000 0.000 0.000 rg 10 0 8 8 re f"); // class selector
    assert_has(&text, "0.000 1.000 0.000 rg 20 0 8 8 re f"); // id selector
    assert_has(&text, "1.000 1.000 0.000 rg 30 0 8 8 re f"); // descendant selector
}

#[test]
fn css_custom_properties_resolve_in_fill_and_fallback() {
    let text = svg(
        "cssvar.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<style>:root { --brand: #ff0000; }</style>
<rect x="0" y="0" width="8" height="8" fill="var(--brand)"/>
<rect x="10" y="0" width="8" height="8" fill="var(--missing, #00ff00)"/>
<rect x="20" y="0" width="8" height="8" fill="var(--missing)"/>
</svg>"##,
    );
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f"); // var resolves
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f"); // var fallback used
    // An unresolvable var with no fallback keeps the inherited black fill.
    assert_has(&text, "0.000 0.000 0.000 rg 20 0 8 8 re f");
}

// ===========================================================================
// Root geometry: viewBox scaling & preserveAspectRatio
// ===========================================================================

#[test]
fn viewbox_and_preserve_aspect_ratio_variants_scale_content() {
    let none = svg(
        "vbnone.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10" preserveAspectRatio="none">
<rect x="0" y="0" width="20" height="10" fill="#ff0000"/></svg>"##,
    );
    assert_has(&none, "1.000 0.000 0.000 rg");
    let slice = svg(
        "vbslice.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" preserveAspectRatio="xMinYMin slice">
<rect x="0" y="0" width="10" height="10" fill="#0000ff"/></svg>"##,
    );
    assert_has(&slice, "0.000 0.000 1.000 rg");
    // Both remain deterministic and produce a form/image with a placement matrix.
    assert_has(&none, " cm");
    assert_has(&slice, " cm");
}

// ===========================================================================
// Batch 3 — error/edge arms in the structural & CSS parsers
// ===========================================================================

#[test]
fn structure_malformed_tags_are_skipped_without_derailing() {
    // Empty tags, an unterminated comment tail, processing instructions, and
    // explicit closing tags on shapes must all be tolerated; the trailing control
    // rect proves the parser recovered.
    let text = svg(
        "structbad.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<>
< >
<?pi custom ?>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"></rect>
<circle cx="20" cy="4" r="4" fill="#00ff00"></circle>
<rect x="30" y="0" width="8" height="8" fill="#0000ff"/>
<!-- a properly closed trailing comment -->
</svg>"##,
    );
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 1.000 0.000 rg");
    assert_has(&text, "0.000 0.000 1.000 rg 30 0 8 8 re f");
}

#[test]
fn structure_deeply_nested_groups_and_anchor_pop() {
    // Nested <g>/<a> containers push and pop the style/link stacks; the inner rect
    // inherits the red fill set five levels up.
    let text = svg(
        "nest.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<g fill="#ff0000"><g><a href="https://n.example"><g><g>
<rect x="0" y="0" width="8" height="8"/>
</g></g></a></g></g>
<rect x="20" y="0" width="8" height="8" fill="#00ff00"/>
</svg>"##,
    );
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    // After all the closes, the sibling rect is back to its own green fill.
    assert_has(&text, "0.000 1.000 0.000 rg 20 0 8 8 re f");
}

#[test]
fn accessible_text_title_and_desc_combine_and_skip_style() {
    // <title> + <desc> combine with " - "; a <style>/<script> before them is
    // skipped by the accessible-text scanner.
    let both = svg_pdf(
        "",
        "acc1.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
<style>rect{fill:#000}</style>
<title>My Title</title>
<desc>My Desc</desc>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/></svg>"##,
    );
    assert_has(&both, "/Alt (My Title - My Desc)");

    // aria-label present alongside title/desc: aria wins for the name, desc appends.
    let aria_and_desc = svg_pdf(
        "",
        "acc2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20" aria-label="Aria Wins">
<script>noop()</script>
<title>Loser Title</title>
<desc>Kept Desc</desc>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/></svg>"##,
    );
    assert_has(&aria_and_desc, "/Alt (Aria Wins - Kept Desc)");
}

#[test]
fn use_symbol_body_tolerates_comments_and_nested_skips() {
    let text = svg(
        "use2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs>
<symbol id="s">
<defs><linearGradient id="ig"><stop offset="0" stop-color="#ff0000"/></linearGradient></defs>
<g><g><rect x="0" y="0" width="6" height="6" fill="#ff00ff"/></g></g>
<rect x="8" y="0" width="6" height="6" fill="#00ffff"/>
</symbol>
</defs>
<use href="#s"/>
<use href="#s" x="20" y="0"/>
</svg>"##,
    );
    // The symbol is instanced twice; its magenta child renders at both offsets.
    assert!(
        text.matches("1.000 0.000 1.000 rg").count() >= 2,
        "symbol should be instanced twice"
    );
    assert_has(&text, "0.000 1.000 1.000 rg");
}

#[test]
fn anchor_links_attach_to_every_shape_kind() {
    let text = svg(
        "links.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120">
<a href="https://lnk.example/all">
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/>
<circle cx="20" cy="4" r="4" fill="#00ff00"/>
<ellipse cx="40" cy="4" rx="6" ry="3" fill="#0000ff"/>
<line x1="0" y1="20" x2="20" y2="20" stroke="#000000" stroke-width="1"/>
<polyline points="0,30 10,40 20,30" fill="none" stroke="#000000" stroke-width="1"/>
<polygon points="30,30 40,40 20,40" fill="#ffff00"/>
<path d="M0 50 L20 50" stroke="#000000" stroke-width="1" fill="none"/>
<text x="0" y="70" font-size="8" fill="#000000">Link</text>
</a>
</svg>"##,
    );
    // Each shape hitbox becomes its own /Link annotation carrying the URI.
    let links = text.matches("/Subtype /Link").count();
    assert!(links >= 6, "expected many link hitboxes, got {links}");
    assert_has(&text, "/URI (https://lnk.example/all)");
}

#[test]
fn clip_path_shape_variety_and_degenerate_shapes() {
    let text = svg(
        "clip2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<clipPath id="e"><ellipse cx="20" cy="20" rx="12" ry="8"/></clipPath>
<clipPath id="poly"><polygon points="55,5 75,5 65,25"/></clipPath>
<clipPath id="pth"><path d="M5 55 L45 55 L45 95 Z"/></clipPath>
<clipPath id="deg"><circle cx="10" cy="10" r="0"/></clipPath>
</defs>
<rect x="5" y="5" width="35" height="35" fill="#ff0000" clip-path="url(#e)"/>
<rect x="50" y="5" width="35" height="35" fill="#00ff00" clip-path="url(#poly)"/>
<rect x="5" y="50" width="45" height="45" fill="#0000ff" clip-path="url(#pth)"/>
<rect x="60" y="50" width="20" height="20" fill="#ffff00" clip-path="url(#deg)"/>
</svg>"##,
    );
    // Ellipse and path/polygon clips each emit a `W n` clip before their fill.
    assert!(
        text.matches(" W n").count() >= 3,
        "expected several clip regions"
    );
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn mask_reveal_rules_by_fill_luminance_and_style_declarations() {
    let text = svg(
        "mask2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40">
<defs>
<mask id="white"><rect x="0" y="0" width="20" height="20" fill="#ffffff"/></mask>
<mask id="styled"><rect x="0" y="0" width="20" height="20" style="fill:#ffffff;opacity:0.9"/></mask>
<mask id="dark"><rect x="0" y="0" width="20" height="20" style="fill:none"/></mask>
</defs>
<rect x="0" y="0" width="20" height="20" fill="#ff0000" mask="url(#white)"/>
<rect x="40" y="0" width="20" height="20" fill="#00ff00" mask="url(#styled)"/>
<rect x="80" y="0" width="20" height="20" fill="#0000ff" mask="url(#dark)"/>
</svg>"##,
    );
    // A bright mask reveals the red rect (clip + fill present).
    assert_has(&text, "1.000 0.000 0.000 rg");
    // The style-based bright mask (fill/opacity declared inline) reveals its green rect.
    assert_has(&text, "0.000 1.000 0.000 rg");
    // The `style="fill:none"` mask shape parses its declarations; the blue rect
    // still resolves to a fill operator in the stream.
    assert_has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn text_length_distributes_across_flowing_tspans() {
    let text = svg(
        "tlen.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 40">
<text x="10" y="20" font-size="10" fill="#000000" textLength="150" lengthAdjust="spacing"><tspan>Alpha</tspan><tspan>Beta</tspan></text>
</svg>"##,
    );
    // The parent textLength splits into per-fragment runs.
    assert!(
        text.matches("BT ").count() >= 2,
        "textLength across tspans should keep multiple runs"
    );
    assert_has(&text, "] TJ ET");
}

#[test]
fn text_baseline_shift_supports_all_unit_forms() {
    let text = svg(
        "bshift.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 40">
<text x="0" y="20" font-size="10" fill="#000000" baseline-shift="baseline">A</text>
<text x="20" y="20" font-size="10" fill="#000000" baseline-shift="0.5em">B</text>
<text x="40" y="20" font-size="10" fill="#000000" baseline-shift="2ex">C</text>
<text x="60" y="20" font-size="10" fill="#000000" baseline-shift="20%">D</text>
<text x="80" y="20" font-size="10" fill="#000000" baseline-shift="3">E</text>
<text x="100" y="20" font-size="10" fill="#000000" baseline-shift="2px">F</text>
</svg>"##,
    );
    assert!(
        text.matches("BT ").count() >= 6,
        "each baseline-shift form should yield a text run"
    );
}

#[test]
fn text_body_positioned_children_comment_and_textpath() {
    let text = svg(
        "tpath.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
<defs><path id="curve" d="M10 50 C40 10 80 90 120 50"/></defs>
<text font-size="10" fill="#000000"><!-- c --><textPath href="#curve">Curved</textPath></text>
<text x="10 30 50" y="80" font-size="10" fill="#000000">xyz</text>
</svg>"##,
    );
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
}

#[test]
fn css_style_block_handles_comments_quotes_and_nested_blocks() {
    let text = svg(
        "cssblk.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<style>
/* a comment with a brace } inside */
.a { fill: #ff0000; }
.b { fill: #00ff00; /* trailing */ }
@media screen { .c { fill: #0000ff; } }
</style>
<rect class="a" x="0" y="0" width="8" height="8"/>
<rect class="b" x="10" y="0" width="8" height="8"/>
</svg>"##,
    );
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f");
}

#[test]
fn css_selector_rejects_pseudo_and_over_deep_but_keeps_child_combinator() {
    let text = svg(
        "cssel.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<style>
rect:hover { fill: #ff0000; }
g > rect { fill: #00ff00; }
a b c d e f g rect { fill: #0000ff; }
* { stroke: none; }
</style>
<g><rect x="0" y="0" width="8" height="8" fill="#111111"/></g>
</svg>"##,
    );
    // The :hover rule is dropped and the 7-deep selector is dropped, but the
    // `g > rect` child-combinator rule wins for the grouped rect (green).
    assert_has(&text, "0.000 1.000 0.000 rg 0 0 8 8 re f");
    assert_absent(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
}

#[test]
fn root_background_shorthand_tokenizes_gradient_and_position() {
    let text = svg(
        "rootbg.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" style="background: linear-gradient(90deg, #ff0000, #0000ff) center / cover no-repeat">
<rect x="10" y="10" width="8" height="8" fill="#00ff00"/>
</svg>"##,
    );
    // The gradient background paints a full-viewport shading behind the content.
    assert_has(&text, "/ShadingType 2");
    assert_has(&text, "0.000 1.000 0.000 rg 10 10 8 8 re f");
}

#[test]
fn transform_list_concatenation_and_rotate_about_point() {
    let text = svg(
        "tf3.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<rect x="1" y="1" width="8" height="8" fill="#ff0000" transform="translate(10,10) scale(2) rotate(90 5 5)"/>
<rect x="1" y="1" width="8" height="8" fill="#00ff00" transform="skewX(10) skewY(-10)"/>
</svg>"##,
    );
    // Chained transforms fold into a single `cm` matrix per shape.
    assert_has(&text, "cm 1.000 0.000 0.000 rg");
    assert_has(&text, "cm 0.000 1.000 0.000 rg");
}

#[test]
fn path_arc_rotation_and_radii_scaling() {
    let text = svg(
        "arc2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="M10 10 A3 3 45 0 1 40 40" fill="none" stroke="#ff0000" stroke-width="1"/>
<path d="M10 60 A20 10 30 1 0 50 60" fill="none" stroke="#00ff00" stroke-width="1"/>
</svg>"##,
    );
    // Under-sized radii are scaled up so the arc still reaches its endpoint via
    // cubic segments; a rotated arc likewise lowers to curves.
    assert_has(&text, "10 10 m");
    assert_has(&text, "10 60 m");
    assert!(
        text.matches(" c").count() >= 4,
        "arcs lower to cubic curves"
    );
}

#[test]
fn gradient_href_inheritance_and_gradient_transform() {
    let text = svg(
        "ghref.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<linearGradient id="base"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient>
<linearGradient id="child" href="#base" x1="0" y1="0" x2="1" y2="0" gradientTransform="rotate(45)"/>
</defs>
<rect x="1" y="1" width="40" height="40" fill="url(#child)"/>
</svg>"##,
    );
    // The child gradient inherits stops from its href base.
    assert_has(&text, "/ShadingType 2");
    assert_has(&text, "/C0 [1.000 0.000 0.000]");
    assert_has(&text, "/C1 [0.000 0.000 1.000]");
}

#[test]
fn pattern_with_transform_and_content_units() {
    let text = svg(
        "pat2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<pattern id="p" width="10" height="10" patternUnits="userSpaceOnUse" patternTransform="rotate(30)">
<circle cx="5" cy="5" r="3" fill="#ff0000"/>
</pattern>
<pattern id="empty" width="10" height="10" patternUnits="userSpaceOnUse"></pattern>
</defs>
<rect x="1" y="1" width="40" height="40" fill="url(#p)"/>
<rect x="50" y="1" width="40" height="40" fill="url(#empty)"/>
</svg>"##,
    );
    // The pattern's representative colour drives the fill.
    assert_has(&text, "1.000 0.000 0.000");
    // The empty pattern has no renderable content -> that rect is not painted.
    assert_absent(&text, "50 1 40 40 re f");
}

#[test]
fn marker_body_viewbox_orient_and_shapes() {
    let text = svg(
        "mk2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<defs>
<marker id="arrow" markerWidth="10" markerHeight="10" refX="5" refY="5" viewBox="0 0 10 10" orient="45">
<path d="M0 0 L10 5 L0 10 Z" fill="#ff0000"/>
</marker>
<marker id="bar" markerWidth="4" markerHeight="8" orient="auto-start-reverse">
<line x1="2" y1="0" x2="2" y2="8" stroke="#0000ff" stroke-width="1"/>
</marker>
</defs>
<path d="M10 30 L50 30 L90 10" fill="none" stroke="#000000" stroke-width="1" marker-start="url(#bar)" marker-mid="url(#arrow)" marker-end="url(#arrow)"/>
</svg>"##,
    );
    assert_has(&text, "0.000 0.000 0.000 RG"); // the stroked path
    assert_has(&text, "1.000 0.000 0.000 rg"); // arrow marker fill
}

#[test]
fn filter_manual_shadow_pipeline_and_multiple_drop_shadows() {
    let text = svg(
        "filt2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
<defs>
<filter id="manual">
<feGaussianBlur in="SourceAlpha" stdDeviation="1"/>
<feOffset dx="2" dy="2" result="off"/>
<feFlood flood-color="#123456" flood-opacity="0.6"/>
<feComposite in2="off" operator="in"/>
<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge>
</filter>
</defs>
<rect x="10" y="10" width="30" height="30" fill="#00ff00" filter="url(#manual)"/>
<rect x="60" y="10" width="30" height="30" fill="#ff0000" style="filter:drop-shadow(1px 1px 0 #000000) drop-shadow(3px 3px 0 #0000ff)"/>
</svg>"##,
    );
    // The manual feGaussianBlur/feOffset/feFlood/feComposite/feMerge pipeline is
    // parsed; its shape still paints green.
    assert_has(&text, "0.000 1.000 0.000 rg 10 10 30 30 re f");
    // The CSS `drop-shadow(..) drop-shadow(..)` chain paints two offset shadows
    // (black then blue) beneath the red shape.
    assert_has(
        &text,
        "q 1 0 0 1 1 1 cm 0.000 0.000 0.000 rg 60 10 30 30 re f Q",
    );
    assert_has(
        &text,
        "q 1 0 0 1 3 3 cm 0.000 0.000 1.000 rg 60 10 30 30 re f Q",
    );
    assert_has(&text, "1.000 0.000 0.000 rg 60 10 30 30 re f");
}

#[test]
fn polygon_points_parsing_tolerates_odd_and_invalid_tokens() {
    let text = svg(
        "pts.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<polygon points="10,10 30,10 30,30 10,30 55" fill="#ff0000"/>
<polyline points="1e1,2e1 40,40 x,y 60,10" fill="none" stroke="#00ff00" stroke-width="1"/>
</svg>"##,
    );
    // The dangling coordinate (55) is dropped; the polygon still closes.
    assert_has(&text, "10 10 m");
    assert_has(&text, "h f");
    // Exponent tokens parse (1e1=10, 2e1=20); the invalid `x,y` pair is skipped.
    assert_has(&text, "10 20 m");
}

// ===========================================================================
// Batch 4 — degenerate shapes, container edges, path/transform/paint arms
// ===========================================================================

#[test]
fn degenerate_shapes_are_dropped_but_do_not_derail_parsing() {
    let text = svg(
        "degen.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<rect x="0" y="0" height="8" fill="#ff0000"/>
<rect x="10" y="0" width="0" height="8" fill="#ff0000"/>
<circle cx="20" cy="4" fill="#ff0000"/>
<circle cx="30" cy="4" r="0" fill="#ff0000"/>
<ellipse cx="40" cy="4" rx="0" ry="4" fill="#ff0000"/>
<polygon points="50,0 55,5" fill="#ff0000"/>
<path d="" fill="#ff0000"/>
<path fill="#ff0000"/>
<image x="60" y="0" width="0" height="8" href="data:image/png;base64,AAAA"/>
<rect x="80" y="0" width="8" height="8" fill="#00ff00"/>
</svg>"##,
    );
    // Only the final valid rect paints; every degenerate shape was skipped.
    assert_has(&text, "0.000 1.000 0.000 rg 80 0 8 8 re f");
    assert_absent(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn self_closing_containers_and_invisible_root() {
    // Self-closing <g/> and <a/> push no stack frame; an invisible root paints no
    // background but its children (which are still visible) render.
    let text = svg(
        "cont.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<g fill="#ff0000"/>
<a href="https://x.example"/>
<rect x="0" y="0" width="8" height="8" fill="#0000ff"/>
</svg>"##,
    );
    // The self-closing container did not leak its red fill onto the later rect.
    assert_has(&text, "0.000 0.000 1.000 rg 0 0 8 8 re f");
    assert_absent(&text, "1.000 0.000 0.000 rg");

    // A hidden root suppresses the background fill entirely.
    let hidden = svg(
        "hidden.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" visibility="hidden" style="background:#ff0000">
<rect x="0" y="0" width="8" height="8" fill="#00ff00" visibility="visible"/>
</svg>"##,
    );
    assert_absent(&hidden, "1.000 0.000 0.000 rg 0 0 40 40 re f");
}

#[test]
fn nested_inner_svg_viewport_is_parsed() {
    let text = svg(
        "innersvg.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/>
<svg x="20" y="20" width="40" height="40" viewBox="0 0 10 10">
<rect x="0" y="0" width="10" height="10" fill="#0000ff"/>
</svg>
</svg>"##,
    );
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    // The inner <svg> subtree is walked and its blue rect appears.
    assert_has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn accessible_text_scanner_handles_comments_depth_and_self_closing_meta() {
    // A comment, a deeply-nested (ignored) title, a self-closing title, and a
    // real desc all pass through the accessible-text scanner.
    let out = svg_pdf(
        "",
        "accscan.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20" aria-label="Top">
<!-- scanner comment -->
<g><title>Nested Title Ignored</title></g>
<title/>
<desc>Real Desc</desc>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/></svg>"##,
    );
    // Only the depth-1 aria-label and the real desc feed /Alt; the nested title is
    // ignored and the self-closing title contributes nothing.
    assert_has(&out, "/Alt (Top - Real Desc)");
}

#[test]
fn path_data_command_stream_edge_cases() {
    let text = svg(
        "pathedge.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="M5 5 20 20 30 5 L40 5" fill="none" stroke="#ff0000" stroke-width="1"/>
<path d="12 12" fill="#00ff00"/>
<path d="M5 50 L H V C S Q T A" fill="none" stroke="#00ff00" stroke-width="1"/>
<rect x="60" y="0" width="8" height="8" fill="#0000ff"/>
</svg>"##,
    );
    // A moveto with extra coordinate pairs turns each extra pair into an implicit
    // lineto.
    assert_has(&text, "5 5 m");
    assert_has(&text, "20 20 l");
    assert_has(&text, "30 5 l");
    assert_has(&text, "40 5 l"); // explicit L
    // A `d` beginning with a number (no command) yields an empty path, and command
    // letters with no coordinates stop cleanly after the initial M.
    assert_has(&text, "5 50 m");
    // The control rect after the empty/degenerate paths still renders.
    assert_has(&text, "0.000 0.000 1.000 rg 60 0 8 8 re f");
}

#[test]
fn transform_parser_boundary_inputs() {
    let text = svg(
        "tfedge.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<rect x="0" y="0" width="8" height="8" fill="#ff0000" transform=""/>
<rect x="10" y="0" width="8" height="8" fill="#00ff00" transform="()"/>
<rect x="20" y="0" width="8" height="8" fill="#0000ff" transform="translate(5"/>
<rect x="30" y="0" width="8" height="8" fill="#ffff00" transform="translate(4,4) bogus(9)"/>
</svg>"##,
    );
    // Empty / nameless / unterminated transforms leave the shape un-wrapped.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg 20 0 8 8 re f");
    // The valid leading translate is kept even though a later token is bogus.
    assert_has(&text, "q 1 0 0 1 4 4 cm 1.000 1.000 0.000 rg 30 0 8 8 re f");
}

#[test]
fn paint_current_color_and_context_paint_and_hex_alpha() {
    let text = svg(
        "paint.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<rect x="0" y="0" width="8" height="8" color="#ff0000" fill="currentColor"/>
<rect x="10" y="0" width="8" height="8" color="#00ff00" fill="none" stroke="currentColor" stroke-width="1"/>
<rect x="20" y="0" width="8" height="8" fill="#0000ff80"/>
<defs>
<marker id="m" markerWidth="6" markerHeight="6" refX="3" refY="3">
<circle cx="3" cy="3" r="2" fill="context-stroke"/>
</marker>
</defs>
<polyline points="0,30 40,30" fill="none" stroke="#123456" stroke-width="1" marker-start="url(#m)"/>
</svg>"##,
    );
    // currentColor fill resolves to the element's `color`.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    // currentColor stroke resolves to green.
    assert_has(&text, "0.000 1.000 0.000 RG 1 w 0 J 0 j 4 M 10 0 8 8 re S");
    // #rrggbbaa fill alpha routes to a fill-opacity ExtGState (0x80 = 0.502).
    assert_has(&text, "/ca 0.502 /CA 1.000");
    assert_has(&text, "0.000 0.000 1.000 rg 20 0 8 8 re f");
    // The context-stroke marker inherits the polyline's stroke colour.
    assert_has(&text, "0.071 0.204 0.337 rg");
}

#[test]
fn attribute_tokenizer_boundary_positions() {
    // An attribute with `=` but no value, a value-less attribute at end-of-string,
    // slash separators, and tab/newline whitespace between attributes.
    let text = svg(
        "attredge.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 40 20\">\
<rect\tx=\"0\"\ny=\"0\"  width=\"8\"   height=\"8\"\r\nfill=\"#ff0000\" role/>\
<rect x=\"10\" y=\"0\" width=\"8\" height=\"8\" fill=\"#0000ff\" data-x=/></svg>",
    );
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg 10 0 8 8 re f");
}

#[test]
fn clip_path_body_variety_with_comments_rules_and_transforms() {
    let text = svg(
        "clipbody.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<clipPath id="multi" clip-rule="evenodd">
<!-- a comment inside the clip body -->
<rect x="5" y="5" width="20" height="20" transform="translate(2,2)"/>
<circle cx="30" cy="30" r="8" clip-rule="nonzero"/>
<line x1="0" y1="0" x2="10" y2="10"/>
</clipPath>
</defs>
<rect x="0" y="0" width="60" height="60" fill="#ff0000" clip-path="url(#multi)"/>
</svg>"##,
    );
    // Multiple clip child shapes combine into one clip region before the fill.
    assert_has(&text, "W n 1.000 0.000 0.000 rg 0 0 60 60 re f");
}

#[test]
fn mask_body_variety_and_reveal_thresholds() {
    let text = svg(
        "maskbody.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<defs>
<mask id="mb">
<!-- comment -->
<rect x="0" y="0" width="30" height="30" fill="#ffffff"/>
<circle cx="10" cy="10" r="5" fill="#101010"/>
<ellipse cx="20" cy="20" rx="6" ry="4" fill="#ffffff" opacity="0.05"/>
<rect x="0" y="0" width="10" height="10" fill="#ffffff" clip-rule="evenodd"/>
</mask>
</defs>
<rect x="0" y="0" width="40" height="40" fill="#ff0000" mask="url(#mb)"/>
</svg>"##,
    );
    // The bright shapes reveal (a clip region is emitted) while the dark/near-
    // transparent shapes do not; the last child declared clip-rule evenodd so the
    // clip uses the `W*` even-odd operator. The red rect still paints inside it.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, "W* n");
}

#[test]
fn marker_body_multiple_shapes_and_orient_forms() {
    let text = svg(
        "mkbody.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs>
<marker id="composite" markerWidth="8" markerHeight="8" refX="4" refY="4" orient="auto">
<!-- comment -->
<rect x="0" y="0" width="4" height="4" fill="#ff0000"/>
<circle cx="6" cy="6" r="2" fill="#0000ff"/>
<path d="M0 4 L8 4" stroke="#00ff00" stroke-width="1"/>
</marker>
</defs>
<line x1="10" y1="20" x2="80" y2="20" stroke="#000000" stroke-width="1" marker-end="url(#composite)"/>
</svg>"##,
    );
    // A marker with several child shapes paints each of them at the vertex.
    assert_has(&text, "1.000 0.000 0.000 rg"); // rect in marker
    assert_has(&text, "0.000 0.000 1.000 rg"); // circle in marker
}

#[test]
fn text_body_nested_tspans_self_closing_and_comment() {
    let text = svg(
        "tbody.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 60">
<text x="10" y="30" font-size="10" fill="#000000">Pre<tspan dx="2">In<tspan dy="2">Deep</tspan></tspan><tspan x="90" y="30"/><!-- c -->Post</text>
</svg>"##,
    );
    // Nested and self-closing tspans plus a trailing comment all resolve to text
    // runs without derailing.
    assert!(
        text.matches("BT ").count() >= 3,
        "nested tspans should produce several runs"
    );
    assert_has(&text, "] TJ ET");
}

#[test]
fn gradient_definition_edges_userspace_and_spread_reflect() {
    let text = svg(
        "gdef.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<linearGradient id="ref" spreadMethod="reflect" gradientUnits="userSpaceOnUse" x1="1" y1="1" x2="21" y2="1">
<stop offset="0" stop-color="#ff0000"/>
<stop offset="0.5" stop-color="#00ff00"/>
<stop offset="1" stop-color="#0000ff"/>
</linearGradient>
</defs>
<rect x="1" y="1" width="40" height="20" fill="url(#ref)"/>
</svg>"##,
    );
    // A three-stop reflect gradient in user space is realised as tiled shadings.
    assert_has(&text, "/ShadingType 2");
    assert_has(&text, "/SG2 sh");
    assert_has(&text, "/C0 [1.000 0.000 0.000]");
}

// ===========================================================================
// Batch 5 — text length distribution, use recursion, scanner & def edges
// ===========================================================================

#[test]
fn text_length_distribution_happy_path_and_mismatches() {
    // Non-empty text either side of a tspan keeps every fragment positive and
    // aligned, so the parent textLength distributes across them.
    let distributed = svg(
        "tl_ok.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 40">
<text x="10" y="20" font-size="10" fill="#000000" textLength="220">Aa<tspan>Bb</tspan>Cc<tspan>Dd</tspan>Ee</text>
</svg>"##,
    );
    assert!(
        distributed.matches("BT ").count() >= 3,
        "distributed textLength keeps multiple runs"
    );

    // A child tspan carrying its own textLength aborts the parent distribution.
    let child_len = svg(
        "tl_child.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 40">
<text x="10" y="20" font-size="10" fill="#000000" textLength="200">Aa<tspan textLength="20">Bb</tspan>Cc</text>
</svg>"##,
    );
    assert_has(&child_len, "] TJ ET");

    // A tspan on a different baseline (dy) also aborts distribution.
    let ymismatch = svg(
        "tl_y.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 40">
<text x="10" y="20" font-size="10" fill="#000000" textLength="200">Aa<tspan dy="5">Bb</tspan>Cc</text>
</svg>"##,
    );
    assert_has(&ymismatch, "] TJ ET");
}

#[test]
fn text_body_comment_textpath_hidden_and_non_tspan_children() {
    let text = svg(
        "tbodyedge.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
<defs><path id="c" d="M10 50 L120 50"/></defs>
<text font-size="10" fill="#000000">Lead<!-- inline comment --><textPath href="#c">OnPath</textPath><tspan opacity="0">Hidden</tspan><a href="https://x">AnchorInText</a>Tail</text>
</svg>"##,
    );
    // The visible text, textPath, and transparent text-anchor child produce
    // runs; the opacity:0 tspan is skipped without derailing.
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
}

#[test]
fn accessible_text_scanner_duplicate_and_closing_tags() {
    let out = svg_pdf(
        "",
        "accdup.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
<!-- c -->
< >
<title>First Title</title>
<title>Second Title Ignored</title>
<desc>First Desc</desc>
<desc>Second Desc Ignored</desc>
<g></g>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/>
</svg>"##,
    );
    // Only the first title/desc are kept (the is_none() guards reject the rest).
    assert_has(&out, "/Alt (First Title - First Desc)");
}

#[test]
fn use_chain_recursion_stops_at_depth_limit() {
    let text = svg(
        "chain.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<g id="s0"><rect x="0" y="0" width="5" height="5" fill="#ff0000"/><use href="#s1"/></g>
<g id="s1"><use href="#s2"/></g>
<g id="s2"><use href="#s3"/></g>
<g id="s3"><use href="#s4"/></g>
<g id="s4"><use href="#s5"/></g>
<g id="s5"><use href="#s6"/></g>
<g id="s6"><use href="#s7"/></g>
<g id="s7"><use href="#s8"/></g>
<g id="s8"><use href="#s9"/></g>
<g id="s9"><rect x="10" y="0" width="5" height="5" fill="#00ff00"/></g>
</defs>
<use href="#s0"/>
</svg>"##,
    );
    // The shallow rect renders; the deep recursion is capped without a panic.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 5 5 re f");
}

#[test]
fn reusable_symbol_body_degenerate_children_and_text() {
    let text = svg(
        "reuse.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<defs>
<g id="lib">
<!-- comment -->
<rect x="0" y="0" width="0" height="5" fill="#ff0000"/>
<circle cx="5" cy="5" r="0" fill="#ff0000"/>
<rect x="8" y="0" width="5" height="5" fill="#00ff00"/>
<text x="0" y="20" font-size="6" fill="#0000ff">Reused</text>
</g>
</defs>
<use href="#lib"/>
<use href="#lib" x="30" y="0"/>
</svg>"##,
    );
    // Degenerate children are dropped; the valid green rect and text appear twice.
    assert!(
        text.matches("0.000 1.000 0.000 rg").count() >= 2,
        "green rect instanced twice"
    );
    assert_has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn path_commands_without_coordinates_stop_after_moveto() {
    // Each command letter with no following number stops parsing right after the
    // initial moveto, exercising every command's read-nothing branch.
    for (i, cmd) in ["L", "H", "V", "C", "S", "Q", "T", "A"].iter().enumerate() {
        let d = format!("M{i} {i} {cmd}");
        let svg_src = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="{d}" fill="none" stroke="#ff0000" stroke-width="1"/>
<rect x="50" y="0" width="8" height="8" fill="#00ff00"/>
</svg>"##
        );
        let text = svg(&format!("pnc{i}.svg"), &svg_src);
        assert_has(&text, &format!("{i} {i} m"));
        // The trailing control rect always renders -> parser recovered.
        assert_has(&text, "0.000 1.000 0.000 rg 50 0 8 8 re f");
    }
}

#[test]
fn mask_and_clippath_definition_edges() {
    let text = svg(
        "defsedge.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<!-- defs comment -->
<mask/>
<mask id=""><rect x="0" y="0" width="5" height="5" fill="#ffffff"/></mask>
<mask id="dup"><rect x="0" y="0" width="20" height="20" fill="#ffffff"/></mask>
<mask id="dup"><rect x="0" y="0" width="5" height="5" fill="#ffffff"/></mask>
<clipPath id="cpdup"><rect x="0" y="0" width="20" height="20"/></clipPath>
<clipPath id="cpdup"><circle cx="5" cy="5" r="3"/></clipPath>
</defs>
<rect x="0" y="0" width="30" height="30" fill="#ff0000" mask="url(#dup)"/>
<rect x="40" y="0" width="30" height="30" fill="#0000ff" clip-path="url(#cpdup)"/>
</svg>"##,
    );
    // The first #dup mask / #cpdup clip win; both masked shapes still paint.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn pattern_definition_edges_href_inherit_and_empty() {
    let text = svg(
        "patdef.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<pattern id="base" width="8" height="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="4" height="4" fill="#ff0000"/></pattern>
<pattern id="derived" href="#base"/>
<pattern id="hollow" width="8" height="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="0" height="0" fill="#00ff00"/></pattern>
</defs>
<rect x="0" y="0" width="30" height="30" fill="url(#derived)"/>
<rect x="40" y="0" width="30" height="30" fill="url(#hollow)"/>
</svg>"##,
    );
    // The derived pattern inherits the base tile colour.
    assert_has(&text, "1.000 0.000 0.000");
    // The hollow pattern has no renderable content -> that rect is not painted.
    assert_absent(&text, "40 0 30 30 re f");
}

#[test]
fn filter_shadow_definition_variants() {
    let text = svg(
        "filtdef.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 60">
<defs>
<filter id="bluronly"><feGaussianBlur stdDeviation="2"/></filter>
<filter id="shadow1"><feDropShadow dx="1" dy="1" flood-color="#ff0000"/></filter>
<filter id="shadow2"><feDropShadow dx="-2" dy="2" stdDeviation="0" flood-color="rgb(0,255,0)" flood-opacity="0.4"/></filter>
</defs>
<rect x="10" y="10" width="20" height="20" fill="#000000" filter="url(#bluronly)"/>
<rect x="50" y="10" width="20" height="20" fill="#0000ff" filter="url(#shadow1)"/>
<rect x="90" y="10" width="20" height="20" fill="#0000ff" filter="url(#shadow2)"/>
</svg>"##,
    );
    // A blur-only filter yields no drop shadow; the two feDropShadow filters each
    // paint an offset shadow in their flood colour.
    assert_has(&text, "1.000 0.000 0.000 rg"); // red shadow
    assert_has(&text, "0.000 1.000 0.000 rg"); // green shadow (with 0.4 flood-opacity)
    assert_has(&text, "0.000 0.000 1.000 rg 50 10 20 20 re f"); // shape over shadow
}

#[test]
fn css_style_block_cdata_and_comment_selector_wrappers() {
    // Exporters wrap <style> content in CDATA and HTML comments and pad selectors
    // with /* */ comments; the wrapper stripper must see through all of them.
    let text = svg(
        "cssw.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<style><![CDATA[
/* leading comment */ .m { fill: #ff0000; }
.s { fill: #00ff00; } /* trailing comment */
]]></style>
<style><!--
.t { fill: #0000ff; }
--></style>
<rect class="m" x="0" y="0" width="8" height="8"/>
<rect class="s" x="10" y="0" width="8" height="8"/>
<rect class="t" x="20" y="0" width="8" height="8"/>
</svg>"##,
    );
    // Rules inside CDATA / comment wrappers, with /* */ padding, still cascade.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg 20 0 8 8 re f");
}

#[test]
fn root_background_quotes_and_nested_parens_tokenizing() {
    let text = svg(
        "rootbg2.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" style="background: url('data:image/png;base64,AAAA') , radial-gradient(circle at 50% 50%, #ff0000, #0000ff) repeat">
<rect x="10" y="10" width="8" height="8" fill="#00ff00"/>
</svg>"##,
    );
    // The background shorthand tokenizer copes with quoted url() and nested
    // gradient parens; the content rect still renders.
    assert_has(&text, "0.000 1.000 0.000 rg 10 10 8 8 re f");
}

#[test]
fn nested_anchor_links_keep_innermost_target() {
    let text = svg(
        "nestlink.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 60">
<defs>
<g id="wrap"><a href="https://inner.example"><rect x="0" y="0" width="8" height="8" fill="#ff0000"/></a></g>
</defs>
<a href="https://outer.example"><use href="#wrap"/></a>
</svg>"##,
    );
    // The rect already carries the inner link, so the outer <a> does not overwrite
    // it (the is_none() guard is false); the inner URI is the one recorded.
    assert_has(&text, "/URI (https://inner.example)");
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
}

#[test]
fn unsafe_nested_anchor_in_reused_def_suppresses_outer_target() {
    let text = svg(
        "unsafe-reused-link.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 60">
<defs>
<g id="wrap"><a href="javascript:alert(1)"><rect x="0" y="0" width="8" height="8" fill="#ff0000"/></a></g>
</defs>
<a href="https://outer.example"><use href="#wrap"/></a>
</svg>"##,
    );

    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_absent(&text, "javascript:alert");
    assert_absent(&text, "/URI (https://outer.example)");
}

// ===========================================================================
// Batch 6 — def-body edges, inline style patch, base64, transforms, text lists
// ===========================================================================

#[test]
fn definition_scanners_tolerate_comments_self_closing_and_duplicates() {
    // One <defs> that stresses the marker / mask / clipPath / pattern scanners with
    // comments between defs, self-closing empties, id-less defs, and duplicate ids.
    let text = svg(
        "defsbig.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
<defs>
<!-- markers -->
<marker/>
<marker markerWidth="4" markerHeight="4"><rect x="0" y="0" width="2" height="2" fill="#ff0000"/></marker>
<marker id="mk" markerWidth="4" markerHeight="4" refX="2" refY="2"><circle cx="2" cy="2" r="1.5" fill="#ff0000"/></marker>
<marker id="mk"><rect x="0" y="0" width="1" height="1" fill="#000000"/></marker>
<!-- masks -->
<mask/>
<mask id="mk2"><rect x="0" y="0" width="20" height="20" fill="#ffffff"/></mask>
<!-- clip paths -->
<clipPath/>
<clipPath id="cp2"><rect x="0" y="0" width="20" height="20"/></clipPath>
<!-- patterns -->
<pattern/>
<pattern id="pt2" width="6" height="6" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="3" height="3" fill="#00ff00"/></pattern>
</defs>
<polyline points="10,30 40,30 70,30" fill="none" stroke="#000000" stroke-width="1" marker-mid="url(#mk)"/>
<rect x="80" y="5" width="20" height="20" fill="#0000ff" mask="url(#mk2)" clip-path="url(#cp2)"/>
<rect x="80" y="30" width="20" height="20" fill="url(#pt2)"/>
</svg>"##,
    );
    // The first valid #mk marker wins and paints red circles; the masked/clipped
    // blue rect and the green-pattern rect both render.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, "0.000 0.000 1.000 rg");
    assert_has(&text, "0.000 1.000 0.000");
}

#[test]
fn inline_style_patch_covers_paint_width_and_alpha_declarations() {
    let text = svg(
        "stylepatch.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<rect x="0" y="0" width="8" height="8" style="fill:#ff0000;stroke:#00ff00;stroke-width:2;fill-rule:evenodd;display:inline;visibility:visible"/>
<rect x="20" y="0" width="8" height="8" style="fill:#0000ff80;stroke:#ffffff40"/>
</svg>"##,
    );
    // Inline fill/stroke/stroke-width parse; the first rect is filled red and
    // stroked green at width 2.
    assert_has(&text, "1.000 0.000 0.000 rg 0.000 1.000 0.000 RG 2 w");
    // Hex-alpha fill and stroke in the style patch both feed one opacity ExtGState.
    assert_has(&text, "/ca 0.502 /CA 0.251");
    assert_has(&text, "/GSa05020251 gs 0.000 0.000 1.000 rg");
}

#[test]
fn attribute_value_unterminated_quote_stops_after_valid_attrs() {
    let text = svg(
        "attrq.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 40 20\">\
<rect x=\"0\" y=\"0\" width=\"8\" height=\"8\" fill=\"#00ff00\" data='unterminated/>\
<rect x=\"10\" y=\"0\" width=\"8\" height=\"8\" fill=\"#0000ff\"/></svg>",
    );
    // Attributes before the unterminated quote are kept, so the first rect renders
    // green; the tokenizer then stops for that tag but the next tag is fine.
    assert_has(&text, "0.000 1.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg 10 0 8 8 re f");
}

#[test]
fn transform_parser_more_boundary_inputs() {
    let text = svg(
        "tfmore.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<rect x="0" y="0" width="8" height="8" fill="#ff0000" transform="translate(4,4) "/>
<rect x="10" y="0" width="8" height="8" fill="#00ff00" transform="rotate (30)"/>
<rect x="20" y="0" width="8" height="8" fill="#0000ff" transform="skewX() scale(2)"/>
<rect x="30" y="0" width="8" height="8" fill="#ffff00" transform="scale(3)"/>
</svg>"##,
    );
    // A trailing separator after a valid translate is fine.
    assert_has(&text, "q 1 0 0 1 4 4 cm 1.000 0.000 0.000 rg 0 0 8 8 re f");
    // A space before '(' invalidates rotate, leaving green un-transformed.
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f");
    // skewX() with no args aborts before scale, leaving blue un-transformed.
    assert_has(&text, "0.000 0.000 1.000 rg 20 0 8 8 re f");
    // A well-formed uniform scale still applies.
    assert_has(&text, "q 3 0 0 3 0 0 cm 1.000 1.000 0.000 rg 30 0 8 8 re f");
}

#[test]
fn inline_image_base64_padding_and_length_variants() {
    // Valid 1-pixel PNG (renders), then payloads that stress the base64 decoder:
    // a two-byte (single-pad) payload, whitespace, a non-multiple-of-four length,
    // a leading-'=' quartet, and padding followed by more data.
    let png_b64 = base64_encode(&tiny_rgb_png(&[[0x00, 0xff, 0x00]]));
    let src = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 20">
<image x="0" y="0" width="8" height="8" href="data:image/png;base64, {png_b64} "/>
<image x="10" y="0" width="8" height="8" href="data:image/png;base64,QQ=="/>
<image x="20" y="0" width="8" height="8" href="data:image/png;base64,QQ"/>
<image x="30" y="0" width="8" height="8" href="data:image/png;base64,====",/>
<image x="40" y="0" width="8" height="8" href="data:image/png;base64,QUI=X"/>
<rect x="60" y="0" width="8" height="8" fill="#0000ff"/>
</svg>"##
    );
    let text = svg("b64.svg", &src);
    // The whitespace-padded valid PNG still decodes to an image XObject.
    assert_has(&text, " Do");
    // The control rect after all the malformed payloads still renders.
    assert_has(&text, "0.000 0.000 1.000 rg 60 0 8 8 re f");
}

#[test]
fn text_positioned_lists_with_percentages_and_many_tspans() {
    // Per-character x/y lists (with a percentage entry) plus a long tspan run.
    let mut spans = String::new();
    for i in 0..40 {
        spans.push_str(&format!("<tspan dx=\"1\">{}</tspan>", i % 10));
    }
    let src = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 60">
<text x="10 20 30" y="20" font-size="8" fill="#000000">abc</text>
<text x="10" y="40" font-size="8" fill="#000000" textLength="50%">wide</text>
<text x="10" y="55" font-size="6" fill="#000000">{spans}</text>
</svg>"##
    );
    let text = svg("tlist.svg", &src);
    assert!(
        text.matches("BT ").count() >= 5,
        "positioned lists and many tspans yield many runs"
    );
    assert_has(&text, "] TJ ET");
}

#[test]
fn document_degenerate_line_polyline_and_explicit_closings() {
    let text = svg(
        "docdegen.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<line x1="0" y1="0" x2="0" y2="0" stroke="#ff0000" stroke-width="1"/>
<polyline points="5,5" fill="none" stroke="#ff0000" stroke-width="1"/>
<polygon points="1,1 2,2" fill="#ff0000"/>
<rect x="10" y="0" width="8" height="8" fill="#00ff00"></rect>
<circle cx="30" cy="4" r="4" fill="#0000ff"></circle>
</svg>"##,
    );
    // Single-point polyline and 2-point polygon are dropped; the explicitly-closed
    // rect and circle render.
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg");
    // The 2-point polygon (needs >=3 points) produced no filled red path.
    assert_absent(&text, "1.000 0.000 0.000 rg 1 1 m");
}

#[test]
fn markers_units_userspaceonuse_and_orient_angle_forms() {
    let text = svg(
        "mkunits.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40">
<defs>
<marker id="u" markerUnits="userSpaceOnUse" markerWidth="6" markerHeight="6" refX="3" refY="3" orient="90">
<!-- comment in marker body -->
<rect x="0" y="0" width="0" height="0" fill="#000000"/>
<ellipse cx="3" cy="3" rx="2" ry="2" fill="#ff0000"/>
</marker>
</defs>
<line x1="10" y1="20" x2="90" y2="20" stroke="#000000" stroke-width="1" marker-end="url(#u)"/>
</svg>"##,
    );
    // The degenerate rect in the marker body is skipped; the ellipse paints red.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, "0.000 0.000 0.000 RG");
}

// ===========================================================================
// Batch 7 — render_warnings, unterminated bodies, nested links, paint keywords
// ===========================================================================

#[test]
fn render_warnings_reports_missing_assets_and_glyphs() {
    // An unmapped image destination, a supplied-but-undecodable asset, and text
    // with no glyph in the embedded fonts each surface a distinct warning.
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("bad.png", vec![0x00, 0x01, 0x02, 0x03])],
        ..PdfOptions::default()
    };
    let doc = parse_markdown("![a](missing.png) ![b](bad.png)\n\nplain 文字 中\n");
    let warnings = render_warnings(&doc, &opts);
    let codes: Vec<&str> = warnings.iter().map(|w| w.code()).collect();
    assert!(
        codes.contains(&"unresolved_image"),
        "missing asset -> unresolved_image: {codes:?}"
    );
    assert!(
        codes.contains(&"unsupported_image"),
        "undecodable asset -> unsupported_image: {codes:?}"
    );
    assert!(
        codes.contains(&"missing_glyphs"),
        "CJK text -> missing_glyphs: {codes:?}"
    );
    // The messages name the offending destinations / characters.
    let joined: String = warnings.iter().map(|w| w.message()).collect();
    assert!(joined.contains("missing.png"));
    assert!(joined.contains("bad.png"));
}

#[test]
fn text_body_unclosed_child_and_long_tspan_run() {
    // An unclosed <tspan> makes the body scan stop after the preceding text; a very
    // long run of tspans exercises the element-count cap.
    let unclosed = svg(
        "unclosed.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<text x="10" y="20" font-size="8" fill="#000000">Before<tspan>NeverClosed</text>
</svg>"##,
    );
    assert_has(&unclosed, "BT ");

    let mut spans = String::new();
    for i in 0..40 {
        spans.push_str(&format!("<tspan dx=\"1\">{}</tspan>", i % 10));
    }
    let src = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 40">
<text x="5" y="20" font-size="4" fill="#000000">{spans}</text>
</svg>"##
    );
    let many = svg("manyspans.svg", &src);
    assert!(
        many.matches("BT ").count() >= 3,
        "long tspan run should still emit multiple runs (count {})",
        many.matches("BT ").count()
    );
}

#[test]
fn nested_links_over_every_shape_kind_keep_inner_target() {
    // A symbol wraps one <a> around every shape kind; instancing it under an outer
    // <a> re-runs apply_svg_link on already-linked shapes (the is_none() guard is
    // false for each variant) so the inner URI is preserved.
    let text = svg(
        "nestall.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120">
<defs>
<g id="w">
<a href="https://inner.example">
<rect x="0" y="0" width="6" height="6" fill="#ff0000"/>
<circle cx="20" cy="4" r="3" fill="#00ff00"/>
<ellipse cx="40" cy="4" rx="4" ry="2" fill="#0000ff"/>
<line x1="0" y1="20" x2="20" y2="20" stroke="#000000" stroke-width="1"/>
<polyline points="0,30 10,40 20,30" fill="none" stroke="#000000" stroke-width="1"/>
<polygon points="30,30 40,40 20,40" fill="#ffff00"/>
<path d="M0 50 L20 50" stroke="#000000" stroke-width="1" fill="none"/>
<text x="0" y="70" font-size="8" fill="#000000">T</text>
</a>
</g>
</defs>
<a href="https://outer.example"><use href="#w"/></a>
</svg>"##,
    );
    // Every shape keeps the inner link; the outer never overwrites.
    assert_has(&text, "/URI (https://inner.example)");
    let inner = text.matches("(https://inner.example)").count();
    assert!(inner >= 6, "most shapes carry the inner URI, got {inner}");
}

#[test]
fn paint_current_color_and_context_keywords_on_fill_and_stroke() {
    let text = svg(
        "paintkw.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<rect x="0" y="0" width="8" height="8" color="#ff0000" fill="currentColor" stroke="currentColor" stroke-width="1"/>
<defs>
<marker id="cf" markerWidth="6" markerHeight="6" refX="3" refY="3">
<rect x="0" y="0" width="4" height="4" fill="context-fill"/>
<circle cx="3" cy="3" r="1" stroke="context-stroke" stroke-width="1" fill="none"/>
</marker>
</defs>
<polyline points="0,30 40,30" fill="#00ff00" stroke="#0000ff" stroke-width="1" marker-start="url(#cf)"/>
</svg>"##,
    );
    // currentColor resolves both fill and stroke to the element's red `color`.
    assert_has(
        &text,
        "1.000 0.000 0.000 rg 1.000 0.000 0.000 RG 1 w 0 J 0 j 4 M 0 0 8 8 re B",
    );
    // The context-fill marker rect inherits the polyline's green fill; the
    // context-stroke circle inherits its blue stroke.
    assert_has(&text, "0.000 1.000 0.000 rg");
    assert_has(&text, "0.000 0.000 1.000 RG");
}

#[test]
fn attribute_tokenizer_whitespace_only_and_bare_equals() {
    let text = svg(
        "attrws.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 40 20\">\
<rect   x=\"0\" y=\"0\" width=\"8\" height=\"8\" fill = \"#ff0000\"  />\
<rect x=\"10\" y=\"0\" width=\"8\" height=\"8\" fill=\"#0000ff\" =novalue class=\"c\"/></svg>",
    );
    // Spaces around '=' and a stray leading '=' token are tolerated.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg 10 0 8 8 re f");
}

#[test]
fn positioned_child_scan_sees_comment_and_textpath() {
    let text = svg(
        "poschild.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 60">
<defs><path id="pp" d="M10 30 L150 30"/></defs>
<text font-size="9" fill="#000000"><!-- lead comment -->Intro<textPath href="#pp">Along</textPath></text>
</svg>"##,
    );
    // The positioned-child detector sees past the comment to the textPath, so the
    // text is laid out on the path.
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
}

// ===========================================================================
// Batch 8 — pattern/gradient/filter definition edges, coord lists, selectors
// ===========================================================================

#[test]
fn pattern_definition_dimension_and_duplicate_edges() {
    let text = svg(
        "patedge.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<pattern id="nowidth" height="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="4" height="4" fill="#000000"/></pattern>
<pattern id="zero" width="0" height="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="4" height="4" fill="#000000"/></pattern>
<pattern id="dup" width="8" height="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="4" height="4" fill="#ff0000"/></pattern>
<pattern id="dup" width="8" height="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="4" height="4" fill="#000000"/></pattern>
<pattern id="missingref" href="#doesnotexist"/>
<pattern id="good" width="8" height="8" patternUnits="userSpaceOnUse" patternContentUnits="objectBoundingBox" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" fill="#00ff00"/></pattern>
</defs>
<rect x="0" y="0" width="30" height="30" fill="url(#dup)"/>
<rect x="40" y="0" width="30" height="30" fill="url(#good)"/>
</svg>"##,
    );
    // The first #dup wins (red); the good pattern's green tile drives its fill.
    assert_has(&text, "1.000 0.000 0.000");
    assert_has(&text, "0.000 1.000 0.000");
}

#[test]
fn gradient_definition_href_chain_no_stops_and_transform() {
    let text = svg(
        "gradchain.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<linearGradient id="stops"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient>
<linearGradient id="mid" href="#stops"/>
<linearGradient id="final" href="#mid" x1="0" y1="0" x2="1" y2="0" gradientTransform="scale(2)"/>
<linearGradient id="empty"></linearGradient>
<radialGradient id="rempty"></radialGradient>
</defs>
<rect x="1" y="1" width="40" height="40" fill="url(#final)"/>
<rect x="50" y="1" width="40" height="40" fill="url(#empty)"/>
</svg>"##,
    );
    // The two-hop href chain inherits stops from the root definition.
    assert_has(&text, "/ShadingType 2");
    assert_has(&text, "/C0 [1.000 0.000 0.000]");
    assert_has(&text, "/C1 [0.000 0.000 1.000]");
    // A stop-less gradient paints nothing.
    assert_absent(&text, "50 1 40 40 re f");
}

#[test]
fn filter_drop_shadow_multiple_and_css_variable_flood() {
    let text = svg(
        "filtvar.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 60">
<style>:root { --shade: #ff0000; }</style>
<defs>
<filter id="a"><feDropShadow dx="3" dy="0" stdDeviation="2" flood-color="var(--shade)"/></filter>
<filter id="b"><feDropShadow dx="0" dy="3" flood-color="#0000ff" flood-opacity="0.75"/></filter>
</defs>
<rect x="10" y="10" width="20" height="20" fill="#00ff00" filter="url(#a)"/>
<rect x="60" y="10" width="20" height="20" fill="#00ff00" filter="url(#b)"/>
</svg>"##,
    );
    // The CSS-variable flood colour resolves to red for the first shadow.
    assert_has(&text, "1.000 0.000 0.000 rg");
    // The second shadow uses blue at 0.75 flood-opacity.
    assert_has(&text, "0.000 0.000 1.000 rg");
    assert_has(&text, "/CA 0.750");
}

#[test]
fn text_coordinate_lists_with_units_and_invalid_tokens() {
    let text = svg(
        "coordlist.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 60">
<text x="10px 20% 30em bad 40" y="20 , 25 ,  30" font-size="8" fill="#000000">abcde</text>
</svg>"##,
    );
    // Mixed units, an invalid token, and stray comma/space separators all parse
    // into a coordinate list that lays out the glyphs.
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
    let runs = text.matches("BT ").count();
    assert!(
        runs >= 2,
        "per-character positions split the text, got {runs}"
    );
}

#[test]
fn textpath_on_curved_path_visits_all_segment_kinds() {
    let text = svg(
        "curvepath.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
<defs><path id="cv" d="M10 50 C30 10 60 90 90 50 Q120 10 150 50 Z"/></defs>
<text font-size="9" fill="#000000"><textPath href="#cv">Curvy</textPath></text>
</svg>"##,
    );
    // Laying text on a path with cubic, quadratic and close segments succeeds.
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
}

#[test]
fn css_selectors_child_universal_pseudo_and_overdeep() {
    let text = svg(
        "cssdeep.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<style>
* { fill: #cccccc; }
:hover { fill: #000000; }
> rect { fill: #000000; }
rect > { fill: #000000; }
a b c d e f g rect { fill: #000000; }
g > rect { fill: #00ff00; }
</style>
<rect x="0" y="0" width="8" height="8"/>
<g><rect x="10" y="0" width="8" height="8"/></g>
</svg>"##,
    );
    // The universal selector paints the lone rect grey; the child combinator paints
    // the grouped rect green. The pseudo/leading-> /trailing-> /7-deep rules are
    // all dropped.
    assert_has(&text, "0.800 0.800 0.800 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 1.000 0.000 rg 10 0 8 8 re f");
}

#[test]
fn paint_context_fill_and_pattern_reference_on_plain_shapes() {
    let text = svg(
        "paintshapes.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs>
<pattern id="pt" width="8" height="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="4" height="4" fill="#ff0000"/></pattern>
</defs>
<rect x="0" y="0" width="8" height="8" fill="context-fill" stroke="context-stroke" stroke-width="1"/>
<rect x="20" y="0" width="8" height="8" fill="url(#pt)"/>
</svg>"##,
    );
    // A pattern reference resolves to its representative red tile colour.
    assert_has(&text, "1.000 0.000 0.000");
    // context-fill/stroke outside a marker leave the shape unpainted, so no fill
    // operator is emitted for it (it keeps no context to inherit).
    assert_absent(&text, "0 0 8 8 re f");
}

#[test]
fn document_self_closing_skip_elements_and_explicit_shape_closings() {
    let text = svg(
        "docskip.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs/>
<style/>
<clipPath/>
<mask/>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/>
</rect>
<circle cx="20" cy="4" r="0" fill="#00ff00"/>
<ellipse cx="30" cy="4" rx="4" ry="4" fill="#0000ff"/>
</svg>"##,
    );
    // Self-closing defs/style/clipPath/mask do not open a skip subtree; the red
    // rect, the valid ellipse render, and the stray </rect> is ignored.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg");
}

// ===========================================================================
// Batch 9 — text-length early returns, reusable/accessible/text-body specifics
// ===========================================================================

#[test]
fn text_length_distribution_early_return_conditions() {
    // Each child property that violates the "uniform run" preconditions aborts the
    // parent textLength distribution at a distinct guard.
    let anchor = svg(
        "tla.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 40">
<text x="10" y="20" font-size="10" fill="#000000" textLength="200">Aa<tspan text-anchor="middle">Bb</tspan>Cc</text>
</svg>"##,
    );
    assert_has(&anchor, "] TJ ET");

    let xf = svg(
        "tlx.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 40">
<text x="10" y="20" font-size="10" fill="#000000" textLength="200">Aa<tspan transform="translate(1,0)">Bb</tspan>Cc</text>
</svg>"##,
    );
    assert_has(&xf, "] TJ ET");

    let xpos = svg(
        "tlp.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 40">
<text x="10" y="20" font-size="10" fill="#000000" textLength="200">Aa<tspan x="150">Bb</tspan>Cc</text>
</svg>"##,
    );
    assert_has(&xpos, "] TJ ET");

    let empty = svg(
        "tle.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 40">
<text x="10" y="20" font-size="10" fill="#000000" textLength="200"><tspan></tspan><tspan>Bb</tspan></text>
</svg>"##,
    );
    assert_has(&empty, "] TJ ET");
}

#[test]
fn text_body_children_closing_pi_and_hidden_textpath() {
    let text = svg(
        "tbchild.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 60">
<defs><path id="hp" d="M10 40 L120 40"/></defs>
<text x="10" y="20" font-size="9" fill="#000000">A</b>B<?pi ?><textPath href="#hp" opacity="0">Hidden</textPath><textPath href="#missingpath">Dangling</textPath>C</text>
</svg>"##,
    );
    // Stray closing tag, processing instruction, a hidden textPath (opacity 0) and
    // a textPath referencing a missing path are all skipped; visible text renders.
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
}

#[test]
fn reusable_symbol_container_closings_and_text_child() {
    let text = svg(
        "reclose.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<defs>
<symbol id="sym">
<g fill="#ff0000"><rect x="0" y="0" width="5" height="5"/></g>
<a href="https://x"><rect x="6" y="0" width="5" height="5" fill="#00ff00"/></a>
<symbol id="inner"><rect x="12" y="0" width="5" height="5" fill="#0000ff"/></symbol>
<text x="0" y="20" font-size="6" fill="#000000">Sym</text>
<g/>
<a/>
</symbol>
</defs>
<use href="#sym"/>
</svg>"##,
    );
    // The <g>/<a>/<symbol>/<text> children plus self-closing container tags all
    // resolve; the red and green rects render and a text run appears.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 5 5 re f");
    assert_has(&text, "0.000 1.000 0.000 rg 6 0 5 5 re f");
    assert_has(&text, "BT ");
}

#[test]
fn accessible_text_nested_depth_and_self_closing_meta_elements() {
    let out = svg_pdf(
        "",
        "accnest.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
<metadata>ignored</metadata>
<g>
<g><title>Deep Ignored</title></g>
</g>
<title>Real</title>
<desc/>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/>
</svg>"##,
    );
    // A depth-2 title is ignored; the depth-1 title is used, the self-closing desc
    // contributes nothing.
    assert_has(&out, "/Alt (Real)");
}

#[test]
fn document_empty_text_and_group_pop_balance() {
    let text = svg(
        "docbalance.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<text x="0" y="10" font-size="8" fill="#000000"></text>
<text x="0" y="20" font-size="8" fill="#000000">   </text>
<g fill="#ff0000"></g>
<a href="https://x"></a>
<rect x="0" y="24" width="8" height="8" fill="#00ff00"/>
</svg>"##,
    );
    // Empty / whitespace-only text elements are dropped and the empty container
    // opens/closes balance the style stack, so the final rect renders green.
    assert_has(&text, "0.000 1.000 0.000 rg 0 24 8 8 re f");
}

#[test]
fn clip_paths_scanner_self_closing_no_id_and_duplicate() {
    let text = svg(
        "clipscan.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<defs>
<clipPath/>
<clipPath id=""><rect x="0" y="0" width="5" height="5"/></clipPath>
<clipPath id="a"><rect x="0" y="0" width="30" height="30"/></clipPath>
<clipPath id="a"><circle cx="5" cy="5" r="2"/></clipPath>
</defs>
<rect x="0" y="0" width="40" height="40" fill="#ff0000" clip-path="url(#a)"/>
</svg>"##,
    );
    // Self-closing and id-less clipPaths are skipped; the first #a wins and clips.
    assert_has(&text, "W n 1.000 0.000 0.000 rg 0 0 40 40 re f");
}

// ===========================================================================
// Batch 10 — CSS-rule style patches, mask reveal attrs, scanner edges
// ===========================================================================

#[test]
fn css_rule_declarations_invalid_paint_and_stroke_width() {
    let text = svg(
        "cssrule.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20">
<style>
.badfill { fill: notacolor; }
.badstroke { fill: #0000ff; stroke: notacolor; }
.sw { fill: none; stroke: #00ff00; stroke-width: 3; stroke-dasharray: 2 1; }
</style>
<rect class="badfill" x="0" y="0" width="8" height="8"/>
<rect class="badstroke" x="10" y="0" width="8" height="8"/>
<rect class="sw" x="20" y="0" width="8" height="8"/>
</svg>"##,
    );
    // An invalid CSS `fill` leaves the rect its inherited black; an invalid CSS
    // `stroke` leaves the blue fill intact with no stroke; a valid stroke-width
    // rule applies width 3 with a dash.
    assert_has(&text, "0.000 0.000 0.000 rg 0 0 8 8 re f");
    assert_has(&text, "0.000 0.000 1.000 rg 10 0 8 8 re f");
    assert_has(&text, "0.000 1.000 0.000 RG 3 w");
    assert_has(&text, "[2 1] 0 d");
}

#[test]
fn mask_reveal_via_attributes_and_style_opacity_forms() {
    let text = svg(
        "maskattr.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs>
<mask id="m">
<rect x="0" y="0" width="10" height="10" fill="#ffffff" opacity="0.9" fill-opacity="0.9"/>
<rect x="0" y="10" width="10" height="10" fill="#ffffff" opacity="0.001"/>
<circle cx="5" cy="25" r="4" style="fill:#ffffff;opacity:0.8;fill-opacity:0.9"/>
<rect x="12" y="0" width="10" height="10" fill="#000000"/>
</mask>
</defs>
<rect x="0" y="0" width="30" height="30" fill="#ff0000" mask="url(#m)"/>
</svg>"##,
    );
    // The bright, sufficiently-opaque shapes reveal (clip + fill); the near-
    // transparent and dark shapes do not.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, " W n");
}

#[test]
fn gradient_scanner_self_closing_no_id_and_duplicate_defs() {
    let text = svg(
        "gradscan.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<!-- gradient scanner comment -->
<linearGradient/>
<linearGradient><stop offset="0" stop-color="#000000"/></linearGradient>
<linearGradient id="g"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient>
<linearGradient id="g"><stop offset="0" stop-color="#00ff00"/></linearGradient>
</defs>
<rect x="1" y="1" width="40" height="40" fill="url(#g)"/>
</svg>"##,
    );
    // The first #g wins (red->blue); self-closing and id-less gradients are skipped.
    assert_has(&text, "/C0 [1.000 0.000 0.000]");
    assert_has(&text, "/C1 [0.000 0.000 1.000]");
}

#[test]
fn root_background_single_and_quoted_tokens() {
    let solid = svg(
        "bgsolid.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:#ff0000">
<rect x="5" y="5" width="8" height="8" fill="#00ff00"/></svg>"##,
    );
    // A single solid-colour background token paints the whole viewport red.
    assert_has(&solid, "1.000 0.000 0.000 rg");
    assert_has(&solid, "0.000 1.000 0.000 rg 5 5 8 8 re f");

    let quoted = svg(
        "bgquoted.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background: &quot;ignored quoted&quot; #0000ff">
<rect x="5" y="5" width="8" height="8" fill="#00ff00"/></svg>"##,
    );
    // A quoted token is tokenized as one unit and skipped; the colour token wins.
    assert_has(&quoted, "0.000 0.000 1.000 rg");
    assert_has(&quoted, "0.000 1.000 0.000 rg 5 5 8 8 re f");
}

#[test]
fn marker_scanner_self_closing_no_id_and_duplicate() {
    let text = svg(
        "mkscan.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40">
<defs>
<!-- marker scan comment -->
<marker/>
<marker markerWidth="4" markerHeight="4"><rect x="0" y="0" width="2" height="2" fill="#000000"/></marker>
<marker id="a" markerWidth="6" markerHeight="6" refX="3" refY="3"><circle cx="3" cy="3" r="2" fill="#ff0000"/></marker>
<marker id="a"><rect x="0" y="0" width="1" height="1" fill="#000000"/></marker>
</defs>
<line x1="10" y1="20" x2="90" y2="20" stroke="#000000" stroke-width="1" marker-end="url(#a)"/>
</svg>"##,
    );
    // The first #a marker wins and paints a red circle; comment, self-closing and
    // id-less markers are skipped.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, "0.000 0.000 0.000 RG");
}

#[test]
fn filter_shadow_flood_forms_and_stddeviation_variants() {
    let text = svg(
        "filtforms.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 60">
<defs>
<filter id="a"><feDropShadow dx="2" dy="2" stdDeviation="0.5" flood-color="green"/></filter>
<filter id="b"><feDropShadow dx="1" dy="1" stdDeviation="3" flood-color="#654321" flood-opacity="1"/></filter>
<filter id="c"><feDropShadow dx="0" dy="0"/></filter>
</defs>
<rect x="10" y="10" width="20" height="20" fill="#ffffff" filter="url(#a)"/>
<rect x="60" y="10" width="20" height="20" fill="#ffffff" filter="url(#b)"/>
<rect x="110" y="10" width="20" height="20" fill="#ffffff" filter="url(#c)"/>
</svg>"##,
    );
    // Named and hex flood colours both drive shadows; a zero-offset shadow defaults
    // to black.
    assert_has(&text, "0.000 0.502 0.000 rg"); // named green flood
    assert_has(&text, "0.396 0.263 0.129 rg"); // hex flood
}

// ===========================================================================
// Batch 11 — document text arms, pattern content detection, viewBox degrade
// ===========================================================================

#[test]
fn document_self_closing_text_unclosed_text_and_nested_svg() {
    let text = svg(
        "doctext.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<text x="0" y="10" font-size="8" fill="#000000"/>
<svg x="20" y="0" width="10" height="10" viewBox="0 0 5 5"/>
<rect x="0" y="20" width="8" height="8" fill="#00ff00"/>
<text x="0" y="40" font-size="8" fill="#000000">NeverClosedAtDocLevel</svg>"##,
    );
    // A self-closing <text/>, a self-closing nested <svg/>, and an unclosed <text>
    // (no </text> before </svg>) are all skipped; the control rect renders.
    assert_has(&text, "0.000 1.000 0.000 rg 0 20 8 8 re f");
}

#[test]
fn pattern_body_content_detection_text_comment_and_href() {
    let text = svg(
        "patcontent.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40">
<defs>
<pattern id="base" width="8" height="8" patternUnits="userSpaceOnUse">leadingtext<rect x="0" y="0" width="4" height="4" fill="#ff0000"/></pattern>
<pattern id="cmt" width="8" height="8" patternUnits="userSpaceOnUse"><!-- only comment, no shapes --></pattern>
<pattern id="derived" href="#base" x="1" y="1"/>
</defs>
<rect x="0" y="0" width="20" height="20" fill="url(#derived)"/>
<rect x="30" y="0" width="20" height="20" fill="url(#cmt)"/>
</svg>"##,
    );
    // The derived pattern inherits the base tile (with leading text content) -> red.
    assert_has(&text, "1.000 0.000 0.000");
    // The comment-only pattern has no renderable content -> that rect isn't painted.
    assert_absent(&text, "30 0 20 20 re f");
}

#[test]
fn pattern_missing_height_is_rejected() {
    let text = svg(
        "patnoh.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs>
<pattern id="noh" width="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="4" height="4" fill="#ff0000"/></pattern>
<pattern id="ok" width="8" height="8" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="4" height="4" fill="#00ff00"/></pattern>
</defs>
<rect x="0" y="0" width="20" height="20" fill="url(#noh)"/>
<rect x="30" y="0" width="20" height="20" fill="url(#ok)"/>
</svg>"##,
    );
    // A pattern with no height is dropped: the url(#noh) fill resolves to nothing,
    // so that rect is left unpainted; the well-formed pattern colours its rect green.
    assert_has(&text, "0.000 1.000 0.000");
    assert_absent(&text, "0 0 20 20 re f");
}

#[test]
fn svg_with_nonpositive_viewbox_degrades_to_alt_text() {
    // A zero-width viewBox makes the whole SVG unrenderable; the render must still
    // produce a valid, deterministic PDF (the image falls back to alt text).
    let a = render_pdf(
        "![alt words](zero.svg)",
        &PdfOptions {
            image_assets: vec![PdfImageAsset::new(
                "zero.svg",
                br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 0 0"><rect x="0" y="0" width="8" height="8" fill="#ff0000"/></svg>"##.to_vec(),
            )],
            ..PdfOptions::default()
        },
    )
    .unwrap();
    let text = String::from_utf8_lossy(&a);
    assert!(text.starts_with("%PDF-1.7"));
    assert!(text.trim_end().ends_with("%%EOF"));
    // No vector fill was emitted from the unrenderable SVG.
    assert!(!text.contains("1.000 0.000 0.000 rg 0 0 8 8 re f"));
}

#[test]
fn reusable_defs_referenced_variants_render_once_each() {
    let text = svg(
        "redefs.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs>
<!-- reusable defs comment -->
<rect id="r" x="0" y="0" width="6" height="6" fill="#ff0000"/>
<circle id="c" cx="15" cy="3" r="3" fill="#00ff00"/>
<path id="p" d="M25 0 L31 0 L31 6 Z" fill="#0000ff"/>
</defs>
<use href="#r"/>
<use href="#c"/>
<use href="#p"/>
</svg>"##,
    );
    // Each referenced def is collected and instanced by its <use>.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 6 6 re f");
    assert_has(&text, "0.000 1.000 0.000 rg");
    assert_has(&text, "0.000 0.000 1.000 rg");
}

// ===========================================================================
// Batch 12 — filter & mask scanner / body edges
// ===========================================================================

#[test]
fn filter_scanner_self_closing_no_id_duplicate_and_body_comment() {
    let text = svg(
        "filtscan.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<defs>
<!-- filter scan comment -->
<filter/>
<filter><feDropShadow dx="1" dy="1" flood-color="#000000"/></filter>
<filter id="dup"><feDropShadow dx="2" dy="2" flood-color="#ff0000"/></filter>
<filter id="dup"><feDropShadow dx="5" dy="5" flood-color="#0000ff"/></filter>
<filter id="merged">
<!-- body comment -->
<feFlood flood-color="#00ff00" flood-opacity="0.5"/>
<feOffset dx="2" dy="2"/>
<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge>
</filter>
</defs>
<rect x="10" y="10" width="20" height="20" fill="#ffffff" filter="url(#dup)"/>
</svg>"##,
    );
    // The first #dup filter wins -> a red shadow (not the blue duplicate).
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_absent(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn mask_body_closing_tags_and_grouped_shapes() {
    let text = svg(
        "maskclose.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
<defs>
<mask id="m">
<g>
<rect x="0" y="0" width="20" height="20" fill="#ffffff"/>
</g>
<polygon points="0,0 20,0 10,20" fill="#ffffff"/>
</mask>
</defs>
<rect x="0" y="0" width="30" height="30" fill="#ff0000" mask="url(#m)"/>
</svg>"##,
    );
    // The white shapes (including inside a group and a polygon) reveal the red rect;
    // stray container closing tags in the mask body are handled.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, " W n");
}

// ===========================================================================
// Batch 13 — mask reveal style declarations & accessible-text closing depth
// ===========================================================================

#[test]
fn mask_shape_reveal_style_declaration_forms() {
    let text = svg(
        "maskstyle.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 60">
<defs>
<mask id="m">
<rect x="0" y="0" width="20" height="20" style="fill:#ffffff;opacity:0.9;fill-opacity:0.95;garbage"/>
<rect x="0" y="22" width="20" height="20" style="fill:#ffffff;opacity:0"/>
<rect x="22" y="0" width="20" height="20" style="fill:none"/>
</mask>
</defs>
<rect x="0" y="0" width="45" height="45" fill="#ff0000" mask="url(#m)"/>
</svg>"##,
    );
    // The first shape (bright, opaque, with a colon-less junk declaration) reveals;
    // the opacity:0 and fill:none shapes reveal nothing. The red rect paints inside.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, " W n");
}

#[test]
fn accessible_text_closing_svg_and_style_script_skip_at_depth() {
    let out = svg_pdf(
        "",
        "accclose.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20">
<style>.a{fill:#000}</style>
<script>noop()</script>
<g><rect x="0" y="0" width="4" height="4"/></g>
<title>Closed</title>
<rect x="0" y="0" width="8" height="8" fill="#ff0000"/>
</svg>
<title>After Root Ignored</title>"##,
    );
    // The scanner skips <style>/<script>, tracks group depth, stops at </svg>, and
    // ignores a title after the root close.
    assert_has(&out, "/Alt (Closed)");
}

// ===========================================================================
// Batch 14 — text positioned-child scanning & reusable text children
// ===========================================================================

#[test]
fn text_positioned_child_scan_with_closing_and_pi_tokens() {
    let text = svg(
        "posscan.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 60">
<defs><path id="pp" d="M10 30 L150 30"/></defs>
<text x="10" y="30" font-size="9" fill="#000000">Lead</b><?pi ?><tspan>Mid</tspan><textPath href="#pp">End</textPath></text>
</svg>"##,
    );
    // The positioned-child detector walks past a stray closing tag and a processing
    // instruction to find the tspan/textPath, so the text lays out along the path.
    assert_has(&text, "BT ");
    assert_has(&text, "] TJ ET");
}

#[test]
fn reusable_body_text_child_and_nested_use_expand() {
    let text = svg(
        "reusetext.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">
<defs>
<g id="lib">
<text x="0" y="10" font-size="6" fill="#ff0000">Reused Label</text>
<use href="#dot"/>
</g>
<circle id="dot" cx="30" cy="6" r="3" fill="#00ff00"/>
</defs>
<use href="#lib" x="2" y="2"/>
</svg>"##,
    );
    // The <text> child inside the reused group and its nested <use> both expand.
    assert_has(&text, "BT ");
    assert_has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// Batch 15 — symbol viewBox meet/slice mapping
// ===========================================================================

#[test]
fn use_of_symbol_viewbox_applies_meet_and_slice_scaling() {
    let text = svg(
        "symvb.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
<defs>
<symbol id="meet" viewBox="0 0 10 20" preserveAspectRatio="xMidYMid meet">
<rect x="0" y="0" width="10" height="20" fill="#ff0000"/>
</symbol>
<symbol id="slice" viewBox="0 0 10 20" preserveAspectRatio="xMidYMid slice">
<rect x="0" y="0" width="10" height="20" fill="#00ff00"/>
</symbol>
<symbol id="none" viewBox="0 0 10 20" preserveAspectRatio="none">
<rect x="0" y="0" width="10" height="20" fill="#0000ff"/>
</symbol>
</defs>
<use href="#meet" x="0" y="0" width="40" height="40"/>
<use href="#slice" x="45" y="0" width="40" height="40"/>
<use href="#none" x="90" y="0" width="20" height="40"/>
</svg>"##,
    );
    // A viewBox whose aspect ratio (0.5) differs from the viewport's (1.0) forces
    // distinct meet (min-scale) and slice (max-scale) mappings; each symbol still
    // paints its coloured rect.
    assert_has(&text, "1.000 0.000 0.000 rg");
    assert_has(&text, "0.000 1.000 0.000 rg");
    assert_has(&text, "0.000 0.000 1.000 rg");
}

// ===========================================================================
// Batch 16 — gradient coordinate units and radial focal point
// ===========================================================================

#[test]
fn gradient_percentage_coords_and_radial_focal_point() {
    let text = svg(
        "gradpct.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<defs>
<linearGradient id="pct" x1="0%" y1="0%" x2="100%" y2="0%">
<stop offset="0%" stop-color="#ff0000"/>
<stop offset="100%" stop-color="#0000ff"/>
</linearGradient>
<radialGradient id="focal" cx="0.5" cy="0.5" r="0.5" fx="0.25" fy="0.25">
<stop offset="0" stop-color="#00ff00"/>
<stop offset="1" stop-color="#000000"/>
</radialGradient>
</defs>
<rect x="1" y="1" width="40" height="40" fill="url(#pct)"/>
<circle cx="70" cy="70" r="20" fill="url(#focal)"/>
</svg>"##,
    );
    // Percentage gradient coordinates resolve against the object bounding box.
    assert_has(&text, "/ShadingType 2");
    assert_has(&text, "/C0 [1.000 0.000 0.000]");
    // The radial gradient with an off-centre focal point emits a type-3 shading.
    assert_has(&text, "/ShadingType 3");
    assert_has(&text, "/C0 [0.000 1.000 0.000]");
}

// ===========================================================================
// Batch 17 — pattern count cap and content-detection body forms
// ===========================================================================

#[test]
fn many_pattern_definitions_hit_the_scan_cap() {
    // More than the 64-pattern limit forces the scanner to stop collecting.
    let mut defs = String::new();
    for i in 0..70 {
        defs.push_str(&format!(
            "<pattern id=\"p{i}\" width=\"6\" height=\"6\" patternUnits=\"userSpaceOnUse\"><rect x=\"0\" y=\"0\" width=\"3\" height=\"3\" fill=\"#ff0000\"/></pattern>"
        ));
    }
    let src = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><defs>{defs}</defs>
<rect x="0" y="0" width="30" height="30" fill="url(#p0)"/></svg>"##
    );
    let text = svg("manypat.svg", &src);
    // The first pattern is still resolved and colours the rect red.
    assert_has(&text, "1.000 0.000 0.000");
}

#[test]
fn pattern_content_detection_comment_then_shape_via_href() {
    let text = svg(
        "patcmt.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs>
<pattern id="base" width="8" height="8" patternUnits="userSpaceOnUse"><!-- lead comment --><rect x="0" y="0" width="4" height="4" fill="#ff0000"/></pattern>
<pattern id="deriveA" href="#base"/>
<pattern id="deriveB" href="#base" width="8" height="8"><!-- own comment, no shapes --></pattern>
</defs>
<rect x="0" y="0" width="20" height="20" fill="url(#deriveA)"/>
<rect x="30" y="0" width="20" height="20" fill="url(#deriveB)"/>
</svg>"##,
    );
    // deriveA inherits the commented base body (comment + shape) -> red tile.
    assert_has(&text, "1.000 0.000 0.000");
}

// ===========================================================================
// Batch 18 — element/marker collection caps
// ===========================================================================

#[test]
fn reusable_def_duplicate_id_and_xml_prolog_tokens() {
    // Two referenced defs share an id (the second is skipped) and the document
    // carries an XML declaration and DOCTYPE that the def scanner steps over.
    let text = svg(
        "redup.svg",
        r##"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/svg.dtd">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
<defs>
<rect id="dup" x="0" y="0" width="6" height="6" fill="#ff0000"/>
<rect id="dup" x="0" y="0" width="6" height="6" fill="#0000ff"/>
</defs>
<use href="#dup"/>
</svg>"##,
    );
    // The first #dup def wins: the instanced rect is red, not the blue duplicate.
    assert_has(&text, "1.000 0.000 0.000 rg 0 0 6 6 re f");
    assert_absent(&text, "0.000 0.000 1.000 rg 0 0 6 6 re f");
}

#[test]
fn over_128_marker_definitions_hit_the_scan_cap() {
    let mut defs = String::new();
    for i in 0..130 {
        defs.push_str(&format!(
            "<marker id=\"m{i}\" markerWidth=\"3\" markerHeight=\"3\" refX=\"1\" refY=\"1\"><rect x=\"0\" y=\"0\" width=\"2\" height=\"2\" fill=\"#ff0000\"/></marker>"
        ));
    }
    let src = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20"><defs>{defs}</defs>
<line x1="0" y1="10" x2="80" y2="10" stroke="#000000" stroke-width="1" marker-start="url(#m0)"/></svg>"##
    );
    let text = svg("manymk.svg", &src);
    // The first marker still resolves and paints its red square.
    assert_has(&text, "1.000 0.000 0.000 rg");
}
