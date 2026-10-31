//! Ordinary PDF prose is measured against the real containing width.
//! Read the emitted CIDs, font widths, matrices and link rectangles rather than
//! accepting the paragraph planner's own assertion that a line fits.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::{
    PageMargins, PageSize, PdfEmitOptions, PdfOptions, parse_markdown, render_pdf,
    render_pdf_document_emitted,
};

fn object(pdf: &[u8], id: usize) -> &[u8] {
    let header = format!("\n{id} 0 obj\n");
    let start = pdf
        .windows(header.len())
        .position(|part| part == header.as_bytes())
        .expect("referenced PDF object exists")
        + header.len();
    &pdf[start..]
}

fn dictionary(object: &[u8]) -> &str {
    let end = object.iter().position(|&byte| byte == b'\n').unwrap();
    std::str::from_utf8(&object[..end]).unwrap()
}

fn number_after(text: &str, marker: &str) -> usize {
    text.split_once(marker)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn stream(pdf: &[u8], id: usize) -> String {
    let object = object(pdf, id);
    let start = object
        .windows(8)
        .position(|part| part == b"\nstream\n")
        .unwrap();
    let dictionary = std::str::from_utf8(&object[..start]).unwrap();
    let len = number_after(dictionary, "/Length ");
    let data = &object[start + 8..start + 8 + len];
    let data = if dictionary.contains("/FlateDecode") {
        franken_markdown::zlib_decompress(data, 16 * 1024 * 1024).unwrap()
    } else {
        assert!(!dictionary.contains("/Filter"));
        data.to_vec()
    };
    String::from_utf8(data).unwrap()
}

struct FontMap {
    unicode: BTreeMap<u16, String>,
    widths: Vec<f32>,
}

fn font_map(pdf: &[u8], id: usize) -> FontMap {
    let font = dictionary(object(pdf, id));
    let cmap = stream(pdf, number_after(font, "/ToUnicode "));
    let mut unicode = BTreeMap::new();
    let mut in_chars = false;
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            in_chars = true;
        } else if line == "endbfchar" {
            in_chars = false;
        } else if in_chars {
            let (cid, text) = line.split_once(' ').unwrap();
            let cid = u16::from_str_radix(cid.trim_matches(['<', '>']), 16).unwrap();
            let text = text.trim_matches(['<', '>']);
            let units: Vec<_> = text
                .as_bytes()
                .chunks_exact(4)
                .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
                .collect();
            unicode.insert(cid, String::from_utf16(&units).unwrap());
        }
    }
    let descendant = number_after(font, "/DescendantFonts [");
    let widths = dictionary(object(pdf, descendant))
        .split_once("/W [0 [")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    FontMap { unicode, widths }
}

#[derive(Debug)]
struct PaintedRun {
    font: String,
    text: String,
    x: f32,
    y: f32,
    width: f32,
    scale: f32,
    size: f32,
}

/// Decode the writer's actual font resources, CIDs, widths, and TJ adjustments.
/// No AST or paragraph-layout helper participates in these assertions.
fn painted_runs(pdf: &[u8]) -> Vec<PaintedRun> {
    let raw = String::from_utf8_lossy(pdf);
    let mut runs = Vec::new();
    for page in raw.lines().filter(|line| line.contains("/Type /Page ")) {
        let entries: Vec<_> = page
            .split_once("/Font <<")
            .unwrap()
            .1
            .split_once(">>")
            .unwrap()
            .0
            .split_whitespace()
            .collect();
        let fonts: BTreeMap<_, _> = entries
            .chunks_exact(4)
            .map(|entry| (entry[0], font_map(pdf, entry[1].parse().unwrap())))
            .collect();
        let content = stream(pdf, number_after(page, "/Contents "));
        for run in content.split("BT ").skip(1) {
            let run = run.split_once(" ET").unwrap().0;
            let (head, tail) = run.split_once('[').expect("a text-showing array");
            let head: Vec<_> = head.split_whitespace().collect();
            let tf = head.iter().position(|&token| token == "Tf").unwrap();
            let tm = head.iter().position(|&token| token == "Tm").unwrap();
            let font_name = head[tf - 2];
            let font = &fonts[font_name];
            let size: f32 = head[tf - 1].parse().unwrap();
            let scale = head
                .iter()
                .position(|&token| token == "Tz")
                .map_or(1.0, |index| head[index - 1].parse::<f32>().unwrap() / 100.0);
            let array = tail
                .split_once(']')
                .unwrap()
                .0
                .replace('<', " <")
                .replace('>', "> ");
            let mut text = String::new();
            let mut width = 0.0;
            for item in array.split_whitespace() {
                if let Some(hex) = item.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
                    for code in hex.as_bytes().chunks_exact(4) {
                        let cid =
                            u16::from_str_radix(std::str::from_utf8(code).unwrap(), 16).unwrap();
                        text.push_str(
                            font.unicode
                                .get(&cid)
                                .expect("painted source has a Unicode map"),
                        );
                        width += font.widths[usize::from(cid)];
                    }
                } else {
                    width -= item.parse::<f32>().unwrap();
                }
            }
            runs.push(PaintedRun {
                font: font_name.to_owned(),
                text,
                x: head[tm - 2].parse().unwrap(),
                y: head[tm - 1].parse().unwrap(),
                width: width * size * scale / 1000.0,
                scale,
                size,
            });
        }
    }
    assert!(!runs.is_empty(), "the source must paint actual text");
    runs
}

fn options(measure: f32, size: f32) -> PdfOptions {
    let mut opts = PdfOptions {
        base_font_size: Some(size),
        page_numbers: false,
        ..PdfOptions::default()
    };
    opts.theme.page.size = PageSize {
        name: "inline-code-test",
        width_pt: measure + 48.0,
        height_pt: 600.0,
    };
    opts.theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    opts
}

fn painted_text(runs: &[PaintedRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

fn assert_inside(runs: &[PaintedRun], options: &PdfOptions) {
    let left = options.theme.page.margins.left_pt;
    let right = options.theme.page.size.width_pt - options.theme.page.margins.right_pt;
    for run in runs {
        assert!(
            run.x >= left - 0.03 && run.x + run.width <= right + 0.03,
            "painted glyphs exceed [{left}, {right}]: {run:?}",
        );
        assert_eq!(run.scale, 1.0, "fit must not shrink glyphs");
    }
}

#[test]
fn short_and_long_ordinary_words_fit_at_the_requested_type_size() {
    for size in [11.0, 24.0] {
        for measure in [44.0, 132.0] {
            let options = options(measure, size);
            for source in [
                "WWWWWWWWWWW",
                "0123456789012345678901234567890123456789",
                "WWéWWΩWW",
            ] {
                let runs = painted_runs(&render_pdf(source, &options).unwrap());
                assert_eq!(painted_text(&runs), source, "all source glyphs survive");
                assert_inside(&runs, &options);
                assert!(runs.iter().all(|run| run.size == size));
            }
        }
    }
}

#[test]
fn heading_and_styled_word_fragments_keep_their_full_text_and_fonts() {
    let options = options(80.0, 11.0);
    for (source, expected) in [
        ("# WWWWWWWWWWW", "WWWWWWWWWWW"),
        ("WWWW**WWWW**WWWW", "WWWWWWWWWWWW"),
        ("WWWW*WWWW*WWWW", "WWWWWWWWWWWW"),
        ("~~WWWWWWWWWWWW~~", "WWWWWWWWWWWW"),
    ] {
        let runs = painted_runs(&render_pdf(source, &options).unwrap());
        assert_eq!(painted_text(&runs), expected);
        assert_inside(&runs, &options);
        if source.contains("**") {
            assert!(runs.iter().any(|run| run.font == "/F2"));
            assert!(runs.iter().any(|run| run.font == "/F1"));
        }
        if source.starts_with('#') {
            assert!(runs.iter().all(|run| run.size == 24.0));
        }
    }
}

#[test]
fn measured_breaks_keep_decomposed_clusters_and_exact_extraction_text() {
    let cluster = "e\u{0301}";
    let expected = cluster.repeat(20);
    let options = options(44.0, 24.0);
    for source in [
        expected.clone(),
        format!("**{}**{}", cluster.repeat(8), cluster.repeat(12)),
    ] {
        let pdf = render_pdf(&source, &options).unwrap();
        let runs = painted_runs(&pdf);
        // The emitted cmap identifies complete canonical glyphs; ActualText
        // must independently retain their original decomposed source spelling.
        assert_eq!(painted_text(&runs), "é".repeat(20));
        assert_inside(&runs, &options);
        let raw = String::from_utf8_lossy(&pdf);
        let mut logical = String::new();
        for page in raw.lines().filter(|line| line.contains("/Type /Page ")) {
            let content = stream(&pdf, number_after(page, "/Contents "));
            for text in content.split("/ActualText <").skip(1) {
                let hex = text.split_once('>').unwrap().0;
                let units: Vec<_> = hex
                    .as_bytes()
                    .chunks_exact(4)
                    .map(|unit| {
                        u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap()
                    })
                    .collect();
                assert_eq!(units[0], 0xfeff);
                logical.push_str(&String::from_utf16(&units[1..]).unwrap());
            }
        }
        assert_eq!(logical, expected);
    }
}

#[test]
fn deeply_indented_prose_uses_its_real_remaining_measure() {
    let options = options(52.0, 11.0);
    for source in ["- WWWWWWWWWWW", "> WWWWWWWWWWW", "> - WWWWWWWWWWW"] {
        let runs = painted_runs(&render_pdf(source, &options).unwrap());
        assert_eq!(painted_text(&runs).replace('•', ""), "WWWWWWWWWWW");
        assert_inside(&runs, &options);
    }
}

#[test]
fn wrapped_url_links_keep_their_targets_and_click_rectangles_inside_the_page() {
    let destination = "https://example.com/WWWWWWWWWWWWWWWWWWWWWWWWWWWWWW";
    let options = options(62.0, 11.0);
    for source in [
        destination.to_owned(),
        format!("[{destination}]({destination})"),
        format!("[WWWW**WWWWWWWW**WWWW]({destination})"),
    ] {
        let pdf = render_pdf(&source, &options).unwrap();
        let runs = painted_runs(&pdf);
        let expected = if source.contains("**") {
            "WWWWWWWWWWWWWWWW"
        } else {
            destination
        };
        assert_eq!(painted_text(&runs), expected);
        assert_inside(&runs, &options);
        let raw = String::from_utf8_lossy(&pdf);
        let annotations: Vec<_> = raw
            .lines()
            .filter(|line| line.contains("/Subtype /Link"))
            .collect();
        assert!(annotations.len() > 1, "link needs wrapped click targets");
        for annotation in annotations {
            assert!(annotation.contains(&format!("/URI ({destination})")));
            let rectangle: Vec<f32> = annotation
                .split_once("/Rect [")
                .unwrap()
                .1
                .split_once(']')
                .unwrap()
                .0
                .split_whitespace()
                .map(|number| number.parse().unwrap())
                .collect();
            assert!(rectangle[0] >= 24.0 - 0.03);
            assert!(
                rectangle[2] <= 86.0 + 0.03,
                "overwide annotation {annotation}"
            );
        }
    }
}

#[test]
fn repaired_lines_participate_in_exact_pagination_and_emitter_parity() {
    let source = "WWWWWWWWWWW".repeat(8);
    let mut options = options(44.0, 11.0);
    options.theme.page.size.height_pt = 180.0;
    options.strict_pagination = true;
    let doc = parse_markdown(&source);
    let chunked = render_pdf_document_emitted(&doc, &options, PdfEmitOptions::default()).unwrap();
    let monolithic = render_pdf_document_emitted(
        &doc,
        &options,
        PdfEmitOptions {
            emission: franken_markdown::PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(chunked, monolithic);
    assert_eq!(chunked, render_pdf(&source, &options).unwrap());
    let runs = painted_runs(&chunked);
    assert_eq!(painted_text(&runs), source);
    assert_inside(&runs, &options);
    assert!(runs.iter().all(|run| run.y >= 24.0));
    let browser = franken_markdown::wasm::WasmRenderOptions {
        theme: options.theme.clone(),
        strict_pagination: true,
        ..franken_markdown::wasm::WasmRenderOptions::default()
    };
    assert_eq!(
        chunked,
        franken_markdown::wasm::render_pdf(&source, &browser)
            .unwrap()
            .bytes
    );
}
