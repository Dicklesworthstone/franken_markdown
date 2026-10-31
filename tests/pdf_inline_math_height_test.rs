//! Tall inline formulas retain every outline and their neighboring Unicode
//! text inside the physical page, including repeated headers and exact layout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    BaselineGrid, PageMargins, PageSize, PdfEmitOptions, PdfOptions, PdfPageEmission,
    RenderWarning, parse_markdown, render_pdf, render_pdf_emitted, render_warnings,
};

use std::collections::BTreeMap;

type Rect = [f64; 4];

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
        let xref = number(std::str::from_utf8(&bytes[at..]).unwrap(), "startxref\n");
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let section: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(section[0], "0");
        let count: usize = section[1].parse().unwrap();
        let offsets = lines
            .take(count)
            .map(|line| line.split_whitespace().next().unwrap().parse().unwrap())
            .collect();
        Self {
            bytes,
            offsets,
            xref,
        }
    }

    fn object(&self, id: usize) -> &'a [u8] {
        let start = self.offsets[id];
        let end = self
            .offsets
            .iter()
            .copied()
            .filter(|&at| at > start)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        assert!(self.bytes[start..end].starts_with(header.as_bytes()));
        &self.bytes[start + header.len()..end]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let body = self.object(id);
        let end = body
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap_or(body.len());
        std::str::from_utf8(&body[..end]).unwrap()
    }

    fn pages(&self) -> Vec<usize> {
        let trailer = std::str::from_utf8(&self.bytes[self.xref..]).unwrap();
        let catalog = self.dictionary(number(trailer, "/Root "));
        references(self.dictionary(number(catalog, "/Pages ")), "/Kids [")
    }

    fn stream(&self, id: usize) -> String {
        let body = self.object(id);
        let start = body
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap()
            + 8;
        let dictionary = self.dictionary(id);
        let payload = &body[start..start + number(dictionary, "/Length ")];
        let decoded = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
        } else {
            assert!(!dictionary.contains("/Filter"));
            payload.to_vec()
        };
        String::from_utf8(decoded).unwrap()
    }

    fn formula_bounds(&self) -> Vec<Rect> {
        self.pages()
            .into_iter()
            .flat_map(|page| {
                let stream = self.stream(number(self.dictionary(page), "/Contents "));
                stream
                    .split("/Formula <</MCID ")
                    .skip(1)
                    .map(|part| {
                        // The invisible ActualText anchor is not visible formula ink.
                        let outlines = part.split_once("/ActualText ").unwrap().0;
                        path_bounds(outlines)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn formula_boxes(&self) -> Vec<Rect> {
        self.formula_elements()
            .into_iter()
            .map(|dictionary| rectangle(dictionary, "/BBox ["))
            .collect()
    }

    fn formula_elements(&self) -> Vec<&'a str> {
        (1..self.offsets.len())
            .map(|id| self.dictionary(id))
            .filter(|dictionary| dictionary.contains("/Type /StructElem /S /Formula "))
            .collect()
    }

    fn formula_links(&self) -> Vec<Rect> {
        (1..self.offsets.len())
            .filter_map(|id| {
                let dictionary = self.dictionary(id);
                dictionary
                    .contains("/URI (https://example.com/formula)")
                    .then(|| rectangle(dictionary, "/Rect ["))
            })
            .collect()
    }
}

fn number(text: &str, key: &str) -> usize {
    text.split_once(key)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn references(text: &str, key: &str) -> Vec<usize> {
    let fields: Vec<_> = text
        .split_once(key)
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .collect();
    assert_eq!(fields.len() % 3, 0);
    fields
        .chunks_exact(3)
        .map(|reference| {
            assert_eq!(&reference[1..], &["0", "R"]);
            reference[0].parse().unwrap()
        })
        .collect()
}

fn rectangle(text: &str, key: &str) -> Rect {
    let fields: Vec<_> = text
        .split_once(key)
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    fields.try_into().unwrap()
}

// Inline math emits absolute PDF paths. The control polygon contains every
// Bezier curve, so its bounds are an independent conservative ink oracle.
fn path_bounds(body: &str) -> Rect {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut points = 0;
    for line in body.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let coordinates = match fields.last() {
            Some(&"m" | &"l") if fields.len() == 3 => &fields[..2],
            Some(&"c") if fields.len() == 7 => &fields[..6],
            _ => continue,
        };
        for pair in coordinates.chunks_exact(2) {
            let x: f64 = pair[0].parse().unwrap();
            let y: f64 = pair[1].parse().unwrap();
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
            points += 1;
        }
    }
    assert!(points > 10, "fixture must paint real formula outlines");
    bounds
}

fn assert_inside(ink: Rect, frame: Rect) {
    assert!(
        ink[0] >= frame[0] - 0.025
            && ink[1] >= frame[1] - 0.025
            && ink[2] <= frame[2] + 0.025
            && ink[3] <= frame[3] + 0.025,
        "formula ink {ink:?} escapes {frame:?}"
    );
}

fn matrix(rows: usize) -> String {
    format!(
        r"\begin{{matrix}}{}\end{{matrix}}",
        vec!["x"; rows].join(r"\\")
    )
}

fn options() -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize {
        name: "tall-inline-math",
        width_pt: 216.0,
        height_pt: 216.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    options
}

fn unicode_map(objects: &Objects<'_>, font: usize) -> BTreeMap<u16, String> {
    let cmap = objects.stream(number(objects.dictionary(font), "/ToUnicode "));
    let mut chars = false;
    let mut map = BTreeMap::new();
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            chars = true;
        } else if line == "endbfchar" {
            chars = false;
        } else if chars {
            let entries: Vec<_> = line.split_whitespace().collect();
            let id = u16::from_str_radix(entries[0].trim_matches(['<', '>']), 16).unwrap();
            let units: Vec<_> = entries[1]
                .trim_matches(['<', '>'])
                .as_bytes()
                .chunks_exact(4)
                .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
                .collect();
            map.insert(id, String::from_utf16(&units).unwrap());
        }
    }
    assert!(!map.is_empty());
    map
}

fn visible_prose(objects: &Objects<'_>, frame: Rect) -> String {
    let mut prose = String::new();
    for page in objects.pages() {
        let page = objects.dictionary(page);
        let entries: Vec<_> = page
            .split_once("/Font << ")
            .unwrap()
            .1
            .split_once(" >>")
            .unwrap()
            .0
            .split_whitespace()
            .collect();
        assert_eq!(entries.len() % 4, 0);
        let fonts: BTreeMap<_, _> = entries
            .chunks_exact(4)
            .map(|entry| (entry[0], unicode_map(objects, entry[1].parse().unwrap())))
            .collect();
        let content = objects.stream(number(page, "/Contents "));
        for line in content.lines().filter(|line| line.contains("BT /F")) {
            let separated = line
                .replace('[', " [ ")
                .replace(']', " ] ")
                .replace('<', " <")
                .replace('>', "> ");
            let words: Vec<_> = separated.split_whitespace().collect();
            let mut font = None;
            let mut mode = 0;
            let mut baseline = None;
            let mut text = String::new();
            for (index, word) in words.iter().enumerate() {
                match *word {
                    "Tf" => font = Some(&fonts[words[index - 2]]),
                    "Tr" => mode = words[index - 1].parse::<u8>().unwrap(),
                    "Tm" => {
                        baseline = Some([
                            words[index - 2].parse::<f64>().unwrap(),
                            words[index - 1].parse::<f64>().unwrap(),
                        ])
                    }
                    "Tj" | "TJ" => {
                        if mode != 3 && !text.is_empty() {
                            let [x, y] =
                                baseline.expect("painted text needs its physical baseline");
                            assert_inside([x, y, x, y], frame);
                            prose.push_str(&text);
                        }
                        text.clear();
                    }
                    hex if mode != 3 && hex.starts_with('<') && hex.ends_with('>') => {
                        let font = font.expect("painted text needs a font");
                        let hex = hex.trim_matches(['<', '>']);
                        assert_eq!(hex.len() % 4, 0);
                        for glyph in hex.as_bytes().chunks_exact(4) {
                            let id = u16::from_str_radix(std::str::from_utf8(glyph).unwrap(), 16)
                                .unwrap();
                            text.push_str(
                                font.get(&id).expect("every painted glyph needs Unicode"),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    prose.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn assert_formulas_inside(pdf: &[u8], expected: usize) {
    let objects = Objects::new(pdf);
    let ink = objects.formula_bounds();
    let boxes = objects.formula_boxes();
    assert_eq!(ink.len(), expected, "all logical formulas must be painted");
    assert_eq!(
        boxes.len(),
        expected,
        "all logical formulas retain their tags"
    );
    for (ink, bbox) in ink.into_iter().zip(boxes) {
        assert_inside(ink, bbox);
        assert_inside(bbox, [24.0, 24.0, 192.0, 192.0]);
    }
    for link in objects.formula_links() {
        assert_inside(link, [24.0, 24.0, 192.0, 192.0]);
    }
    // Repeated headers paint as artifacts rather than duplicate Formula tags.
    // Inspect every physical page as well, so their ink cannot evade the oracle.
    for page in objects.pages() {
        let content = objects.stream(number(objects.dictionary(page), "/Contents "));
        if content.contains("/ActualText ") {
            assert_inside(path_bounds(&content), [24.0, 24.0, 192.0, 192.0]);
        }
    }
}

#[test]
fn thirty_row_inline_matrices_keep_all_outlines_and_neighboring_unicode_visible() {
    let source = matrix(30);
    for delimiter in ["$", "$$"] {
        let markdown = format!("Before café {delimiter}{source}{delimiter} after résumé.");
        let pdf = render_pdf(&markdown, &options()).unwrap();
        assert_formulas_inside(&pdf, 1);
        let objects = Objects::new(&pdf);
        assert_eq!(
            visible_prose(&objects, [24.0, 24.0, 192.0, 192.0]),
            "Beforecaféafterrésumé."
        );
        let outlines: usize = objects
            .pages()
            .into_iter()
            .map(|page| {
                let content = objects.stream(number(objects.dictionary(page), "/Contents "));
                let formula = content
                    .split_once("/Formula <</MCID ")
                    .unwrap()
                    .1
                    .split_once("/ActualText ")
                    .unwrap()
                    .0;
                formula.lines().filter(|line| *line == "f").count()
            })
            .sum();
        assert_eq!(
            outlines, 30,
            "every matrix row must keep its visible x outline"
        );
    }
}

#[test]
fn opposite_ascent_and_descent_extents_share_one_safe_row_budget() {
    let source = matrix(30);
    let markdown =
        format!("Before $\\frac{{{source}}}{{x}}$ middle $\\frac{{x}}{{{source}}}$ after.");
    let pdf = render_pdf(&markdown, &options()).unwrap();
    assert_formulas_inside(&pdf, 2);
    assert_eq!(
        visible_prose(&Objects::new(&pdf), [24.0, 24.0, 192.0, 192.0]),
        "Beforemiddleafter."
    );
}

#[test]
fn positioned_mark_ink_and_deep_formulas_share_the_same_row_budget() {
    // This checked-in fixture has independently recorded mark/mkmk anchors:
    // the first mark is 0.5 em above its base, each following mark adds 0.15 em,
    // and every visible glyph has the triangle bbox [0.02, 0, 0.22, 0.4] em.
    // The complete stack is 3.9 em high, well beyond nominal text ascent.
    const FONT: &[u8] = include_bytes!("../fmd-font/fonts/test-shaping/FmdShaping.ttf");
    let stacked = format!("a\u{0301}{}", "\u{0307}".repeat(20));
    let source = format!(r"\frac{{x}}{{{}}}", matrix(30));
    for markdown in [
        format!("{stacked} ${source}$ b"),
        format!("| a | b |\n| --- | --- |\n| {stacked} | ${source}$ b |\n"),
    ] {
        for grid in [None, Some(BaselineGrid::from_points(12.0).unwrap())] {
            let mut opts = options();
            opts.theme.page.baseline_grid = grid;
            opts.font_assets.body_regular = Some(FONT.to_vec());
            let pdf = render_pdf(&markdown, &opts).unwrap();
            assert_formulas_inside(&pdf, 1);
            let objects = Objects::new(&pdf);
            let prose = visible_prose(&objects, [24.0, 24.0, 192.0, 192.0]);
            assert_eq!(prose.matches(&stacked).count(), 1, "stack lost: {prose}");
            assert!(prose.ends_with('b'), "neighboring prose lost: {prose}");
            let mut positioned = 0;
            for page in objects.pages() {
                let content = objects.stream(number(objects.dictionary(page), "/Contents "));
                for line in content.lines().filter(|line| {
                    line.contains("BT /F") && line.contains("> Tj") && !line.contains(" 3 Tr")
                }) {
                    let fields: Vec<_> = line.split_whitespace().collect();
                    let font_at = fields.iter().position(|word| *word == "Tf").unwrap();
                    let size: f64 = fields[font_at - 1].parse().unwrap();
                    let cmap = unicode_map(
                        &objects,
                        number(
                            objects.dictionary(page),
                            &format!("{} ", fields[font_at - 2]),
                        ),
                    );
                    let mut baseline = None;
                    for (index, word) in fields.iter().enumerate() {
                        if *word == "Tm" {
                            baseline = Some((
                                fields[index - 2].parse::<f64>().unwrap(),
                                fields[index - 1].parse::<f64>().unwrap(),
                            ));
                        } else if *word == "Tj" {
                            let cid =
                                u16::from_str_radix(fields[index - 1].trim_matches(['<', '>']), 16)
                                    .unwrap();
                            // A joined positioned run may retain its trailing
                            // space; that glyph has no triangle or visible ink.
                            if cmap[&cid].chars().all(char::is_whitespace) {
                                continue;
                            }
                            let (x, y) = baseline.unwrap();
                            assert_inside(
                                [x + 0.02 * size, y, x + 0.22 * size, y + 0.4 * size],
                                [24.0, 24.0, 192.0, 192.0],
                            );
                            positioned += 1;
                        }
                    }
                }
            }
            assert_eq!(positioned, stacked.chars().count());
        }
    }
}

#[test]
fn tall_formula_fits_headings_quotes_lists_definitions_and_notes() {
    let source = matrix(30);
    for markdown in [
        format!("# Before ${source}$ after"),
        format!("# Prelude\n\nBefore ${source}$ after"),
        format!("> Before $${source}$$ after"),
        format!("- Before ${source}$ after"),
        format!("Term\n: Before ${source}$ after"),
        format!("Reference[^eq].\n\n[^eq]: Before ${source}$ after"),
    ] {
        let pdf = render_pdf(&markdown, &options()).unwrap();
        assert_formulas_inside(&pdf, 1);
        let prose = visible_prose(&Objects::new(&pdf), [24.0, 24.0, 192.0, 192.0]);
        assert!(
            prose.contains("Before") && prose.contains("after"),
            "prose lost: {prose}"
        );
    }
}

#[test]
fn tall_table_cells_fit_below_every_repeated_header_with_links_and_prose() {
    let source = matrix(30);
    let mut markdown = String::from("| Formula | Label |\n| --- | --- |\n");
    for index in 0..4 {
        markdown.push_str(&format!(
            "| [${source}$](https://example.com/formula) | café {index} |\n"
        ));
    }
    let pdf = render_pdf(&markdown, &options()).unwrap();
    assert_formulas_inside(&pdf, 4);
    let objects = Objects::new(&pdf);
    assert!(
        objects.pages().len() >= 4,
        "each tall body row needs its own page"
    );
    assert_eq!(objects.formula_links().len(), 4);
    let prose = visible_prose(&objects, [24.0, 24.0, 192.0, 192.0]);
    for index in 0..4 {
        assert!(
            prose.contains(&format!("café{index}")),
            "body row lost: {prose}"
        );
    }
    assert_eq!(prose.matches("FormulaLabel").count(), objects.pages().len());
}

#[test]
fn tall_header_and_body_formulas_share_the_page_instead_of_clipping() {
    let source = matrix(30);
    let markdown = format!(
        "| ${source}$ | Heading |\n| --- | --- |\n| ${source}$ | Before |\n| ${source}$ | after |\n"
    );
    for grid in [None, Some(BaselineGrid::from_points(12.0).unwrap())] {
        let mut opts = options();
        opts.theme.page.baseline_grid = grid;
        let pdf = render_pdf(&markdown, &opts).unwrap();
        assert_formulas_inside(&pdf, 3);
        let objects = Objects::new(&pdf);
        let prose = visible_prose(&objects, [24.0, 24.0, 192.0, 192.0]);
        assert!(prose.contains("Before") && prose.contains("after"));
    }
}

#[test]
fn vertical_fit_uses_the_same_geometry_under_grid_and_strict_pagination() {
    let source = matrix(30);
    let markdown =
        format!("Above.\n\nBefore [${source}$](https://example.com/formula) after.\n\nBelow.");
    let mut opts = options();
    opts.strict_pagination = true;
    opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(12.0).unwrap());
    let pdf = render_pdf(&markdown, &opts).unwrap();
    assert_formulas_inside(&pdf, 1);
    let prose = visible_prose(&Objects::new(&pdf), [24.0, 24.0, 192.0, 192.0]);
    assert_eq!(prose, "Above.Beforeafter.Below.");
}

#[test]
fn height_fits_are_reported_without_mutating_source_or_cached_geometry() {
    let source = matrix(30);
    let markdown = format!("Before ${source}$ after.");
    let opts = options();
    let mut tall = opts.clone();
    tall.theme.page.size.height_pt = 1600.0;
    let original = render_pdf(&markdown, &tall).unwrap();
    let pdf = render_pdf(&markdown, &opts).unwrap();
    let warnings = render_warnings(&parse_markdown(&markdown), &opts);
    let fits: Vec<_> = warnings
        .iter()
        .filter_map(|warning| match warning {
            RenderWarning::MathHeightOverflow {
                source: found,
                height_pt,
                measure_pt,
            } => {
                assert!(source.starts_with(found));
                assert_eq!(warning.code(), "math_height_overflow");
                Some((*height_pt, *measure_pt))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        fits.len(),
        1,
        "one fitted formula needs one diagnostic: {warnings:?}"
    );
    assert!(fits[0].0 > 350 && fits[0].1 < 168);
    assert_eq!(original, render_pdf(&markdown, &tall).unwrap());
    assert_eq!(pdf, render_pdf(&markdown, &opts).unwrap());
    assert_eq!(
        pdf,
        render_pdf_emitted(
            &markdown,
            &opts,
            PdfEmitOptions {
                emission: PdfPageEmission::Monolithic,
                ..PdfEmitOptions::default()
            }
        )
        .unwrap()
    );
    let layer =
        franken_markdown::pdf::verification_text_layer(&parse_markdown(&markdown), &opts).unwrap();
    let verified: String = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(verified, format!("Before {source} after."));
    assert!(
        layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .all(|run| !run.text.contains('\u{1e}'))
    );
}
