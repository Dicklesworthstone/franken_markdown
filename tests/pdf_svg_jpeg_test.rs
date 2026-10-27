//! Embedded JPEGs retain their original samples and SVG viewport semantics.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfImageAsset, PdfOptions, render_pdf};

fn jpeg() -> Vec<u8> {
    let hex: String = include_str!("fixtures/jpeg-quadrants.hex")
        .split_whitespace()
        .collect();
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn data_uri(bytes: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        franken_markdown::html::base64_encode(bytes)
    )
}

fn render(body: &str) -> Vec<u8> {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"80\">\
         <rect width=\"120\" height=\"80\" fill=\"white\"/>{body}</svg>"
    );
    let options = PdfOptions {
        image_assets: vec![PdfImageAsset::new("photo.svg", svg.into_bytes())],
        ..PdfOptions::default()
    };
    render_pdf("![Photographic figure](photo.svg)", &options).unwrap()
}

/// Read only the streams actually referenced by PDF pages, using their declared
/// byte lengths (binary JPEG/font data may contain arbitrary delimiter bytes).
fn page_content(pdf: &[u8]) -> String {
    let mut contents = String::new();
    for reference in String::from_utf8_lossy(pdf).split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let header = format!("\n{number} 0 obj\n");
        let start = pdf
            .windows(header.len())
            .position(|bytes| bytes == header.as_bytes())
            .unwrap()
            + header.len();
        let object = &pdf[start..];
        let stream = object
            .windows(8)
            .position(|bytes| bytes == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&object[..stream]).unwrap();
        let length: usize = dictionary
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let encoded = &object[stream + 8..stream + 8 + length];
        let decoded;
        let bytes = if dictionary.contains("/Filter /FlateDecode") {
            decoded = franken_markdown::zlib_decompress(encoded, 1 << 20).unwrap();
            decoded.as_slice()
        } else {
            assert!(!dictionary.contains("/Filter"));
            encoded
        };
        contents.push_str(std::str::from_utf8(bytes).unwrap());
    }
    assert!(!contents.is_empty());
    contents
}

#[test]
fn embedded_jpeg_is_lossless_deterministic_and_reuses_one_image_object() {
    let original = jpeg();
    let uri = data_uri(&original);
    let source = format!(
        "<image x=\"4\" y=\"5\" width=\"32\" height=\"16\" href=\"{uri}\"/>\
         <image x=\"44\" y=\"5\" width=\"32\" height=\"16\" href=\"{uri}\"/>"
    );
    let pdf = render(&source);
    assert_eq!(pdf, render(&source));
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/Subtype /Image").count(), 1);
    assert_eq!(raw.matches("/Filter /DCTDecode").count(), 1);
    assert!(raw.contains("/Width 32 /Height 16 /ColorSpace /DeviceRGB"));
    assert!(raw.contains("/Alt (Photographic figure)"));
    assert_eq!(
        pdf.windows(original.len())
            .filter(|bytes| *bytes == original.as_slice())
            .count(),
        1,
        "JPEG entropy-coded data must pass through without recompression"
    );
    let content = page_content(&pdf);
    assert!(content.contains("32 0 0 -16 4 21 cm /Im1 Do"));
    assert!(content.contains("32 0 0 -16 44 21 cm /Im1 Do"));
}

#[test]
fn jpeg_meet_slice_and_stretch_use_the_svg_image_viewport() {
    let uri = data_uri(&jpeg());
    let pdf = render(&format!(
        "<image x=\"4\" y=\"2\" width=\"10\" height=\"8\" href=\"{uri}\"/>\
         <image x=\"18\" y=\"2\" width=\"10\" height=\"8\" preserveAspectRatio=\"none\" href=\"{uri}\"/>\
         <image x=\"32\" y=\"2\" width=\"6\" height=\"8\" preserveAspectRatio=\"xMidYMid slice\" href=\"{uri}\"/>"
    ));
    let content = page_content(&pdf);
    assert!(content.contains("10 0 0 -5 4 8.5 cm /Im1 Do"), "{content}");
    assert!(content.contains("10 0 0 -8 18 10 cm /Im1 Do"), "{content}");
    assert!(
        content.contains("32 2 6 8 re W n 16 0 0 -8 27 10 cm /Im1 Do"),
        "{content}"
    );
}

#[test]
fn jpeg_preserves_svg_opacity_transform_clip_and_link() {
    let uri = data_uri(&jpeg());
    let pdf = render(&format!(
        "<defs><clipPath id=\"frame\"><rect x=\"4\" y=\"5\" width=\"20\" height=\"10\"/></clipPath></defs>\
         <a href=\"https://example.com/photo\"><image x=\"4\" y=\"5\" width=\"32\" height=\"16\" \
         transform=\"translate(3 2)\" opacity=\"0.5\" clip-path=\"url(#frame)\" href=\"{uri}\"/></a>"
    ));
    let content = page_content(&pdf);
    assert!(content.contains("/GSa05000500 gs"), "{content}");
    assert!(content.contains("1 0 0 1 3 2 cm"), "{content}");
    assert!(
        content.contains("4 5 m 24 5 l 24 15 l 4 15 l h W n"),
        "{content}"
    );
    assert!(content.contains("/Im1 Do"), "{content}");
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("/URI (https://example.com/photo)"));
}

#[test]
fn jpeg_data_urls_accept_base64_percent_encoding_and_xlink() {
    let bytes = jpeg();
    let encoded = franken_markdown::html::base64_encode(&bytes);
    let percent: String = bytes.iter().map(|byte| format!("%{byte:02X}")).collect();
    let baseline = render(&format!(
        "<image width=\"32\" height=\"16\" href=\"data:image/jpeg;base64,{encoded}\"/>"
    ));
    for uri in [
        format!("DATA:IMAGE/JPEG;BASE64,{encoded}"),
        format!("data:image/jpeg;charset=binary;base64,{encoded}"),
        format!("data:image/jpeg,{percent}"),
    ] {
        let pdf = render(&format!(
            "<image width=\"32\" height=\"16\" xlink:href=\"{uri}\"/>"
        ));
        assert_eq!(
            pdf, baseline,
            "equivalent data URLs must share exact output"
        );
    }
}

#[test]
fn invalid_or_mislabeled_jpeg_payloads_do_not_create_image_resources() {
    let encoded = franken_markdown::html::base64_encode(&jpeg());
    for uri in [
        format!("data:image/jpeg-bogus;base64,{encoded}"),
        format!("data:image/jpeg;base64,{encoded}AAAA"),
        "data:image/jpeg;base64,AAAA".into(),
        "data:image/jpeg;base64,=AAA".into(),
        "data:image/jpeg,%FF%D8%".into(),
        "data:image/jpeg;base64,PHN2Zz48L3N2Zz4=".into(),
        format!("data:image/png;base64,{encoded}"),
        "https://example.com/photo.jpg".into(),
    ] {
        let pdf = render(&format!(
            "<image width=\"32\" height=\"16\" href=\"{uri}\"/>"
        ));
        assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
        assert!(!page_content(&pdf).contains("/Im1 Do"));
    }
}

#[test]
fn jpeg_inside_nested_svg_is_collected_and_painted() {
    let uri = data_uri(&jpeg());
    let inner = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"16\">\
         <image width=\"32\" height=\"16\" href=\"{uri}\"/></svg>"
    );
    let encoded = franken_markdown::html::base64_encode(inner.as_bytes());
    let pdf = render(&format!(
        "<image width=\"64\" height=\"32\" href=\"data:image/svg+xml;base64,{encoded}\"/>"
    ));
    assert_eq!(
        String::from_utf8_lossy(&pdf)
            .matches("/Filter /DCTDecode")
            .count(),
        1
    );
    assert!(page_content(&pdf).contains("/Im1 Do"));
}
