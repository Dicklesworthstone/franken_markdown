//! Authored discretionary breaks are tested through emitted PDF glyphs and geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::{
    PageMargins, PageSize, PdfOptions, parse_markdown, render_pdf, render_warnings,
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
                .as_chunks::<4>()
                .0
                .iter()
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
    text: String,
    x: f32,
    y: f32,
    width: f32,
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
            .as_chunks::<4>()
            .0
            .iter()
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
                    for code in hex.as_bytes().as_chunks::<4>().0 {
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
                text,
                x: head[tm - 2].parse().unwrap(),
                y: head[tm - 1].parse().unwrap(),
                width: width * size * scale / 1000.0,
            });
        }
    }
    assert!(!runs.is_empty(), "the source must paint actual text");
    runs
}

fn options(measure: f32) -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize {
        name: "discretionary-test",
        width_pt: measure + 72.0,
        height_pt: 300.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options
}

fn lines(pdf: &[u8]) -> Vec<String> {
    let mut rows: Vec<(f32, String)> = Vec::new();
    for run in painted_runs(pdf) {
        if let Some((y, text)) = rows.last_mut()
            && (*y - run.y).abs() < 0.01
        {
            text.push_str(&run.text);
        } else {
            rows.push((run.y, run.text));
        }
    }
    rows.into_iter().map(|(_, text)| text).collect()
}

#[test]
fn unused_hints_have_no_glyph_advance_or_missing_glyph_warning() {
    for (hinted, visible) in [
        ("co\u{00ad}operate", "cooperate"),
        ("f\u{00ad}i", "fi"),
        ("A\u{00ad}V", "AV"),
        ("of\u{00ad}fice", "office"),
        ("of\u{200b}fice", "office"),
        ("alpha\u{200b}beta", "alphabeta"),
        ("inter&shy;nationalization", "internationalization"),
        (
            "customer&#x200B;Account&#x200B;Identifier",
            "customerAccountIdentifier",
        ),
        ("e\u{00ad}\u{0301}lite", "e\u{0301}lite"),
        ("e\u{200b}\u{00ad}\u{0301}lite", "e\u{0301}lite"),
        ("**al\u{00ad}pha** *be\u{200b}ta*", "**alpha** *beta*"),
        (
            "[co\u{00ad}operate](https://example.com/)",
            "[cooperate](https://example.com/)",
        ),
        ("> alpha\u{200b}beta", "> alphabeta"),
        ("- co\u{00ad}operate", "- cooperate"),
    ] {
        let options = PdfOptions::default();
        let hinted_pdf = render_pdf(hinted, &options).unwrap();
        let visible_pdf = render_pdf(visible, &options).unwrap();
        assert_eq!(hinted_pdf, visible_pdf, "unbroken {hinted:?}");
        assert!(render_warnings(&parse_markdown(hinted), &options).is_empty());
    }
}

#[test]
fn authored_breaks_are_used_without_missing_boxes_or_invented_spaces() {
    let options = options(72.0);
    for (source, expected) in [
        ("WWWW\u{00ad}WWWW", vec!["WWWW-", "WWWW"]),
        ("WWWW\u{200b}WWWW", vec!["WWWW", "WWWW"]),
        ("WWWW\u{200b}\u{00ad}\u{200b}WWWW", vec!["WWWW-", "WWWW"]),
    ] {
        let pdf = render_pdf(source, &options).unwrap();
        assert_eq!(lines(&pdf), expected, "{source:?}");
        for run in painted_runs(&pdf) {
            assert!(run.x >= 35.99);
            assert!(run.x + run.width <= 108.01, "overflow: {run:?}");
        }
        assert!(render_warnings(&parse_markdown(source), &options).is_empty());
    }
}

#[test]
fn tight_unbroken_words_keep_cross_hint_ligatures_and_kerning() {
    for (hinted, visible) in [
        ("WWWWf\u{00ad}i", "WWWWfi"),
        ("WWWWA\u{00ad}V", "WWWWAV"),
        ("WWof\u{200b}fice", "WWoffice"),
        ("WWWWf\u{00ad}f\u{200b}i", "WWWWffi"),
    ] {
        let wide = render_pdf(visible, &PdfOptions::default()).unwrap();
        let width: f32 = painted_runs(&wide).iter().map(|run| run.width).sum();
        assert!(
            width > 40.0,
            "exercise the real measure above the page minimum"
        );
        let options = options(width + 0.02);
        assert!(
            render_pdf(hinted, &options).unwrap() == render_pdf(visible, &options).unwrap(),
            "unused hint must not change the shaped fit of {hinted:?} at {width}pt"
        );
    }
}

#[test]
fn chosen_hyphens_fit_without_becoming_standalone_lines() {
    for (source, size, measure) in [
        ("WWW\u{00ad}MMM", 24.0, 72.0),
        ("WWWWWWW\u{00ad}MMMMMMM", 11.0, 72.0),
        ("> WWWWW\u{00ad}MMMMM", 12.0, 72.0),
        ("- WWWWW\u{00ad}MMMMM", 12.0, 72.0),
        ("| Word |\n| --- |\n| WWWWWWW\u{00ad}MMMMMMM |", 11.0, 72.0),
        ("$\\text{WWW}$\u{00ad}MMM", 24.0, 72.0),
    ] {
        let mut options = options(measure);
        options.base_font_size = Some(size);
        let pdf = render_pdf(source, &options).unwrap();
        let rows = lines(&pdf);
        assert!(rows.iter().all(|row| row != "-"), "{source:?}: {rows:?}");
        for pair in rows.windows(2) {
            assert!(
                !(pair[0].ends_with('W') && pair[1].starts_with('M')),
                "a line ending at the authored boundary needs its hyphen: {source:?}: {rows:?}"
            );
        }
        // Math has its own formula extraction contract and glyph resources.
        // Ordinary source text must retain every character across all wraps.
        if !source.starts_with('$') {
            assert_eq!(
                rows.iter()
                    .map(|row| row.matches('W').count())
                    .sum::<usize>(),
                source.matches('W').count(),
                "{source:?}: {rows:?}"
            );
        }
        assert_eq!(
            rows.iter()
                .map(|row| row.matches('M').count())
                .sum::<usize>(),
            source.matches('M').count(),
            "{source:?}: {rows:?}"
        );
        for run in painted_runs(&pdf) {
            assert!(
                run.x + run.width <= 36.0 + measure + 0.02,
                "{source:?}: {run:?}"
            );
        }
    }
}

#[test]
fn boundary_and_consecutive_hints_never_create_empty_content() {
    let options = PdfOptions::default();
    for source in [
        "\u{00ad}\u{200b}alpha\u{200b}\u{00ad}\u{200b}beta\u{00ad}\u{200b}",
        "al\u{00ad}\u{00ad}pha\u{200b}\u{200b}beta",
    ] {
        assert_eq!(
            render_pdf(source, &options).unwrap(),
            render_pdf("alphabeta", &options).unwrap()
        );
    }
    assert_eq!(
        render_pdf("\u{00ad}\u{200b}", &options).unwrap(),
        render_pdf("", &options).unwrap()
    );
}

#[test]
fn literal_code_keeps_its_source_controls() {
    let options = PdfOptions::default();
    let source = "`co\u{00ad}operate alpha\u{200b}beta`";
    let pdf = render_pdf(source, &options).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("<00AD>"));
    assert!(
        render_warnings(&parse_markdown(source), &options)
            .iter()
            .any(|warning| warning.code() == "missing_glyphs")
    );
    assert_ne!(pdf, render_pdf("`cooperate alphabeta`", &options).unwrap());
}

#[test]
fn table_cells_use_authored_breaks_and_suppress_unused_hints() {
    let options = options(72.0);
    let source = "| Word |\n| --- |\n| WWWW\u{00ad}WWWW |\n| WWWW\u{200b}WWWW |\n";
    let pdf = render_pdf(source, &options).unwrap();
    let rows = lines(&pdf);
    assert!(rows.iter().any(|line| line == "WWWW-"), "{rows:?}");
    assert_eq!(
        rows.iter().filter(|line| line.as_str() == "WWWW").count(),
        3,
        "{rows:?}"
    );
    for run in painted_runs(&pdf) {
        assert!(run.x >= 35.99);
        assert!(run.x + run.width <= 108.01, "overflow: {run:?}");
    }
    assert!(render_warnings(&parse_markdown(source), &options).is_empty());
}

#[test]
fn hints_do_not_turn_combining_marks_or_leading_spaces_into_breaks() {
    let options = options(72.0);
    for (source, visible) in [
        (
            "| Word |\n| --- |\n| e\u{00ad}\u{0301}lite |",
            "| Word |\n| --- |\n| e\u{0301}lite |",
        ),
        (
            "| Word |\n| --- |\n| WWWW \u{00ad}WWWW |",
            "| Word |\n| --- |\n| WWWW WWWW |",
        ),
        (
            "| alpha\u{00ad} | beta |\n| --- | --- |\n| WWWW | WWWW |",
            "| alpha | beta |\n| --- | --- |\n| WWWW | WWWW |",
        ),
    ] {
        assert_eq!(
            render_pdf(source, &options).unwrap(),
            render_pdf(visible, &options).unwrap()
        );
    }
}

#[test]
fn hyphens_next_to_formulas_are_text_not_new_formula_objects() {
    let options = options(72.0);
    let pdf = render_pdf("$\\text{WWWW}$\u{00ad}WWWW", &options).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&pdf)
            .matches("/S /Formula ")
            .count(),
        1
    );
    assert!(
        String::from_utf8_lossy(&pdf).contains("<002D>"),
        "chosen hyphen is in the text font"
    );
}
