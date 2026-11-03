//! Source-carried image bytes remain portable publication resources.

use franken_markdown::{
    BookInput, HtmlOptions, PdfImageAsset, build_book, parse_markdown, render_epub,
};
use std::fmt::Write as _;

const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAE0lEQVR4nGP4z8DwHwyBNIhgAAA/0gX7yxemzgAAAABJRU5ErkJggg==";
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><rect width="32" height="16" fill="#ff0000"/></svg>"##;
const ANIMATION: &str = include_str!("../fixtures/gif/animated-colors.base64");

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

fn fixtures() -> Vec<(&'static str, &'static str, Vec<u8>)> {
    let jpeg: String = include_str!("../fixtures/jpeg-quadrants.hex")
        .split_whitespace()
        .collect();
    vec![
        ("image/png", ".png", fixture_base64(PNG)),
        (
            "image/jpeg",
            ".jpg",
            jpeg.as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect(),
        ),
        ("image/gif", ".gif", fixture_base64(ANIMATION)),
        ("image/svg+xml", ".svg", SVG.to_vec()),
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

#[test]
fn markdown_and_html_data_images_become_exact_manifested_epub_resources() {
    let fixtures = fixtures();
    let mut source = "# Portable illustrations\n\n".to_string();
    for (mime, _, payload) in &fixtures {
        let destination = uri(mime, payload);
        writeln!(source, "![Markdown image]({destination})\n").unwrap();
        writeln!(
            source,
            "<img src=\"{destination}\" alt=\"Raw image\" width=\"32\">\n"
        )
        .unwrap();
    }
    let document = parse_markdown(&source);
    let before = document.clone();
    let options = HtmlOptions::default();
    let archive = render_epub(&document, &options).unwrap();
    let entries = super::parse_central_directory(&archive);
    let resources: Vec<_> = entries
        .iter()
        .filter(|entry| entry.name.starts_with("OEBPS/assets/"))
        .collect();
    assert_eq!(
        resources.len(),
        4,
        "the same Markdown/raw image shares one asset"
    );
    let xhtml = super::entry_text(&archive, "OEBPS/chapter-1.xhtml");
    let opf = super::entry_text(&archive, "OEBPS/content.opf");
    super::assert_well_formed_xml(&xhtml);
    super::assert_well_formed_xml(&opf);
    assert_eq!(xhtml.matches("<img ").count(), 8);
    assert!(!xhtml.contains("src=\"data:"));
    assert!(!xhtml.contains("Image unavailable"));
    for (mime, extension, expected) in fixtures {
        let entry = resources
            .iter()
            .find(|entry| entry.name.ends_with(extension))
            .unwrap();
        assert_eq!(super::recover_entry(&archive, entry), expected, "{mime}");
        let href = entry.name.strip_prefix("OEBPS/").unwrap();
        assert!(
            opf.contains(&format!("href=\"{href}\" media-type=\"{mime}\"")),
            "{mime}"
        );
        assert_eq!(
            xhtml.matches(&format!("src=\"{href}\"")).count(),
            2,
            "{mime}"
        );
    }
    assert_eq!(document, before);
    assert_eq!(archive, render_epub(&document, &options).unwrap());
}

#[test]
fn books_share_embedded_animation_across_chapters_and_keep_navigation() {
    let bytes = fixture_base64(ANIMATION);
    let canonical = uri("image/gif", &bytes);
    let alternate = format!(
        "DATA:IMAGE/GIF;BASE64,{}",
        percent_encode(ANIMATION.trim().as_bytes())
    );
    let book = build_book(&[
        BookInput {
            path: "guide/start.md".into(),
            source: format!("# Start\n\n![Animation]({canonical})\n\n[Next](../end.md#end)\n"),
        },
        BookInput {
            path: "end.md".into(),
            source: format!("# End\n\n<img src=\"{alternate}\" alt=\"Animation\">\n\n[Back](guide/start.md#start)\n"),
        },
    ]).unwrap();
    let documents: Vec<_> = book
        .chapters
        .iter()
        .map(|chapter| chapter.doc.clone())
        .collect();
    let options = HtmlOptions::default();
    let archive = franken_markdown::epub::render_book_epub(&book, &options).unwrap();
    let entries = super::parse_central_directory(&archive);
    let resources: Vec<_> = entries
        .iter()
        .filter(|entry| entry.name.starts_with("OEBPS/assets/"))
        .collect();
    assert_eq!(
        resources.len(),
        1,
        "equivalent data encodings share original GIF bytes"
    );
    assert!(resources[0].name.ends_with(".gif"));
    assert_eq!(super::recover_entry(&archive, resources[0]), bytes);
    let href = resources[0].name.strip_prefix("OEBPS/").unwrap();
    for (page, destination) in [
        ("OEBPS/chapter-1.xhtml", "chapter-2.xhtml#end"),
        ("OEBPS/chapter-2.xhtml", "chapter-1.xhtml#start"),
    ] {
        let xhtml = super::entry_text(&archive, page);
        super::assert_well_formed_xml(&xhtml);
        assert!(xhtml.contains(&format!("src=\"{href}\"")));
        assert!(xhtml.contains(&format!("href=\"{destination}\"")));
        assert!(!xhtml.contains("src=\"data:"));
    }
    let nav = super::entry_text(&archive, "OEBPS/nav.xhtml");
    super::assert_well_formed_xml(&nav);
    assert!(nav.contains("chapter-1.xhtml"));
    assert!(nav.contains("chapter-2.xhtml"));
    let site = franken_markdown::book::render_book_site(&book, &options).unwrap();
    for (page, destination) in [
        ("guide__start.html", "end.html#end"),
        ("end.html", "guide__start.html#start"),
    ] {
        let html = super::entry_text(&site, page);
        assert!(html.contains(&format!("src=\"{canonical}\"")));
        assert!(html.contains(&format!("href=\"{destination}\"")));
    }
    for (chapter, expected) in book.chapters.iter().zip(documents) {
        assert_eq!(chapter.doc, expected);
    }
    assert_eq!(
        archive,
        franken_markdown::epub::render_book_epub(&book, &options).unwrap()
    );
}

#[test]
fn percent_encoded_svg_is_packaged_after_the_existing_style_cleanup() {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><style>@import url('https://example.invalid/font.css'); rect{fill:blue}</style><rect width='16' height='16'/></svg>";
    let cleaned = svg.replace("@import url('https://example.invalid/font.css'); ", "");
    let source = format!(
        "![Vector](data:image/svg+xml;charset=utf-8,{})",
        percent_encode(svg.as_bytes())
    );
    let archive = render_epub(&parse_markdown(&source), &HtmlOptions::default()).unwrap();
    let entries = super::parse_central_directory(&archive);
    let resource = entries
        .iter()
        .find(|entry| entry.name.starts_with("OEBPS/assets/"))
        .unwrap();
    assert!(resource.name.ends_with(".svg"));
    assert_eq!(super::recover_entry(&archive, resource), cleaned.as_bytes());
    let xhtml = super::entry_text(&archive, "OEBPS/chapter-1.xhtml");
    super::assert_well_formed_xml(&xhtml);
    assert!(xhtml.contains("alt=\"Vector\""));
    assert!(!xhtml.contains("data:image/"));
}

#[test]
fn rejected_payloads_and_invalid_overrides_never_enter_the_epub_manifest() {
    let valid = uri("image/png", &fixture_base64(PNG));
    let mismatch = uri("image/png", SVG);
    let source = format!(
        "# Rejected images\n\n![Invalid data](data:image/png;base64,invalid)\n\n![Mismatched data]({mismatch})\n\n<img src=\"data:text/html;base64,PHN2Zy8+\" alt=\"Non-image data\">\n\n![Invalid override]({valid})\n\n[Blocked data link]({valid})\n"
    );
    let options = HtmlOptions {
        image_assets: vec![
            PdfImageAsset::new(&valid, Vec::new()),
            PdfImageAsset::new(&valid, SVG.to_vec()),
        ],
        ..HtmlOptions::default()
    };
    let archive = render_epub(&parse_markdown(&source), &options).unwrap();
    let entries = super::parse_central_directory(&archive);
    assert!(
        !entries
            .iter()
            .any(|entry| entry.name.starts_with("OEBPS/assets/"))
    );
    let xhtml = super::entry_text(&archive, "OEBPS/chapter-1.xhtml");
    super::assert_well_formed_xml(&xhtml);
    assert!(!xhtml.contains("<img"));
    assert!(!xhtml.contains("<a href=\"data:"));
    for alt in [
        "Invalid data",
        "Mismatched data",
        "Non-image data",
        "Invalid override",
        "Blocked data link",
    ] {
        assert!(xhtml.contains(alt), "{alt}");
    }
    let opf = super::entry_text(&archive, "OEBPS/content.opf");
    assert!(!opf.contains("media-type=\"image/"));
}
