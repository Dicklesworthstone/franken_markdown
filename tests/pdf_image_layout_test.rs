//! Standalone Markdown figures retain wrappers, links and list ownership.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, Document, Inline, PageMargins, PageSize, PdfEmitOptions, PdfImageAsset, PdfOptions,
    parse_markdown, render_pdf, render_pdf_document, render_pdf_document_emitted,
    render_pdf_document_profiled, render_warnings,
};

const SVG: &[u8] = include_bytes!("fixtures/pdf/linked-figure.svg");

fn options() -> PdfOptions {
    PdfOptions {
        image_assets: vec![PdfImageAsset::new("figure.svg", SVG.to_vec())],
        ..PdfOptions::default()
    }
}

fn pdf_text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// Read the page's actual /Contents objects, using each stream's declared
/// length and filter. Small figure-only streams can be shorter uncompressed;
/// a helper that only inflates zlib payloads would silently skip their drawing.
fn page_content(pdf: &[u8]) -> String {
    let mut content = String::new();
    for reference in pdf_text(pdf).split("/Contents ").skip(1) {
        let object: usize = reference
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let marker = format!("\n{object} 0 obj\n");
        let start = pdf
            .windows(marker.len())
            .position(|bytes| bytes == marker.as_bytes())
            .unwrap()
            + marker.len();
        let body = &pdf[start..];
        let stream = body
            .windows(b"\nstream\n".len())
            .position(|bytes| bytes == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&body[..stream]).unwrap();
        let length: usize = dictionary
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let payload_start = stream + b"\nstream\n".len();
        let payload = &body[payload_start..payload_start + length];
        let decoded;
        let raw = if dictionary.contains("/Filter /FlateDecode") {
            decoded =
                franken_markdown::zlib_decompress(payload, 1 << 24).expect("valid page DEFLATE");
            decoded.as_slice()
        } else {
            assert!(!dictionary.contains("/Filter"), "unsupported page filter");
            payload
        };
        content.push_str(std::str::from_utf8(raw).expect("page drawing operators"));
        content.push('\n');
    }
    assert!(!content.is_empty(), "at least one page content stream");
    content
}

fn array_after(text: &str, key: &str) -> [f32; 4] {
    let start = text.split_once(key).expect("array key").1;
    let values: Vec<f32> = start
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|number| number.parse().unwrap())
        .collect();
    values.try_into().expect("four rectangle coordinates")
}

fn object_containing<'a>(text: &'a str, needle: &str) -> (usize, &'a str) {
    let offset = text.find(needle).expect("object content");
    let start = text[..offset].rfind(" 0 obj\n").expect("object header");
    let number_start = text[..start].rfind('\n').map_or(0, |pos| pos + 1);
    let number = text[number_start..start].parse().unwrap();
    let body = text[start + " 0 obj\n".len()..]
        .split_once("\nendobj")
        .unwrap()
        .0;
    (number, body)
}

#[test]
fn linked_svg_draws_a_figure_with_an_image_sized_click_target_and_tagged_owner() {
    let opts = options();
    let source = "[![Architecture](figure.svg)](https://example.com/details)";
    let document = parse_markdown(source);
    let pdf = render_pdf_document(&document, &opts).unwrap();
    let text = pdf_text(&pdf);
    let content = page_content(&pdf);
    assert!(content.contains("/Figure <</MCID"));
    assert!(
        content.contains("0.035 0.412 0.855 rg"),
        "actual SVG paint: {content}"
    );
    assert!(text.contains("/Alt (Architecture)"));
    assert!(text.contains("/URI (https://example.com/details)"));
    let rectangle = array_after(&text, "/Rect [");
    assert_eq!(rectangle, array_after(&text, "/BBox ["));
    assert_eq!(rectangle[2] - rectangle[0], 120.0);
    assert_eq!(rectangle[3] - rectangle[1], 90.0);

    let (link_id, link) = object_containing(&text, "/S /Link ");
    let (_, figure) = object_containing(&text, "/S /Figure ");
    let (_, annotation) = object_containing(&text, "/Subtype /Link ");
    assert!(
        figure.contains(&format!("/P {link_id} 0 R")),
        "Figure belongs to Link"
    );
    assert!(
        link.contains("/Type /OBJR"),
        "annotation belongs to Link: {link}"
    );
    assert!(link.contains("/Pg "), "Link has its page association");
    assert!(annotation.contains("/StructParent "));
    assert!(render_warnings(&document, &opts).is_empty());
    assert_eq!(pdf, render_pdf(source, &opts).unwrap());
    assert_eq!(
        pdf,
        render_pdf_document_profiled(&document, &opts)
            .unwrap()
            .bytes
    );
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&document, &opts, PdfEmitOptions::monolithic()).unwrap()
    );
}

#[test]
fn formatting_wrappers_preserve_the_same_standalone_figure() {
    let opts = options();
    let bare = render_pdf("![Architecture](figure.svg)", &opts).unwrap();
    for source in [
        "*![Architecture](figure.svg)*",
        "**![Architecture](figure.svg)**",
        "~~![Architecture](figure.svg)~~",
        "***![Architecture](figure.svg)***",
    ] {
        assert_eq!(render_pdf(source, &opts).unwrap(), bare, "{source}");
    }
    let linked = render_pdf(
        "[**![Architecture](figure.svg)**](https://example.com)",
        &opts,
    )
    .unwrap();
    let text = pdf_text(&linked);
    assert!(text.contains("/S /Figure ") && text.contains("/URI (https://example.com)"));
}

#[test]
fn linked_figures_resolve_internal_destinations_and_drop_missing_annotations() {
    let opts = options();
    for (target, resolved) in [("destination", true), ("missing", false)] {
        let source = format!("[![Architecture](figure.svg)](#{target})\n\n# Destination");
        let pdf = render_pdf(&source, &opts).unwrap();
        let text = pdf_text(&pdf);
        assert!(text.contains("/S /Figure "));
        assert_eq!(text.contains("/Subtype /Link "), resolved);
        if resolved {
            let (_, annotation) = object_containing(&text, "/Subtype /Link ");
            assert!(annotation.contains("/Dest [") && !annotation.contains("/URI "));
        }
    }
}

#[test]
fn enclosing_markdown_link_owns_the_whole_svg_click_target() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="120"><a href="https://inner.example"><rect width="160" height="120" fill="blue"/><text x="10" y="30">Linked text</text></a></svg>"#;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("figure.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let bare = pdf_text(&render_pdf("![Architecture](figure.svg)", &opts).unwrap());
    assert!(bare.contains("/URI (https://inner.example)"));

    let linked = pdf_text(
        &render_pdf(
            "[![Architecture](figure.svg)](https://outer.example)",
            &opts,
        )
        .unwrap(),
    );
    assert_eq!(linked.matches("/Subtype /Link ").count(), 1);
    assert!(linked.contains("/URI (https://outer.example)"));
    assert!(!linked.contains("https://inner.example"));
    assert!(linked.contains("/StructParent "));

    let unresolved =
        pdf_text(&render_pdf("[![Architecture](figure.svg)](#missing)", &opts).unwrap());
    assert!(
        !unresolved.contains("/Subtype /Link "),
        "an unresolved outer target must not expose an unrelated inner target"
    );
}

#[test]
fn unsafe_enclosing_links_never_remove_or_activate_the_image() {
    let opts = options();
    let image = Inline::Image {
        alt: "Architecture".into(),
        dest: "figure.svg".into(),
        title: None,
    };
    for dest in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "data:text/html,hello",
        "file:///etc/passwd",
        "java\nscript:alert(1)",
    ] {
        let document = Document {
            blocks: vec![Block::Paragraph(vec![Inline::Link {
                content: vec![Inline::Strong(vec![image.clone()])],
                dest: dest.into(),
                title: None,
            }])],
        };
        let text = pdf_text(&render_pdf_document(&document, &opts).unwrap());
        assert!(text.contains("/S /Figure "), "{dest}");
        assert!(
            !text.contains("/Subtype /Link "),
            "unsafe annotation: {dest}"
        );
        assert!(!text.contains("/S /Link "), "unsafe link structure: {dest}");
    }
}

#[test]
fn missing_and_undecodable_linked_images_keep_their_alt_text_and_warning() {
    let source = "[![Fallback label](figure.svg)](https://example.com)";
    let document = parse_markdown(source);
    for opts in [
        PdfOptions::default(),
        PdfOptions {
            image_assets: vec![PdfImageAsset::new("figure.svg", b"bad image".to_vec())],
            ..PdfOptions::default()
        },
    ] {
        let pdf = render_pdf_document(&document, &opts).unwrap();
        let text = pdf_text(&pdf);
        assert!(!text.contains("/S /Figure "));
        assert!(text.contains("/URI (https://example.com)"));
        let layer = franken_markdown::pdf::verification_text_layer(&document, &opts).unwrap();
        assert!(
            layer
                .pages
                .iter()
                .flat_map(|page| &page.runs)
                .any(|run| run.text == "Fallback label")
        );
        assert_eq!(render_warnings(&document, &opts).len(), 1);
    }
}

#[test]
fn leading_list_figures_keep_normal_size_markers_and_nested_list_structure() {
    let opts = options();
    for (prefix, marker) in [("- ", "•"), ("3. ", "3."), ("- [x] ", "[x]")] {
        let source = format!(
            "{prefix}[![Architecture](figure.svg)](https://example.com)\n\n  Continued body."
        );
        let document = parse_markdown(&source);
        let pdf = render_pdf_document(&document, &opts).unwrap();
        let text = pdf_text(&pdf);
        for tag in ["/S /L ", "/S /LI ", "/S /LBody ", "/S /Lbl ", "/S /Figure "] {
            assert!(text.contains(tag), "missing {tag} for {source}");
        }
        let layer = franken_markdown::pdf::verification_text_layer(&document, &opts).unwrap();
        let run = layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .find(|run| run.kind == "image")
            .unwrap();
        assert_eq!(run.text.trim(), marker);
        assert_eq!(run.size, 11.0);
        let bbox = array_after(&text, "/BBox [");
        assert!(
            (run.y - (bbox[3] - 14.52)).abs() < 0.02,
            "top-aligned marker: {run:?}, {bbox:?}"
        );
        assert!(run.x < bbox[0], "marker stays in the gutter");
        assert!(run.overshoot.is_none());
        assert_eq!(pdf, render_pdf(&source, &opts).unwrap());
    }

    let text = pdf_text(
        &render_pdf(
            "> - Outer item\n>   - [![Nested](figure.svg)](https://example.com)",
            &opts,
        )
        .unwrap(),
    );
    assert_eq!(text.matches("/S /L ").count(), 2);
    assert!(text.contains("/S /BlockQuote ") && text.contains("/Alt (Nested)"));
}

#[test]
fn tall_list_figure_and_marker_move_together_and_stay_in_the_page_box() {
    for optimal in [false, true] {
        let mut opts = options();
        opts.optimal_pagination = optimal;
        opts.theme.page.size = PageSize {
            name: "image-test",
            width_pt: 200.0,
            height_pt: 160.0,
        };
        opts.theme.page.margins = PageMargins {
            top_pt: 18.0,
            right_pt: 18.0,
            bottom_pt: 18.0,
            left_pt: 18.0,
        };
        let document = parse_markdown(
            "# Heading\n\nSome introductory text that fills space.\n\n- [![Architecture](figure.svg)](https://example.com)\n\nAfter the figure.",
        );
        let pdf = render_pdf_document(&document, &opts).unwrap();
        let text = pdf_text(&pdf);
        let bbox = array_after(&text, "/BBox [");
        assert!(bbox[0] >= 18.0 && bbox[1] >= 18.0);
        assert!(bbox[2] <= 182.0 && bbox[3] <= 142.0);
        let layer = franken_markdown::pdf::verification_text_layer(&document, &opts).unwrap();
        let figures: Vec<_> = layer
            .pages
            .iter()
            .flat_map(|page| {
                page.runs
                    .iter()
                    .filter(|run| run.kind == "image")
                    .map(move |run| (page.number, run))
            })
            .collect();
        assert_eq!(figures.len(), 1);
        assert!(figures[0].0 > 1, "figure moves to a page with enough room");
        assert_eq!(figures[0].1.text, "•");
        assert_eq!(
            pdf,
            render_pdf_document_emitted(&document, &opts, PdfEmitOptions::monolithic()).unwrap()
        );
    }
}

#[test]
fn blockquote_bars_cover_the_complete_figure_and_its_list_marker() {
    for source in [
        "> ![Architecture](figure.svg)",
        "> - [![Architecture](figure.svg)](https://example.com)",
    ] {
        let pdf = render_pdf(source, &options()).unwrap();
        let bbox = array_after(&pdf_text(&pdf), "/BBox [");
        let content = page_content(&pdf);
        let stroke: Vec<_> = content
            .split_once("2.50 w ")
            .expect("quote bar")
            .1
            .split_whitespace()
            .take(6)
            .collect();
        let top: f32 = stroke[1].parse().unwrap();
        let bottom: f32 = stroke[4].parse().unwrap();
        assert!(
            (top - bbox[3]).abs() < 0.02,
            "bar covers figure top: {source}"
        );
        assert!(
            (bottom - bbox[1]).abs() < 0.02,
            "bar covers figure bottom: {source}"
        );
    }
}

fn png_chunk(kind: &[u8; 4], data: &[u8], png: &mut Vec<u8>) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    png.extend_from_slice(&0u32.to_be_bytes());
}

#[test]
fn linked_png_embeds_the_raster_instead_of_its_alt_text() {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&32u32.to_be_bytes());
    header.extend_from_slice(&16u32.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    let pixels = vec![0u8; 16 * (1 + 32 * 3)];
    png_chunk(b"IHDR", &header, &mut png);
    png_chunk(
        b"IDAT",
        &franken_markdown::compress::zlib_compress(&pixels),
        &mut png,
    );
    png_chunk(b"IEND", &[], &mut png);
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("badge.png", png)],
        ..PdfOptions::default()
    };
    let pdf = render_pdf(
        "[![Build passing](badge.png)](https://example.com/build)",
        &opts,
    )
    .unwrap();
    let text = pdf_text(&pdf);
    let content = page_content(&pdf);
    assert!(text.contains("/Subtype /Image /Width 32 /Height 16"));
    assert!(content.contains("/Im1 Do"));
    assert!(text.contains("/Alt (Build passing)"));
    assert_eq!(array_after(&text, "/Rect ["), array_after(&text, "/BBox ["));
}
