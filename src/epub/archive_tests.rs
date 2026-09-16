//! Archive-level checks without external ZIP or XML dependencies.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use franken_markdown::{BookInput, PdfImageAsset, build_book, crc32, parse_markdown, zlib_decompress};

const RED_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
    8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207,
    192, 240, 31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
    96, 130,
];
const BLUE_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
    8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 96, 96,
    248, 255, 31, 0, 3, 2, 1, 255, 230, 119, 11, 174, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
    96, 130,
];

struct Entry<'a> {
    name: &'a str,
    method: u16,
    crc: u32,
    size: usize,
    data: &'a [u8],
}

fn u16le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("u16 field"))
}

fn u32le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("u32 field"))
}

fn entries(bytes: &[u8]) -> Vec<Entry<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    while bytes.get(at..at + 4) == Some(b"PK\x03\x04".as_slice()) {
        assert_eq!(u16le(bytes, at + 6) & 8, 0, "no data descriptors");
        let compressed = u32le(bytes, at + 18) as usize;
        let name_len = usize::from(u16le(bytes, at + 26));
        let extra_len = usize::from(u16le(bytes, at + 28));
        let name_end = at + 30 + name_len;
        let data_start = name_end + extra_len;
        let data_end = data_start + compressed;
        out.push(Entry {
            name: std::str::from_utf8(&bytes[at + 30..name_end]).expect("UTF-8 entry name"),
            method: u16le(bytes, at + 8),
            crc: u32le(bytes, at + 14),
            size: u32le(bytes, at + 22) as usize,
            data: &bytes[data_start..data_end],
        });
        at = data_end;
    }
    assert_eq!(bytes.get(at..at + 4), Some(b"PK\x01\x02".as_slice()));
    assert_eq!(out.first().map(|entry| (entry.name, entry.method)), Some(("mimetype", 0)));
    let names: std::collections::BTreeSet<_> = out.iter().map(|entry| entry.name).collect();
    assert_eq!(names.len(), out.len(), "duplicate archive paths");
    out
}

fn adler32(bytes: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Given independently selected expected bytes, re-wrap raw DEFLATE with the
/// corresponding Adler checksum. The validating decoder plus the ZIP CRC and
/// length checks establish that the archive actually carries these bytes.
fn assert_entry(records: &[Entry<'_>], name: &str, expected: &[u8]) {
    let entry = records.iter().find(|entry| entry.name == name).expect("missing archive entry");
    assert_eq!(entry.size, expected.len(), "size for {name}");
    assert_eq!(entry.crc, crc32(expected), "CRC for {name}");
    match entry.method {
        0 => assert_eq!(entry.data, expected, "stored payload for {name}"),
        8 => {
            let mut wrapped = vec![0x78, 0x9C];
            wrapped.extend_from_slice(entry.data);
            wrapped.extend_from_slice(&adler32(expected).to_be_bytes());
            assert_eq!(zlib_decompress(&wrapped, expected.len()).as_deref(), Some(expected), "deflated payload for {name}");
        }
        other => panic!("unexpected ZIP method {other}"),
    }
}

fn image_options(bytes: &[u8]) -> HtmlOptions {
    HtmlOptions {
        title: Some("Pictures".into()),
        image_assets: vec![PdfImageAsset { destination: "pic.png".into(), bytes: bytes.to_vec() }],
        ..HtmlOptions::default()
    }
}

fn rendered_body(doc: &Document, opts: &HtmlOptions) -> String {
    let mut opts = opts.clone();
    opts.custom_css = Some(String::new());
    opts.allow_raw_html = false;
    let page = franken_markdown::html::render(doc, &opts);
    html_fragment_to_xhtml(extract_main_body(&page).expect("renderer main body"))
}

#[test]
fn single_document_packages_host_png_once_with_matching_manifest() {
    let doc = parse_markdown("# Pictures\n\n![one](pic.png) ![two](pic.png)\n");
    let opts = image_options(RED_PNG);
    let archive = render_epub(&doc, &opts).expect("EPUB");
    let records = entries(&archive);
    let body = rendered_body(&doc, &opts);
    let prepared = resources::prepare(&body).expect("resources");
    assert_eq!(prepared.resources.len(), 1);
    assert_eq!(prepared.body.matches("assets/image-1.png").count(), 2);
    assert_eq!(records.len(), 7);
    assert_entry(&records, "mimetype", MIMETYPE);
    assert_entry(&records, "OEBPS/assets/image-1.png", RED_PNG);
    assert_entry(&records, "OEBPS/chapter-1.xhtml", chapter_xhtml("Pictures", "en", &prepared.body).as_bytes());
    let id = content_identifier("Pictures", "en", &body);
    assert_entry(&records, "OEBPS/content.opf", content_opf("Pictures", "en", &id, &prepared).as_bytes());
    assert_eq!(archive, render_epub(&doc, &opts).expect("repeat EPUB"));
}

#[test]
fn image_bytes_change_identity_even_when_rewritten_chapter_is_identical() {
    let doc = parse_markdown("# Pictures\n\n![pixel](pic.png)\n");
    let red = render_epub(&doc, &image_options(RED_PNG)).expect("red EPUB");
    let blue = render_epub(&doc, &image_options(BLUE_PNG)).expect("blue EPUB");
    let red = entries(&red);
    let blue = entries(&blue);
    let r_chapter = red.iter().find(|entry| entry.name == "OEBPS/chapter-1.xhtml").unwrap();
    let b_chapter = blue.iter().find(|entry| entry.name == "OEBPS/chapter-1.xhtml").unwrap();
    assert_eq!(r_chapter.data, b_chapter.data);
    let r_opf = red.iter().find(|entry| entry.name == "OEBPS/content.opf").unwrap();
    let b_opf = blue.iter().find(|entry| entry.name == "OEBPS/content.opf").unwrap();
    assert_ne!(r_opf.data, b_opf.data, "asset bytes must affect identifier");
    assert_entry(&red, "OEBPS/assets/image-1.png", RED_PNG);
    assert_entry(&blue, "OEBPS/assets/image-1.png", BLUE_PNG);
}

#[test]
fn custom_styles_are_packaged_scoped_and_affect_identity() {
    let doc = parse_markdown("# Title\n");
    let css = ".fmd { line-height: 1.8; }";
    let opts = HtmlOptions { title: Some("Title".into()), custom_css: Some(css.into()), ..HtmlOptions::default() };
    let styled = render_epub(&doc, &opts).expect("styled EPUB");
    let records = entries(&styled);
    assert_entry(&records, "OEBPS/style.css", css.as_bytes());
    let body = format!("<main class=\"fmd\">\n{}</main>\n", rendered_body(&doc, &opts));
    assert_entry(&records, "OEBPS/chapter-1.xhtml", chapter_xhtml("Title", "en", &body).as_bytes());
    let plain_opts = HtmlOptions { custom_css: None, ..opts };
    let plain = render_epub(&doc, &plain_opts).expect("plain EPUB");
    let plain = entries(&plain);
    assert_ne!(records.iter().find(|e| e.name == "OEBPS/content.opf").unwrap().data, plain.iter().find(|e| e.name == "OEBPS/content.opf").unwrap().data);
}

#[test]
fn book_archive_contains_every_chapter_and_isolated_image_payload() {
    let book = build_book(&[
        BookInput { path: "first.md".into(), source: "# First\n\n![pixel](pic.png)\n".into() },
        BookInput { path: "second.md".into(), source: "# Second\n\n![pixel](pic.png)\n".into() },
    ]).expect("book");
    let opts = image_options(RED_PNG);
    let archive = render_book_epub(&book, &opts).expect("book EPUB");
    let records = entries(&archive);
    assert_eq!(records.len(), 9);
    assert_entry(&records, "mimetype", MIMETYPE);
    for (index, chapter) in book.chapters.iter().enumerate() {
        let mut chapter_opts = opts.clone();
        chapter_opts.title = Some(chapter.title.clone());
        let body = rendered_body(&chapter.doc, &chapter_opts);
        let prefix = format!("chapter-{}-", index + 1);
        let prepared = resources::prepare_with_prefix(&body, &prefix).expect("chapter resources");
        let wrapped = format!("<main class=\"fmd\">\n{}</main>\n", prepared.body);
        assert_entry(&records, &format!("OEBPS/chapter-{}.xhtml", index + 1), chapter_xhtml(&chapter.title, "en", &wrapped).as_bytes());
        assert_entry(&records, &format!("OEBPS/assets/{prefix}image-1.png"), RED_PNG);
    }
}
