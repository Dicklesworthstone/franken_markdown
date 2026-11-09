//! Continuation headers must leave physical page space for real table content.
//! Read the emitted PDF's own CMaps, operators and tags; no producer-side
//! pagination or verification helper supplies the content/placement oracle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use std::collections::BTreeMap;

use franken_markdown::{
    BaselineGrid, PageMargins, PageSize, PdfEmitOptions, PdfOptions, PdfPageEmission,
    PdfPaginationStrategy, RenderError, render_pdf, render_pdf_emitted, render_pdf_reported,
};

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
            .rposition(|part| part == marker)
            .unwrap();
        let xref = number(std::str::from_utf8(&bytes[at..]).unwrap(), "startxref\n");
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let section: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(section[0], "0");
        let count: usize = section[1].parse().unwrap();
        let offsets = lines
            .take(count)
            .map(|line| line.split_whitespace().next().unwrap().parse().unwrap())
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
        let body = self.object(id);
        let end = body
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap_or(body.len());
        std::str::from_utf8(&body[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let body = self.object(id);
        let start = body
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap()
            + 8;
        let dictionary = self.dictionary(id);
        let payload = &body[start..start + number(dictionary, "/Length ")];
        let bytes = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
        } else {
            assert!(!dictionary.contains("/Filter"));
            payload.to_vec()
        };
        String::from_utf8(bytes).unwrap()
    }

    fn pages(&self) -> Vec<usize> {
        let trailer = std::str::from_utf8(&self.bytes[self.xref..]).unwrap();
        let catalog = self.dictionary(number(trailer, "/Root "));
        let pages = self.dictionary(number(catalog, "/Pages "));
        let fields: Vec<_> = pages
            .split_once("/Kids [")
            .unwrap()
            .1
            .split_once(']')
            .unwrap()
            .0
            .split_whitespace()
            .collect();
        assert_eq!(fields.len() % 3, 0);
        fields
            .as_chunks::<3>()
            .0
            .iter()
            .map(|part| {
                assert_eq!(&part[1..], &["0", "R"]);
                part[0].parse().unwrap()
            })
            .collect()
    }

    fn fonts(&self, page: usize) -> BTreeMap<String, BTreeMap<u16, String>> {
        let dictionary = self.dictionary(page);
        let fields: Vec<_> = dictionary
            .split_once("/Font <<")
            .unwrap()
            .1
            .split_once(">>")
            .unwrap()
            .0
            .split_whitespace()
            .collect();
        assert_eq!(fields.len() % 4, 0);
        fields
            .as_chunks::<4>()
            .0
            .iter()
            .map(|part| {
                assert_eq!(&part[2..], &["0", "R"]);
                let font = self.dictionary(part[1].parse().unwrap());
                let cmap = self.stream(number(font, "/ToUnicode "));
                let mut mappings = BTreeMap::new();
                for block in cmap.split("beginbfchar\n").skip(1) {
                    for line in block.split_once("endbfchar").unwrap().0.lines() {
                        let mut fields = line.split_whitespace();
                        let source = fields.next().unwrap().trim_matches(['<', '>']);
                        let target = fields.next().unwrap().trim_matches(['<', '>']);
                        mappings.insert(
                            u16::from_str_radix(source, 16).unwrap(),
                            String::from_utf16(&hex_units(target)).unwrap(),
                        );
                    }
                }
                assert!(!mappings.is_empty());
                (part[0].to_owned(), mappings)
            })
            .collect()
    }

    fn text(&self) -> Vec<Vec<TextRun>> {
        self.pages()
            .into_iter()
            .map(|page| {
                let fonts = self.fonts(page);
                let stream = self.stream(number(self.dictionary(page), "/Contents "));
                decode_text(&stream, &fonts)
            })
            .collect()
    }

    fn count_tag(&self, tag: &str) -> usize {
        let marker = format!("/Type /StructElem /S /{tag} ");
        (1..self.offsets.len())
            .filter(|&id| self.dictionary(id).contains(&marker))
            .count()
    }
}

#[derive(Debug)]
struct TextRun {
    text: String,
    y: f32,
    artifact: bool,
}

fn decode_text(content: &str, fonts: &BTreeMap<String, BTreeMap<u16, String>>) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut marks = Vec::new();
    let mut font = None;
    let mut y = None;
    let mut text = String::new();
    let mut run_y = None;
    let mut run_artifact = None;
    let mut in_text = false;
    for operation in pdf_content::operations(content) {
        match operation.operator {
            "BDC" | "BMC" => marks.push(operation.operands[0] == "/Artifact"),
            "EMC" => {
                marks.pop().expect("balanced marked-content stack");
            }
            "BT" => {
                assert!(!in_text);
                in_text = true;
                y = None;
                assert!(text.is_empty());
            }
            "ET" => {
                assert!(in_text);
                in_text = false;
                if !text.is_empty() {
                    runs.push(TextRun {
                        text: std::mem::take(&mut text),
                        y: run_y.unwrap(),
                        artifact: run_artifact.unwrap(),
                    });
                }
            }
            "Tf" => font = Some(&fonts[operation.operands[0]]),
            "Tm" => y = Some(operation.operands[5].parse::<f32>().unwrap()),
            "Tj" | "TJ" => {
                assert!(in_text);
                let ids = pdf_content::glyph_ids(&operation);
                if ids.is_empty() {
                    continue;
                }
                let y = y.expect("painted text has a baseline");
                let artifact = marks.iter().any(|&artifact| artifact);
                if !text.is_empty() && (run_y != Some(y) || run_artifact != Some(artifact)) {
                    runs.push(TextRun {
                        text: std::mem::take(&mut text),
                        y: run_y.unwrap(),
                        artifact: run_artifact.unwrap(),
                    });
                }
                run_y = Some(y);
                run_artifact = Some(artifact);
                let font = font.expect("painted text has a font");
                for id in ids {
                    text.push_str(font.get(&id).expect("painted glyph has Unicode"));
                }
            }
            _ => {}
        }
    }
    assert!(marks.is_empty());
    assert!(!in_text);
    runs
}

#[test]
fn row_decoder_preserves_nested_artifacts_and_continuing_note_text() {
    let fonts = BTreeMap::from([(
        "/F1".to_owned(),
        BTreeMap::from([
            (1, "Header".to_owned()),
            (2, " row".to_owned()),
            (3, "¹ ".to_owned()),
            (4, "Body".to_owned()),
        ]),
    )]);
    let content = "/Artifact BMC
        BT /F1 11 Tf 1 0 0 1 24 80 Tm
        /Span << /ActualText <FEFF0058> >> BDC [<0001>] TJ EMC
        <0002> Tj ET
        EMC
        BT /F1 11 Tf 1 0 0 1 24 60 Tm
        /Lbl << /MCID 0 >> BDC <0003> Tj EMC
        /P << /MCID 1 >> BDC /Span << /ActualText (BT ET Tj) >> BDC
        <0004> Tj EMC EMC
        ET";
    let runs = decode_text(content, &fonts);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "Header row");
    assert_eq!(runs[0].y, 80.0);
    assert!(runs[0].artifact);
    assert_eq!(runs[1].text, "¹ Body");
    assert_eq!(runs[1].y, 60.0);
    assert!(!runs[1].artifact);
}

fn hex_units(hex: &str) -> Vec<u16> {
    assert_eq!(hex.len() % 4, 0);
    (0..hex.len())
        .step_by(4)
        .map(|at| u16::from_str_radix(&hex[at..at + 4], 16).unwrap())
        .collect()
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

fn options() -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize {
        name: "table-header-test",
        width_pt: 216.0,
        height_pt: 216.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    options
}

fn source(headers: usize, rows: usize, linked: bool, notes: bool) -> String {
    let head = (0..headers)
        .map(|i| {
            if linked && i == 0 {
                "[H000](https://example.com/header)".to_owned()
            } else {
                format!("H{i:03}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut text = format!("| {head} | Description |\n| --- | --- |\n");
    for i in 0..rows {
        let label = if linked {
            format!("[R{i:03}](https://example.com/row/{i})")
        } else {
            format!("R{i:03}")
        };
        let note = if notes {
            format!("[^n{i}]")
        } else {
            String::new()
        };
        text.push_str(&format!("| {label}{note} | value |\n"));
    }
    text.push_str("\nAfter sentinel.\n");
    if notes {
        for i in 0..rows {
            text.push_str(&format!("\n[^n{i}]: Note{i:03}.\n"));
        }
    }
    text
}

fn assert_content(pdf: &[u8], headers: usize, rows: usize) -> Vec<Vec<TextRun>> {
    let objects = Objects::new(pdf);
    let pages = objects.text();
    assert!(pages.len() > 1);
    let mut logical = String::new();
    for (page, runs) in pages.iter().enumerate() {
        for run in runs {
            assert!(
                (23.99..=192.01).contains(&run.y),
                "page {} text escaped printable body: {run:?}",
                page + 1
            );
            if !run.artifact {
                logical.push_str(&run.text);
                logical.push('\n');
            }
        }
    }
    for i in 0..headers {
        let token = format!("H{i:03}");
        assert_eq!(
            logical.matches(&token).count(),
            1,
            "header must remain exactly once logically: {token}"
        );
    }
    for i in 0..rows {
        let token = format!("R{i:03}");
        assert_eq!(
            logical.matches(&token).count(),
            1,
            "row must remain exactly once logically: {token}"
        );
    }
    assert!(
        logical.contains("After sentinel."),
        "following prose was lost: {logical}"
    );
    assert_eq!(objects.count_tag("Table"), 1);
    assert_eq!(objects.count_tag("TR"), rows + 1);
    assert_eq!(objects.count_tag("TH"), 2);
    assert_eq!(objects.count_tag("TD"), rows * 2);
    pages
}

#[test]
fn oversized_header_retains_original_header_every_row_and_following_prose() {
    let source = source(100, 20, false, false);
    for optimal in [false, true] {
        let options = PdfOptions {
            optimal_pagination: optimal,
            ..options()
        };
        let pdf = render_pdf(&source, &options).unwrap();
        let pages = assert_content(&pdf, 100, 20);
        assert!(
            !pages.iter().flatten().any(|run| run.artifact),
            "a header taller than the page must not repeat"
        );
        assert_eq!(pdf, render_pdf(&source, &options).unwrap());
        for emission in [PdfPageEmission::Monolithic, PdfPageEmission::Chunked] {
            assert_eq!(
                pdf,
                render_pdf_emitted(
                    &source,
                    &options,
                    PdfEmitOptions {
                        emission,
                        ..PdfEmitOptions::default()
                    }
                )
                .unwrap()
            );
        }
    }
}

#[test]
fn near_full_headers_cannot_manufacture_extra_body_capacity() {
    // 28 words leave room for one physical row, less than the old 40pt floor.
    // 32 words fit by themselves but leave no room for even the first row.
    for headers in [28, 32] {
        let source = source(headers, 20, false, false);
        for optimal in [false, true] {
            let options = PdfOptions {
                optimal_pagination: optimal,
                ..options()
            };
            let pages = assert_content(&render_pdf(&source, &options).unwrap(), headers, 20);
            if headers == 28 {
                assert!(
                    pages.iter().flatten().any(|run| run.artifact),
                    "the fitting header must still repeat"
                );
            } else {
                assert!(
                    !pages.iter().flatten().any(|run| run.artifact),
                    "a header must leave room for its body row"
                );
            }
        }
    }
}

#[test]
fn oversized_header_does_not_hide_a_wrapped_body_row() {
    let words = (0..120)
        .map(|i| format!("W{i:03}"))
        .collect::<Vec<_>>()
        .join(" ");
    let source = source(100, 1, false, false)
        .replace("| R000 | value |", &format!("| R000 {words} | value |"));
    let pdf = render_pdf(&source, &options()).unwrap();
    let pages = assert_content(&pdf, 100, 1);
    let text: String = pages
        .iter()
        .flatten()
        .map(|run| run.text.as_str())
        .collect();
    for i in 0..120 {
        assert!(
            text.contains(&format!("W{i:03}")),
            "wrapped source word {i} was lost"
        );
    }
}

#[test]
fn oversized_header_respects_fractional_baseline_grid() {
    let source = source(100, 20, false, false);
    let mut options = options();
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(12.5).unwrap());
    let pages = assert_content(&render_pdf(&source, &options).unwrap(), 100, 20);
    for run in pages.iter().flatten() {
        let pitch = (192.0 - run.y) / 12.5;
        assert!(
            (pitch - pitch.round()).abs() < 0.001,
            "off-grid baseline: {run:?}"
        );
    }
}

#[test]
fn header_repetition_respects_page_footnote_reservations() {
    let source = source(24, 20, false, true);
    for optimal in [false, true] {
        let options = PdfOptions {
            optimal_pagination: optimal,
            ..options()
        };
        let pdf = render_pdf(&source, &options).unwrap();
        let pages = assert_content(&pdf, 24, 20);
        let logical: String = pages
            .iter()
            .flatten()
            .filter(|run| !run.artifact)
            .map(|run| run.text.as_str())
            .collect();
        for i in 0..20 {
            assert_eq!(
                logical.matches(&format!("Note{i:03}.")).count(),
                1,
                "note {i} lost or duplicated"
            );
        }
        assert!(
            !logical.contains("Notes"),
            "these are page-foot notes, not endnotes"
        );
        for i in 0..20 {
            let locate = |needle: &str| {
                pages
                    .iter()
                    .enumerate()
                    .find_map(|(page, runs)| {
                        runs.iter()
                            .find(|run| run.text.contains(needle))
                            .map(|run| (page, run.y))
                    })
                    .unwrap()
            };
            let (row_page, row_y) = locate(&format!("R{i:03}"));
            let (note_page, note_y) = locate(&format!("Note{i:03}."));
            assert_eq!(
                row_page, note_page,
                "note {i} must stay with its citing row"
            );
            assert!(note_y < row_y, "note {i} must sit below its citing row");
        }
        let objects = Objects::new(&pdf);
        let note_links: Vec<_> = (1..objects.offsets.len())
            .map(|id| objects.dictionary(id))
            .filter(|dictionary| dictionary.contains("/Subtype /Link "))
            .collect();
        assert_eq!(
            note_links.len(),
            20,
            "every note mark retains its annotation"
        );
        assert!(
            note_links
                .iter()
                .all(|dictionary| dictionary.contains("/Dest [")),
            "every note mark resolves to an actual destination"
        );
    }
}

#[test]
fn original_header_and_body_links_keep_visible_rectangles_and_table_tags() {
    let pdf = render_pdf(&source(100, 20, true, false), &options()).unwrap();
    assert_content(&pdf, 100, 20);
    let objects = Objects::new(&pdf);
    let mut links = 0;
    for id in 1..objects.offsets.len() {
        let dictionary = objects.dictionary(id);
        if !dictionary.contains("/Subtype /Link ") {
            continue;
        }
        let fields: Vec<f32> = dictionary
            .split_once("/Rect [")
            .unwrap()
            .1
            .split_once(']')
            .unwrap()
            .0
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(fields.len(), 4);
        assert!(
            fields.iter().all(|value| (0.0..=216.0).contains(value)),
            "link off page: {dictionary}"
        );
        assert!(dictionary.contains("/StructParent "));
        links += 1;
    }
    assert_eq!(
        links, 21,
        "the source header link and every row link remain"
    );
}

#[test]
fn ordinary_headers_repeat_and_exact_requests_still_refuse_infeasible_tables() {
    let mut options = options();
    options.strict_pagination = true;
    let source = source(4, 30, false, false);
    let report = render_pdf_reported(&source, &options).unwrap();
    assert_eq!(
        report.pagination.page_breaks.strategy,
        PdfPaginationStrategy::ExactHeight
    );
    let pages = assert_content(&report.bytes, 4, 30);
    assert!(
        pages
            .iter()
            .skip(1)
            .flat_map(|page| page.iter())
            .any(|run| run.artifact)
    );
    assert!(matches!(
        render_pdf(&self::source(100, 20, false, false), &options),
        Err(RenderError::Pagination(_))
    ));
}
