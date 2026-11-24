//! Check final font widths and TJ adjustments, independently of layout helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use std::collections::BTreeMap;

use franken_markdown::{
    FontFamily, PageMargins, PageSize, PdfEmitOptions, PdfOptions, PdfPageEmission, render_pdf,
    render_pdf_emitted,
};

fn object(pdf: &[u8], id: usize) -> &[u8] {
    let marker = format!("\n{id} 0 obj\n");
    let start = pdf
        .windows(marker.len())
        .position(|part| part == marker.as_bytes())
        .unwrap();
    &pdf[start + marker.len()..]
}

fn dictionary(object: &[u8]) -> &str {
    let end = object.iter().position(|&byte| byte == b'\n').unwrap();
    std::str::from_utf8(&object[..end]).unwrap()
}

fn number(text: &str, marker: &str) -> usize {
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
    let dict = std::str::from_utf8(&object[..start]).unwrap();
    let data = &object[start + 8..start + 8 + number(dict, "/Length ")];
    let data = if dict.contains("/FlateDecode") {
        franken_markdown::zlib_decompress(data, 16 * 1024 * 1024).unwrap()
    } else {
        assert!(!dict.contains("/Filter"));
        data.to_vec()
    };
    String::from_utf8(data).unwrap()
}

struct Font {
    unicode: BTreeMap<u16, String>,
    widths: Vec<f32>,
}

fn font(pdf: &[u8], id: usize) -> Font {
    let dict = dictionary(object(pdf, id));
    let cmap = stream(pdf, number(dict, "/ToUnicode "));
    let mut unicode = BTreeMap::new();
    let mut inside = false;
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            inside = true;
        } else if line == "endbfchar" {
            inside = false;
        } else if inside {
            let (cid, value) = line.split_once(' ').unwrap();
            let cid = u16::from_str_radix(cid.trim_matches(['<', '>']), 16).unwrap();
            let units: Vec<_> = value
                .trim_matches(['<', '>'])
                .as_bytes()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
                .collect();
            unicode.insert(cid, String::from_utf16(&units).unwrap());
        }
    }
    let descendant = number(dict, "/DescendantFonts [");
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
    Font { unicode, widths }
}

#[derive(Debug)]
struct Run {
    page: usize,
    text: String,
    x: f32,
    y: f32,
    width: f32,
    size: f32,
}

fn runs(pdf: &[u8]) -> Vec<Run> {
    let raw = String::from_utf8_lossy(pdf);
    let mut result = Vec::new();
    for (page_index, page) in raw
        .lines()
        .filter(|line| line.contains("/Type /Page "))
        .enumerate()
    {
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
            .as_chunks::<4>()
            .0
            .iter()
            .map(|entry| (entry[0], font(pdf, entry[1].parse().unwrap())))
            .collect();
        let content = stream(pdf, number(page, "/Contents "));
        let mut selected = "";
        let mut size = 0.0;
        let mut scale = 1.0;
        let mut x = 0.0;
        let mut y = 0.0;
        let mut invisible = false;
        for operation in pdf_content::operations(&content) {
            match operation.operator {
                "BT" => {
                    x = 0.0;
                    y = 0.0;
                }
                "Tf" => {
                    selected = operation.operands[0];
                    size = operation.operands[1].parse().unwrap();
                }
                "Tm" => {
                    assert_eq!(&operation.operands[..4], &["1", "0", "0", "1"]);
                    x = operation.operands[4].parse().unwrap();
                    y = operation.operands[5].parse().unwrap();
                }
                "Tz" => scale = operation.operands[0].parse::<f32>().unwrap() / 100.0,
                "Tr" => invisible = operation.operands[0] == "3",
                "TJ" | "Tj" => {
                    let font = &fonts[selected];
                    let cids = pdf_content::glyph_ids(&operation);
                    let text: String = cids.iter().map(|cid| font.unicode[cid].as_str()).collect();
                    let mut width: f32 =
                        cids.iter().map(|cid| font.widths[usize::from(*cid)]).sum();
                    for operand in &operation.operands {
                        if let Ok(adjustment) = operand.parse::<f32>() {
                            width -= adjustment;
                        }
                    }
                    width *= size * scale / 1000.0;
                    if !invisible && !text.is_empty() {
                        result.push(Run {
                            page: page_index,
                            text,
                            x,
                            y,
                            width,
                            size,
                        });
                    }
                    x += width;
                }
                _ => {}
            }
        }
    }
    assert!(!result.is_empty());
    result
}

fn options(width: f32) -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.font = FontFamily::Serif;
    options.theme.page.size = PageSize {
        name: "cell-fit",
        width_pt: width + 72.0,
        height_pt: 600.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options
}

#[test]
fn serif_table_words_fit_emitted_cell_bounds_with_positive_kerning() {
    let word = "o".repeat(107);
    let source = format!("| H |\n| --- |\n| [{word}](https://example.com/cell) |\n");
    for measure in [72.0, 81.0, 95.0] {
        for explicit_spacing in [false, true] {
            let mut options = options(measure);
            options.use_theme_spacing = explicit_spacing;
            options.theme.spacing.table_cell_padding_x_em = 0.0;
            options.theme.spacing.table_cell_padding_y_em = 0.0;
            let pdf = render_pdf(&source, &options).unwrap();
            let paint = runs(&pdf);
            let header = paint.iter().find(|run| run.text == "H").unwrap();
            // One left-aligned column on a page with equal side margins has
            // symmetric inner edges, including the real emitted table padding.
            let right = options.theme.page.size.width_pt - header.x;
            let body: Vec<_> = paint
                .iter()
                .filter(|run| run.text.chars().all(|ch| ch == 'o'))
                .collect();
            assert!(body.len() > 3, "the fixture exercises actual wrapping");
            let recovered: String = body.iter().map(|run| run.text.as_str()).collect();
            assert_eq!(recovered, word);
            // A run of identical glyphs in a fixed-width column needs the
            // same number per full row. Repairing estimated rows separately
            // used to alternate full lines and one-character continuations.
            let full_row = body[0].text.len();
            assert!(
                body[..body.len() - 1]
                    .iter()
                    .all(|run| run.text.len() == full_row),
                "uneven wrapping at measure {measure}: {:?}",
                body.iter().map(|run| run.text.len()).collect::<Vec<_>>()
            );
            assert!(body.last().unwrap().text.len() <= full_row);
            for run in body {
                assert!(
                    run.x >= header.x - 0.02 && run.x + run.width <= right + 0.03,
                    "measure={measure}, explicit_spacing={explicit_spacing}, right={right}: {run:?}"
                );
                assert_eq!(run.size, 7.5, "repair must not shrink table type");
                assert!(run.y >= 36.0 && run.page == 0);
            }
            let raw = String::from_utf8_lossy(&pdf);
            assert!(raw.contains("/URI (https://example.com/cell)"));
            assert!(raw.contains("/S /TD"));
        }
    }
}

#[test]
fn table_repairs_have_identical_streaming_and_browser_adapter_output() {
    let source = format!("| H |\n| --- |\n| {} |\n", "o".repeat(107));
    let options = options(72.0);
    let native = render_pdf(&source, &options).unwrap();
    for emission in [PdfPageEmission::Monolithic, PdfPageEmission::Chunked] {
        let other = render_pdf_emitted(
            &source,
            &options,
            PdfEmitOptions {
                emission,
                ..PdfEmitOptions::default()
            },
        )
        .unwrap();
        assert_eq!(other, native);
    }
    let browser = franken_markdown::wasm::render_pdf(
        &source,
        &franken_markdown::wasm::WasmRenderOptions {
            theme: options.theme.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(browser.bytes, native);
}

#[test]
fn a_list_toc_keeps_its_page_label_clear_of_indivisible_title_glyphs() {
    let source = "1000. [[TOC]]\n\n# W\n";
    // The hanging ordinal uses about 36pt at the normal 11pt body size.
    // At 48pt the remaining measure fits the serif W, but not W and 1.
    for (measure, stacked) in [(48.0, true), (90.0, false)] {
        let options = options(measure);
        let pdf = render_pdf(source, &options).unwrap();
        let paint = runs(&pdf);
        let title = paint
            .iter()
            .find(|run| run.text == "W" && run.size <= 11.0)
            .unwrap();
        let label = paint
            .iter()
            .find(|run| run.text == "1" && run.size == title.size)
            .unwrap();
        let heading = paint
            .iter()
            .find(|run| run.text == "W" && run.size > 11.0)
            .unwrap();
        assert_eq!(title.page, label.page);
        assert_eq!(label.text, (heading.page + 1).to_string());
        let right = options.theme.page.size.width_pt - options.theme.page.margins.right_pt;
        assert!(title.x + title.width <= right + 0.02);
        assert!((label.x + label.width - right).abs() < 0.02);
        if stacked {
            assert_eq!(title.size, 10.0);
            assert!(
                label.x < title.x + title.width,
                "the two runs need separate rows"
            );
            assert!(label.y < title.y - title.size);
        } else {
            assert_eq!(label.y, title.y);
            assert!(label.x >= title.x + title.width);
        }
    }
}
