//! GIF animations keep their original bytes in SVG and a real first-frame
//! raster (including transparency) in static PDF output.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    FontAssets, PdfImageAsset, PdfOptions, PdfRunningImage, PdfRunningImagePosition,
    parse_markdown, render_pdf, render_warnings,
    svg::{SvgOptions, render_svg_with_resources},
};

const ANIMATION: &str = include_str!("fixtures/gif/animated-colors.base64");

fn animation() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut value = 0u32;
    let mut bits = 0;
    for byte in ANIMATION.trim().bytes().take_while(|&byte| byte != b'=') {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("invalid fixture encoding"),
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((value >> bits) as u8);
        }
    }
    bytes
}

fn options(bytes: Vec<u8>) -> PdfOptions {
    PdfOptions {
        image_assets: vec![PdfImageAsset::new("animation", bytes)],
        ..PdfOptions::default()
    }
}

// Read image streams by their declared lengths, never by scanning binary
// payloads for endstream. These tests inspect the actual emitted samples.
fn image_streams(pdf: &[u8]) -> Vec<(String, Vec<u8>)> {
    let marker = b"<< /Type /XObject /Subtype /Image ";
    let mut rest = pdf;
    let mut images = Vec::new();
    while let Some(start) = rest.windows(marker.len()).position(|part| part == marker) {
        rest = &rest[start..];
        let header_end = rest
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap();
        let header = std::str::from_utf8(&rest[..header_end])
            .unwrap()
            .to_string();
        let len: usize = header
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(header.contains("/Filter /FlateDecode"));
        assert!(!header.contains("/Predictor"));
        let payload = &rest[header_end + 8..header_end + 8 + len];
        images.push((
            header,
            franken_markdown::zlib_decompress(payload, 1024).unwrap(),
        ));
        rest = &rest[header_end + 8 + len..];
    }
    images
}

fn assert_first_frame(pdf: &[u8]) {
    let images = image_streams(pdf);
    assert_eq!(images.len(), 2, "one RGB image and its alpha soft mask");
    let (rgb, pixels) = images
        .iter()
        .find(|(h, _)| h.contains("/DeviceRGB"))
        .unwrap();
    assert!(rgb.contains("/Width 2 /Height 2"));
    assert!(rgb.contains("/SMask "));
    assert_eq!(pixels, &[255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0]);
    let (_, alpha) = images
        .iter()
        .find(|(h, _)| h.contains("/DeviceGray"))
        .unwrap();
    assert_eq!(alpha, &[255, 255, 255, 0]);
}

#[test]
fn pdf_decodes_first_frame_colors_transparency_alt_text_and_link() {
    let source = "[![Animated colors](animation)](https://example.com/animation)";
    let options = options(animation());
    let pdf = render_pdf(source, &options).unwrap();
    assert_eq!(pdf, render_pdf(source, &options).unwrap());
    assert_first_frame(&pdf);
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Alt (Animated colors)"));
    assert!(text.contains("/URI (https://example.com/animation)"));
    assert!(render_warnings(&parse_markdown(source), &options).is_empty());
}

#[test]
fn inline_table_and_running_gifs_reuse_the_same_decoded_resource() {
    let mut options = options(animation());
    options.running.header.image = Some(PdfRunningImage {
        dest: "animation".into(),
        position: PdfRunningImagePosition::Left,
        height_pt: Some(16),
    });
    let source = "Before ![Inline animation](animation) after.\n\n\
                  | Status |\n| --- |\n| ![Table animation](animation) |\n";
    let pdf = render_pdf(source, &options).unwrap();
    assert_first_frame(&pdf);
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Alt (Inline animation)"));
    assert!(text.contains("/Alt (Table animation)"));
    assert!(render_warnings(&parse_markdown(source), &options).is_empty());
}

#[test]
fn gifs_embedded_inside_svg_keep_their_first_frame_in_pdf() {
    let encoded = ANIMATION.trim();
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"80\">\
         <image x=\"4\" y=\"5\" width=\"32\" height=\"32\" \
         href=\"data:image/gif;base64,{encoded}\"/></svg>"
    );
    let options = options(svg.into_bytes());
    let pdf = render_pdf("![Composed graphic](animation)", &options).unwrap();
    assert_first_frame(&pdf);
    assert!(String::from_utf8_lossy(&pdf).contains("/Alt (Composed graphic)"));
}

#[test]
fn svg_embeds_complete_animation_for_host_assets_and_data_uris() {
    let source = "[![Animated colors](animation)](https://example.com/animation)";
    let assets = vec![PdfImageAsset::new("animation", animation())];
    let render = |source: &str, assets: &[PdfImageAsset]| {
        render_svg_with_resources(
            &parse_markdown(source),
            &SvgOptions::default(),
            &FontAssets::default(),
            assets,
        )
        .unwrap()
    };
    let (svg, _, warnings) = render(source, &assets);
    assert!(warnings.is_empty(), "{warnings:?}");
    let svg = String::from_utf8(svg).unwrap();
    let uri = format!("data:image/gif;base64,{}", ANIMATION.trim());
    assert!(svg.contains(&format!("href=\"{uri}\"")));
    assert!(svg.contains("https://example.com/animation"));
    assert!(svg.contains("Animated colors"));
    let source = format!("[![Animated colors]({uri})](https://example.com/animation)");
    let (embedded, _, warnings) = render(&source, &[]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(svg.as_bytes(), embedded);
}

#[test]
fn malformed_gif_keeps_visible_fallback_and_diagnostics() {
    let source = "![Corrupt animation](animation)";
    let mut oversized = animation();
    oversized[6..10].copy_from_slice(&[0xff; 4]);
    let mut truncated = animation();
    truncated.pop();
    for bytes in [oversized, truncated, b"GIF89a".to_vec()] {
        let options = options(bytes);
        let pdf = render_pdf(source, &options).unwrap();
        assert!(image_streams(&pdf).is_empty());
        assert!(
            render_warnings(&parse_markdown(source), &options)
                .iter()
                .any(|warning| warning.code() == "unsupported_image")
        );
        let report =
            franken_markdown::verify::verify_pdf(&parse_markdown(source), &options).unwrap();
        let text = report
            .pages
            .iter()
            .flat_map(|p| &p.runs)
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            text.contains("Corrupt") && text.contains("animation"),
            "{text}"
        );
        let (_, _, warnings) = render_svg_with_resources(
            &parse_markdown(source),
            &SvgOptions::default(),
            &FontAssets::default(),
            &options.image_assets,
        )
        .unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.code == "svg_image_invalid")
        );
    }
}

#[test]
fn browser_neutral_pdf_api_uses_the_same_gif_decoder() {
    let source = "![Animated colors](animation)";
    let native = options(animation());
    let browser = franken_markdown::wasm::WasmRenderOptions {
        pdf_image_assets: native.image_assets.clone(),
        ..franken_markdown::wasm::WasmRenderOptions::default()
    };
    assert_eq!(
        render_pdf(source, &native).unwrap(),
        franken_markdown::wasm::render_pdf(source, &browser)
            .unwrap()
            .bytes
    );
}
