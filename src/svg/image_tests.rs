//! Image container, geometry, resource identity and API regressions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use franken_markdown::{FontAssetSlot, FontAssets, PdfImageAsset};
use super::image_source::{decode_data_uri, inspect};

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let checksum = franken_markdown::crc32(&out[start..]);
    out.extend_from_slice(&checksum.to_be_bytes());
}

// Valid RGBA PNGs with stored DEFLATE blocks, not header-only fake images.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut raw = Vec::new();
    for _ in 0..height {
        raw.push(0);
        for _ in 0..width { raw.extend_from_slice(&[20, 40, 60, 128]); }
    }
    let mut zlib = vec![0x78, 0x01];
    let chunks = raw.len().div_ceil(60_000);
    for (index, bytes) in raw.chunks(60_000).enumerate() {
        zlib.push(u8::from(index + 1 == chunks));
        let len = bytes.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(bytes);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw { a = (a + u32::from(*byte)) % 65521; b = (b + a) % 65521; }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut result = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut result, b"IHDR", &header);
    chunk(&mut result, b"IDAT", &zlib);
    chunk(&mut result, b"IEND", &[]);
    result
}

fn image(destination: &str, alt: &str) -> Inline {
    Inline::Image { dest: destination.to_owned(), alt: alt.to_owned(), title: None }
}

fn document(inlines: Vec<Inline>) -> Document {
    Document { blocks: vec![Block::Paragraph(inlines)] }
}

#[test]
fn png_container_crc_and_intrinsic_dimensions() {
    let bytes = png(200, 100);
    assert_eq!(inspect(&bytes).unwrap(), ("image/png", 150.0, 75.0));
    let mut damaged = bytes.clone();
    damaged[20] ^= 1;
    assert_eq!(inspect(&damaged).unwrap_err().code, "svg_image_invalid");
    for length in [0, 8, 16, 29, bytes.len() - 1] {
        assert!(inspect(&bytes[..length]).is_err());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(inspect(&trailing).is_err());
}

#[test]
fn jpeg_metadata_scan_accepts_baseline_and_progressive_frames() {
    // Container headers are sufficient to inspect; entropy decoding is the viewer's job.
    for sof in [0xc0, 0xc2] {
        let bytes = [0xff, 0xd8, 0xff, 0xe0, 0, 4, 1, 2,
            0xff, sof, 0, 11, 8, 0, 20, 0, 40, 1, 1, 0x11, 0,
            0xff, 0xda, 0, 8, 1, 1, 0, 0, 63, 0, 1, 0xff, 0xd9];
        assert_eq!(inspect(&bytes).unwrap(), ("image/jpeg", 30.0, 15.0));
        assert!(inspect(&bytes[..bytes.len() - 1]).is_err());
    }
}

#[test]
fn svg_sizes_resolve_units_entities_and_viewbox_aspect_ratio() {
    for (svg, expected) in [
        (r#"<svg width="100pt" height="2in"/>"#, (100.0, 144.0)),
        (r#"<svg viewBox="-20 30 200 100"/>"#, (150.0, 75.0)),
        (r#"<svg width="96px" viewBox="0 0 4 2"/>"#, (72.0, 36.0)),
        (r#"<svg height="96px" viewBox="0 0 4 2"/>"#, (144.0, 72.0)),
        (r#"<svg width="100%" height="10em" viewBox="0 0 4 2"/>"#, (3.0, 1.5)),
        (r#"<svg width="1&#x30;0px" height="1e2"/>"#, (75.0, 75.0)),
        (r#"<svg/>"#, (225.0, 112.5)),
    ] {
        let (mime, w, h) = inspect(svg.as_bytes()).unwrap();
        assert_eq!(mime, "image/svg+xml");
        assert!((w - expected.0).abs() < 0.00001, "{svg}");
        assert!((h - expected.1).abs() < 0.00001, "{svg}");
    }
}

#[test]
fn invalid_xml_dtds_duplicate_dimensions_and_invalid_geometry_are_refused() {
    for svg in [
        "<svg><g></svg>", "<svg width='1' width='2'/>", "<svg/><svg/>",
        "<!DOCTYPE svg SYSTEM 'https://example.com/external'><svg/>",
        "<?xml-stylesheet href='external.css'?><svg/>",
        "<svg><text>&undefined;</text></svg>", "<svg width='NaN'/>",
        "<svg width='-1'/>", "<svg width='1e999'/>", "<svg viewBox='0 0 0 1'/>",
        "<svg>\u{0}</svg>", "<svg width='1'height='1'/>",
    ] {
        assert!(inspect(svg.as_bytes()).is_err(), "accepted {svg:?}");
    }
    assert_eq!(inspect(b"<svg width='20000' height='1'/>").unwrap_err().code, "svg_image_dimensions");
}

#[test]
fn data_uri_base64_and_percent_encoding_round_trip() {
    for length in 0..513 {
        let bytes: Vec<_> = (0..length).map(|n| ((n * 37 + length) % 256) as u8).collect();
        let uri = format!("data:image/png;base64,{}", franken_markdown::html::base64_encode(&bytes));
        assert_eq!(decode_data_uri(&uri).unwrap(), bytes);
    }
    assert_eq!(decode_data_uri("data:image/svg+xml,%3Csvg%20viewBox=%270%200%201%201%27/%3E").unwrap(),
        b"<svg viewBox='0 0 1 1'/>".to_vec());
    assert_eq!(decode_data_uri("data:image/png;base64,%41%51%3D%3D").unwrap(), [1]);
}

#[test]
fn invalid_base64_padding_and_uri_types_are_refused() {
    for payload in ["A", "AAAAA", "=AAA", "A===", "AB==", "AAB=", "AA==AAAA", "AA$=", "AA%"] {
        assert!(decode_data_uri(&format!("data:image/png;base64,{payload}")).is_err(), "{payload}");
    }
    for uri in ["data:text/html,hello", "data:image/svg+xml,%q0", "data:image/png;base64;base64,AA==", "data:image/png;base64", "data:image/png;unknown,abc"] {
        assert!(decode_data_uri(uri).is_err(), "{uri}");
    }
}

#[test]
fn images_render_and_deduplicate_across_destination_keys() {
    let bytes = png(2, 1);
    let assets = [PdfImageAsset::new("a", bytes.clone()), PdfImageAsset::new("b", bytes)];
    let doc = document(vec![image("a", "first"), Inline::SoftBreak, image("b", "second")]);
    let (svg, report, warnings) = render_svg_with_resources(&doc, &SvgOptions::default(), &FontAssets::default(), &assets).unwrap();
    let svg = String::from_utf8(svg).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(svg.matches("<image ").count(), 1);
    assert_eq!(svg.matches("href=\"#i0\"").count(), 2);
    assert!(svg.contains("aria-label=\"first\""));
    assert!(svg.contains("aria-label=\"second\""));
    assert_eq!(report.glyphs_drawn, 0);
    assert!(!svg.contains("href=\"a\"") && !svg.contains("href=\"b\""));
}

#[test]
fn data_images_work_through_existing_svg_entry_points() {
    let uri = format!("data:image/png;base64,{}", franken_markdown::html::base64_encode(&png(2, 1)));
    let doc = document(vec![image(&uri, "plot")]);
    let (bytes, _, warnings) = render_svg_with_diagnostics(&doc, &SvgOptions::default());
    assert!(warnings.is_empty());
    assert!(String::from_utf8(bytes).unwrap().contains("<image "));
}

#[test]
fn image_boxes_fit_width_preserve_ratio_and_reserve_vertical_space() {
    let p = Poster::new(&SvgOptions::default()).with_resources(&FontAssets::default(),
        &[PdfImageAsset::new("wide", png(200, 100))]).unwrap();
    let word = p.image_word("wide", "", RStyle::BODY, 11.0, 40.0);
    let run = word.image.as_ref().unwrap();
    assert_eq!((run.width, run.height), (40.0, 20.0));
    let (ascent, height) = p.line_metrics(&[word], 9.35, 16.0);
    assert_eq!(ascent, 20.0);
    assert!(height >= 20.0);
    let lines = p.wrap(&[Piece::Text("prefix".into(), RStyle::BODY),
        Piece::Image("wide".into(), "plot".into(), RStyle::BODY)], 11.0, 40.0);
    assert_eq!(lines.iter().flatten().filter(|word| word.image.is_some()).count(), 1);
    assert!(lines.iter().all(|line| p.words_width(line, 11.0) <= 40.00001));
}

#[test]
fn table_image_height_is_preserved_and_failures_warn_only_at_paint() {
    let mut p = Poster::new(&SvgOptions::default()).with_resources(&FontAssets::default(),
        &[PdfImageAsset::new("plot", png(100, 200))]).unwrap();
    let top = p.y;
    p.table(&Table { align: vec![Align::Left], head: vec![vec![Inline::Text("H".into())]],
        rows: vec![vec![vec![image("plot", "image")]], vec![vec![image("missing", "lost")]]] }, 72.0, 540.0);
    assert!(p.y - top >= 150.0);
    assert_eq!(p.warnings.len(), 1);
    assert_eq!(p.warnings[0].code, "svg_image_missing");
    assert_eq!(p.ops.iter().filter(|op| matches!(op, Op::Image { .. })).count(), 1);
}

#[test]
fn embedded_svg_is_contained_as_image_bytes_not_injected_markup() {
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><script>alert(1)</script><text>A &amp; B</text></svg>"#;
    let assets = [PdfImageAsset::new("icon", source.as_bytes().to_vec())];
    let doc = document(vec![image("icon", "\"><script>\u{0} &")] );
    let (bytes, _, warnings) = render_svg_with_resources(&doc, &SvgOptions::default(), &FontAssets::default(), &assets).unwrap();
    assert!(warnings.is_empty());
    let svg = String::from_utf8(bytes).unwrap();
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("\u{0}"));
    assert!(svg.contains("&quot;&gt;&lt;script&gt;  &amp;"));
    assert!(svg.contains(&franken_markdown::html::base64_encode(source.as_bytes())));
}

#[test]
fn first_asset_wins_even_when_invalid_and_no_url_is_fetched() {
    let assets = [PdfImageAsset::new(" key ", vec![0]), PdfImageAsset::new("key", png(1, 1))];
    let doc = document(vec![image("key", "kept"), Inline::SoftBreak, image("https://example.com/private.png", "network")]);
    let (bytes, report, warnings) = render_svg_with_resources(&doc, &SvgOptions::default(), &FontAssets::default(), &assets).unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(report.glyphs_drawn > 0);
    assert!(!String::from_utf8(bytes).unwrap().contains("<image "));
}

#[test]
fn resource_admission_limits_fail_before_rendering() {
    let options = SvgOptions::default();
    let fonts = FontAssets::default();
    let doc = document(vec![image("bad", "bad")]);
    assert!(render_svg_with_resources(&doc, &options, &fonts,
        &[PdfImageAsset::new("", png(1, 1))]).is_err());
    assert!(render_svg_with_resources(&doc, &options, &fonts,
        &vec![PdfImageAsset::new("a", vec![]); 4097]).is_err());
    let invalid_fonts = FontAssets { body_regular: Some(vec![]), ..Default::default() };
    assert!(render_svg_with_resources(&doc, &options, &invalid_fonts, &[]).is_err());
}

#[test]
fn custom_body_font_uses_its_actual_outline_and_no_viewer_font() {
    let doc = document(vec![Inline::Text("A".into())]);
    let fonts = FontAssets::default().with_slot(FontAssetSlot::BodyRegular,
        franken_markdown::text::bundled::CM_REGULAR.to_vec()).unwrap();
    let (actual, _, _) = render_svg_with_resources(&doc, &SvgOptions::default(), &fonts, &[]).unwrap();
    let mut reference_options = SvgOptions::default();
    reference_options.theme.font = franken_markdown::FontFamily::Serif;
    let expected = render_svg(&doc, &reference_options);
    assert_eq!(actual, expected);
    assert_ne!(actual, render_svg(&doc, &SvgOptions::default()));
    assert!(!String::from_utf8(actual).unwrap().contains("<text"));
}

#[test]
fn resources_and_diagnostics_are_deterministic_and_plain_output_is_unchanged() {
    let doc = document(vec![image("plot", "chart"), image("missing", "lost")]);
    let assets = [PdfImageAsset::new("plot", png(1, 1))];
    let options = SvgOptions::default();
    let fonts = FontAssets::default();
    let first = render_svg_with_resources(&doc, &options, &fonts, &assets).unwrap();
    assert_eq!(first, render_svg_with_resources(&doc, &options, &fonts, &assets).unwrap());
    let ordinary = document(vec![Inline::Text("plain prose".into())]);
    assert_eq!(render_svg_with_resources(&ordinary, &options, &fonts, &[]).unwrap(),
        render_svg_with_diagnostics(&ordinary, &options));
}
