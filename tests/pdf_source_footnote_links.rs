//! Public HTML note fragments keep their source meaning in standalone PDFs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, Document, Inline, PdfASettings, PdfEmitOptions, PdfImageAsset, PdfOptions,
    parse_markdown, render_pdf_document, render_pdf_document_emitted, render_pdf_document_pdfa,
    render_pdf_document_profiled, render_pdf_document_reported,
};

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|part| part == needle)
}

// Follow the classic xref, so binary font/image streams cannot impersonate PDF
// dictionaries. This checks the serialized result, not the navigation planner.
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
                "local links cannot become URL actions"
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
fn ordinary_note_navigation_does_not_become_a_citation_or_move_its_note() {
    let mut doc = parse_markdown("[Read note](#fn-n) [Return](#fnref-1)");
    doc.blocks.push(Block::PageBreak);
    doc.blocks
        .extend(parse_markdown("Actual claim[^n].\n\n[^n]: Supporting evidence.").blocks);
    let options = PdfOptions {
        footnote_backlinks: true,
        ..Default::default()
    };
    let bytes = render_pdf_document(&doc, &options).unwrap();
    let annotations = links(&bytes);
    assert_eq!(annotations.len(), 4);
    assert!(annotations.iter().all(Option::is_some), "{annotations:?}");
    assert_eq!(
        annotations[0], annotations[2],
        "public note link matches the citation"
    );
    assert_eq!(
        annotations[1], annotations[3],
        "public return link matches the backlink"
    );
    let note = annotations[0].unwrap();
    let citation = annotations[1].unwrap();
    let pages: Vec<_> = objects(&bytes)
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            contains(dictionary(object), b"/Type /Page ").then_some(index + 1)
        })
        .collect();
    assert_eq!(pages.len(), 2);
    assert_eq!(note.page, pages[1], "the note must stay on the second page");
    assert_eq!(
        citation.page, note.page,
        "return target is the actual citation"
    );
    assert!(note.y < citation.y, "the note is at the page foot");
}

#[test]
fn decoded_note_fragments_follow_first_use_instead_of_definition_order() {
    let doc = parse_markdown(
        "[Second](?edition=2#fn-%CE%B2) First.[^a] Second.[^β] [First](#fn-a) [Second again](#fn-β)\n\n\
         [^unused]: Unreferenced definition first.\n\n[^β]: Second cited definition.\n\n[^a]: First cited definition last.\n",
    );
    let annotations = links(&render_pdf_document(&doc, &PdfOptions::default()).unwrap());
    assert_eq!(annotations.len(), 5);
    assert!(annotations.iter().all(Option::is_some), "{annotations:?}");
    assert_eq!(annotations[0], annotations[2]);
    assert_eq!(annotations[0], annotations[4]);
    assert_eq!(annotations[1], annotations[3]);
    assert_ne!(annotations[0], annotations[1]);
}

#[test]
fn ambiguous_ids_and_unreferenced_notes_cannot_capture_public_links() {
    let doc = parse_markdown(
        "# fn-n\n\n## fnref-1\n\nCited.[^n] [Ambiguous note](#fn-n) [Ambiguous return](#fnref-1) \
         [Unreferenced](#fn-unused) [Missing](#fn-missing)\n\n## fn-heading\n\n[Heading](#fn-heading)\n\n\
         [^n]: Referenced note.\n\n[^unused]: Unreferenced note.\n",
    );
    let annotations = links(&render_pdf_document(&doc, &PdfOptions::default()).unwrap());
    assert_eq!(annotations.len(), 4);
    assert!(
        annotations[0].is_some(),
        "the actual citation still resolves"
    );
    assert_eq!(&annotations[1..3], &[None, None]);
    assert!(
        annotations[3].is_some(),
        "ordinary namespace-shaped headings still work"
    );
    assert_ne!(annotations[0], annotations[3]);
}

#[test]
fn note_body_cycles_and_html_links_share_emitted_destinations() {
    let doc = parse_markdown(
        "Source.[^a] <a href='#fn-a'>First</a> [Nested](#fn-b)\n\n\
         [^a]: First note cites another.[^b]\n\n[^b]: Second note returns to the first.[^a]\n",
    );
    let annotations = links(&render_pdf_document(&doc, &PdfOptions::default()).unwrap());
    assert_eq!(annotations.len(), 5);
    assert!(annotations.iter().all(Option::is_some), "{annotations:?}");
    assert_eq!(annotations[0], annotations[1]);
    assert_eq!(annotations[0], annotations[3]);
    assert_eq!(annotations[2], annotations[4]);
    assert_ne!(annotations[0], annotations[2]);
}

#[test]
fn links_across_semantic_containers_keep_the_same_note_destination() {
    let doc = parse_markdown(
        "> [Quote](#fn-n)\n\n- [List](#fn-n)\n\n| Note |\n| --- |\n| [Cell](#fn-n) |\n\n\
         Term\n: [Definition](#fn-n)\n\nClaim.[^n]\n\n[^n]: Supporting evidence.\n",
    );
    let annotations = links(&render_pdf_document(&doc, &PdfOptions::default()).unwrap());
    assert_eq!(annotations.len(), 5);
    let destination = annotations[0].expect("the quoted link resolves");
    assert!(annotations.iter().all(|entry| *entry == Some(destination)));
}

#[test]
fn an_unreferenced_definition_does_not_become_a_citation_through_an_ordinary_link() {
    let doc = parse_markdown(
        "[Unused](#fn-unused) [Return](#fnref-1)\n\n[^unused]: Retained as an endnote.\n",
    );
    assert!(links(&render_pdf_document(&doc, &PdfOptions::default()).unwrap()).is_empty());
}

#[test]
fn authored_uri_cannot_be_captured_by_the_temporary_navigation_namespace() {
    let authored = format!("fmd-book-pdf-nav/0/{:020}/{}", 0, "0".repeat(128));
    let check = |doc: &Document| {
        let bytes = render_pdf_document(doc, &PdfOptions::default()).unwrap();
        let objects = objects(&bytes);
        let annotations: Vec<_> = objects
            .iter()
            .map(|object| dictionary(object))
            .filter(|dict| contains(dict, b"/Subtype /Link"))
            .collect();
        assert_eq!(annotations.len(), 3);
        assert_eq!(
            annotations
                .iter()
                .filter(|dict| contains(dict, b"/Dest ["))
                .count(),
            2
        );
        assert!(contains(
            annotations[2],
            format!("/URI ({authored})").as_bytes()
        ));
        assert!(!contains(&bytes, b"fmd-book-pdf-nav/1/"));
    };
    for padding in ["", " ", "\t", "\0", "\u{0085}"] {
        let mut doc = parse_markdown("Claim.[^n] [Note](#fn-n)\n\n[^n]: Evidence.\n");
        doc.blocks.insert(
            1,
            Block::Paragraph(vec![Inline::Link {
                dest: format!("{padding}{authored}{padding}"),
                title: None,
                content: vec![Inline::Text("Keep URL".into())],
            }]),
        );
        check(&doc);
    }
    for padding in [" ", "&#32;"] {
        let doc = parse_markdown(&format!(
            "Claim.[^n] [Note](#fn-n) <a href='{padding}{authored}{padding}'>Keep URL</a>\n\n[^n]: Evidence.\n"
        ));
        check(&doc);
    }
}

#[test]
fn encoded_svg_uris_cannot_impersonate_source_navigation_tokens() {
    use franken_markdown::book::{BookInput, BookRenderer};

    let authored = format!("fmd-book-pdf-nav/0/{:020}/{}", 0, "0".repeat(128));
    let escaped = authored.replacen('-', "&#45;", 1);
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='10'>\
         <a href='{escaped}'><rect width='20' height='10'/></a></svg>"
    );
    let base64 = format!(
        "data:image/svg+xml;base64,{}",
        franken_markdown::html::base64_encode(svg.as_bytes())
    );
    let percent = format!(
        "data:image/svg+xml,{}",
        svg.bytes()
            .map(|byte| format!("%{byte:02X}"))
            .collect::<String>()
    );
    let nested = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='10'>\
         <a href='{escaped}'><image width='20' height='10' href='{base64}'/></a></svg>"
    );
    for (destination, assets) in [
        (
            "asset.svg".to_string(),
            vec![PdfImageAsset::new("asset.svg", svg.into_bytes())],
        ),
        (base64, vec![]),
        (percent, vec![]),
        (
            "asset.svg".to_string(),
            vec![PdfImageAsset::new("asset.svg", nested.into_bytes())],
        ),
    ] {
        for inline in [false, true] {
            let image = if inline {
                format!("Inline ![Figure]({destination}) in prose.")
            } else {
                format!("![Figure]({destination})")
            };
            let source = format!("Claim.[^n] [Note](#fn-n)\n\n{image}\n\n[^n]: Evidence.\n");
            let doc = parse_markdown(&source);
            let options = PdfOptions {
                image_assets: assets.clone(),
                ..Default::default()
            };
            let book = BookRenderer::new(&[BookInput {
                path: "notes.md".into(),
                source,
            }])
            .unwrap();
            for bytes in [
                render_pdf_document(&doc, &options).unwrap(),
                book.render_pdf_with_options(&options, PdfASettings::OFF)
                    .unwrap(),
            ] {
                let objects = objects(&bytes);
                let annotations: Vec<_> = objects
                    .iter()
                    .map(|object| dictionary(object))
                    .filter(|dict| contains(dict, b"/Subtype /Link"))
                    .collect();
                assert_eq!(
                    annotations.len(),
                    3,
                    "the figure retains its own active link"
                );
                let uri = format!("/URI ({authored})");
                let asset = annotations
                    .iter()
                    .find(|dict| contains(dict, uri.as_bytes()))
                    .expect("encoded SVG anchor retains the authored URI");
                assert!(!contains(asset, b"/StructParent"));
                assert!(!contains(asset, b"/Dest ["));
                assert_eq!(
                    annotations
                        .iter()
                        .filter(|dict| {
                            contains(dict, b"/StructParent") && contains(dict, b"/Dest [")
                        })
                        .count(),
                    2,
                    "both source note links still bind"
                );
                assert_eq!(
                    annotations
                        .iter()
                        .filter(|dict| contains(dict, b"/S /URI"))
                        .count(),
                    1
                );
            }
        }
    }
}

#[test]
fn source_linked_block_and_inline_images_still_bind_to_notes() {
    let svg = b"<svg xmlns='http://www.w3.org/2000/svg' width='20' height='10'><rect width='20' height='10'/></svg>";
    let options = PdfOptions {
        image_assets: vec![PdfImageAsset::new("image.svg", svg.to_vec())],
        ..Default::default()
    };
    for link in [
        "[![Read note](image.svg)](#fn-n)",
        "Before [![Read note](image.svg)](#fn-n) after.",
    ] {
        let doc = parse_markdown(&format!("{link}\n\nClaim.[^n]\n\n[^n]: Evidence.\n"));
        let annotations = links(&render_pdf_document(&doc, &options).unwrap());
        assert_eq!(annotations.len(), 2);
        assert!(annotations[0].is_some());
        assert_eq!(annotations[0], annotations[1]);
    }
}

#[test]
fn oversized_note_navigation_uses_the_real_endnote_page() {
    let mut source = String::from("Claim.[^long] [Full note](#fn-long)\n\n[^long]: Evidence.\n\n");
    for index in 0..80 {
        source.push_str(&format!(
            "    Paragraph {index} preserves the full supporting evidence.\n\n"
        ));
    }
    let doc = parse_markdown(&source);
    let bytes = render_pdf_document(&doc, &PdfOptions::default()).unwrap();
    let annotations = links(&bytes);
    assert_eq!(annotations.len(), 2);
    assert!(annotations[0].is_some());
    assert_eq!(annotations[0], annotations[1]);
    assert!(
        objects(&bytes)
            .iter()
            .filter(|object| contains(dictionary(object), b"/Type /Page "))
            .count()
            > 1
    );
}

#[test]
fn every_public_pdf_entrypoint_binds_the_same_note_fragments() {
    let doc: Document =
        parse_markdown("Claim.[^n] [Read](#fn-n) [Return](#fnref-1)\n\n[^n]: Evidence.");
    let options = PdfOptions::default();
    let before = doc.clone();
    let expected = render_pdf_document(&doc, &options).unwrap();
    assert_eq!(links(&expected).len(), 3);
    for emission in [
        franken_markdown::PdfPageEmission::Chunked,
        franken_markdown::PdfPageEmission::Monolithic,
    ] {
        let emit = PdfEmitOptions {
            emission,
            ..Default::default()
        };
        assert_eq!(
            render_pdf_document_emitted(&doc, &options, emit).unwrap(),
            expected
        );
        assert_eq!(
            render_pdf_document_reported(&doc, &options, emit)
                .unwrap()
                .bytes,
            expected
        );
    }
    assert_eq!(
        render_pdf_document_profiled(&doc, &options).unwrap().bytes,
        expected
    );
    assert_eq!(
        render_pdf_document_pdfa(&doc, &options, PdfASettings::OFF).unwrap(),
        expected
    );
    let archived = render_pdf_document_pdfa(&doc, &options, PdfASettings::a2b_strict()).unwrap();
    assert_eq!(links(&archived), links(&expected));
    assert_eq!(doc, before, "rendering cannot mutate the source AST");
}
