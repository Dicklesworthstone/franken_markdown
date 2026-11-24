//! Joint paragraph choices must preserve real continued-note pages and source.
//! The oracle below reads the emitted xref, font maps and painted PDF operators;
//! it does not use the paragraph or continuation planner to infer its assertions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use std::collections::BTreeMap;

use franken_markdown::{
    BaselineGrid, Block, PageMargins, PdfEmitOptions, PdfOptions, PdfParagraphVariantStatus,
    parse_markdown, render_pdf_document_reported,
};

const PROSE: &str = "The old stone bridge stood beside the river while the town woke slowly. \
    A small boat crossed the clear water and a child watched from the steps. \
    The morning light reached the houses along the bank and the shop doors opened. \
    People walked to work, shared news with friends, and stopped to hear the birds. \
    By noon the market was full of fresh bread, bright fruit, and flowers from the hills.";

fn prose() -> String {
    [PROSE; 3].join(" ")
}

fn options(grid: bool) -> PdfOptions {
    let mut options = PdfOptions {
        optimal_pagination: true,
        split_footnotes: true,
        footnote_backlinks: true,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    // The ungridded 440pt fixture has a 14-line baseline and a certified
    // 13-line same-measure alternative. A 13.333pt grid advances these body
    // rows by two pitches; 360pt likewise admits thirteen, but not fourteen.
    options.theme.page.size.width_pt = 512.0;
    options.theme.page.size.height_pt = if grid { 432.0 } else { 268.0 };
    options.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options.theme.page.baseline_grid = grid.then(|| BaselineGrid::from_points(13.333).unwrap());
    options.running.footer.center = Some("FOLIO {page}/{pages}".into());
    options
}

fn note(rows: usize) -> String {
    let lines = (0..rows)
        .map(|index| format!("NOTE{index:03} preserved source line."))
        .collect::<Vec<_>>()
        .join("  \n    ");
    format!("[^long]: {lines}\n")
}

fn compact(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

const PRECISION: f64 = 0.015;

struct Objects<'a> {
    bytes: &'a [u8],
    offsets: Vec<usize>,
    xref: usize,
}

impl<'a> Objects<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let marker = b"startxref\n";
        let start = bytes
            .windows(marker.len())
            .rposition(|x| x == marker)
            .unwrap();
        let xref = std::str::from_utf8(&bytes[start + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let section: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(
            section[0], "0",
            "the writer emits one complete xref subsection"
        );
        let count: usize = section[1].parse().unwrap();
        let offsets = lines
            .take(count)
            .map(|line| {
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts[2] == "n" {
                    parts[0].parse().unwrap()
                } else {
                    0
                }
            })
            .collect();
        Self {
            bytes,
            offsets,
            xref,
        }
    }

    fn body(&self, id: usize) -> &'a [u8] {
        let start = self.offsets[id];
        let end = self
            .offsets
            .iter()
            .copied()
            .filter(|&offset| offset > start)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        let object = &self.bytes[start..end];
        assert!(
            object.starts_with(header.as_bytes()),
            "xref must identify the actual object"
        );
        &object[header.len()..]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let body = self.body(id);
        let end = body
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap_or(body.len());
        std::str::from_utf8(&body[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let body = self.body(id);
        let start = body
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&body[..start]).unwrap();
        let length = number_after(dictionary, "/Length ") as usize;
        let payload = &body[start + 8..start + 8 + length];
        let bytes = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
        } else {
            assert!(
                !dictionary.contains("/Filter"),
                "unsupported test stream filter"
            );
            payload.to_vec()
        };
        String::from_utf8(bytes).unwrap()
    }
}

fn number_after(dictionary: &str, key: &str) -> f64 {
    dictionary
        .split_once(key)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .trim_end_matches(']')
        .parse()
        .unwrap()
}

fn reference_after(dictionary: &str, key: &str) -> usize {
    number_after(dictionary, key) as usize
}

#[derive(Debug, Clone)]
struct FontMap {
    unicode: BTreeMap<u16, String>,
    cap_height: f64,
    descent: f64,
}

fn font_map(objects: &Objects<'_>, id: usize) -> FontMap {
    let font = objects.dictionary(id);
    let cmap = objects.stream(reference_after(font, "/ToUnicode "));
    let mut unicode = BTreeMap::new();
    let mut in_char_map = false;
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            in_char_map = true;
            continue;
        }
        if line == "endbfchar" {
            in_char_map = false;
            continue;
        }
        if !in_char_map {
            continue;
        }
        let entries: Vec<_> = line.split_whitespace().collect();
        let id = u16::from_str_radix(entries[0].trim_matches(['<', '>']), 16).unwrap();
        let encoded = entries[1].trim_matches(['<', '>']);
        let units: Vec<u16> = encoded
            .as_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
            .collect();
        unicode.insert(id, String::from_utf16(&units).unwrap());
    }
    assert!(
        !unicode.is_empty(),
        "font must expose selectable painted glyphs"
    );
    let descendant = objects.dictionary(reference_after(font, "/DescendantFonts ["));
    let descriptor = objects.dictionary(reference_after(descendant, "/FontDescriptor "));
    FontMap {
        unicode,
        cap_height: number_after(descriptor, "/CapHeight ") / 1000.0,
        descent: number_after(descriptor, "/Descent ") / 1000.0,
    }
}

#[derive(Debug, Clone)]
struct PaintedRun {
    x: f64,
    y: f64,
    size: f64,
    cap_height: f64,
    descent: f64,
    text: String,
    artifact: bool,
}

#[derive(Debug, Clone)]
struct PaintedPage {
    width: f64,
    height: f64,
    runs: Vec<PaintedRun>,
    object: usize,
}

fn decode_painted_runs(content: &str, fonts: &BTreeMap<&str, FontMap>) -> Vec<PaintedRun> {
    let mut runs = Vec::new();
    let mut artifacts = Vec::new();
    let mut font = None;
    let mut size = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;
    let mut render_mode = 0;
    let mut in_text = false;
    let mut pending: Option<PaintedRun> = None;
    for operation in pdf_content::operations(content) {
        match operation.operator {
            "BDC" | "BMC" => artifacts.push(
                operation.operands[0] == "/Artifact" || artifacts.last().copied().unwrap_or(false),
            ),
            "EMC" => {
                artifacts.pop().expect("balanced marked content");
            }
            "BT" => {
                assert!(!in_text);
                in_text = true;
            }
            "ET" | "Tf" | "Tm" | "Tr" => {
                if let Some(run) = pending.take() {
                    runs.push(run);
                }
                match operation.operator {
                    "ET" => {
                        assert!(in_text);
                        in_text = false;
                    }
                    "Tf" => {
                        font = Some(&fonts[operation.operands[0]]);
                        size = operation.operands[1].parse().unwrap();
                    }
                    "Tr" => render_mode = operation.operands[0].parse::<u8>().unwrap(),
                    "Tm" => {
                        let matrix = operation
                            .operands
                            .iter()
                            .map(|value| value.parse::<f64>().unwrap())
                            .collect::<Vec<_>>();
                        assert_eq!(
                            &matrix[..4],
                            &[1.0, 0.0, 0.0, 1.0],
                            "fixture text is untransformed"
                        );
                        x = matrix[4];
                        y = matrix[5];
                    }
                    _ => unreachable!(),
                }
            }
            "Td" | "TD" | "T*" => {
                panic!("extend the decoder before accepting relative text motion")
            }
            "TJ" | "Tj" => {
                assert!(in_text);
                let ids = pdf_content::glyph_ids(&operation);
                if render_mode == 3 || ids.is_empty() {
                    continue;
                }
                let face = font.expect("glyph painting needs an active font");
                let artifact = artifacts.last().copied().unwrap_or(false);
                if pending.as_ref().is_some_and(|run| run.artifact != artifact)
                    && let Some(run) = pending.take()
                {
                    runs.push(run);
                }
                let run = pending.get_or_insert_with(|| PaintedRun {
                    x,
                    y,
                    size,
                    cap_height: face.cap_height,
                    descent: face.descent,
                    text: String::new(),
                    artifact,
                });
                for id in ids {
                    run.text.push_str(
                        face.unicode
                            .get(&id)
                            .expect("painted glyph must have a ToUnicode mapping"),
                    );
                }
            }
            _ => {}
        }
    }
    assert!(artifacts.is_empty(), "balanced marked-content stack");
    assert!(!in_text && pending.is_none(), "balanced text objects");
    runs
}

fn painted_pages(pdf: &[u8]) -> Vec<PaintedPage> {
    let objects = Objects::new(pdf);
    let pages = (1..objects.offsets.len())
        .find(|&id| objects.dictionary(id).contains("/Type /Pages "))
        .unwrap();
    let kids = objects
        .dictionary(pages)
        .split_once("/Kids [")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(kids.len() % 3, 0);
    kids.as_chunks::<3>()
        .0
        .iter()
        .map(|reference| {
            let dictionary = objects.dictionary(reference[0].parse().unwrap());
            let media: Vec<f64> = dictionary
                .split_once("/MediaBox [")
                .unwrap()
                .1
                .split_once(']')
                .unwrap()
                .0
                .split_whitespace()
                .map(|x| x.parse().unwrap())
                .collect();
            assert_eq!(&media[..2], &[0.0, 0.0]);
            let resources = dictionary
                .split_once("/Font <<")
                .unwrap()
                .1
                .split_once(">>")
                .unwrap()
                .0;
            let resources: Vec<_> = resources.split_whitespace().collect();
            assert_eq!(resources.len() % 4, 0);
            let fonts: BTreeMap<&str, FontMap> = resources
                .as_chunks::<4>()
                .0
                .iter()
                .map(|entry| (entry[0], font_map(&objects, entry[1].parse().unwrap())))
                .collect();
            let content = objects.stream(reference_after(dictionary, "/Contents "));
            let runs = decode_painted_runs(&content, &fonts);
            PaintedPage {
                width: media[2],
                height: media[3],
                runs,
                object: reference[0].parse().unwrap(),
            }
        })
        .collect()
}

fn text(pages: &[PaintedPage]) -> String {
    pages
        .iter()
        .flat_map(|page| &page.runs)
        .filter(|run| !run.artifact)
        .map(|run| run.text.as_str())
        .collect()
}

fn locate<'a>(pages: &'a [PaintedPage], marker: &str) -> (usize, &'a PaintedRun) {
    pages
        .iter()
        .enumerate()
        .find_map(|(index, page)| {
            page.runs
                .iter()
                .find(|run| !run.artifact && run.text.contains(marker))
                .map(|run| (index, run))
        })
        .unwrap_or_else(|| panic!("missing painted marker {marker}"))
}

fn assert_markers(pages: &[PaintedPage], prefix: &str, count: usize) {
    let text = text(pages);
    let mut previous = 0;
    for index in 0..count {
        let marker = format!("{prefix}{index:03}");
        assert_eq!(text.matches(&marker).count(), 1, "retain {marker} once");
        let offset = text.find(&marker).unwrap();
        assert!(offset >= previous, "source order changed at {marker}");
        previous = offset + marker.len();
    }
}

fn destinations(pdf: &[u8], pages: &[PaintedPage]) -> Vec<(usize, f64)> {
    let objects = Objects::new(pdf);
    (1..objects.offsets.len())
        .filter_map(|id| {
            let dictionary = objects.dictionary(id);
            if !dictionary.contains("/Subtype /Link") {
                return None;
            }
            let (_, destination) = dictionary.split_once("/Dest [")?;
            let fields: Vec<_> = destination
                .split_once(']')
                .unwrap()
                .0
                .split_whitespace()
                .collect();
            assert_eq!(&fields[1..5], &["0", "R", "/XYZ", "null"]);
            let object: usize = fields[0].parse().unwrap();
            Some((
                pages.iter().position(|page| page.object == object).unwrap(),
                fields[5].parse().unwrap(),
            ))
        })
        .collect()
}

fn assert_destinations(pdf: &[u8], pages: &[PaintedPage], targets: &[&str]) {
    let mut actual = destinations(pdf, pages);
    assert_eq!(actual.len(), targets.len());
    for marker in targets {
        let (page, run) = locate(pages, marker);
        let y = run.y + run.size * 0.9;
        let index = actual
            .iter()
            .position(|&(target_page, target_y)| {
                page == target_page && (target_y - y).abs() < 0.025
            })
            .unwrap_or_else(|| {
                panic!("missing destination for {marker}: expected {page}/{y}, got {actual:?}")
            });
        actual.remove(index);
    }
    assert!(actual.is_empty());
}

/// Only join discretionary end hyphens in this explicitly hyphen-free prose.
/// Do not erase punctuation or count selectable maps as proof of painted text.
fn body_prose(page: &PaintedPage) -> (String, usize) {
    assert!(!PROSE.contains('-'));
    let mut rows: Vec<(f64, String)> = Vec::new();
    for run in page
        .runs
        .iter()
        .filter(|run| !run.artifact && run.size == 11.0)
    {
        if let Some((y, text)) = rows.last_mut()
            && (*y - run.y).abs() < PRECISION
        {
            text.push_str(&run.text);
        } else {
            rows.push((run.y, run.text.clone()));
        }
    }
    let mut source = String::new();
    for (_, row) in &rows {
        let row = row.trim_end();
        source.push_str(
            row.strip_suffix(['-', '\u{00ad}', '\u{2010}'])
                .unwrap_or(row),
        );
    }
    (compact(&source), rows.len())
}

fn assert_geometry_and_folios(pages: &[PaintedPage], options: &PdfOptions) {
    let margins = options.theme.page.margins;
    let pitch = options
        .theme
        .page
        .baseline_grid
        .map(|grid| f64::from(grid.spacing_milli_points()) / 1000.0);
    for (index, page) in pages.iter().enumerate() {
        let top = page.height - f64::from(margins.top_pt);
        let mut rows = BTreeMap::<i64, (f64, f64)>::new();
        for run in page
            .runs
            .iter()
            .filter(|run| !run.artifact && run.text.chars().any(|ch| ch.is_ascii_alphabetic()))
        {
            let ink_top = run.y + run.cap_height * run.size;
            let ink_bottom = run.y + run.descent * run.size;
            assert!(ink_top <= top + PRECISION, "page {index}: {run:?}");
            assert!(
                ink_bottom >= f64::from(margins.bottom_pt) - PRECISION,
                "page {index}: {run:?}"
            );
            assert!(
                run.x >= f64::from(margins.left_pt) - PRECISION
                    && run.x < page.width - f64::from(margins.right_pt),
                "page {index}: {run:?}"
            );
            if let Some(pitch) = pitch {
                let steps = (top - run.y) / pitch;
                assert!(
                    (steps - steps.round()).abs() * pitch < PRECISION,
                    "page {index} baseline is off grid: {run:?}"
                );
            }
            rows.entry((run.y * 1000.0).round() as i64)
                .and_modify(|bounds| {
                    bounds.0 = bounds.0.min(ink_bottom);
                    bounds.1 = bounds.1.max(ink_top);
                })
                .or_insert((ink_bottom, ink_top));
        }
        let rows: Vec<_> = rows.into_values().collect();
        for pair in rows.windows(2) {
            assert!(
                pair[0].1 <= pair[1].0 + PRECISION,
                "page {index} has overlapping body/note ink: {pair:?}"
            );
        }
        let folio: String = page
            .runs
            .iter()
            .filter(|run| run.artifact && run.text.starts_with("FOLIO "))
            .map(|run| run.text.as_str())
            .collect();
        assert_eq!(folio, format!("FOLIO {}/{}", index + 1, pages.len()));
    }
}

fn assert_verifier_markers(
    document: &franken_markdown::Document,
    options: &PdfOptions,
    pages: &[PaintedPage],
    markers: &[String],
) {
    let verified = franken_markdown::verify::verify_pdf_result(document, options).unwrap();
    assert_eq!(verified.page_count, pages.len());
    assert_eq!(verified.pages.len(), pages.len());
    assert!(
        verified.findings.iter().all(|finding| {
            !finding.code.starts_with("pagination_")
                && finding.code != "paragraph_pagination_limited"
        }),
        "successful complete shape selection is reported consistently: {:?}",
        verified.findings
    );
    for marker in markers {
        let (painted_page, painted) = locate(pages, marker);
        let matches: Vec<_> = verified
            .pages
            .iter()
            .enumerate()
            .flat_map(|(page, runs)| {
                runs.runs
                    .iter()
                    .filter(|run| run.text.contains(marker.as_str()))
                    .map(move |run| (page, run))
            })
            .collect();
        assert_eq!(matches.len(), 1, "verified identity of {marker}");
        assert_eq!(matches[0].0, painted_page, "verified page of {marker}");
        assert!(
            (f64::from(matches[0].1.y) - painted.y).abs() < PRECISION,
            "verified baseline of {marker}"
        );
        assert!(
            (f64::from(matches[0].1.size) - painted.size).abs() < PRECISION,
            "verified font size of {marker}"
        );
    }
}

#[test]
fn shorter_shape_saves_a_body_page_and_reanchors_the_entire_continued_note() {
    let source = format!("{}\n\nCITATION.[^long]\n\n{}", prose(), note(40));
    let mut document = parse_markdown(&source);
    document.blocks.insert(1, Block::PageBreak);
    let options = options(false);
    let mut baseline_options = options.clone();
    baseline_options.optimal_pagination = false;
    let baseline =
        render_pdf_document_reported(&document, &baseline_options, PdfEmitOptions::default())
            .unwrap();
    let baseline_pages = painted_pages(&baseline.bytes);
    let result =
        render_pdf_document_reported(&document, &options, PdfEmitOptions::default()).unwrap();
    assert_eq!(
        result.pagination.paragraph_variants.status,
        PdfParagraphVariantStatus::Planned
    );
    assert_eq!(
        result.pagination.paragraph_variants.selected_alternatives,
        1
    );
    assert!(result.pagination.paragraph_variants.offered_paragraphs >= 1);
    assert!(result.pagination.paragraph_variants.failure.is_none());
    assert!(result.pagination.page_breaks.is_exact());
    let pages = painted_pages(&result.bytes);
    assert_eq!(body_prose(&pages[0]), (compact(&prose()), 13));
    assert_eq!(locate(&pages, "CITATION").0, 1);
    assert_eq!(locate(&baseline_pages, "CITATION").0, 2);
    assert_eq!(pages.len() + 1, baseline_pages.len());
    assert_eq!(locate(&pages, "NOTE000").0, locate(&pages, "CITATION").0);
    assert!(locate(&pages, "NOTE039").0 > locate(&pages, "CITATION").0);
    assert_markers(&pages, "NOTE", 40);
    assert!(!text(&pages).contains("Notes"));
    assert_geometry_and_folios(&pages, &options);
    assert_destinations(&result.bytes, &pages, &["NOTE000", "CITATION"]);
    let mut markers = (0..40).map(|i| format!("NOTE{i:03}")).collect::<Vec<_>>();
    markers.push("CITATION".into());
    assert_verifier_markers(&document, &options, &pages, &markers);
    assert_eq!(
        result,
        render_pdf_document_reported(&document, &options, PdfEmitOptions::monolithic(),).unwrap(),
        "both emitters replay the identical selected body and note pages"
    );
    assert_eq!(
        result,
        render_pdf_document_reported(&document, &options, PdfEmitOptions::default(),).unwrap(),
        "joint choice and continuation remain byte deterministic"
    );
}

#[test]
fn strict_fractional_grid_selects_the_shape_without_moving_or_duplicating_citations() {
    let source = format!(
        "{}\n\nFIRSTCLAIM.[^long]\n\nREPEATCLAIM.[^long]\n\n{}",
        prose(),
        note(40),
    );
    let mut document = parse_markdown(&source);
    document.blocks.insert(1, Block::PageBreak);
    let mut options = options(true);
    options.optimal_pagination = false;
    options.strict_pagination = true;
    let strict =
        render_pdf_document_reported(&document, &options, PdfEmitOptions::default()).unwrap();
    assert_eq!(
        strict.pagination.paragraph_variants.status,
        PdfParagraphVariantStatus::Planned
    );
    assert_eq!(
        strict.pagination.paragraph_variants.selected_alternatives,
        1
    );
    assert!(strict.pagination.page_breaks.is_exact());
    let pages = painted_pages(&strict.bytes);
    assert_eq!(body_prose(&pages[0]), (compact(&prose()), 13));
    assert_eq!(locate(&pages, "FIRSTCLAIM").0, 1);
    assert_eq!(locate(&pages, "NOTE000").0, locate(&pages, "FIRSTCLAIM").0);
    assert!(locate(&pages, "NOTE039").0 > locate(&pages, "REPEATCLAIM").0);
    assert_markers(&pages, "NOTE", 40);
    for marker in ["FIRSTCLAIM", "REPEATCLAIM"] {
        assert_eq!(text(&pages).matches(marker).count(), 1);
    }
    assert_geometry_and_folios(&pages, &options);
    assert_destinations(&strict.bytes, &pages, &["NOTE000", "NOTE000", "FIRSTCLAIM"]);
    let mut markers = (0..40).map(|i| format!("NOTE{i:03}")).collect::<Vec<_>>();
    markers.extend(["FIRSTCLAIM".into(), "REPEATCLAIM".into()]);
    assert_verifier_markers(&document, &options, &pages, &markers);
    options.strict_pagination = false;
    options.optimal_pagination = true;
    let optimal =
        render_pdf_document_reported(&document, &options, PdfEmitOptions::monolithic()).unwrap();
    assert_eq!(
        strict, optimal,
        "strict admission and emission mode must not alter a successful exact selection"
    );
}

#[test]
fn continued_note_with_no_body_alternatives_is_not_reported_as_unsupported() {
    let document = parse_markdown(&format!("CITATION.[^long]\n\n{}", note(40)));
    let options = options(false);
    let result =
        render_pdf_document_reported(&document, &options, PdfEmitOptions::default()).unwrap();
    assert_eq!(
        result.pagination.paragraph_variants.status,
        PdfParagraphVariantStatus::NoAlternatives
    );
    assert_eq!(
        result.pagination.paragraph_variants.selected_alternatives,
        0
    );
    assert!(!result.pagination.paragraph_variants.is_limited());
    assert!(result.pagination.page_breaks.is_exact());
    let pages = painted_pages(&result.bytes);
    assert_markers(&pages, "NOTE", 40);
    assert_eq!(locate(&pages, "CITATION").0, locate(&pages, "NOTE000").0);
    assert!(locate(&pages, "NOTE039").0 > locate(&pages, "NOTE000").0);
    assert_geometry_and_folios(&pages, &options);
}
