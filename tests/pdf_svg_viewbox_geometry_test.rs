//! SVG user units must not change image size, visible ink or link hitboxes.
//! Read real page streams and compose their graphics states independently of
//! the renderer's layout and SVG-transform helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PdfEmitOptions, PdfImageAsset, PdfOptions, PdfPageEmission, PdfRunningImage,
    PdfRunningImagePosition, parse_markdown, render_pdf, render_pdf_document_emitted,
};

type Rect = [f64; 4];
type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
const FULL: Rect = [0.0, 0.0, 1.0, 1.0];
const HEADER: &str = "/Artifact <</Type /Pagination /Subtype /Header>> BDC\n";
const FOOTER: &str = "/Artifact <</Type /Pagination /Subtype /Footer>> BDC\n";

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
            .collect::<Vec<_>>();
        assert_eq!(offsets.len(), count);
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

    fn dictionaries(&self) -> impl Iterator<Item = &'a str> + '_ {
        (1..self.offsets.len()).map(|id| self.dictionary(id))
    }

    fn content(&self) -> Vec<String> {
        self.dictionaries()
            .filter(|dict| dict.contains("/Type /Page "))
            .map(|page| {
                let id = number(page, "/Contents ");
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
            })
            .collect()
    }

    fn figure_boxes(&self) -> Vec<Rect> {
        self.dictionaries()
            .filter(|dict| dict.contains("/Type /StructElem /S /Figure "))
            .map(|dict| rectangle(dict, "/BBox ["))
            .collect()
    }

    fn links(&self, target: &str) -> Vec<Rect> {
        let uri = format!("/URI ({target})");
        self.dictionaries()
            .filter(|dict| dict.contains("/Subtype /Link") && dict.contains(&uri))
            .map(|dict| rectangle(dict, "/Rect ["))
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

fn rectangle(text: &str, key: &str) -> Rect {
    text.split_once(key)
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

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

fn transformed_rect(matrix: Matrix, rect: Rect) -> Rect {
    let [a, b, c, d, e, f] = matrix;
    let [x, y, w, h] = rect;
    let points = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
        .map(|(x, y)| (a * x + c * y + e, b * x + d * y + f));
    [
        points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max),
        points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max),
    ]
}

fn intersect(a: Rect, b: Rect) -> Rect {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

#[derive(Clone, Copy)]
struct State {
    matrix: Matrix,
    clip: Option<Rect>,
}

#[derive(Debug)]
struct Paint {
    uncut: Rect,
    visible: Rect,
    clip: Option<Rect>,
}

// Fixtures intentionally use one filled rectangle. Follow cm, q/Q and the
// actual rectangular clipping path; a slice cannot pass just by having the
// correct metadata while drawing beyond the viewport.
fn rectangle_paint(body: &str) -> Paint {
    let tokens: Vec<_> = body.split_whitespace().collect();
    let mut state = State {
        matrix: IDENTITY,
        clip: None,
    };
    let mut stack = Vec::new();
    let mut path = None;
    let mut paint = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match *token {
            "q" => stack.push(state),
            "Q" => state = stack.pop().unwrap(),
            "cm" => {
                let matrix = std::array::from_fn(|offset| {
                    let field = tokens[index - 6 + offset];
                    assert!(!field.contains(['e', 'E']), "PDF numbers forbid exponents");
                    let value: f64 = field.parse().unwrap();
                    assert!(value.is_finite());
                    value
                });
                state.matrix = multiply(state.matrix, matrix);
            }
            "re" => {
                let rect =
                    std::array::from_fn(|offset| tokens[index - 4 + offset].parse().unwrap());
                path = Some(transformed_rect(state.matrix, rect));
            }
            "W" | "W*" => {
                let rect = path.unwrap();
                state.clip = Some(state.clip.map_or(rect, |clip| intersect(clip, rect)));
            }
            "n" => path = None,
            "f" => {
                let uncut = path.take().unwrap();
                paint.push(Paint {
                    uncut,
                    visible: state.clip.map_or(uncut, |clip| intersect(clip, uncut)),
                    clip: state.clip,
                });
            }
            _ => {}
        }
    }
    assert!(stack.is_empty());
    assert_eq!(paint.len(), 1, "one real filled rectangle: {body}");
    paint.pop().unwrap()
}

fn assert_rect(actual: Rect, expected: Rect) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 0.025),
        "actual {actual:?}, expected {expected:?}"
    );
}

fn within(frame: Rect, relative: Rect) -> Rect {
    let w = frame[2] - frame[0];
    let h = frame[3] - frame[1];
    [
        frame[0] + relative[0] * w,
        frame[1] + relative[1] * h,
        frame[0] + relative[2] * w,
        frame[1] + relative[3] * h,
    ]
}

struct Case {
    view_box: Rect,
    aspect: &'static str,
    // Expected artwork bounds as fractions of the image viewport, before any
    // slice clipping. This is specified independently of SVG transformation code.
    uncut: Rect,
}

fn cases() -> [Case; 10] {
    [
        Case {
            view_box: [0.25, -0.5, 0.5, 0.25],
            aspect: "xMidYMid meet",
            uncut: FULL,
        },
        Case {
            view_box: [250_000.0, -500_000.0, 1_000_000.0, 500_000.0],
            aspect: "xMidYMid meet",
            uncut: FULL,
        },
        Case {
            view_box: [
                -1_000_000_000.0,
                500_000_000.0,
                1_000_000_000.0,
                500_000_000.0,
            ],
            aspect: "none",
            uncut: FULL,
        },
        Case {
            view_box: [0.25, -0.5, 0.5, 0.5],
            aspect: "none",
            uncut: FULL,
        },
        Case {
            view_box: [0.25, -0.5, 0.5, 0.5],
            aspect: "xMinYMin meet",
            uncut: [0.0, 0.0, 0.5, 1.0],
        },
        Case {
            view_box: [0.25, -0.5, 0.8, 0.2],
            aspect: "xMaxYMax meet",
            uncut: [0.0, 0.0, 1.0, 0.5],
        },
        Case {
            view_box: [2_000_000.0, -1_000_000.0, 1_000_000.0, 250_000.0],
            aspect: "xMidYMid meet",
            uncut: [0.0, 0.25, 1.0, 0.75],
        },
        Case {
            view_box: [0.25, -0.5, 0.5, 0.5],
            aspect: "xMidYMax slice",
            uncut: [0.0, 0.0, 1.0, 2.0],
        },
        Case {
            view_box: [2_000_000.0, -1_000_000.0, 1_000_000.0, 250_000.0],
            aspect: "xMaxYMid slice",
            uncut: [-1.0, 0.0, 1.0, 1.0],
        },
        Case {
            view_box: [10.0, -20.0, 1080.0, 540.0],
            aspect: "xMidYMid meet",
            uncut: FULL,
        },
    ]
}

fn options(case: &Case) -> PdfOptions {
    let [x, y, w, h] = case.view_box;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"100\" viewBox=\"{x} {y} {w} {h}\" preserveAspectRatio=\"{}\"><a href=\"https://example.com/shape\"><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"#125abc\"/></a></svg>",
        case.aspect,
    );
    PdfOptions {
        image_assets: vec![PdfImageAsset::new("image", svg.into_bytes())],
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

fn verify_paint(case: &Case, paint: &Paint, frame: Rect) {
    let expected = within(frame, case.uncut);
    assert_rect(paint.uncut, expected);
    let visible = if case.aspect.ends_with("slice") {
        assert_rect(paint.clip.expect("slice clips the real drawing"), frame);
        intersect(expected, frame)
    } else {
        expected
    };
    assert!(visible[2] > visible[0] && visible[3] > visible[1]);
    assert_rect(paint.visible, visible);
}

#[test]
fn standalone_and_inline_figures_honor_fractional_huge_and_offset_viewboxes() {
    for case in cases() {
        for source in ["![Geometry](image)", "Before ![Geometry](image) after."] {
            let opts = options(&case);
            let pdf = render_pdf(source, &opts).unwrap();
            let objects = Objects::new(&pdf);
            let figures = objects.figure_boxes();
            assert_eq!(figures.len(), 1);
            let frame = figures[0];
            assert!((frame[2] - frame[0]) > 30.0);
            assert!((frame[3] - frame[1]) > 15.0);
            assert!(((frame[2] - frame[0]) / (frame[3] - frame[1]) - 2.0).abs() < 0.005);
            if source.starts_with('!') {
                assert_rect(
                    [0.0, 0.0, frame[2] - frame[0], frame[3] - frame[1]],
                    [0.0, 0.0, 150.0, 75.0],
                );
            }
            let pages = objects.content();
            assert_eq!(pages.len(), 1);
            let figure = pages[0]
                .split_once("/Figure <</MCID ")
                .unwrap()
                .1
                .split_once("EMC")
                .unwrap()
                .0;
            let paint = rectangle_paint(figure);
            verify_paint(&case, &paint, frame);
            let links = objects.links("https://example.com/shape");
            assert_eq!(links.len(), 1);
            assert_rect(links[0], paint.visible);
        }
    }
}

#[test]
fn markdown_link_owns_the_full_letterboxed_viewport() {
    let case = &cases()[4];
    let opts = options(case);
    let pdf = render_pdf("[![Geometry](image)](https://example.com/figure)", &opts).unwrap();
    let objects = Objects::new(&pdf);
    assert!(objects.links("https://example.com/shape").is_empty());
    let figures = objects.figure_boxes();
    let links = objects.links("https://example.com/figure");
    assert_eq!(figures.len(), 1);
    assert_eq!(links.len(), 1);
    assert_rect(links[0], figures[0]);
    let pages = objects.content();
    let figure = pages[0]
        .split_once("/Figure <</MCID ")
        .unwrap()
        .1
        .split_once("EMC")
        .unwrap()
        .0;
    verify_paint(case, &rectangle_paint(figure), figures[0]);
}

#[test]
fn running_svg_artwork_uses_the_same_geometry_without_links_or_figures() {
    for case in cases() {
        let mut opts = options(&case);
        opts.running.header.image = Some(PdfRunningImage {
            dest: "image".into(),
            position: PdfRunningImagePosition::Left,
            height_pt: Some(16),
        });
        opts.running.footer.image = Some(PdfRunningImage {
            dest: "image".into(),
            position: PdfRunningImagePosition::Right,
            height_pt: Some(16),
        });
        let pdf = render_pdf("Body paragraph.", &opts).unwrap();
        let objects = Objects::new(&pdf);
        assert!(objects.figure_boxes().is_empty());
        assert!(objects.links("https://example.com/shape").is_empty());
        let pages = objects.content();
        assert_eq!(pages.len(), 1);
        for marker in [HEADER, FOOTER] {
            let band = pages[0]
                .split_once(marker)
                .unwrap()
                .1
                .split_once("\nEMC\n")
                .unwrap()
                .0;
            assert!(!band.contains("/MCID"));
            let paint = rectangle_paint(band);
            let frame = paint.clip.expect("running artwork clips to its fitted box");
            assert_rect(
                [0.0, 0.0, frame[2] - frame[0], frame[3] - frame[1]],
                [0.0, 0.0, 32.0, 16.0],
            );
            let page = &opts.theme.page;
            if marker == HEADER {
                assert!((frame[0] - f64::from(page.margins.left_pt)).abs() < 0.025);
                assert!(frame[1] >= f64::from(page.size.height_pt - page.margins.top_pt));
            } else {
                assert!(
                    (frame[2] - f64::from(page.size.width_pt - page.margins.right_pt)).abs()
                        < 0.025
                );
                assert!(frame[3] <= f64::from(page.margins.bottom_pt));
            }
            verify_paint(&case, &paint, frame);
        }
    }
}

#[test]
fn exact_scale_control_and_all_page_emitters_remain_deterministic() {
    let ordinary = Case {
        view_box: [0.0, 0.0, 200.0, 100.0],
        aspect: "xMidYMid meet",
        uncut: FULL,
    };
    let opts = options(&ordinary);
    let pdf = render_pdf("![Geometry](image)", &opts).unwrap();
    assert!(Objects::new(&pdf).content()[0].contains("q 0.75 0 0 -0.75 72 720 cm\n"));
    assert_eq!(pdf, render_pdf("![Geometry](image)", &opts).unwrap());

    let document = parse_markdown("![Geometry](image)\n\nBefore ![Geometry](image) after.");
    for case in cases() {
        let opts = options(&case);
        let chunked =
            render_pdf_document_emitted(&document, &opts, PdfEmitOptions::default()).unwrap();
        let monolithic = render_pdf_document_emitted(
            &document,
            &opts,
            PdfEmitOptions {
                emission: PdfPageEmission::Monolithic,
                ..PdfEmitOptions::default()
            },
        )
        .unwrap();
        assert_eq!(chunked, monolithic);
    }
}
