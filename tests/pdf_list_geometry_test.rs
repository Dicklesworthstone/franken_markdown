//! List containment is checked from emitted font widths, CIDs and text matrices.
//! The expected page bounds and source order do not use the layout producer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    BaselineGrid, Block, Document, Inline, List, ListItem, PageMargins, PdfEmitOptions,
    PdfImageAsset, PdfOptions, parse_markdown, render_pdf, render_pdf_document,
    render_pdf_document_emitted,
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
    opts.theme.page.size.width_pt = measure + 72.0;
    opts.theme.page.size.height_pt = 360.0;
    opts.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    opts
}

fn text(runs: &[PaintedRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

fn assert_inside(runs: &[PaintedRun], opts: &PdfOptions) {
    let left = opts.theme.page.margins.left_pt;
    let right = opts.theme.page.size.width_pt - opts.theme.page.margins.right_pt;
    let top = opts.theme.page.size.height_pt - opts.theme.page.margins.top_pt;
    let bottom = opts.theme.page.margins.bottom_pt;
    for pair in runs.windows(2) {
        if pair[0].page == pair[1].page && (pair[0].y - pair[1].y).abs() < 0.01 {
            assert!(
                pair[0].x + pair[0].width <= pair[1].x + 0.03,
                "painted list runs overlap on one baseline: {pair:?}"
            );
        }
    }
    for run in runs {
        assert!(
            run.x >= left - 0.03 && run.x + run.width <= right + 0.03,
            "painted list source exceeds [{left}, {right}]: {run:?}"
        );
        assert!(run.y >= bottom - 0.03 && run.y <= top + 0.03, "{run:?}");
        assert_eq!(run.scale, 1.0, "containment must not shrink glyphs");
    }
}

fn nested(depth: usize, leaf: Block) -> Document {
    let mut block = leaf;
    for _ in 0..depth {
        block = Block::List(List {
            ordered: false,
            start: 1,
            tight: true,
            items: vec![ListItem {
                task: None,
                blocks: vec![block],
            }],
        });
    }
    Document {
        blocks: vec![block],
    }
}

#[test]
fn deep_lists_keep_every_marker_and_item_inside_the_page_at_original_size() {
    let source: String = (0..30)
        .map(|depth| format!("{}- LEVEL{depth:02}\n", "  ".repeat(depth)))
        .collect();
    let expected: String = (0..30).map(|depth| format!("•LEVEL{depth:02}")).collect();
    for mode in 0..3 {
        let mut opts = PdfOptions {
            optimal_pagination: mode > 0,
            ..PdfOptions::default()
        };
        if mode == 2 {
            opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
        }
        let pdf = render_pdf(&source, &opts).unwrap();
        let runs = painted_runs(&pdf);
        assert_eq!(text(&runs), expected, "every source item survives in order");
        assert_inside(&runs, &opts);
        assert!(runs.iter().all(|run| run.size == 11.0));
        let raw = String::from_utf8_lossy(&pdf);
        assert_eq!(
            raw.matches("/S /LI ").count(),
            30,
            "each source item owns one tag"
        );
    }
}

#[test]
fn wide_ordered_labels_wrap_completely_above_their_items_without_scaling() {
    let doc = Document {
        blocks: vec![Block::List(List {
            ordered: true,
            start: u64::MAX,
            tight: true,
            items: ["AA", "BB", "CC"]
                .into_iter()
                .map(|label| ListItem {
                    task: None,
                    blocks: vec![Block::Paragraph(vec![Inline::Text(label.into())])],
                })
                .collect(),
        })],
    };
    let opts = options(80.0, 24.0);
    let pdf = render_pdf_document(&doc, &opts).unwrap();
    let runs = painted_runs(&pdf);
    let expected: String = ["AA", "BB", "CC"]
        .into_iter()
        .enumerate()
        .map(|(index, label)| format!("{}.{label}", u128::from(u64::MAX) + index as u128))
        .collect();
    assert_eq!(text(&runs), expected);
    assert_inside(&runs, &opts);
    assert!(runs.iter().all(|run| run.size == 24.0));
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default()).unwrap()
    );
    assert_eq!(pdf, render_pdf_document(&doc, &opts).unwrap());
}

#[test]
fn exhausted_nesting_keeps_empty_items_and_single_vector_task_markers() {
    let tasks = Block::List(List {
        ordered: false,
        start: 1,
        tight: true,
        items: vec![
            ListItem {
                task: Some(true),
                blocks: vec![Block::Paragraph(vec![Inline::Text("DONE".into())])],
            },
            ListItem {
                task: Some(false),
                blocks: Vec::new(),
            },
            ListItem {
                task: None,
                blocks: Vec::new(),
            },
        ],
    });
    let doc = nested(8, tasks);
    let opts = options(90.0, 24.0);
    let pdf = render_pdf_document(&doc, &opts).unwrap();
    let runs = painted_runs(&pdf);
    assert_eq!(text(&runs), format!("{}[x]DONE[ ]•", "•".repeat(8)));
    assert_inside(&runs, &opts);
    assert!(runs.iter().all(|run| run.size == 24.0));
    let raw = String::from_utf8_lossy(&pdf);
    let states: usize = raw
        .lines()
        .filter(|line| line.contains("/Type /Page "))
        .map(|page| {
            stream(&pdf, number_after(page, "/Contents "))
                .matches("3 Tr")
                .count()
        })
        .sum();
    assert_eq!(states, 2, "exactly one accessible checkbox state per task");
    assert_eq!(raw.matches("/S /LI ").count(), 11);
}

#[test]
fn list_owned_quotes_headings_and_definitions_reclaim_exhausted_indentation() {
    let mut source = String::new();
    for depth in 0..6 {
        source.push_str(&format!("{}- LEVEL{depth}\n", "  ".repeat(depth)));
    }
    source.push_str("\n            > # HEAD77\n            >\n            > QUOTE77\n\n            TERM77\n            : DEF77\n\n            [LINK77](https://example.test/item)\n");
    let opts = options(90.0, 24.0);
    let doc = parse_markdown(&source);
    let pdf = render_pdf_document(&doc, &opts).unwrap();
    let runs = painted_runs(&pdf);
    assert_inside(&runs, &opts);
    let actual = text(&runs);
    for retained in ["HEAD77", "QUOTE77", "TERM77", "DEF77", "LINK77"] {
        assert!(actual.contains(retained), "missing {retained}: {actual}");
    }
    assert!(
        String::from_utf8_lossy(&pdf)
            .matches("/Subtype /Link")
            .count()
            > 0
    );
    assert!(String::from_utf8_lossy(&pdf).contains("https://example.test/item"));
}

#[test]
fn stacked_markers_keep_linked_figures_and_image_galleries_as_complete_blocks() {
    let image = Inline::Image {
        dest: "portrait.svg".into(),
        alt: "List illustration".into(),
        title: None,
    };
    let doc = nested(
        8,
        Block::List(List {
            ordered: false,
            start: 1,
            tight: false,
            items: vec![
                ListItem {
                    task: None,
                    blocks: vec![
                        Block::Paragraph(vec![Inline::Link {
                            dest: "https://example.test/figure".into(),
                            title: None,
                            content: vec![image.clone()],
                        }]),
                        Block::Paragraph(vec![Inline::Text("AFTER77".into())]),
                    ],
                },
                ListItem {
                    task: None,
                    blocks: vec![Block::Paragraph(vec![
                        image.clone(),
                        Inline::SoftBreak,
                        image,
                    ])],
                },
            ],
        }),
    );
    let mut opts = options(90.0, 24.0);
    opts.image_assets.push(PdfImageAsset::new("portrait.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="400"><rect width="100" height="400" fill="#2080c0"/></svg>"##.to_vec()));
    let original = doc.clone();
    let pdf = render_pdf_document(&doc, &opts).unwrap();
    assert_eq!(doc, original);
    let runs = painted_runs(&pdf);
    assert_inside(&runs, &opts);
    assert_eq!(text(&runs), format!("{}•AFTER77•", "•".repeat(8)));
    assert!(runs.iter().all(|run| run.size == 24.0));
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/S /Figure ").count(), 3);
    assert_eq!(raw.matches("/Alt (List illustration)").count(), 3);
    assert!(raw.contains("https://example.test/figure"));
    for figure in raw.split("/S /Figure ").skip(1) {
        let object = figure.split("endobj").next().unwrap();
        let bbox: Vec<f32> = object
            .split_once("/BBox [")
            .unwrap()
            .1
            .split_once(']')
            .unwrap()
            .0
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert!(bbox[0] >= 35.97 && bbox[2] <= 126.03, "{bbox:?}");
        assert!(bbox[1] >= 35.97 && bbox[3] <= 324.03, "{bbox:?}");
        assert!(
            bbox[3] - bbox[1] > 100.0,
            "a gallery picture must not become a text icon: {bbox:?}"
        );
    }
}
