//! List paragraphs participate in same-size paragraph/page selection while
//! preserving painted text, hanging indents, labels and tagged ancestry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    BaselineGrid, Block, Document, Inline, List, ListItem, PdfEmitOptions, PdfOptions,
    PdfParagraphVariantStatus, parse_markdown, render_pdf, render_pdf_document_reported,
    render_pdf_reported,
};
use std::collections::BTreeMap;

const PROSE: &str = "The old stone bridge stood beside the river while the town woke slowly. \
    A small boat crossed the clear water and a child watched from the steps. \
    The morning light reached the houses along the bank and the shop doors opened. \
    People walked to work, shared news with friends, and stopped to hear the birds. \
    By noon the market was full of fresh bread, bright fruit, and flowers from the hills.";

fn prose() -> String {
    [PROSE; 3].join(" ")
}

fn options(width: f32, height: f32) -> PdfOptions {
    let mut opts = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = width + 72.0;
    opts.theme.page.size.height_pt = height + 72.0;
    opts.theme.page.margins = franken_markdown::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    opts
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

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
        let xref = std::str::from_utf8(&bytes[at + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let section: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(section[0], "0");
        let count: usize = section[1].parse().unwrap();
        let offsets = lines
            .take(count)
            .map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields[2] == "n" {
                    fields[0].parse().unwrap()
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
            .filter(|&at| at > start)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        let object = &self.bytes[start..end];
        assert!(
            object.starts_with(header.as_bytes()),
            "xref must point to the actual object"
        );
        &object[header.len()..]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let object = self.body(id);
        let end = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap_or(object.len());
        std::str::from_utf8(&object[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let object = self.body(id);
        let at = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&object[..at]).unwrap();
        let length = integer_after(dictionary, "/Length ");
        let payload = &object[at + 8..at + 8 + length];
        let bytes = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
        } else {
            assert!(
                !dictionary.contains("/Filter"),
                "extend the decoder for a new stream filter"
            );
            payload.to_vec()
        };
        String::from_utf8(bytes).unwrap()
    }
}

fn integer_after(dictionary: &str, key: &str) -> usize {
    // A paragraph's /S /P name is distinct from the later /P parent entry.
    // Read a numeric dictionary value, not the first occurrence of its spelling.
    let words: Vec<_> = dictionary.split_whitespace().collect();
    words
        .windows(2)
        .find_map(|pair| {
            (pair[0] == key.trim())
                .then(|| pair[1].parse().ok())
                .flatten()
        })
        .unwrap_or_else(|| panic!("missing numeric {key} in {dictionary}"))
}

fn unicode_map(objects: &Objects<'_>, font: usize) -> BTreeMap<u16, String> {
    let cmap = objects.stream(integer_after(objects.dictionary(font), "/ToUnicode "));
    let mut chars = false;
    let mut map = BTreeMap::new();
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            chars = true;
            continue;
        }
        if line == "endbfchar" {
            chars = false;
            continue;
        }
        if !chars {
            continue;
        }
        let entries: Vec<_> = line.split_whitespace().collect();
        let id = u16::from_str_radix(entries[0].trim_matches(['<', '>']), 16).unwrap();
        let encoded = entries[1].trim_matches(['<', '>']);
        let units = encoded
            .as_bytes()
            .chunks_exact(4)
            .map(|part| u16::from_str_radix(std::str::from_utf8(part).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        map.insert(id, String::from_utf16(&units).unwrap());
    }
    assert!(
        !map.is_empty(),
        "painted font must have a selectable glyph map"
    );
    map
}

fn painted_pages(pdf: &[u8]) -> Vec<String> {
    decode_pages(pdf, false)
}

fn prose_pages(pdf: &[u8]) -> Vec<String> {
    // This fixture has no literal hyphens. Only a painted trailing hyphen
    // followed by a lower physical baseline can be a layout discretionary.
    // Never erase internal hyphens or normalize the official Markdown corpus.
    assert!(!PROSE.contains('-'));
    decode_pages(pdf, true)
}

fn decode_pages(pdf: &[u8], dehyphenate_fixture: bool) -> Vec<String> {
    let objects = Objects::new(pdf);
    let page_tree = (1..objects.offsets.len())
        .find(|&id| objects.dictionary(id).contains("/Type /Pages "))
        .unwrap();
    let kids = objects
        .dictionary(page_tree)
        .split_once("/Kids [")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(kids.len() % 3, 0);
    let mut page_text = Vec::new();
    for kid in kids.chunks_exact(3) {
        assert_eq!(&kid[1..], &["0", "R"]);
        let page = objects.dictionary(kid[0].parse().unwrap());
        let resources = page
            .split_once("/Font <<")
            .unwrap()
            .1
            .split_once(">>")
            .unwrap()
            .0
            .split_whitespace()
            .collect::<Vec<_>>();
        assert_eq!(resources.len() % 4, 0);
        let fonts = resources
            .chunks_exact(4)
            .map(|entry| (entry[0], unicode_map(&objects, entry[1].parse().unwrap())))
            .collect::<BTreeMap<_, _>>();
        let content = objects.stream(integer_after(page, "/Contents "));
        let mut painted = String::new();
        let mut last_y: Option<f32> = None;
        for line in content.lines().filter(|line| line.contains("BT /F")) {
            let separated = line
                .replace('[', " [ ")
                .replace(']', " ] ")
                .replace('<', " <")
                .replace('>', "> ");
            let words: Vec<_> = separated.split_whitespace().collect();
            let mut font = None;
            let mut mode = 0;
            let mut y: Option<f32> = None;
            let mut text = String::new();
            for (index, word) in words.iter().enumerate() {
                match *word {
                    "Tf" => font = Some(&fonts[words[index - 2]]),
                    "Tr" => mode = words[index - 1].parse::<u8>().unwrap(),
                    "Tm" => y = Some(words[index - 1].parse().unwrap()),
                    "Tj" | "TJ" => {
                        if mode != 3 && !text.is_empty() {
                            if dehyphenate_fixture
                                && painted.ends_with('-')
                                && text.chars().next().is_some_and(char::is_alphabetic)
                                && last_y
                                    .zip(y)
                                    .is_some_and(|(before, after)| after < before - 0.01)
                            {
                                painted.pop();
                            }
                            painted.push_str(&text);
                            last_y = y;
                        }
                        text.clear();
                    }
                    hex if hex.starts_with('<') && hex.ends_with('>') => {
                        let font = font.expect("painted glyph needs an active font");
                        let hex = hex.trim_matches(['<', '>']);
                        assert_eq!(hex.len() % 4, 0);
                        for glyph in hex.as_bytes().chunks_exact(4) {
                            let id = u16::from_str_radix(std::str::from_utf8(glyph).unwrap(), 16)
                                .unwrap();
                            text.push_str(
                                font.get(&id)
                                    .expect("painted glyph must have a Unicode mapping"),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        page_text.push(painted.chars().filter(|c| !c.is_whitespace()).collect());
    }
    page_text
}

fn body_streams(pdf: &[u8]) -> Vec<String> {
    let objects = Objects::new(pdf);
    (1..objects.offsets.len())
        .filter(|&id| objects.dictionary(id).contains("/Type /Page "))
        .map(|id| objects.stream(integer_after(objects.dictionary(id), "/Contents ")))
        .collect()
}

fn assert_body_size(pdf: &[u8], expected: f32) {
    let mut count = 0;
    for content in body_streams(pdf) {
        for line in content.lines().filter(|line| line.contains("BT /F")) {
            let fields: Vec<_> = line.split_whitespace().collect();
            for (index, field) in fields.iter().enumerate() {
                if *field == "Tf" {
                    let size: f32 = fields[index - 1].parse().unwrap();
                    assert_eq!(size, expected, "list optimization cannot shrink text");
                    count += 1;
                }
            }
        }
    }
    assert!(count > 0);
}

#[test]
fn long_list_paragraph_uses_a_same_measure_shape_to_avoid_an_extra_page() {
    let source = format!("- {}", prose());
    // Exactly the ordinary 440pt paragraph measure plus its 19pt list gutter.
    let opts = options(459.0, 197.0);
    let mut original = opts.clone();
    original.optimal_pagination = false;
    let before = render_pdf(&source, &original).unwrap();
    let after = render_pdf_reported(&source, &opts).unwrap();
    assert!(after.pagination.page_breaks.is_exact());
    assert_eq!(
        after.pagination.paragraph_variants.status,
        PdfParagraphVariantStatus::Planned
    );
    assert_eq!(after.pagination.paragraph_variants.selected_alternatives, 1);
    assert_eq!(painted_pages(&before).len(), 2);
    assert_eq!(painted_pages(&after.bytes).len(), 1);
    let expected = format!("•{}", compact(&prose()));
    assert_eq!(prose_pages(&before).concat(), expected);
    assert_eq!(prose_pages(&after.bytes).concat(), expected);
    assert_body_size(&before, 11.0);
    assert_body_size(&after.bytes, 11.0);
    let syntax = String::from_utf8_lossy(&after.bytes);
    for tag in ["/S /L ", "/S /LI ", "/S /LBody ", "/S /P "] {
        assert_eq!(syntax.matches(tag).count(), 1, "one logical {tag}");
    }
    let document = parse_markdown(&source);
    let monolithic =
        render_pdf_document_reported(&document, &opts, PdfEmitOptions::monolithic()).unwrap();
    assert_eq!(after, monolithic);
}

#[test]
fn ordered_marker_overflow_preserves_every_public_ast_item() {
    let doc = Document {
        blocks: vec![Block::List(List {
            ordered: true,
            start: u64::MAX,
            tight: true,
            items: ["First", "Second"]
                .into_iter()
                .map(|text| ListItem {
                    task: None,
                    blocks: vec![Block::Paragraph(vec![Inline::Text(text.into())])],
                })
                .collect(),
        })],
    };
    let report =
        render_pdf_document_reported(&doc, &PdfOptions::default(), PdfEmitOptions::default())
            .unwrap();
    assert_eq!(
        painted_pages(&report.bytes).concat(),
        format!("{}.First{}.Second", u64::MAX, u128::from(u64::MAX) + 1)
    );
}

fn paragraph_list_depths(pdf: &[u8]) -> Vec<usize> {
    let objects = Objects::new(pdf);
    let mut result = Vec::new();
    for id in 1..objects.offsets.len() {
        let dictionary = objects.dictionary(id);
        if !dictionary.contains("/Type /StructElem /S /P ") {
            continue;
        }
        let mut parent = integer_after(dictionary, "/P ");
        let mut lists = 0;
        let mut ancestry = Vec::new();
        let mut steps = 0;
        while parent != 0 {
            let dictionary = objects.dictionary(parent);
            if dictionary.contains("/S /L ") {
                lists += 1;
            }
            if dictionary.contains("/S /Document ") {
                break;
            }
            ancestry.push(
                dictionary
                    .split_once("/S /")
                    .unwrap()
                    .1
                    .split_whitespace()
                    .next()
                    .unwrap(),
            );
            parent = integer_after(dictionary, "/P ");
            steps += 1;
            assert!(steps < objects.offsets.len(), "acyclic tagged ancestry");
        }
        assert_eq!(ancestry.len(), lists * 3, "complete list/container chain");
        for triple in ancestry.chunks_exact(3) {
            assert_eq!(
                triple,
                ["LBody", "LI", "L"],
                "each paragraph retains its actual item owner"
            );
        }
        result.push(lists);
    }
    result
}

#[test]
fn nested_linked_list_alternative_keeps_labels_indents_and_tagged_ancestry() {
    let source = format!("- Outer.\n\n  - [{}](https://example.com/river)\n", prose());
    let mut opts = options(478.0, 220.0);
    opts.strict_pagination = true;
    let report = render_pdf_reported(&source, &opts).unwrap();
    assert!(report.pagination.page_breaks.is_exact());
    assert!(report.pagination.paragraph_variants.selected_alternatives > 0);
    assert_eq!(painted_pages(&report.bytes).len(), 1);
    assert_eq!(
        prose_pages(&report.bytes).concat(),
        format!("•Outer.•{}", compact(&prose()))
    );
    assert_body_size(&report.bytes, 11.0);
    assert_eq!(paragraph_list_depths(&report.bytes), vec![1, 2]);
    let syntax = String::from_utf8_lossy(&report.bytes);
    assert_eq!(syntax.matches("/S /LI ").count(), 2);
    assert!(syntax.contains("/URI (https://example.com/river)"));

    // Inspect actual text matrices: both paragraph bodies retain their own
    // hanging content column after the nested alternative is installed.
    let positions: Vec<f32> = body_streams(&report.bytes)
        .iter()
        .flat_map(|content| {
            content
                .lines()
                .filter(|line| line.contains(" Tm "))
                .map(|line| {
                    let fields: Vec<_> = line.split_whitespace().collect();
                    let matrix = fields.iter().position(|field| *field == "Tm").unwrap();
                    fields[matrix - 2].parse().unwrap()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        positions.iter().any(|x| (*x - 74.0).abs() < 0.02),
        "nested body at 36 + 19 + 19pt: {positions:?}"
    );
}

#[test]
fn loose_list_paragraphs_use_candidates_without_repeating_the_item_label() {
    let source = format!("9. Opening.\n\n   {}\n", prose());
    let opts = options(470.0, 210.0);
    let report = render_pdf_reported(&source, &opts).unwrap();
    assert!(report.pagination.paragraph_variants.attempted_paragraphs > 0);
    assert!(report.pagination.paragraph_variants.offered_paragraphs > 0);
    assert_eq!(
        prose_pages(&report.bytes).concat(),
        format!("9.Opening.{}", compact(&prose()))
    );
    let syntax = String::from_utf8_lossy(&report.bytes);
    assert_eq!(syntax.matches("/S /LI ").count(), 1);
    assert_eq!(
        painted_pages(&report.bytes).concat().matches("9.").count(),
        1
    );
    assert_eq!(paragraph_list_depths(&report.bytes), vec![1, 1]);
    assert_body_size(&report.bytes, 11.0);
}

#[test]
fn list_grid_and_page_notes_keep_complete_citations_and_regular_baselines() {
    let source = format!(
        "- {} A note[^one].\n\n[^one]: Citation retained.\n",
        prose()
    );
    let mut opts = options(459.0, 560.0);
    opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
    let report = render_pdf_reported(&source, &opts).unwrap();
    assert!(report.pagination.page_breaks.is_exact());
    assert!(report.pagination.paragraph_variants.attempted_paragraphs > 0);
    assert!(report.pagination.paragraph_variants.offered_paragraphs > 0);
    let painted = prose_pages(&report.bytes).concat();
    assert!(painted.contains(&compact(&prose())));
    assert_eq!(painted.matches("Citationretained.").count(), 1);
    assert!(String::from_utf8_lossy(&report.bytes).contains("/Subtype /Link"));
    let top = opts.theme.page.size.height_pt - opts.theme.page.margins.top_pt;
    let mut baselines = 0;
    for stream in body_streams(&report.bytes) {
        for line in stream.lines().filter(|line| line.contains(" Tm ")) {
            let fields: Vec<_> = line.split_whitespace().collect();
            let font = fields.iter().position(|field| *field == "Tf").unwrap();
            let size: f32 = fields[font - 1].parse().unwrap();
            if size != 11.0 {
                continue;
            }
            let matrix = fields.iter().position(|field| *field == "Tm").unwrap();
            let y: f32 = fields[matrix - 1].parse().unwrap();
            let steps = (top - y) / 14.0;
            assert!(
                (steps - steps.round()).abs() < 0.002,
                "body baseline remains on-grid: {line}"
            );
            baselines += 1;
        }
    }
    assert!(baselines >= 4);
    let mono = render_pdf_document_reported(
        &parse_markdown(&source),
        &opts,
        PdfEmitOptions::monolithic(),
    )
    .unwrap();
    assert_eq!(report, mono);
}

#[test]
fn selected_task_list_shape_keeps_one_checked_vector_marker_and_selectable_label() {
    let source = format!("- [x] {}", prose());
    let opts = options(465.0, 197.0);
    let report = render_pdf_reported(&source, &opts).unwrap();
    assert!(report.pagination.page_breaks.is_exact());
    assert_eq!(
        report.pagination.paragraph_variants.selected_alternatives,
        1
    );
    assert_eq!(painted_pages(&report.bytes).len(), 1);
    // The checkbox is vector art. Its selectable [x] anchor uses render mode 3,
    // so the visible-glyph decoder must recover exactly the paragraph body.
    assert_eq!(prose_pages(&report.bytes).concat(), compact(&prose()));
    assert_eq!(paragraph_list_depths(&report.bytes), vec![1]);
    assert_body_size(&report.bytes, 11.0);
    let streams = body_streams(&report.bytes).concat();
    assert_eq!(streams.matches("f 1 1 1 RG").count(), 1, "one checked box");
    assert_eq!(
        streams.matches("3 Tr").count(),
        1,
        "one selectable task label"
    );
    let syntax = String::from_utf8_lossy(&report.bytes);
    for scalar in ["<005B>", "<0078>", "<005D>"] {
        assert!(
            syntax.contains(scalar),
            "task label remains selectable: {scalar}"
        );
    }
}

#[test]
fn selected_list_shape_reanchors_numbered_notes_on_the_baseline_grid() {
    // Attaching the citation to the existing paragraph retains an admissible
    // shorter shape. The 448pt body lies between its one-page capacity and the
    // original 14-line shape's capacity, including the reserved note region.
    let source = format!("- {}[^one]\n\n[^one]: Citation retained.\n", prose());
    let mut opts = options(459.0, 448.0);
    opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
    opts.strict_pagination = true;
    let report = render_pdf_reported(&source, &opts).unwrap();
    assert!(report.pagination.page_breaks.is_exact());
    assert_eq!(
        report.pagination.paragraph_variants.selected_alternatives,
        1
    );
    let pages = prose_pages(&report.bytes);
    assert_eq!(pages.len(), 1);
    let painted = pages.concat();
    assert!(painted.contains(&compact(&prose())));
    assert_eq!(painted.matches("Citationretained.").count(), 1);
    assert_eq!(painted.matches('•').count(), 1);
    let syntax = String::from_utf8_lossy(&report.bytes);
    assert!(syntax.contains("/Subtype /Link"));
    let top = opts.theme.page.size.height_pt - opts.theme.page.margins.top_pt;
    let mut body_baselines = 0;
    for stream in body_streams(&report.bytes) {
        for line in stream.lines().filter(|line| line.contains(" Tm ")) {
            let fields: Vec<_> = line.split_whitespace().collect();
            let font = fields.iter().position(|field| *field == "Tf").unwrap();
            if fields[font - 1].parse::<f32>().unwrap() != 11.0 {
                continue;
            }
            let matrix = fields.iter().position(|field| *field == "Tm").unwrap();
            let y: f32 = fields[matrix - 1].parse().unwrap();
            let steps = (top - y) / 14.0;
            assert!(
                (steps - steps.round()).abs() < 0.002,
                "selected baseline stays on-grid: {line}"
            );
            body_baselines += 1;
        }
    }
    assert!(body_baselines >= 13);
    let mono = render_pdf_document_reported(
        &parse_markdown(&source),
        &opts,
        PdfEmitOptions::monolithic(),
    )
    .unwrap();
    assert_eq!(report, mono);
}
