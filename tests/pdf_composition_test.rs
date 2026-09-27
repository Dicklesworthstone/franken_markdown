//! End-to-end PDF composition regressions. This integration target exercises
//! the public renderer without compiling the much larger private unit suite.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::{PdfEmitOptions, RenderWarning, verification_text_layer};
use franken_markdown::{FontFamily, PdfImageAsset, PdfOptions, fonts, parse_markdown};

fn nfd(source: &str) -> String {
    source
        .replace('é', "e\u{0301}")
        .replace('Å', "A\u{030a}")
        .replace('ö', "o\u{0308}")
        .replace('ï', "i\u{0308}")
}

fn content(pdf: &[u8]) -> String {
    let mut all = String::new();
    for reference in String::from_utf8_lossy(pdf).split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let header = format!("\n{number} 0 obj\n");
        let offset = pdf
            .windows(header.len())
            .position(|x| x == header.as_bytes())
            .unwrap()
            + header.len();
        let object = &pdf[offset..];
        let start = object.windows(8).position(|x| x == b"\nstream\n").unwrap();
        let dictionary = std::str::from_utf8(&object[..start]).unwrap();
        let len: usize = dictionary
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let bytes = &object[start + 8..start + 8 + len];
        if dictionary.contains("/FlateDecode") {
            all.push_str(
                std::str::from_utf8(
                    &franken_markdown::zlib_decompress(bytes, 32 * 1024 * 1024).unwrap(),
                )
                .unwrap(),
            );
        } else {
            all.push_str(std::str::from_utf8(bytes).unwrap());
        }
    }
    assert!(!all.is_empty());
    all
}

fn draw_ops(pdf: &[u8]) -> Vec<String> {
    content(pdf)
        .lines()
        .filter(|line| line.contains("BT /F"))
        .map(str::to_owned)
        .collect()
}

fn render(source: &str, options: &PdfOptions, name: &str) -> Vec<u8> {
    let doc = parse_markdown(source);
    franken_markdown::render_pdf_document(&doc, options)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn options(family: FontFamily) -> PdfOptions {
    let mut options = PdfOptions {
        page_numbers: false,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    options.theme.font = family;
    options
}

#[test]
fn public_mixed_styles_and_tables_match_precomposed_glyph_geometry() {
    let canonical = "# Café\n\nCafé **Å** *ö* ***é*** [Café](https://example.com) ~~Café~~ and `naïve`.\n\n| Café | Å |\n| --- | --- |\n| Café | **é** |\n\n```\ncafé\n```\n";
    let decomposed = nfd(canonical);
    for (family, name) in [(FontFamily::Sans, "sans"), (FontFamily::Serif, "serif")] {
        let opts = options(family);
        let a = render(canonical, &opts, &format!("{name}-canonical"));
        let b = render(&decomposed, &opts, &format!("{name}-decomposed"));
        assert_eq!(
            draw_ops(&a),
            draw_ops(&b),
            "all emitted font/glyph/TJ/position operators: {name}"
        );
        assert!(
            !franken_markdown::pdf::render_warnings(&parse_markdown(&decomposed), &opts)
                .iter()
                .any(|w| matches!(w, RenderWarning::MissingGlyphs { .. }))
        );
        let x = verification_text_layer(&parse_markdown(canonical), &opts).unwrap();
        let y = verification_text_layer(&parse_markdown(&decomposed), &opts).unwrap();
        assert_eq!(x.page_count, y.page_count);
        for (a, b) in x.pages.iter().zip(&y.pages) {
            assert_eq!(a.runs.len(), b.runs.len());
            for (a, b) in a.runs.iter().zip(&b.runs) {
                assert_eq!(
                    (a.x, a.y, a.size, a.overshoot),
                    (b.x, b.y, b.size, b.overshoot)
                );
                assert_eq!(nfd(&a.text), b.text);
            }
        }
    }
}

#[test]
fn public_exact_original_spelling_and_canonical_subset_map() {
    let opts = options(FontFamily::Sans);
    let source = "Café\n\nCafe\u{0301}\n\nCafé\n\nCafe\u{0301}";
    let pdf = render(source, &opts, "mixed-occurrences");
    assert_eq!(
        content(&pdf)
            .matches("/ActualText <FEFF00430061006600650301>")
            .count(),
        2
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("> <00E9>\n"));
    assert!(!raw.contains("> <00650301>\n"));
    let only = render("Cafe\u{0301}", &opts, "nfd-only");
    assert!(String::from_utf8_lossy(&only).contains("> <00E9>\n"));
}

#[test]
fn public_code_and_table_wrapping_preserve_source_clusters() {
    for (family, name) in [(FontFamily::Sans, "sans"), (FontFamily::Serif, "serif")] {
        let mut opts = options(family);
        opts.theme.page.size.width_pt = 200.0;
        opts.theme.page.margins.left_pt = 20.0;
        opts.theme.page.margins.right_pt = 20.0;
        let code = nfd(&"éÅoffice".repeat(20));
        let cell = nfd(&"é".repeat(70));
        let markdown = format!(
            "```\n{code}\n```\n\n| A | B | C | D |\n| --- | --- | --- | --- |\n| {cell} | {cell} | {cell} | {cell} |\n"
        );
        render(&markdown, &opts, &format!("{name}-narrow"));
        let doc = parse_markdown(&markdown);
        let layer = verification_text_layer(&doc, &opts).unwrap();
        let mut joined_code = String::new();
        for row in layer.pages.iter().flat_map(|p| &p.runs) {
            assert!(
                row.overshoot.is_none(),
                "{name} {:?}: {:?}",
                row.text,
                row.overshoot
            );
            assert!(
                !row.text
                    .chars()
                    .next()
                    .is_some_and(|ch| matches!(ch as u32, 0x0300..=0x036f))
            );
            if row.kind == "code" {
                joined_code.push_str(&row.text);
            }
        }
        assert_eq!(joined_code, code);
    }
}

#[test]
fn public_host_font_missing_composite_is_not_silently_admitted() {
    let body = franken_markdown::text::Font::parse(
        fonts::body_bytes(FontFamily::Sans, fonts::FontStyle::Regular).to_vec(),
    )
    .unwrap();
    let mut opts = options(FontFamily::Sans);
    opts.font_assets.body_regular = Some(body.subset(&['e', ' ']).unwrap());
    let source = "e\u{0301}";
    let warnings = franken_markdown::pdf::render_warnings(&parse_markdown(source), &opts);
    assert!(warnings.iter().any(|w| matches!(w, RenderWarning::MissingGlyphs { count: 1, sample } if sample == "\u{0301}")), "{warnings:?}");
    let fallback = render(source, &opts, "host-missing-composite");
    assert!(!content(&fallback).contains("/ActualText"));
    let strong = format!("**{source}**");
    assert!(
        !franken_markdown::pdf::render_warnings(&parse_markdown(&strong), &opts)
            .iter()
            .any(|w| matches!(w, RenderWarning::MissingGlyphs { .. }))
    );
}

#[test]
fn public_chunked_composition_is_deterministic() {
    use franken_markdown::ast::{Block, Document, Inline};
    let mut blocks = Vec::new();
    for p in 0..3 {
        if p > 0 {
            blocks.push(Block::PageBreak);
        }
        blocks.push(Block::Paragraph(vec![Inline::Text("Cafe\u{0301}".into())]));
    }
    let doc = Document { blocks };
    let opts = options(FontFamily::Sans);
    let chunked =
        franken_markdown::render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default())
            .unwrap();
    let mono =
        franken_markdown::render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic())
            .unwrap();
    assert_eq!(chunked, mono);
    assert_eq!(
        chunked,
        franken_markdown::render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default())
            .unwrap()
    );
    assert_eq!(
        content(&chunked)
            .matches("/ActualText <FEFF00430061006600650301>")
            .count(),
        3
    );
}

#[test]
fn public_svg_anchor_and_replayed_strokes_keep_exact_source() {
    let mut outputs = Vec::new();
    for (source, name) in [
        ("Café", "svg-canonical"),
        ("Cafe\u{0301}", "svg-decomposed"),
    ] {
        let mut opts = options(FontFamily::Sans);
        opts.image_assets.push(PdfImageAsset { destination: "marked.svg".into(), bytes: format!("<svg xmlns='http://www.w3.org/2000/svg' width='200' height='40'><text x='100' y='25' text-anchor='middle' font-size='18' fill='#123456' stroke='#654321' stroke-width='1' paint-order='stroke fill'>{source}</text></svg>").into_bytes() });
        let pdf = render("![marked](marked.svg)", &opts, name);
        assert!(
            !franken_markdown::pdf::render_warnings(
                &parse_markdown("![marked](marked.svg)"),
                &opts
            )
            .iter()
            .any(|w| matches!(w, RenderWarning::MissingGlyphs { .. }))
        );
        outputs.push(pdf);
    }
    assert_eq!(draw_ops(&outputs[0]), draw_ops(&outputs[1]));
    let stream = content(&outputs[1]);
    assert_eq!(
        stream
            .matches("/ActualText <FEFF00430061006600650301>")
            .count(),
        1
    );
    let marked = stream
        .split_once("/ActualText")
        .unwrap()
        .1
        .split_once("EMC")
        .unwrap()
        .0;
    assert_eq!(marked.matches("BT /F").count(), 2);
}
