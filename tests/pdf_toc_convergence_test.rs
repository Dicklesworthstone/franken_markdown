//! TOC convergence regressions decode emitted page labels, bookmarks, and links.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::Font;
use franken_markdown::verify::verify_pdf_result;
use franken_markdown::{
    BaselineGrid, Block, PdfEmitOptions, PdfOptions, parse_markdown, render_pdf_document,
    render_pdf_document_emitted, render_pdf_document_reported,
};
use std::collections::BTreeMap;

// Independently inspect the emitted objects through their cross-reference offsets.
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
            .rposition(|p| p == marker)
            .unwrap();
        let xref: usize = std::str::from_utf8(&bytes[at + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let header: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(header[0], "0");
        let count: usize = header[1].parse().unwrap();
        let mut offsets = vec![0];
        assert_eq!(lines.next().unwrap().trim(), "0000000000 65535 f");
        for id in 1..count {
            let row: Vec<_> = lines.next().unwrap().split_whitespace().collect();
            assert_eq!(&row[1..], &["00000", "n"]);
            let offset: usize = row[0].parse().unwrap();
            assert!(offset < xref);
            assert!(bytes[offset..].starts_with(format!("{id} 0 obj\n").as_bytes()));
            offsets.push(offset);
        }
        assert_eq!(lines.next(), Some("trailer"));
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
        &self.bytes[start + header.len()..end]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let body = self.body(id);
        let end = body
            .windows(8)
            .position(|p| p == b"\nstream\n")
            .unwrap_or(body.len());
        std::str::from_utf8(&body[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let body = self.body(id);
        let at = body.windows(8).position(|p| p == b"\nstream\n").unwrap();
        let dict = self.dictionary(id);
        let length = integer(dict, "/Length ").unwrap();
        let payload = &body[at + 8..at + 8 + length];
        let bytes = if dict.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
        } else {
            assert!(!dict.contains("/Filter"));
            payload.to_vec()
        };
        String::from_utf8(bytes).unwrap()
    }

    fn pages(&self) -> Vec<usize> {
        let catalog = self.dictionary(1);
        let pages = self.dictionary(integer(catalog, "/Pages ").unwrap());
        references(pages, "/Kids [")
    }
}

// The fixtures use simple ASCII dictionary values. Object boundaries always
// come from xref, never a search inside compressed or embedded font bytes.
fn integer(dictionary: &str, key: &str) -> Option<usize> {
    dictionary
        .split_once(key)?
        .1
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn references(dictionary: &str, key: &str) -> Vec<usize> {
    let Some((_, tail)) = dictionary.split_once(key) else {
        return Vec::new();
    };
    let fields: Vec<_> = tail.split_once(']').unwrap().0.split_whitespace().collect();
    assert_eq!(fields.len() % 3, 0);
    fields
        .as_chunks::<3>()
        .0
        .iter()
        .map(|fields| {
            assert_eq!(&fields[1..], &["0", "R"]);
            fields[0].parse().unwrap()
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Destination {
    page: usize,
    y: f64,
}

fn destination(dictionary: &str) -> Option<Destination> {
    let fields: Vec<_> = dictionary
        .split_once("/Dest [")?
        .1
        .split_once(']')?
        .0
        .split_whitespace()
        .collect();
    assert_eq!(&fields[1..5], &["0", "R", "/XYZ", "null"]);
    assert_eq!(fields[6], "null");
    Some(Destination {
        page: fields[0].parse().unwrap(),
        y: fields[5].parse().unwrap(),
    })
}

fn unicode_map(objects: &Objects<'_>, font: usize) -> BTreeMap<u16, String> {
    let cmap = objects.stream(integer(objects.dictionary(font), "/ToUnicode ").unwrap());
    let mut active = false;
    let mut map = BTreeMap::new();
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            active = true;
            continue;
        }
        if line == "endbfchar" {
            active = false;
            continue;
        }
        if !active {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let glyph = u16::from_str_radix(fields[0].trim_matches(['<', '>']), 16).unwrap();
        let hex = fields[1].trim_matches(['<', '>']);
        let units: Vec<_> = hex
            .as_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
            .collect();
        map.insert(glyph, String::from_utf16(&units).unwrap());
    }
    assert!(!map.is_empty());
    map
}

struct PaintedRun {
    page: usize,
    size: f64,
    x: f64,
    y: f64,
    text: String,
}

fn painted_runs(objects: &Objects<'_>) -> Vec<PaintedRun> {
    let mut runs = Vec::new();
    for page in objects.pages() {
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
        assert_eq!(resources.len() % 4, 0);
        let fonts: BTreeMap<_, _> = resources
            .as_chunks::<4>()
            .0
            .iter()
            .map(|entry| (entry[0], unicode_map(objects, entry[1].parse().unwrap())))
            .collect();
        let content = objects.stream(integer(dictionary, "/Contents ").unwrap());
        for line in content.lines().filter(|line| line.contains("BT /F")) {
            let spaced = line
                .replace('[', " [ ")
                .replace(']', " ] ")
                .replace('<', " <")
                .replace('>', "> ");
            let words: Vec<_> = spaced.split_whitespace().collect();
            let mut font = None;
            let mut size = 0.0;
            let (mut x, mut y) = (0.0, 0.0);
            let mut mode = 0;
            let mut text = String::new();
            for (index, word) in words.iter().enumerate() {
                match *word {
                    "Tf" => {
                        font = Some(&fonts[words[index - 2]]);
                        size = words[index - 1].parse().unwrap();
                    }
                    "Tr" => mode = words[index - 1].parse::<u8>().unwrap(),
                    "Tm" => {
                        x = words[index - 2].parse().unwrap();
                        y = words[index - 1].parse().unwrap();
                    }
                    "Tj" | "TJ" => {
                        if mode != 3 && !text.is_empty() {
                            runs.push(PaintedRun {
                                page,
                                size,
                                x,
                                y,
                                text: std::mem::take(&mut text),
                            });
                        } else {
                            text.clear();
                        }
                    }
                    hex if hex.starts_with('<') && hex.ends_with('>') => {
                        let hex = hex.trim_matches(['<', '>']);
                        assert_eq!(hex.len() % 4, 0);
                        for glyph in hex.as_bytes().as_chunks::<4>().0 {
                            let id = u16::from_str_radix(std::str::from_utf8(glyph).unwrap(), 16)
                                .unwrap();
                            text.push_str(&font.unwrap()[&id]);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    runs
}

fn link_at_run(objects: &Objects<'_>, run: &PaintedRun) -> Destination {
    references(objects.dictionary(run.page), "/Annots [")
        .into_iter()
        .find_map(|id| {
            let dictionary = objects.dictionary(id);
            let rectangle: Vec<f64> = dictionary
                .split_once("/Rect [")?
                .1
                .split_once(']')?
                .0
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            (run.x >= rectangle[0] - 0.02
                && run.x <= rectangle[2] + 0.02
                && run.y >= rectangle[1]
                && run.y <= rectangle[3])
                .then(|| destination(dictionary))
                .flatten()
        })
        .unwrap_or_else(|| panic!("no resolved annotation for {}", run.text))
}

/// A valid host font with proportional page numerals. Preserve the admitted
/// outlines and every unrelated metric; repair both table and font checksums.
fn proportional_digits_font() -> Vec<u8> {
    let mut bytes =
        include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf").to_vec();
    let font = Font::parse(bytes.clone()).unwrap();
    let digit = usize::from(font.glyph_index('1'));
    let tables = usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap()));
    let table = |tag: &[u8; 4]| {
        (0..tables)
            .map(|index| 12 + index * 16)
            .find(|&at| &bytes[at..at + 4] == tag)
            .unwrap()
    };
    let hmtx = table(b"hmtx");
    let head = table(b"head");
    let hmtx_start = u32::from_be_bytes(bytes[hmtx + 8..hmtx + 12].try_into().unwrap()) as usize;
    let head_start = u32::from_be_bytes(bytes[head + 8..head + 12].try_into().unwrap()) as usize;
    bytes[hmtx_start + digit * 4..hmtx_start + digit * 4 + 2]
        .copy_from_slice(&900u16.to_be_bytes());
    bytes[head_start + 8..head_start + 12].fill(0);
    let checksum = |data: &[u8]| {
        data.chunks(4).fold(0u32, |sum, part| {
            let mut word = [0; 4];
            word[..part.len()].copy_from_slice(part);
            sum.wrapping_add(u32::from_be_bytes(word))
        })
    };
    for index in 0..tables {
        let at = 12 + index * 16;
        let start = u32::from_be_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
        let len = u32::from_be_bytes(bytes[at + 12..at + 16].try_into().unwrap()) as usize;
        let sum = checksum(&bytes[start..start + len]);
        bytes[at + 4..at + 8].copy_from_slice(&sum.to_be_bytes());
    }
    let adjustment = 0xb1b0_afbau32.wrapping_sub(checksum(&bytes));
    bytes[head_start + 8..head_start + 12].copy_from_slice(&adjustment.to_be_bytes());
    bytes
}

const TITLE: &str = "A complete reader guide";

fn proportional_options() -> PdfOptions {
    let mut options = PdfOptions::default();
    let font_bytes = proportional_digits_font();
    let font = Font::parse(font_bytes.clone()).unwrap();
    options
        .font_assets
        .set_slot(franken_markdown::FontAssetSlot::BodyRegular, font_bytes)
        .unwrap();
    options.theme.page.margins = franken_markdown::theme::PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    let kern = font.gpos_kerning();
    let width = |text: &str| {
        let glyphs: Vec<_> = text.chars().map(|ch| font.glyph_index(ch)).collect();
        let advances: i32 = glyphs
            .iter()
            .map(|&gid| i32::from(font.advance_width(gid)))
            .sum();
        let kerning: i32 = glyphs
            .windows(2)
            .map(|pair| i32::from(kern.pair(pair[0], pair[1])))
            .sum();
        (advances + kerning) as f32 * 11.0 / f32::from(font.units_per_em)
    };
    // A narrow digit permits a one-line title; the wider 1 forces another
    // line. The 190pt page height makes the old five-pass planner oscillate.
    options.theme.page.size.width_pt = 48.0 + width(TITLE) + width("2") + width(" ") * 2.0 + 1.5;
    options.theme.page.size.height_pt = 190.0;
    options
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

/// All fixture headings are H1s and contain no literal numeric text. Compare
/// their actual painted text and pages with the outline, then decode each
/// printed page label and check its clickable destination through the page tree.
fn assert_toc_labels(pdf: &[u8], titles: &[&str], copies: usize) -> Vec<usize> {
    let objects = Objects::new(pdf);
    let pages = objects.pages();
    let runs = painted_runs(&objects);
    let outline = integer(objects.dictionary(1), "/Outlines ").unwrap();
    let mut current = integer(objects.dictionary(outline), "/First ");
    let mut headings = Vec::new();
    while let Some(id) = current {
        let dictionary = objects.dictionary(id);
        let title = dictionary
            .split_once("/Title (")
            .unwrap()
            .1
            .split_once(')')
            .unwrap()
            .0;
        headings.push((title, destination(dictionary).unwrap()));
        current = integer(dictionary, "/Next ");
    }
    assert_eq!(
        headings.iter().map(|(title, _)| *title).collect::<Vec<_>>(),
        titles
    );
    let mut painted_headings = runs.iter().filter(|run| run.size > 11.01);
    for (title, dest) in &headings {
        let first = painted_headings.next().expect("heading text is retained");
        assert_eq!(
            first.page, dest.page,
            "bookmark must reach painted heading {title}"
        );
        let mut text = compact(&first.text);
        let expected = compact(title);
        while text.len() < expected.len() {
            text.push_str(&compact(&painted_headings.next().unwrap().text));
        }
        assert_eq!(text, expected, "heading source text is retained exactly");
    }
    assert!(
        painted_headings.next().is_none(),
        "no duplicated heading text"
    );

    let labels: Vec<_> = runs
        .iter()
        .filter(|run| run.text.parse::<usize>().is_ok())
        .collect();
    assert_eq!(
        labels.len(),
        titles.len() * copies,
        "one page label per TOC entry"
    );
    let mut found = vec![0; titles.len()];
    let mut printed_pages = Vec::new();
    for label in labels {
        let linked = link_at_run(&objects, label);
        let heading = headings
            .iter()
            .position(|(_, dest)| *dest == linked)
            .unwrap();
        found[heading] += 1;
        let page = pages.iter().position(|&page| page == linked.page).unwrap() + 1;
        assert_eq!(
            label.text,
            page.to_string(),
            "printed TOC page for {}",
            titles[heading]
        );
        printed_pages.push(page);
    }
    assert_eq!(
        found,
        vec![copies; titles.len()],
        "each heading has every requested TOC entry"
    );
    printed_pages
}

#[test]
fn proportional_page_labels_do_not_publish_stale_destinations() {
    let document = parse_markdown(&format!("[[TOC]]\n\n# {TITLE}\n\nBody.\n"));
    let mut options = proportional_options();
    for height in (100..=260).step_by(2) {
        options.theme.page.size.height_pt = height as f32;
        let pdf = render_pdf_document(&document, &options).unwrap();
        assert_toc_labels(&pdf, &[TITLE], 1);
        if height == 190
            && let Ok(directory) = std::env::var("FMD_TOC_PROBE_DIR")
        {
            std::fs::write(std::path::Path::new(&directory).join("converged.pdf"), &pdf).unwrap();
        }
    }
}

#[test]
fn strict_grid_and_verification_share_converged_toc_pages() {
    let document = parse_markdown(&format!("[[TOC]]\n\n# {TITLE}\n\nBody.\n"));
    for grid in [false, true] {
        let mut options = proportional_options();
        options.strict_pagination = true;
        if grid {
            options.theme.page.size.height_pt = 420.0;
            options.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
        }
        let rendered =
            render_pdf_document_reported(&document, &options, PdfEmitOptions::default()).unwrap();
        assert!(rendered.pagination.page_breaks.is_exact());
        let printed_pages = assert_toc_labels(&rendered.bytes, &[TITLE], 1);
        let verified = verify_pdf_result(&document, &options).unwrap();
        assert_eq!(
            verified.page_count,
            Objects::new(&rendered.bytes).pages().len()
        );
        let verified_toc = verified
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .filter(|run| run.kind == "paragraph")
            .find(|run| run.text.ends_with(&printed_pages[0].to_string()))
            .expect("verification retains the same TOC label");
        assert!(!verified_toc.text.is_empty());
        assert_eq!(
            rendered.bytes,
            render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap()
        );
    }
}

#[test]
fn strict_toc_refusal_is_preserved_by_verification() {
    let document = parse_markdown(&format!("[[TOC]]\n\n# {TITLE}\n\nBody.\n"));
    let mut options = proportional_options();
    options.strict_pagination = true;
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
    let rendered = render_pdf_document(&document, &options)
        .err()
        .expect("the grid-snapped heading cannot fit this page body");
    let verified = verify_pdf_result(&document, &options).unwrap_err();
    assert_eq!(rendered.code(), "pagination_infeasible");
    assert_eq!(verified.to_string(), rendered.to_string());
}

#[test]
fn repeated_tocs_match_all_headings_across_page_digit_boundaries() {
    let mut document = parse_markdown("[[TOC]]\n\n> [[TOC]]\n");
    let titles: Vec<_> = ('A'..='L')
        .map(|letter| format!("{TITLE} {letter}"))
        .collect();
    for title in &titles {
        document.blocks.push(Block::PageBreak);
        document
            .blocks
            .extend(parse_markdown(&format!("# {title}\n\nBody.\n")).blocks);
    }
    let mut options = proportional_options();
    options.theme.page.size.height_pt = 260.0;
    let titles: Vec<_> = titles.iter().map(String::as_str).collect();
    for strict in [false, true] {
        options.strict_pagination = strict;
        let pdf = render_pdf_document(&document, &options).unwrap();
        let printed_pages = assert_toc_labels(&pdf, &titles, 2);
        assert!(
            printed_pages.contains(&9),
            "fixture crosses the one-digit boundary"
        );
        assert!(
            printed_pages.contains(&10),
            "fixture crosses the two-digit boundary"
        );
        assert_eq!(pdf, render_pdf_document(&document, &options).unwrap());
    }
}
