//! Native renderer tests; no JavaScript or HTML stand-in is used here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::book::BookWorkspace;
use crate::{BookInput, PdfImageAsset, crc32, zlib_decompress};

fn renderer() -> BookRenderer {
    let mut renderer = BookRenderer::new(&[
        BookInput {
            path: "guide/start.md".into(),
            source: "---\nlang: fr\n---\n# Start\n\n[Next](../end.md#end) ![Figure](figure.svg)\n\n> Quote[^1]\n\n[^1]: A note\n".into(),
        },
        BookInput {
            path: "end.md".into(),
            source: "# End\n\n[Back](guide/start.md#start)\n\nUnselectedBodySentinel\n".into(),
        },
    ]).unwrap();
    renderer.options_mut().custom_css = Some(".fmd{line-height:1.6}".into());
    renderer.options_mut().lang = Some("de".into());
    renderer.set_image("guide/figure.svg", br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#.to_vec()).unwrap();
    renderer
}

fn u16_at(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()))
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

// Read the real ZIP local entry, validate CRC/size, then re-wrap raw DEFLATE
// with the expected Adler checksum for the crate's validating zlib decoder.
fn assert_zip_page(archive: &[u8], name: &str, expected: &[u8]) {
    let mut at = 0;
    while archive.get(at..at + 4) == Some(b"PK\x03\x04".as_slice()) {
        let size = u32_at(archive, at + 18) as usize;
        let name_len = u16_at(archive, at + 26);
        let start = at + 30 + name_len + u16_at(archive, at + 28);
        if &archive[at + 30..at + 30 + name_len] == name.as_bytes() {
            assert_eq!(u32_at(archive, at + 22) as usize, expected.len());
            assert_eq!(u32_at(archive, at + 14), crc32(expected));
            assert_eq!(u16_at(archive, at + 8), 8);
            let (mut a, mut b) = (1u32, 0u32);
            for byte in expected {
                a = (a + u32::from(*byte)) % 65521;
                b = (b + a) % 65521;
            }
            let mut wrapped = vec![0x78, 0x9c];
            wrapped.extend_from_slice(&archive[start..start + size]);
            wrapped.extend_from_slice(&((b << 16) | a).to_be_bytes());
            assert_eq!(zlib_decompress(&wrapped, expected.len()).as_deref(), Some(expected));
            return;
        }
        at = start + size;
    }
    panic!("missing chapter {name}");
}

#[test]
fn selected_html_is_byte_identical_to_the_real_site_archive_member() {
    let renderer = renderer();
    let archive = renderer.render_site().unwrap();
    for index in 0..renderer.book.chapters.len() {
        let html = renderer.render_chapter_html(index).unwrap();
        assert_zip_page(&archive, &renderer.book.chapters[index].out_name, html.as_bytes());
        assert_eq!(renderer.render_chapter_html(index).unwrap(), html);
    }
}

#[test]
fn chapter_preview_preserves_links_images_language_and_is_read_only() {
    let renderer = renderer();
    let original = renderer.book.chapters[0].doc.clone();
    let assets = renderer.options.pdf_image_assets.clone();
    let html = renderer.render_chapter_html(0).unwrap();
    assert!(html.contains("href=\"end.html#end\""));
    assert!(html.contains("lang=\"fr\""));
    assert!(html.contains("data:image/svg+xml;base64,"));
    assert!(html.contains("fmd-book-nav"));
    assert!(!html.contains("UnselectedBodySentinel"));
    assert!(renderer.render_chapter_html(1).unwrap().contains("lang=\"de\""));
    assert_eq!(renderer.book.chapters[0].doc, original);
    assert_eq!(renderer.options.pdf_image_assets, assets);
    assert_eq!(renderer.render_chapter_preview(0).unwrap(), renderer.render_chapter_preview(0).unwrap());
}

#[test]
fn large_book_map_is_complete_but_only_selected_body_is_rendered() {
    let inputs: Vec<_> = (0..129).map(|index| BookInput {
        path: format!("chapter-{index}.md"),
        source: format!("# Chapter {index}\n\nBodySentinel{index:04}\n"),
    }).collect();
    let mut renderer = BookRenderer::new(&inputs).unwrap();
    renderer.options_mut().custom_css = Some(String::new());
    let json = String::from_utf8(renderer.render_chapter_preview(128).unwrap()).unwrap();
    assert!(json.contains("\"selected\":128"));
    assert_eq!(json.matches("\"source\":").count(), 129);
    assert!(json.contains("BodySentinel0128"));
    assert!(!json.contains("BodySentinel0000"));
    assert_eq!(json.matches("\"html\":").count(), 1);
}

#[test]
fn preview_admits_exact_page_and_json_boundaries() {
    let renderer = renderer();
    let html = renderer.render_chapter_html(0).unwrap();
    let expected = renderer.render_chapter_preview(0).unwrap();
    assert_eq!(renderer.chapter_preview_with_limits(0, html.len(), expected.len()).unwrap(), expected);
    assert!(renderer.chapter_preview_with_limits(0, html.len() - 1, expected.len()).is_err());
    assert!(renderer.chapter_preview_with_limits(0, html.len(), expected.len() - 1).is_err());
    assert!(renderer.chapter_preview_with_limits(0, html.len(), 1).is_err());
}

#[test]
fn json_encoding_preflights_control_expansion_and_preserves_unicode() {
    let value = "\0\u{1f}\n\t\r\"\\é😀";
    let expected = json_string(value);
    let mut result = String::new();
    append_json(&mut result, value, expected.len()).unwrap();
    assert_eq!(result, expected);
    let mut untouched = "prefix".to_string();
    assert!(append_json(&mut untouched, value, 6 + expected.len() - 1).is_err());
    assert_eq!(untouched, "prefix");
    assert!(append_json(&mut String::new(), "", 1).is_err());
}

#[test]
fn invalid_index_metadata_and_mutable_assets_are_rejected_without_poisoning() {
    let mut renderer = renderer();
    let before = renderer.render_chapter_preview(0).unwrap();
    for index in [2, usize::MAX] {
        assert!(renderer.render_chapter_preview(index).is_err());
        assert!(renderer.render_chapter_html(index).is_err());
    }
    let title = renderer.book.chapters[1].title.clone();
    renderer.book.chapters[1].title = "x".repeat(METADATA_BYTES + 1);
    assert!(renderer.render_chapter_preview(0).is_err());
    renderer.book.chapters[1].title = title;
    renderer.options.pdf_image_assets.push(PdfImageAsset {
        destination: "invalid".into(), bytes: Vec::new(),
    });
    assert!(renderer.render_chapter_preview(0).is_err());
    renderer.options.pdf_image_assets.pop();
    assert_eq!(renderer.render_chapter_preview(0).unwrap(), before);
}

#[test]
fn workspace_include_edits_reach_selected_preview_without_recreating_options() {
    let mut workspace = BookWorkspace::from_sources(
        &[BookInput { path: "start.md".into(), source: "# Start\n\n{{#include shared.md}}\n".into() }],
        &[BookInput { path: "shared.md".into(), source: "Original snippet".into() }],
    ).unwrap();
    workspace.options_mut().custom_css = Some(".fmd{color:purple}".into());
    assert!(workspace.render_chapter_html(0).unwrap().contains("Original snippet"));
    workspace.update_sources_at_revision(
        &[BookInput { path: "shared.md".into(), source: "Changed snippet".into() }], 0,
    ).unwrap();
    let html = workspace.render_chapter_html(0).unwrap();
    assert!(html.contains("Changed snippet"));
    assert!(!html.contains("Original snippet"));
    assert!(html.contains(".fmd{color:purple}"));
}
