//! Fenced-code source and diagram rows must survive narrow-page layout.
//! Decode emitted font widths, CIDs and text matrices independently of layout.
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

fn options(measure: f32) -> PdfOptions {
    let mut opts = PdfOptions::default();
    opts.theme.page.size = PageSize {
        name: "fenced-code-test",
        width_pt: measure + 48.0,
        height_pt: 300.0,
    };
    opts.theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    opts
}

fn assert_inside(runs: &[PaintedRun], opts: &PdfOptions) {
    let right = opts.theme.page.size.width_pt - 24.0;
    let top = opts.theme.page.size.height_pt - 24.0;
    for run in runs {
        assert!(
            run.x >= 24.0 - 0.03 && run.x + run.width <= right + 0.03,
            "code exceeds body right {right}: {run:?}",
        );
        assert!(run.y >= 24.0 - 0.03 && run.y <= top + 0.03, "{run:?}");
        assert_eq!(run.scale, 1.0, "a fitted diagram keeps glyph proportions");
        assert!(run.size > 0.0, "a font must not round to zero in the PDF");
    }
}

fn physical_rows(runs: &[PaintedRun]) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut previous_y = None;
    for run in runs {
        if previous_y != Some(run.y) {
            rows.push(String::new());
            previous_y = Some(run.y);
        }
        rows.last_mut().unwrap().push_str(&run.text);
    }
    rows
}

fn box_rows() -> Vec<String> {
    vec![
        format!("+BEGIN{}END+", "-".repeat(70)),
        format!("|{}|", " ".repeat(77)),
        format!("+BOTTOM{}END+", "-".repeat(69)),
    ]
}

#[test]
fn preserved_diagrams_fit_without_losing_rows_or_right_hand_source() {
    let rows = box_rows();
    let opts = options(80.0);
    for lang in ["ascii", "text", ""] {
        let source = format!("```{lang}\n{}\n```\n", rows.join("\n"));
        let pdf = render_pdf(&source, &opts).unwrap();
        let runs = painted_runs(&pdf);
        assert_eq!(physical_rows(&runs), rows);
        assert_inside(&runs, &opts);
        let size = runs[0].size;
        assert!(size < 6.0, "the old floor cannot fit this complete row");
        assert!(runs.iter().all(|run| run.size == size));
        assert_eq!(
            String::from_utf8_lossy(&pdf)
                .matches("/Type /Page ")
                .count(),
            1
        );
    }
}

#[test]
fn numbered_diagram_rows_keep_the_gutter_source_and_emitter_parity() {
    let rows: Vec<_> = (0..12)
        .map(|i| format!("ROW{i:02}|{}|END", "-".repeat(70)))
        .collect();
    let source = format!("```ascii\n{}\n```\n", rows.join("\n"));
    let mut opts = options(80.0);
    opts.code_line_numbers = true;
    let pdf = render_pdf(&source, &opts).unwrap();
    let runs = painted_runs(&pdf);
    let expected: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| format!("{:>2}{row}", i + 1))
        .collect();
    assert_eq!(physical_rows(&runs), expected);
    assert_inside(&runs, &opts);
    assert!(
        runs.iter()
            .all(|run| run.size == runs[0].size && run.size < 6.0)
    );
    let doc = parse_markdown(&source);
    assert_eq!(
        pdf,
        render_pdf_document_emitted(
            &doc,
            &opts,
            PdfEmitOptions {
                emission: franken_markdown::PdfPageEmission::Monolithic,
                ..PdfEmitOptions::default()
            }
        )
        .unwrap()
    );
    let browser = franken_markdown::wasm::WasmRenderOptions {
        theme: opts.theme.clone(),
        code_line_numbers: true,
        ..franken_markdown::wasm::WasmRenderOptions::default()
    };
    assert_eq!(
        pdf,
        franken_markdown::wasm::render_pdf(&source, &browser)
            .unwrap()
            .bytes
    );
    assert_eq!(pdf, render_pdf(&source, &opts).unwrap());
}

#[test]
fn nested_numbered_code_wraps_at_its_requested_size_inside_the_actual_body() {
    let first = format!("{}TAIL", "W".repeat(30));
    let second = format!("{}END", "X".repeat(30));
    let source = format!("> > - ```rust\n> >   {first}\n> >   {second}\n> >   ```\n");
    let mut opts = options(80.0);
    opts.code_line_numbers = true;
    opts.strict_pagination = true;
    let pdf = render_pdf(&source, &opts).unwrap();
    let runs = painted_runs(&pdf);
    assert_inside(&runs, &opts);
    let labels: Vec<_> = runs
        .iter()
        .filter_map(|run| run.text.trim().parse::<usize>().ok())
        .collect();
    assert_eq!(
        labels,
        [1, 2],
        "continuation lines do not invent source line numbers"
    );
    let text: String = runs
        .iter()
        .filter(|run| run.text.trim().parse::<usize>().is_err())
        .flat_map(|run| run.text.chars())
        .filter(|&ch| ch != '•')
        .collect();
    assert_eq!(text, format!("{first}{second}"));
    assert!(
        runs.iter()
            .filter(|run| run.font == "/F4")
            .all(|run| run.size == 9.5)
    );
    assert!(
        String::from_utf8_lossy(&pdf)
            .matches("/Type /Page ")
            .count()
            > 1
    );
}

#[test]
fn fractional_code_size_and_decomposed_clusters_use_the_painted_width() {
    let source = format!("```unknown\n{}\n```\n", "e\u{0301}".repeat(80));
    let mut opts = options(44.0);
    opts.base_font_size = Some(11.0 * 1.125);
    let pdf = render_pdf(&source, &opts).unwrap();
    let runs = painted_runs(&pdf);
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        "é".repeat(80)
    );
    assert_inside(&runs, &opts);
    assert!(runs.iter().all(|run| run.size == 10.69));
}

#[test]
fn unrepresentably_wide_diagram_rows_fall_back_to_complete_legible_wrapping() {
    let text = "W".repeat(12_000);
    let source = format!("```ascii\n{text}\n```\n");
    let mut opts = options(44.0);
    opts.theme.page.size.height_pt = 6000.0;
    let pdf = render_pdf(&source, &opts).unwrap();
    let runs = painted_runs(&pdf);
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        text
    );
    assert_inside(&runs, &opts);
    assert!(runs.iter().all(|run| run.size == 9.5));
}

#[test]
fn ordinary_fitting_code_and_diagrams_keep_their_requested_size() {
    let opts = options(468.0);
    for source in [
        "```rust\nlet answer = 42;\n```\n",
        "```ascii\n+------+\n| A->B |\n+------+\n```\n",
    ] {
        let runs = painted_runs(&render_pdf(source, &opts).unwrap());
        assert_inside(&runs, &opts);
        assert!(runs.iter().all(|run| run.size == 9.5));
    }
}
