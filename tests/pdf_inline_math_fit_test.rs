//! Real emitted outline geometry, tags and links for bounded inline formulas.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    BaselineGrid, PageMargins, PageSize, PdfEmitOptions, PdfOptions, PdfPageEmission,
    RenderWarning, parse_markdown, render_pdf, render_pdf_emitted, render_warnings,
};

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

fn formula(terms: usize) -> String {
    (0..terms)
        .map(|index| format!("x_{{{index}}}"))
        .collect::<Vec<_>>()
        .join("+")
}

fn options() -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize {
        name: "math-fit",
        width_pt: 300.0,
        height_pt: 500.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    options
}

#[test]
fn oversized_formulas_fit_prose_headings_quotes_lists_and_adjoining_text() {
    let source = formula(40);
    for markdown in [
        format!("Before ${source}$ after."),
        format!("Before $${source}$$ after."),
        format!("# Heading ${source}$"),
        format!("# Heading $${source}$$"),
        format!("> Before ${source}$ after."),
        format!("- Before ${source}$ after."),
        format!("prefix${source}$suffix${source}$tail"),
        format!("Before[^eq].\n\n[^eq]: ${source}$ after."),
    ] {
        let pdf = render_pdf(&markdown, &options()).unwrap();
        let objects = Objects::new(&pdf);
        let outlines = objects.formula_bounds();
        assert!(!outlines.is_empty(), "formula lost: {markdown}");
        for ink in outlines {
            assert_inside(ink, [24.0, 24.0, 276.0, 476.0]);
        }
        for bbox in objects.formula_boxes() {
            assert_inside(bbox, [24.0, 24.0, 276.0, 476.0]);
        }
    }
}

#[test]
fn narrow_table_formulas_have_local_bounds_tags_and_click_targets() {
    let source = formula(12);
    let cell = format!("[${source}$](https://example.com/formula)");
    let display_cell = format!("[$${source}$$](https://example.com/formula)");
    let markdown = format!(
        "| One | Two | Three | Four |\n| --- | --- | --- | --- |\n| {cell} | {display_cell} | {cell} | {display_cell} |\n"
    );
    let options = PdfOptions::default();
    let pdf = render_pdf(&markdown, &options).unwrap();
    let objects = Objects::new(&pdf);
    let boxes = objects.formula_boxes();
    let outlines = objects.formula_bounds();
    let links = objects.formula_links();
    assert_eq!(
        boxes.len(),
        4,
        "each formula must remain identified inside its table cell"
    );
    assert_eq!(outlines.len(), 4);
    assert_eq!(links.len(), 4);
    for element in objects.formula_elements() {
        let parent = objects.dictionary(number(element, "/P "));
        assert!(
            parent.contains("/S /TD "),
            "formula must belong to its logical cell: {parent}"
        );
    }
    for ((ink, bbox), link) in outlines.iter().zip(&boxes).zip(&links) {
        assert_inside(*ink, *bbox);
        assert_inside(*ink, *link);
        assert_inside(*bbox, [72.0, 72.0, 540.0, 720.0]);
    }
    for pair in boxes.windows(2) {
        assert!(
            pair[0][2] < pair[1][0],
            "neighboring formula cells overlap: {pair:?}"
        );
    }
    assert!(String::from_utf8_lossy(&pdf).contains(&format!("/Alt ({source})")));
    let warnings = render_warnings(&parse_markdown(&markdown), &options);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.code() == "math_overflow"),
        "cell fit must be reported: {warnings:?}"
    );
}

#[test]
fn fitting_warnings_use_the_local_measure_and_heading_size() {
    let source = formula(40);
    let only_fit = |markdown: &str| {
        let warnings = render_warnings(&parse_markdown(markdown), &options());
        let fits: Vec<_> = warnings
            .iter()
            .filter_map(|warning| match warning {
                RenderWarning::MathOverflow {
                    width_pt,
                    measure_pt,
                    ..
                } => Some((*width_pt, *measure_pt)),
                _ => None,
            })
            .collect();
        assert_eq!(
            fits.len(),
            1,
            "one final fit should yield one warning: {warnings:?}"
        );
        fits[0]
    };
    let body = only_fit(&format!("${source}$"));
    let heading = only_fit(&format!("# ${source}$"));
    let quote = only_fit(&format!("> ${source}$"));
    assert_eq!(body.1, 252, "configured page width minus its margins");
    assert_eq!(heading.1, body.1);
    assert!(
        heading.0 > body.0,
        "a heading must use its own larger font size"
    );
    assert_eq!(quote.1, 234, "blockquote reserves its 18-point indentation");
    assert_eq!(quote.0, body.0);
}

#[test]
fn repeated_table_header_math_keeps_one_logical_formula_and_balanced_marks() {
    let source = formula(20);
    let mut markdown = format!("| ${source}$ | Status |\n| --- | --- |\n");
    for index in 0..90 {
        markdown.push_str(&format!("| Item {index} | Ready |\n"));
    }
    let pdf = render_pdf(&markdown, &options()).unwrap();
    let objects = Objects::new(&pdf);
    let pages = objects.pages();
    assert!(pages.len() > 1, "fixture must repeat its table header");
    let formulas = objects.formula_elements();
    assert_eq!(
        formulas.len(),
        1,
        "repeated paint cannot duplicate logical formula children"
    );
    let parent = objects.dictionary(number(formulas[0], "/P "));
    assert!(parent.contains("/S /TH "));
    for (index, page) in pages.into_iter().enumerate() {
        let stream = objects.stream(number(objects.dictionary(page), "/Contents "));
        assert_eq!(
            stream.matches("/Formula <</MCID ").count(),
            usize::from(index == 0)
        );
        assert_eq!(
            stream.matches("/ActualText ").count(),
            1,
            "each painted header retains its formula"
        );
        let tokens: Vec<_> = stream.split_whitespace().collect();
        let opens = tokens
            .iter()
            .filter(|&&token| token == "BDC" || token == "BMC")
            .count();
        let closes = tokens.iter().filter(|&&token| token == "EMC").count();
        assert_eq!(
            opens, closes,
            "page {index} has unmatched marked-content operators"
        );
    }
}

#[test]
fn fitting_includes_negative_kerns_and_ink_under_grid_and_exact_pagination() {
    let source = format!(r"\hspace{{-30em}}\frac{{{}}}{{y}}", formula(30));
    let markdown = format!("Above.\n\n[${source}$](https://example.com/formula)\n\nBelow.");
    let mut options = options();
    options.strict_pagination = true;
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
    let pdf = render_pdf(&markdown, &options).unwrap();
    let objects = Objects::new(&pdf);
    let outlines = objects.formula_bounds();
    let boxes = objects.formula_boxes();
    let links = objects.formula_links();
    assert_eq!(outlines.len(), 1);
    assert_eq!(boxes.len(), 1);
    assert_eq!(links.len(), 1);
    assert_inside(outlines[0], boxes[0]);
    assert_inside(outlines[0], links[0]);
    assert_inside(outlines[0], [24.0, 24.0, 276.0, 476.0]);
}

#[test]
fn fitted_math_keeps_deterministic_chunked_and_monolithic_bytes() {
    let source = formula(30);
    let markdown = format!("## Equation\n\n[${source}$](https://example.com/formula)\n\nAfter.");
    let options = options();
    let mut wide = options.clone();
    wide.theme.page.size.width_pt = 1600.0;
    let wide_before = render_pdf(&markdown, &wide).unwrap();
    let pdf = render_pdf(&markdown, &options).unwrap();
    assert_eq!(pdf, render_pdf(&markdown, &options).unwrap());
    assert_eq!(
        wide_before,
        render_pdf(&markdown, &wide).unwrap(),
        "a local fit cannot mutate cached formula geometry"
    );
    let layer =
        franken_markdown::pdf::verification_text_layer(&parse_markdown(&markdown), &options)
            .unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(
        runs.iter().any(|run| run.text == source),
        "verification must expose original TeX source"
    );
    assert!(
        runs.iter()
            .all(|run| !run.text.contains('\u{1e}') && run.overshoot.is_none())
    );
    assert_eq!(
        pdf,
        render_pdf_emitted(
            &markdown,
            &options,
            PdfEmitOptions {
                emission: PdfPageEmission::Monolithic,
                ..PdfEmitOptions::default()
            }
        )
        .unwrap()
    );
}
