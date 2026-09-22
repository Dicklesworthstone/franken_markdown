//! Real parsed-book/font/ZIP regression cases. No renderer doubles.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use franken_markdown::{BookInput, FontAssetSlot, FontAssets, FontFamily, build_book, crc32, zlib_decompress};
use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::text::Font;
use franken_markdown::book::BookRenderer;

fn options() -> HtmlOptions {
    HtmlOptions {
        title: Some("Atlas".into()),
        font_assets: FontAssets::default().with_slot(FontAssetSlot::BodyRegular,
            fonts::body_bytes(FontFamily::Sans, FontStyle::Regular).to_vec()).unwrap(),
        ..HtmlOptions::default()
    }
}

fn chapters(sources: &[(&str, &str)]) -> Book {
    build_book(&sources.iter().map(|(path, source)| BookInput {
        path: (*path).into(), source: (*source).into(),
    }).collect::<Vec<_>>()).unwrap()
}

struct Entry<'a> { name: &'a str, method: u16, crc: u32, size: usize, payload: &'a [u8] }
fn entries(bytes: &[u8]) -> Vec<Entry<'_>> {
    let u16_at = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let mut result = Vec::new(); let mut at = 0;
    while bytes.get(at..at + 4) == Some(b"PK\x03\x04".as_slice()) {
        assert_eq!(u16_at(at + 6) & 8, 0);
        let name_end = at + 30 + usize::from(u16_at(at + 26));
        let start = name_end + usize::from(u16_at(at + 28));
        let end = start + u32_at(at + 18) as usize;
        result.push(Entry { name: std::str::from_utf8(&bytes[at + 30..name_end]).unwrap(),
            method: u16_at(at + 8), crc: u32_at(at + 14), size: u32_at(at + 22) as usize,
            payload: &bytes[start..end] });
        at = end;
    }
    assert_eq!(bytes.get(at..at + 4), Some(b"PK\x01\x02".as_slice()));
    assert_eq!(result[0].name, "mimetype"); assert_eq!(result[0].method, 0);
    assert_eq!(result[0].payload, MIMETYPE);
    assert_eq!(result.iter().map(|entry| entry.name).collect::<std::collections::BTreeSet<_>>().len(), result.len());
    result
}
fn assert_payload(entries: &[Entry<'_>], name: &str, expected: &[u8]) {
    let entry = entries.iter().find(|entry| entry.name == name).expect("archive resource exists");
    assert_eq!(entry.size, expected.len(), "{name}"); assert_eq!(entry.crc, crc32(expected), "{name}");
    if entry.method == 0 { assert_eq!(entry.payload, expected); return; }
    assert_eq!(entry.method, 8);
    let mut a = 1u32; let mut b = 0u32;
    for byte in expected { a = (a + u32::from(*byte)) % 65521; b = (b + a) % 65521; }
    let mut wrapped = vec![0x78, 0x9c]; wrapped.extend_from_slice(entry.payload);
    wrapped.extend_from_slice(&((b << 16) | a).to_be_bytes());
    assert_eq!(zlib_decompress(&wrapped, expected.len()).as_deref(), Some(expected), "{name}");
}

#[test]
fn all_chapters_and_navigation_link_a_single_declared_font_package() {
    let book = chapters(&[("a.md", "# First\n\nPlain\n"), ("b.md", "# Second\n\nText\n")]);
    let opts = options(); let prepared = prepare_book(&book, &opts).unwrap();
    let output = render_book_epub(&book, &opts).unwrap(); let records = entries(&output);
    let font_count = records.iter().filter(|entry| entry.name.starts_with("OEBPS/fonts/")).count();
    assert!(font_count > 0 && font_count <= 6);
    assert_eq!(prepared.opf.matches("media-type=\"font/ttf\"").count(), font_count);
    assert_eq!(prepared.opf.matches("<itemref ").count(), 2);
    assert!(!prepared.opf.split_once("<spine>").unwrap().1.contains("epub-font"));
    assert_payload(&records, "OEBPS/content.opf", prepared.opf.as_bytes());
    assert_payload(&records, "OEBPS/nav.xhtml", prepared.nav.as_bytes());
    for chapter in &prepared.chapters {
        assert_eq!(chapter.xhtml.matches("href=\"embedded-fonts.css\"").count(), 1);
        assert_payload(&records, &format!("OEBPS/{}", chapter.file), chapter.xhtml.as_bytes());
    }
    assert_eq!(prepared.nav.matches("href=\"embedded-fonts.css\"").count(), 1);
    assert_eq!(records.iter().filter(|entry| entry.name == "OEBPS/embedded-fonts.css").count(), 1);
    assert_eq!(output, render_book_epub(&book, &opts).unwrap());
}

#[test]
fn font_payload_includes_glyphs_unique_to_later_chapters_and_metadata_titles() {
    let mut book = chapters(&[("a.md", "# First\n\nASCII\n"), ("b.md", "# Last\n\nCafé\n")]);
    book.chapters[1].title = "Über".into();
    let opts = options(); let output = render_book_epub(&book, &opts).unwrap();
    // For this fixture all other emitted/CSS characters are ASCII. Select the
    // expected repertoire independently rather than reading private collector state.
    let mut keep: std::collections::BTreeSet<char> = (' '..='~').collect();
    keep.extend(['\n', '\u{a0}', '•', '◦', '▪', '↩', 'é', 'Ü']);
    let keep = keep.into_iter().collect::<Vec<_>>();
    let expected = fonts::load_body(FontFamily::Sans, FontStyle::Regular).unwrap().subset(&keep).unwrap();
    assert_payload(&entries(&output), "OEBPS/fonts/font-1.ttf", &expected);
    let font = Font::parse(expected).unwrap();
    for ch in ['é', 'Ü'] { assert_ne!(font.glyph_index(ch), 0); }
}

#[test]
fn repeated_chapters_do_not_multiply_font_resources_or_subset_payloads() {
    let one = chapters(&[("one.md", "# Same\n\nSame words\n")]);
    let many = build_book(&(0..20).map(|i| BookInput {
        path: format!("part-{i}.md"), source: "# Same\n\nSame words\n".into(),
    }).collect::<Vec<_>>()).unwrap();
    let opts = options();
    let first = prepare_book(&one, &opts).unwrap(); let repeated = prepare_book(&many, &opts).unwrap();
    assert_eq!(first.fonts.fingerprint(), repeated.fonts.fingerprint());
    assert_eq!(first.fonts.byte_len(), repeated.fonts.byte_len());
    let single = render_book_epub(&one, &opts).unwrap(); let multiple = render_book_epub(&many, &opts).unwrap();
    let first_entries = entries(&single); let repeated_entries = entries(&multiple);
    let select = |records: &[Entry<'_>]| records.iter().filter(|entry| entry.name.starts_with("OEBPS/fonts/"))
        .map(|entry| (entry.name.to_string(), entry.crc, entry.size)).collect::<Vec<_>>();
    assert_eq!(select(&first_entries), select(&repeated_entries));
}

#[test]
fn author_css_keeps_precedence_and_exact_bytes_across_the_whole_book() {
    let book = chapters(&[("a.md", "# First"), ("b.md", "# Last")]);
    for css in [None, Some(""), Some("body{font-family:fantasy;}\n")] {
        let mut opts = options(); opts.custom_css = css.map(str::to_string);
        let prepared = prepare_book(&book, &opts).unwrap();
        for page in prepared.chapters.iter().map(|chapter| &chapter.xhtml).chain(std::iter::once(&prepared.nav)) {
            let font = page.find("href=\"embedded-fonts.css\"").unwrap();
            let style = page.find("href=\"style.css\"").unwrap();
            assert_eq!(font < style, css.is_some());
        }
        let output = render_book_epub(&book, &opts).unwrap();
        assert_payload(&entries(&output), "OEBPS/style.css", prepared.css.as_bytes());
    }
}

#[test]
fn font_choice_changes_book_identity_without_changing_source_documents() {
    let book = chapters(&[("a.md", "# First"), ("b.md", "# Last")]);
    let originals: Vec<_> = book.chapters.iter().map(|chapter| chapter.doc.clone()).collect();
    let mut opts = options(); let before = opts.font_assets.clone();
    let first = prepare_book(&book, &opts).unwrap();
    assert_eq!(opts.font_assets, before);
    opts.font_assets.set_slot(FontAssetSlot::BodyRegular,
        fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec()).unwrap();
    let second = prepare_book(&book, &opts).unwrap();
    assert_ne!(first.opf, second.opf);
    assert_eq!(first.chapters[0].xhtml, second.chapters[0].xhtml);
    assert_eq!(originals, book.chapters.iter().map(|chapter| chapter.doc.clone()).collect::<Vec<_>>());
}

#[test]
fn image_resources_and_rewritten_chapter_links_survive_font_packaging() {
    let book = chapters(&[("guide/a.md", "# First\n\n[Next](../b.md#last)\n\n![Shape](figure.svg)\n"),
        ("b.md", "# Last\n\n[Back](guide/a.md#first)\n")]);
    let mut opts = options();
    opts.image_assets.push(PdfImageAsset { destination: "guide/figure.svg".into(),
        bytes: b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>".to_vec() });
    let prepared = prepare_book(&book, &opts).unwrap();
    assert!(prepared.chapters[0].xhtml.contains("href=\"chapter-2.xhtml#last\""));
    assert!(prepared.chapters[1].xhtml.contains("href=\"chapter-1.xhtml#first\""));
    assert!(prepared.opf.contains("properties=\"svg\""));
    let image = &prepared.chapters[0].content.resources[0];
    let output = render_book_epub(&book, &opts).unwrap();
    assert_payload(&entries(&output), &format!("OEBPS/{}", image.href), &opts.image_assets[0].bytes);
    assert!(prepared.opf.contains("media-type=\"font/ttf\""));
}

#[test]
fn retained_source_bundle_uses_expanded_text_and_reuses_the_book_font_options() {
    let source = [BookInput { path: "start.md".into(), source: "# Start\n\n{{#include shared.md}}\n".into() }];
    let includes = [BookInput { path: "shared.md".into(), source: "Café\n".into() }];
    let mut renderer = BookRenderer::from_sources(&source, &includes).unwrap();
    let opts = options();
    renderer.options_mut().title = opts.title.clone();
    renderer.options_mut().font_assets = opts.font_assets.clone();
    let expected = chapters(&[("start.md", "# Start\n\nCafé\n")]);
    assert_eq!(renderer.render_epub().unwrap(), render_book_epub(&expected, &opts).unwrap());
    renderer.options_mut().font_assets.body_regular = Some(b"bad font".to_vec());
    assert!(renderer.render_epub().is_err());
}

#[test]
fn default_book_archive_and_identity_remain_on_the_font_free_path() {
    let book = chapters(&[("a.md", "# A"), ("b.md", "# B")]);
    let opts = HtmlOptions::default(); let prepared = prepare_book(&book, &opts).unwrap();
    assert!(prepared.fonts.fingerprint().is_none());
    assert_eq!(prepared.fonts.byte_len(), 0);
    assert!(!prepared.opf.contains("epub-font"));
    assert!(!prepared.nav.contains("embedded-fonts"));
    let mut old = ZipWriter::new();
    old.add_stored("mimetype", MIMETYPE);
    old.add_deflated("META-INF/container.xml", CONTAINER_XML.as_bytes());
    old.add_deflated("OEBPS/content.opf", prepared.opf.as_bytes());
    old.add_deflated("OEBPS/nav.xhtml", prepared.nav.as_bytes());
    old.add_deflated("OEBPS/style.css", prepared.css.as_bytes());
    for chapter in &prepared.chapters { old.add_deflated(&format!("OEBPS/{}", chapter.file), chapter.xhtml.as_bytes()); }
    assert_eq!(old.finish(), render_book_epub(&book, &opts).unwrap());
    let mut bad = opts; bad.font_assets.body_bold_weight = Some(1001);
    assert!(render_book_epub(&book, &bad).is_err());
}
