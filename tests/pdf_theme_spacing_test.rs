//! Shared spacing changes measured PDF rows, not just their painted offsets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    BaselineGrid, Block, Inline, PageMargins, PdfEmitOptions, PdfOptions, RenderError,
    parse_markdown, render_pdf, render_pdf_document_emitted, render_pdf_reported,
};
use std::collections::BTreeMap;

struct Objects<'a> {
    bytes: &'a [u8],
    offsets: Vec<usize>,
    xref: usize,
}

impl<'a> Objects<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let marker = b"startxref\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|s| s == marker)
            .unwrap();
        let xref = number(std::str::from_utf8(&bytes[at..]).unwrap(), "startxref\n");
        let mut rows = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(rows.next(), Some("xref"));
        let section: Vec<_> = rows.next().unwrap().split_whitespace().collect();
        assert_eq!(section[0], "0");
        let count: usize = section[1].parse().unwrap();
        let offsets = rows
            .take(count)
            .map(|row| row.split_whitespace().next().unwrap().parse().unwrap())
            .collect();
        Self {
            bytes,
            offsets,
            xref,
        }
    }

    fn object(&self, id: usize) -> &'a [u8] {
        let start = self.offsets[id];
        let end = self
            .offsets
            .iter()
            .copied()
            .filter(|&at| at > start)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        assert!(self.bytes[start..end].starts_with(header.as_bytes()));
        &self.bytes[start + header.len()..end]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let object = self.object(id);
        let end = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap_or(object.len());
        std::str::from_utf8(&object[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let object = self.object(id);
        let start = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap()
            + 8;
        let dictionary = self.dictionary(id);
        let data = &object[start..start + number(dictionary, "/Length ")];
        let data = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(data, 16 * 1024 * 1024).unwrap()
        } else {
            assert!(!dictionary.contains("/Filter"));
            data.to_vec()
        };
        String::from_utf8(data).unwrap()
    }

    fn pages(&self) -> Vec<usize> {
        let trailer = std::str::from_utf8(&self.bytes[self.xref..]).unwrap();
        let catalog = self.dictionary(number(trailer, "/Root "));
        let pages = self.dictionary(number(catalog, "/Pages "));
        let kids: Vec<_> = pages
            .split_once("/Kids [")
            .unwrap()
            .1
            .split_once(']')
            .unwrap()
            .0
            .split_whitespace()
            .collect();
        kids.as_chunks::<3>()
            .0
            .iter()
            .map(|entry| {
                assert_eq!(&entry[1..], &["0", "R"]);
                entry[0].parse().unwrap()
            })
            .collect()
    }

    fn font(&self, id: usize) -> BTreeMap<u16, String> {
        let font = self.dictionary(id);
        let cmap = self.stream(number(font, "/ToUnicode "));
        let mut unicode = BTreeMap::new();
        for block in cmap.split("beginbfchar\n").skip(1) {
            for row in block.split_once("endbfchar").unwrap().0.lines() {
                let mut words = row.split_whitespace();
                let cid = words.next().unwrap().trim_matches(['<', '>']);
                let encoded = words.next().unwrap().trim_matches(['<', '>']);
                unicode.insert(
                    u16::from_str_radix(cid, 16).unwrap(),
                    String::from_utf16(&units(encoded)).unwrap(),
                );
            }
        }
        assert!(!unicode.is_empty());
        unicode
    }

    fn runs(&self) -> Vec<Run> {
        let mut runs = Vec::new();
        for (index, page) in self.pages().into_iter().enumerate() {
            let dictionary = self.dictionary(page);
            let entries: Vec<_> = dictionary
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
                .map(|entry| (entry[0], self.font(entry[1].parse().unwrap())))
                .collect();
            let content = self.stream(number(dictionary, "/Contents "));
            for run in content.split("BT ").skip(1) {
                let run = run.split_once(" ET").unwrap().0;
                let words: Vec<_> = run.split_whitespace().collect();
                let tf = words.iter().position(|&word| word == "Tf").unwrap();
                let tm = words.iter().position(|&word| word == "Tm").unwrap();
                let font = &fonts[words[tf - 2]];
                let mut text = String::new();
                for hex in run.split_once(" Tm ").unwrap().1.split('<').skip(1) {
                    for cid in units(hex.split_once('>').unwrap().0) {
                        text.push_str(&font[&cid]);
                    }
                }
                runs.push(Run {
                    page: index,
                    text,
                    x: words[tm - 2].parse().unwrap(),
                    y: words[tm - 1].parse().unwrap(),
                    size: words[tf - 1].parse().unwrap(),
                });
            }
        }
        runs
    }
}

fn number(text: &str, key: &str) -> usize {
    text.split_once(key)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn units(hex: &str) -> Vec<u16> {
    assert_eq!(hex.len() % 4, 0);
    (0..hex.len())
        .step_by(4)
        .map(|at| u16::from_str_radix(&hex[at..at + 4], 16).unwrap())
        .collect()
}

#[derive(Debug)]
struct Run {
    page: usize,
    text: String,
    x: f32,
    y: f32,
    size: f32,
}

fn options() -> PdfOptions {
    PdfOptions {
        use_theme_spacing: true,
        ..PdfOptions::default()
    }
}

fn find<'a>(runs: &'a [Run], text: &str) -> &'a Run {
    runs.iter()
        .find(|run| run.text == text || run.text.ends_with(&format!(" {text}")))
        .unwrap_or_else(|| panic!("missing {text} in {runs:?}"))
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.03,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn literal_theme_leading_controls_emitted_baselines_without_changing_type_size() {
    let source = "ALPHA  \nBRAVO  \nCHARLIE";
    for leading in [1.0, 1.7, 2.0, 4.0] {
        let mut options = options();
        options.theme.spacing.line_height = leading;
        let pdf = render_pdf(source, &options).unwrap();
        let runs = Objects::new(&pdf).runs();
        for pair in runs.windows(2) {
            assert_eq!(pair[0].page, pair[1].page);
            close(pair[0].y - pair[1].y, 11.0 * leading);
        }
        for run in &runs {
            close(run.size, 11.0);
        }
        assert_eq!(
            runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>(),
            ["ALPHA", "BRAVO", "CHARLIE"]
        );
    }
}

#[test]
fn tight_leading_reserves_inline_code_chip_depth_at_page_boundaries() {
    let mut options = options();
    options.base_font_size = Some(12.0);
    options.strict_pagination = true;
    options.theme.spacing.line_height = 1.0;
    options.theme.page.size.height_pt = 144.0;
    options.theme.page.margins.top_pt = 36.0;
    options.theme.page.margins.bottom_pt = 36.0;
    let source = (0..6).map(|_| "`A`").collect::<Vec<_>>().join("  \n");
    let pdf = render_pdf(&source, &options).unwrap();
    let objects = Objects::new(&pdf);
    assert_eq!(
        objects.pages().len(),
        2,
        "six 12.72pt chips cannot fit in 72pt"
    );
    let runs = objects.runs();
    assert_eq!(runs.len(), 6);
    for run in &runs {
        assert_eq!(run.text, "A");
        close(run.size, 12.0);
        assert!(run.y - run.size * 0.26 >= 36.0 - 0.01, "{run:?}");
        assert!(run.y + run.size * 0.74 <= 108.0 + 0.01, "{run:?}");
    }
}

#[test]
fn table_padding_is_per_side_and_participates_in_row_measurement() {
    let source = "| HEADER |\n| --- |\n| FIRST |\n| SECOND |\n";
    let mut options = options();
    options.table_font_size = Some(10.0);
    options.theme.spacing.table_cell_padding_x_em = 1.5;
    options.theme.spacing.table_cell_padding_y_em = 0.0;
    let plain = render_pdf(source, &options).unwrap();
    let plain = Objects::new(&plain).runs();
    options.theme.spacing.table_cell_padding_y_em = 0.6;
    let padded = render_pdf(source, &options).unwrap();
    let padded = Objects::new(&padded).runs();
    for name in ["HEADER", "FIRST", "SECOND"] {
        close(find(&padded, name).x, 72.0 + 15.0);
        close(find(&padded, name).size, 10.0);
    }
    close(find(&plain, "HEADER").y - find(&padded, "HEADER").y, 6.0);
    let ordinary_step = find(&plain, "FIRST").y - find(&plain, "SECOND").y;
    let padded_step = find(&padded, "FIRST").y - find(&padded, "SECOND").y;
    close(ordinary_step, 17.0);
    close(padded_step - ordinary_step, 12.0);
}

#[test]
fn vertical_padding_belongs_to_a_logical_row_not_each_wrapped_fragment() {
    let mut document = parse_markdown("| HEADER |\n| --- |\n| FIRST |\n| NEXT |\n");
    let Block::Table(table) = &mut document.blocks[0] else {
        panic!("expected a table");
    };
    table.rows[0][0] = vec![
        Inline::Text("FIRST".to_owned()),
        Inline::HardBreak,
        Inline::Text("MIDDLE".to_owned()),
        Inline::HardBreak,
        Inline::Text("LAST".to_owned()),
    ];
    let mut options = options();
    options.table_font_size = Some(10.0);
    options.theme.spacing.table_cell_padding_y_em = 0.6;
    let pdf =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap();
    let runs = Objects::new(&pdf).runs();
    close(find(&runs, "FIRST").y - find(&runs, "MIDDLE").y, 17.0);
    close(find(&runs, "MIDDLE").y - find(&runs, "LAST").y, 17.0);
    close(find(&runs, "LAST").y - find(&runs, "NEXT").y, 29.0);
}

#[test]
fn list_quote_and_footnote_rows_use_the_same_resolved_leading() {
    let source = concat!(
        "- LISTONE  \n  LISTTWO\n\n",
        "> QUOTEONE  \n> QUOTETWO\n\n",
        "A claim.[^n]\n\n",
        "[^n]: NOTEONE  \n    NOTETWO\n",
    );
    let mut options = options();
    options.theme.spacing.line_height = 2.0;
    let pdf = render_pdf(source, &options).unwrap();
    let runs = Objects::new(&pdf).runs();
    for (first, second, size) in [
        ("LISTONE", "LISTTWO", 11.0),
        ("QUOTEONE", "QUOTETWO", 11.0),
        ("NOTEONE", "NOTETWO", 9.35),
    ] {
        let first = find(&runs, first);
        let second = find(&runs, second);
        assert_eq!(first.page, second.page);
        close(first.size, size);
        close(second.size, size);
        close(first.y - second.y, 2.0 * size);
    }
}

#[test]
fn footnote_table_padding_uses_its_actual_smaller_type_size() {
    let source = concat!(
        "A claim.[^n]\n\n",
        "[^n]:\n",
        "    | NHEADER |\n",
        "    | --- |\n",
        "    | NFIRST |\n",
        "    | NSECOND |\n",
    );
    let mut options = options();
    options.table_font_size = Some(10.0);
    options.theme.spacing.line_height = 2.0;
    options.theme.spacing.table_cell_padding_x_em = 1.5;
    options.theme.spacing.table_cell_padding_y_em = 0.6;
    let pdf = render_pdf(source, &options).unwrap();
    let runs = Objects::new(&pdf).runs();
    for name in ["NHEADER", "NFIRST", "NSECOND"] {
        let run = find(&runs, name);
        close(run.size, 8.5);
        close(run.x, 72.0 + 1.5 * 8.5);
        assert_eq!(run.page, 0);
        assert!(
            run.y < 220.0,
            "note table should stay at the page foot: {run:?}"
        );
    }
    close(
        find(&runs, "NFIRST").y - find(&runs, "NSECOND").y,
        3.2 * 8.5,
    );
}

#[test]
fn padding_changes_panel_capacity_without_losing_source_columns() {
    let mut options = options();
    options.theme.page.size.width_pt = 162.0;
    options.theme.page.margins.left_pt = 36.0;
    options.theme.page.margins.right_pt = 36.0;
    options.theme.spacing.table_cell_padding_x_em = 2.0;
    options.theme.spacing.table_cell_padding_y_em = 0.25;
    options.table_font_size = Some(10.0);
    let source = "| AHEAD | BHEAD | CHEAD |\n| --- | --- | --- |\n| AONE | BONE | CONE |\n";
    let pdf = render_pdf(source, &options).unwrap();
    let objects = Objects::new(&pdf);
    let tables = (1..objects.offsets.len())
        .filter(|&id| {
            objects
                .dictionary(id)
                .contains("/Type /StructElem /S /Table ")
        })
        .count();
    assert_eq!(
        tables, 3,
        "each 60pt minimum grid column needs its own 90pt panel"
    );
    let runs = objects.runs();
    for name in ["AHEAD", "BHEAD", "CHEAD", "AONE", "BONE", "CONE"] {
        assert_eq!(
            runs.iter().filter(|run| run.text == name).count(),
            1,
            "{runs:?}"
        );
        close(find(&runs, name).x, 56.0);
    }
}

#[test]
fn impossible_horizontal_padding_returns_an_error_without_discarding_the_request() {
    let mut options = options();
    options.table_font_size = Some(10.0);
    options.theme.page.size.width_pt = 162.0;
    options.theme.page.margins.left_pt = 36.0;
    options.theme.page.margins.right_pt = 36.0;
    options.theme.spacing.table_cell_padding_x_em = 8.0;
    assert!(render_pdf("A paragraph.", &options).is_ok());
    let source = "> | HEADER |\n> | --- |\n> | VALUE |\n";
    let error = match render_pdf(source, &options) {
        Err(error) => error,
        Ok(_) => panic!("a 180pt minimum padded cell cannot fit a 90pt page body"),
    };
    assert!(matches!(error, RenderError::InvalidInput(_)));
    assert!(error.to_string().contains("pdf_theme_spacing"));
    assert!(error.to_string().contains("horizontal table padding"));
    assert!(
        franken_markdown::verify::verify_pdf_result(&parse_markdown(source), &options).is_err()
    );
}

#[test]
fn impossible_vertical_padding_is_rejected_before_strict_or_fallback_pagination() {
    let source = "| HEADER |\n| --- |\n| VALUE |\n";
    let mut options = options();
    options.table_font_size = Some(10.0);
    options.theme.spacing.table_cell_padding_y_em = 8.0;
    options.theme.page.size.height_pt = 144.0;
    options.theme.page.margins.top_pt = 36.0;
    options.theme.page.margins.bottom_pt = 36.0;
    for strict in [false, true] {
        for grid in [None, Some(BaselineGrid::from_points(12.0).unwrap())] {
            options.strict_pagination = strict;
            options.theme.page.baseline_grid = grid;
            let error = match render_pdf(source, &options) {
                Err(error) => error,
                Ok(_) => panic!("a 177pt padded row cannot fit a 72pt page body"),
            };
            assert!(matches!(error, RenderError::InvalidInput(_)));
            assert!(error.to_string().contains("pdf_theme_spacing"));
            assert!(error.to_string().contains("vertical spacing"));
            let error =
                franken_markdown::verify::verify_pdf_result(&parse_markdown(source), &options)
                    .unwrap_err();
            assert!(error.to_string().contains("pdf_theme_spacing"));
        }
    }
}

#[test]
fn fitting_rows_keep_source_when_a_continuation_header_cannot_fit() {
    let source = "| HEADER |\n| --- |\n| FIRST |\n| SECOND |\n| THIRD |\n";
    let mut options = options();
    options.table_font_size = Some(10.0);
    options.theme.spacing.table_cell_padding_y_em = 2.0;
    options.theme.page.size.height_pt = 144.0;
    options.theme.page.margins.top_pt = 36.0;
    options.theme.page.margins.bottom_pt = 36.0;
    let pdf = render_pdf(source, &options).unwrap();
    let objects = Objects::new(&pdf);
    let runs = objects.runs();
    assert!(objects.pages().len() >= 4);
    for name in ["HEADER", "FIRST", "SECOND", "THIRD"] {
        assert_eq!(runs.iter().filter(|run| run.text == name).count(), 1);
        let run = find(&runs, name);
        // A 57pt line box has 33.6pt above and 23.4pt below its baseline.
        assert!(run.y + 33.6 <= 108.0 + 0.01, "{run:?}");
        assert!(run.y - 23.4 >= 36.0 - 0.01, "{run:?}");
    }
}

#[test]
fn padded_tables_repeat_headers_inside_strict_and_gridded_pages() {
    let source = format!(
        "| HEADER |\n| --- |\n{}",
        (0..18)
            .map(|row| format!("| ROW{row:02} |\n"))
            .collect::<String>()
    );
    let mut options = options();
    options.strict_pagination = true;
    options.table_font_size = Some(10.0);
    options.theme.spacing.table_cell_padding_y_em = 1.0;
    options.theme.page.size.height_pt = 288.0;
    options.theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 36.0,
        bottom_pt: 24.0,
        left_pt: 36.0,
    };
    for grid in [None, Some(BaselineGrid::from_points(12.0).unwrap())] {
        options.theme.page.baseline_grid = grid;
        let rendered = render_pdf_reported(&source, &options).unwrap();
        assert!(rendered.pagination.page_breaks.is_exact());
        let objects = Objects::new(&rendered.bytes);
        assert!(
            objects.pages().len() >= 4,
            "padding must consume physical page capacity"
        );
        let runs = objects.runs();
        for page in 0..objects.pages().len() {
            assert!(
                runs.iter()
                    .any(|run| run.page == page && run.text == "HEADER")
            );
        }
        for row in 0..18 {
            assert_eq!(
                runs.iter()
                    .filter(|run| run.text == format!("ROW{row:02}"))
                    .count(),
                1
            );
        }
        for run in &runs {
            assert!(run.y > 24.0 && run.y < 264.0, "{run:?}");
            if grid.is_some() {
                close((264.0 - run.y) / 12.0, ((264.0 - run.y) / 12.0).round());
            }
        }
        let document = parse_markdown(&source);
        assert_eq!(
            rendered.bytes,
            render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap()
        );
        assert_eq!(
            franken_markdown::verify::verify_pdf_result(&document, &options)
                .unwrap()
                .page_count,
            objects.pages().len()
        );
    }
}

#[test]
fn invalid_explicit_spacing_is_rejected_and_legacy_output_stays_identical() {
    let source = "A paragraph.\n\n| HEADER |\n| --- |\n| VALUE |\n";
    let default = render_pdf(source, &PdfOptions::default()).unwrap();
    for (field, value) in [
        (0, f32::NAN),
        (0, 0.99),
        (0, 4.01),
        (1, f32::INFINITY),
        (1, -0.1),
        (1, 8.01),
        (2, f32::NAN),
        (2, -0.1),
        (2, 8.01),
    ] {
        let mut options = options();
        match field {
            0 => options.theme.spacing.line_height = value,
            1 => options.theme.spacing.table_cell_padding_x_em = value,
            _ => options.theme.spacing.table_cell_padding_y_em = value,
        }
        let error = match render_pdf(source, &options) {
            Err(error) => error,
            Ok(_) => panic!("invalid spacing field {field}={value} was accepted"),
        };
        assert!(matches!(error, RenderError::InvalidInput(_)));
        assert!(error.to_string().contains("pdf_theme_spacing"));
        assert!(
            franken_markdown::verify::verify_pdf_result(&parse_markdown(source), &options).is_err()
        );
        options.use_theme_spacing = false;
        assert_eq!(render_pdf(source, &options).unwrap(), default);
    }
}
