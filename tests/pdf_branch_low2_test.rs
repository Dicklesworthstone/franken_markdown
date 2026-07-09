//! Second-wave branch-coverage tests for the SVG parsing / CSS / transform /
//! path / colour subsystem in the first half (source lines < 12700) of
//! `src/pdf.rs`.
//!
//! These pick up the *residual* edge arms left uncovered after
//! `tests/pdf_branch_low_test.rs` and `tests/pdf_coverage_test.rs`: alternate
//! transform keywords (skewX/skewY, degenerate `translate()`), relative path
//! command implicit-line/quadratic arms, elliptical-arc degeneracies, the
//! `color-mix()` split/weight/percentage helpers, `rgb()` slash/comma-alpha
//! forms, root-background compositing, and the many "scan cap" ceilings the
//! happy-path suite never reaches.
//!
//! Every SVG is delivered through the public `render_pdf` API as a Markdown
//! image asset; the vector stream is emitted uncompressed so we pin the exact
//! PDF operator substrings (`rg`/`RG`, `cm`, `m`/`l`/`c`, `sh`, `/ca`) each arm
//! produces.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfImageAsset, PdfOptions, render_pdf};

/// Render `![alt](name)` with a single supplied SVG asset and return the raw
/// PDF as lossy UTF-8. Also asserts byte-for-byte determinism.
fn svg(name: &str, body: &str) -> String {
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(name, body.as_bytes().to_vec())],
        ..PdfOptions::default()
    };
    let a = render_pdf(&format!("![d]({name})"), &opts).unwrap();
    let b = render_pdf(&format!("![d]({name})"), &opts).unwrap();
    assert_eq!(a, b, "SVG render must be deterministic: {name}");
    String::from_utf8_lossy(&a).into_owned()
}

#[track_caller]
fn has(text: &str, needle: &str) {
    assert!(
        text.contains(needle),
        "expected PDF to contain {needle:?}\n--- window ---\n{}",
        window(text, needle)
    );
}

#[track_caller]
fn absent(text: &str, needle: &str) {
    assert!(
        !text.contains(needle),
        "expected PDF NOT to contain {needle:?}"
    );
}

fn window(text: &str, _needle: &str) -> String {
    let start = text.find(" re f").or_else(|| text.find(" m ")).unwrap_or(0);
    let mut lo = start.saturating_sub(200);
    let mut hi = (start + 500).min(text.len());
    while lo > 0 && !text.is_char_boundary(lo) {
        lo -= 1;
    }
    while hi < text.len() && !text.is_char_boundary(hi) {
        hi += 1;
    }
    text[lo..hi].to_string()
}

/// Wrap a set of SVG child elements in a standard root with a viewBox.
fn doc(children: &str) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{children}</svg>"##)
}

// ===========================================================================
// Transforms: skew keywords, is_identity short-circuit tails, empty argument
// lists, and separator handling.
// ===========================================================================

#[test]
fn transform_skew_x_makes_identity_check_fail_on_c_component() {
    // skewX yields matrix(1,0,tan,1,0,0): a==1, b==0, c!=0 -> the `c` arm of
    // `is_identity` (line 351) evaluates false, so a `cm` transform is emitted.
    let text = svg(
        "skewx.svg",
        &doc(
            r##"<rect x="0" y="0" width="10" height="10" fill="#ff0000" transform="skewX(45)"/>"##,
        ),
    );
    // tan(45deg) == 1, so the shear coefficient is 1.000 in the c slot.
    has(&text, "1 0 1 1 0 0 cm"); // skewX(45): c=tan45~=1
    has(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn transform_skew_y_emits_b_component_matrix() {
    // skewY(45) -> matrix(1,tan,0,1,0,0): exercises the `skewy` transform arm
    // (line 12183) and the `b` slot of the emitted `cm`.
    let text = svg(
        "skewy.svg",
        &doc(
            r##"<rect x="0" y="0" width="10" height="10" fill="#0000ff" transform="skewY(45)"/>"##,
        ),
    );
    has(&text, "1 1 0 1 0 0 cm"); // skewY(45): b=tan45~=1
    has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn transform_scale_single_axis_makes_identity_check_fail_on_d_component() {
    // scale(1,2) -> matrix(1,0,0,2,0,0): a==1,b==0,c==0,d!=1 -> the `d` arm of
    // is_identity (line 352) evaluates false.
    let text = svg(
        "scaley.svg",
        &doc(
            r##"<rect x="0" y="0" width="10" height="10" fill="#00ff00" transform="scale(1,2)"/>"##,
        ),
    );
    has(&text, "1 0 0 2 0 0 cm");
}

#[test]
fn transform_empty_argument_lists_drop_the_transform() {
    // `translate()` and `scale()` have empty number lists, so their `if
    // !nums.is_empty()` guards (lines 12166/12169) are false and the whole
    // transform parse bails to identity: no `cm` is emitted for the shape.
    let text = svg(
        "emptyargs.svg",
        &doc(
            r##"<rect x="3" y="3" width="4" height="4" fill="#ff0000" transform="translate()"/>
<rect x="20" y="20" width="4" height="4" fill="#00ff00" transform="scale()"/>"##,
        ),
    );
    // Both rects still paint at their literal coordinates (identity transform).
    has(&text, "1.000 0.000 0.000 rg 3 3 4 4 re f");
    has(&text, "0.000 1.000 0.000 rg 20 20 4 4 re f");
}

#[test]
fn transform_leading_separators_before_function_are_skipped() {
    // A transform value that opens with comma/space separators exercises the
    // separator-skipping loop (line 12198) before the first function name.
    let text = svg(
        "sep.svg",
        &doc(
            r##"<rect x="0" y="0" width="6" height="6" fill="#ff0000" transform=" , translate(10,10)"/>"##,
        ),
    );
    has(&text, "1 0 0 1 10 10 cm");
}

// ===========================================================================
// Path data: relative implicit line after `m`, relative quadratic, exponent
// signs, and elliptical-arc degeneracies.
// ===========================================================================

#[test]
fn path_lowercase_moveto_extra_pairs_become_relative_lines() {
    // A lowercase `m` with trailing coordinate pairs treats them as *relative*
    // line-tos (line 12250 `line_cmd == 'l'` true branch).
    let text = svg(
        "mrel.svg",
        &doc(r##"<path d="m10,10 5,5 5,-5 z" fill="#ff0000"/>"##),
    );
    has(&text, "10 10 m");
    // 10,10 + 5,5 = 15,15 ; then +5,-5 = 20,10. Pin the accumulated x==20.
    has(&text, "20 ");
}

#[test]
fn path_relative_quadratic_accumulates_control_point() {
    // Lowercase `q` (line 12370) makes the control + endpoint relative to the
    // current point.
    let text = svg(
        "qrel.svg",
        &doc(r##"<path d="M10,10 q10,10 20,0 z" fill="#00ff00"/>"##),
    );
    // A quadratic is emitted as a cubic `c` operator.
    has(&text, " c");
    has(&text, "10 10 m");
}

#[test]
fn path_numbers_accept_signed_exponents() {
    // `1e+1` / `2e-1` exercise the exponent-sign arm of read_svg_number_token
    // (line 12630).
    let text = svg(
        "expnum.svg",
        &doc(r##"<path d="M1e+1,1e+1 L2e+1,1e+1 3e+1,2e-1 z" fill="#0000ff"/>"##),
    );
    // 1e+1 == 10 : the move lands at (10,10).
    has(&text, "10 10 m");
    has(&text, "20 ");
}

#[test]
fn path_arc_with_coincident_endpoints_is_a_noop_segment() {
    // An arc whose endpoint equals the current point hits the coincident
    // start==end guard (line 12474) and emits no curve for that arc.
    let text = svg(
        "arcnoop.svg",
        &doc(r##"<path d="M50,50 A20,20 0 0 1 50,50 L60,60 z" fill="#ff0000"/>"##),
    );
    has(&text, "50 50 m");
    // The trailing explicit line to 60,60 is still emitted.
    has(&text, "60 ");
}

#[test]
fn path_arc_with_zero_radius_degrades_to_straight_line() {
    // rx or ry <= EPSILON (line 12480) turns the arc into a plain line to the
    // endpoint.
    let text = svg(
        "arczero.svg",
        &doc(r##"<path d="M10,10 A0,20 0 0 1 40,40 z" fill="#00ff00"/>"##),
    );
    has(&text, "10 10 m");
    has(&text, "40 40 l");
}

#[test]
fn path_arc_sweep_flag_variants_choose_opposite_delta_directions() {
    // sweep=0 (line 12524 `!sweep && delta > 0`) and sweep=1 (line 12526
    // `sweep && delta < 0`) select opposite arc directions; both render curves.
    let cw = svg(
        "arccw.svg",
        &doc(
            r##"<path d="M10,50 A20,20 0 0 0 50,50" fill="none" stroke="#ff0000" stroke-width="2"/>"##,
        ),
    );
    let ccw = svg(
        "arcccw.svg",
        &doc(
            r##"<path d="M10,50 A20,20 0 0 1 50,50" fill="none" stroke="#0000ff" stroke-width="2"/>"##,
        ),
    );
    has(&cw, " c");
    has(&ccw, " c");
    // The two sweeps produce different curve control points.
    assert_ne!(cw, ccw, "opposite sweep flags must render distinct arcs");
}

// ===========================================================================
// color-mix(): srgb-space gate, part-count gate, weight/percentage helpers,
// transparent-alpha collapse, and the top-level slash/comma splitters.
// ===========================================================================

#[test]
fn color_mix_fill_blends_two_colors_by_weight() {
    // Happy path through parse_svg_color_mix: red 25% / blue -> 25% red, 75%
    // blue mix.
    let text = svg(
        "cmix.svg",
        &doc(
            r##"<rect x="0" y="0" width="8" height="8" fill="color-mix(in srgb, #ff0000 25%, #0000ff)"/>"##,
        ),
    );
    // 0.25*1 red, 0.75*1 blue.
    has(&text, "0.250 0.000 0.750 rg");
}

#[test]
fn color_mix_rejects_non_srgb_space_and_wrong_part_count() {
    // `in lab` fails svg_color_mix_space_is_srgb; a 4-part list fails the
    // `parts.len() != 3` gate. Both fall back to the inherited black fill.
    let text = svg(
        "cmixbad.svg",
        &doc(
            r##"<rect x="0" y="0" width="8" height="8" fill="color-mix(in lab, red, blue)"/>
<rect x="10" y="0" width="8" height="8" fill="color-mix(in srgb, red, green, blue)"/>"##,
        ),
    );
    has(&text, "0.000 0.000 0.000 rg 0 0 8 8 re f");
    has(&text, "0.000 0.000 0.000 rg 10 0 8 8 re f");
}

#[test]
fn color_mix_over_percentage_weight_is_rejected() {
    // A single explicit weight above 100% has no valid complement -> the
    // svg_color_mix_weights `first <= 1.0` guard (line 12038) is false and the
    // mix is dropped, leaving black.
    let text = svg(
        "cmixover.svg",
        &doc(
            r##"<rect x="0" y="0" width="8" height="8" fill="color-mix(in srgb, red 150%, blue)"/>"##,
        ),
    );
    has(&text, "0.000 0.000 0.000 rg 0 0 8 8 re f");
}

#[test]
fn color_mix_nested_rgb_commas_stay_inside_parentheses() {
    // A comma inside `rgb(...)` is at paren-depth 1, so split_svg_top_level_commas
    // does not split there (line 11910 guard false); the mix still resolves.
    let text = svg(
        "cmixnest.svg",
        &doc(
            r##"<rect x="0" y="0" width="8" height="8" fill="color-mix(in srgb, rgb(255,0,0) 50%, rgb(0,0,255) 50%)"/>"##,
        ),
    );
    has(&text, "0.500 0.000 0.500 rg");
}

#[test]
fn root_background_color_mix_composites_over_base() {
    // color-mix used as a root background token goes through the
    // color-over-background compositor (lines 4079-4092): a 50/50 red/blue mix
    // paints a full-viewport purple rectangle behind the content.
    let text = svg(
        "bgmix.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:color-mix(in srgb, #ff0000 50%, #0000ff 50%)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "0.500 0.000 0.500 rg");
    has(&text, "0.000 1.000 0.000 rg 5 5 6 6 re f");
}

#[test]
fn root_background_color_mix_all_transparent_falls_back_to_base() {
    // Both components transparent -> mixed alpha collapses to <= 0.001 (line
    // 4091), so the compositor returns the base colour; the foreground rect
    // still paints normally.
    let text = svg(
        "bgmixclear.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:color-mix(in srgb, transparent 50%, transparent 50%)"><rect x="5" y="5" width="6" height="6" fill="#123456"/></svg>"##,
    );
    // #123456 -> 0.071 0.204 0.337
    has(&text, "0.071 0.204 0.337 rg 5 5 6 6 re f");
}

// ===========================================================================
// rgb()/rgba() alpha edge forms: comma part counts and the top-level slash
// splitter's double-slash arm.
// ===========================================================================

#[test]
fn rgb_two_and_five_comma_parts_are_rejected() {
    // comma_parts.len() of 2 or 5 hits neither the 3/4 arm nor the `== 1` arm
    // (line 11802 false branch) -> None -> inherited black fill.
    let text = svg(
        "rgbparts.svg",
        &doc(r##"<rect x="0" y="0" width="8" height="8" fill="rgb(1,2)"/>
<rect x="10" y="0" width="8" height="8" fill="rgb(1,2,3,4,5)"/>"##),
    );
    has(&text, "0.000 0.000 0.000 rg 0 0 8 8 re f");
    has(&text, "0.000 0.000 0.000 rg 10 0 8 8 re f");
}

#[test]
fn rgb_double_slash_alpha_is_rejected() {
    // Two top-level slashes trip split_svg_top_level_slash's `slash.is_some()`
    // arm (line 11936) -> None -> black fill.
    let text = svg(
        "rgbslash.svg",
        &doc(r##"<rect x="0" y="0" width="8" height="8" fill="rgb(10 20 30 / 50% / 10%)"/>"##),
    );
    has(&text, "0.000 0.000 0.000 rg 0 0 8 8 re f");
}

// ===========================================================================
// preserveAspectRatio + viewBox edge forms.
// ===========================================================================

#[test]
fn preserve_aspect_ratio_defer_and_none_keywords() {
    // `defer none` -> the `defer` branch consumes the first token then reads
    // `none` (SvgAspectScaleMode::None). Renders content without letterbox
    // scaling clamps.
    let text = svg(
        "par.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" width="80" height="40" preserveAspectRatio="defer none"><rect x="0" y="0" width="40" height="40" fill="#ff0000"/></svg>"##,
    );
    has(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn preserve_aspect_ratio_slice_meet_alignment_variants() {
    // xMaxYMax slice exercises the align parse (max/max) and the `slice` mode
    // token arm.
    let text = svg(
        "parslice.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" width="80" height="80" preserveAspectRatio="xMaxYMax slice"><rect x="0" y="0" width="40" height="40" fill="#00ff00"/></svg>"##,
    );
    has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// Font / text style keyword arms.
// ===========================================================================

#[test]
fn font_style_oblique_maps_to_italic_slant() {
    // `oblique` reaches the `starts_with("oblique")` arm (line 11088); text is
    // still emitted.
    let text = svg(
        "oblique.svg",
        &doc(
            r##"<text x="10" y="20" font-size="10" font-style="oblique" fill="#ff0000">Hi</text>"##,
        ),
    );
    has(&text, "Tf");
    has(&text, "TJ");
}

#[test]
fn font_family_named_body_family_selects_body_font() {
    // A recognised body family ("Georgia") reaches svg_font_family_is_body true
    // (line 11118).
    let text = svg(
        "bodyfam.svg",
        &doc(
            r##"<text x="10" y="20" font-size="10" font-family="Georgia, serif" fill="#0000ff">Hey</text>"##,
        ),
    );
    has(&text, "TJ");
}

#[test]
fn xml_space_default_keyword_collapses_whitespace() {
    // `xml:space="default"` hits the `else if ... "default"` arm (line 11219).
    let text = svg(
        "xmlspace.svg",
        &doc(
            r##"<text x="10" y="20" font-size="10" xml:space="default" fill="#ff0000">a   b</text>"##,
        ),
    );
    has(&text, "TJ");
}

// ===========================================================================
// Paint-order duplicate keyword arms.
// ===========================================================================

#[test]
fn paint_order_duplicate_tokens_are_rejected_to_default_order() {
    // Duplicate `fill fill` trips the `if !seen_fill` guard-false path (line
    // 10880) -> parse returns None -> default paint order (fill, then stroke).
    let dup = svg(
        "poduph.svg",
        &doc(
            r##"<rect x="0" y="0" width="20" height="20" fill="#ff0000" stroke="#0000ff" stroke-width="4" paint-order="fill fill"/>"##,
        ),
    );
    // A custom valid order stroke-before-fill renders differently.
    let custom = svg(
        "pocustom.svg",
        &doc(
            r##"<rect x="0" y="0" width="20" height="20" fill="#ff0000" stroke="#0000ff" stroke-width="4" paint-order="stroke fill"/>"##,
        ),
    );
    has(&dup, "1.000 0.000 0.000 rg");
    has(&custom, "1.000 0.000 0.000 rg");
    assert_ne!(
        dup, custom,
        "custom paint order must reorder fill/stroke ops"
    );
}

#[test]
fn paint_order_duplicate_stroke_and_markers_reject() {
    let text = svg(
        "podupss.svg",
        &doc(
            r##"<rect x="0" y="0" width="10" height="10" fill="#00ff00" stroke="#000000" stroke-width="2" paint-order="stroke stroke"/>
<line x1="0" y1="0" x2="20" y2="20" stroke="#ff0000" stroke-width="2" paint-order="markers markers"/>"##,
        ),
    );
    has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// Scan caps: exercise the ">= N" ceilings by feeding many definitions.
// ===========================================================================

#[test]
fn many_linear_gradient_definitions_hit_the_scan_cap() {
    // >256 <linearGradient> defs trip the definition scan cap (line 7088). The
    // first gradient (within cap) still registers a shading.
    let mut defs = String::new();
    for i in 0..300 {
        defs.push_str(&format!(
            r##"<linearGradient id="g{i}"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient>"##
        ));
    }
    let body = format!(
        r##"<defs>{defs}</defs><rect x="0" y="0" width="10" height="10" fill="url(#g0)"/>"##
    );
    let text = svg("gradcap.svg", &doc(&body));
    has(&text, "/ShadingType 2");
}

#[test]
fn many_pattern_definitions_hit_the_scan_cap() {
    // >128 <pattern> defs trip the pattern scan cap (line 6906).
    let mut defs = String::new();
    for i in 0..160 {
        defs.push_str(&format!(
            r##"<pattern id="p{i}" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#ff0000"/></pattern>"##
        ));
    }
    let body = format!(
        r##"<defs>{defs}</defs><rect x="0" y="0" width="20" height="20" fill="url(#p0)"/>"##
    );
    let text = svg("patcap.svg", &doc(&body));
    has(&text, " re f");
}

#[test]
fn many_clip_paths_and_masks_hit_their_scan_caps() {
    // >128 <clipPath> and >128 <mask> defs trip lines 8014 / 8128.
    let mut defs = String::new();
    for i in 0..150 {
        defs.push_str(&format!(
            r##"<clipPath id="c{i}"><rect x="0" y="0" width="10" height="10"/></clipPath>"##
        ));
    }
    for i in 0..150 {
        defs.push_str(&format!(
            r##"<mask id="m{i}"><rect x="0" y="0" width="10" height="10" fill="#ffffff"/></mask>"##
        ));
    }
    let body = format!(
        r##"<defs>{defs}</defs><rect x="0" y="0" width="10" height="10" fill="#00ff00" clip-path="url(#c0)"/>"##
    );
    let text = svg("clipcap.svg", &doc(&body));
    has(&text, " re f");
}

#[test]
fn many_use_references_and_reusable_defs_hit_caps() {
    // >256 <use> refs (line 6574) and the reusable-def scan cap (line 6652).
    // The <use> elements live inside <defs> so they are counted by the flat
    // ref/def scanners but never rendered, keeping the content stream small
    // (uncompressed) while still tripping both ceilings. Only the trailing
    // green rect paints.
    let mut inner = String::new();
    for i in 0..300 {
        inner.push_str(&format!(
            r##"<rect id="r{i}" x="0" y="0" width="4" height="4" fill="#ff0000"/>"##
        ));
    }
    for i in 0..300 {
        inner.push_str(&format!(r##"<use href="#r{i}"/>"##));
    }
    let body =
        format!(r##"<defs>{inner}</defs><rect x="0" y="0" width="6" height="6" fill="#00ff00"/>"##);
    let text = svg("usecap.svg", &doc(&body));
    has(&text, "0.000 1.000 0.000 rg 0 0 6 6 re f");
}

#[test]
fn many_marker_shapes_hit_the_marker_body_cap() {
    // >64 shapes inside one <marker> body trip the shapes cap (line 7882).
    let mut shapes = String::new();
    for _ in 0..80 {
        shapes.push_str(r##"<rect x="0" y="0" width="1" height="1"/>"##);
    }
    let body = format!(
        r##"<defs><marker id="mk" markerWidth="4" markerHeight="4" refX="2" refY="2">{shapes}</marker></defs>
<line x1="0" y1="0" x2="40" y2="40" stroke="#ff0000" stroke-width="2" marker-end="url(#mk)"/>"##
    );
    let text = svg("markercap.svg", &doc(&body));
    has(&text, "RG");
}

#[test]
fn many_gradient_stops_hit_the_stop_cap() {
    // >64 <stop> children trip the gradient stop cap (line 7309).
    let mut stops = String::new();
    for i in 0..80 {
        let o = i as f32 / 80.0;
        stops.push_str(&format!(r##"<stop offset="{o}" stop-color="#ff0000"/>"##));
    }
    let body = format!(
        r##"<defs><linearGradient id="g">{stops}</linearGradient></defs>
<rect x="0" y="0" width="40" height="40" fill="url(#g)"/>"##
    );
    let text = svg("stopcap.svg", &doc(&body));
    has(&text, " re");
}

#[test]
fn many_css_variables_and_rules_hit_their_caps() {
    // >256 CSS custom properties (line 6385) and many style rules (lines
    // 6434/7188) trip the CSS scan caps.
    let mut vars = String::new();
    for i in 0..300 {
        vars.push_str(&format!("--v{i}: #ff0000;\n"));
    }
    let mut rules = String::new();
    for i in 0..300 {
        rules.push_str(&format!(".k{i} {{ fill: #00ff00; }}\n"));
    }
    let body = format!(
        r##"<style>:root {{ {vars} }} {rules}</style>
<rect x="0" y="0" width="10" height="10" class="k0" fill="#0000ff"/>"##
    );
    let text = svg("csscap.svg", &doc(&body));
    has(&text, " re f");
}

#[test]
fn css_selector_with_many_classes_hits_the_class_cap() {
    // A compound selector with >8 class segments trips line 8943.
    let body = r##"<style>.a.b.c.d.e.f.g.h.i.j { fill: #ff0000; } </style>
<rect x="0" y="0" width="10" height="10" class="a b c d e f g h i j" fill="#0000ff"/>"##;
    let text = svg("classcap.svg", &doc(body));
    has(&text, " re f");
}

// ===========================================================================
// Batch 2 fixes: transform/arc arms whose *other* side was still missed.
// ===========================================================================

#[test]
fn transform_empty_skew_arguments_drop_the_transform() {
    // `skewY()` has an empty number list -> the `if !nums.is_empty()` guard on
    // the skewy arm (line 12183) is false, so the whole transform bails.
    let text = svg(
        "skewempty.svg",
        &doc(
            r##"<rect x="4" y="4" width="6" height="6" fill="#ff0000" transform="skewY()"/>
<rect x="20" y="20" width="6" height="6" fill="#00ff00" transform="skewX()"/>"##,
        ),
    );
    // Both rects keep identity placement (no cm applied for the dropped skews).
    has(&text, "1.000 0.000 0.000 rg 4 4 6 6 re f");
    has(&text, "0.000 1.000 0.000 rg 20 20 6 6 re f");
}

#[test]
fn path_arc_endpoint_matches_x_but_not_y_is_not_coincident() {
    // start.x == end.x but start.y != end.y: the first coincidence comparison
    // is true while the second is false (line 12474, remaining arm), so the arc
    // still draws curve segments.
    let text = svg(
        "arcxy.svg",
        &doc(
            r##"<path d="M50,50 A20,20 0 0 1 50,80" fill="none" stroke="#ff0000" stroke-width="2"/>"##,
        ),
    );
    has(&text, "50 50 m");
    has(&text, " c");
}

#[test]
fn path_arc_zero_y_radius_degrades_to_line() {
    // rx > EPSILON but ry <= EPSILON (line 12480, remaining arm) -> straight
    // line to the endpoint.
    let text = svg(
        "arcry.svg",
        &doc(r##"<path d="M10,10 A20,0 0 0 1 40,40 z" fill="#00ff00"/>"##),
    );
    has(&text, "10 10 m");
    has(&text, "40 40 l");
}

// ===========================================================================
// Root-background solid colours: transparent / none / zero-alpha and the
// multi-token tokenizer.
// ===========================================================================

#[test]
fn background_color_transparent_and_none_keywords_paint_nothing() {
    // `background-color:transparent` / `none` hit the transparent/none arm
    // (line 3665) returning Some(None): no coloured viewport rectangle, but the
    // foreground rect still paints.
    let t = svg(
        "bgtrans.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background-color:transparent"><rect x="5" y="5" width="6" height="6" fill="#ff0000"/></svg>"##,
    );
    has(&t, "1.000 0.000 0.000 rg 5 5 6 6 re f");
    let n = svg(
        "bgnone.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background-color:none"><rect x="5" y="5" width="6" height="6" fill="#0000ff"/></svg>"##,
    );
    has(&n, "0.000 0.000 1.000 rg 5 5 6 6 re f");
}

#[test]
fn background_color_zero_alpha_collapses_to_no_background() {
    // `rgba(255,0,0,0)` -> paint alpha 0 <= 0.001 (line 3669) -> Some(None).
    let text = svg(
        "bgalpha0.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background-color:rgba(255,0,0,0)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "0.000 1.000 0.000 rg 5 5 6 6 re f");
}

#[test]
fn background_shorthand_multiple_space_tokens_pick_the_color_token() {
    // A multi-token `background` value drives the top-level tokenizer (lines
    // 3693-3717): a leading keyword token is skipped and the colour token wins.
    let text = svg(
        "bgmultitoken.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background: repeat #0000ff"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "0.000 0.000 1.000 rg");
    has(&text, "0.000 1.000 0.000 rg 5 5 6 6 re f");
}

// ===========================================================================
// Root-background CSS gradients: directions, radial descriptors, positions,
// implicit-offset stop interpolation.
// ===========================================================================

#[test]
fn background_linear_gradient_to_right_direction() {
    // `to right` reaches the `to ` prefix arm (line 3851) and the right/bottom
    // direction words; a linear shading is registered.
    let text = svg(
        "bgtoright.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(to right, #ff0000, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "/ShadingType 2");
}

#[test]
fn background_linear_gradient_angle_and_single_stop_rejected() {
    // An angle direction (`45deg`) parses; a one-stop gradient (parts.len() < 2,
    // line 3816) is rejected so no shading is produced for it.
    let ok = svg(
        "bgangle.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(45deg, #ff0000, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&ok, "/ShadingType 2");
    let bad = svg(
        "bg1stop.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(#ff0000)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    // The degenerate gradient yields no shading; the foreground rect still paints.
    absent(&bad, "/ShadingType");
    has(&bad, "0.000 1.000 0.000 rg 5 5 6 6 re f");
}

#[test]
fn background_radial_gradient_descriptor_and_position() {
    // `radial-gradient(circle at right bottom, ...)` reaches the radial branch
    // (line 3802), the ` at ` descriptor (line 3888) and the right/bottom
    // position components (lines 3939/3941). Produces a radial shading.
    let text = svg(
        "bgradial.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:radial-gradient(circle at right bottom, #ff0000, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "/ShadingType 3");
}

#[test]
fn background_radial_gradient_single_stop_rejected() {
    // A radial gradient with one stop trips parts.len() < 2 (line 3835).
    let text = svg(
        "bgradial1.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:radial-gradient(#ff0000)"><rect x="5" y="5" width="6" height="6" fill="#123456"/></svg>"##,
    );
    absent(&text, "/ShadingType 3");
    has(&text, "0.071 0.204 0.337 rg 5 5 6 6 re f");
}

#[test]
fn background_gradient_middle_stop_offset_is_interpolated() {
    // Three stops where the middle has no explicit offset drive the
    // implicit-offset interpolation loop (line 3999): first->0, last->1, middle
    // interpolated to 0.5.
    let text = svg(
        "bg3stop.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(to bottom, #ff0000, #00ff00, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#ffffff"/></svg>"##,
    );
    has(&text, "/ShadingType 2");
}

// ===========================================================================
// color-mix used as a CSS gradient stop -> the color-over-background compositor.
// ===========================================================================

#[test]
fn background_gradient_color_mix_stop_composites_over_base() {
    // A color-mix stop drives parse_svg_css_color_mix_over_background: the
    // happy path (srgb, 3 parts, finite alpha) registers a shading.
    let text = svg(
        "bgmixstop.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(to right, color-mix(in srgb, #ff0000 50%, #0000ff) 0%, #00ff00 100%)"><rect x="5" y="5" width="6" height="6" fill="#ffffff"/></svg>"##,
    );
    has(&text, "/ShadingType 2");
}

#[test]
fn background_gradient_color_mix_stop_bad_space_or_parts_rejects_gradient() {
    // A non-srgb space or wrong part count in a color-mix stop makes the stop
    // (and thus the whole gradient) fail (line 4081), leaving no shading.
    let lab = svg(
        "bgmixlab.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(to right, color-mix(in lab, red, blue) 0%, #00ff00 100%)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    absent(&lab, "/ShadingType 2");
    let parts = svg(
        "bgmixparts.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(to right, color-mix(in srgb, red, green, blue) 0%, #00ff00 100%)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    absent(&parts, "/ShadingType 2");
}

#[test]
fn background_gradient_color_mix_transparent_stop_returns_base() {
    // A fully transparent color-mix stop collapses mixed_alpha to <= 0.001
    // (line 4091) and returns the base colour; the gradient still resolves.
    let text = svg(
        "bgmixtrans.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(to right, color-mix(in srgb, transparent 50%, transparent 50%) 0%, #0000ff 100%)"><rect x="5" y="5" width="6" height="6" fill="#ffffff"/></svg>"##,
    );
    has(&text, "/ShadingType 2");
}

// ===========================================================================
// viewBox + root-geometry degenerate forms.
// ===========================================================================

#[test]
fn viewbox_with_too_few_numbers_falls_back_to_width_height() {
    // A viewBox with 3 numbers fails `nums.len() >= 4` (line 4503) so geometry
    // falls back to width/height; the shape still renders.
    let text = svg(
        "vbshort.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40" width="40" height="40"><rect x="5" y="5" width="6" height="6" fill="#ff0000"/></svg>"##,
    );
    has(&text, "1.000 0.000 0.000 rg 5 5 6 6 re f");
}

#[test]
fn viewbox_with_zero_dimension_falls_back_to_width_height() {
    // viewBox width 0 fails `nums[2] > 0` (line 4503) -> width/height geometry.
    let text = svg(
        "vbzero.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 0 40" width="40" height="40"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "0.000 1.000 0.000 rg 5 5 6 6 re f");
}

#[test]
fn preserve_aspect_ratio_bare_defer_keyword_uses_default() {
    // `defer` with no following alignment token -> the second `tokens.next()`
    // yields None (line 4530) and DEFAULT alignment is used.
    let text = svg(
        "pardefer.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" width="80" height="40" preserveAspectRatio="defer"><rect x="0" y="0" width="40" height="40" fill="#ff0000"/></svg>"##,
    );
    has(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn preserve_aspect_ratio_bad_alignment_token_uses_default() {
    // An alignment token that does not start with `x` (line 4562) fails
    // parse_svg_preserve_aspect_align and DEFAULT alignment is used.
    let text = svg(
        "parbad.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" width="80" height="40" preserveAspectRatio="bogus meet"><rect x="0" y="0" width="40" height="40" fill="#00ff00"/></svg>"##,
    );
    has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// Degenerate embedded <image>.
// ===========================================================================

#[test]
fn embedded_image_zero_width_or_height_is_dropped() {
    // width 0 (line 4997 `w <= 0.0`) and height 0 drop the embedded image
    // before the href is ever decoded; the trailing rect keeps the SVG
    // non-empty. The href carries no literal angle brackets so the outer
    // <image> tag still parses.
    let text = svg(
        "imgzero.svg",
        &doc(
            r##"<image x="0" y="0" width="0" height="10" href="data:image/png;base64,AAAA"/>
<image x="0" y="0" width="10" height="0" href="data:image/png;base64,AAAA"/>
<rect x="20" y="20" width="6" height="6" fill="#ff0000"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg 20 20 6 6 re f");
}

// ===========================================================================
// Embedded <image> data URIs: percent-encoded inline SVG and base64 SVG.
// The href is percent-encoded so no literal angle brackets leak into the
// outer <image> tag.
// ===========================================================================

/// Minimal standard base64 (no line wraps) matching the decoder's alphabet.
fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut chunks = bytes.chunks_exact(3);
    for c in &mut chunks {
        out.push(T[(c[0] >> 2) as usize] as char);
        out.push(T[(((c[0] & 3) << 4) | (c[1] >> 4)) as usize] as char);
        out.push(T[(((c[1] & 15) << 2) | (c[2] >> 6)) as usize] as char);
        out.push(T[(c[2] & 63) as usize] as char);
    }
    match chunks.remainder() {
        [a] => {
            out.push(T[(a >> 2) as usize] as char);
            out.push(T[((a & 3) << 4) as usize] as char);
            out.push_str("==");
        }
        [a, b] => {
            out.push(T[(a >> 2) as usize] as char);
            out.push(T[(((a & 3) << 4) | (b >> 4)) as usize] as char);
            out.push(T[((b & 15) << 2) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

#[test]
fn embedded_image_percent_encoded_inline_svg_decodes_and_renders() {
    // A `data:image/svg+xml,%3Csvg...%3E` href drives decode_svg_svg_data_uri's
    // non-base64 branch (line 5064 else) and decode_svg_data_uri_payload's
    // percent decoder (lines 5075/5085-5088). The nested red rect is rendered
    // inside the outer image viewport.
    let inner = "%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 4 4'%3E%3Crect width='4' height='4' fill='%23ff0000'/%3E%3C/svg%3E";
    let text = svg(
        "imginline.svg",
        &doc(&format!(
            r##"<image x="0" y="0" width="40" height="40" href="data:image/svg+xml,{inner}"/>"##
        )),
    );
    // The nested SVG's red fill appears in the composed output.
    has(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn embedded_image_svg_data_uri_bad_suffix_is_rejected() {
    // `data:image/svg+xmlZ,...` leaves a non-empty, non-`;` suffix (line 5061)
    // so the image is dropped; the trailing rect keeps the doc non-empty.
    let text = svg(
        "imgbadsuffix.svg",
        &doc(
            r##"<image x="0" y="0" width="40" height="40" href="data:image/svg+xmlZ,%3Csvg/%3E"/>
<rect x="10" y="10" width="6" height="6" fill="#0000ff"/>"##,
        ),
    );
    has(&text, "0.000 0.000 1.000 rg 10 10 6 6 re f");
}

#[test]
fn embedded_image_base64_inline_svg_decodes_and_renders() {
    // A `data:image/svg+xml;base64,<b64>` href drives decode_svg_svg_data_uri's
    // base64 branch (line 5064) and decode_svg_base64_payload's padded-quartet
    // arm (lines 5138/5141-5148). The nested green rect renders.
    let inner = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 4 4'><rect width='4' height='4' fill='#00ff00'/></svg>";
    let uri = format!("data:image/svg+xml;base64,{}", b64(inner.as_bytes()));
    let text = svg(
        "imgb64.svg",
        &doc(&format!(
            r##"<image x="0" y="0" width="40" height="40" href="{uri}"/>"##
        )),
    );
    has(&text, "0.000 1.000 0.000 rg");
}

#[test]
fn embedded_image_base64_with_trailing_data_after_padding_is_rejected() {
    // Extra content after a `=` padding terminator trips the `finished` guard
    // (line 5121); the image is dropped.
    let inner = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 4 4'><rect width='4' height='4' fill='#00ff00'/></svg>";
    let mut enc = b64(inner.as_bytes());
    // Guarantee a padding char exists, then append stray data past it.
    if !enc.contains('=') {
        enc.push('=');
    }
    enc.push_str("QUJD");
    let text = svg(
        "imgb64bad.svg",
        &doc(&format!(
            r##"<image x="0" y="0" width="40" height="40" href="data:image/svg+xml;base64,{enc}"/>
<rect x="10" y="10" width="6" height="6" fill="#0000ff"/>"##
        )),
    );
    has(&text, "0.000 0.000 1.000 rg 10 10 6 6 re f");
}

// ===========================================================================
// Radial-gradient descriptor variants + gradient stop positions.
// ===========================================================================

#[test]
fn background_radial_gradient_without_descriptor_uses_default_center() {
    // parts[0] is a stop colour (not a shape/`at` descriptor), so
    // parse_svg_css_radial_gradient_descriptor returns None (line 3839 else)
    // and the default (0.5,0.5) centre is used.
    let text = svg(
        "bgradialnodesc.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:radial-gradient(#ff0000, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "/ShadingType 3");
}

#[test]
fn background_radial_gradient_shape_only_descriptor_no_at_clause() {
    // `circle` with no ` at ` clause fails the split_once (line 3888) and falls
    // through to the shape-keyword match.
    let text = svg(
        "bgradialshape.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:radial-gradient(ellipse farthest-corner, #ff0000, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "/ShadingType 3");
}

#[test]
fn background_radial_gradient_at_left_top_position_components() {
    // `at left top` exercises the left (line 3938) and top (line 3940) position
    // component arms.
    let text = svg(
        "bgatlefttop.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:radial-gradient(circle at left top, #ff0000, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "/ShadingType 3");
}

#[test]
fn background_gradient_transparent_stop_keyword_returns_base() {
    // A literal `transparent` gradient stop hits the transparent/none arm of
    // parse_svg_css_color_over_background (line 4059) returning the base colour.
    let text = svg(
        "bgtransstop.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(to right, transparent 0%, #0000ff 100%)"><rect x="5" y="5" width="6" height="6" fill="#ffffff"/></svg>"##,
    );
    has(&text, "/ShadingType 2");
}

#[test]
fn background_gradient_two_middle_stops_without_offsets_interpolate() {
    // Two consecutive middle stops with no explicit offsets exercise the
    // implicit-offset run loop (line 3999) across a multi-element run.
    let text = svg(
        "bg4stop.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:linear-gradient(to bottom, #ff0000 0%, #00ff00, #0000ff, #ffff00 100%)"><rect x="5" y="5" width="6" height="6" fill="#808080"/></svg>"##,
    );
    has(&text, "/ShadingType 2");
}

// ===========================================================================
// viewBox / preserveAspectRatio remaining arms.
// ===========================================================================

#[test]
fn viewbox_with_zero_height_falls_back_to_width_height() {
    // viewBox height 0 fails `nums[3] > 0` (line 4503, remaining arm).
    let text = svg(
        "vbzeroh.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 0" width="40" height="40"><rect x="5" y="5" width="6" height="6" fill="#ff0000"/></svg>"##,
    );
    has(&text, "1.000 0.000 0.000 rg 5 5 6 6 re f");
}

#[test]
fn preserve_aspect_ratio_empty_value_uses_default() {
    // A whitespace-only preserveAspectRatio yields no first token (line 4522)
    // and DEFAULT alignment is used.
    let text = svg(
        "parempty.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" width="80" height="40" preserveAspectRatio="   "><rect x="0" y="0" width="40" height="40" fill="#00ff00"/></svg>"##,
    );
    has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// Native gradient <stop> colour/opacity edge arms (observable as /C0 /C1).
// ===========================================================================

/// A native linear gradient fill; returns the raw PDF containing the shading.
fn grad(name: &str, stops: &str) -> String {
    svg(
        name,
        &doc(&format!(
            r##"<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0">{stops}</linearGradient></defs>
<rect x="0" y="0" width="40" height="40" fill="url(#g)"/>"##
        )),
    )
}

#[test]
fn gradient_stop_zero_opacity_becomes_white() {
    // stop-opacity 0 (line 7628 `opacity <= 0.001`) forces the stop colour to
    // white regardless of stop-color.
    let text = grad(
        "stopalpha0.svg",
        r##"<stop offset="0" stop-color="#ff0000" stop-opacity="0"/><stop offset="1" stop-color="#0000ff"/>"##,
    );
    has(&text, "/C0 [1.000 1.000 1.000] /C1 [0.000 0.000 1.000]");
}

#[test]
fn gradient_stop_transparent_color_becomes_white() {
    // stop-color:transparent (line 7693) sets opacity 0 -> white stop.
    let text = grad(
        "stoptrans.svg",
        r##"<stop offset="0" stop-color="transparent"/><stop offset="1" stop-color="#00ff00"/>"##,
    );
    has(&text, "/C0 [1.000 1.000 1.000] /C1 [0.000 1.000 0.000]");
}

#[test]
fn gradient_stop_color_alpha_blends_toward_white() {
    // Alpha embedded in stop-color must feed the same white-composited native
    // shading approximation as stop-opacity; it should not be dropped.
    let text = grad(
        "stoprgba.svg",
        r##"<stop offset="0" stop-color="rgba(255 0 0 / 50%)"/><stop offset="1" stop-color="#0000ff"/>"##,
    );
    has(&text, "/C0 [1.000 0.500 0.500] /C1 [0.000 0.000 1.000]");
}

#[test]
fn gradient_stop_color_alpha_multiplies_stop_opacity() {
    // stop-color alpha and stop-opacity are independent SVG properties, so the
    // effective native shading approximation uses their product.
    let text = grad(
        "stoprgbaopacity.svg",
        r##"<stop offset="0" stop-color="rgba(255,0,0,0.5)" stop-opacity="0.5"/><stop offset="1" stop-color="#0000ff"/>"##,
    );
    has(&text, "/C0 [1.000 0.750 0.750] /C1 [0.000 0.000 1.000]");
}

#[test]
fn gradient_stop_transparent_color_stays_transparent_with_stop_opacity() {
    // `transparent` is color alpha 0, not an ordinary black stop whose opacity
    // can be resurrected by a later stop-opacity property.
    let text = grad(
        "stoptransparentopacity.svg",
        r##"<stop offset="0" stop-color="transparent" stop-opacity="0.5"/><stop offset="1" stop-color="#0000ff"/>"##,
    );
    has(&text, "/C0 [1.000 1.000 1.000] /C1 [0.000 0.000 1.000]");
}

#[test]
fn gradient_stop_style_opacity_blends_toward_white() {
    // stop-opacity supplied via the `style` attribute (line 7615): a half-opaque
    // red stop becomes 1.0/0.5/0.5.
    let text = grad(
        "stopstyle.svg",
        r##"<stop offset="0" style="stop-color:#ff0000;stop-opacity:0.5"/><stop offset="1" stop-color="#0000ff"/>"##,
    );
    has(&text, "/C0 [1.000 0.500 0.500]");
}

#[test]
fn gradient_stop_named_color_supported_and_unsupported() {
    // Standard SVG/CSS named stop-colors go through the parse_svg_color branch
    // and resolve; an invalid token still fails to parse so the stop keeps its
    // default black.
    let ok = grad(
        "stopgreen.svg",
        r##"<stop offset="0" stop-color="chartreuse"/><stop offset="1" stop-color="red"/>"##,
    );
    // chartreuse == #7fff00 ; red == #ff0000
    has(&ok, "/C0 [0.498 1.000 0.000] /C1 [1.000 0.000 0.000]");
    let bad = grad(
        "stopunknown.svg",
        r##"<stop offset="0" stop-color="not-a-color"/><stop offset="1" stop-color="#0000ff"/>"##,
    );
    // The invalid token leaves the first stop at default black.
    has(&bad, "/C0 [0.000 0.000 0.000] /C1 [0.000 0.000 1.000]");
}

#[test]
fn standard_named_svg_colors_drive_fill_and_stroke_paint() {
    let text = svg(
        "namedcolors.svg",
        &doc(
            r##"<rect x="4" y="4" width="12" height="8" fill="orange" stroke="gray" stroke-width="2"/>
<rect x="24" y="4" width="12" height="8" fill="rebeccapurple" stroke="lightgrey" stroke-width="2"/>"##,
        ),
    );
    has(&text, "1.000 0.647 0.000 rg");
    has(&text, "0.502 0.502 0.502 RG");
    has(&text, "0.400 0.200 0.600 rg");
    has(&text, "0.827 0.827 0.827 RG");
}

// ===========================================================================
// clipPath units + degenerate clip shapes.
// ===========================================================================

#[test]
fn clip_path_object_bounding_box_units_still_clip() {
    // clipPathUnits="objectBoundingBox" (line 8198) scales the clip to the
    // shape bbox; a clip (`W n`) is emitted and the fill still paints.
    let text = svg(
        "clipobb.svg",
        &doc(
            r##"<defs><clipPath id="c" clipPathUnits="objectBoundingBox"><rect x="0" y="0" width="0.5" height="1"/></clipPath></defs>
<rect x="0" y="0" width="40" height="40" fill="#ff0000" clip-path="url(#c)"/>"##,
        ),
    );
    has(&text, " W n");
    has(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn clip_path_zero_dimension_rect_yields_no_clip_shape() {
    // A clip rect with width 0 (line 8270 `w <= 0.0`) produces no ops; the
    // clipPath is empty so the shape renders unclipped.
    let text = svg(
        "clipzero.svg",
        &doc(
            r##"<defs><clipPath id="c"><rect x="0" y="0" width="0" height="10"/></clipPath></defs>
<rect x="0" y="0" width="20" height="20" fill="#00ff00" clip-path="url(#c)"/>"##,
        ),
    );
    has(&text, "0.000 1.000 0.000 rg");
}

#[test]
fn clip_path_zero_radius_circle_and_ellipse_are_skipped() {
    // clip circle r=0 (line 8289) and clip ellipse rx=0 (line 8306) produce no
    // ops. A valid clip rect keeps a usable clip so the shape renders.
    let text = svg(
        "clipcircle0.svg",
        &doc(
            r##"<defs><clipPath id="c"><circle cx="5" cy="5" r="0"/><ellipse cx="5" cy="5" rx="0" ry="4"/><rect x="0" y="0" width="30" height="30"/></clipPath></defs>
<rect x="0" y="0" width="20" height="20" fill="#0000ff" clip-path="url(#c)"/>"##,
        ),
    );
    has(&text, " W n");
    has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn clip_rule_from_style_declaration_applies() {
    // clip-rule supplied via the shape `style` (lines 8255/8256) selects the
    // even-odd clip operator `W* n`.
    let text = svg(
        "cliprule.svg",
        &doc(
            r##"<defs><clipPath id="c"><path d="M0 0 H30 V30 H0 Z" style="clip-rule:evenodd"/></clipPath></defs>
<rect x="0" y="0" width="20" height="20" fill="#ff0000" clip-path="url(#c)"/>"##,
        ),
    );
    has(&text, "W* n");
}

// ===========================================================================
// Mask shape reveal via style declarations.
// ===========================================================================

#[test]
fn mask_shape_fill_and_opacity_via_style_control_reveal() {
    // A mask rect whose white fill and full opacity come from the `style`
    // attribute (lines 8221/8225) reveals the masked shape.
    let reveal = svg(
        "maskstyle.svg",
        &doc(
            r##"<defs><mask id="m"><rect x="0" y="0" width="40" height="40" style="fill:#ffffff;opacity:1"/></mask></defs>
<rect x="0" y="0" width="30" height="30" fill="#ff0000" mask="url(#m)"/>"##,
        ),
    );
    has(&reveal, "1.000 0.000 0.000 rg");
    // A style fill-opacity that drops the effective alpha hides the shape.
    let hide = svg(
        "maskhide.svg",
        &doc(
            r##"<defs><mask id="m"><rect x="0" y="0" width="40" height="40" style="fill:#ffffff;fill-opacity:0"/></mask></defs>
<rect x="0" y="0" width="30" height="30" fill="#00ff00" mask="url(#m)"/>
<rect x="35" y="35" width="4" height="4" fill="#0000ff"/>"##,
        ),
    );
    // The masked green rect is hidden; the unmasked blue marker remains.
    has(&hide, "0.000 0.000 1.000 rg 35 35 4 4 re f");
    assert_ne!(reveal, hide, "mask reveal vs hide must differ");
}

#[test]
fn mask_shape_fill_color_alpha_controls_reveal_threshold() {
    // Alpha embedded in the mask fill color contributes to the hard-mask
    // luminance threshold exactly like fill-opacity.
    let hide = svg(
        "maskfillalphahide.svg",
        &doc(
            r##"<defs><mask id="m"><rect x="0" y="0" width="40" height="40" fill="rgba(255 255 255 / 49%)"/></mask></defs>
<rect x="0" y="0" width="30" height="30" fill="#00ff00" mask="url(#m)"/>"##,
        ),
    );
    has(&hide, "0 0 0 0 re W n 0.000 1.000 0.000 rg 0 0 30 30 re f");

    let reveal = svg(
        "maskfillalphareveal.svg",
        &doc(
            r##"<defs><mask id="m"><rect x="0" y="0" width="40" height="40" style="fill:rgba(255 255 255 / 50%)"/></mask></defs>
<rect x="0" y="0" width="30" height="30" fill="#ff0000" mask="url(#m)"/>"##,
        ),
    );
    has(
        &reveal,
        "0 0 m 40 0 l 40 40 l 0 40 l h W n 1.000 0.000 0.000 rg 0 0 30 30 re f",
    );
}

#[test]
fn self_closing_mask_hides_instead_of_failing_open() {
    // A valid but empty mask reference must not degrade into "no mask"; it
    // becomes an empty hard clip that suppresses the target.
    let text = svg(
        "maskselfclosing.svg",
        &doc(r##"<defs><mask id="m"/></defs>
<rect x="0" y="0" width="30" height="30" fill="#0000ff" mask="url(#m)"/>
<text x="0" y="45" font-size="10" fill="#ff0000" mask="url(#m)">Hidden</text>"##),
    );
    has(&text, "0 0 0 0 re W n 0.000 0.000 1.000 rg 0 0 30 30 re f");
    has(&text, "0 0 0 0 re W n\n");
}

// ===========================================================================
// base64 data-URI decoder reject arms.
// ===========================================================================

#[test]
fn embedded_image_base64_length_not_multiple_of_four_is_rejected() {
    // A payload whose non-whitespace length is not a multiple of 4 trips line
    // 5107/5108 (`encoded_len % 4 != 0`); the image is dropped.
    let text = svg(
        "b64len.svg",
        &doc(
            r##"<image x="0" y="0" width="40" height="40" href="data:image/svg+xml;base64,ABCDE"/>
<rect x="10" y="10" width="6" height="6" fill="#ff0000"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg 10 10 6 6 re f");
}

#[test]
fn embedded_image_base64_leading_padding_is_rejected() {
    // A quartet whose first symbol is `=` trips the malformed-quartet guard
    // (line 5138 `quartet[0] == 64`).
    let text = svg(
        "b64pad.svg",
        &doc(
            r##"<image x="0" y="0" width="40" height="40" href="data:image/svg+xml;base64,=AAA"/>
<rect x="10" y="10" width="6" height="6" fill="#00ff00"/>"##,
        ),
    );
    has(&text, "0.000 1.000 0.000 rg 10 10 6 6 re f");
}

// ===========================================================================
// Letter-spacing units and baseline-shift numeric form (observable via glyph
// positions: the styled render differs from the unstyled one).
// ===========================================================================

#[test]
fn letter_spacing_ex_and_percent_units_shift_glyphs() {
    // `ex` (line 11266) and `%` (line 11268) letter-spacing units both change
    // the emitted glyph advances relative to the default spacing.
    let base = svg(
        "lsbase.svg",
        &doc(r##"<text x="10" y="20" font-size="10" fill="#ff0000">WIDE</text>"##),
    );
    let ex = svg(
        "lsex.svg",
        &doc(
            r##"<text x="10" y="20" font-size="10" letter-spacing="1ex" fill="#ff0000">WIDE</text>"##,
        ),
    );
    let pct = svg(
        "lspct.svg",
        &doc(
            r##"<text x="10" y="20" font-size="10" letter-spacing="20%" fill="#ff0000">WIDE</text>"##,
        ),
    );
    assert_ne!(base, ex, "ex letter-spacing must change output");
    assert_ne!(base, pct, "percent letter-spacing must change output");
    assert_ne!(ex, pct, "ex and percent spacing differ");
}

#[test]
fn baseline_shift_numeric_value_shifts_text() {
    // A numeric baseline-shift (line 11301 finite branch) shifts the text
    // baseline relative to the default.
    let base = svg(
        "bsbase.svg",
        &doc(r##"<text x="10" y="20" font-size="10" fill="#0000ff">Up</text>"##),
    );
    let shifted = svg(
        "bsshift.svg",
        &doc(r##"<text x="10" y="20" font-size="10" baseline-shift="4" fill="#0000ff">Up</text>"##),
    );
    assert_ne!(base, shifted, "numeric baseline-shift must move the text");
}

// ===========================================================================
// CSS filter: drop-shadow() length units, malformed forms, and filter keyword
// dispatch.
// ===========================================================================

/// Render a rect with a `style="filter:..."` declaration.
fn filtered(name: &str, style_filter: &str) -> String {
    svg(
        name,
        &doc(&format!(
            r##"<rect x="10" y="10" width="20" height="20" fill="#00ff00" style="filter:{style_filter}"/>"##
        )),
    )
}

#[test]
fn drop_shadow_em_unit_equals_twelve_px() {
    // parse_svg_filter_length maps `1em` to 12 user units (line 10828), so a
    // 1em/1em drop-shadow renders byte-identically to a 12px/12px one.
    let em = filtered("dsem.svg", "drop-shadow(1em 1em #123456)");
    let px = filtered("dspx12.svg", "drop-shadow(12px 12px #123456)");
    // The shadow paints in #123456 == 0.071 0.204 0.337.
    has(&em, "0.071 0.204 0.337 rg");
    assert_eq!(em, px, "1em must resolve to 12px in drop-shadow offsets");
}

#[test]
fn drop_shadow_ex_unit_equals_six_px() {
    // `1ex` maps to 6 user units (line 10830).
    let ex = filtered("dsex.svg", "drop-shadow(1ex 1ex #654321)");
    let px = filtered("dspx6.svg", "drop-shadow(6px 6px #654321)");
    has(&ex, "0.396 0.263 0.129 rg");
    assert_eq!(ex, px, "1ex must resolve to 6px in drop-shadow offsets");
}

#[test]
fn drop_shadow_percent_unit_divides_by_hundred() {
    // `100%` maps to 1.0 user unit (line 10832).
    let pct = filtered("dspct.svg", "drop-shadow(100% 100% #ff0000)");
    let px = filtered("dspx1.svg", "drop-shadow(1px 1px #ff0000)");
    has(&pct, "1.000 0.000 0.000 rg");
    assert_eq!(pct, px, "100% must resolve to 1px in drop-shadow offsets");
}

#[test]
fn filter_none_and_empty_keyword_yield_no_shadow() {
    // `filter:none` hits the empty/none arm (line 10716) returning Some(None):
    // the shape renders with no offset shadow, identical to no filter at all.
    let none = filtered("filternone.svg", "none");
    let plain = svg(
        "filterplain.svg",
        &doc(r##"<rect x="10" y="10" width="20" height="20" fill="#00ff00"/>"##),
    );
    assert_eq!(none, plain, "filter:none must render like no filter");
}

#[test]
fn filter_url_reference_in_style_resolves_named_filter() {
    // `filter:url(#f)` in a style declaration (line 10722) resolves a named
    // <filter> that synthesizes a blue drop shadow via feDropShadow.
    let text = svg(
        "filterurl.svg",
        &doc(
            r##"<defs><filter id="f"><feDropShadow dx="3" dy="3" flood-color="#0000ff"/></filter></defs>
<rect x="10" y="10" width="20" height="20" fill="#00ff00" style="filter:url(#f)"/>"##,
        ),
    );
    // The synthesized shadow paints in blue.
    has(&text, "0.000 0.000 1.000 rg");
    has(&text, "0.000 1.000 0.000 rg");
}

#[test]
fn filter_bare_drop_shadow_keyword_falls_back_and_blur_is_ignored() {
    // A bare `drop-shadow` token (no `(` call) does not match the function
    // parser but still satisfies `contains("drop-shadow")` (line 10729), so the
    // fallback shadow is used; an unrelated filter function matches nothing.
    let fallback = filtered("dsfallback.svg", "drop-shadow");
    let blur = filtered("dsblur.svg", "blur(3px)");
    // The fallback shadow renders the shape twice (offset); blur leaves one.
    assert_ne!(
        fallback, blur,
        "fallback drop-shadow must add a shadow layer"
    );
    // The blur-only filter renders like a plain rect.
    let plain = svg(
        "dsplain.svg",
        &doc(r##"<rect x="10" y="10" width="20" height="20" fill="#00ff00"/>"##),
    );
    assert_eq!(blur, plain, "unrecognized filter function is ignored");
}

#[test]
fn drop_shadow_third_length_is_blur_radius_and_color_still_parses() {
    // Three lengths (dx dy blur) plus a colour: the blur length is consumed by
    // the `lengths.len() < 3` guard (line 10767) and the colour still applies.
    let text = filtered("ds3len.svg", "drop-shadow(2px 3px 4px #abcdef)");
    // #abcdef == 0.671 0.808 0.937
    has(&text, "0.671 0.804 0.937 rg");
}

#[test]
fn drop_shadow_many_whitespace_tokens_hit_the_token_cap() {
    // A drop-shadow argument list with more than 16 whitespace tokens trips the
    // token cap in split_svg_top_level_whitespace (line 10800); the first two
    // lengths still form a shadow.
    let text = filtered(
        "dstokens.svg",
        "drop-shadow(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 #ff0000)",
    );
    // The trailing colour token is past the 16-token cap, so the shadow keeps
    // its default black; the green shape still paints.
    has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// Stroke paint alpha via a style declaration (observable as a stroke /CA).
// ===========================================================================

#[test]
fn stroke_hex_alpha_via_style_sets_stroke_ca() {
    // A stroke colour with an 8-digit hex alpha in the `style` attribute
    // (line 9065) sets the stroke-opacity ExtGState.
    let text = svg(
        "strokealpha.svg",
        &doc(
            r##"<rect x="5" y="5" width="20" height="20" fill="none" style="stroke:#ff000080;stroke-width:3"/>"##,
        ),
    );
    // 0x80/255 == 0.502 stroke alpha.
    has(&text, "/CA 0.502");
    has(&text, "1.000 0.000 0.000 RG");
}

// ===========================================================================
// CSS selector parsing: child combinator, and rejected malformed selectors.
// ===========================================================================

#[test]
fn css_child_combinator_selector_matches_direct_child() {
    // `g > rect` builds a Child relation (lines 8842/8845) that matches a rect
    // directly inside a g, recolouring it.
    let text = svg(
        "csschild.svg",
        &doc(r##"<style>g > rect { fill: #ff0000; }</style>
<g><rect x="0" y="0" width="10" height="10" fill="#0000ff"/></g>"##),
    );
    has(&text, "1.000 0.000 0.000 rg");
    absent(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn css_pseudo_class_selector_is_rejected() {
    // A selector containing `:` is rejected outright (line 8820) so the rule
    // never applies; the rect keeps its inline blue fill.
    let text = svg(
        "csspseudo.svg",
        &doc(r##"<style>rect:hover { fill: #ff0000; }</style>
<rect x="0" y="0" width="10" height="10" fill="#0000ff"/>"##),
    );
    has(&text, "0.000 0.000 1.000 rg");
    absent(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn css_selector_empty_class_and_double_id_are_rejected() {
    // A `.` with no following ident (line 8932) and a double-id `#a#b`
    // (line 8937) both fail to parse; neither rule recolours the rect.
    let text = svg(
        "cssbadsel.svg",
        &doc(
            r##"<style>. { fill: #ff0000; } rect#a#b { fill: #ff0000; }</style>
<rect id="a" x="0" y="0" width="10" height="10" fill="#0000ff"/>"##,
        ),
    );
    has(&text, "0.000 0.000 1.000 rg");
    absent(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn css_selector_over_six_parts_is_rejected() {
    // A descendant chain deeper than six parts (line 8860) is rejected.
    let text = svg(
        "cssdeep.svg",
        &doc(r##"<style>svg g g g g g g rect { fill: #ff0000; }</style>
<rect x="0" y="0" width="10" height="10" fill="#0000ff"/>"##),
    );
    has(&text, "0.000 0.000 1.000 rg");
    absent(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn css_selector_trailing_child_combinator_is_rejected() {
    // A `>` with nothing valid after it (line 8842) fails to parse.
    let text = svg(
        "csstrailgt.svg",
        &doc(r##"<style>g > { fill: #ff0000; }</style>
<g><rect x="0" y="0" width="10" height="10" fill="#0000ff"/></g>"##),
    );
    has(&text, "0.000 0.000 1.000 rg");
    absent(&text, "1.000 0.000 0.000 rg");
}

// ===========================================================================
// feDropShadow: style-supplied flood colour/opacity and defaulted offsets.
// ===========================================================================

#[test]
fn fe_drop_shadow_style_flood_color_and_opacity_apply() {
    // feDropShadow with no dx/dy (lines 10643/10646 fall through to the fallback
    // offsets) and flood-color/flood-opacity supplied via `style`
    // (lines 10659/10678/10683) synthesizes a red, half-opaque shadow.
    let text = svg(
        "fedsstyle.svg",
        &doc(
            r##"<defs><filter id="f"><feDropShadow style="flood-color:#ff0000;flood-opacity:0.5"/></filter></defs>
<rect x="20" y="20" width="20" height="20" fill="#00ff00" filter="url(#f)"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, "0.000 1.000 0.000 rg");
    has(&text, "/ca 0.500");
}

#[test]
fn fe_drop_shadow_flood_color_alpha_composes_with_opacity() {
    // Alpha embedded in `flood-color` is part of the shadow flood paint and must
    // multiply with the independent `flood-opacity` property.
    let text = svg(
        "fedscoloralpha.svg",
        &doc(
            r##"<defs><filter id="f"><feDropShadow dx="1" dy="1" flood-color="rgb(255 0 0 / 50%)" flood-opacity="0.5"/></filter></defs>
<rect x="20" y="20" width="20" height="20" fill="#00ff00" filter="url(#f)"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, "/ca 0.250");
    has(&text, "0.000 1.000 0.000 rg");
}

#[test]
fn fe_drop_shadow_style_flood_color_alpha_survives_later_opacity() {
    // The style declaration pass must not overwrite a previously parsed colour
    // alpha when a later `flood-opacity` declaration is applied.
    let text = svg(
        "fedsstylealpha.svg",
        &doc(
            r##"<defs><filter id="f"><feDropShadow style="flood-color:rgb(255 0 0 / 50%);flood-opacity:0.5"/></filter></defs>
<rect x="20" y="20" width="20" height="20" fill="#00ff00" filter="url(#f)"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, "/ca 0.250");
    has(&text, "0.000 1.000 0.000 rg");
}

#[test]
fn filter_primitive_chain_flood_color_alpha_composes_with_opacity() {
    // Manual shadow filter pipelines use <feFlood>, not <feDropShadow>, but
    // they share the same flood paint semantics.
    let text = svg(
        "floodchainalpha.svg",
        &doc(r##"<defs><filter id="f">
<feOffset in="SourceAlpha" dx="1" dy="1" result="off"/>
<feGaussianBlur in="off" stdDeviation="1" result="blur"/>
<feFlood flood-color="rgb(255 0 0 / 50%)" flood-opacity="0.5" result="flood"/>
<feComposite operator="in" in="flood" in2="blur" result="shadow"/>
<feMerge><feMergeNode in="shadow"/><feMergeNode in="SourceGraphic"/></feMerge>
</filter></defs>
<rect x="20" y="20" width="20" height="20" fill="#00ff00" filter="url(#f)"/>"##),
    );
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, "/ca 0.250");
    has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// <use> geometry: x/y translation and the url(#id) href form.
// ===========================================================================

#[test]
fn use_with_x_y_offset_translates_the_instance() {
    // `<use x=.. y=..>` (line 9926) concatenates a translate onto the instance
    // transform, shifting the referenced rect.
    let text = svg(
        "usexy.svg",
        &doc(
            r##"<defs><rect id="r" x="0" y="0" width="8" height="8" fill="#ff0000"/></defs>
<use href="#r" x="10" y="20"/>"##,
        ),
    );
    // The translate(10,20) appears as a cm transform around the instance.
    has(&text, "1 0 0 1 10 20 cm");
    has(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn use_href_url_function_form_resolves_reference() {
    // A `href="url(#r)"` fragment (line 9828 else branch via parse_svg_paint_url_id)
    // still resolves the referenced shape.
    let text = svg(
        "useurl.svg",
        &doc(
            r##"<defs><rect id="r" x="0" y="0" width="8" height="8" fill="#00ff00"/></defs>
<use href="url(#r)"/>"##,
        ),
    );
    has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// Markdown paragraph / table layout-cache ceilings (non-SVG paths).
// ===========================================================================

/// Render plain Markdown (no image assets) and return the PDF as lossy UTF-8.
fn md(markdown: &str) -> String {
    let a = render_pdf(markdown, &PdfOptions::default()).unwrap();
    let b = render_pdf(markdown, &PdfOptions::default()).unwrap();
    assert_eq!(a, b, "markdown render must be deterministic");
    String::from_utf8_lossy(&a).into_owned()
}

#[test]
fn many_distinct_paragraphs_fill_the_simple_paragraph_cache() {
    // More than 256 distinct simple paragraphs push the layout cache past its
    // capacity (line 2286 `entries.len() >= SIMPLE_PARAGRAPH_LAYOUT_CACHE_MAX`);
    // every paragraph still renders.
    let mut doc = String::new();
    for i in 0..300 {
        doc.push_str(&format!("Paragraph number {i} of the document.\n\n"));
    }
    let text = md(&doc);
    // The 300 paragraphs span multiple pages; their combined content is large
    // enough to be FlateDecode-compressed.
    assert!(
        text.matches("/Type /Page ").count() > 1,
        "300 paragraphs should span multiple pages"
    );
    has(&text, "/Filter /FlateDecode");
}

#[test]
fn overlong_paragraph_text_bypasses_the_cache() {
    // A single paragraph whose text exceeds 4096 bytes trips the byte ceiling
    // (line 2287) so it is laid out without being cached; it still renders.
    let mut doc = String::from("word ");
    while doc.len() <= 4200 {
        doc.push_str("word ");
    }
    let text = md(&doc);
    // The long paragraph wraps across many lines and renders as a valid page.
    has(&text, "/Type /Page");
    has(&text, "/Filter /FlateDecode");
}

#[test]
fn wide_table_exceeding_column_cap_still_renders() {
    // A table with more than 256 columns trips the cell-cap key guard
    // (line 2335 `table.align.len() > TABLE_LAYOUT_CACHE_MAX_CELLS`), so the
    // table is laid out without caching but still renders.
    let header = format!("|{}|\n", "h|".repeat(260));
    let sep = format!("|{}|\n", "---|".repeat(260));
    let row = format!("|{}|\n", "c|".repeat(260));
    let text = md(&format!("{header}{sep}{row}"));
    has(&text, "/Type /Page");
}

#[test]
fn tall_table_exceeding_line_cap_still_renders() {
    // A table with more than 512 laid-out lines trips the line ceiling
    // (line 2525 `lines.len() > TABLE_LAYOUT_CACHE_MAX_LINES`).
    let mut doc = String::from("| A | B |\n| --- | --- |\n");
    for i in 0..520 {
        doc.push_str(&format!("| row {i} | value {i} |\n"));
    }
    let text = md(&doc);
    has(&text, "/Type /Page");
}

#[test]
fn many_distinct_tables_fill_the_table_cache() {
    // More than 64 distinct tables push the table layout cache past capacity
    // (line 2523 `entries.len() >= TABLE_LAYOUT_CACHE_MAX`).
    let mut doc = String::new();
    for i in 0..70 {
        doc.push_str(&format!(
            "| H{i} | K{i} |\n| --- | --- |\n| a{i} | b{i} |\n\ngap {i}\n\n"
        ));
    }
    let text = md(&doc);
    has(&text, "/Type /Page");
}

// ===========================================================================
// Radial gradient position words on the crossed axis.
// ===========================================================================

#[test]
fn background_radial_position_reversed_axis_words() {
    // `at top left` puts a vertical word (`top`) in the x slot and a horizontal
    // word (`left`) in the y slot, exercising the guard-false paths of the
    // position component match arms (lines 3938/3940).
    // The reversed-axis position words fail to resolve, so the descriptor and
    // thus the gradient are rejected; the foreground green rect still paints.
    let text = svg(
        "bgtopleft.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:radial-gradient(circle at top left, #ff0000, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "0.000 1.000 0.000 rg 5 5 6 6 re f");
    let br = svg(
        "bgbottomright.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background:radial-gradient(circle at bottom right, #ff0000, #0000ff)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&br, "0.000 1.000 0.000 rg 5 5 6 6 re f");
}

// ===========================================================================
// Quoted url(#id) paint references.
// ===========================================================================

#[test]
fn paint_url_reference_with_quotes_resolves_the_gradient() {
    // Single- and double-quoted ids inside url(...) exercise the quote-stripping
    // arms of parse_svg_paint_url_id (lines 11695/11696); both resolve the
    // gradient and register its shading.
    let single = svg(
        "urlsingle.svg",
        &doc(
            r##"<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs>
<rect x="0" y="0" width="20" height="20" fill="url('#g')"/>"##,
        ),
    );
    has(&single, "/ShadingType 2");
    let double = svg(
        "urldouble.svg",
        &doc(
            r##"<defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='0'><stop offset='0' stop-color='#ff0000'/><stop offset='1' stop-color='#0000ff'/></linearGradient></defs>
<rect x='0' y='0' width='20' height='20' fill='url("#g")'/>"##,
        ),
    );
    has(&double, "/ShadingType 2");
}

// ===========================================================================
// CSS custom-property var() fallbacks.
// ===========================================================================

#[test]
fn css_var_with_non_custom_name_uses_fallback() {
    // `var(bad, #ff0000)` has a name not starting with `--` (line 11651) so the
    // fallback colour is used.
    let text = svg(
        "varbadname.svg",
        &doc(r##"<rect x="0" y="0" width="10" height="10" fill="var(notdashed, #ff0000)"/>"##),
    );
    has(&text, "1.000 0.000 0.000 rg 0 0 10 10 re f");
}

#[test]
fn css_var_undefined_custom_property_uses_comma_fallback() {
    // `var(--missing, #00ff00)` splits name/fallback at the top-level comma
    // (line 11666); the undefined property falls back to green.
    let text = svg(
        "varfallback.svg",
        &doc(r##"<rect x="0" y="0" width="10" height="10" fill="var(--missing, #00ff00)"/>"##),
    );
    has(&text, "0.000 1.000 0.000 rg 0 0 10 10 re f");
}

#[test]
fn css_var_resolves_font_style_keyword() {
    // A `var(--slant)` font-style (line 11081) resolves through the custom
    // property table; the italic result changes the rendered text.
    let normal = svg(
        "varfsnormal.svg",
        &doc(r##"<text x="10" y="20" font-size="10" fill="#ff0000">Ay</text>"##),
    );
    let italic = svg(
        "varfsitalic.svg",
        &doc(
            r##"<style>:root { --slant: italic; }</style><text x="10" y="20" font-size="10" font-style="var(--slant)" fill="#ff0000">Ay</text>"##,
        ),
    );
    assert_ne!(normal, italic, "var()-resolved italic must change the text");
}

// ===========================================================================
// color-mix single explicit weight + non-srgb space.
// ===========================================================================

#[test]
fn color_mix_single_second_weight_infers_complement() {
    // Only the second colour carries a weight: `(None, Some(0.3))` with the
    // second <= 1.0 arm (line 12039) infers a 0.7/0.3 split.
    let text = svg(
        "cmixsecond.svg",
        &doc(
            r##"<rect x="0" y="0" width="8" height="8" fill="color-mix(in srgb, #ff0000, #0000ff 30%)"/>"##,
        ),
    );
    // 0.7 red + 0.3 blue.
    has(&text, "0.700 0.000 0.300 rg");
}

#[test]
fn color_mix_non_srgb_space_word_is_rejected() {
    // `in hsl` is a well-formed two-word space that is not srgb (line 11955),
    // so the mix is rejected and the fill stays black.
    let text = svg(
        "cmixhsl.svg",
        &doc(r##"<rect x="0" y="0" width="8" height="8" fill="color-mix(in hsl, red, blue)"/>"##),
    );
    has(&text, "0.000 0.000 0.000 rg 0 0 8 8 re f");
}

// ===========================================================================
// Paint keyword `none` and url(...) fallback colour.
// ===========================================================================

#[test]
fn fill_url_missing_gradient_falls_back_to_trailing_color() {
    // `url(#missing) #ff0000` has no matching gradient, so the trailing colour
    // after `)` is parsed as the fallback (line 11581).
    let text = svg(
        "urlfallback.svg",
        &doc(r##"<rect x="0" y="0" width="10" height="10" fill="url(#missing) #ff0000"/>"##),
    );
    has(&text, "1.000 0.000 0.000 rg 0 0 10 10 re f");
}

// ===========================================================================
// Stroke miter-limit, empty dash array, vector-effect.
// ===========================================================================

#[test]
fn stroke_miter_limit_below_one_is_ignored() {
    // A miter-limit below 1.0 fails the `>= 1.0` guard (line 10959); the stroke
    // still renders (with the default miter limit).
    let low = svg(
        "miterlow.svg",
        &doc(
            r##"<path d="M0,0 L20,0 L20,20" fill="none" stroke="#ff0000" stroke-width="4" stroke-miterlimit="0.5" stroke-linejoin="miter"/>"##,
        ),
    );
    has(&low, "1.000 0.000 0.000 RG");
    // A valid miter-limit renders differently (emits an M operator value).
    let ok = svg(
        "miterok.svg",
        &doc(
            r##"<path d="M0,0 L20,0 L20,20" fill="none" stroke="#ff0000" stroke-width="4" stroke-miterlimit="8" stroke-linejoin="miter"/>"##,
        ),
    );
    assert_ne!(low, ok, "an accepted miter-limit must change stroke setup");
}

#[test]
fn stroke_vector_effect_non_scaling_is_parsed() {
    // vector-effect="non-scaling-stroke" (line 11027) is recognised; the stroke
    // renders.
    let text = svg(
        "vectoreffect.svg",
        &doc(
            r##"<rect x="2" y="2" width="16" height="16" fill="none" stroke="#0000ff" stroke-width="2" vector-effect="non-scaling-stroke"/>"##,
        ),
    );
    has(&text, "0.000 0.000 1.000 RG");
}

// ===========================================================================
// SVG accessible text (<title>/<desc>) feeding the image /Alt, with nested
// groups and a duplicate <desc>.
// ===========================================================================

/// Render `![](name)` with an EMPTY markdown alt so the SVG's own accessible
/// text becomes the figure's /Alt.
fn svg_noalt(name: &str, body: &str) -> String {
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(name, body.as_bytes().to_vec())],
        ..PdfOptions::default()
    };
    let bytes = render_pdf(&format!("![]({name})"), &opts).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn accessible_desc_only_survives_nested_group_and_duplicate_desc() {
    // The scanner walks into a nested <g> (closing `</g>` hits line 4249), takes
    // the first <desc>, then skips a second <desc> because desc is already set
    // (line 4284 guard-false). With no <title>, the desc alone becomes /Alt.
    let text = svg_noalt(
        "accdesc.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><g><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></g><desc>First Desc</desc><desc>Second Desc</desc></svg>"##,
    );
    has(&text, "/Alt (First Desc)");
    absent(&text, "Second Desc");
}

#[test]
fn accessible_aria_label_with_desc_combines_over_nested_content() {
    // aria-label supplies the name and <desc> is appended with " - " even when
    // the root has nested children the scanner must descend through.
    let text = svg_noalt(
        "accaria.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" aria-label="Aria Name"><g><g><rect x="0" y="0" width="8" height="8" fill="#00ff00"/></g></g><desc>Deep Desc</desc></svg>"##,
    );
    has(&text, "/Alt (Aria Name - Deep Desc)");
}

// ===========================================================================
// Markers: viewBox mapping and angle-unit orient forms.
// ===========================================================================

#[test]
fn marker_viewbox_maps_marker_content_to_marker_dimensions() {
    // A marker with a viewBox and markerWidth/markerHeight (line 7903) maps its
    // child content into the marker viewport at each vertex.
    let text = svg(
        "markervb.svg",
        &doc(
            r##"<defs><marker id="m" viewBox="0 0 10 10" markerWidth="6" markerHeight="6" refX="5" refY="5" orient="auto"><circle cx="5" cy="5" r="4" fill="#ff0000"/></marker></defs>
<line x1="10" y1="10" x2="40" y2="40" stroke="#000000" stroke-width="2" marker-end="url(#m)"/>"##,
        ),
    );
    // The marker's red circle renders as filled curves at the line end.
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, " c");
}

#[test]
fn marker_orient_radian_and_turn_units_are_converted() {
    // orient in radians (line 7936) and turns (line 7940) both parse to a finite
    // angle; the marker renders at each configured vertex.
    let rad = svg(
        "markerrad.svg",
        &doc(
            r##"<defs><marker id="m" markerWidth="4" markerHeight="4" refX="2" refY="2" orient="1.5rad"><rect x="0" y="0" width="4" height="4" fill="#0000ff"/></marker></defs>
<polyline points="0,0 20,20 40,0" fill="none" stroke="#000000" stroke-width="2" marker-start="url(#m)"/>"##,
        ),
    );
    has(&rad, "0.000 0.000 1.000 rg");
    let turn = svg(
        "markerturn.svg",
        &doc(
            r##"<defs><marker id="m" markerWidth="4" markerHeight="4" refX="2" refY="2" orient="0.25turn"><rect x="0" y="0" width="4" height="4" fill="#00ff00"/></marker></defs>
<polyline points="0,0 20,20 40,0" fill="none" stroke="#000000" stroke-width="2" marker-start="url(#m)"/>"##,
        ),
    );
    has(&turn, "0.000 1.000 0.000 rg");
}

#[test]
fn marker_orient_grad_unit_is_converted() {
    // orient in gradians (line 7938) converts to degrees.
    let text = svg(
        "markergrad.svg",
        &doc(
            r##"<defs><marker id="m" markerWidth="4" markerHeight="4" refX="2" refY="2" orient="100grad"><rect x="0" y="0" width="4" height="4" fill="#ff0000"/></marker></defs>
<line x1="0" y1="0" x2="40" y2="0" stroke="#000000" stroke-width="2" marker-end="url(#m)"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
}

// ===========================================================================
// Gradient / pattern xlink:href inheritance reject arms (self-ref, missing).
// ===========================================================================

#[test]
fn gradient_self_href_does_not_inherit() {
    // A gradient whose href points at itself fails the `href != id` guard
    // (line 7106) so no inheritance happens; its own stops still render.
    let text = svg(
        "gradself.svg",
        &doc(
            r##"<defs><linearGradient id="g" href="#g" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#g)"/>"##,
        ),
    );
    has(&text, "/ShadingType 2");
    has(&text, "/C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000]");
}

#[test]
fn gradient_href_to_missing_id_does_not_inherit() {
    // A gradient referencing a non-existent id fails the parent-lookup guard
    // (line 7107); its own stops still render.
    let text = svg(
        "gradmiss.svg",
        &doc(
            r##"<defs><linearGradient id="g" href="#nope" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#00ff00"/><stop offset="1" stop-color="#ff0000"/></linearGradient></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#g)"/>"##,
        ),
    );
    has(&text, "/ShadingType 2");
    has(&text, "/C0 [0.000 1.000 0.000] /C1 [1.000 0.000 0.000]");
}

#[test]
fn pattern_self_href_does_not_inherit() {
    // A pattern whose href points at itself fails the `href != id` guard
    // (line 6940); its own body still tiles.
    let text = svg(
        "patself.svg",
        &doc(
            r##"<defs><pattern id="p" href="#p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#ff0000"/></pattern></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#p)"/>"##,
        ),
    );
    has(&text, " re f");
}

// ===========================================================================
// Reusable <symbol> body: nested groups, text child, skipped <defs>.
// ===========================================================================

#[test]
fn use_symbol_body_with_nested_group_and_text_child() {
    // A used <symbol> body containing a nested <g> (closing arm line 9986), a
    // <text> child (line 10055) and a skipped <defs> (line 10003) all parse.
    let text = svg_noalt(
        "usesym.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 60"><defs><symbol id="s"><defs><rect id="unused" width="1" height="1"/></defs><g><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></g><text x="2" y="20" font-size="8" fill="#0000ff">Hi</text></symbol></defs><use href="#s"/></svg>"##,
    );
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, " TJ");
}

// ===========================================================================
// Visibility attribute.
// ===========================================================================

#[test]
fn visibility_hidden_shape_is_not_painted() {
    // visibility="hidden" (line 10240 via parse_svg_visibility_visible) removes
    // the shape's paint while a visible sibling still renders.
    let text = svg(
        "vishidden.svg",
        &doc(
            r##"<rect x="0" y="0" width="10" height="10" fill="#ff0000" visibility="hidden"/>
<rect x="20" y="20" width="10" height="10" fill="#00ff00" visibility="visible"/>"##,
        ),
    );
    has(&text, "0.000 1.000 0.000 rg 20 20 10 10 re f");
    absent(&text, "1.000 0.000 0.000 rg");
}

// ===========================================================================
// Clip path with a polygon shape.
// ===========================================================================

#[test]
fn clip_path_polygon_shape_clips_the_target() {
    // A polygon clip shape (lines 8426/8431 in svg_poly_path_ops) produces a
    // closed clip region; the clipped fill still paints under `W n`.
    let text = svg(
        "clippoly.svg",
        &doc(
            r##"<defs><clipPath id="c"><polygon points="0,0 30,0 30,30 0,30"/></clipPath></defs>
<rect x="0" y="0" width="40" height="40" fill="#ff0000" clip-path="url(#c)"/>"##,
        ),
    );
    has(&text, " W n");
    has(&text, "1.000 0.000 0.000 rg");
}

// ===========================================================================
// Body sub-scanners classifying processing-instructions (`<?...?>`) and
// non-comment declarations (`<!...>`) as skippable children.
// ===========================================================================

#[test]
fn text_body_skips_processing_instruction_and_declaration_children() {
    // A <text> body containing a <?pi?> and a <!DECL> exercises the
    // starts_with('?') / starts_with('!') classification arms (line 5603); the
    // real tspan still renders.
    let text = svg(
        "textpi.svg",
        &doc(
            r##"<text x="10" y="20" font-size="10"><?xml-stylesheet?><!SOMEDECL><tspan fill="#ff0000">Hi</tspan></text>"##,
        ),
    );
    has(&text, " TJ");
}

#[test]
fn clip_body_skips_processing_instruction_and_declaration_children() {
    // A <clipPath> body with a <?pi?> and a <!DECL> (line 8049); the rect clip
    // shape still applies.
    let text = svg(
        "clippi.svg",
        &doc(
            r##"<defs><clipPath id="c"><?pi?><!DECL><rect x="0" y="0" width="20" height="20"/></clipPath></defs>
<rect x="0" y="0" width="30" height="30" fill="#ff0000" clip-path="url(#c)"/>"##,
        ),
    );
    has(&text, " W n");
    has(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn mask_body_skips_processing_instruction_and_declaration_children() {
    // A <mask> body with a <?pi?> and a <!DECL> (line 8164); the white mask rect
    // still reveals the target.
    let text = svg(
        "maskpi.svg",
        &doc(
            r##"<defs><mask id="m"><?pi?><!DECL><rect x="0" y="0" width="40" height="40" fill="#ffffff"/></mask></defs>
<rect x="0" y="0" width="30" height="30" fill="#00ff00" mask="url(#m)"/>"##,
        ),
    );
    has(&text, "0.000 1.000 0.000 rg");
}

#[test]
fn marker_body_skips_processing_instruction_and_declaration_children() {
    // A <marker> body with a <?pi?> and a <!DECL> (line 7841); the marker rect
    // still renders at the vertex.
    let text = svg(
        "markerpi.svg",
        &doc(
            r##"<defs><marker id="mk" markerWidth="4" markerHeight="4" refX="2" refY="2"><?pi?><!DECL><rect x="0" y="0" width="4" height="4" fill="#ff0000"/></marker></defs>
<line x1="0" y1="0" x2="40" y2="40" stroke="#000000" stroke-width="2" marker-end="url(#mk)"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
}

#[test]
fn symbol_body_skips_processing_instruction_and_declaration_children() {
    // A used <symbol> body with a <?pi?> and a <!DECL> (line 9971); the rect
    // still renders.
    let text = svg_noalt(
        "sympi.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><defs><symbol id="s"><?pi?><!DECL><rect x="0" y="0" width="10" height="10" fill="#0000ff"/></symbol></defs><use href="#s"/></svg>"##,
    );
    has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn filter_body_skips_processing_instruction_and_declaration_children() {
    // A <filter> body with a <?pi?> and a <!DECL> (line 10385); the feDropShadow
    // still synthesizes a shadow.
    let text = svg(
        "filterpi.svg",
        &doc(
            r##"<defs><filter id="f"><?pi?><!DECL><feDropShadow dx="2" dy="2" flood-color="#ff0000"/></filter></defs>
<rect x="20" y="20" width="20" height="20" fill="#00ff00" filter="url(#f)"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, "0.000 1.000 0.000 rg");
}

// ===========================================================================
// Root background: background-image:none and var()-resolved gradient layer.
// ===========================================================================

#[test]
fn background_image_none_keyword_clears_layers() {
    // `background-image:none` (line 3734) yields no image layers; the solid
    // background-color still paints.
    let text = svg(
        "bgimgnone.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background-color:#ff0000;background-image:none"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, "0.000 1.000 0.000 rg 5 5 6 6 re f");
}

#[test]
fn background_image_var_resolves_to_gradient_layer() {
    // A `background-image:var(--bg)` (line 3781) resolves to a linear gradient
    // custom property and registers the shading.
    let text = svg(
        "bgimgvar.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="--bg:linear-gradient(to right, #ff0000, #0000ff);background-image:var(--bg)"><rect x="5" y="5" width="6" height="6" fill="#00ff00"/></svg>"##,
    );
    has(&text, "/ShadingType 2");
}

// ===========================================================================
// Pattern definitions with degenerate width/height.
// ===========================================================================

#[test]
fn pattern_with_zero_width_or_height_is_skipped() {
    // Patterns whose width or height resolves to 0 are dropped by the finite/
    // positive guard (line 6790); the fills fall back and the marker rect keeps
    // the document non-empty.
    let text = svg(
        "patzero.svg",
        &doc(
            r##"<defs><pattern id="pw" width="0" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#ff0000"/></pattern><pattern id="ph" width="4" height="0" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#0000ff"/></pattern></defs>
<rect x="0" y="0" width="10" height="10" fill="url(#pw)"/>
<rect x="20" y="0" width="10" height="10" fill="url(#ph)"/>
<rect x="40" y="40" width="6" height="6" fill="#00ff00"/>"##,
        ),
    );
    has(&text, "0.000 1.000 0.000 rg 40 40 6 6 re f");
}

// ===========================================================================
// Document-level scanner: XML prolog / DOCTYPE, nested groups, empty tag.
// ===========================================================================

#[test]
fn document_xml_prolog_and_doctype_are_skipped() {
    // An `<?xml?>` prolog and a `<!DOCTYPE>` at document scope hit the
    // starts_with('?') / starts_with('!') classification arms (line 3149); the
    // shape still renders.
    let text = svg(
        "docprolog.svg",
        r##"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE svg><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><rect x="5" y="5" width="10" height="10" fill="#ff0000"/></svg>"##,
    );
    has(&text, "1.000 0.000 0.000 rg 5 5 10 10 re f");
}

#[test]
fn document_nested_groups_pop_the_style_stack() {
    // Nested <g> containers at document scope push/pop the style stack; the
    // inner `</g>` closes with more than one frame present (line 3226).
    let text = svg(
        "docnested.svg",
        &doc(
            r##"<g fill="#ff0000"><g fill="#00ff00"><rect x="0" y="0" width="10" height="10"/></g><rect x="20" y="0" width="10" height="10"/></g>"##,
        ),
    );
    // Inner group green rect, outer group red rect.
    has(&text, "0.000 1.000 0.000 rg 0 0 10 10 re f");
    has(&text, "1.000 0.000 0.000 rg 20 0 10 10 re f");
}

#[test]
fn document_empty_angle_bracket_tag_is_skipped() {
    // A stray `< >` produces an empty tag name (line 3217) and is skipped; the
    // real rect still renders.
    let text = svg(
        "docempty.svg",
        &doc(r##"< ><rect x="5" y="5" width="10" height="10" fill="#0000ff"/>"##),
    );
    has(&text, "0.000 0.000 1.000 rg 5 5 10 10 re f");
}

// ===========================================================================
// Gradient body scanner: XML prolog inside defs and a non-stop child element.
// ===========================================================================

#[test]
fn gradient_definition_scan_skips_prolog_and_non_stop_children() {
    // A `<?pi?>` between gradient definitions (line 6602) and a non-`stop`
    // child inside the gradient body (line 7302) are both skipped; the two real
    // stops still form a shading.
    let text = svg(
        "gradscan.svg",
        &doc(
            r##"<defs><?pi?><linearGradient id="g" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff0000"/><desc>ignored</desc><stop offset="1" stop-color="#0000ff"/></linearGradient></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#g)"/>"##,
        ),
    );
    has(&text, "/ShadingType 2");
    has(&text, "/C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000]");
}

#[test]
fn pattern_definition_scan_skips_prolog_between_defs() {
    // A `<?pi?>` between pattern definitions (line 6688) is skipped; the real
    // pattern still tiles.
    let text = svg(
        "patscan.svg",
        &doc(
            r##"<defs><?pi?><pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#00ff00"/></pattern></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#p)"/>"##,
        ),
    );
    has(&text, " re f");
}

// ===========================================================================
// Gradient stop styled by a CSS <style> rule (transparent / colour / opacity).
// ===========================================================================

#[test]
fn gradient_stop_css_rule_transparent_and_opacity() {
    // A `<style>` rule targeting stops sets stop-color:transparent (line 7267)
    // and stop-opacity (line 7274) on classed stops, blending toward white.
    let text = svg(
        "gradstopcss.svg",
        &doc(
            r##"<style>.clear { stop-color: transparent; } .half { stop-opacity: 0.5; stop-color: #ff0000; }</style>
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0"><stop class="clear" offset="0"/><stop class="half" offset="1"/></linearGradient></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#g)"/>"##,
        ),
    );
    has(&text, "/ShadingType 2");
    // The transparent first stop becomes white; the half-opaque red -> 1,0.5,0.5.
    has(&text, "/C0 [1.000 1.000 1.000] /C1 [1.000 0.500 0.500]");
}

// ===========================================================================
// Pattern content detection: an inheriting pattern whose own body has no
// renderable elements (only <>, <?pi?>, <!decl>, closing tags).
// ===========================================================================

#[test]
fn pattern_non_renderable_body_inherits_parent_content() {
    // The child pattern's body is only empty/PI/declaration/closing tokens, so
    // svg_pattern_body_has_renderable_content walks every classification arm
    // (lines 6981-6984) and returns false; the body is inherited from the
    // href parent, which tiles a red rect.
    let text = svg(
        "patinherit.svg",
        &doc(
            r##"<defs><pattern id="base" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#ff0000"/></pattern><pattern id="p" href="#base" width="4" height="4" patternUnits="userSpaceOnUse"><><?pi?><!decl></x></pattern></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#p)"/>"##,
        ),
    );
    // The inherited red tile paints inside the pattern-filled rect.
    has(&text, "1.000 0.000 0.000 rg");
}

// ===========================================================================
// Single-stop gradients fall back to their representative solid colour.
// ===========================================================================

#[test]
fn single_stop_linear_gradient_falls_back_to_solid_color() {
    // A linear gradient with only one usable stop trips `stops.len() < 2`
    // (line 7369); the fill uses the representative solid colour, not a shading.
    let text = svg(
        "grad1linear.svg",
        &doc(
            r##"<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff0000"/></linearGradient></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#g)"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
    absent(&text, "/ShadingType 2");
}

#[test]
fn single_stop_radial_gradient_falls_back_to_solid_color() {
    // A radial gradient with only one usable stop trips `stops.len() < 2`
    // (line 7420); the fill uses the representative solid colour.
    let text = svg(
        "grad1radial.svg",
        &doc(
            r##"<defs><radialGradient id="g"><stop offset="0" stop-color="#0000ff"/></radialGradient></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#g)"/>"##,
        ),
    );
    has(&text, "0.000 0.000 1.000 rg");
    absent(&text, "/ShadingType 3");
}

// ===========================================================================
// textPath on a group-transformed path.
// ===========================================================================

#[test]
fn text_path_under_group_transform_still_places_glyphs() {
    // A <textPath> whose ancestor group carries a transform (line 5698
    // `!transform.is_identity()`) still lays glyphs along the referenced path.
    let text = svg(
        "textpathxf.svg",
        &doc(
            r##"<defs><path id="curve" d="M10,50 C30,10 70,10 90,50"/></defs>
<g transform="translate(2,2)"><text font-size="8" fill="#ff0000"><textPath href="#curve">Curved</textPath></text></g>"##,
        ),
    );
    has(&text, " TJ");
}

// ===========================================================================
// Non-ASCII glyph width in a spaced text advance.
// ===========================================================================

#[test]
fn non_ascii_text_with_word_spacing_uses_wide_glyph_advance() {
    // A word-spaced text containing a non-ASCII, non-whitespace glyph exercises
    // the wide-glyph else branch of svg_text_advance (line 6039 false side);
    // it renders and differs from the pure-ASCII spacing.
    let ascii = svg(
        "adv_ascii.svg",
        &doc(
            r##"<text x="10" y="20" font-size="10" word-spacing="3" fill="#ff0000">ab cd</text>"##,
        ),
    );
    let wide = svg(
        "adv_wide.svg",
        &doc(
            r##"<text x="10" y="20" font-size="10" word-spacing="3" fill="#ff0000">áb çd</text>"##,
        ),
    );
    has(&wide, " TJ");
    assert_ne!(
        ascii, wide,
        "non-ascii glyphs must change the advance layout"
    );
}

// ===========================================================================
// Rounded rectangle path, mask units, clip child variety, use-of-anchor.
// ===========================================================================

#[test]
fn rounded_rectangle_emits_corner_curves() {
    // A rect with positive rx and ry takes the rounded-corner branch of
    // svg_rect_path_ops (line 8385 false side), emitting cubic corner curves.
    let text = svg(
        "roundrect.svg",
        &doc(r##"<rect x="2" y="2" width="30" height="20" rx="6" ry="5" fill="#ff0000"/>"##),
    );
    has(&text, "1.000 0.000 0.000 rg");
    has(&text, " c");
}

#[test]
fn mask_object_bounding_box_content_units_still_reveal() {
    // maskContentUnits="objectBoundingBox" routes through the units parser
    // (line 8198) while the white mask rect still reveals the target.
    let text = svg(
        "maskobb.svg",
        &doc(
            r##"<defs><mask id="m" maskContentUnits="objectBoundingBox"><rect x="0" y="0" width="1" height="1" fill="#ffffff"/></mask></defs>
<rect x="0" y="0" width="30" height="30" fill="#00ff00" mask="url(#m)"/>"##,
        ),
    );
    has(&text, "0.000 1.000 0.000 rg");
}

#[test]
fn clip_path_fill_rule_from_style_uses_evenodd_operator() {
    // fill-rule (rather than clip-rule) supplied via style (line 8256) selects
    // the even-odd clip operator.
    let text = svg(
        "clipfillrule.svg",
        &doc(
            r##"<defs><clipPath id="c"><path d="M0 0 H30 V30 H0 Z" style="fill-rule:evenodd"/></clipPath></defs>
<rect x="0" y="0" width="20" height="20" fill="#ff0000" clip-path="url(#c)"/>"##,
        ),
    );
    has(&text, "W* n");
}

#[test]
fn clip_path_non_clip_shape_child_is_skipped() {
    // A <text> child inside a clipPath is not a clip shape, so svg_clip_shape_ops
    // returns None (line 8177 false side); the valid rect clip still applies.
    let text = svg(
        "clipnonshape.svg",
        &doc(
            r##"<defs><clipPath id="c"><text x="0" y="0">nope</text><rect x="0" y="0" width="20" height="20"/></clipPath></defs>
<rect x="0" y="0" width="30" height="30" fill="#0000ff" clip-path="url(#c)"/>"##,
        ),
    );
    has(&text, " W n");
    has(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn use_of_anchor_definition_applies_its_link() {
    // A reusable <a> definition (line 9765 `def.tag == "a"`) applies its own
    // link to the expanded content when instantiated by <use>.
    let text = svg(
        "useanchor.svg",
        &doc(
            r##"<defs><a id="lk" href="https://example.com"><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></a></defs>
<use href="#lk"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
    // The anchor's link produces a URI annotation.
    has(&text, "/URI (https://example.com)");
}

#[test]
fn document_explicit_closing_shape_tags_are_handled() {
    // Explicit `</rect>` closing tags at document scope reach the non-container
    // closing branch (line 3226 matches-false side); both rects render.
    let text = svg(
        "docclose.svg",
        &doc(
            r##"<rect x="0" y="0" width="10" height="10" fill="#ff0000"></rect><rect x="20" y="0" width="10" height="10" fill="#00ff00"></rect>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg 0 0 10 10 re f");
    has(&text, "0.000 1.000 0.000 rg 20 0 10 10 re f");
}

#[test]
fn empty_font_family_value_is_ignored() {
    // An empty font-family value (line 11111) leaves the inherited font; the
    // text still renders.
    let text = svg(
        "emptyfam.svg",
        &doc(r##"<text x="10" y="20" font-size="10" font-family="" fill="#ff0000">Hi</text>"##),
    );
    has(&text, " TJ");
}

// ===========================================================================
// Text position lists with units and skipped tokens.
// ===========================================================================

#[test]
fn text_position_list_parses_units_and_skips_bad_tokens() {
    // A multi-value `x` list containing a unit-suffixed value and a non-numeric
    // token exercises the unit-scan loop (line 6289) and the bad-token skip
    // (line 6280); per-glyph positions differ from a single-x layout.
    let list = svg(
        "poslist.svg",
        &doc(r##"<text x="10 20px zz 30" y="20" font-size="10" fill="#ff0000">abcd</text>"##),
    );
    let single = svg(
        "possingle.svg",
        &doc(r##"<text x="10" y="20" font-size="10" fill="#ff0000">abcd</text>"##),
    );
    has(&list, " TJ");
    assert_ne!(
        list, single,
        "per-character x positions must change the layout"
    );
}

// ===========================================================================
// Mask shape revealed via a partial fill-opacity in its style.
// ===========================================================================

#[test]
fn mask_shape_fill_opacity_style_half_still_reveals() {
    // A white mask rect with fill-opacity:0.5 in its style (lines 8228/8229)
    // yields an effective luminance*alpha of exactly 0.5, which reveals; the
    // masked shape paints.
    let text = svg(
        "maskhalf.svg",
        &doc(
            r##"<defs><mask id="m"><rect x="0" y="0" width="40" height="40" style="fill:#ffffff;fill-opacity:0.5"/></mask></defs>
<rect x="0" y="0" width="30" height="30" fill="#ff0000" mask="url(#m)"/>"##,
        ),
    );
    has(&text, "1.000 0.000 0.000 rg");
}

// ===========================================================================
// Self-closing pattern element in the definition scan.
// ===========================================================================

#[test]
fn self_closing_pattern_definition_inherits_via_href() {
    // A self-closing `<pattern/>` (line 6704 self-closing arm) with an href
    // inherits its parent's tile content.
    let text = svg(
        "patselfclose.svg",
        &doc(
            r##"<defs><pattern id="base" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#0000ff"/></pattern><pattern id="p" href="#base" width="4" height="4" patternUnits="userSpaceOnUse"/></defs>
<rect x="0" y="0" width="20" height="20" fill="url(#p)"/>"##,
        ),
    );
    has(&text, " re f");
}

// ===========================================================================
// Oversized element count and CSS @-rule / empty custom-property.
// ===========================================================================

#[test]
fn svg_exceeding_element_cap_is_rejected() {
    // More than 4096 elements trips the document element ceiling (line 3504),
    // so the whole SVG is rejected: none of its red rects reach the PDF, while
    // a small control SVG paints its rect normally.
    let mut big =
        String::from(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">"##);
    for _ in 0..4200 {
        big.push_str(r##"<rect x="0" y="0" width="1" height="1" fill="#ff0000"/>"##);
    }
    big.push_str("</svg>");
    let oversized = svg("bigdoc.svg", &big);
    absent(&oversized, "1.000 0.000 0.000 rg");
    let control = svg(
        "smalldoc.svg",
        &doc(r##"<rect x="0" y="0" width="10" height="10" fill="#ff0000"/>"##),
    );
    has(&control, "1.000 0.000 0.000 rg");
}

#[test]
fn css_at_rule_selector_is_skipped() {
    // A selector starting with `@` is skipped (line 6528); the following valid
    // rule still recolours the rect.
    let text = svg(
        "cssatrule.svg",
        &doc(r##"<style>@media screen { } rect { fill: #00ff00; }</style>
<rect x="0" y="0" width="10" height="10" fill="#0000ff"/>"##),
    );
    has(&text, "0.000 1.000 0.000 rg");
    absent(&text, "0.000 0.000 1.000 rg");
}

#[test]
fn css_empty_custom_property_name_is_ignored() {
    // A `--:` declaration whose name is only the two dashes (line 6476,
    // `name.len() <= 2`) is ignored; a normal var still resolves.
    let text = svg(
        "cssemptyvar.svg",
        &doc(r##"<style>:root { --: broken; --ok: #ff0000; }</style>
<rect x="0" y="0" width="10" height="10" fill="var(--ok)"/>"##),
    );
    has(&text, "1.000 0.000 0.000 rg 0 0 10 10 re f");
}

/// Regression: `drop-shadow()` with fewer than two lengths used to panic with
/// index-out-of-bounds because `bool::then_some` evaluates its argument (and
/// its `lengths[0]`/`lengths[1]` indexing) eagerly even when the guard is
/// false. Both malformed forms must render the shape unshadowed, not crash.
#[test]
fn drop_shadow_with_fewer_than_two_lengths_renders_without_shadow() {
    for (name, filter) in [
        ("shadow-color-only.svg", "drop-shadow(red)"),
        ("shadow-no-lengths.svg", "drop-shadow(foo bar)"),
        ("shadow-one-length.svg", "drop-shadow(4px)"),
    ] {
        let text = svg(
            name,
            &format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><rect x="4" y="4" width="30" height="30" fill="#204060" style="filter: {filter}"/></svg>"##
            ),
        );
        has(&text, "0.125 0.251 0.376 rg");
        assert!(
            !text.contains("0.890 0.900 0.920 rg"),
            "{name}: malformed drop-shadow() must not synthesize the gray fallback shadow"
        );
        assert_eq!(
            text.matches("0.125 0.251 0.376 rg").count(),
            1,
            "{name}: exactly one fill pass (no shadow layer duplicate)"
        );
    }
}
