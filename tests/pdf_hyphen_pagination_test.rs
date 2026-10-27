//! Page keeps follow actual word breaks, not hyphen-shaped source punctuation.
//! Inspect emitted page/font references and painted glyphs, independently of
//! the renderer's measured-line and verification-text helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PdfEmitOptions, PdfOptions, parse_markdown, render_pdf, render_pdf_document_reported,
    render_pdf_reported,
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
        for line in content.lines().filter(|line| line.contains("BT /F")) {
            let separated = line
                .replace('[', " [ ")
                .replace(']', " ] ")
                .replace('<', " <")
                .replace('>', "> ");
            let words: Vec<_> = separated.split_whitespace().collect();
            let mut font = None;
            let mut mode = 0;
            let mut text = String::new();
            for (index, word) in words.iter().enumerate() {
                match *word {
                    "Tf" => font = Some(&fonts[words[index - 2]]),
                    "Tr" => mode = words[index - 1].parse::<u8>().unwrap(),
                    "Tj" | "TJ" => {
                        if mode != 3 {
                            painted.push_str(&text);
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

fn options() -> PdfOptions {
    let mut options = PdfOptions {
        strict_pagination: true,
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 400.0;
    options.theme.page.size.height_pt = 220.0;
    options.theme.page.margins = franken_markdown::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options
}

fn assert_literal_lines(source: &str, hyphen: char, name: &str) {
    let options = options();
    let rendered = render_pdf_reported(source, &options).unwrap_or_else(|error| {
        panic!("{name}: a literal hyphen is not a word continuation: {error}")
    });
    assert!(rendered.pagination.page_breaks.is_exact(), "{name}");
    let pages = painted_pages(&rendered.bytes);
    assert!(pages.len() > 1, "{name}: exercise real page boundaries");
    let painted = pages.concat();
    for index in 0..24 {
        let marker = format!("Row{index:02}");
        assert_eq!(
            painted.matches(&marker).count(),
            1,
            "{name}: lost or duplicated {marker}"
        );
    }
    assert_eq!(
        painted.matches(hyphen).count(),
        24,
        "{name}: preserve every source hyphen"
    );
    let monolithic = render_pdf_document_reported(
        &parse_markdown(source),
        &options,
        PdfEmitOptions::monolithic(),
    )
    .unwrap();
    assert_eq!(
        rendered, monolithic,
        "{name}: both emitters retain page-boundary provenance"
    );
    let mut best_effort = options;
    best_effort.strict_pagination = false;
    best_effort.optimal_pagination = true;
    assert_eq!(
        rendered.bytes,
        render_pdf(source, &best_effort).unwrap(),
        "{name}"
    );
}

#[test]
fn literal_hyphens_at_hard_breaks_do_not_make_paragraphs_unsplittable() {
    for hyphen in ['-', '\u{2010}'] {
        let lines: Vec<_> = (0..24)
            .map(|index| format!("Row{index:02} {hyphen}"))
            .collect();
        assert_literal_lines(&lines.join("  \n"), hyphen, "hard-line paragraph");
        let quoted = lines
            .iter()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("  \n");
        assert_literal_lines(&quoted, hyphen, "quoted hard-line paragraph");
        let listed = format!("- {}", lines.join("  \n  "));
        assert_literal_lines(&listed, hyphen, "list hard-line paragraph");
    }
}

#[test]
fn literal_hyphens_at_code_and_table_boundaries_keep_strict_exports_feasible() {
    let code = format!(
        "```text\n{}```\n",
        (0..24)
            .map(|index| format!("Row{index:02} -\n"))
            .collect::<String>()
    );
    assert_literal_lines(&code, '-', "source-code lines");
    let table = format!(
        "| Row | Value |\n| --- | --- |\n{}",
        (0..24)
            .map(|index| format!("| Row{index:02} | - |\n"))
            .collect::<String>()
    );
    assert_literal_lines(&table, '-', "table with missing values");
}

#[test]
fn genuine_hyphenated_word_continuations_stay_on_the_same_page() {
    let mut options = options();
    options.theme.page.size.width_pt = 144.0;
    options.theme.page.size.height_pt = 177.0;
    // The odd line capacity makes a hyphenated fragment at the page foot a
    // tempting split. The original words contain no hyphens, so every drawn
    // hyphen is an inserted dictionary break, independently of layout metadata.
    let word = "characteristically";
    let source = std::iter::repeat_n(word, 24).collect::<Vec<_>>().join(" ");
    let rendered = render_pdf_reported(&source, &options).unwrap();
    assert!(rendered.pagination.page_breaks.is_exact());
    let pages = painted_pages(&rendered.bytes);
    assert!(pages.len() >= 3);
    assert!(
        pages.iter().any(|page| page.contains('-')),
        "fixture must really hyphenate words"
    );
    for page in &pages[..pages.len() - 1] {
        assert!(
            !page.ends_with('-'),
            "a word continuation crossed a page: {page}"
        );
    }
    assert_eq!(pages.concat().replace('-', ""), word.repeat(24));
}

#[test]
fn chosen_source_hyphen_breaks_keep_compound_words_together() {
    let mut options = options();
    options.theme.page.size.width_pt = 144.0;
    options.theme.page.size.height_pt = 177.0;
    let word = "aaaaaaaa-bbbbbbbbb";
    let source = std::iter::repeat_n(word, 24).collect::<Vec<_>>().join(" ");
    let rendered = render_pdf_reported(&source, &options).unwrap();
    assert!(rendered.pagination.page_breaks.is_exact());
    let pages = painted_pages(&rendered.bytes);
    assert!(pages.len() >= 3);
    for page in &pages[..pages.len() - 1] {
        assert!(
            !page.ends_with('-'),
            "a real intra-word source-hyphen break crossed a page: {page}"
        );
    }
    assert_eq!(pages.concat(), word.repeat(24));
}
