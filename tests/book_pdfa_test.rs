//! Exercise the real book assembler, PDF/A writer and post-layout link binder.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer, book_pdf_document_with_assets, render_book_pdf};
use franken_markdown::{PageSize, PdfAMode, PdfASettings, PdfImageAsset, PdfOptions};

fn input(path: &str, source: &str) -> BookInput {
    BookInput { path: path.into(), source: source.into() }
}

fn book() -> BookRenderer {
    BookRenderer::new(&[
        input("guide/one.md", "# Same\n\n[Next](two.md#same) [Self](#same) [Missing](two.md#absent)"),
        input("guide/two.md", "# Same\n\n[Back](one.md#same)"),
    ]).unwrap()
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

// Independent inspection of the writer's classic xref. Never split binary
// font/image streams on 'obj' or 'endobj' substrings to infer object boundaries.
fn objects(bytes: &[u8]) -> Vec<&[u8]> {
    assert!(bytes.starts_with(b"%PDF-"));
    let start = bytes.windows(9).rposition(|part| part == b"startxref").unwrap();
    let offset: usize = std::str::from_utf8(&bytes[start + 9..]).unwrap()
        .split_whitespace().next().unwrap().parse().unwrap();
    let mut lines = std::str::from_utf8(&bytes[offset..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let header: Vec<_> = lines.next().unwrap().split_whitespace().collect();
    assert_eq!(header[0], "0");
    let count: usize = header[1].parse().unwrap();
    assert_eq!(lines.next().unwrap().trim(), "0000000000 65535 f");
    let mut positions = Vec::new();
    for id in 1..count {
        let row: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(&row[1..], &["00000", "n"]);
        let position: usize = row[0].parse().unwrap();
        assert!(position < offset);
        assert!(bytes[position..].starts_with(format!("{id} 0 obj").as_bytes()));
        positions.push((position, id));
    }
    assert_eq!(lines.next(), Some("trailer"));
    positions.sort_unstable();
    assert!(positions.windows(2).all(|pair| pair[0].0 < pair[1].0));
    let mut result = vec![&bytes[0..0]; count - 1];
    for (index, &(start, id)) in positions.iter().enumerate() {
        let end = positions.get(index + 1).map_or(offset, |&(next, _)| next);
        result[id - 1] = &bytes[start..end];
    }
    result
}

fn dictionary(object: &[u8]) -> &[u8] {
    let end = object.windows(7).position(|part| part == b"stream\n").unwrap_or(object.len());
    &object[..end]
}

fn destination(object: &[u8]) -> Option<&[u8]> {
    let dict = dictionary(object);
    let start = dict.windows(7).position(|part| part == b"/Dest [")? + 6;
    let end = start + dict[start..].iter().position(|&byte| byte == b']')? + 1;
    Some(&dict[start..end])
}

fn assert_archival_objects(bytes: &[u8], options: &PdfOptions) {
    let parsed = objects(bytes);
    let catalog = parsed.iter().find(|object| contains(dictionary(object), b"/Type /Catalog")).unwrap();
    assert!(contains(dictionary(catalog), b"/Metadata "));
    assert!(contains(dictionary(catalog), b"/OutputIntents ["));
    assert_eq!(parsed.iter().filter(|object| contains(dictionary(object), b"/Type /Metadata")).count(), 1);
    assert_eq!(parsed.iter().filter(|object| contains(dictionary(object), b"/Type /OutputIntent")).count(), 1);
    let xmp = franken_markdown::pdfa::xmp_packet(
        options.title.as_deref().unwrap_or(""),
        options.author.as_deref().unwrap_or(""),
        options.metadata_epoch_seconds,
    );
    assert!(contains(bytes, &xmp));
    assert!(contains(bytes, b"/FontFile2 "));
    assert!(contains(bytes, b"/ToUnicode "));
    assert!(contains(bytes, b"acsp"));
    for annotation in parsed.iter().filter(|object| contains(dictionary(object), b"/Subtype /Link")) {
        assert!(contains(dictionary(annotation), b"/F 4"));
    }
    assert!(!contains(bytes, b"fmd-book-pdf-nav/"));
}

#[test]
fn off_profile_preserves_ordinary_book_bytes_with_and_without_navigation() {
    for renderer in [book(), BookRenderer::new(&[input("plain.md", "# Plain\n\nText.")]).unwrap()] {
        for toc in [false, true] {
            let options = PdfOptions { toc, page_numbers: true, ..Default::default() };
            let expected = render_book_pdf(renderer.book(), &options).unwrap();
            assert_eq!(renderer.render_pdf_with_options(&options, PdfASettings::OFF).unwrap(), expected);
        }
        assert_eq!(renderer.render_pdf_pdfa(PdfASettings::OFF).unwrap(), renderer.render_pdf().unwrap());
    }
}

#[test]
fn archival_book_binds_duplicate_heading_links_to_actual_chapter_destinations() {
    let renderer = book();
    let options = PdfOptions { title: Some("Archive".into()), ..Default::default() };
    let bytes = renderer.render_pdf_with_options(&options, PdfASettings::a2b_strict()).unwrap();
    assert_archival_objects(&bytes, &options);
    let parsed = objects(&bytes);
    let outlines: Vec<_> = parsed.iter()
        .filter(|object| contains(dictionary(object), b"/Title "))
        .filter_map(|object| destination(object)).collect();
    let annotations: Vec<_> = parsed.iter()
        .filter(|object| contains(dictionary(object), b"/Subtype /Link"))
        .filter_map(|object| destination(object)).collect();
    assert_eq!(outlines.len(), 2);
    assert_ne!(outlines[0], outlines[1]);
    assert_eq!(annotations, [outlines[1], outlines[0], outlines[0]]);
}

#[test]
fn archival_mode_preserves_existing_page_font_streams_and_toc_destinations() {
    let renderer = BookRenderer::new(&[
        input("one.md", "# One\n\n[Landing](two.md) [Note](#detail) cite[^n]\n\n[^n]: ## Detail\n\n    Body."),
        input("two.md", "Leading text.\n\n## Details\n\n[Back](one.md#one)"),
    ]).unwrap();
    let options = PdfOptions { toc: true, page_numbers: true, ..Default::default() };
    let normal = renderer.render_pdf_with_options(&options, PdfASettings::OFF).unwrap();
    let archive = renderer.render_pdf_with_options(&options, PdfASettings::a2b()).unwrap();
    let normal_objects = objects(&normal);
    let archive_objects = objects(&archive);
    assert_eq!(archive_objects.len(), normal_objects.len() + 3);
    let mut streams = 0;
    for (normal, archival) in normal_objects.iter().zip(&archive_objects) {
        if contains(normal, b"stream\n") {
            assert_eq!(normal, archival, "existing stream object changed");
            streams += 1;
        }
        assert_eq!(destination(normal), destination(archival));
    }
    assert!(streams >= 3, "page and embedded-font streams must be exercised");
    assert!(!contains(&archive, b"fmd-book-pdf-nav/"));
}

#[test]
fn source_includes_and_link_free_books_match_the_document_archival_writer() {
    let renderer = BookRenderer::from_sources(
        &[input("guide/start.md", "# Intro\n\n{{#include ../parts/body.md}}"), input("end.md", "# End")],
        &[input("parts/body.md", "Included **content**.")],
    ).unwrap();
    let options = PdfOptions { title: Some("Included".into()), ..Default::default() };
    let document = book_pdf_document_with_assets(renderer.book(), &[]).unwrap();
    let expected = franken_markdown::render_pdf_document_pdfa(&document, &options, PdfASettings::a2b()).unwrap();
    assert_eq!(renderer.render_pdf_with_options(&options, PdfASettings::a2b()).unwrap(), expected);
}

#[test]
fn borrowed_and_retained_options_preserve_metadata_geometry_and_source_state() {
    let mut renderer = book();
    renderer.options_mut().title = Some("Retained title".into());
    let stored = format!("{:?}", renderer.options());
    let documents: Vec<_> = renderer.book().chapters.iter().map(|chapter| chapter.doc.clone()).collect();
    let mut options = PdfOptions {
        title: Some("Archive & proof".into()), author: Some("A < B".into()),
        metadata_epoch_seconds: Some(1_700_000_000), ..Default::default()
    };
    options.theme.page.size = PageSize { name: "custom", width_pt: 360.0, height_pt: 504.0 };
    let bytes = renderer.render_pdf_with_options(&options, PdfASettings::a2b()).unwrap();
    assert_archival_objects(&bytes, &options);
    let pages: Vec<_> = objects(&bytes).into_iter()
        .filter(|object| contains(dictionary(object), b"/MediaBox ")).collect();
    assert!(!pages.is_empty());
    for page in pages {
        let dict = std::str::from_utf8(dictionary(page)).unwrap();
        let geometry = dict.split("/MediaBox").nth(1).unwrap().trim_start()
            .strip_prefix('[').unwrap().split(']').next().unwrap();
        let values: Vec<f32> = geometry.split_whitespace().map(|v| v.parse().unwrap()).collect();
        assert_eq!(values, [0.0, 0.0, 360.0, 504.0]);
    }
    assert_eq!(bytes, renderer.render_pdf_with_options(&options, PdfASettings::a2b()).unwrap());
    assert_eq!(format!("{:?}", renderer.options()), stored);
    assert_eq!(renderer.book().chapters.iter().map(|chapter| chapter.doc.clone()).collect::<Vec<_>>(), documents);
    let retained = renderer.render_pdf_pdfa(PdfASettings::a2b()).unwrap();
    assert_archival_objects(&retained, &renderer.options().pdf_options());
    assert_ne!(retained, bytes);
}

#[test]
fn strict_checks_run_before_unreferenced_notes_are_discarded() {
    for link in ["", "\n\n[Self](#one)"] {
        let source = format!("# One{link}\n\n[^unused]: [Forbidden](file:///private.txt)");
        let renderer = BookRenderer::new(&[input("one.md", &source)]).unwrap();
        let error = renderer.render_pdf_pdfa(PdfASettings::a2b_strict()).unwrap_err();
        assert!(error.to_string().contains("pdf_a_file_uri"));
        assert!(renderer.render_pdf_pdfa(PdfASettings::a2b()).is_ok());
        assert!(renderer.render_pdf().is_ok());
    }
}

#[test]
fn strict_rejection_preserves_the_book_and_ordinary_calls_still_work() {
    for uri in ["file:///private.txt", "FiLe:///private.txt", "javascript:alert"] {
        let renderer = BookRenderer::new(&[
            input("one.md", &format!("# One\n\n[Next](two.md#two)\n\n> [Forbidden]({uri})")),
            input("two.md", "# Two"),
        ]).unwrap();
        let before = renderer.render_pdf().unwrap();
        assert!(renderer.render_pdf_pdfa(PdfASettings::a2b_strict()).unwrap_err().to_string().contains("pdf_a_"));
        assert_eq!(renderer.render_pdf().unwrap(), before);
        let archived = renderer.render_pdf_pdfa(PdfASettings::a2b()).unwrap();
        assert!(!contains(&archived, format!("/URI ({uri})").as_bytes()));
        assert!(!contains(&archived, b"fmd-book-pdf-nav/"));
    }
}

#[test]
fn strict_without_a_profile_is_not_silently_downgraded() {
    let error = book().render_pdf_pdfa(PdfASettings { mode: PdfAMode::Off, strict: true }).unwrap_err();
    assert!(error.to_string().contains("pdf_a_strict_without_profile"));
}

#[test]
fn chapter_relative_image_payloads_reach_the_archival_writer() {
    // A real, deterministic 1x1 RGB PNG. Its CRCs and zlib data are independently
    // verified by the authoring check; this is input data, not a decoder double.
    let png = vec![
        137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1,0,0,0,1,8,2,0,0,0,
        144,119,83,222,0,0,0,12,73,68,65,84,120,156,99,248,207,192,0,0,3,1,1,0,
        201,254,146,239,0,0,0,0,73,69,78,68,174,66,96,130,
    ];
    let renderer = BookRenderer::new(&[
        input("guide/start.md", "# Start\n\n![Pixel](../assets/pixel.png)\n\n[Next](../end.md#end)"),
        input("end.md", "# End"),
    ]).unwrap();
    let options = PdfOptions {
        image_assets: vec![PdfImageAsset::new("assets/pixel.png", png)],
        ..Default::default()
    };
    let bytes = renderer.render_pdf_with_options(&options, PdfASettings::a2b_strict()).unwrap();
    assert_eq!(objects(&bytes).iter().filter(|object| contains(dictionary(object), b"/Subtype /Image")).count(), 1);
    assert!(!contains(&bytes, b"fmd-book-pdf-nav/"));
    assert_eq!(options.image_assets[0].destination, "assets/pixel.png");
}
