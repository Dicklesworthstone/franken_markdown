//! Read running titles from emitted page artifacts through the real PDF font
//! resources and ToUnicode maps. Renderer layout metadata is not the oracle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{
    BaselineGrid, PdfEmitOptions, PdfOptions, PdfPageEmission, parse_markdown, render_pdf,
    render_pdf_document_emitted,
};

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
        let start = body.windows(8).position(|part| part == b"\nstream\n").unwrap();
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

#[derive(Debug, Default)]
struct Page {
    header: String,
    footer: String,
    body: String,
    heading_baselines: BTreeSet<i64>,
}

/// Decode glyph shows only: ActualText, outlines and structure-tree titles
/// cannot make a missing or incorrect running title pass these checks.
fn pages(pdf: &[u8]) -> Vec<Page> {
    let objects = Objects::new(pdf);
    let root = objects.dictionary(integer(objects.dictionary(1), "/Pages "));
    let kids: Vec<_> = root
        .split_once("/Kids [")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .collect();
    let (kids, remainder) = kids.as_chunks::<3>();
    assert!(remainder.is_empty());
    kids.iter()
        .map(|reference| {
            assert_eq!(&reference[1..], &["0", "R"]);
            let dictionary = objects.dictionary(reference[0].parse().unwrap());
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
            let mut result = Page::default();
            let mut font = None;
            let mut mode = 0;
            let mut y = 0.0_f64;
            let mut marks = Vec::<Vec<&str>>::new();
            for operation in pdf_content::operations(&content) {
                match operation.operator {
                    "Tf" => font = Some(&fonts[operation.operands[0]]),
                    "Tr" => mode = operation.operands[0].parse::<u8>().unwrap(),
                    "Tm" => y = operation.operands[5].parse().unwrap(),
                    "BDC" | "BMC" => marks.push(operation.operands),
                    "EMC" => {
                        marks.pop().expect("balanced marked content");
                    }
                    "Tj" | "TJ" if mode != 3 => {
                        let text: String = pdf_content::glyph_ids(&operation)
                            .iter()
                            .map(|cid| font.expect("selected font")[cid].as_str())
                            .collect();
                        let header = marks.iter().any(|mark| mark.contains(&"/Header"));
                        let footer = marks.iter().any(|mark| mark.contains(&"/Footer"));
                        if header || footer {
                            assert!(marks.iter().any(|mark| {
                                mark.first() == Some(&"/Artifact")
                                    && mark.contains(&"/Pagination")
                            }));
                            if header {
                                result.header.push_str(&text);
                            } else {
                                result.footer.push_str(&text);
                            }
                        } else {
                            result.body.push_str(&text);
                            if marks.iter().any(|mark| {
                                mark.first().is_some_and(|tag| {
                                    matches!(*tag, "/H1" | "/H2" | "/H3" | "/H4" | "/H5" | "/H6")
                                })
                            }) {
                                result.heading_baselines.insert((y * 1000.0).round() as i64);
                            }
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

fn options() -> PdfOptions {
    let mut options = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    options.running.header.left = Some("{chapter}".into());
    options.running.footer.left = Some("{section}".into());
    options
}

fn contexts(pages: &[Page]) -> Vec<(&str, &str)> {
    pages
        .iter()
        .map(|page| (page.header.as_str(), page.footer.as_str()))
        .collect()
}

#[test]
fn first_heading_selects_a_coherent_pair_and_last_heading_carries_forward() {
    let source = concat!(
        "# North\n\n## Introduction\n\nBody one.\n\n<!-- pagebreak -->\n\n",
        "## First section\n\nBody two.\n\n### Last subsection\n\n",
        "<!-- pagebreak -->\n\nContinuation without a heading.\n\n",
        "<!-- pagebreak -->\n\n## Old chapter section\n\n# South\n\n",
        "<!-- pagebreak -->\n\nFinal continuation.\n",
    );
    let pages = pages(&render_pdf(source, &options()).unwrap());
    assert_eq!(
        contexts(&pages),
        [
            ("North", "North"),
            ("North", "First section"),
            ("North", "Last subsection"),
            ("North", "Old chapter section"),
            ("South", "South"),
        ]
    );
}

#[test]
fn heading_free_pages_do_not_invent_context_from_document_metadata() {
    let mut options = options();
    options.title = Some("Metadata title".into());
    options.author = Some("Metadata author".into());
    let source = "Preface.\n\n<!-- pagebreak -->\n\n## First section\n\nBody.\n";
    let pages = pages(&render_pdf(source, &options).unwrap());
    assert_eq!(contexts(&pages), [("", ""), ("", "First section")]);
}

#[test]
fn wrapped_heading_uses_complete_plain_source_without_recursive_tokens() {
    let title = "Curious {pages} café chapter with a deliberately wrapped heading";
    let source = "# *Curious* `{pages}` café chapter with a deliberately wrapped heading\n\nBody.\n";
    let pages = pages(&render_pdf(source, &options()).unwrap());
    assert_eq!(pages.len(), 1);
    assert!(
        pages[0].heading_baselines.len() > 1,
        "fixture heading must actually wrap"
    );
    assert_eq!(pages[0].header, title);
    assert_eq!(pages[0].footer, title);
}

#[test]
fn footnote_and_generated_toc_headings_do_not_change_running_context() {
    let mut source = String::from(
        "# ACTUALCHAPTER\n\nCitation.[^small]\n\n<!-- pagebreak -->\n\nBody continues.\n\n\
         [^small]: # FOOTNOTECHAPTER\n\n    ## FOOTNOTESECTION\n\n    Short note.\n\n\
         [^large]: # ENDNOTECHAPTER\n\n",
    );
    for _ in 0..35 {
        source.push_str(
            "    A complete unreferenced note paragraph continues in the trailing notes.\n\n",
        );
    }
    let mut options = options();
    options.toc = true;
    let pages = pages(&render_pdf(&source, &options).unwrap());
    assert!(pages.len() >= 3, "fixture must exercise continued endnotes");
    let body: String = pages.iter().map(|page| page.body.as_str()).collect();
    for marker in [
        "FOOTNOTECHAPTER",
        "FOOTNOTESECTION",
        "ENDNOTECHAPTER",
        "Notes",
    ] {
        assert!(body.contains(marker), "fixture did not paint {marker}");
    }
    assert!(pages.iter().any(|page| page.header == "ACTUALCHAPTER"));
    let mut reached_body = false;
    for page in &pages {
        reached_body |= page.header == "ACTUALCHAPTER";
        let expected = if reached_body { "ACTUALCHAPTER" } else { "" };
        assert_eq!(page.header, expected);
        assert_eq!(page.footer, expected);
    }
}

#[test]
fn skipping_first_page_keeps_its_context_for_later_pages() {
    let mut options = options();
    options.running.skip_first_page = true;
    let source = "# Hidden on first page\n\nBody.\n\n<!-- pagebreak -->\n\nContinuation.\n";
    let pages = pages(&render_pdf(source, &options).unwrap());
    assert_eq!(
        contexts(&pages),
        [("", ""), ("Hidden on first page", "Hidden on first page")]
    );
}

#[test]
fn natural_pagination_and_both_emitters_use_the_final_heading_pages() {
    let paragraph = "A normal paragraph fills the page while preserving ordinary measured text and readable spacing. ".repeat(4);
    let mut source = String::from("# ALPHACHAPTER\n\n");
    for _ in 0..14 {
        source.push_str(&paragraph);
        source.push_str("\n\n");
    }
    source.push_str("## BETASECTION\n\n");
    for _ in 0..14 {
        source.push_str(&paragraph);
        source.push_str("\n\n");
    }
    let mut options = options();
    options.optimal_pagination = true;
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
    let document = parse_markdown(&source);
    let render = |emission| {
        render_pdf_document_emitted(
            &document,
            &options,
            PdfEmitOptions {
                emission,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let chunked = render(PdfPageEmission::Chunked);
    assert_eq!(chunked, render(PdfPageEmission::Monolithic));
    let pages = pages(&chunked);
    assert!(pages.len() >= 3, "fixture must span natural page boundaries");
    let section_page = pages
        .iter()
        .position(|page| page.body.contains("BETASECTION"))
        .expect("new section must actually be painted");
    assert!(section_page > 0 && section_page + 1 < pages.len());
    for (index, page) in pages.iter().enumerate() {
        assert_eq!(page.header, "ALPHACHAPTER");
        assert_eq!(
            page.footer,
            if index < section_page {
                "ALPHACHAPTER"
            } else {
                "BETASECTION"
            }
        );
    }
    let mut plain = options.clone();
    plain.running = Default::default();
    let plain = render_pdf(&source, &plain).unwrap();
    let plain_text = String::from_utf8_lossy(&plain);
    let chrome_text = String::from_utf8_lossy(&chunked);
    assert_eq!(
        plain_text.matches("/StructElem").count(),
        chrome_text.matches("/StructElem").count()
    );
    assert_eq!(pages.len(), self::pages(&plain).len());
}

#[test]
fn book_navigation_and_chapter_local_notes_preserve_running_titles() {
    let mut book = BookRenderer::new(&[
        BookInput {
            path: "first.md".into(),
            source: "# NORTHCHAPTER\n\n[Next](second.md#southchapter). Citation.[^n]\n\n[^n]: # NOTACHAPTER\n\n    First note.\n".into(),
        },
        BookInput {
            path: "second.md".into(),
            source: "# SOUTHCHAPTER\n\n[Back](first.md#northchapter). Citation.[^n]\n\n<!-- pagebreak -->\n\nContinuation.\n\n[^n]: # ANOTHERNOTE\n\n    Second note.\n".into(),
        },
    ])
    .unwrap();
    book.options_mut().running = options().running;
    let pdf = book.render_pdf().unwrap();
    let pages = pages(&pdf);
    assert_eq!(
        contexts(&pages),
        [
            ("NORTHCHAPTER", "NORTHCHAPTER"),
            ("SOUTHCHAPTER", "SOUTHCHAPTER"),
            ("SOUTHCHAPTER", "SOUTHCHAPTER"),
        ]
    );
    let body: String = pages.iter().map(|page| page.body.as_str()).collect();
    assert!(body.contains("NOTACHAPTER") && body.contains("ANOTHERNOTE"));
    assert_eq!(pdf, book.render_pdf().unwrap());
}
