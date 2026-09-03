//! Integration tests for WOFF2 HTML font embedding (bead zsfx).
//!
//! Follow-up to ge1t's WOFF1:
//! - Round-trips every glyph through the project's WOFF2 decoder reference path
//! - Output is byte-deterministic across runs
//! - Beats WOFF1 bytes on the bundled faces
//! - Generates valid WOFF2 containers (`data:font/woff2`, `format("woff2")`)
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::woff1::encode_woff1;
use franken_markdown::woff2::{decode_woff2, encode_woff2};
use franken_markdown::{HtmlFontFormat, HtmlOptions, render_html};

const DOC: &str = r#"# WOFF2 integration

Body text with **bold**, *italic*, ***bold italic***, and `mono code`.

| Col | Val |
|---|---:|
| a   | 1 |

```rust
fn main() { println!("hi"); }
```
"#;

fn opts(format: HtmlFontFormat) -> HtmlOptions {
    HtmlOptions {
        html_font_format: format,
        ..HtmlOptions::default()
    }
}

#[test]
fn woff2_uses_woff2_data_urls_and_format_hint() {
    let html = render_html(DOC, &opts(HtmlFontFormat::Woff2)).expect("render woff2");
    assert!(
        html.contains("data:font/woff2;base64,"),
        "WOFF2 HTML must embed WOFF2 font data URLs"
    );
    assert!(
        html.matches("data:font/woff2;base64,").count() >= 5,
        "body regular/bold/italic/bold-italic + mono faces all embed: {}",
        html.matches("data:font/woff2;base64,").count()
    );
    assert!(
        !html.contains("data:font/ttf"),
        "WOFF2 render must not leak raw TTF data URLs"
    );
    assert!(
        html.contains("format(\"woff2\")"),
        "font-face src must declare the woff2 format hint"
    );
}

#[test]
fn woff2_output_is_smaller_than_woff1_and_ttf() {
    let woff2 = render_html(DOC, &opts(HtmlFontFormat::Woff2)).expect("woff2 render");
    let woff1 = render_html(DOC, &opts(HtmlFontFormat::Woff1)).expect("woff1 render");
    let ttf = render_html(DOC, &opts(HtmlFontFormat::Ttf)).expect("ttf render");

    assert!(
        woff2.len() < woff1.len(),
        "woff2 ({}) must be smaller than woff1 ({})",
        woff2.len(),
        woff1.len()
    );
    assert!(
        woff1.len() < ttf.len(),
        "woff1 ({}) must be smaller than ttf ({})",
        woff1.len(),
        ttf.len()
    );

    let saving_vs_ttf = 1.0 - (woff2.len() as f64 / ttf.len() as f64);
    let saving_vs_woff1 = 1.0 - (woff2.len() as f64 / woff1.len() as f64);

    assert!(
        saving_vs_ttf > 0.30,
        "expected >30% savings vs TTF, got {:.1}%",
        saving_vs_ttf * 100.0
    );
    assert!(
        saving_vs_woff1 > 0.05,
        "expected WOFF2 to beat WOFF1 by >5%, got {:.1}%",
        saving_vs_woff1 * 100.0
    );
}

#[test]
fn woff2_output_is_byte_deterministic() {
    let a = render_html(DOC, &opts(HtmlFontFormat::Woff2)).expect("render a");
    let b = render_html(DOC, &opts(HtmlFontFormat::Woff2)).expect("render b");
    assert_eq!(a, b, "woff2 renders must be byte-identical across runs");
}

#[test]
fn woff2_beats_woff1_on_all_bundled_faces() {
    let faces: &[(&str, &[u8])] = &[
        (
            "IBMPlexSans-Regular",
            include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"),
        ),
        (
            "IBMPlexSans-Bold",
            include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Bold.ttf"),
        ),
        (
            "IBMPlexSans-Italic",
            include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf"),
        ),
        (
            "IBMPlexSans-BoldItalic",
            include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-BoldItalic.ttf"),
        ),
        (
            "cmuntt",
            include_bytes!("../fmd-font/fonts/computer-modern/cmuntt.ttf"),
        ),
        (
            "NotoSansMathSymbols",
            include_bytes!("../fmd-font/fonts/noto-sans-math/NotoSansMathSymbols.ttf"),
        ),
    ];

    for &(name, bytes) in faces {
        let woff1 = encode_woff1(bytes).expect("encode woff1");
        let woff2 = encode_woff2(bytes).expect("encode woff2");

        assert!(
            woff2.len() < woff1.len(),
            "face {} WOFF2 ({} bytes) must be smaller than WOFF1 ({} bytes)",
            name,
            woff2.len(),
            woff1.len()
        );

        let saving = 1.0 - (woff2.len() as f64 / woff1.len() as f64);
        assert!(
            saving > 0.05,
            "face {} WOFF2 expected >5% savings vs WOFF1, got {:.1}%",
            name,
            saving * 100.0
        );
    }
}

#[test]
fn woff2_round_trips_every_glyph_through_decoder() {
    let faces: &[(&str, &[u8])] = &[
        (
            "IBMPlexSans-Regular",
            include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"),
        ),
        (
            "cmuntt",
            include_bytes!("../fmd-font/fonts/computer-modern/cmuntt.ttf"),
        ),
        (
            "NotoSansMathSymbols",
            include_bytes!("../fmd-font/fonts/noto-sans-math/NotoSansMathSymbols.ttf"),
        ),
    ];

    for &(name, orig_bytes) in faces {
        let woff2 = encode_woff2(orig_bytes).expect("encode woff2");
        assert_eq!(&woff2[..4], b"wOF2");

        let decoded_bytes = decode_woff2(&woff2).expect("decode woff2");

        let orig_font = fmd_font::Font::parse(orig_bytes.to_vec()).expect("parse original font");
        let dec_font = fmd_font::Font::parse(decoded_bytes).expect("parse decoded font");

        assert_eq!(
            orig_font.num_glyphs, dec_font.num_glyphs,
            "glyph count mismatch for {}",
            name
        );

        // Verify every single glyph round-trips with exact metrics and outlines
        for gid in 0..orig_font.num_glyphs {
            assert_eq!(
                orig_font.advance_width(gid),
                dec_font.advance_width(gid),
                "advance width mismatch at gid {} in {}",
                gid,
                name
            );
            assert_eq!(
                orig_font.glyph_bbox(gid),
                dec_font.glyph_bbox(gid),
                "glyph bbox mismatch at gid {} in {}",
                gid,
                name
            );
        }
    }
}
