//! JPEG orientation is part of the poster's intrinsic size and image payload.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::svg::{SvgOptions, render_svg_with_resources};
use franken_markdown::{FontAssets, HtmlOptions, PdfImageAsset, parse_markdown, render_html};

fn jpeg() -> Vec<u8> {
    let hex: String = include_str!("fixtures/jpeg-quadrants.hex")
        .split_whitespace()
        .collect();
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn exif(orientation: u16, little_endian: bool) -> Vec<u8> {
    let mut payload = b"Exif\0\0".to_vec();
    if little_endian {
        payload.extend_from_slice(b"II\x2a\0\x08\0\0\0\x01\0");
        payload.extend_from_slice(b"\x12\x01\x03\0\x01\0\0\0");
        payload.extend_from_slice(&orientation.to_le_bytes());
    } else {
        payload.extend_from_slice(b"MM\0\x2a\0\0\0\x08\0\x01");
        payload.extend_from_slice(b"\x01\x12\0\x03\0\0\0\x01");
        payload.extend_from_slice(&orientation.to_be_bytes());
    }
    payload.extend_from_slice(&[0; 6]); // Inline SHORT padding and next IFD offset.
    payload
}

fn with_metadata(payload: &[u8], after_frame: bool) -> Vec<u8> {
    let bytes = jpeg();
    let position = if after_frame {
        bytes
            .windows(2)
            .position(|pair| pair == [0xff, 0xda])
            .unwrap()
    } else {
        2
    };
    let mut output = bytes[..position].to_vec();
    output.extend_from_slice(&[0xff, 0xe1]);
    output.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
    output.extend_from_slice(payload);
    output.extend_from_slice(&bytes[position..]);
    output
}

fn poster(source: &str, bytes: &[u8]) -> String {
    let document = parse_markdown(source);
    let (output, _, warnings) = render_svg_with_resources(
        &document,
        &SvgOptions::default(),
        &FontAssets::default(),
        &[PdfImageAsset::new("photo", bytes.to_vec())],
    )
    .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    String::from_utf8(output).unwrap()
}

fn image_dimensions(output: &str, expected_width: f64, expected_height: f64) {
    assert!(
        output.contains(&format!(
            "<symbol id=\"i0\" viewBox=\"0 0 {expected_width:.2} {expected_height:.2}\""
        )),
        "symbol must use displayed dimensions"
    );
    let image_use = output
        .lines()
        .find(|line| line.starts_with("<use href=\"#i0\""))
        .unwrap();
    assert!(image_use.contains(&format!("width=\"{expected_width:.2}\"")));
    assert!(image_use.contains(&format!("height=\"{expected_height:.2}\"")));
}

#[test]
fn every_exif_orientation_reserves_the_displayed_aspect_ratio() {
    // The real fixture stores 32 x 16 pixels; poster dimensions are points.
    // Mirrors do not exchange dimensions. Transposes/quarter-turns do.
    for (orientation, width, height) in [
        (1, 24.0, 12.0),
        (2, 24.0, 12.0),
        (3, 24.0, 12.0),
        (4, 24.0, 12.0),
        (5, 12.0, 24.0),
        (6, 12.0, 24.0),
        (7, 12.0, 24.0),
        (8, 12.0, 24.0),
    ] {
        for little_endian in [true, false] {
            let bytes = with_metadata(&exif(orientation, little_endian), false);
            let output = poster("![Photo](photo)", &bytes);
            image_dimensions(&output, width, height);
            assert_eq!(output, poster("![Photo](photo)", &bytes));
        }
    }
}

#[test]
fn jpeg_metadata_after_the_frame_still_controls_the_poster_box() {
    let bytes = with_metadata(&exif(6, true), true);
    let output = poster("![Photo](photo)", &bytes);
    image_dimensions(&output, 12.0, 24.0);
}

#[test]
fn absent_invalid_and_truncated_exif_keep_the_accepted_image() {
    let mut bad_offset = exif(6, true);
    bad_offset[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    for bytes in [
        jpeg(),
        with_metadata(&exif(0, true), false),
        with_metadata(&exif(9, false), false),
        with_metadata(b"Exif\0\0II\x2a\0", false),
        with_metadata(&bad_offset, false),
    ] {
        image_dimensions(&poster("![Photo](photo)", &bytes), 24.0, 12.0);
    }
}

#[test]
fn oriented_jpeg_bytes_are_embedded_once_without_rewriting_exif() {
    let bytes = with_metadata(&exif(6, false), false);
    let uri = format!(
        "data:image/jpeg;base64,{}",
        franken_markdown::html::base64_encode(&bytes)
    );
    let output = poster("![First](photo)\n\n![Second](photo)", &bytes);
    image_dimensions(&output, 12.0, 24.0);
    assert_eq!(output.matches(&uri).count(), 1);
    assert_eq!(output.matches("<use href=\"#i0\"").count(), 2);
    assert!(output.contains("aria-label=\"First\""));
    assert!(output.contains("aria-label=\"Second\""));

    // HTML also hands the original JPEG to its viewer. No second rotation or
    // transcode is introduced at either caller-owned image boundary.
    let html = render_html(
        "![Photo](photo)",
        &HtmlOptions {
            image_assets: vec![PdfImageAsset::new("photo", bytes)],
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    assert!(html.contains(&format!("<img src=\"{uri}\" alt=\"Photo\">")));
}
