//! Multi-page footnotes retain every painted source line and navigation target.
//! Glyphs and coordinates are decoded from the emitted PDF, independently of layout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::ast::{DefinitionItem, List, ListItem};
use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{
    BaselineGrid, Block, Document, Inline, PageMargins, PdfEmitOptions, PdfOptions, Profile,
    parse_markdown, parse_markdown_with_profile, render_pdf, render_pdf_document_emitted,
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
            .filter(|run| run.artifact && run.text.starts_with("FOLIO "))
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

/// Read the physical bounds of filled artifact paths. The fixtures use the
/// default white page, so these are emitted code/quote panels and inline chips,
/// not a page-colour rectangle. Control points of their rounded corners stay
/// within the enclosing rectangle and give its exact outer bounds.
fn artifact_fills(page: &PaintedPage) -> Vec<[f64; 4]> {
    let mut artifacts = Vec::new();
    let mut path: Option<[f64; 4]> = None;
    let mut fills = Vec::new();
    for operation in pdf_content::operations(&page.content) {
        match operation.operator {
            "BDC" | "BMC" => artifacts.push(
                operation.operands[0] == "/Artifact" || artifacts.last().copied().unwrap_or(false),
            ),
            "EMC" => {
                artifacts.pop().expect("balanced artifact scope");
            }
            "cm" => panic!("extend the bounds reader before accepting transformed fixtures"),
            "m" | "l" | "c" if artifacts.last() == Some(&true) => {
                for pair in operation.operands.as_chunks::<2>().0 {
                    let x = pair[0].parse::<f64>().unwrap();
                    let y = pair[1].parse::<f64>().unwrap();
                    let bounds = path.get_or_insert([x, y, x, y]);
                    bounds[0] = bounds[0].min(x);
                    bounds[1] = bounds[1].min(y);
                    bounds[2] = bounds[2].max(x);
                    bounds[3] = bounds[3].max(y);
                }
            }
            "re" if artifacts.last() == Some(&true) => {
                let values: Vec<f64> = operation
                    .operands
                    .iter()
                    .map(|value| value.parse().unwrap())
                    .collect();
                let [x, y, width, height] = values.as_slice() else {
                    panic!("complete rectangle operands");
                };
                path = Some([*x, *y, x + width, y + height]);
            }
            "v" | "y" => panic!("extend the bounds reader before accepting shorthand curves"),
            "f" | "f*" => {
                if artifacts.last() == Some(&true)
                    && let Some(bounds) = path.take()
                {
                    fills.push(bounds);
                }
                path = None;
            }
            "S" | "s" | "n" | "B" | "B*" | "b" | "b*" => path = None,
            _ => {}
        }
    }
    assert!(artifacts.is_empty());
    fills
}

fn assert_panel_bounds(pages: &[PaintedPage], options: &PdfOptions) {
    let margins = options.theme.page.margins;
    let mut panels = 0;
    for (index, page) in pages.iter().enumerate() {
        let body_bottom = page
            .runs
            .iter()
            .filter(|run| !run.artifact && run.size >= 10.9)
            .filter(|run| run.text.chars().any(|c| c.is_ascii_alphabetic()))
            .map(|run| run.y + run.descent * run.size)
            .fold(f64::INFINITY, f64::min);
        for [left, bottom, right, top] in artifact_fills(page) {
            panels += 1;
            assert!(
                left >= f64::from(margins.left_pt) - 0.025
                    && right <= page.width - f64::from(margins.right_pt) + 0.025
                    && bottom >= f64::from(margins.bottom_pt) - 0.025
                    && top <= page.height - f64::from(margins.top_pt) + 0.025,
                "page {} paints a panel outside the content box: {left}/{bottom}/{right}/{top}",
                index + 1,
            );
            assert!(
                top < body_bottom,
                "page {} paints a note panel into body ink",
                index + 1,
            );
        }
    }
    assert!(panels > 0, "fixture must exercise actual painted panels");
}

fn structure_count(pdf: &[u8], tag: &str) -> usize {
    let objects = Objects::new(pdf);
    let marker = format!("/S /{tag} ");
    (1..objects.offsets.len())
        .filter(|&id| {
            let dictionary = objects.dictionary(id);
            dictionary.contains("/Type /StructElem ") && dictionary.contains(&marker)
        })
        .count()
}

fn assert_first_note_navigation(pdf: &[u8], pages: &[PaintedPage], citation: &str) {
    let (body_page, body) = locate(pages, citation);
    let (label_page, label) = pages
        .iter()
        .enumerate()
        .find_map(|(index, page)| {
            page.runs
                .iter()
                .find(|run| !run.artifact && run.size < 10.9 && run.text.trim() == "¹")
                .map(|run| (index, run))
        })
        .expect("first-block rich note keeps its standalone numbered prefix");
    assert_eq!(label_page, body_page);
    let mut actual = destinations(pdf, pages);
    assert_eq!(actual.len(), 2, "citation and optional return navigation");
    for (page, run) in [(label_page, label), (body_page, body)] {
        let y = run.y + run.size * 0.9;
        let index = actual
            .iter()
            .position(|&(target_page, target_y)| {
                page == target_page && (target_y - y).abs() < 0.025
            })
            .expect("navigation binds to the actual painted prefix or citation");
        actual.remove(index);
    }
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
fn table_and_code_note_continues_while_unreferenced_notes_keep_their_fallback() {
    let mut source = String::from("CITATION.[^rich]\n\n[^rich]: RICHSTART\n\n    ```text\n");
    for index in 0..40 {
        source.push_str(&format!("    CODE{index:03} retained code row.\n"));
    }
    source.push_str(
        "    ```\n\n    | Column |\n    | --- |\n    | TABLECELL |\n\n[^orphan]: ORPHAN remains an endnote.\n",
    );
    let split = options();
    let bytes = render_pdf(&source, &split).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "CODE", 40);
    assert!(text(&pages).contains("Notes"));
    assert_eq!(text(&pages).matches("RICHSTART").count(), 1);
    assert_eq!(text(&pages).matches("TABLECELL").count(), 1);
    assert_eq!(text(&pages).matches("ORPHAN").count(), 1);
    assert_eq!(locate(&pages, "RICHSTART").0, locate(&pages, "CITATION").0);
    assert!(locate(&pages, "CODE039").0 > locate(&pages, "RICHSTART").0);
    assert!(locate(&pages, "TABLECELL").1.size < 10.0);
    assert!(locate(&pages, "ORPHAN").1.size >= 10.9);
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

#[test]
fn first_block_code_note_continues_with_numbering_panels_and_return_navigation() {
    let mut source = String::from("CODECITATION.[^rich]\n\n");
    for index in 0..8 {
        source.push_str(&format!("BODY{index:03} ordinary document paragraph.\n\n"));
    }
    source.push_str("[^rich]:\n    ```rust\n");
    for index in 0..48 {
        if index > 0 && index % 8 == 0 {
            source.push_str("    \n");
        }
        source.push_str(&format!("    let item = \"CODE{index:03}\";\n"));
    }
    source.push_str("    ```\n");
    let mut options = options();
    options.footnote_backlinks = true;
    options.code_line_numbers = true;
    let document = parse_markdown(&source);
    let bytes =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "CODE", 48);
    assert_markers(&pages, "BODY", 8);
    assert_eq!(
        locate(&pages, "CODE000").0,
        locate(&pages, "CODECITATION").0
    );
    assert!(locate(&pages, "CODE047").0 > locate(&pages, "CODE000").0);
    assert!(!text(&pages).contains("Notes"));
    for index in 0..48 {
        let (page, row) = locate(&pages, &format!("CODE{index:03}"));
        let number = (index + 1 + index / 8).to_string();
        assert!(
            pages[page].runs.iter().any(|run| {
                !run.artifact && run.text.trim() == number && (run.y - row.y).abs() < PRECISION
            }),
            "source code row {index} retains its original line number",
        );
        assert!(
            artifact_fills(&pages[page]).iter().any(|bounds| {
                bounds[0] <= row.x && row.x <= bounds[2] && bounds[1] < row.y && row.y < bounds[3]
            }),
            "source code row {index} retains its painted panel",
        );
    }
    let numbers: BTreeMap<_, _> = pages
        .iter()
        .flat_map(|page| &page.runs)
        .filter(|run| !run.artifact)
        .filter_map(|run| run.text.trim().parse::<usize>().ok())
        .fold(BTreeMap::new(), |mut counts, number| {
            *counts.entry(number).or_insert(0usize) += 1;
            counts
        });
    assert_eq!(
        numbers,
        (1..=53).map(|number| (number, 1)).collect(),
        "all source rows, including blank lines, retain one line number",
    );
    assert_eq!(structure_count(&bytes, "Note"), 1);
    assert_eq!(structure_count(&bytes, "Code"), 1);
    assert_first_note_navigation(&bytes, &pages, "CODECITATION");
    assert_geometry(&pages, &options);
    assert_panel_bounds(&pages, &options);
    assert_eq!(
        bytes,
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap(),
    );
    assert_eq!(
        franken_markdown::verify::verify_pdf(&document, &options)
            .unwrap()
            .page_count,
        pages.len(),
    );
}

#[test]
fn nested_lists_and_alert_note_keep_source_labels_containers_and_panel_bounds() {
    let mut source = String::from(
        "LISTCITATION.[^rich]\n\n[^rich]:\n    > [!NOTE]\n    > QUOTEINTRO retained introduction.\n    >\n",
    );
    for index in 0..12 {
        let marker = format!("{}.", index + 1);
        let indent = " ".repeat(marker.len() + 1);
        source.push_str(&format!(
            "    > {marker} NOTE_LIST{index:03} outer item.\n    > {indent}- NOTE_NEST{index:03} inner item.\n",
        ));
    }
    for height in [192.0, 216.0, 240.0] {
        let mut options = options();
        options.theme.page.size.height_pt = height;
        options.footnote_backlinks = true;
        let bytes = render_pdf(&source, &options).unwrap();
        let pages = painted_pages(&bytes);
        assert_markers(&pages, "NOTE_LIST", 12);
        assert_markers(&pages, "NOTE_NEST", 12);
        assert_eq!(
            locate(&pages, "QUOTEINTRO").0,
            locate(&pages, "LISTCITATION").0
        );
        assert_eq!(locate(&pages, "Note").0, locate(&pages, "QUOTEINTRO").0);
        assert!(locate(&pages, "NOTE_NEST011").0 > locate(&pages, "QUOTEINTRO").0);
        assert!(!text(&pages).contains("Notes"));
        for index in 0..12 {
            let (page, row) = locate(&pages, &format!("NOTE_LIST{index:03}"));
            let label = format!("{}.", index + 1);
            assert!(
                pages[page].runs.iter().any(|run| {
                    !run.artifact && run.text.trim() == label && (run.y - row.y).abs() < PRECISION
                }),
                "ordered item {index} retains its source marker on the same row",
            );
        }
        assert_eq!(structure_count(&bytes, "Note"), 1);
        assert_eq!(structure_count(&bytes, "BlockQuote"), 1);
        assert_eq!(structure_count(&bytes, "L"), 13);
        assert_eq!(structure_count(&bytes, "LI"), 24);
        assert_eq!(structure_count(&bytes, "Lbl"), 25);
        assert_first_note_navigation(&bytes, &pages, "LISTCITATION");
        assert_geometry(&pages, &options);
        assert_panel_bounds(&pages, &options);
    }
}

#[test]
fn first_block_list_marker_and_alert_title_stay_with_the_first_source_row() {
    let mut source = String::from(
        "NESTEDCITATION.[^rich]\n\n[^rich]:\n    - > [!WARNING]\n      > FIRSTSOURCE retained warning.\n      >\n",
    );
    for index in 0..36 {
        source.push_str(&format!(
            "      > NOTE_QUOTE{index:03} preserved source row.  \n"
        ));
    }
    let mut options = options();
    options.theme.page.size.height_pt = 192.0;
    options.footnote_backlinks = true;
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "NOTE_QUOTE", 36);
    let citation_page = locate(&pages, "NESTEDCITATION").0;
    assert_eq!(locate(&pages, "FIRSTSOURCE").0, citation_page);
    assert_eq!(locate(&pages, "Warning").0, citation_page);
    assert!(locate(&pages, "NOTE_QUOTE035").0 > citation_page);
    assert!(!text(&pages).contains("Notes"));
    assert_eq!(structure_count(&bytes, "Note"), 1);
    assert_eq!(structure_count(&bytes, "L"), 1);
    assert_eq!(structure_count(&bytes, "LI"), 1);
    assert_eq!(structure_count(&bytes, "Lbl"), 2);
    assert_eq!(structure_count(&bytes, "BlockQuote"), 1);
    assert_first_note_navigation(&bytes, &pages, "NESTEDCITATION");
    assert_geometry(&pages, &options);
    assert_panel_bounds(&pages, &options);
}

fn table_note_source(rows: usize, prefix: &str) -> String {
    let mut source = String::from("TABLECITATION.[^data]\n\n");
    for index in 0..8 {
        source.push_str(&format!("BODY{index:03} ordinary body paragraph.\n\n"));
    }
    source.push_str("[^data]:\n");
    source.push_str(&format!(
        "{prefix}| KEYHEADER | VALUEHEADER |\n{prefix}| --- | --- |\n"
    ));
    for index in 0..rows {
        source.push_str(&format!(
            "{prefix}| ROW{index:03} | START{index:03} complete measured content with enough words to wrap across physical text lines while retaining the whole source row END{index:03} |\n"
        ));
    }
    source
}

fn assert_table_continuation(pages: &[PaintedPage], rows: usize) {
    assert_markers(pages, "ROW", rows);
    assert_markers(pages, "START", rows);
    assert_markers(pages, "END", rows);
    assert!(
        !text(pages).contains("Notes"),
        "table stays in the page-foot flow"
    );
    let first_page = locate(pages, "ROW000").0;
    assert_eq!(first_page, locate(pages, "TABLECITATION").0);
    let mut table_pages = BTreeSet::new();
    for index in 0..rows {
        let (key_page, key) = locate(pages, &format!("ROW{index:03}"));
        let (start_page, start) = locate(pages, &format!("START{index:03}"));
        let (end_page, end) = locate(pages, &format!("END{index:03}"));
        assert_eq!(
            key_page, start_page,
            "source row {index} starts with its key"
        );
        assert_eq!(start_page, end_page, "source row {index} must stay atomic");
        assert!(
            start.y > end.y + 1.0,
            "fixture row {index} must actually wrap"
        );
        assert!(key.size < 10.0, "continued rows keep footnote type size");
        table_pages.insert(key_page);
    }
    assert!(
        table_pages.len() >= 3,
        "fixture must exercise multiple table fragments"
    );
    for page_index in table_pages {
        let page = &pages[page_index];
        for label in ["KEYHEADER", "VALUEHEADER"] {
            let headers: Vec<_> = page
                .runs
                .iter()
                .filter(|run| run.text.contains(label))
                .collect();
            assert_eq!(
                headers.len(),
                1,
                "page {} has one complete header",
                page_index + 1
            );
            assert_eq!(headers[0].artifact, page_index != first_page);
            assert!(
                page.runs
                    .iter()
                    .filter(|run| run.text.contains("ROW"))
                    .all(|run| run.y < headers[0].y),
                "the header precedes every source row on page {}",
                page_index + 1,
            );
            assert!(headers[0].y < page.height - 36.0 && headers[0].y > 36.0);
        }
        let repeated: String = page
            .runs
            .iter()
            .filter(|run| run.artifact && !run.text.starts_with("FOLIO "))
            .map(|run| run.text.as_str())
            .collect();
        assert_eq!(
            repeated,
            if page_index == first_page {
                ""
            } else {
                "KEYHEADERVALUEHEADER"
            }
        );
    }
    assert_eq!(
        text(pages).matches("KEYHEADER").count(),
        1,
        "one accessible source header"
    );
    assert_eq!(
        text(pages).matches("VALUEHEADER").count(),
        1,
        "one accessible source header"
    );
}

fn assert_table_note_structure(pdf: &[u8], rows: usize) {
    assert_eq!(structure_count(pdf, "Note"), 1);
    assert_eq!(structure_count(pdf, "Table"), 1);
    assert_eq!(structure_count(pdf, "TR"), rows + 1);
    assert_eq!(structure_count(pdf, "TH"), 2);
    assert_eq!(structure_count(pdf, "TD"), rows * 2);
    let objects = Objects::new(pdf);
    let mut checked = 0;
    for id in 1..objects.offsets.len() {
        if !objects
            .dictionary(id)
            .contains("/Type /StructElem /S /Table ")
        {
            continue;
        }
        let mut parent = reference_after(objects.dictionary(id), "/P ");
        let mut found_note = false;
        for _ in 0..16 {
            let dictionary = objects.dictionary(parent);
            if dictionary.contains("/S /Note ") {
                found_note = true;
                break;
            }
            if !dictionary.contains("/Type /StructElem ") {
                break;
            }
            parent = reference_after(dictionary, "/P ");
        }
        assert!(
            found_note,
            "the logical table remains inside its source Note"
        );
        checked += 1;
    }
    assert_eq!(checked, 1);
}

#[test]
fn tables_continue_at_complete_rows_with_repeated_headers_and_source_owned_tags() {
    let rows = 18;
    let source = table_note_source(rows, "    ");
    let mut options = options();
    options.footnote_backlinks = true;
    let document = parse_markdown(&source);
    let bytes =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let pages = painted_pages(&bytes);
    assert_table_continuation(&pages, rows);
    assert_table_note_structure(&bytes, rows);
    assert_geometry(&pages, &options);
    assert_first_note_navigation(&bytes, &pages, "TABLECITATION");
    let verified = franken_markdown::verify::verify_pdf(&document, &options).unwrap();
    assert_eq!(verified.page_count, pages.len());
    assert_eq!(
        bytes,
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap(),
        "both PDF writers retain the same table headers, rows and page counts",
    );
    assert_eq!(bytes, render_pdf(&source, &options).unwrap());
    let browser = franken_markdown::wasm::render_pdf(
        &source,
        &franken_markdown::wasm::WasmRenderOptions {
            theme: options.theme.clone(),
            running: options.running.clone(),
            split_footnotes: true,
            footnote_backlinks: true,
            metadata_epoch_seconds: options.metadata_epoch_seconds,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bytes, browser.bytes);
}

#[test]
fn nested_table_notes_retain_quote_geometry_and_literal_cell_padding() {
    let rows = 12;
    let source = table_note_source(rows, "      > ").replacen(
        "      > | KEYHEADER",
        "    - > | KEYHEADER",
        1,
    );
    let mut options = options();
    options.use_theme_spacing = true;
    options.theme.spacing.table_cell_padding_y_em = 0.6;
    options.footnote_backlinks = true;
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert_table_continuation(&pages, rows);
    assert_table_note_structure(&bytes, rows);
    assert_eq!(structure_count(&bytes, "BlockQuote"), 1);
    assert_eq!(structure_count(&bytes, "L"), 1);
    assert_eq!(structure_count(&bytes, "LI"), 1);
    assert_geometry(&pages, &options);
    assert_panel_bounds(&pages, &options);
    assert_first_note_navigation(&bytes, &pages, "TABLECITATION");
}

#[test]
fn oversized_table_rows_and_headers_preserve_the_complete_endnote_fallback() {
    let long = (0..140)
        .map(|index| format!("SOURCE{index:03} "))
        .collect::<String>();
    for (header, row) in [
        ("Column".to_string(), long.clone()),
        (long.clone(), "LASTROW".to_string()),
    ] {
        let source = format!(
            "CITATION.[^oversized]\n\n[^oversized]:\n    | {header} |\n    | --- |\n    | {row} |\n"
        );
        let options = options();
        let bytes = render_pdf(&source, &options).unwrap();
        let pages = painted_pages(&bytes);
        assert!(
            text(&pages).contains("Notes"),
            "indivisible table unit cannot fit"
        );
        assert_markers(&pages, "SOURCE", 140);
        let mut original = options.clone();
        original.split_footnotes = false;
        pdf_content::assert_same_paint(&bytes, &render_pdf(&source, &original).unwrap());
    }
}

#[test]
fn simultaneous_table_notes_reserve_both_first_rows_and_keep_their_own_headers() {
    let rows = 16;
    let mut source = String::from("MIXEDCITATION.[^alpha][^beta]\n\n");
    for (id, header, prefix) in [
        ("alpha", "AHEADER", "ALPHAROW"),
        ("beta", "BHEADER", "BETAROW"),
    ] {
        source.push_str(&format!("[^{id}]:\n    | {header} |\n    | --- |\n"));
        for index in 0..rows {
            source.push_str(&format!("    | {prefix}{index:03} preserved cell |\n"));
        }
        source.push('\n');
    }
    let mut options = options();
    options.theme.page.size.height_pt = 240.0;
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert!(!text(&pages).contains("Notes"));
    for (header, prefix) in [("AHEADER", "ALPHAROW"), ("BHEADER", "BETAROW")] {
        assert_markers(&pages, prefix, rows);
        assert_eq!(locate(&pages, &format!("{prefix}000")).0, 0);
        let mut seen_pages = BTreeSet::new();
        for index in 0..rows {
            let (page, row) = locate(&pages, &format!("{prefix}{index:03}"));
            let headers: Vec<_> = pages[page]
                .runs
                .iter()
                .filter(|run| run.text.contains(header))
                .collect();
            assert_eq!(headers.len(), 1);
            assert!(headers[0].y > row.y);
            assert_eq!(headers[0].artifact, page != 0);
            seen_pages.insert(page);
        }
        assert!(seen_pages.len() >= 2);
        assert_eq!(text(&pages).matches(header).count(), 1);
    }
    assert_eq!(structure_count(&bytes, "Note"), 2);
    assert_eq!(structure_count(&bytes, "Table"), 2);
    assert_eq!(structure_count(&bytes, "TH"), 2);
    assert_eq!(structure_count(&bytes, "TD"), rows * 2);
    assert_geometry(&pages, &options);
}

#[test]
fn financial_table_note_footings_stay_with_their_rows_and_never_repeat() {
    let rows = 20;
    let mut source = String::from(
        "CITATION.[^statement]\n\n[^statement]:\n    | Item | Amount |\n    | --- | ---. |\n",
    );
    for index in 0..rows {
        source.push_str(&format!(
            "    | ENTRY{index:03} | {index}.125 |\n    | | === |\n",
        ));
    }
    let options = options();
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "ENTRY", rows);
    assert!(!text(&pages).contains("Notes") && !text(&pages).contains("==="));
    let rules: Vec<Vec<[f64; 4]>> = pages
        .iter()
        .map(|page| {
            page.content
                .split("/Artifact BMC\n")
                .skip(1)
                .filter_map(|block| {
                    let artifact = block.split_once("\nEMC\n").unwrap().0;
                    let coordinates = artifact.split_once(" w ")?.1;
                    let fields: Vec<_> = coordinates.split_whitespace().collect();
                    if fields.len() != 7 || fields[2] != "m" || fields[5..] != ["l", "S"] {
                        return None;
                    }
                    let rule = [0, 1, 3, 4].map(|index| fields[index].parse::<f64>().unwrap());
                    (rule[0] > 36.01).then_some(rule)
                })
                .collect()
        })
        .collect();
    assert_eq!(rules.iter().map(Vec::len).sum::<usize>(), rows * 2);
    for index in 0..rows {
        let (page, row) = locate(&pages, &format!("ENTRY{index:03}"));
        let attached: Vec<_> = rules[page]
            .iter()
            .filter(|rule| row.y > rule[1] && row.y - rule[1] < 9.0)
            .collect();
        assert_eq!(
            attached.len(),
            2,
            "both footing strokes stay with source row {index}"
        );
        assert!((attached[0][1] - attached[1][1] - 2.0).abs() < 0.015);
        for rule in attached {
            assert_eq!(rule[1], rule[3]);
            assert!(rule[1] - 0.35 >= 35.99 && rule[2] <= 324.01);
        }
    }
    assert_eq!(structure_count(&bytes, "TR"), rows + 1);
    assert_eq!(structure_count(&bytes, "TD"), rows * 2);
    assert_geometry(&pages, &options);
    assert_panel_bounds(&pages, &options);
}

#[test]
fn header_only_table_note_keeps_its_label_without_empty_continuation_pages() {
    let source = "EMPTYCITATION.[^empty]\n\n[^empty]:\n    | EMPTYHEADER |\n    | --- |\n";
    let mut options = options();
    options.footnote_backlinks = true;
    let bytes = render_pdf(source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert_eq!(pages.len(), 1);
    assert!(!text(&pages).contains("Notes"));
    assert_eq!(text(&pages).matches("EMPTYHEADER").count(), 1);
    assert_eq!(structure_count(&bytes, "Note"), 1);
    assert_eq!(structure_count(&bytes, "Table"), 1);
    assert_eq!(structure_count(&bytes, "TH"), 1);
    assert_eq!(structure_count(&bytes, "TD"), 0);
    assert_first_note_navigation(&bytes, &pages, "EMPTYCITATION");
    assert_geometry(&pages, &options);
}

#[test]
fn sequential_tables_in_one_note_repeat_only_their_own_headers() {
    let rows = 10;
    let mut source = String::from("CITATION.[^tables]\n\n[^tables]:\n");
    for (header, prefix) in [("FIRSTHEADER", "FIRSTCELL"), ("LASTHEADER", "LASTCELL")] {
        source.push_str(&format!("    | {header} |\n    | --- |\n"));
        for index in 0..rows {
            source.push_str(&format!("    | {prefix}{index:03} preserved source |\n"));
        }
        source.push('\n');
    }
    let mut options = options();
    options.theme.page.size.height_pt = 240.0;
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert!(!text(&pages).contains("Notes"));
    for (header, prefix) in [("FIRSTHEADER", "FIRSTCELL"), ("LASTHEADER", "LASTCELL")] {
        assert_markers(&pages, prefix, rows);
        let original = locate(&pages, header).0;
        let mut table_pages = BTreeSet::new();
        for index in 0..rows {
            let (page, row) = locate(&pages, &format!("{prefix}{index:03}"));
            let headers: Vec<_> = pages[page]
                .runs
                .iter()
                .filter(|run| run.text.contains(header))
                .collect();
            assert_eq!(headers.len(), 1);
            assert!(headers[0].y > row.y);
            assert_eq!(headers[0].artifact, page != original);
            table_pages.insert(page);
        }
        assert!(table_pages.len() >= 2);
        assert_eq!(text(&pages).matches(header).count(), 1);
    }
    assert_eq!(structure_count(&bytes, "Note"), 1);
    assert_eq!(structure_count(&bytes, "Table"), 2);
    assert_eq!(structure_count(&bytes, "TH"), 2);
    assert_eq!(structure_count(&bytes, "TD"), rows * 2);
    assert_geometry(&pages, &options);
}

// Sectioned notes need a content-based geometry oracle: a note's H1/H2 can be
// larger than body prose, so font size cannot identify which region owns it.
fn assert_section_note_geometry(pages: &[PaintedPage], options: &PdfOptions) {
    let margins = options.theme.page.margins;
    for (index, page) in pages.iter().enumerate() {
        let body_bottom = page
            .runs
            .iter()
            .filter(|run| !run.artifact && run.text.contains("BODY"))
            .map(|run| run.y + run.descent * run.size)
            .fold(f64::INFINITY, f64::min);
        let mut rows = BTreeMap::<i64, (f64, f64)>::new();
        let mut has_note = false;
        for run in page
            .runs
            .iter()
            .filter(|run| !run.artifact && run.text.chars().any(|c| c.is_ascii_alphabetic()))
        {
            let top = run.y + run.cap_height * run.size;
            let bottom = run.y + run.descent * run.size;
            assert!(
                top <= page.height - f64::from(margins.top_pt) + 0.025
                    && bottom >= f64::from(margins.bottom_pt) - 0.025,
                "page {} paints outside the body: {run:?}",
                index + 1,
            );
            assert!(
                run.x >= f64::from(margins.left_pt) - 0.025
                    && run.x < page.width - f64::from(margins.right_pt),
                "page {} has an out-of-bounds text origin: {run:?}",
                index + 1,
            );
            rows.entry((run.y * 1000.0).round() as i64)
                .and_modify(|bounds| {
                    bounds.0 = bounds.0.min(bottom);
                    bounds.1 = bounds.1.max(top);
                })
                .or_insert((bottom, top));
            if !run.text.contains("BODY") {
                has_note = true;
                assert!(
                    top < body_bottom,
                    "page {} paints a sectioned note into body text: {run:?}",
                    index + 1,
                );
            }
        }
        let rows: Vec<_> = rows.into_values().collect();
        for adjacent in rows.windows(2) {
            assert!(
                adjacent[0].1 <= adjacent[1].0 + 0.025,
                "page {} overlaps distinct text rows: {adjacent:?}",
                index + 1,
            );
        }
        if has_note {
            let width = page.width - f64::from(margins.left_pt + margins.right_pt);
            assert_eq!(separator_count(page, width), 1, "page {}", index + 1);
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

fn assert_note_heading_ownership(pdf: &[u8], level: u8, count: usize) {
    let objects = Objects::new(pdf);
    let tag = format!("/S /H{level} ");
    let headings: Vec<_> = (1..objects.offsets.len())
        .filter(|&id| {
            let dictionary = objects.dictionary(id);
            dictionary.contains("/Type /StructElem ") && dictionary.contains(&tag)
        })
        .collect();
    assert_eq!(headings.len(), count, "one logical heading per source heading");
    assert_eq!(structure_count(pdf, "Note"), 1);
    for heading in headings {
        let mut owner = reference_after(objects.dictionary(heading), "/P ");
        let mut visited = BTreeSet::new();
        loop {
            assert!(visited.insert(owner), "structure parent chain must be acyclic");
            let dictionary = objects.dictionary(owner);
            assert!(
                dictionary.contains("/Type /StructElem "),
                "heading escaped its logical note",
            );
            if dictionary.contains("/S /Note ") {
                break;
            }
            owner = reference_after(dictionary, "/P ");
        }
    }
}

fn heading_rule_y(page: &PaintedPage, width: f64) -> Vec<f64> {
    let mut artifacts = Vec::new();
    let mut start = None;
    let mut end = None;
    let mut rules = Vec::new();
    for operation in pdf_content::operations(&page.content) {
        match operation.operator {
            "BDC" | "BMC" => artifacts.push(
                operation.operands[0] == "/Artifact" || artifacts.last().copied().unwrap_or(false),
            ),
            "EMC" => {
                artifacts.pop().expect("balanced artifact scopes");
            }
            "m" | "l" if artifacts.last() == Some(&true) => {
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
                    && artifacts.last() == Some(&true)
                    && (y1 - y0).abs() < PRECISION
                    && (x1 - x0 - width).abs() < PRECISION
                {
                    rules.push(y0);
                }
            }
            _ => {}
        }
    }
    rules
}

#[test]
fn sectioned_note_keeps_wrapped_heading_rule_subject_and_navigation_on_its_first_page() {
    for level in 1..=6 {
        let mut source = format!(
            "BODYCITE.[^section]\n\n[^section]:\n    {} BEGINHEAD detailed evidence with several measured words ending in ENDHEAD\n\n    FIRSTSUBJECT follows the complete heading.\n\n",
            "#".repeat(level),
        );
        for index in 0..48 {
            source.push_str(&format!("    NOTE{index:03} retained source row.  \n"));
        }
        let mut options = options();
        options.footnote_backlinks = true;
        let document = parse_markdown(&source);
        let bytes =
            render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
        let pages = painted_pages(&bytes);
        let first_page = locate(&pages, "BODYCITE").0;
        let (_, begin) = locate(&pages, "BEGINHEAD");
        let (_, end) = locate(&pages, "ENDHEAD");
        let (_, subject) = locate(&pages, "FIRSTSUBJECT");
        for marker in ["BEGINHEAD", "ENDHEAD", "FIRSTSUBJECT"] {
            assert_eq!(locate(&pages, marker).0, first_page, "{marker}");
            assert_eq!(text(&pages).matches(marker).count(), 1, "{marker}");
        }
        assert!(pages.len() > 1, "fixture must continue onto later pages");
        assert_markers(&pages, "NOTE", 48);
        assert!(!text(&pages).contains("Notes"));
        if level <= 2 {
            assert!(begin.y > end.y, "fixture must wrap its large heading");
            let rules = heading_rule_y(&pages[first_page], 288.0);
            assert_eq!(rules.len(), 1, "the complete H1/H2 has one underline");
            assert!(end.y > rules[0] && rules[0] > subject.y);
        }
        assert_note_heading_ownership(&bytes, level as u8, 1);
        assert_first_note_navigation(&bytes, &pages, "BODYCITE");
        assert_section_note_geometry(&pages, &options);
        assert_eq!(
            bytes,
            render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap(),
        );
        assert_eq!(
            franken_markdown::verify::verify_pdf(&document, &options)
                .unwrap()
                .page_count,
            pages.len(),
        );
    }
}

#[test]
fn late_note_heading_moves_with_its_subject_and_retains_its_real_link_destination() {
    let mut source = String::from(
        "BODYCITE.[^section] [BODYJUMP](#latehead)\n\n[^section]: INTRO retained introduction.\n\n",
    );
    for index in 0..30 {
        source.push_str(&format!("    BEFORE{index:03} preserved source row.  \n"));
    }
    source.push_str("\n    ## LATEHEAD\n\n    LATETEXT follows its section title.\n\n");
    for index in 0..30 {
        source.push_str(&format!("    AFTER{index:03} preserved source row.  \n"));
    }
    let mut options = options();
    options.footnote_backlinks = true;
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    let (heading_page, heading) = locate(&pages, "LATEHEAD");
    assert!(heading_page > locate(&pages, "BODYCITE").0);
    assert_eq!(heading_page, locate(&pages, "LATETEXT").0);
    assert_markers(&pages, "BEFORE", 30);
    assert_markers(&pages, "AFTER", 30);
    assert!(!text(&pages).contains("Notes"));
    let targets = destinations(&bytes, &pages);
    assert_eq!(targets.len(), 3, "citation, return label, and authored heading link");
    assert!(targets.iter().any(|&(page, y)| {
        page == heading_page && (y - heading.y - heading.size * 0.9).abs() < 0.025
    }));
    assert_note_heading_ownership(&bytes, 2, 1);
    assert_section_note_geometry(&pages, &options);
}

#[test]
fn section_heading_keeps_table_header_and_complete_first_row_in_its_note_fragment() {
    let mut source = String::from(
        "BODYCITE.[^section]\n\n[^section]:\n    ### TABLETITLE\n\n    | Column |\n    | --- |\n",
    );
    for index in 0..24 {
        source.push_str(&format!(
            "    | ROW{index:03} repeated measured words repeated measured words repeated measured words END{index:03} |\n",
        ));
    }
    let options = options();
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    let first_page = locate(&pages, "BODYCITE").0;
    for marker in ["TABLETITLE", "Column", "ROW000", "END000"] {
        assert_eq!(locate(&pages, marker).0, first_page, "{marker}");
    }
    assert!(locate(&pages, "ROW023").0 > first_page);
    assert_markers(&pages, "ROW", 24);
    assert_markers(&pages, "END", 24);
    assert!(!text(&pages).contains("Notes"));
    assert_note_heading_ownership(&bytes, 3, 1);
    assert_eq!(structure_count(&bytes, "Table"), 1);
    assert_eq!(structure_count(&bytes, "TH"), 1);
    assert_eq!(structure_count(&bytes, "TD"), 24);
    assert_section_note_geometry(&pages, &options);
}

#[test]
fn heading_underlines_do_not_bind_distinct_list_items_into_one_oversized_note_unit() {
    for level in [2, 3] {
        let mut source = String::from("BODYCITE.[^section]\n\n[^section]:\n");
        for index in 0..18 {
            source.push_str(&format!("    - {} HEAD{index:03}\n", "#".repeat(level)));
        }
        let mut options = options();
        options.theme.page.size.height_pt = 240.0;
        options.footnote_backlinks = true;
        let bytes = render_pdf(&source, &options).unwrap();
        let pages = painted_pages(&bytes);
        assert_markers(&pages, "HEAD", 18);
        assert!(!text(&pages).contains("Notes"));
        assert_eq!(locate(&pages, "HEAD000").0, locate(&pages, "BODYCITE").0);
        assert!(locate(&pages, "HEAD017").0 > locate(&pages, "HEAD000").0);
        assert_note_heading_ownership(&bytes, level as u8, 18);
        assert_eq!(structure_count(&bytes, "LI"), 18);
        assert_first_note_navigation(&bytes, &pages, "BODYCITE");
        assert_section_note_geometry(&pages, &options);
    }
}

#[test]
fn impossible_heading_keep_preserves_complete_unchanged_endnote_fallback() {
    let mut source = String::from("BODYCITE.[^section]\n\n[^section]:\n");
    for index in 0..24 {
        source.push_str(&format!("    ## BIG{index:03}\n\n"));
    }
    source.push_str("    LASTSUBJECT retains its whole source.\n");
    let options = options();
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert!(text(&pages).contains("Notes"));
    assert_markers(&pages, "BIG", 24);
    assert_eq!(text(&pages).matches("LASTSUBJECT").count(), 1);
    let mut ordinary = options.clone();
    ordinary.split_footnotes = false;
    assert_eq!(bytes, render_pdf(&source, &ordinary).unwrap());
}

#[test]
fn nested_quotation_headings_do_not_capture_outdented_section_titles() {
    let mut source = String::from("BODYCITE.[^section]\n\n[^section]:\n");
    for index in 0..12 {
        source.push_str(&format!(
            "    > > ## INNER{index:03}\n    >\n    > ### OUTER{index:03}\n    >\n",
        ));
    }
    let mut options = options();
    options.theme.page.size.height_pt = 300.0;
    options.footnote_backlinks = true;
    let bytes = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "INNER", 12);
    assert_markers(&pages, "OUTER", 12);
    assert!(!text(&pages).contains("Notes"));
    assert_eq!(locate(&pages, "INNER000").0, locate(&pages, "BODYCITE").0);
    assert!(locate(&pages, "OUTER011").0 > locate(&pages, "INNER000").0);
    assert_note_heading_ownership(&bytes, 2, 12);
    assert_note_heading_ownership(&bytes, 3, 12);
    assert_eq!(structure_count(&bytes, "BlockQuote"), 13);
    assert_first_note_navigation(&bytes, &pages, "BODYCITE");
    assert_section_note_geometry(&pages, &options);
    let margins = options.theme.page.margins;
    let mut panels = 0;
    for page in &pages {
        let body_bottom = page
            .runs
            .iter()
            .filter(|run| !run.artifact && run.text.contains("BODY"))
            .map(|run| run.y + run.descent * run.size)
            .fold(f64::INFINITY, f64::min);
        for [left, bottom, right, top] in artifact_fills(page) {
            panels += 1;
            assert!(
                left >= f64::from(margins.left_pt) - 0.025
                    && right <= page.width - f64::from(margins.right_pt) + 0.025
                    && bottom >= f64::from(margins.bottom_pt) - 0.025
                    && top <= page.height - f64::from(margins.top_pt) + 0.025,
                "a continued quote panel leaves the content box",
            );
            assert!(top < body_bottom, "a note panel overlaps body text");
        }
    }
    assert!(panels > 0, "fixture must paint the nested quotation panels");
}

fn definition_text(text: impl Into<String>) -> Vec<Inline> {
    vec![Inline::Text(text.into())]
}

fn definition_note(blocks: Vec<Block>) -> Document {
    Document {
        blocks: vec![
            Block::Paragraph(vec![
                Inline::Text("BODYCITE.".into()),
                Inline::FootnoteRef { id: "terms".into() },
            ]),
            Block::FootnoteDefinition {
                id: "terms".into(),
                blocks,
            },
        ],
    }
}

fn definition_options() -> PdfOptions {
    let mut options = options();
    options.profile = Some(Profile::GfmPlus);
    options.footnote_backlinks = true;
    options
}

#[test]
fn gfm_plus_definition_note_keeps_all_terms_with_its_first_description() {
    let mut source = String::from("BODYCITE.[^terms]\n\n[^terms]:\n");
    for index in 0..20 {
        source.push_str(&format!(
            "    TERM{index:03}\n    ALIAS{index:03}\n    : DESC{index:03} first definition.\n    : MORE{index:03} another definition.\n\n",
        ));
    }
    let document = parse_markdown_with_profile(&source, Some(Profile::GfmPlus));
    let Block::FootnoteDefinition { blocks, .. } = &document.blocks[1] else {
        panic!("fixture must retain its source footnote");
    };
    let items: Vec<_> = blocks
        .iter()
        .flat_map(|block| match block {
            Block::DefinitionList(items) => items.as_slice(),
            _ => &[],
        })
        .collect();
    assert_eq!(items.len(), 20, "exercise parsed definition lists, not prose");
    assert!(
        items
            .iter()
            .all(|item| item.terms.len() == 2 && item.definitions.len() == 2),
    );
    let options = definition_options();
    let bytes =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let pages = painted_pages(&bytes);
    for prefix in ["TERM", "ALIAS", "DESC", "MORE"] {
        assert_markers(&pages, prefix, 20);
    }
    for index in 0..20 {
        let (term_page, term) = locate(&pages, &format!("TERM{index:03}"));
        let (alias_page, _) = locate(&pages, &format!("ALIAS{index:03}"));
        let (description_page, description) = locate(&pages, &format!("DESC{index:03}"));
        assert_eq!(term_page, alias_page);
        assert_eq!(term_page, description_page, "definition item {index}");
        assert!(description.x > term.x, "retain the definition's indentation");
    }
    assert_eq!(locate(&pages, "TERM000").0, locate(&pages, "BODYCITE").0);
    assert!(locate(&pages, "TERM019").0 > locate(&pages, "TERM000").0);
    assert!(!text(&pages).contains("Notes"));
    assert_eq!(structure_count(&bytes, "Note"), 1);
    assert_first_note_navigation(&bytes, &pages, "BODYCITE");
    assert_section_note_geometry(&pages, &options);
    assert_eq!(
        bytes,
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap(),
    );
    assert_eq!(bytes, render_pdf(&source, &options).unwrap());
}

#[test]
fn wrapped_definition_terms_stay_whole_while_a_long_description_continues() {
    let mut description = Vec::new();
    for index in 0..64 {
        if index > 0 {
            description.push(Inline::HardBreak);
        }
        description.push(Inline::Text(format!("DESC{index:03} retained source row.")));
    }
    let document = definition_note(vec![Block::DefinitionList(vec![DefinitionItem {
        terms: vec![
            definition_text(
                "TERMSTART several explanatory words several explanatory words several explanatory words TERMEND",
            ),
            definition_text(
                "ALIASSTART another expanded term with several measured words and more words ALIASEND",
            ),
        ],
        definitions: vec![description, definition_text("OTHERDEF retained second definition.")],
    }])]);
    let options = definition_options();
    let bytes =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let pages = painted_pages(&bytes);
    let citation_page = locate(&pages, "BODYCITE").0;
    for marker in ["TERMSTART", "TERMEND", "ALIASSTART", "ALIASEND", "DESC000"] {
        assert_eq!(locate(&pages, marker).0, citation_page, "{marker}");
        assert_eq!(text(&pages).matches(marker).count(), 1);
    }
    assert!(locate(&pages, "TERMSTART").1.y > locate(&pages, "TERMEND").1.y);
    assert!(locate(&pages, "ALIASSTART").1.y > locate(&pages, "ALIASEND").1.y);
    assert_markers(&pages, "DESC", 64);
    assert_eq!(text(&pages).matches("OTHERDEF").count(), 1);
    assert!(locate(&pages, "DESC063").0 > citation_page);
    assert!(!text(&pages).contains("Notes"));
    assert_eq!(structure_count(&bytes, "Note"), 1);
    assert_first_note_navigation(&bytes, &pages, "BODYCITE");
    assert_section_note_geometry(&pages, &options);
}

#[test]
fn nested_definition_notes_preserve_quote_and_list_ownership() {
    let items = (0..24)
        .map(|index| DefinitionItem {
            terms: vec![definition_text(format!("TERM{index:03}"))],
            definitions: vec![definition_text(format!("DESC{index:03} retained description."))],
        })
        .collect();
    let document = definition_note(vec![Block::BlockQuote(vec![Block::List(List {
        ordered: false,
        start: 1,
        tight: false,
        items: vec![ListItem {
            task: None,
            blocks: vec![Block::DefinitionList(items)],
        }],
    })])]);
    let options = definition_options();
    let bytes =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let pages = painted_pages(&bytes);
    assert_markers(&pages, "TERM", 24);
    assert_markers(&pages, "DESC", 24);
    assert!(!text(&pages).contains("Notes"));
    for index in 0..24 {
        assert_eq!(
            locate(&pages, &format!("TERM{index:03}")).0,
            locate(&pages, &format!("DESC{index:03}")).0,
        );
    }
    assert_eq!(locate(&pages, "TERM000").0, locate(&pages, "BODYCITE").0);
    assert!(locate(&pages, "TERM023").0 > locate(&pages, "TERM000").0);
    assert_eq!(structure_count(&bytes, "Note"), 1);
    assert_eq!(structure_count(&bytes, "BlockQuote"), 1);
    assert_eq!(structure_count(&bytes, "L"), 1);
    assert_eq!(structure_count(&bytes, "LI"), 1);
    assert_first_note_navigation(&bytes, &pages, "BODYCITE");
    assert_section_note_geometry(&pages, &options);
    // Every term/description uses ordinary note type in this fixture.
    assert_panel_bounds(&pages, &options);
}

#[test]
fn empty_definition_items_keep_their_prefix_without_capturing_adjacent_items() {
    for termless in [false, true] {
        let first = if termless {
            DefinitionItem {
                terms: vec![Vec::new()],
                definitions: vec![definition_text("FIRSTSOURCE description without a term.")],
            }
        } else {
            DefinitionItem {
                terms: vec![definition_text("FIRSTSOURCE term without a description.")],
                definitions: vec![Vec::new()],
            }
        };
        let mut items = vec![first];
        for index in 0..24 {
            items.push(DefinitionItem {
                terms: Vec::new(),
                definitions: Vec::new(),
            });
            items.push(DefinitionItem {
                terms: vec![definition_text(format!("EMPTY{index:03} terminal term."))],
                definitions: Vec::new(),
            });
        }
        let document = definition_note(vec![Block::DefinitionList(items)]);
        for height in [144.0, 160.0, 180.0, 192.0] {
            let mut options = definition_options();
            options.theme.page.size.height_pt = height;
            let bytes =
                render_pdf_document_emitted(&document, &options, PdfEmitOptions::default())
                    .unwrap();
            let pages = painted_pages(&bytes);
            assert!(!text(&pages).contains("Notes"));
            assert_eq!(locate(&pages, "FIRSTSOURCE").0, locate(&pages, "BODYCITE").0);
            assert_markers(&pages, "EMPTY", 24);
            assert!(locate(&pages, "EMPTY023").0 > locate(&pages, "FIRSTSOURCE").0);
            assert_eq!(structure_count(&bytes, "Note"), 1);
            assert_first_note_navigation(&bytes, &pages, "BODYCITE");
            assert_section_note_geometry(&pages, &options);
        }
    }
}

#[test]
fn definition_term_authority_does_not_leak_into_ordinary_bold_paragraphs() {
    let mut blocks = vec![Block::DefinitionList(vec![DefinitionItem {
        terms: vec![definition_text("TERMINTRO")],
        definitions: vec![definition_text("DESCINTRO retained description.")],
    }])];
    for index in 0..32 {
        blocks.push(Block::Paragraph(vec![Inline::Strong(definition_text(
            format!("BOLD{index:03} independent ordinary paragraph."),
        ))]));
    }
    let document = definition_note(blocks);
    let options = definition_options();
    let bytes =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let pages = painted_pages(&bytes);
    assert!(!text(&pages).contains("Notes"));
    assert_markers(&pages, "BOLD", 32);
    assert_eq!(locate(&pages, "TERMINTRO").0, locate(&pages, "DESCINTRO").0);
    assert!(locate(&pages, "BOLD031").0 > locate(&pages, "BOLD000").0);
    assert_first_note_navigation(&bytes, &pages, "BODYCITE");
    assert_section_note_geometry(&pages, &options);
}

#[test]
fn oversized_definition_term_group_preserves_the_complete_ordinary_endnote() {
    let document = definition_note(vec![Block::DefinitionList(vec![DefinitionItem {
        terms: (0..32)
            .map(|index| definition_text(format!("TERM{index:03}")))
            .collect(),
        definitions: vec![definition_text("LASTDESC belongs to every source term.")],
    }])]);
    let options = definition_options();
    let bytes =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let pages = painted_pages(&bytes);
    assert!(text(&pages).contains("Notes"));
    assert_markers(&pages, "TERM", 32);
    assert_eq!(text(&pages).matches("LASTDESC").count(), 1);
    let mut ordinary = options.clone();
    ordinary.split_footnotes = false;
    assert_eq!(
        bytes,
        render_pdf_document_emitted(&document, &ordinary, PdfEmitOptions::default()).unwrap(),
    );
}
