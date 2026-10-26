//! Baseline-grid acceptance reads the PDF's own pages, text matrices and fonts.
//! No layout, pagination or verification-text-layer helper supplies the oracle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::{
    BaselineGrid, PageMargins, PdfEmitOptions, PdfImageAsset, PdfOptions, parse_markdown,
    render_pdf, render_pdf_document_emitted,
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
            .chunks_exact(4)
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
    content: String,
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
    kids.chunks_exact(3)
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
                .chunks_exact(4)
                .map(|entry| (entry[0], font_map(&objects, entry[1].parse().unwrap())))
                .collect();
            let content = objects.stream(reference_after(dictionary, "/Contents "));
            let mut runs = Vec::new();
            let mut artifacts = Vec::new();
            for line in content.lines() {
                if line.ends_with(" BDC") || line.ends_with(" BMC") {
                    artifacts.push(
                        line.starts_with("/Artifact") || artifacts.last().copied().unwrap_or(false),
                    );
                    continue;
                }
                if line == "EMC" {
                    artifacts.pop().expect("balanced marked content");
                    continue;
                }
                if !line.contains("BT /F") {
                    continue;
                }
                assert!(
                    !line.contains(" Td"),
                    "extend the decoder before accepting relative text motion"
                );
                // PDF permits adjacent hex strings and kerning numbers, e.g.
                // [<0001>42<0002>]. Separate lexical boundaries, not just spaces.
                let spaced = line
                    .replace('[', " [ ")
                    .replace(']', " ] ")
                    .replace('<', " <")
                    .replace('>', "> ");
                let words: Vec<_> = spaced.split_whitespace().collect();
                let mut font = None;
                let mut size = 0.0;
                let mut x = 0.0;
                let mut y = 0.0;
                let mut render_mode = 0;
                let mut text = String::new();
                for (index, word) in words.iter().enumerate() {
                    match *word {
                        "Tf" => {
                            font = Some(&fonts[words[index - 2]]);
                            size = words[index - 1].parse().unwrap();
                        }
                        "Tr" => render_mode = words[index - 1].parse::<u8>().unwrap(),
                        "Tm" => {
                            let matrix: Vec<f64> = words[index - 6..index]
                                .iter()
                                .map(|x| x.parse().unwrap())
                                .collect();
                            assert_eq!(
                                &matrix[..4],
                                &[1.0, 0.0, 0.0, 1.0],
                                "fixture text is untransformed"
                            );
                            x = matrix[4];
                            y = matrix[5];
                        }
                        "Tj" | "TJ" => {
                            if render_mode != 3 && !text.is_empty() {
                                let face = font.unwrap();
                                runs.push(PaintedRun {
                                    x,
                                    y,
                                    size,
                                    cap_height: face.cap_height,
                                    descent: face.descent,
                                    text: std::mem::take(&mut text),
                                    artifact: artifacts.last().copied().unwrap_or(false),
                                });
                            } else {
                                text.clear();
                            }
                        }
                        hex if hex.starts_with('<') && hex.ends_with('>') => {
                            let face = font.expect("glyph painting needs an active font");
                            let hex = hex.trim_matches(['<', '>']);
                            assert_eq!(hex.len() % 4, 0);
                            for glyph in hex.as_bytes().chunks_exact(4) {
                                let id =
                                    u16::from_str_radix(std::str::from_utf8(glyph).unwrap(), 16)
                                        .unwrap();
                                text.push_str(
                                    face.unicode
                                        .get(&id)
                                        .expect("painted glyph must have a ToUnicode mapping"),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            assert!(artifacts.is_empty(), "balanced marked-content stack");
            PaintedPage {
                width: media[2],
                height: media[3],
                runs,
                content,
            }
        })
        .collect()
}

fn is_body_baseline(run: &PaintedRun) -> bool {
    !run.artifact
        && run
            .text
            .chars()
            .any(|character| character.is_ascii_alphabetic())
}

fn check_grid(pages: &[PaintedPage], opts: &PdfOptions, pitch: f64) -> Result<(), String> {
    let margins = opts.theme.page.margins;
    for (page_index, page) in pages.iter().enumerate() {
        let origin = page.height - f64::from(margins.top_pt);
        for run in page.runs.iter().filter(|run| is_body_baseline(run)) {
            let steps = (origin - run.y) / pitch;
            let distance = ((steps - steps.round()) * pitch).abs();
            if distance > PRECISION {
                return Err(format!(
                    "page {} {:?}: y={} is {}pt off grid",
                    page_index + 1,
                    run.text,
                    run.y,
                    distance
                ));
            }
            let top = run.y + run.cap_height * run.size;
            let bottom = run.y + run.descent * run.size;
            if top > origin + 0.025 || bottom < f64::from(margins.bottom_pt) - 0.025 {
                return Err(format!(
                    "page {} {:?}: font ink bounds {bottom}..{top} leave body {}..{origin}",
                    page_index + 1,
                    run.text,
                    margins.bottom_pt
                ));
            }
            if run.x < f64::from(margins.left_pt) - 0.025
                || run.x > page.width - f64::from(margins.right_pt) + 0.025
            {
                return Err(format!(
                    "page {} {:?}: x={} leaves horizontal body bounds",
                    page_index + 1,
                    run.text,
                    run.x
                ));
            }
        }
    }
    Ok(())
}

fn check_separate_rows(pages: &[PaintedPage]) -> Result<(), String> {
    for (page_index, page) in pages.iter().enumerate() {
        let mut rows: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
        for run in page.runs.iter().filter(|run| is_body_baseline(run)) {
            let key = (run.y * 100.0).round() as i64;
            let row = rows
                .entry(key)
                .or_insert((f64::INFINITY, f64::NEG_INFINITY));
            row.0 = row.0.min(run.y + run.descent * run.size);
            row.1 = row.1.max(run.y + run.cap_height * run.size);
        }
        let rows: Vec<_> = rows.values().collect();
        for pair in rows.windows(2) {
            if pair[0].1 > pair[1].0 + 0.025 {
                return Err(format!(
                    "page {} distinct rows overlap: {:?}, {:?}",
                    page_index + 1,
                    pair[0],
                    pair[1]
                ));
            }
        }
    }
    Ok(())
}

fn body_text(pages: &[PaintedPage]) -> String {
    pages
        .iter()
        .flat_map(|page| &page.runs)
        .filter(|run| !run.artifact)
        .flat_map(|run| run.text.chars())
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn options(pitch: f32) -> PdfOptions {
    let mut options = PdfOptions {
        page_numbers: false,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(f64::from(pitch)).unwrap());
    options.theme.page.size.width_pt = 360.0;
    options.theme.page.size.height_pt = 300.0;
    options.theme.page.margins = PageMargins {
        top_pt: 37.25,
        right_pt: 28.5,
        bottom_pt: 29.75,
        left_pt: 41.125,
    };
    options
}

#[test]
fn disabled_grid_preserves_preexisting_pdf_bytes() {
    // Recorded before baseline-grid implementation, from the exact-source PDF
    // acceptance fixture at a341e9c. This test cannot regenerate its expectation.
    let mut opts = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = 400.0;
    opts.theme.page.size.height_pt = 172.0;
    opts.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    assert!(opts.theme.page.baseline_grid.is_none());
    let bytes = render_pdf("A  \nB  \nC", &opts).unwrap();
    let fingerprint = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    });
    assert_eq!((bytes.len(), fingerprint), (3296, 0xfb6e_259f_6c5b_594c));
}

#[test]
fn fractional_grid_phase_and_capacity_hold_across_pages_and_orientations() {
    let letters: Vec<String> = (0..80)
        .map(|index| char::from(b'A' + (index % 26) as u8).to_string())
        .collect();
    let source = letters.join("  \n");
    for (width, height, pitch, size) in [
        (360.0, 300.0, 13.333, 11.25),
        (520.0, 220.0, 16.125, 12.375),
    ] {
        let mut opts = options(pitch);
        opts.theme.page.size.width_pt = width;
        opts.theme.page.size.height_pt = height;
        opts.base_font_size = Some(size);
        opts.optimal_pagination = width > height;
        let pages = painted_pages(&render_pdf(&source, &opts).unwrap());
        assert!(pages.len() >= 3, "fixture must exercise page transitions");
        check_grid(&pages, &opts, f64::from(pitch)).unwrap();
        check_separate_rows(&pages).unwrap();
        assert_eq!(body_text(&pages), letters.concat());
        for page in &pages {
            let rows: Vec<_> = page
                .runs
                .iter()
                .filter(|run| is_body_baseline(run))
                .collect();
            assert!(
                rows.windows(2)
                    .all(|pair| pair[0].y > pair[1].y + PRECISION),
                "every source letter owns a distinct descending line"
            );
            assert!(
                rows.iter()
                    .all(|run| (run.size - f64::from(size)).abs() <= PRECISION),
                "grid must not shrink fonts"
            );
            let body_height = f64::from(
                height - opts.theme.page.margins.top_pt - opts.theme.page.margins.bottom_pt,
            );
            assert!(
                rows.len() <= (body_height / f64::from(pitch)).floor() as usize + 1,
                "page cannot contain more grid positions than its physical body"
            );
        }
    }
}

const MIXED: &str = "# HEADING\n\nALPHA BODY TEXT WITH MULTIPLE WORDS THAT WRAP INTO MORE THAN ONE LINE ON A NARROW PAGE.\n\n> QUOTE\n>\n> - LISTROOT\n>   - LISTCHILD\n\n```text\nCODEONE\nCODETWO\n```\n\n| HEADKEY | HEADVALUE |\n|---|---|\n| ROWONE | VALUEONE |\n| ROWTWO | VALUETWO |\n\nOMEGA\n";

#[test]
fn mixed_content_uses_one_phase_without_changing_font_sizes_or_content_order() {
    let opts = options(16.0);
    let bytes = render_pdf(MIXED, &opts).unwrap();
    let pages = painted_pages(&bytes);
    check_grid(&pages, &opts, 16.0).unwrap();
    check_separate_rows(&pages).unwrap();
    let painted = body_text(&pages);
    let mut previous = 0;
    for token in [
        "HEADING",
        "ALPHA",
        "QUOTE",
        "LISTROOT",
        "LISTCHILD",
        "CODEONE",
        "CODETWO",
        "HEADKEY",
        "HEADVALUE",
        "ROWONE",
        "VALUEONE",
        "ROWTWO",
        "VALUETWO",
        "OMEGA",
    ] {
        let at = painted
            .find(token)
            .unwrap_or_else(|| panic!("missing painted {token}: {painted}"));
        assert!(at >= previous, "painted source order changed at {token}");
        previous = at;
    }
    let mut disabled = opts.clone();
    disabled.theme.page.baseline_grid = None;
    let original = painted_pages(&render_pdf(MIXED, &disabled).unwrap());
    let sizes = |pages: &[PaintedPage]| -> BTreeSet<i64> {
        pages
            .iter()
            .flat_map(|page| &page.runs)
            .filter(|run| is_body_baseline(run))
            .map(|run| (run.size * 100.0).round() as i64)
            .collect()
    };
    assert_eq!(sizes(&pages), sizes(&original));
    assert!(
        check_grid(&original, &disabled, 16.0).is_err(),
        "negative control: ordinary layout is not already on the requested grid"
    );
}

#[test]
fn table_headers_and_page_notes_keep_the_page_grid() {
    let mut source = String::from("| HEADER | VALUE |\n|---|---|\n");
    for index in 0..36 {
        source.push_str(&format!("| ROW{index:02} | CELL{index:02} |\n"));
    }
    source.push_str("\nCLAIM[^n]\n\n[^n]: NOTEBODY\n");
    let opts = options(16.0);
    let pages = painted_pages(&render_pdf(&source, &opts).unwrap());
    assert!(pages.len() >= 3);
    // Repeated table headers are accessibility artifacts, but are painted
    // within the body and must still meet its grid and ink-clearance contract.
    // This fixture has no running bands; admit only its two known header cells
    // to the coordinate checks, while retaining logical content extraction.
    let mut visible_rows = pages.clone();
    for page in &mut visible_rows {
        for run in &mut page.runs {
            if matches!(run.text.as_str(), "HEADER" | "VALUE") {
                run.artifact = false;
            }
        }
    }
    check_grid(&visible_rows, &opts, 16.0).unwrap();
    check_separate_rows(&visible_rows).unwrap();
    let painted = body_text(&pages);
    let header_copies = pages
        .iter()
        .flat_map(|page| &page.runs)
        .filter(|run| run.text == "HEADER")
        .count();
    assert!(header_copies >= 3, "table headers actually repeat");
    assert_eq!(
        painted.matches("HEADER").count(),
        1,
        "only the source header belongs in the logical reading order"
    );
    let mut previous = 0;
    for index in 0..36 {
        let token = format!("ROW{index:02}");
        let at = painted.find(&token).unwrap();
        assert!(at >= previous);
        previous = at;
    }
    let locate = |needle: &str| {
        pages
            .iter()
            .enumerate()
            .find_map(|(index, page)| {
                page.runs
                    .iter()
                    .find(|run| run.text.contains(needle))
                    .map(|run| (index, run))
            })
            .unwrap()
    };
    let (claim_page, claim) = locate("CLAIM");
    let (note_page, note) = locate("NOTEBODY");
    assert_eq!(
        claim_page, note_page,
        "the grid must preserve the note's citing page"
    );
    assert!(note.y + note.cap_height * note.size < claim.y + claim.descent * claim.size);
    assert!(note.size < claim.size);
}

#[test]
fn figures_resume_the_body_grid_and_running_bands_keep_their_own_positions() {
    let mut opts = options(16.0);
    opts.theme.page.margins.top_pt = 44.0;
    opts.theme.page.margins.bottom_pt = 44.0;
    opts.running.header.left = Some("HEADER".into());
    opts.running.footer.center = Some("FOOTER {page}".into());
    opts.image_assets.push(PdfImageAsset::new("figure.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="43"><rect width="120" height="43" fill="#aa3344"/></svg>"##.to_vec()));
    let source = "BEFORE\n\n![FIGURE](figure.svg)\n\nAFTER\n";
    let bytes = render_pdf(source, &opts).unwrap();
    let pages = painted_pages(&bytes);
    check_grid(&pages, &opts, 16.0).unwrap();
    check_separate_rows(&pages).unwrap();
    assert_eq!(body_text(&pages), "BEFOREAFTER");
    assert!(
        pages.iter().any(|page| page.content.contains("/Figure")),
        "fixture emits a real figure region"
    );
    let body: Vec<_> = pages
        .iter()
        .flat_map(|page| &page.runs)
        .filter(|run| is_body_baseline(run))
        .collect();
    assert!(
        body[0].y - body[1].y > 30.0,
        "figure space must not collapse"
    );
    let mut disabled = opts.clone();
    disabled.theme.page.baseline_grid = None;
    let original = painted_pages(&render_pdf(source, &disabled).unwrap());
    let chrome = |page: &PaintedPage| {
        page.runs
            .iter()
            .filter(|run| run.artifact)
            .map(|run| (run.text.clone(), run.x, run.y, run.size))
            .collect::<Vec<_>>()
    };
    assert!(!chrome(&pages[0]).is_empty());
    assert_eq!(chrome(&pages[0]), chrome(&original[0]));
}

#[test]
fn tiny_list_figures_reserve_visible_marker_ink_at_the_lower_body_edge() {
    for prefix in ["- ", "3. "] {
        for following_prose in [false, true] {
            let mut opts = options(16.0);
            opts.theme.page.size.height_pt = 320.0;
            opts.theme.page.margins.top_pt = 40.0;
            opts.theme.page.margins.bottom_pt = 40.0;
            opts.image_assets.push(PdfImageAsset::new(
                "tiny.svg",
                br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="#aa3344"/></svg>"##.to_vec(),
            ));
            let mut source = format!("{prefix}![TINY](tiny.svg)\n").repeat(16);
            if following_prose {
                source.push_str("\nAFTER\n");
            }
            let pages = painted_pages(&render_pdf(&source, &opts).unwrap());
            assert!(pages.len() >= 2, "markers must exercise page breaks");
            check_grid(&pages, &opts, 16.0).unwrap();
            let mut markers = 0;
            let mut images = 0;
            let mut lowest_marker = f64::INFINITY;
            for page in &pages {
                let origin = page.height - 40.0;
                let mut previous_bottom = f64::INFINITY;
                // In this fixture every visible glyph run is either a bullet,
                // an ordered marker, or AFTER. Do not drop nonalphabetic
                // markers just because the ordinary body oracle skips note
                // superscripts and other non-baseline annotations.
                for run in page.runs.iter().filter(|run| !run.artifact) {
                    let distance =
                        (((origin - run.y) / 16.0).round() * 16.0 - (origin - run.y)).abs();
                    assert!(distance <= PRECISION, "off-grid marker: {run:?}");
                    let top = run.y + run.cap_height * run.size;
                    let bottom = run.y + run.descent * run.size;
                    assert!(top <= origin + 0.025, "marker leaves top: {run:?}");
                    assert!(bottom >= 40.0 - 0.025, "marker leaves bottom: {run:?}");
                    assert!(
                        top <= previous_bottom + 0.025,
                        "marker or following prose overlaps the preceding row: {run:?}"
                    );
                    previous_bottom = bottom;
                    assert!((run.size - 11.0).abs() < PRECISION);
                    if run.text != "AFTER" {
                        markers += 1;
                        lowest_marker = lowest_marker.min(run.y);
                    }
                }
                // The supplied SVG is a unit square. Its emitted matrix,
                // rather than a layout estimate or structure-tree BBox, must
                // retain the natural 0.75pt dimensions at 96dpi.
                for line in page
                    .content
                    .lines()
                    .filter(|line| line.starts_with("q ") && line.ends_with(" cm"))
                {
                    let values: Vec<f64> = line
                        .split_whitespace()
                        .skip(1)
                        .take(6)
                        .map(|number| number.parse().unwrap())
                        .collect();
                    assert_eq!(&values[..4], &[0.75, 0.0, 0.0, -0.75]);
                    assert!(values[5] <= origin + 0.025);
                    assert!(values[5] - 0.75 >= 40.0 - 0.025);
                    images += 1;
                }
            }
            assert_eq!(markers, 16, "every list marker is visibly painted");
            assert_eq!(images, 16, "every unit-square figure is visibly painted");
            assert!(lowest_marker <= 88.0, "fixture reaches the lower body edge");
            if following_prose {
                assert_eq!(pages.last().unwrap().runs.last().unwrap().text, "AFTER");
            }
        }
    }
}

#[test]
fn long_fractional_page_note_keeps_the_original_page_phase() {
    let mut opts = options(13.337);
    opts.theme.page.size.height_pt = 14_400.0;
    // 400 note lines fit within the allowed page-note share of this legal
    // tall page. Summing their pitch as f32 drifts by almost 0.02pt; an exact
    // grid must preserve the page phase rather than inheriting that sum.
    let mut source = String::from("CLAIM[^n]\n\n[^n]: NOTE000  \n");
    for index in 1..400 {
        source.push_str(&format!("    NOTE{index:03}"));
        source.push_str(if index == 399 { "\n" } else { "  \n" });
    }
    let pages = painted_pages(&render_pdf(&source, &opts).unwrap());
    assert_eq!(pages.len(), 1, "the note fits its citing page");
    check_grid(&pages, &opts, 13.337).unwrap();
    check_separate_rows(&pages).unwrap();
    let painted = body_text(&pages);
    let mut previous = painted.find("CLAIM").unwrap();
    for index in 0..400 {
        let token = format!("NOTE{index:03}");
        let at = painted.find(&token).unwrap();
        assert!(
            at > previous,
            "note content must be painted in source order"
        );
        previous = at;
    }
    let note = pages[0]
        .runs
        .iter()
        .find(|run| run.text.contains("NOTE000"))
        .unwrap();
    assert!(note.size < 11.0, "the fixture must use a page-foot note");
}

#[test]
fn chunked_and_monolithic_grid_outputs_are_identical() {
    let source = MIXED.repeat(5);
    let opts = options(16.0);
    let document = parse_markdown(&source);
    let chunked = render_pdf_document_emitted(&document, &opts, PdfEmitOptions::default()).unwrap();
    let monolithic =
        render_pdf_document_emitted(&document, &opts, PdfEmitOptions::monolithic()).unwrap();
    assert_eq!(chunked, monolithic);
    assert_eq!(chunked, render_pdf(&source, &opts).unwrap());
    check_grid(&painted_pages(&chunked), &opts, 16.0).unwrap();
}

#[test]
fn invalid_grids_and_incompatible_layout_constraints_fail_explicitly() {
    for pitch in [0.0, -1.0, 0.5, 145.0, f64::NAN, f64::INFINITY] {
        assert!(
            BaselineGrid::from_points(pitch).is_err(),
            "invalid pitch {pitch}"
        );
    }
    let mut opts = options(16.0);
    opts.fit_to_pages = Some(1);
    let error = render_pdf("A", &opts).unwrap_err().to_string();
    assert!(error.contains("pdf_baseline_grid_incompatible:"), "{error}");
    opts.fit_to_pages = None;
    opts.theme.page.size.height_pt = 80.0;
    opts.theme.page.margins.top_pt = 36.0;
    opts.theme.page.margins.bottom_pt = 36.0;
    let error = render_pdf("A", &opts).unwrap_err().to_string();
    assert!(
        error.contains("baseline_grid"),
        "grid overflow must be explicit: {error}"
    );
}

#[test]
fn emitted_coordinate_checks_reject_off_grid_and_overlapping_negative_controls() {
    let opts = options(16.0);
    let pages = painted_pages(&render_pdf("A  \nB  \nC", &opts).unwrap());
    check_grid(&pages, &opts, 16.0).unwrap();
    check_separate_rows(&pages).unwrap();
    let mut shifted = pages.clone();
    shifted[0].runs[0].y += 3.0;
    assert!(check_grid(&shifted, &opts, 16.0).is_err());
    let mut overlap = pages;
    overlap[0].runs[1].y = overlap[0].runs[0].y - 0.5;
    assert!(
        check_separate_rows(&overlap).is_err(),
        "independent font-bound check must detect colliding rows"
    );
}
