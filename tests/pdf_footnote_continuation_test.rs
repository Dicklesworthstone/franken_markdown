//! Multi-page footnotes retain every painted source line and navigation target.
//! Glyphs and coordinates are decoded from the emitted PDF, independently of layout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{
    BaselineGrid, PageMargins, PdfEmitOptions, PdfOptions, parse_markdown, render_pdf,
    render_pdf_document_emitted,
};

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
    content: String,
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
                content,
            }
        })
        .collect()
}

fn options() -> PdfOptions {
    let mut options = PdfOptions {
        split_footnotes: true,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 360.0;
    options.theme.page.size.height_pt = 360.0;
    options.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options.running.footer.center = Some("FOLIO {page}/{pages}".into());
    options
}

fn numbered_note(id: &str, prefix: &str, rows: usize) -> String {
    let lines = (0..rows)
        .map(|index| format!("{prefix}{index:03} preserved source line."))
        .collect::<Vec<_>>()
        .join("  \n    ");
    format!("[^{id}]: {lines}\n")
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

fn separator_count(page: &PaintedPage, content_width: f64) -> usize {
    let mut marks = Vec::new();
    let mut start = None;
    let mut end = None;
    let mut count = 0;
    for operation in pdf_content::operations(&page.content) {
        match operation.operator {
            "BDC" | "BMC" => marks.push(
                operation.operands[0] == "/Artifact" || marks.last().copied().unwrap_or(false),
            ),
            "EMC" => {
                marks.pop().expect("balanced artifact scope");
            }
            "m" | "l" if marks.last() == Some(&true) => {
                let point = (
                    operation.operands[0].parse::<f64>().unwrap(),
                    operation.operands[1].parse::<f64>().unwrap(),
                );
                if operation.operator == "m" {
                    start = Some(point);
                    end = None;
                } else {
                    end = Some(point);
                }
            }
            "S" => {
                if let (Some((x0, y0)), Some((x1, y1))) = (start.take(), end.take())
                    && marks.last() == Some(&true)
                    && (y1 - y0).abs() < PRECISION
                    && (x1 - x0 - content_width * 0.3).abs() < PRECISION
                {
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

fn assert_geometry(pages: &[PaintedPage], options: &PdfOptions) {
    let margins = options.theme.page.margins;
    for (index, page) in pages.iter().enumerate() {
        let body_bottom = page
            .runs
            .iter()
            .filter(|run| !run.artifact && run.size >= 10.9)
            .filter(|run| run.text.chars().any(|c| c.is_ascii_alphabetic()))
            .map(|run| run.y + run.descent * run.size)
            .fold(f64::INFINITY, f64::min);
        let mut has_note = false;
        let mut rows = BTreeMap::<i64, (f64, f64)>::new();
        let mut marked_note_baselines = BTreeSet::new();
        for run in page
            .runs
            .iter()
            .filter(|run| !run.artifact && run.text.chars().any(|c| c.is_ascii_alphabetic()))
        {
            let top = run.y + run.cap_height * run.size;
            let bottom = run.y + run.descent * run.size;
            let baseline = (run.y * 1000.0).round() as i64;
            rows.entry(baseline)
                .and_modify(|bounds| {
                    bounds.0 = bounds.0.min(bottom);
                    bounds.1 = bounds.1.max(top);
                })
                .or_insert((bottom, top));
            assert!(
                top <= page.height - f64::from(margins.top_pt) + 0.025
                    && bottom >= f64::from(margins.bottom_pt) - 0.025,
                "page {} has out-of-bounds ink: {run:?}",
                index + 1,
            );
            assert!(
                run.x >= f64::from(margins.left_pt) - 0.025
                    && run.x <= page.width - f64::from(margins.right_pt) + 0.025,
                "page {} has out-of-bounds horizontal origin: {run:?}",
                index + 1,
            );
            if run.size < 10.9 {
                has_note = true;
                if ["NOTE", "ALPHA", "BETA", "STYLE"]
                    .iter()
                    .any(|marker| run.text.contains(marker))
                {
                    assert!(
                        marked_note_baselines.insert(baseline),
                        "page {} paints separate source rows on one baseline: {run:?}",
                        index + 1,
                    );
                }
                assert!(
                    top < body_bottom,
                    "page {} has note/body overlap: {run:?}, body bottom {body_bottom}",
                    index + 1,
                );
            }
        }
        let rows: Vec<_> = rows.into_values().collect();
        for adjacent in rows.windows(2) {
            assert!(
                adjacent[0].1 <= adjacent[1].0 + 0.025,
                "page {} overlaps successive text rows: {adjacent:?}",
                index + 1,
            );
        }
        if has_note {
            let width = page.width - f64::from(margins.left_pt + margins.right_pt);
            assert_eq!(separator_count(page, width), 1, "page {}", index + 1);
        }
        let footer: String = page
            .runs
            .iter()
            .filter(|run| run.artifact)
            .map(|run| run.text.as_str())
            .collect();
        assert_eq!(footer, format!("FOLIO {}/{}", index + 1, pages.len()));
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

#[test]
fn long_note_continues_through_body_and_note_only_pages_without_lost_or_repeated_text() {
    let mut source = String::from("BODY000 first citation.[^long]\n\n");
    for index in 1..16 {
        source.push_str(&format!("BODY{index:03} normal body paragraph.\n\n"));
    }
    source.push_str(&numbered_note("long", "NOTE", 80));
    let options = options();
    let document = parse_markdown(&source);
    let bytes =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let pages = painted_pages(&bytes);
    assert!(
        pages.len() >= 3,
        "fixture must cross multiple page boundaries"
    );
    assert_markers(&pages, "BODY", 16);
    assert_markers(&pages, "NOTE", 80);
    assert_eq!(locate(&pages, "NOTE000").0, locate(&pages, "BODY000").0);
    assert!(locate(&pages, "NOTE079").0 > locate(&pages, "BODY015").0);
    let note_pages: BTreeSet<_> = (0..80)
        .map(|index| {
            let (page, run) = locate(&pages, &format!("NOTE{index:03}"));
            assert!(run.size < 11.0, "continuations retain footnote size");
            page
        })
        .collect();
    assert!(note_pages.len() >= 3);
    let mut note_rows = vec![0; pages.len()];
    for index in 0..80 {
        note_rows[locate(&pages, &format!("NOTE{index:03}")).0] += 1;
    }
    let last_body_page = locate(&pages, "BODY015").0;
    let largest_body_foot = note_rows[..=last_body_page].iter().copied().max().unwrap();
    assert!(
        note_rows[last_body_page + 1..pages.len() - 1]
            .iter()
            .any(|&rows| rows > largest_body_foot),
        "non-final note-only pages use more than the body-page foot allowance: {note_rows:?}",
    );
    assert!(
        !text(&pages).contains("Notes"),
        "referenced prose stays at page feet"
    );
    assert_geometry(&pages, &options);
    assert_destinations(&bytes, &pages, &["NOTE000"]);
    assert_eq!(
        String::from_utf8_lossy(&bytes).matches("/S /Note ").count(),
        1
    );
    let verified = franken_markdown::verify::verify_pdf(&document, &options).unwrap();
    assert_eq!(
        verified.page_count,
        pages.len(),
        "verification counts note-only tail pages"
    );
    assert_eq!(
        bytes,
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap(),
        "both emitters walk exactly the same continuation pages",
    );
    assert_eq!(bytes, render_pdf(&source, &options).unwrap());
    let browser = franken_markdown::wasm::render_pdf(
        &source,
        &franken_markdown::wasm::WasmRenderOptions {
            theme: options.theme.clone(),
            running: options.running.clone(),
            split_footnotes: true,
            metadata_epoch_seconds: options.metadata_epoch_seconds,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        bytes, browser.bytes,
        "browser core uses the same continuation pager"
    );
}

#[test]
fn two_notes_on_one_citing_line_and_a_later_note_keep_all_fragments_and_links() {
    let mut source = String::from("FIRSTCLAIM.[^alpha][^beta]\n\n");
    for index in 0..10 {
        source.push_str(&format!("BODY{index:03} ordinary continuation.\n\n"));
    }
    source.push_str("LATERCLAIM.[^short]\n\n");
    source.push_str(&numbered_note("alpha", "ALPHA", 36));
    source.push('\n');
    source.push_str(&numbered_note("beta", "BETA", 36));
    source.push_str("\n[^short]: SHORTNOTE keeps its later citation.\n");
    let options = options();
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "ALPHA", 36);
    assert_markers(&pages, "BETA", 36);
    assert_markers(&pages, "BODY", 10);
    let citing_page = locate(&pages, "FIRSTCLAIM").0;
    assert_eq!(locate(&pages, "ALPHA000").0, citing_page);
    assert_eq!(locate(&pages, "BETA000").0, citing_page);
    assert_eq!(
        locate(&pages, "SHORTNOTE").0,
        locate(&pages, "LATERCLAIM").0
    );
    assert_eq!(text(&pages).matches("SHORTNOTE").count(), 1);
    assert_geometry(&pages, &options);
    assert_destinations(&bytes, &pages, &["ALPHA000", "BETA000", "SHORTNOTE"]);
    assert_eq!(
        String::from_utf8_lossy(&bytes).matches("/S /Note ").count(),
        3
    );
}

#[test]
fn oversubscribed_first_fragments_keep_complete_notes_and_real_destinations() {
    let mut source = String::from("CROWDCLAIM");
    for index in 0..9 {
        source.push_str(&format!("[^n{index}]"));
    }
    source.push_str(".\n\n");
    for index in 0..9 {
        source.push_str(&numbered_note(
            &format!("n{index}"),
            &format!("EDGE{index}ROW"),
            8,
        ));
        source.push('\n');
    }
    let mut options = options();
    options.theme.page.size.height_pt = 192.0;
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    let citing_page = locate(&pages, "CROWDCLAIM").0;
    let mut continued = 0;
    let mut fallback = 0;
    let targets: Vec<_> = (0..9).map(|index| format!("EDGE{index}ROW000")).collect();
    for (index, marker) in targets.iter().enumerate() {
        assert_markers(&pages, &format!("EDGE{index}ROW"), 8);
        let (page, run) = locate(&pages, marker);
        if run.size < 11.0 {
            continued += 1;
            assert_eq!(page, citing_page, "admitted first fragment {marker}");
        } else {
            fallback += 1;
        }
    }
    assert!(
        continued > 0 && fallback > 0,
        "fixture must cross first-fragment admission: {continued}/{fallback}"
    );
    assert!(text(&pages).contains("Notes"));
    assert_geometry(&pages, &options);
    let target_refs: Vec<_> = targets.iter().map(String::as_str).collect();
    assert_destinations(&bytes, &pages, &target_refs);
    assert_eq!(
        String::from_utf8_lossy(&bytes).matches("/S /Note ").count(),
        9
    );
}

#[test]
fn inline_styles_and_late_backlink_survive_the_split() {
    let mut source = String::from("CITINGLINE.[^long]\n\n");
    let lines = (0..60)
        .map(|index| format!("STYLE{index:03} *emphasis* **bold** `code` café."))
        .collect::<Vec<_>>()
        .join("  \n    ");
    source.push_str(&format!(
        "[^long]: {lines}\n\n    [LASTLINK](https://example.org/note)\n"
    ));
    let mut options = options();
    options.footnote_backlinks = true;
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "STYLE", 60);
    assert!(locate(&pages, "STYLE059").0 > locate(&pages, "STYLE000").0);
    assert_eq!(text(&pages).matches("LASTLINK").count(), 1);
    assert!(String::from_utf8_lossy(&bytes).contains("/URI (https://example.org/note)"));
    assert_destinations(&bytes, &pages, &["STYLE000", "CITINGLINE"]);
    assert_geometry(&pages, &options);
    assert_eq!(
        String::from_utf8_lossy(&bytes).matches("/S /Note ").count(),
        1
    );
}

#[test]
fn rich_and_unreferenced_notes_keep_the_complete_existing_fallback() {
    let mut source = String::from("CITATION.[^rich]\n\n[^rich]: RICHSTART\n\n    ```text\n");
    for index in 0..40 {
        source.push_str(&format!("    CODE{index:03} retained code row.\n"));
    }
    source.push_str("    ```\n\n[^orphan]: ORPHAN remains an endnote.\n");
    let split = options();
    let mut original = split.clone();
    original.split_footnotes = false;
    let bytes = render_pdf(&source, &split).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "CODE", 40);
    assert!(text(&pages).contains("Notes"));
    assert_eq!(text(&pages).matches("RICHSTART").count(), 1);
    assert_eq!(text(&pages).matches("ORPHAN").count(), 1);
    pdf_content::assert_same_paint(&bytes, &render_pdf(&source, &original).unwrap());
    assert_destinations(&bytes, &pages, &["RICHSTART"]);
}

#[test]
fn book_local_note_ids_and_backlinks_survive_continuation_pages() {
    let first = format!(
        "# FIRSTCHAPTER\n\nFIRSTCITATION.[^n]\n\n{}",
        numbered_note("n", "FIRSTNOTE", 48),
    );
    let second = format!(
        "# SECONDCHAPTER\n\nSECONDCITATION.[^n]\n\n{}",
        numbered_note("n", "SECONDNOTE", 48),
    );
    let mut book = BookRenderer::new(&[
        BookInput {
            path: "first.md".into(),
            source: first,
        },
        BookInput {
            path: "second.md".into(),
            source: second,
        },
    ])
    .unwrap();
    let options = options();
    book.options_mut().theme = options.theme.clone();
    book.options_mut().running = options.running.clone();
    book.options_mut().metadata_epoch_seconds = options.metadata_epoch_seconds;
    book.options_mut().split_footnotes = true;
    book.options_mut().footnote_backlinks = true;
    let bytes = book.render_pdf().unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "FIRSTNOTE", 48);
    assert_markers(&pages, "SECONDNOTE", 48);
    assert_eq!(
        locate(&pages, "FIRSTNOTE000").0,
        locate(&pages, "FIRSTCITATION").0
    );
    assert_eq!(
        locate(&pages, "SECONDNOTE000").0,
        locate(&pages, "SECONDCITATION").0
    );
    assert!(locate(&pages, "FIRSTNOTE047").0 > locate(&pages, "FIRSTNOTE000").0);
    assert!(locate(&pages, "SECONDNOTE047").0 > locate(&pages, "SECONDNOTE000").0);
    assert_destinations(
        &bytes,
        &pages,
        &[
            "FIRSTNOTE000",
            "SECONDNOTE000",
            "FIRSTCITATION",
            "SECONDCITATION",
        ],
    );
    assert_geometry(&pages, &options);
    assert_eq!(
        String::from_utf8_lossy(&bytes).matches("/S /Note ").count(),
        2
    );
    assert_eq!(bytes, book.render_pdf().unwrap());
}

#[test]
fn incompatible_pagination_modes_fail_explicitly_without_weakened_constraints() {
    let source = format!("CITATION.[^long]\n\n{}", numbered_note("long", "NOTE", 40));
    for (name, mut options) in [
        ("optimal", options()),
        ("strict", options()),
        ("grid", options()),
    ] {
        match name {
            "optimal" => options.optimal_pagination = true,
            "strict" => options.strict_pagination = true,
            "grid" => {
                options.theme.page.baseline_grid = Some(BaselineGrid::from_points(13.333).unwrap());
            }
            _ => unreachable!(),
        }
        let error = render_pdf(&source, &options).unwrap_err().to_string();
        assert!(
            error.contains("pdf_split_footnotes_incompatible"),
            "{name}: {error}",
        );
    }
}
