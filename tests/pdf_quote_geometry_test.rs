//! Quotes must keep source, borders and child blocks in one physical measure.
//! Read actual PDF CIDs, widths, drawings, links and tags independently of layout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    BaselineGrid, PageMargins, PdfEmitOptions, PdfImageAsset, PdfOptions, parse_markdown,
    render_pdf, render_pdf_document_emitted,
};
use std::collections::BTreeMap;

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
    page: usize,
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
                page: page_index + 1,
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
    let mut opts = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = measure + 72.0;
    opts.theme.page.size.height_pt = 540.0;
    opts.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    opts
}

fn quote(source: &str, depth: usize) -> String {
    source
        .lines()
        .map(|line| format!("{}{line}\n", "> ".repeat(depth)))
        .collect()
}

fn text(runs: &[PaintedRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-')
        .collect()
}

fn contents(pdf: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(pdf)
        .lines()
        .filter(|line| line.contains("/Type /Page "))
        .map(|page| stream(pdf, number_after(page, "/Contents ")))
        .collect()
}

fn assert_inside(runs: &[PaintedRun], opts: &PdfOptions) {
    let left = opts.theme.page.margins.left_pt;
    let right = opts.theme.page.size.width_pt - opts.theme.page.margins.right_pt;
    let top = opts.theme.page.size.height_pt - opts.theme.page.margins.top_pt;
    let bottom = opts.theme.page.margins.bottom_pt;
    for run in runs {
        assert!(
            run.x >= left - 0.03 && run.x + run.width <= right + 0.03,
            "painted source exceeds [{left}, {right}]: {run:?}"
        );
        assert!(run.y >= bottom - 0.03 && run.y <= top + 0.03, "{run:?}");
        assert_eq!(run.scale, 1.0, "quotes must not shrink glyphs");
    }
}

/// Rounded panels contain only m/l/c coordinates; inspect their complete path,
/// including curve control points, so a compressed bar cannot put its padding
/// outside the body's left edge. The 2.5pt quote strokes are checked separately.
fn assert_quote_artifacts(pdf: &[u8], opts: &PdfOptions, expect_strokes: bool) {
    let left = opts.theme.page.margins.left_pt;
    let right = opts.theme.page.size.width_pt - opts.theme.page.margins.right_pt;
    let mut bars = 0;
    let mut panels = 0;
    let runs = painted_runs(pdf);
    for (index, content) in contents(pdf).iter().enumerate() {
        for line in content.lines() {
            if let Some((_, path)) = line.split_once("RG 2.50 w ") {
                let fields: Vec<_> = path.split_whitespace().collect();
                let x: f32 = fields[0].parse().unwrap();
                let end: f32 = fields[3].parse().unwrap();
                assert_eq!(x, end);
                assert!(
                    x - 1.25 >= left - 0.03 && x + 1.25 <= right + 0.03,
                    "{line}"
                );
                let y0: f32 = fields[1].parse().unwrap();
                let y1: f32 = fields[4].parse().unwrap();
                for run in runs.iter().filter(|run| {
                    run.page == index + 1 && run.y >= y0.min(y1) && run.y <= y0.max(y1)
                }) {
                    assert!(
                        run.x >= x + 1.25 - 0.03,
                        "quote stroke crosses foreground: {run:?}, {line}"
                    );
                }
                bars += 1;
            }
            if !(line.starts_with("q ") && line.ends_with(" f Q")) {
                continue;
            }
            let Some((_, path)) = line.split_once(" rg ") else {
                continue;
            };
            panels += 1;
            let mut operands = Vec::new();
            for field in path.split_whitespace() {
                if let Ok(value) = field.parse::<f32>() {
                    operands.push(value);
                } else if matches!(field, "m" | "l" | "c") {
                    for point in operands.as_chunks::<2>().0 {
                        assert!(
                            point[0] >= left - 0.03 && point[0] <= right + 0.03,
                            "background leaves [{left}, {right}]: {line}"
                        );
                    }
                    operands.clear();
                } else {
                    operands.clear();
                }
            }
        }
    }
    assert!(
        panels > 0,
        "a quote keeps its shaded panel even without room for a stroke"
    );
    assert_eq!(
        bars > 0,
        expect_strokes,
        "quote strokes must respect the actual foreground lane"
    );
}

fn assert_quote_chain(pdf: &[u8], depth: usize) {
    let raw = String::from_utf8_lossy(pdf);
    let lines: Vec<_> = raw.lines().collect();
    let quotes: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("/Type /StructElem /S /BlockQuote "))
        .map(|(index, line)| {
            let id: usize = lines[index - 1]
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            (id, number_after(line, "/P "))
        })
        .collect();
    assert_eq!(
        quotes.len(),
        depth,
        "decorative compression must retain every logical quote"
    );
    for pair in quotes.windows(2) {
        assert_eq!(
            pair[1].1, pair[0].0,
            "each nested tag keeps its source parent"
        );
    }
}

fn rectangle(line: &str, key: &str) -> Vec<f32> {
    line.split_once(key)
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect()
}

#[test]
fn ordinary_and_narrow_fitting_quotes_keep_their_exact_output() {
    // These fingerprints were recorded from the pre-change CLI with the same
    // source, geometry and fixed epoch, before implementing quote containment.
    let ordinary = "> An ordinary quoted paragraph with a [useful link](https://example.com/quote).\n>\n> > Nested quotation with **bold text**.\n>\n> - First item\n> - Second item\n>\n> ```rust\n> let value = 42;\n> ```\n>\n> | Name | Value |\n> | --- | --- |\n> | alpha | 10 |\n> | beta | 20 |\n\nFollowing paragraph.\n";
    let regular = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let mut fractional = regular.clone();
    fractional.theme.page.margins.left_pt = 24.115;
    let mut narrow = options(72.0);
    narrow.theme.page.size.height_pt = 288.0;
    for (source, opts, bytes, hash) in [
        (ordinary, regular, 18_144, 0x061c_40da_75fd_f715_u64),
        (
            "> > > QUOTE_END_SENTINEL\n",
            narrow,
            5_982,
            0x71f3_acb8_5020_3fe3_u64,
        ),
        (
            "> > > > > > FITTING77\n",
            fractional,
            5_877,
            0x9646_7fad_5b10_f12b_u64,
        ),
    ] {
        let pdf = render_pdf(source, &opts).unwrap();
        let actual = pdf.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        assert_eq!(pdf.len(), bytes);
        assert_eq!(actual, hash);
    }
}

#[test]
fn nested_quotes_keep_every_glyph_border_and_ancestor_on_valid_narrow_pages() {
    for (measure, depth, size) in [
        (72.0, 4, 11.0),
        (108.0, 6, 11.0),
        (468.0, 34, 11.0),
        (40.0, 6, 24.0),
    ] {
        let source = quote("WWÉ77WW", depth);
        for strict in [false, true] {
            let mut opts = options(measure);
            opts.base_font_size = Some(size);
            opts.strict_pagination = strict;
            let pdf = render_pdf(&source, &opts).unwrap();
            let runs = painted_runs(&pdf);
            assert_eq!(text(&runs), "WWÉ77WW");
            assert_inside(&runs, &opts);
            assert_quote_artifacts(&pdf, &opts, measure > 4.0 * size);
            assert_quote_chain(&pdf, depth);
            assert!(runs.iter().all(|run| run.size == size));
        }
    }
}

#[test]
fn repaired_quotes_keep_representative_words_whole_at_the_requested_size() {
    let letter = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    for (depth, opts) in [(6, options(108.0)), (34, letter)] {
        for strict in [false, true] {
            let mut opts = opts.clone();
            opts.strict_pagination = strict;
            let source = quote("QUOTED WORDS MUST STAY VISIBLE", depth);
            let pdf = render_pdf(&source, &opts).unwrap();
            let runs = painted_runs(&pdf);
            let lines: Vec<_> = runs.iter().map(|run| run.text.trim()).collect();
            assert_eq!(
                lines,
                ["QUOTED", "WORDS", "MUST", "STAY", "VISIBLE"],
                "the repaired measure must retain complete words, not a column of single glyphs"
            );
            assert!(runs.iter().all(|run| run.size == 11.0));
            assert_inside(&runs, &opts);
            assert_quote_artifacts(&pdf, &opts, true);
            assert_quote_chain(&pdf, depth);
        }
    }
}

#[test]
fn a_fitting_wide_heading_yields_only_the_decorative_stroke() {
    let source = quote("# W", 6);
    let mut opts = options(40.0);
    opts.base_font_size = Some(18.0);
    let pdf = render_pdf(&source, &opts).unwrap();
    let runs = painted_runs(&pdf);
    assert_eq!(text(&runs), "W");
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].size, 39.27,
        "the heading retains its requested hierarchy"
    );
    assert_inside(&runs, &opts);
    assert_quote_chain(&pdf, 6);
    assert_quote_artifacts(&pdf, &opts, false);
}

#[test]
fn compressed_alerts_preserve_their_label_links_and_styled_source_clusters() {
    let source = quote(
        "[!WARNING]\nALERT77 [LINK77](https://example.com/quote) **Wé77** and *e\u{0301}77*.\n",
        6,
    );
    let mut opts = options(108.0);
    opts.strict_pagination = true;
    let pdf = render_pdf(&source, &opts).unwrap();
    let runs = painted_runs(&pdf);
    assert_inside(&runs, &opts);
    assert_quote_artifacts(&pdf, &opts, true);
    assert_quote_chain(&pdf, 6);
    let actual = compact(&text(&runs));
    assert_eq!(actual, "WarningALERT77LINK77Wé77andé77.");
    assert!(runs.iter().all(|run| run.size == 11.0));
    let raw = String::from_utf8_lossy(&pdf);
    let links: Vec<_> = raw
        .lines()
        .filter(|line| line.contains("/Subtype /Link"))
        .collect();
    assert!(!links.is_empty());
    for link in links {
        assert!(link.contains("/URI (https://example.com/quote)"));
        let bounds = rectangle(link, "/Rect [");
        assert!(bounds[0] >= 35.97 && bounds[2] <= 144.03, "{bounds:?}");
    }
}

#[test]
fn child_lists_code_tables_definitions_and_figures_share_the_quote_frame() {
    let body = "# HEAD77\n\nPROSE77\n\n999999999. LIST77\n           - [x] TASK77\n\n```rust\nCODE77LONG\nTAIL77END\n```\n\nTERM77\n: DEF77\n\n| H0 | H1 |\n| --- | --- |\n| D0 | D1 |\n\n[![FIG77](figure.svg)](https://example.com/figure)\n\nINLINE77 ![BADGE77](figure.svg) END77\n\n---\n\nDONE77\n";
    let source = quote(body, 6);
    let mut opts = options(108.0);
    opts.code_line_numbers = true;
    opts.image_assets.push(PdfImageAsset::new("figure.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><rect width="100" height="80" fill="#2080c0"/></svg>"##.to_vec()));
    let doc = parse_markdown(&source);
    let original = doc.clone();
    let pdf = render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default()).unwrap();
    assert_eq!(doc, original);
    let runs = painted_runs(&pdf);
    assert_inside(&runs, &opts);
    assert_quote_artifacts(&pdf, &opts, true);
    assert_quote_chain(&pdf, 6);
    let actual = compact(&text(&runs));
    for token in [
        "HEAD77",
        "PROSE77",
        "999999999.",
        "LIST77",
        "[x]",
        "TASK77",
        "CODE77LONG",
        "TAIL77END",
        "TERM77",
        "DEF77",
        "H0",
        "H1",
        "D0",
        "D1",
        "INLINE77",
        "END77",
        "DONE77",
    ] {
        assert_eq!(
            actual.matches(token).count(),
            1,
            "{token} lost or duplicated: {actual}"
        );
    }
    let code: Vec<_> = runs.iter().filter(|run| run.font == "/F4").collect();
    assert!(!code.is_empty());
    assert!(
        code.iter().all(|run| run.size == 9.5),
        "ordinary code keeps its requested size"
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("/S /H1 "));
    assert!(raw.contains("/S /Table "));
    assert_eq!(raw.matches("/S /LI ").count(), 2);
    let figures: Vec<_> = raw
        .lines()
        .filter(|line| line.contains("/S /Figure "))
        .collect();
    assert_eq!(figures.len(), 2);
    // The shared four-em reserve exceeds this table's 32pt structural minimum.
    // Every descendant stays in the same readable 44pt frame [100,144].
    for run in &runs {
        assert!(
            run.x >= 99.97,
            "a child reclaimed through its quote border: {run:?}"
        );
    }
    for figure in figures {
        let bounds = rectangle(figure, "/BBox [");
        assert!(bounds[0] >= 99.97 && bounds[2] <= 144.03, "{bounds:?}");
    }
    assert!(raw.contains("/URI (https://example.com/figure)"));
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap()
    );
}

#[test]
fn deeply_quoted_toc_markers_fit_with_links_to_their_headings() {
    for automatic in [false, true] {
        let source = quote(
            if automatic {
                "# FIRST77\n\nBODY77\n\n## SECOND77\n\nTAIL77\n"
            } else {
                "[TOC]\n\n# FIRST77\n\nBODY77\n\n## SECOND77\n\nTAIL77\n"
            },
            6,
        );
        let mut opts = options(108.0);
        opts.toc = automatic;
        let pdf = render_pdf(&source, &opts).unwrap();
        let runs = painted_runs(&pdf);
        assert_inside(&runs, &opts);
        assert_quote_artifacts(&pdf, &opts, true);
        let actual = compact(&text(&runs));
        assert_eq!(actual.matches("FIRST77").count(), 2);
        assert_eq!(actual.matches("SECOND77").count(), 2);
        assert!(actual.contains("BODY77") && actual.contains("TAIL77"));
        assert!(String::from_utf8_lossy(&pdf).contains("/Dest ["));
    }
}

#[test]
fn footnote_and_endnote_quote_frames_use_their_final_type_sizes_in_every_emitter() {
    // The readable measure needs fewer physical rows. Keep this source large
    // enough to exercise full-size, multipage endnote fallback after reflow.
    let long = "ENDNOTE77".repeat(48);
    let source = format!(
        "CLAIM77[^small][^large]\n\n[^small]: {}SMALL77\n\n[^large]: {}{long}\n",
        "> ".repeat(6),
        "> ".repeat(6)
    );
    let mut opts = options(108.0);
    opts.theme.page.size.height_pt = 792.0;
    opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(13.125).unwrap());
    opts.strict_pagination = true;
    let doc = parse_markdown(&source);
    let pdf = render_pdf(&source, &opts).unwrap();
    let runs = painted_runs(&pdf);
    assert_inside(&runs, &opts);
    assert_quote_artifacts(&pdf, &opts, true);
    let foot: String = runs
        .iter()
        .filter(|run| run.size == 9.35)
        .map(|run| run.text.as_str())
        .collect();
    let body: String = runs
        .iter()
        .filter(|run| run.size == 11.0)
        .map(|run| run.text.as_str())
        .collect();
    assert!(
        compact(&foot).contains("SMALL77"),
        "small quote remains a page note: {foot}"
    );
    assert_eq!(compact(&body).matches("ENDNOTE77").count(), 48, "{body}");
    assert!(runs.iter().map(|run| run.page).max().unwrap() > 1);
    for emission in [PdfEmitOptions::default(), PdfEmitOptions::monolithic()] {
        assert_eq!(
            pdf,
            render_pdf_document_emitted(&doc, &opts, emission).unwrap()
        );
    }
    let browser = franken_markdown::wasm::WasmRenderOptions {
        theme: opts.theme.clone(),
        metadata_epoch_seconds: opts.metadata_epoch_seconds,
        strict_pagination: true,
        ..franken_markdown::wasm::WasmRenderOptions::default()
    };
    assert_eq!(
        pdf,
        franken_markdown::wasm::render_pdf(&source, &browser)
            .unwrap()
            .bytes
    );
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/S /BlockQuote ").count(), 12);
    assert_eq!(
        raw.lines()
            .filter(|line| line.contains("/Subtype /Link") && line.contains("/Dest ["))
            .count(),
        2
    );
}

#[test]
fn inherited_list_clamps_do_not_move_headings_or_definitions_out_of_quotes() {
    for (source, size, depth, heading_size) in [
        ("- > > # [i](https://example.com/quote)\n", 18.0, 2, 39.27),
        ("> - > # [i](https://example.com/quote)\n", 18.0, 2, 39.27),
        ("- > > > # [i](https://example.com/quote)\n", 11.0, 3, 24.0),
    ] {
        let mut opts = options(80.0);
        opts.base_font_size = Some(size);
        let pdf = render_pdf(source, &opts).unwrap();
        let runs = painted_runs(&pdf);
        assert_eq!(text(&runs), "•i");
        assert_inside(&runs, &opts);
        assert_quote_artifacts(&pdf, &opts, true);
        let heading = runs.iter().find(|run| run.text == "i").unwrap();
        assert_eq!(heading.size, heading_size);
        let content = contents(&pdf);
        assert_eq!(content.len(), 1);
        let panels: Vec<_> = content[0]
            .lines()
            .filter(|line| line.starts_with("q ") && line.ends_with(" f Q"))
            .map(|line| {
                // Every fill in this text-only fixture is a rounded quote
                // panel. Its actual m/l/c operands are coordinate pairs.
                let coordinates: Vec<f32> = line
                    .split_once(" rg ")
                    .unwrap()
                    .1
                    .split_whitespace()
                    .filter_map(|token| token.parse().ok())
                    .collect();
                let left = coordinates
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|point| point[0])
                    .fold(f32::INFINITY, f32::min);
                let right = coordinates
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|point| point[0])
                    .fold(f32::NEG_INFINITY, f32::max);
                (left, right)
            })
            .collect();
        assert_eq!(panels.len(), depth);
        assert_eq!(
            content[0].matches("RG 2.50 w ").count(),
            depth,
            "a small glyph with ample containing space retains every border"
        );
        let raw = String::from_utf8_lossy(&pdf);
        assert_eq!(raw.matches("/S /BlockQuote ").count(), depth);
        assert_eq!(raw.matches("/S /LI ").count(), 1);
        let links: Vec<_> = raw
            .lines()
            .filter(|line| line.contains("/Subtype /Link"))
            .collect();
        assert_eq!(links.len(), 1);
        let link = rectangle(links[0], "/Rect [");
        for &(left, right) in &panels {
            assert!(
                heading.x >= left - 0.03 && heading.x + heading.width <= right + 0.03,
                "heading escaped its painted panel [{left},{right}]: {heading:?}"
            );
            assert!(
                link[0] >= left - 0.03 && link[2] <= right + 0.03,
                "link escaped its painted panel [{left},{right}]: {link:?}"
            );
        }
        let mut rules = 0;
        for line in content[0].lines().filter(|line| line.ends_with(" l S")) {
            let Some((_, path)) = line.split_once(" w ") else {
                continue;
            };
            let fields: Vec<_> = path.split_whitespace().collect();
            let x0: f32 = fields[0].parse().unwrap();
            let y0: f32 = fields[1].parse().unwrap();
            let x1: f32 = fields[3].parse().unwrap();
            let y1: f32 = fields[4].parse().unwrap();
            if (y0 - y1).abs() < 0.01 && x0 != x1 {
                for &(left, right) in &panels {
                    assert!(
                        x0 >= left - 0.03 && x1 <= right + 0.03,
                        "heading rule or link underline escaped its panel: {line}"
                    );
                }
                rules += 1;
            }
        }
        assert_eq!(
            rules, 2,
            "the H1 rule and link underline are both inspected"
        );
    }

    for source in ["- > > t\n  > > : d\n", "> - > t\n>   > : d\n"] {
        let mut opts = options(92.0);
        opts.base_font_size = Some(18.0);
        let pdf = render_pdf(source, &opts).unwrap();
        let runs = painted_runs(&pdf);
        assert_eq!(text(&runs), "•td");
        assert_inside(&runs, &opts);
        assert_quote_artifacts(&pdf, &opts, true);
        let term = runs.iter().find(|run| run.text == "t").unwrap();
        let definition = runs.iter().find(|run| run.text == "d").unwrap();
        assert_eq!(term.size, 18.0);
        assert_eq!(definition.size, 18.0);
        assert!(
            (definition.x - term.x - 14.0).abs() < 0.025,
            "the actual definition gutter survives the list's body-em clamp: {term:?}, {definition:?}"
        );
    }
}
