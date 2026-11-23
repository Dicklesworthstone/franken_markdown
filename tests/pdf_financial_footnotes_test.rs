//! Financial annotations retain measured decimal anchors and real note navigation.
//! The oracle decodes painted glyphs, CID widths and TJ positions from the PDF.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use std::collections::BTreeMap;

use franken_markdown::{
    BaselineGrid, FontFamily, PageMargins, PageSize, PdfASettings, PdfEmitOptions, PdfOptions,
    PdfPageEmission, render_pdf, render_pdf_emitted, render_pdf_pdfa,
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
struct Glyph {
    page: usize,
    text: String,
    x: f32,
    y: f32,
    width: f32,
    size: f32,
}

fn glyphs(pdf: &[u8]) -> Vec<Glyph> {
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
                    // Validate that no unhandled show operator or string is skipped.
                    let _ = pdf_content::glyph_ids(&operation);
                    for operand in &operation.operands {
                        if let Some(hex) = operand
                            .strip_prefix('<')
                            .and_then(|text| text.strip_suffix('>'))
                        {
                            for chunk in hex.as_bytes().as_chunks::<4>().0 {
                                let cid =
                                    u16::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16)
                                        .unwrap();
                                let width = font.widths[usize::from(cid)] * size * scale / 1000.0;
                                if !invisible {
                                    result.push(Glyph {
                                        page: page_index,
                                        text: font.unicode[&cid].clone(),
                                        x,
                                        y,
                                        width,
                                        size,
                                    });
                                }
                                x += width;
                            }
                        } else if let Ok(adjustment) = operand.parse::<f32>() {
                            x -= adjustment * size * scale / 1000.0;
                        }
                    }
                }
                "Td" | "TD" | "T*" => panic!("extend decoder for relative text motion"),
                _ => {}
            }
        }
    }
    assert!(!result.is_empty());
    result
}

fn row<'a>(paint: &'a [Glyph], label: &str) -> Vec<&'a Glyph> {
    let label = paint.iter().find(|glyph| glyph.text == label).unwrap();
    paint
        .iter()
        .filter(|glyph| glyph.page == label.page && glyph.y == label.y)
        .collect()
}

fn row_text(row: &[&Glyph]) -> String {
    row.iter().map(|glyph| glyph.text.as_str()).collect()
}

fn dot(row: &[&Glyph]) -> f32 {
    row.iter().find(|glyph| glyph.text == ".").unwrap().x
}

fn options() -> PdfOptions {
    PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        footnote_backlinks: true,
        ..PdfOptions::default()
    }
}

const SOURCE: &str = "| Item | Value |\n| --- | ---. |\n\
    | A | 123.4 |\n\
    | B | **12.34**[^audit] |\n\
    | C | [7.125](https://example.com/amount)[^audit][^basis] |\n\
    | D | (25.50)[^basis] |\n\
    | E | 9[^audit] |\n\
    | F | [-3.25[^basis]](https://example.com/linked-note) |\n\
    | | === |\n\n\
    [^audit]: Audited amount\n\n[^basis]: Measurement basis\n";

#[test]
fn annotated_amounts_align_painted_decimals_and_preserve_note_links() {
    for font in [FontFamily::Sans, FontFamily::Serif] {
        for grid in [false, true] {
            let mut opts = options();
            opts.theme.font = font;
            opts.strict_pagination = true;
            opts.theme.page.baseline_grid = grid.then(|| BaselineGrid::from_points(13.0).unwrap());
            let pdf = render_pdf(SOURCE, &opts).unwrap();
            let paint = glyphs(&pdf);
            let expected = dot(&row(&paint, "A"));
            for (label, text) in [
                ("A", "A123.4"),
                ("B", "B12.34¹"),
                ("C", "C7.125¹²"),
                ("D", "D(25.50)²"),
                ("F", "F-3.25²"),
            ] {
                let data = row(&paint, label);
                assert_eq!(row_text(&data), text);
                assert!(
                    (dot(&data) - expected).abs() < 0.03,
                    "{font:?}, grid={grid}, {label}: decimal {} vs {expected}",
                    dot(&data)
                );
                assert!(
                    data.iter().all(|glyph| glyph.size == 10.0),
                    "no font-size change to force alignment"
                );
            }
            let integer = row(&paint, "E");
            assert_eq!(row_text(&integer), "E9¹");
            let nine = integer.iter().find(|glyph| glyph.text == "9").unwrap();
            assert!(
                (nine.x + nine.width - expected).abs() < 0.03,
                "integer virtual decimal must use its numeric edge"
            );
            let raw = String::from_utf8_lossy(&pdf);
            assert!(raw.contains("/URI (https://example.com/amount)"));
            assert!(raw.contains("/URI (https://example.com/linked-note)"));
            for number in [1, 2] {
                assert!(raw.contains(&format!("/ID (fmd-note-{number})")));
                assert!(raw.contains("/S /Reference "), "references stay structured");
                assert!(
                    raw.contains(&format!("(fmd-fn-{number})")),
                    "notes retain named destinations"
                );
            }
            assert!(
                paint
                    .iter()
                    .all(|glyph| glyph.x >= 71.97 && glyph.x + glyph.width <= 540.03)
            );
        }
    }
}

#[test]
fn decimal_admission_never_reclassifies_authored_markers_or_mid_number_notes() {
    let source = "| Amount |\n| ---. |\n\
        | 123.4 |\n\
        | 12.34[^audit] |\n\
        | 12.34¹ |\n\
        | 12.34[¹](#fmd-fn-1) |\n\
        | 12[^audit].34 |\n\
        | 12.34[^missing] |\n\n[^audit]: Audit\n";
    let opts = options();
    let pdf = render_pdf(source, &opts).unwrap();
    let ordinary = render_pdf(&source.replace("---.", "---:"), &opts).unwrap();
    let actual = glyphs(&pdf);
    let right = glyphs(&ordinary);
    // The third through sixth data rows must retain ordinary right alignment.
    // Match by actual physical baseline: note/source text and row count stay exact.
    let mut lines: Vec<_> = actual
        .iter()
        .filter(|glyph| glyph.text == ".")
        .map(|glyph| glyph.y)
        .collect();
    lines.dedup();
    assert_eq!(lines.len(), 6);
    for y in &lines[2..] {
        let a: Vec<_> = actual.iter().filter(|glyph| glyph.y == *y).collect();
        let b: Vec<_> = right.iter().filter(|glyph| glyph.y == *y).collect();
        assert_eq!(row_text(&a), row_text(&b));
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(&b) {
            assert!(
                (a.x - b.x).abs() < 0.001,
                "authored/invalid source must retain its position: {a:?} vs {b:?}"
            );
        }
    }
}

#[test]
fn annotated_financial_tables_match_pdfa_emitters_and_browser_adapter() {
    let opts = options();
    let native = render_pdf(SOURCE, &opts).unwrap();
    for emission in [PdfPageEmission::Monolithic, PdfPageEmission::Chunked] {
        assert_eq!(
            native,
            render_pdf_emitted(
                SOURCE,
                &opts,
                PdfEmitOptions {
                    emission,
                    ..Default::default()
                }
            )
            .unwrap()
        );
    }
    let archival = render_pdf_pdfa(SOURCE, &opts, PdfASettings::a2b()).unwrap();
    pdf_content::assert_same_paint(&archival, &native);
    let browser = franken_markdown::wasm::render_pdf(
        SOURCE,
        &franken_markdown::wasm::WasmRenderOptions {
            metadata_epoch_seconds: opts.metadata_epoch_seconds,
            footnote_backlinks: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(browser.bytes, native);
}

#[test]
fn annotated_columns_keep_one_anchor_across_pages_and_reserve_the_full_suffix() {
    let mut source = String::from("| Item | Value |\n| --- | ---. |\n");
    for index in 0..24 {
        let value = if index % 2 == 0 {
            format!("**{}.{}**", 1000 + index, "5".repeat(index % 4 + 1))
        } else {
            format!("{}.{}", index, "6".repeat(index % 4 + 1))
        };
        source.push_str(&format!("| Row{index} | {value}[^audit][^basis] |\n"));
    }
    source.push_str("| | === |\n\n[^audit]: Audited amount\n\n[^basis]: Measurement basis\n");
    let mut opts = options();
    opts.strict_pagination = true;
    opts.theme.page.size = PageSize {
        name: "financial-notes",
        width_pt: 270.0,
        height_pt: 220.0,
    };
    opts.theme.page.margins = PageMargins {
        top_pt: 30.0,
        right_pt: 30.0,
        bottom_pt: 30.0,
        left_pt: 30.0,
    };
    let pdf = render_pdf(&source, &opts).unwrap();
    let paint = glyphs(&pdf);
    assert!(paint.iter().any(|glyph| glyph.page > 1));
    let dots: Vec<_> = paint.iter().filter(|glyph| glyph.text == ".").collect();
    assert_eq!(
        dots.len(),
        24,
        "every amount survives wrapping and pagination"
    );
    for dot in dots {
        assert!((dot.x - paint.iter().find(|glyph| glyph.text == ".").unwrap().x).abs() < 0.03);
        let data: Vec<_> = paint
            .iter()
            .filter(|glyph| glyph.page == dot.page && glyph.y == dot.y)
            .collect();
        assert!(row_text(&data).ends_with("¹²"));
        assert!(data.iter().all(|glyph| glyph.size == 10.0));
        assert!(
            data.iter()
                .all(|glyph| glyph.x >= 29.97 && glyph.x + glyph.width <= 240.03),
            "numeric and citation widths stay inside their real row: {data:?}"
        );
    }
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches(" /S /TD ").count(), 48);
    assert_eq!(raw.matches(" /S /Note ").count(), 2);
}
