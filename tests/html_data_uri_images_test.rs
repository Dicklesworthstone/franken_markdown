//! Portable source images remain images without weakening general URL policy.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Document, Inline};
use franken_markdown::{HtmlOptions, PdfImageAsset, render_html, render_html_document};
use std::fmt::Write as _;

const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAE0lEQVR4nGP4z8DwHwyBNIhgAAA/0gX7yxemzgAAAABJRU5ErkJggg==";
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><rect width="32" height="16" fill="#ff0000"/></svg>"##;

fn fixture_base64(source: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut value = 0u32;
    let mut bits = 0;
    for byte in source.trim().bytes().take_while(|&byte| byte != b'=') {
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
            out.push((value >> bits) as u8);
        }
    }
    out
}

fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    let jpeg: String = include_str!("fixtures/jpeg-quadrants.hex")
        .split_whitespace()
        .collect();
    vec![
        ("image/png", fixture_base64(PNG)),
        (
            "image/jpeg",
            jpeg.as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect(),
        ),
        (
            "image/gif",
            fixture_base64(include_str!("fixtures/gif/animated-colors.base64")),
        ),
        ("image/svg+xml", SVG.to_vec()),
    ]
}

fn uri(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        franken_markdown::html::base64_encode(bytes)
    )
}

fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        write!(out, "%{byte:02X}").unwrap();
    }
    out
}

fn source_pair(destination: &str) -> String {
    format!(
        "![Markdown alt](<{destination}> \"Caption & detail\")\n\n<img src=\"{destination}\" alt=\"Raw alt\" title=\"Caption &amp; detail\" width=\"32\">\n"
    )
}

#[test]
fn embedded_raster_vector_and_animated_images_survive_markdown_and_safe_html() {
    for (mime, bytes) in fixtures() {
        let destination = uri(mime, &bytes);
        let html = render_html(&source_pair(&destination), &HtmlOptions::default()).unwrap();
        assert_eq!(html.matches("<img ").count(), 2, "{mime}");
        assert_eq!(
            html.matches(&format!("src=\"{destination}\"")).count(),
            2,
            "{mime}"
        );
        assert!(html.contains("alt=\"Markdown alt\""));
        assert!(html.contains("alt=\"Raw alt\""));
        assert_eq!(html.matches("title=\"Caption &amp; detail\"").count(), 2);
    }
}

#[test]
fn image_payload_spelling_normalizes_without_altering_bytes() {
    for (mime, bytes) in fixtures() {
        let canonical = uri(mime, &bytes);
        for source in [
            format!(
                "DaTa:{};ChArSeT=UtF-8;BaSe64,{}",
                mime.to_ascii_uppercase(),
                percent_encode(franken_markdown::html::base64_encode(&bytes).as_bytes())
            ),
            format!("data:{mime},{}", percent_encode(&bytes)),
        ] {
            let html = render_html(&source_pair(&source), &HtmlOptions::default()).unwrap();
            assert_eq!(
                html.matches(&format!("src=\"{canonical}\"")).count(),
                2,
                "{mime}"
            );
            assert!(!html.contains(&source));
        }
    }
}

#[test]
fn embedded_images_survive_references_and_nested_document_content() {
    let destination = uri("image/svg+xml", SVG);
    let source = format!(
        "# Heading ![Heading]({destination})\n\n![Reference][figure]\n\n[figure]: {destination}\n\n> ![Quote]({destination})\n\n- ![List]({destination})\n\n| Image |\n| --- |\n| ![Cell]({destination}) |\n\nNote[^n].\n\n[^n]: ![Footnote]({destination})\n"
    );
    let html = render_html(&source, &HtmlOptions::default()).unwrap();
    assert_eq!(html.matches(&format!("src=\"{destination}\"")).count(), 6);
    for alt in ["Heading", "Reference", "Quote", "List", "Cell", "Footnote"] {
        assert!(html.contains(&format!("alt=\"{alt}\"")), "{alt}");
    }
}

#[test]
fn explicit_bindings_win_before_decode_and_invalid_first_bindings_suppress_fallback() {
    let broken = "data:image/png;base64,not-a-png";
    let options = HtmlOptions {
        image_assets: vec![
            PdfImageAsset::new(format!("  {broken}  "), SVG.to_vec()),
            PdfImageAsset::new(broken, fixture_base64(PNG)),
        ],
        ..HtmlOptions::default()
    };
    let html = render_html(&source_pair(broken), &options).unwrap();
    assert_eq!(html.matches(&uri("image/svg+xml", SVG)).count(), 2);
    assert_eq!(options.image_assets[0].bytes, SVG);
    assert_eq!(options.image_assets[0].destination, format!("  {broken}  "));
    assert_eq!(options.image_assets[1].bytes, fixture_base64(PNG));

    let valid = uri("image/png", &fixture_base64(PNG));
    for bytes in [Vec::new(), b"not an image".to_vec()] {
        let options = HtmlOptions {
            image_assets: vec![
                PdfImageAsset::new(&valid, bytes),
                PdfImageAsset::new(&valid, SVG.to_vec()),
            ],
            ..HtmlOptions::default()
        };
        let html = render_html(&source_pair(&valid), &options).unwrap();
        assert!(!html.contains("<img"));
        assert!(html.contains("Markdown alt"));
        assert!(html.contains("Raw alt"));
        assert!(!html.contains("data:image/"));
    }
}

#[test]
fn data_links_and_executable_or_nonimage_destinations_remain_blocked() {
    let image = uri("image/png", &fixture_base64(PNG));
    let source = format!("[Link]({image})\n\n<a href=\"{image}\">Raw link</a>");
    let html = render_html(&source, &HtmlOptions::default()).unwrap();
    assert!(!html.contains("<a href="));
    assert!(html.contains("Link"));
    assert!(html.contains("Raw link"));
    assert!(!html.contains(&image));

    for destination in [
        "javascript:alert(1)",
        "vbscript:payload",
        "file:///secret.png",
        "data:text/html;base64,PHN2Zy8+",
        "data:application/xhtml+xml;base64,PHN2Zy8+",
        "data:image/webp;base64,PHN2Zy8+",
        "da%74a:image/png;base64,invalid",
    ] {
        let options = HtmlOptions {
            image_assets: vec![PdfImageAsset::new(destination, SVG.to_vec())],
            ..HtmlOptions::default()
        };
        let html = render_html(&source_pair(destination), &options).unwrap();
        assert!(!html.contains("<img"), "{destination}");
        assert!(html.contains("Markdown alt"), "{destination}");
        assert!(html.contains("Raw alt"), "{destination}");
    }
}

#[test]
fn malformed_mismatched_and_unbounded_images_keep_visible_alt_text() {
    let mut corrupt_png = fixture_base64(PNG);
    corrupt_png[29] ^= 1; // IHDR CRC: a signature alone is not a valid image.
    let mut rejected = vec![
        "data:image/png;base64,".to_string(),
        "data:image/png;base64,%%%".to_string(),
        "data:image/png;base64,AAAAA".to_string(),
        "data:image/png;base64,AA==trailing".to_string(),
        "data:image/png;charset=latin1;base64,AAAA".to_string(),
        "data:image/png;base64;charset=utf-8,AAAA".to_string(),
        "data:image/svg+xml,%3".to_string(),
        format!("data:image/png;{},AAAA", "x".repeat(256)),
        uri("image/jpeg", &fixture_base64(PNG)),
        uri("image/png", SVG),
        uri("image/gif", b"GIF89a"),
        uri("image/png", &corrupt_png),
        uri("image/jpeg", &[0xff, 0xd8, 0xff, 0xd9]),
    ];
    for svg in [
        "<html><svg/></html>",
        "<svgscript/>",
        "<svg><g></svg>",
        "<!DOCTYPE svg [<!ENTITY payload SYSTEM 'file:///secret'>]><svg/>",
        "<svg width='0' height='1'/>",
        "<svg width='16385' height='1'/>",
        "<svg width='11000' height='11000'/>",
    ] {
        rejected.push(uri("image/svg+xml", svg.as_bytes()));
    }
    for (index, destination) in rejected.iter().enumerate() {
        let html = render_html(&source_pair(destination), &HtmlOptions::default()).unwrap();
        assert!(!html.contains("<img"), "rejected image {index}");
        assert!(html.contains("Markdown alt"), "rejected image {index}");
        assert!(html.contains("Raw alt"), "rejected image {index}");
        assert!(!html.contains(destination), "rejected image {index}");
    }
}

#[test]
fn svg_stays_in_image_context_and_reuses_remote_import_cleanup() {
    let original = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" onload="alert(1)"><style>@import url('https://example.invalid/style.css'); rect{fill:red}</style><script>alert(2)</script><foreignObject><div xmlns="http://www.w3.org/1999/xhtml">Content</div></foreignObject><rect width="16" height="16"><animate attributeName="opacity" values="0;1" dur="1s"/></rect></svg>"#;
    let cleaned = std::str::from_utf8(original)
        .unwrap()
        .replace("@import url('https://example.invalid/style.css'); ", "");
    let destination = uri("image/svg+xml", original);
    let source = format!("{}\n[Open image]({destination})", source_pair(&destination));
    let html = render_html(&source, &HtmlOptions::default()).unwrap();
    assert_eq!(
        html.matches(&uri("image/svg+xml", cleaned.as_bytes()))
            .count(),
        2
    );
    for markup in [
        "<script",
        "<svg",
        "<foreignObject",
        "onload=",
        "<a href=",
        "@import",
    ] {
        assert!(!html.contains(markup), "outer document contains {markup}");
    }
    assert!(html.contains("Open image"));
}

#[test]
fn decoded_data_and_explicit_bindings_enforce_the_image_byte_limit() {
    const LIMIT: usize = 32 * 1024 * 1024;
    let mut payload = Vec::with_capacity(LIMIT + 64);
    payload.extend_from_slice(b"<svg width='1' height='1'><desc>");
    payload.resize(LIMIT, b'x');
    payload.extend_from_slice(b"</desc></svg>");
    let mut destination = String::with_capacity(payload.len() + 19);
    destination.push_str("data:image/svg+xml,");
    destination.push_str(std::str::from_utf8(&payload).unwrap());
    let document = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Image {
            dest: destination,
            title: None,
            alt: "Oversized source".into(),
        }])],
    };
    let html = render_html_document(&document, &HtmlOptions::default()).unwrap();
    assert!(html.contains("<p>Oversized source</p>"));
    assert!(!html.contains("<img"));
    drop(document);

    let destination = uri("image/svg+xml", SVG);
    let options = HtmlOptions {
        image_assets: vec![
            PdfImageAsset::new(&destination, payload),
            PdfImageAsset::new(&destination, SVG.to_vec()),
        ],
        ..HtmlOptions::default()
    };
    let html = render_html(&source_pair(&destination), &options).unwrap();
    assert!(!html.contains("<img"));
    assert!(html.contains("Markdown alt"));
    assert!(html.contains("Raw alt"));
}
