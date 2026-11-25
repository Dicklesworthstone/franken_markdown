//! The verification layer must agree with independently decoded publication.
//! These checks read the PDF xref, page tree, local annotations, ToUnicode maps
//! and painted text matrices; renderer layout metadata is never their oracle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{PageMargins, PdfASettings, PdfOptions};

#[path = "support/pdf_content.rs"]
mod pdf_content;

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
            .rposition(|value| value == marker)
            .unwrap();
        let xref = std::str::from_utf8(&bytes[start + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut rows = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(rows.next(), Some("xref"));
        let header: Vec<_> = rows.next().unwrap().split_whitespace().collect();
        assert_eq!(header[0], "0");
        let count: usize = header[1].parse().unwrap();
        assert_eq!(rows.next().unwrap().trim(), "0000000000 65535 f");
        let mut offsets = vec![0];
        for id in 1..count {
            let fields: Vec<_> = rows.next().unwrap().split_whitespace().collect();
            assert_eq!(&fields[1..], &["00000", "n"]);
            let offset: usize = fields[0].parse().unwrap();
            assert!(offset < xref);
            assert!(bytes[offset..].starts_with(format!("{id} 0 obj\n").as_bytes()));
            offsets.push(offset);
        }
        assert_eq!(rows.next(), Some("trailer"));
        Self {
            bytes,
            offsets,
            xref,
        }
    }

    fn body(&self, id: usize) -> &'a [u8] {
        let offset = self.offsets[id];
        let end = self
            .offsets
            .iter()
            .copied()
            .filter(|&next| next > offset)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        &self.bytes[offset + header.len()..end]
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
        let dictionary = self.dictionary(id);
        let length = integer(dictionary, "/Length ");
        let bytes = &body[start + 8..start + 8 + length];
        let decoded = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(bytes, 1 << 26).unwrap()
        } else {
            assert!(!dictionary.contains("/Filter"));
            bytes.to_vec()
        };
        String::from_utf8(decoded).unwrap()
    }
}

fn integer(dictionary: &str, key: &str) -> usize {
    dictionary
        .split_once(key)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn unicode_map(objects: &Objects<'_>, font: usize) -> BTreeMap<u16, String> {
    let cmap = objects.stream(integer(objects.dictionary(font), "/ToUnicode "));
    let mut active = false;
    let mut map = BTreeMap::new();
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            active = true;
        } else if line == "endbfchar" {
            active = false;
        } else if active {
            let fields: Vec<_> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 2);
            let cid = u16::from_str_radix(fields[0].trim_matches(['<', '>']), 16).unwrap();
            let (units, remainder) = fields[1]
                .trim_matches(['<', '>'])
                .as_bytes()
                .as_chunks::<4>();
            assert!(remainder.is_empty());
            let units: Vec<_> = units
                .iter()
                .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
                .collect();
            assert!(map.insert(cid, String::from_utf16(&units).unwrap()).is_none());
        }
    }
    assert!(!map.is_empty());
    map
}

fn references(dictionary: &str, key: &str) -> Vec<usize> {
    let Some((_, tail)) = dictionary.split_once(key) else {
        return Vec::new();
    };
    let values: Vec<_> = tail.split_once(']').unwrap().0.split_whitespace().collect();
    let (references, remainder) = values.as_chunks::<3>();
    assert!(remainder.is_empty());
    references
        .iter()
        .map(|reference| {
            assert_eq!(&reference[1..], &["0", "R"]);
            reference[0].parse().unwrap()
        })
        .collect()
}

fn page_ids(objects: &Objects<'_>) -> Vec<usize> {
    let catalog = (1..objects.offsets.len())
        .find(|&id| objects.dictionary(id).contains("/Type /Catalog "))
        .unwrap();
    let root = objects.dictionary(integer(objects.dictionary(catalog), "/Pages "));
    let pages = references(root, "/Kids [");
    assert!(!pages.is_empty());
    for &id in &pages {
        assert!(objects.dictionary(id).contains("/Type /Page "));
    }
    pages
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Destination {
    page: usize,
    y: f32,
}

fn local_destinations(bytes: &[u8]) -> Vec<Option<Destination>> {
    let objects = Objects::new(bytes);
    let pages = page_ids(&objects);
    let mut result = Vec::new();
    for &page in &pages {
        for id in references(objects.dictionary(page), "/Annots [") {
            let dictionary = objects.dictionary(id);
            assert!(dictionary.contains("/Subtype /Link"));
            assert!(!dictionary.contains("fmd-book-pdf-nav/"));
            assert!(!dictionary.contains("/S /URI"));
            let Some((_, tail)) = dictionary.split_once("/Dest [") else {
                result.push(None);
                continue;
            };
            let values: Vec<_> = tail.split_once(']').unwrap().0.split_whitespace().collect();
            assert_eq!(values.len(), 7);
            assert_eq!(&values[1..5], &["0", "R", "/XYZ", "null"]);
            assert_eq!(values[6], "null");
            let object: usize = values[0].parse().unwrap();
            let page = pages.iter().position(|&id| id == object).unwrap() + 1;
            let y: f32 = values[5].parse().unwrap();
            assert!(y.is_finite() && y > 0.0);
            result.push(Some(Destination { page, y }));
        }
    }
    result
}

#[derive(Debug)]
struct PaintedLine {
    text: String,
    x: f32,
    y: f32,
    size: f32,
    kind: &'static str,
}

fn close(left: f32, right: f32) -> bool {
    // This writer rounds text matrices and sizes to two decimal places.
    (left - right).abs() <= 0.011
}

fn painted_lines(pdf: &[u8], options: &PdfOptions) -> Vec<Vec<PaintedLine>> {
    let objects = Objects::new(pdf);
    page_ids(&objects)
        .into_iter()
        .map(|page| {
            let dictionary = objects.dictionary(page);
            let resources: Vec<_> = dictionary
                .split_once("/Font <<")
                .unwrap()
                .1
                .split_once(">>")
                .unwrap()
                .0
                .split_whitespace()
                .collect();
            let (resources, remainder) = resources.as_chunks::<4>();
            assert!(remainder.is_empty());
            let fonts: BTreeMap<_, _> = resources
                .iter()
                .map(|resource| {
                    assert_eq!(&resource[2..], &["0", "R"]);
                    (
                        resource[0],
                        unicode_map(&objects, resource[1].parse().unwrap()),
                    )
                })
                .collect();
            let content = objects.stream(integer(dictionary, "/Contents "));
            let mut result: Vec<PaintedLine> = Vec::new();
            let mut font = None;
            let mut mode = 0;
            let mut x = 0.0_f32;
            let mut y = 0.0_f32;
            let mut size = 0.0_f32;
            let mut marks = Vec::<Vec<&str>>::new();
            for operation in pdf_content::operations(&content) {
                match operation.operator {
                    "Tf" => {
                        font = Some(&fonts[operation.operands[0]]);
                        size = operation.operands[1].parse().unwrap();
                    }
                    "Tr" => mode = operation.operands[0].parse::<u8>().unwrap(),
                    "Tm" => {
                        x = operation.operands[4].parse().unwrap();
                        y = operation.operands[5].parse().unwrap();
                    }
                    "BDC" | "BMC" => marks.push(operation.operands),
                    "EMC" => {
                        marks.pop().expect("balanced marked content");
                    }
                    "Tj" | "TJ" if mode != 3 => {
                        let text: String = pdf_content::glyph_ids(&operation)
                            .iter()
                            .map(|cid| font.expect("selected font")[cid].as_str())
                            .collect();
                        if text.is_empty() {
                            continue;
                        }
                        // Repeated table headers are also pagination artifacts,
                        // but remain in the body, below the top margin band.
                        // These fixtures use 36pt margins, which fitting retains.
                        let header = y > options.theme.page.size.height_pt
                            - options.theme.page.margins.top_pt
                            && marks.iter().any(|mark| mark.contains(&"/Header"));
                        let footer = y < options.theme.page.margins.bottom_pt
                            && marks.iter().any(|mark| mark.contains(&"/Footer"));
                        let kind = if header {
                            "running_header"
                        } else if footer {
                            "running_footer"
                        } else {
                            "body"
                        };
                        if let Some(line) = result.iter_mut().find(|line| {
                            line.kind == kind && close(line.y, y) && close(line.size, size)
                        }) {
                            line.text.push_str(&text);
                        } else {
                            result.push(PaintedLine {
                                text,
                                x,
                                y,
                                size,
                                kind,
                            });
                        }
                    }
                    _ => {}
                }
            }
            assert!(marks.is_empty());
            result
        })
        .collect()
}

fn assert_painted_text_matches(
    bytes: &[u8],
    options: &PdfOptions,
    pages: &[franken_markdown::pdf::VerifyPage],
) {
    let painted = painted_lines(bytes, options);
    assert_eq!(pages.len(), painted.len(), "physical page-tree agreement");
    for (index, (reported, painted)) in pages.iter().zip(&painted).enumerate() {
        assert_eq!(reported.number, index + 1);
        let reported: Vec<_> = reported
            .runs
            .iter()
            .filter(|run| !run.text.is_empty())
            .collect();
        assert_eq!(reported.len(), painted.len(), "page {}", index + 1);
        for (reported, painted) in reported.iter().zip(painted) {
            assert_eq!(reported.text, painted.text, "page {}", index + 1);
            assert!(close(reported.x, painted.x), "{reported:?} != {painted:?}");
            assert!(close(reported.y, painted.y), "{reported:?} != {painted:?}");
            assert!(close(reported.size, painted.size), "{reported:?} != {painted:?}");
            let kind = match reported.kind {
                "running_header" | "running_footer" => reported.kind,
                _ => "body",
            };
            assert_eq!(kind, painted.kind);
        }
    }
}

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn narrow_options() -> PdfOptions {
    let mut options = PdfOptions {
        lang: Some("en-US".into()),
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 260.0;
    options.theme.page.size.height_pt = 480.0;
    options.theme.page.margins = PageMargins {
        left_pt: 36.0,
        right_pt: 36.0,
        top_pt: 36.0,
        bottom_pt: 36.0,
    };
    options
}

#[test]
fn chapter_scoped_report_destinations_match_actual_pdf_link_objects() {
    let renderer = BookRenderer::new(&[
        input(
            "one.md",
            "# Same\n\n[LOCAL](#same) [CROSS](two.md#same) [LAND](two.md) [BROKEN](#absent)",
        ),
        input("two.md", "# Same\n\n[SELF](#same) [BACK](one.md#same)"),
    ])
    .unwrap();
    for toc in [false, true] {
        let options = PdfOptions {
            toc,
            ..PdfOptions::default()
        };
        let pdf = renderer
            .render_pdf_with_options(&options, PdfASettings::OFF)
            .unwrap();
        let report = renderer
            .verify_pdf_with_options(&options, PdfASettings::OFF)
            .unwrap();
        let objects = Objects::new(&pdf);
        assert_eq!(report.layout.page_count, page_ids(&objects).len());
        let annotations = local_destinations(&pdf);
        // Automatic TOC links precede the six short, unsplit authored links.
        assert!(annotations.len() >= 6);
        let authored = &annotations[annotations.len() - 6..];
        assert_eq!(report.destinations.len(), 6);
        for ((chapter, destination), emitted) in [
            ("one.md", "#same"),
            ("one.md", "two.md#same"),
            ("one.md", "two.md"),
            ("one.md", "#absent"),
            ("two.md", "#same"),
            ("two.md", "one.md#same"),
        ]
        .into_iter()
        .zip(authored)
        {
            let reported = report
                .destinations
                .iter()
                .find(|target| target.chapter == chapter && target.destination == destination)
                .unwrap();
            if let Some(emitted) = emitted {
                assert_eq!(reported.status, "resolved", "{reported:?}");
                assert_eq!(reported.page, Some(emitted.page), "{reported:?}");
                assert!(close(reported.y.unwrap(), emitted.y), "{reported:?} != {emitted:?}");
            } else {
                assert_eq!(reported.status, "unresolved", "{reported:?}");
                assert_eq!(reported.page, None);
                assert_eq!(reported.y, None);
            }
        }
        assert_eq!(authored[0], authored[5], "back-link retains first chapter");
        assert_eq!(authored[1], authored[4], "forward-link retains second chapter");
        assert!(authored[0].unwrap().page < authored[4].unwrap().page);
        assert!(authored[3].is_none(), "missing link must not invent a destination");
    }
}

#[test]
fn fitted_multilingual_contents_and_running_bands_match_painted_text() {
    const WORDS: &str = "verantwoordelijkheid onafhankelijk aardappel Donaudampfschifffahrt Computer constitution informatique extraordinary ";
    let text = WORDS.repeat(12);
    let renderer = BookRenderer::new(&[
        input(
            "de.md",
            &format!("---\ntitle: German chapter\nlang: de-DE\n---\n# First\n\n{text}"),
        ),
        input(
            "nl.md",
            &format!("---\ntitle: Dutch chapter\nlang: nl-NL\n---\n# Second\n\n{text}"),
        ),
    ])
    .unwrap();
    let mut options = narrow_options();
    options.toc = true;
    options.base_font_size = Some(12.0);
    options.fit_to_pages = Some(2);
    options.running.header.left = Some("{chapter}".into());
    options.running.footer.right = Some("{page} / {pages}".into());
    let pdf = renderer
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    let report = renderer
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    assert_painted_text_matches(&pdf, &options, &report.layout.pages);
    assert_eq!(report.layout.page_count, report.layout.pages.len());
    let runs: Vec<_> = report
        .layout
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .collect();
    assert!(runs.iter().any(|run| run.text == "German chapter"));
    assert!(runs.iter().any(|run| run.text == "Dutch chapter"));
    assert!(
        runs.iter().any(|run| {
            run.kind == "paragraph" && run.text.starts_with("German chapter")
        }),
        "generated contents must be present in the captured body"
    );
    assert!(
        runs.iter().any(|run| run.kind == "paragraph" && run.size < 12.0),
        "the fixture must exercise actual fitting, not only the option"
    );
    for page in &report.layout.pages {
        assert!(page.runs.iter().any(|run| {
            run.kind == "running_footer"
                && run.text == format!("{} / {}", page.number, report.layout.page_count)
        }));
    }
}

#[test]
fn repeated_table_headers_are_captured_on_every_page_that_paints_them() {
    let mut source = String::from("| HEADER | VALUE |\n| --- | --- |\n");
    for index in 0..55 {
        source.push_str(&format!("| Row{index} | Amount{index} |\n"));
    }
    let renderer = BookRenderer::new(&[input("table.md", &source)]).unwrap();
    let mut options = narrow_options();
    options.theme.page.size.height_pt = 260.0;
    let pdf = renderer
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    let report = renderer
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    assert!(report.layout.page_count > 2);
    assert_painted_text_matches(&pdf, &options, &report.layout.pages);
    for page in &report.layout.pages {
        assert!(page.runs.iter().any(|run| run.kind == "table_header"));
    }
}
