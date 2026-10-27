//! Inline-code source spelling and fixed spacing survive actual PDF emission.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::{FontFamily, PageMargins, PageSize, PdfOptions, render_pdf};

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

fn mono_text(runs: &[PaintedRun]) -> String {
    runs.iter()
        .filter(|run| run.font == "/F4")
        .map(|run| run.text.as_str())
        .collect()
}

#[test]
fn emitted_inline_code_preserves_repeated_edge_and_only_spaces() {
    for (source, expected) in [
        ("before `a  b` after", "a  b"),
        ("before `  a   b  ` after", " a   b "),
        ("`   `", "   "),
        ("`a\nb`", "a b"),
    ] {
        let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
        assert_eq!(mono_text(&painted_runs(&pdf)), expected, "{source}");
    }
}

#[test]
fn literal_spaces_survive_headings_lists_quotes_and_table_cells() {
    for source in [
        "# `  a   b  `",
        "- `  a   b  `",
        "> `  a   b  `",
        "| Sample |\n| --- |\n| `  a   b  ` |",
        "~~[`  a   b  `](https://example.com/code)~~",
    ] {
        let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
        assert_eq!(mono_text(&painted_runs(&pdf)), " a   b ", "{source}");
    }
}

#[test]
fn identifiers_wrap_without_added_hyphens_or_clipped_glyphs() {
    for size in [11.0, 24.0] {
        for measure in [44.0, 132.0] {
            let opts = options(measure, size);
            for code in [
                "characteristically",
                "abcdefghijk",
                "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
                "path/to_some_resource?key=abcdefghijk&other=0123456789",
                "  leading  internal    and trailing  ",
            ] {
                let source = format!("` {code} `");
                let runs = painted_runs(&render_pdf(&source, &opts).unwrap());
                assert_eq!(mono_text(&runs), code, "size={size}, measure={measure}");
                for run in &runs {
                    assert!(
                        run.x + run.width <= 24.0 + measure + 0.03,
                        "literal glyphs run off the page: {run:?}"
                    );
                }
            }
        }
    }
    let runs = painted_runs(
        &render_pdf("before `characteristically` after", &options(132.0, 11.0)).unwrap(),
    );
    assert_eq!(mono_text(&runs), "characteristically");
}

#[test]
fn code_spaces_keep_fixed_width_inside_justified_prose() {
    let mut opts = options(132.0, 11.0);
    opts.microtype = franken_markdown::layout::MicrotypeOptions::CONSERVATIVE;
    let source = "This is `a  b c` followed by ordinary words that need enough room to span more than one line of text.";
    let runs = painted_runs(&render_pdf(source, &opts).unwrap());
    assert!(runs.windows(2).any(|pair| pair[0].y != pair[1].y));
    let code: Vec<_> = runs.iter().filter(|run| run.font == "/F4").collect();
    assert_eq!(mono_text(&runs), "a  b c");
    for run in &code {
        assert_eq!(run.scale, 1.0, "literal code must not expand");
    }
    for pair in code.windows(2).filter(|pair| pair[0].y == pair[1].y) {
        assert!(
            (pair[1].x - pair[0].x - pair[0].width).abs() <= 0.03,
            "justification opened a gap inside code: {pair:?}"
        );
    }
}

#[test]
fn measured_code_breaks_follow_host_font_widths_and_symbol_fallback() {
    let code = "officestaffaffiliationwwwwwwwwwwwwww";
    let mut opts = options(44.0, 24.0);
    opts.font_assets.mono_regular =
        Some(fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec());
    let runs = painted_runs(&render_pdf(&format!("`{code}`"), &opts).unwrap());
    assert_eq!(mono_text(&runs), code);
    for run in &runs {
        assert!(run.x + run.width <= 68.03, "host font overflows: {run:?}");
    }

    opts.font_assets.mono_regular = Some(
        fonts::load_mono(FontStyle::Regular)
            .unwrap()
            .subset(&[' '])
            .unwrap(),
    );
    let code = "∫∫∫∫∫∫∫∫∫∫∫";
    let runs = painted_runs(&render_pdf(&format!("`{code}`"), &opts).unwrap());
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        code
    );
    assert!(
        runs.iter().any(|run| run.font != "/F4"),
        "fixture must use fallback"
    );
    for run in &runs {
        assert!(
            run.x + run.width <= 68.03,
            "fallback glyphs overflow: {run:?}"
        );
    }
}

#[test]
fn inline_code_pdf_is_deterministic_and_stays_within_the_list_measure() {
    let opts = options(60.0, 24.0);
    let source = "- `abcdefghijk  ABCDEFGHIJKLMNOPQRSTUVWXYZ`";
    let first = render_pdf(source, &opts).unwrap();
    assert_eq!(first, render_pdf(source, &opts).unwrap());
    let runs = painted_runs(&first);
    assert_eq!(mono_text(&runs), "abcdefghijk  ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    for run in &runs {
        assert!(
            run.x + run.width <= 84.03,
            "indented code overflows: {run:?}"
        );
    }
}

#[test]
fn unsupported_inline_math_preserves_its_literal_source() {
    let source = r"\fmnNotACommand{a  b}";
    for (measure, delimiter) in [(44.0, "$"), (252.0, "$"), (44.0, "$$"), (252.0, "$$")] {
        let opts = options(measure, 11.0);
        let markdown = format!("Before {delimiter}{source}{delimiter} after.");
        let runs = painted_runs(&render_pdf(&markdown, &opts).unwrap());
        assert_eq!(mono_text(&runs), source);
        for run in &runs {
            assert!(
                run.x + run.width <= 24.0 + measure + 0.03,
                "literal fallback overflows: {run:?}"
            );
        }
        let warnings =
            franken_markdown::render_warnings(&franken_markdown::parse_markdown(&markdown), &opts);
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            franken_markdown::RenderWarning::MathFallback { source: retained, .. }
                if retained == source
        )));
    }
}
