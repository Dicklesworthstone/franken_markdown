//! Default PDF pagination keeps complete heading text and its underline on the
//! same page. Decode emitted page resources, CIDs and rule strokes independently
//! of layout/planner helpers, so an orphan underline cannot satisfy the check.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, PageMargins, PdfEmitOptions, PdfImageAsset, PdfOptions, parse_markdown, render_pdf,
    render_pdf_document, render_pdf_document_emitted,
};
use std::collections::BTreeMap;

struct PaintedPage {
    text: String,
    rules: usize,
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

fn painted_pages(pdf: &[u8]) -> Vec<PaintedPage> {
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
        page_text.push(PaintedPage {
            text: painted.chars().filter(|c| !c.is_whitespace()).collect(),
            rules: content
                .lines()
                .filter(|line| line.contains(" RG 0.7 w ") && line.ends_with(" l S"))
                .count(),
        });
    }
    page_text
}

fn small_options() -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.page.size.width_pt = 240.0;
    options.theme.page.size.height_pt = 250.0;
    options.theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    options
}

fn page_of(pages: &[PaintedPage], text: &str) -> usize {
    let matching: Vec<_> = pages
        .iter()
        .enumerate()
        .filter(|(_, page)| page.text.contains(text))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(matching.len(), 1, "{text} must occur on exactly one page");
    matching[0]
}

#[test]
fn default_short_sections_keep_the_title_rule_and_first_content_together() {
    for (options, count, max_pages) in [(PdfOptions::default(), 20, 3), (small_options(), 5, 2)] {
        let source = "# MAINHEADING\n\n".to_string()
            + &(0..count)
                .map(|index| format!("## SECTION{index:02}\n\nBODY{index:02}\n\n"))
                .collect::<String>();
        let pdf = render_pdf(&source, &options).unwrap();
        let pages = painted_pages(&pdf);
        assert!(
            pages.len() <= max_pages,
            "an orphan heading must not consume an extra page"
        );
        assert_eq!(page_of(&pages, "MAINHEADING"), page_of(&pages, "BODY00"));
        for index in 0..count {
            assert_eq!(
                page_of(&pages, &format!("SECTION{index:02}")),
                page_of(&pages, &format!("BODY{index:02}"))
            );
        }
        for page in &pages {
            let headings = page.text.matches("SECTION").count()
                + usize::from(page.text.contains("MAINHEADING"));
            assert_eq!(
                page.rules, headings,
                "a heading and its painted underline belong on the same page: {}",
                page.text
            );
        }
        let doc = parse_markdown(&source);
        assert_eq!(
            pdf,
            render_pdf_document_emitted(&doc, &options, PdfEmitOptions::default()).unwrap()
        );
        assert_eq!(
            pdf,
            render_pdf_document_emitted(&doc, &options, PdfEmitOptions::monolithic()).unwrap()
        );
    }
}

#[test]
fn mixed_heading_followers_and_page_notes_remain_with_their_headings() {
    let mut source = String::from("# MAINHEADING\n\n");
    let mut options = small_options();
    options.theme.page.size.height_pt = 380.0;
    options.image_assets.push(PdfImageAsset::new("tile.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="60" viewBox="0 0 80 60"><rect width="80" height="60" fill="#eee"/><text x="4" y="30" font-size="12">FIGURE</text></svg>"##.to_vec()));
    let mut followers = Vec::new();
    for index in 0..15 {
        source.push_str(&format!("## SECTION{index:02}\n\n"));
        let label = match index % 5 {
            0 => {
                source.push_str(&format!("BODY{index:02}[^n{index}]\n\n"));
                format!("BODY{index:02}")
            }
            1 => {
                source.push_str(&format!(
                    "| TABLE{index:02} | Value |\n|---|---|\n| ROW{index:02} | 42 |\n\n"
                ));
                format!("TABLE{index:02}")
            }
            2 => {
                source.push_str(&format!("- LIST{index:02}\n- TAIL{index:02}\n\n"));
                format!("LIST{index:02}")
            }
            3 => {
                source.push_str(&format!("> QUOTE{index:02}\n\n"));
                format!("QUOTE{index:02}")
            }
            _ => {
                source.push_str("![A figure](tile.svg)\n\n");
                // Repeated artwork is associated with its section below by
                // checking its first visible word after the section heading.
                String::from("FIGURE")
            }
        };
        followers.push(label);
    }
    for index in (0..15).step_by(5) {
        source.push_str(&format!(
            "[^n{index}]: NOTE{index:02} retained at the page foot.\n\n"
        ));
    }
    let pdf = render_pdf(&source, &options).unwrap();
    let pages = painted_pages(&pdf);
    assert_eq!(page_of(&pages, "MAINHEADING"), page_of(&pages, "BODY00"));
    for (index, follower) in followers.iter().enumerate() {
        let heading = format!("SECTION{index:02}");
        let page = &pages[page_of(&pages, &heading)];
        let after_heading = page.text.split_once(&heading).unwrap().1;
        let section = after_heading.split("SECTION").next().unwrap();
        assert!(
            section.contains(follower),
            "{heading} must retain its first content: {}",
            page.text
        );
        if index % 5 == 0 {
            assert_eq!(
                page_of(&pages, follower),
                page_of(&pages, &format!("NOTE{index:02}"))
            );
        }
    }
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/S /Figure").count(), 3);
    assert_eq!(raw.matches("/S /LI").count(), 6);
}

#[test]
fn a_wrapped_heading_that_fits_moves_as_a_whole_with_its_first_body_line() {
    let mut options = small_options();
    options.theme.page.size.height_pt = 300.0;
    let words: Vec<_> = (0..6).map(|index| format!("TITLE{index:02}")).collect();
    let source = (0..7)
        .map(|index| format!("PRE{index:02}\n\n"))
        .collect::<String>()
        + &format!("# {}\n\nBODYAFTER\n\nEND\n", words.join(" "));
    let pages = painted_pages(&render_pdf(&source, &options).unwrap());
    let heading_page = page_of(&pages, &words[0]);
    assert!(
        heading_page > 0,
        "the fixture must move the heading past preceding content"
    );
    for word in &words {
        assert_eq!(page_of(&pages, word), heading_page);
    }
    assert_eq!(page_of(&pages, "BODYAFTER"), heading_page);
    assert_eq!(pages[heading_page].rules, 1);
}

#[test]
fn explicit_breaks_win_and_oversized_headings_still_make_progress() {
    let mut doc = parse_markdown("# BEFORE\n\n# AFTER\n\nBODY\n");
    doc.blocks.insert(1, Block::PageBreak);
    let pages = painted_pages(&render_pdf_document(&doc, &PdfOptions::default()).unwrap());
    assert_eq!(pages.len(), 2);
    assert_eq!(page_of(&pages, "BEFORE"), 0);
    assert_eq!(page_of(&pages, "AFTER"), 1);
    assert_eq!(page_of(&pages, "BODY"), 1);
    assert_eq!(pages[0].rules, 1);
    assert_eq!(pages[1].rules, 1);

    let words: Vec<_> = (0..60).map(|index| format!("H{index:03}")).collect();
    let source = format!("# {}\n\nBODYAFTER\n", words.join(" "));
    let pages = painted_pages(&render_pdf(&source, &small_options()).unwrap());
    assert!(
        pages.len() > 2 && pages.len() < 20,
        "an oversized heading must paginate with bounded progress"
    );
    assert!(pages.iter().all(|page| !page.text.is_empty()));
    assert_eq!(
        pages
            .iter()
            .map(|page| page.text.as_str())
            .collect::<String>(),
        words.join("") + "BODYAFTER"
    );
    assert_eq!(pages.iter().map(|page| page.rules).sum::<usize>(), 1);
}
