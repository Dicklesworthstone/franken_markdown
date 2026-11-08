//! Preserve complete first-block note figures through real PDF preparation.
//! Inspect both emitted figure metadata and decoded SVG drawing operations.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PdfEmitOptions, PdfImageAsset, PdfOptions, parse_markdown, render_pdf_document,
    render_pdf_document_emitted,
};

fn options(height: u32) -> PdfOptions {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="{height}"><rect width="400" height="{height}" fill="blue"/></svg>"#
    );
    PdfOptions {
        image_assets: vec![PdfImageAsset::new("figure.svg", svg.into_bytes())],
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

/// Fixtures paint one filled rectangle per figure. Follow each actual matrix
/// and rectangle, rather than treating a declared bounding box as painted ink.
fn figure_bounds(pdf: &[u8]) -> Vec<[f32; 4]> {
    let text = pdf_text(pdf);
    let tagged: Vec<_> = text
        .split("/Type /StructElem /S /Figure ")
        .skip(1)
        .map(|figure| array_after(figure, "/BBox ["))
        .collect();
    let content = page_content(pdf);
    let painted: Vec<_> = content
        .split("/Figure <</MCID ")
        .skip(1)
        .map(|figure| {
            let fields: Vec<_> = figure
                .split_once("EMC")
                .unwrap()
                .0
                .split_whitespace()
                .collect();
            let cm = fields.iter().position(|&field| field == "cm").unwrap();
            let re = fields.iter().position(|&field| field == "re").unwrap();
            assert_eq!(fields[re + 1], "f", "a painted rectangle");
            assert_eq!(
                fields[..re].iter().filter(|&&field| field == "cm").count(),
                1
            );
            let [a, b, c, d, e, f]: [f32; 6] =
                std::array::from_fn(|index| fields[cm - 6 + index].parse().unwrap());
            let [x, y, w, h]: [f32; 4] =
                std::array::from_fn(|index| fields[re - 4 + index].parse().unwrap());
            let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
                .map(|(x, y)| (a * x + c * y + e, b * x + d * y + f));
            [
                corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min),
                corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min),
                corners
                    .iter()
                    .map(|p| p.0)
                    .fold(f32::NEG_INFINITY, f32::max),
                corners
                    .iter()
                    .map(|p| p.1)
                    .fold(f32::NEG_INFINITY, f32::max),
            ]
        })
        .collect();
    assert_eq!(painted.len(), tagged.len());
    for (painted, tagged) in painted.iter().zip(&tagged) {
        assert!(
            painted
                .iter()
                .zip(tagged)
                .all(|(a, b)| (a - b).abs() < 0.025),
            "painted {painted:?}, tagged {tagged:?}"
        );
    }
    tagged
}

fn assert_sizes(pdf: &[u8], count: usize, width: f32, height: f32) -> Vec<[f32; 4]> {
    let figures = figure_bounds(pdf);
    assert_eq!(figures.len(), count);
    for bounds in &figures {
        assert!(
            (bounds[2] - bounds[0] - width).abs() < 0.025,
            "figure width: {bounds:?}, expected {width}"
        );
        assert!(
            (bounds[3] - bounds[1] - height).abs() < 0.025,
            "figure height: {bounds:?}, expected {height}"
        );
    }
    figures
}

fn note_destinations(pdf: &[u8]) -> Vec<f32> {
    pdf_text(pdf)
        .split("/Subtype /Link")
        .skip(1)
        .filter_map(|annotation| {
            let object = annotation.split_once("endobj").unwrap().0;
            let destination = object.split_once("/Dest [")?.1.split_once(']').unwrap().0;
            let fields: Vec<_> = destination.split_whitespace().collect();
            assert_eq!(&fields[1..5], &["0", "R", "/XYZ", "null"]);
            Some(fields[5].parse().unwrap())
        })
        .collect()
}

#[test]
fn first_note_figures_keep_their_size_links_and_labels() {
    for figure in [
        "![Architecture](figure.svg)",
        "**![Architecture](figure.svg)**",
        "[***![Architecture](figure.svg)***](https://example.com/evidence)",
    ] {
        for backlinks in [false, true] {
            let source = format!("Evidence[^plot].\n\n[^plot]: {figure}\n");
            let document = parse_markdown(&source);
            let original = document.clone();
            let opts = PdfOptions {
                footnote_backlinks: backlinks,
                ..options(200)
            };
            let pdf = render_pdf_document(&document, &opts).unwrap();
            let bounds = assert_sizes(&pdf, 1, 300.0, 150.0)[0];
            let raw = pdf_text(&pdf);
            assert!(raw.contains("/Alt (Architecture)"));
            assert!(
                !raw.contains("/Title (Notes)"),
                "the figure fits at the page foot"
            );
            if figure.contains("https://") {
                let annotation = raw
                    .split("/Subtype /Link")
                    .skip(1)
                    .find(|part| {
                        part.split_once("endobj")
                            .unwrap()
                            .0
                            .contains("/URI (https://example.com/evidence)")
                    })
                    .unwrap();
                assert_eq!(array_after(annotation, "/Rect ["), bounds);
            }
            let report = franken_markdown::verify::verify_pdf(&document, &opts).unwrap();
            assert_eq!(report.page_count, 1);
            // Verification runs contain a complete line, so the citation
            // remains attached to its prose while the figure's label is alone.
            let text_runs: Vec<_> = report.pages[0]
                .runs
                .iter()
                .filter(|run| !run.text.is_empty())
                .collect();
            assert_eq!(
                text_runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<Vec<_>>(),
                ["Evidence¹.", "¹"],
                "the body citation and separate note label are each printed once"
            );
            let mut label_positions: Vec<_> =
                text_runs.iter().map(|run| run.y + run.size * 0.9).collect();
            label_positions.sort_by(f32::total_cmp);
            assert!(
                label_positions[0] > bounds[3],
                "note label precedes its figure"
            );
            let mut destinations = note_destinations(&pdf);
            destinations.sort_by(f32::total_cmp);
            let expected = if backlinks {
                label_positions.as_slice()
            } else {
                &label_positions[..1]
            };
            assert_eq!(destinations.len(), expected.len());
            for (actual, expected) in destinations.iter().zip(expected) {
                assert!((actual - expected).abs() < 0.02);
            }
            assert_eq!(
                pdf,
                render_pdf_document_emitted(&document, &opts, PdfEmitOptions::monolithic())
                    .unwrap()
            );
            assert_eq!(document, original);
        }
    }
}

#[test]
fn tall_first_figure_falls_back_to_complete_endnotes() {
    let document = parse_markdown("Evidence[^plot].\n\n[^plot]: ![Architecture](figure.svg)\n");
    let opts = PdfOptions {
        footnote_backlinks: true,
        ..options(600)
    };
    let pdf = render_pdf_document(&document, &opts).unwrap();
    assert_sizes(&pdf, 1, 300.0, 450.0);
    assert!(pdf_text(&pdf).contains("/Title (Notes)"));
    assert_eq!(note_destinations(&pdf).len(), 2);
}

#[test]
fn first_note_gallery_and_explicit_endnotes_retain_each_figure() {
    let document =
        parse_markdown("Evidence[^plot].\n\n[^plot]: ![First](figure.svg) ![Second](figure.svg)\n");
    let opts = options(100);
    let pdf = render_pdf_document(&document, &opts).unwrap();
    assert_sizes(&pdf, 2, 300.0, 75.0);
    assert!(!pdf_text(&pdf).contains("/Title (Notes)"));
    let prepared = document.with_endnotes();
    let endnotes = render_pdf_document(&prepared, &opts).unwrap();
    assert_sizes(&endnotes, 2, 300.0, 75.0);
    assert!(pdf_text(&endnotes).contains("/Title (Notes)"));
}

#[test]
fn prose_images_retain_inline_sizing() {
    let document = parse_markdown(
        "Evidence[^plot].\n\n[^plot]: See ![Architecture](figure.svg) for evidence.\n",
    );
    let pdf = render_pdf_document(&document, &options(200)).unwrap();
    let bounds = figure_bounds(&pdf);
    assert_eq!(bounds.len(), 1);
    assert!(bounds[0][3] - bounds[0][1] < 25.0);
}
