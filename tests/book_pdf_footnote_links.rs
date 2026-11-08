//! Inspect emitted PDF destinations independently of the book navigation map.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{PdfASettings, PdfOptions};

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn book(chapters: &[(&str, &str)]) -> BookRenderer {
    BookRenderer::new(
        &chapters
            .iter()
            .map(|(path, source)| input(path, source))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|part| part == needle)
}

// Follow the writer's classic xref, never split binary font/image streams on
// delimiter-shaped bytes. Each returned index is the object number minus one.
fn objects(bytes: &[u8]) -> Vec<&[u8]> {
    assert!(bytes.starts_with(b"%PDF-"));
    let start = bytes
        .windows(9)
        .rposition(|part| part == b"startxref")
        .unwrap();
    let offset: usize = std::str::from_utf8(&bytes[start + 9..])
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
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
    let end = object
        .windows(7)
        .position(|part| part == b"stream\n")
        .unwrap_or(object.len());
    &object[..end]
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Destination {
    page: usize,
    y: f32,
}

fn links(bytes: &[u8]) -> Vec<Option<Destination>> {
    assert!(!contains(bytes, b"fmd-book-pdf-nav/"));
    let objects = objects(bytes);
    objects
        .iter()
        .map(|object| dictionary(object))
        .filter(|dict| contains(dict, b"/Subtype /Link"))
        .map(|dict| {
            assert!(
                !contains(dict, b"/S /URI"),
                "local links cannot leak as URLs"
            );
            let start = dict.windows(7).position(|part| part == b"/Dest [")? + 7;
            let end = start + dict[start..].iter().position(|&byte| byte == b']').unwrap();
            let values: Vec<_> = std::str::from_utf8(&dict[start..end])
                .unwrap()
                .split_whitespace()
                .collect();
            assert_eq!(values.len(), 7);
            assert_eq!(&values[1..5], &["0", "R", "/XYZ", "null"]);
            assert_eq!(values[6], "null");
            let page: usize = values[0].parse().unwrap();
            let y: f32 = values[5].parse().unwrap();
            assert!(contains(dictionary(objects[page - 1]), b"/Type /Page "));
            assert!(y.is_finite() && y > 0.0);
            Some(Destination { page, y })
        })
        .collect()
}

#[test]
fn chapter_scoped_note_links_bind_to_the_actual_citation_destinations() {
    let renderer = book(&[
        (
            "one.md",
            "# One\n\nFirst source.[^n]\n\n[Local note](#fn-n) [Other note](two.md#fn-n)\n\n[^n]: First citation body.\n",
        ),
        (
            "two.md",
            "# Two\n\nSecond source.[^n]\n\n[Back to first](one.md#fn-n)\n\n[^n]: Second citation body.\n",
        ),
    ]);
    for toc in [false, true] {
        for settings in [PdfASettings::OFF, PdfASettings::a2b_strict()] {
            let options = PdfOptions {
                toc,
                ..Default::default()
            };
            let bytes = renderer
                .render_pdf_with_options(&options, settings)
                .unwrap();
            let annotations = links(&bytes);
            let body_start = if toc {
                // TOC titles, dot leaders and page numbers can each carry
                // their own annotation. Only the five body links are fixed.
                assert!(annotations.len() > 5);
                annotations.len() - 5
            } else {
                assert_eq!(annotations.len(), 5);
                0
            };
            let body = &annotations[body_start..];
            assert!(body.iter().all(Option::is_some), "{body:?}");
            assert_eq!(body[0], body[1], "local note matches its source citation");
            assert_eq!(body[0], body[4], "back-link keeps first chapter scope");
            assert_eq!(body[2], body[3], "forward-link keeps second chapter scope");
            assert_ne!(body[0].unwrap().page, body[3].unwrap().page);
        }
    }
}

#[test]
fn note_numbers_follow_first_use_not_definition_order_or_anchor_spelling() {
    let renderer = book(&[(
        "one.md",
        "# Notes\n\n[Second note](?edition=2#fn-%CE%B2) First.[^a] Second.[^β] [First note](#fn-a) [Second again](#fn-β)\n\n[^unused]: Unreferenced definition first.\n\n[^β]: Second cited definition.\n\n[^a]: First cited definition last.\n",
    )]);
    let annotations = links(&renderer.render_pdf().unwrap());
    assert_eq!(annotations.len(), 5);
    assert!(annotations.iter().all(Option::is_some), "{annotations:?}");
    assert_eq!(annotations[0], annotations[2]);
    assert_eq!(annotations[0], annotations[4]);
    assert_eq!(annotations[1], annotations[3]);
    assert_ne!(annotations[0], annotations[1]);
}

#[test]
fn note_only_references_and_cycles_keep_their_emitted_endnote_destinations() {
    let renderer = book(&[(
        "one.md",
        "# Notes\n\nSource.[^a] [First](#fn-a) [Nested](#fn-b)\n\n[^a]: First note cites another.[^b]\n\n[^b]: Second note returns to the first.[^a]\n",
    )]);
    let annotations = links(&renderer.render_pdf().unwrap());
    assert_eq!(annotations.len(), 5);
    assert!(annotations.iter().all(Option::is_some), "{annotations:?}");
    // Body references come first. The note-only citation falls back to Notes
    // in body flow, and the first note is painted in the page-bottom region.
    assert_eq!(annotations[0], annotations[1]);
    assert_eq!(annotations[0], annotations[3]);
    assert_eq!(annotations[2], annotations[4]);
    assert_ne!(annotations[0], annotations[2]);
}

#[test]
fn oversized_note_links_keep_the_actual_multipage_fallback_destination() {
    let mut source = String::from(
        "# Long citation\n\nSource.[^large] [Full citation](#fn-large)\n\n[^large]: A long citation.\n\n",
    );
    for index in 0..90 {
        source.push_str(&format!(
            "    Paragraph {index} preserves its original citation text and extends the note.\n\n"
        ));
    }
    let renderer = book(&[("one.md", &source)]);
    let bytes = renderer.render_pdf().unwrap();
    let annotations = links(&bytes);
    assert_eq!(annotations.len(), 2);
    assert!(annotations[0].is_some());
    assert_eq!(annotations[0], annotations[1]);
    assert!(
        objects(&bytes)
            .iter()
            .filter(|object| contains(dictionary(object), b"/Type /Page "))
            .count()
            > 1,
        "the oversized note must exercise a real multipage endnote fallback"
    );
}

#[test]
fn missing_unreferenced_and_ambiguous_note_ids_do_not_capture_other_targets() {
    let renderer = book(&[(
        "one.md",
        "# fn-n\n\nCited.[^n] [Ambiguous](#fn-n) [Unreferenced](#fn-unused) [Missing](#fn-absent)\n\n## fn-heading\n\n[Ordinary heading](#fn-heading)\n\n[^n]: Referenced note.\n\n[^unused]: Unreferenced note.\n",
    )]);
    let bytes = renderer.render_pdf().unwrap();
    let annotations = links(&bytes);
    assert_eq!(annotations.len(), 5);
    assert!(
        annotations[0].is_some(),
        "the ordinary citation still works"
    );
    assert_eq!(&annotations[1..4], &[None, None, None]);
    assert!(
        annotations[4].is_some(),
        "prefixes do not reserve heading IDs"
    );
    assert_ne!(annotations[0], annotations[4]);
}

#[test]
fn included_definitions_keep_their_containing_chapter_scope() {
    let renderer = BookRenderer::from_sources(
        &[
            input(
                "guide/one.md",
                "# First\n\n{{#include ../parts/first.md}}\n\n[Other note](../two.md#fn-n)\n",
            ),
            input(
                "two.md",
                "# Second\n\nSource.[^n] [First note](guide/one.md#fn-n)\n\n[^n]: Second note.\n",
            ),
        ],
        &[input(
            "parts/first.md",
            "Source.[^n] [Local note](#fn-n)\n\n[^n]: Included first note.\n",
        )],
    )
    .unwrap();
    let annotations = links(&renderer.render_pdf().unwrap());
    assert_eq!(annotations.len(), 5);
    assert!(annotations.iter().all(Option::is_some), "{annotations:?}");
    assert_eq!(annotations[0], annotations[1]);
    assert_eq!(annotations[0], annotations[4]);
    assert_eq!(annotations[2], annotations[3]);
    assert_ne!(annotations[0], annotations[3]);
}

#[test]
fn missing_unreferenced_and_ambiguous_citation_ids_do_not_capture_heading_targets() {
    let renderer = book(&[(
        "one.md",
        "# fnref-1\n\nCited.[^n] [Ambiguous](#fnref-1) [Unreferenced](#fnref-2) [Missing](#fnref-99)\n\n## fnref-heading\n\n[Ordinary heading](#fnref-heading)\n\n[^n]: Referenced note.\n\n[^unused]: Unreferenced note.\n",
    )]);
    let annotations = links(&renderer.render_pdf().unwrap());
    assert_eq!(annotations.len(), 5);
    assert!(annotations[0].is_some(), "the note citation remains linked");
    assert_eq!(&annotations[1..4], &[None, None, None]);
    assert!(
        annotations[4].is_some(),
        "a prefix alone does not reserve a heading ID"
    );
    assert_ne!(annotations[0], annotations[4]);
}
