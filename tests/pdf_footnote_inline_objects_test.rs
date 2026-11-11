//! Inline objects in continued notes retain painted geometry and source ownership.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::{
    PageMargins, PdfEmitOptions, PdfImageAsset, PdfOptions, parse_markdown, render_pdf,
    render_pdf_document_emitted,
};

type Rect = [f64; 4];

struct Object {
    dictionary: String,
    stream: Option<Vec<u8>>,
}

struct Pdf {
    objects: BTreeMap<usize, Object>,
    pages: Vec<usize>,
}

fn number(source: &str, key: &str) -> usize {
    let words = source.split_whitespace().collect::<Vec<_>>();
    words
        .windows(2)
        .find_map(|pair| (pair[0] == key).then(|| pair[1].parse().ok()).flatten())
        .unwrap_or_else(|| panic!("missing numeric {key}: {source}"))
}

fn rect(source: &str, key: &str) -> Rect {
    source
        .split_once(key)
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

impl Pdf {
    fn new(bytes: &[u8]) -> Self {
        let marker = b"startxref\n";
        let start = bytes
            .windows(marker.len())
            .rposition(|part| part == marker)
            .unwrap();
        let xref: usize = std::str::from_utf8(&bytes[start + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let section = lines.next().unwrap().split_whitespace().collect::<Vec<_>>();
        assert_eq!(section[0], "0");
        let offsets = lines
            .take(section[1].parse().unwrap())
            .map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                if fields[2] == "n" {
                    fields[0].parse().unwrap()
                } else {
                    0
                }
            })
            .collect::<Vec<usize>>();
        let objects = offsets
            .iter()
            .enumerate()
            .filter(|(_, offset)| **offset != 0)
            .map(|(id, &offset)| {
                let end = offsets
                    .iter()
                    .copied()
                    .filter(|&value| value > offset)
                    .min()
                    .unwrap_or(xref);
                let header = format!("{id} 0 obj\n");
                assert!(bytes[offset..end].starts_with(header.as_bytes()));
                let body = &bytes[offset + header.len()..end];
                let stream_start = body.windows(8).position(|part| part == b"\nstream\n");
                let dictionary = std::str::from_utf8(&body[..stream_start.unwrap_or(body.len())])
                    .unwrap()
                    .to_owned();
                let stream = stream_start.map(|at| {
                    let length = number(&dictionary, "/Length");
                    let encoded = &body[at + 8..at + 8 + length];
                    if dictionary.contains("/FlateDecode") {
                        franken_markdown::zlib_decompress(encoded, 1 << 26).unwrap()
                    } else {
                        assert!(!dictionary.contains("/Filter"));
                        encoded.to_vec()
                    }
                });
                (id, Object { dictionary, stream })
            })
            .collect::<BTreeMap<_, _>>();
        let pages = objects
            .values()
            .find(|object| object.dictionary.contains("/Type /Pages "))
            .unwrap();
        let kids = pages
            .dictionary
            .split_once("/Kids [")
            .unwrap()
            .1
            .split_once(']')
            .unwrap()
            .0
            .split_whitespace()
            .collect::<Vec<_>>();
        let (kids, remainder) = kids.as_chunks::<3>();
        assert!(remainder.is_empty());
        let pages = kids
            .iter()
            .map(|reference| {
                assert_eq!(&reference[1..], &["0", "R"]);
                reference[0].parse().unwrap()
            })
            .collect();
        Self { objects, pages }
    }

    fn dictionary(&self, id: usize) -> &str {
        &self.objects[&id].dictionary
    }

    fn stream(&self, id: usize) -> &str {
        std::str::from_utf8(self.objects[&id].stream.as_ref().unwrap()).unwrap()
    }

    fn content(&self, page: usize) -> &str {
        self.stream(number(self.dictionary(page), "/Contents"))
    }

    fn runs(&self, page: usize) -> Vec<Run> {
        let dictionary = self.dictionary(page);
        let fields = dictionary
            .split_once("/Font <<")
            .unwrap()
            .1
            .split_once(">>")
            .unwrap()
            .0
            .split_whitespace()
            .collect::<Vec<_>>();
        let (entries, remainder) = fields.as_chunks::<4>();
        assert!(remainder.is_empty());
        let mut fonts = BTreeMap::new();
        for entry in entries {
            let font = self.dictionary(entry[1].parse().unwrap());
            let cmap = self.stream(number(font, "/ToUnicode"));
            let mut mapping = BTreeMap::new();
            let mut active = false;
            for line in cmap.lines() {
                if line.ends_with(" beginbfchar") {
                    active = true;
                    continue;
                }
                if line == "endbfchar" {
                    active = false;
                    continue;
                }
                if active {
                    let words = line.split_whitespace().collect::<Vec<_>>();
                    let cid = u16::from_str_radix(words[0].trim_matches(['<', '>']), 16).unwrap();
                    let encoded = words[1].trim_matches(['<', '>']).as_bytes();
                    let (units, remainder) = encoded.as_chunks::<4>();
                    assert!(remainder.is_empty());
                    let units = units
                        .iter()
                        .map(|unit| {
                            u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap()
                        })
                        .collect::<Vec<_>>();
                    mapping.insert(cid, String::from_utf16(&units).unwrap());
                }
            }
            fonts.insert(entry[0], mapping);
        }
        let (mut x, mut y, mut size) = (0.0, 0.0, 0.0);
        let mut font = None;
        let mut hidden = false;
        let mut artifacts = Vec::new();
        let mut runs = Vec::new();
        for operation in pdf_content::operations(self.content(page)) {
            match operation.operator {
                "BDC" | "BMC" => artifacts.push(
                    operation.operands[0] == "/Artifact"
                        || artifacts.last().copied().unwrap_or(false),
                ),
                "EMC" => {
                    artifacts.pop().expect("balanced marked content");
                }
                "Tf" => {
                    font = Some(&fonts[operation.operands[0]]);
                    size = operation.operands[1].parse().unwrap();
                }
                "Tm" => {
                    x = operation.operands[4].parse().unwrap();
                    y = operation.operands[5].parse().unwrap();
                }
                "Tr" => hidden = operation.operands[0] == "3",
                "Td" | "TD" | "T*" => panic!("extend the oracle for relative text motion"),
                "TJ" | "Tj" if !hidden && artifacts.last() != Some(&true) => {
                    let text: String = pdf_content::glyph_ids(&operation)
                        .into_iter()
                        .map(|cid| font.unwrap()[&cid].as_str())
                        .collect();
                    if !text.is_empty() {
                        runs.push(Run { text, x, y, size });
                    }
                }
                _ => {}
            }
        }
        assert!(artifacts.is_empty());
        runs
    }

    fn elements(&self, tag: &str) -> Vec<(usize, usize, Rect)> {
        let key = format!("/Type /StructElem /S /{tag} ");
        self.objects
            .iter()
            .filter(|(_, object)| object.dictionary.contains(&key))
            .map(|(&id, object)| {
                (
                    id,
                    number(&object.dictionary, "/Pg"),
                    rect(&object.dictionary, "/BBox ["),
                )
            })
            .collect()
    }

    fn ancestry(&self, id: usize) -> Vec<&str> {
        let mut result = Vec::new();
        let mut parent = number(self.dictionary(id), "/P");
        loop {
            let dictionary = self.dictionary(parent);
            let tag = dictionary
                .split_once("/S /")
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap();
            result.push(tag);
            if tag == "Document" {
                return result;
            }
            assert!(result.len() < self.objects.len(), "acyclic structure");
            parent = number(dictionary, "/P");
        }
    }
}

#[derive(Debug)]
struct Run {
    text: String,
    x: f64,
    y: f64,
    size: f64,
}

fn options() -> PdfOptions {
    let mut options = PdfOptions {
        split_footnotes: true,
        footnote_backlinks: true,
        image_assets: vec![PdfImageAsset::new("badge.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#125abc"/></svg>"##.to_vec())],
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 360.0;
    options.theme.page.size.height_pt = 300.0;
    options.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options
}

fn source(rows: usize, content: &str) -> String {
    let mut source = String::from("CITATION.[^objects]\n\n");
    for index in 0..8 {
        source.push_str(&format!("BODY{index:03} ordinary source paragraph.\n\n"));
    }
    let rows = (0..rows)
        .map(|index| format!("NOTE{index:03} {content} retained."))
        .collect::<Vec<_>>()
        .join("  \n    ");
    source.push_str(&format!("[^objects]: {rows}\n"));
    source
}

fn inside(ink: Rect, bounds: Rect) {
    assert!(
        ink[2] > ink[0] && ink[3] > ink[1],
        "nonempty object: {ink:?}"
    );
    assert!(
        ink[0] >= bounds[0] - 0.03
            && ink[1] >= bounds[1] - 0.03
            && ink[2] <= bounds[2] + 0.03
            && ink[3] <= bounds[3] + 0.03,
        "{ink:?} escapes {bounds:?}"
    );
}

// Read actual outlines, independently of their declared accessibility boxes.
fn formula_ink(content: &str) -> Vec<Rect> {
    content
        .split("/Formula <</MCID ")
        .skip(1)
        .map(|formula| {
            let outlines = formula.split_once("/ActualText ").unwrap().0;
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            let mut points = 0;
            for line in outlines.lines() {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                let coordinates = match fields.last() {
                    Some(&"m" | &"l") if fields.len() == 3 => &fields[..2],
                    Some(&"c") if fields.len() == 7 => &fields[..6],
                    _ => continue,
                };
                for pair in coordinates.as_chunks::<2>().0 {
                    let x: f64 = pair[0].parse().unwrap();
                    let y: f64 = pair[1].parse().unwrap();
                    bounds[0] = bounds[0].min(x);
                    bounds[1] = bounds[1].min(y);
                    bounds[2] = bounds[2].max(x);
                    bounds[3] = bounds[3].max(y);
                    points += 1;
                }
            }
            assert!(points > 10, "a formula must paint real vector outlines");
            bounds
        })
        .collect()
}

fn image_ink(content: &str) -> Vec<Rect> {
    content
        .split("/Figure <</MCID ")
        .skip(1)
        .map(|figure| {
            let fields = figure
                .split_once("EMC")
                .unwrap()
                .0
                .split_whitespace()
                .collect::<Vec<_>>();
            let matrix = fields.iter().position(|&field| field == "cm").unwrap();
            let rectangle = fields.iter().position(|&field| field == "re").unwrap();
            assert_eq!(
                fields[rectangle + 1],
                "f",
                "fixture paints one SVG rectangle"
            );
            assert_eq!(
                fields[..rectangle]
                    .iter()
                    .filter(|&&field| field == "cm")
                    .count(),
                1
            );
            let [a, b, c, d, e, f]: [f64; 6] =
                std::array::from_fn(|index| fields[matrix - 6 + index].parse().unwrap());
            let [x, y, w, h]: [f64; 4] =
                std::array::from_fn(|index| fields[rectangle - 4 + index].parse().unwrap());
            let points = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
                .map(|(x, y)| (a * x + c * y + e, b * x + d * y + f));
            [
                points
                    .iter()
                    .map(|point| point.0)
                    .fold(f64::INFINITY, f64::min),
                points
                    .iter()
                    .map(|point| point.1)
                    .fold(f64::INFINITY, f64::min),
                points
                    .iter()
                    .map(|point| point.0)
                    .fold(f64::NEG_INFINITY, f64::max),
                points
                    .iter()
                    .map(|point| point.1)
                    .fold(f64::NEG_INFINITY, f64::max),
            ]
        })
        .collect()
}

fn assert_continued(pdf: &Pdf, rows: usize) {
    let runs = pdf
        .pages
        .iter()
        .map(|&page| pdf.runs(page))
        .collect::<Vec<_>>();
    let text = runs
        .iter()
        .flatten()
        .map(|run| run.text.as_str())
        .collect::<String>();
    assert!(
        !text.contains("Notes"),
        "eligible long notes must stay in page-foot flow"
    );
    let mut note_pages = BTreeSet::new();
    let mut previous = 0;
    for index in 0..rows {
        let marker = format!("NOTE{index:03}");
        assert_eq!(
            text.matches(&marker).count(),
            1,
            "paint {marker} exactly once"
        );
        let offset = text.find(&marker).unwrap();
        assert!(offset >= previous, "preserve note source order");
        previous = offset;
        let (page, run) = runs
            .iter()
            .enumerate()
            .find_map(|(page, runs)| {
                runs.iter()
                    .find(|run| run.text.contains(&marker))
                    .map(|run| (page, run))
            })
            .unwrap();
        assert!(
            (run.size - 9.35).abs() < 0.015,
            "retain footnote size: {run:?}"
        );
        assert!(run.x >= 36.0 - 0.03 && run.x < 324.0);
        assert!(run.y > 36.0 && run.y < 264.0);
        if index == 0 {
            assert_eq!(page, 0, "first fragment belongs on the citation page");
        }
        note_pages.insert(page);
    }
    assert!(
        note_pages.len() >= 3,
        "exercise repeated continuation boundaries"
    );
    assert_eq!(
        pdf.objects
            .values()
            .filter(|object| object.dictionary.contains("/S /Note "))
            .count(),
        1
    );
    for &page in &pdf.pages {
        let body_bottom = pdf
            .runs(page)
            .iter()
            .filter(|run| run.text.contains("BODY") || run.text.contains("CITATION"))
            .map(|run| run.y - run.size * 0.3)
            .fold(f64::INFINITY, f64::min);
        for tag in ["Formula", "Figure"] {
            let elements = pdf
                .elements(tag)
                .into_iter()
                .filter(|(_, object_page, _)| *object_page == page)
                .collect::<Vec<_>>();
            let painted = if tag == "Formula" {
                formula_ink(pdf.content(page))
            } else {
                image_ink(pdf.content(page))
            };
            assert_eq!(elements.len(), painted.len());
            for ((_, _, bounds), ink) in elements.into_iter().zip(painted) {
                inside(bounds, [36.0, 36.0, 324.0, 264.0]);
                inside(ink, bounds);
                assert!(bounds[3] < body_bottom, "{tag} overlaps document body");
            }
        }
    }
}

#[test]
fn inline_formulas_continue_without_endnote_fallback_or_lost_geometry() {
    let source = source(36, r"$\frac{x^2}{y_3}$");
    let options = options();
    let bytes = render_pdf(&source, &options).unwrap();
    let pdf = Pdf::new(&bytes);
    assert_continued(&pdf, 36);
    let formulas = pdf.elements("Formula");
    assert_eq!(formulas.len(), 36);
    for (id, _, _) in formulas {
        assert!(pdf.ancestry(id).contains(&"Note"));
    }
    assert_eq!(bytes, render_pdf(&source, &options).unwrap());
    assert_eq!(
        bytes,
        render_pdf_document_emitted(
            &parse_markdown(&source),
            &options,
            PdfEmitOptions::monolithic()
        )
        .unwrap()
    );
}

#[test]
fn linked_inline_images_continue_with_note_ownership_and_full_annotation_bounds() {
    let source = source(36, "[![badge](badge.svg)](https://example.org/icon)");
    let options = options();
    let bytes = render_pdf(&source, &options).unwrap();
    let pdf = Pdf::new(&bytes);
    assert_continued(&pdf, 36);
    let figures = pdf.elements("Figure");
    assert_eq!(figures.len(), 36);
    let links = pdf
        .objects
        .values()
        .filter(|object| {
            object
                .dictionary
                .contains("/URI (https://example.org/icon)")
        })
        .map(|object| rect(&object.dictionary, "/Rect ["))
        .collect::<Vec<_>>();
    assert_eq!(links.len(), figures.len());
    for ((id, _, bounds), link) in figures.into_iter().zip(links) {
        assert!(pdf.ancestry(id).contains(&"Note"));
        inside(bounds, link);
        inside(link, bounds);
        assert!((bounds[2] - bounds[0] - 15.0).abs() < 0.03);
        assert!((bounds[3] - bounds[1] - 15.0).abs() < 0.03);
    }
}

#[test]
fn indivisible_figures_oversized_table_rows_and_unreferenced_notes_keep_existing_fallback() {
    for block in [
        "![figure](badge.svg)".to_owned(),
        "**[![figure](badge.svg)](https://example.org/figure)**".to_owned(),
        "![first](badge.svg) ![second](badge.svg)".to_owned(),
        format!(
            "| Column |\n    | --- |\n    | {} |",
            "Retained cell ".repeat(180)
        ),
    ] {
        let source = format!(
            "CITATION.[^objects]\n\n[^objects]: {block}\n\n[^orphan]: Orphan $x^2$ keeps its source.\n"
        );
        let options = options();
        let bytes = render_pdf(&source, &options).unwrap();
        let mut original = options.clone();
        original.split_footnotes = false;
        pdf_content::assert_same_paint(&bytes, &render_pdf(&source, &original).unwrap());
    }
}

#[test]
fn continued_table_cells_preserve_inline_objects_links_and_note_ancestry() {
    let rows = 24;
    let mut source = String::from(
        "CITATION.[^table]\n\n[^table]:\n    | Key | Formula | Image |\n    | --- | --- | --- |\n",
    );
    for index in 0..rows {
        source.push_str(&format!(
            "    | CELL{index:03} | $\\frac{{x^2}}{{y_3}}$ | [![badge](badge.svg)](https://example.org/icon) |\n",
        ));
    }
    let options = options();
    let bytes = render_pdf(&source, &options).unwrap();
    let pdf = Pdf::new(&bytes);
    let runs: Vec<_> = pdf.pages.iter().map(|&page| pdf.runs(page)).collect();
    let text: String = runs.iter().flatten().map(|run| run.text.as_str()).collect();
    assert!(!text.contains("Notes"));
    let mut row_pages = BTreeSet::new();
    for index in 0..rows {
        let marker = format!("CELL{index:03}");
        assert_eq!(text.matches(&marker).count(), 1);
        let (page, row) = runs
            .iter()
            .enumerate()
            .find_map(|(page, runs)| {
                runs.iter()
                    .find(|run| run.text.contains(&marker))
                    .map(|run| (page, run))
            })
            .unwrap();
        assert!(row.size < 10.0);
        assert!(row.y >= 36.0 && row.y < 264.0);
        if index == 0 {
            assert_eq!(page, 0, "first table row stays with its citation");
        }
        row_pages.insert(page);
    }
    assert!(row_pages.len() >= 3);
    for tag in ["Formula", "Figure"] {
        let elements = pdf.elements(tag);
        assert_eq!(elements.len(), rows);
        for (id, page, bounds) in elements {
            assert!(pdf.ancestry(id).contains(&"Note"));
            assert!(pdf.ancestry(id).contains(&"TD"));
            inside(bounds, [36.0, 36.0, 324.0, 264.0]);
            let body_bottom = pdf
                .runs(page)
                .iter()
                .filter(|run| run.text.contains("CITATION"))
                .map(|run| run.y - run.size * 0.3)
                .fold(f64::INFINITY, f64::min);
            assert!(bounds[3] < body_bottom);
        }
    }
    let links = pdf
        .objects
        .values()
        .filter(|object| {
            object
                .dictionary
                .contains("/URI (https://example.org/icon)")
        })
        .collect::<Vec<_>>();
    assert_eq!(links.len(), rows);
    for (figure, link) in pdf.elements("Figure").into_iter().zip(links) {
        let link = rect(&link.dictionary, "/Rect [");
        inside(figure.2, link);
        inside(link, figure.2);
    }
    assert_eq!(
        bytes,
        render_pdf_document_emitted(
            &parse_markdown(&source),
            &options,
            PdfEmitOptions::monolithic()
        )
        .unwrap()
    );
}
