//! Containing measures must bound real image ink, links and tagged figures.
//! Inspect emitted PDF objects and drawing matrices independently of layout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PageMargins, PageSize, PdfEmitOptions, PdfImageAsset, PdfOptions, PdfPageEmission,
    parse_markdown, render_pdf, render_pdf_document_emitted,
};

type Rect = [f64; 4];
type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

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

    fn figures(&self) -> Vec<(&'a str, Rect)> {
        (1..self.offsets.len())
            .filter_map(|id| {
                let dictionary = self.dictionary(id);
                dictionary
                    .contains("/Type /StructElem /S /Figure ")
                    .then(|| {
                        let alt = dictionary
                            .split_once("/Alt (")
                            .unwrap()
                            .1
                            .split_once(')')
                            .unwrap()
                            .0;
                        (alt, rectangle(dictionary, "/BBox ["))
                    })
            })
            .collect()
    }

    fn image_links(&self) -> Vec<Rect> {
        self.pages()
            .into_iter()
            .flat_map(|page| {
                let dictionary = self.dictionary(page);
                if !dictionary.contains("/Annots [") {
                    return Vec::new();
                }
                references(dictionary, "/Annots [")
                    .into_iter()
                    .filter_map(|id| {
                        let annotation = self.dictionary(id);
                        annotation
                            .contains("/URI (https://example.com/image)")
                            .then(|| rectangle(annotation, "/Rect ["))
                    })
                    .collect()
            })
            .collect()
    }

    fn painted_images(&self) -> Vec<Rect> {
        let mut bounds = Vec::new();
        for page in self.pages() {
            let stream = self.stream(number(self.dictionary(page), "/Contents "));
            for figure in stream.split("/Figure <</MCID ").skip(1) {
                let body = figure.split_once("EMC").unwrap().0;
                bounds.extend(painted_rectangles(body));
            }
        }
        bounds
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
    let list = text.split_once(key).unwrap().1.split_once(']').unwrap().0;
    let fields: Vec<_> = list.split_whitespace().collect();
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
    let fields: Vec<f64> = text
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

// These fixtures contain only raster Do operations or a filled SVG rectangle.
// Follow actual graphics-state transforms; metadata is not the ink oracle.
fn painted_rectangles(body: &str) -> Vec<Rect> {
    let tokens: Vec<_> = body.split_whitespace().collect();
    let mut matrix = IDENTITY;
    let mut stack = Vec::new();
    let mut bounds = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match *token {
            "q" => stack.push(matrix),
            "Q" => matrix = stack.pop().unwrap(),
            "cm" => {
                let next =
                    std::array::from_fn(|offset| tokens[index - 6 + offset].parse().unwrap());
                matrix = multiply(matrix, next);
            }
            "Do" if tokens[index - 1].starts_with("/Im") => {
                bounds.push(transformed_rect(matrix, [0.0, 0.0, 1.0, 1.0]));
            }
            "re" if tokens.get(index + 1) == Some(&"f") => {
                let rectangle =
                    std::array::from_fn(|offset| tokens[index - 4 + offset].parse().unwrap());
                bounds.push(transformed_rect(matrix, rectangle));
            }
            _ => {}
        }
    }
    assert!(stack.is_empty());
    bounds
}

fn near(a: Rect, b: Rect) -> bool {
    a.into_iter().zip(b).all(|(a, b)| (a - b).abs() < 0.025)
}

fn assert_contained(rect: Rect, frame: Rect) {
    assert!(
        rect[2] > rect[0] && rect[3] > rect[1],
        "empty image: {rect:?}"
    );
    assert!(
        rect[0] >= frame[0] - 0.025
            && rect[1] >= frame[1] - 0.025
            && rect[2] <= frame[2] + 0.025
            && rect[3] <= frame[3] + 0.025,
        "image {rect:?} is outside {frame:?}"
    );
}

fn options(width: f32, height: f32, asset: PdfImageAsset) -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize {
        name: "fit-test",
        width_pt: width,
        height_pt: height,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 20.0,
        right_pt: 20.0,
        bottom_pt: 20.0,
        left_pt: 20.0,
    };
    options.image_assets = vec![asset];
    options
}

fn svg(width: u32, height: u32) -> PdfImageAsset {
    PdfImageAsset::new("image", format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\"><rect width=\"{width}\" height=\"{height}\" fill=\"#125abc\"/></svg>"
    ).into_bytes())
}

fn png(width: u32, height: u32) -> PdfImageAsset {
    fn chunk(bytes: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(payload);
        let mut crc = !0u32;
        for &byte in kind.iter().chain(payload) {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
            }
        }
        bytes.extend_from_slice(&(!crc).to_be_bytes());
    }
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = width.to_be_bytes().to_vec();
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut bytes, b"IHDR", &header);
    let row: Vec<u8> = std::iter::once(0)
        .chain([18, 90, 188].into_iter().cycle().take(width as usize * 3))
        .collect();
    chunk(
        &mut bytes,
        b"IDAT",
        &franken_markdown::compress::zlib_compress(&row.repeat(height as usize)),
    );
    chunk(&mut bytes, b"IEND", &[]);
    PdfImageAsset::new("image", bytes)
}

fn rotated_jpeg() -> PdfImageAsset {
    let hex: String = include_str!("fixtures/jpeg-inline-portrait.hex")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let mut bytes: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    // Original20x1200 image, rotated90degrees: its displayed aspect is60:1.
    let mut exif = b"Exif\0\0II".to_vec();
    exif.extend_from_slice(&42u16.to_le_bytes());
    exif.extend_from_slice(&8u32.to_le_bytes());
    exif.extend_from_slice(&1u16.to_le_bytes());
    exif.extend_from_slice(&0x0112u16.to_le_bytes());
    exif.extend_from_slice(&3u16.to_le_bytes());
    exif.extend_from_slice(&1u32.to_le_bytes());
    exif.extend_from_slice(&6u16.to_le_bytes());
    exif.extend_from_slice(&0u16.to_le_bytes());
    exif.extend_from_slice(&0u32.to_le_bytes());
    let mut app1 = vec![0xff, 0xe1];
    app1.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    app1.extend(exif);
    bytes.splice(2..2, app1);
    PdfImageAsset::new("image", bytes)
}

fn verify(source: &str, options: &PdfOptions, expected: usize, aspect: f64) {
    let pdf = render_pdf(source, options).unwrap();
    for asset in &options.image_assets {
        if asset.bytes.starts_with(&[0xff, 0xd8]) {
            assert!(
                pdf.windows(asset.bytes.len())
                    .any(|part| part == asset.bytes)
            );
        }
    }
    let objects = Objects::new(&pdf);
    let figures = objects.figures();
    assert_eq!(figures.len(), expected);
    let links = objects.image_links();
    assert_eq!(links.len(), expected);
    let paint = objects.painted_images();
    assert_eq!(paint.len(), expected);
    let frame = [
        20.0,
        20.0,
        f64::from(options.theme.page.size.width_pt) - 20.0,
        f64::from(options.theme.page.size.height_pt) - 20.0,
    ];
    for (_, rect) in &figures {
        assert_contained(*rect, frame);
        assert!(
            ((rect[2] - rect[0]) / (rect[3] - rect[1]) - aspect).abs() < aspect * 0.025,
            "aspect changed: {rect:?}"
        );
        assert!(
            links.iter().any(|link| near(*link, *rect)),
            "no matching link for {rect:?}"
        );
        assert!(
            paint.iter().any(|ink| near(*ink, *rect)),
            "paint {paint:?} disagrees with {rect:?}"
        );
    }
    for ink in paint {
        assert_contained(ink, frame);
    }
}

#[test]
fn wide_images_fit_narrow_prose_with_matching_ink_links_and_figures() {
    let source = "Before [![wide](image)](https://example.com/image) after.";
    for asset in [png(1200, 20), svg(1200, 20), rotated_jpeg()] {
        let options = options(160.0, 220.0, asset);
        verify(source, &options, 1, 60.0);
    }
}

#[test]
fn images_fit_table_cells_without_overlapping_the_next_column() {
    let source = "| [![header-left](image)](https://example.com/image) | [![header-right](image)](https://example.com/image) |\n|---|---|\n| [![left](image)](https://example.com/image) | [![right](image)](https://example.com/image) |\n";
    for asset in [png(1200, 20), svg(1200, 20), rotated_jpeg()] {
        let options = options(400.0, 500.0, asset);
        verify(source, &options, 4, 60.0);
        let pdf = render_pdf(source, &options).unwrap();
        let objects = Objects::new(&pdf);
        let figures = objects.figures();
        let left = figures.iter().find(|(alt, _)| *alt == "left").unwrap().1;
        let right = figures.iter().find(|(alt, _)| *alt == "right").unwrap().1;
        assert!(
            left[2] < right[0],
            "adjacent cells overlap: {left:?}, {right:?}"
        );
    }
}

#[test]
fn prefixed_indented_and_whitespace_free_images_keep_their_real_measure() {
    for source in [
        "> 1. Before [![nested](image)](https://example.com/image) after.",
        "Term\n: Before [![definition](image)](https://example.com/image) after.",
        "prefix[![adjacent](image)](https://example.com/image)suffix",
        "[![first](image)](https://example.com/image)[![second](image)](https://example.com/image)",
    ] {
        let count = source.matches("![").count();
        verify(source, &options(144.0, 300.0, png(1200, 20)), count, 60.0);
    }
}

#[test]
fn repeated_assets_keep_independent_context_sizes_and_emission_parity() {
    let source = "Before [![prose](image)](https://example.com/image) after.\n\n| Left | Right |\n|---|---|\n| [![left](image)](https://example.com/image) | [![right](image)](https://example.com/image) |\n";
    let options = options(400.0, 260.0, png(1200, 20));
    verify(source, &options, 3, 60.0);
    let document = parse_markdown(source);
    let chunked =
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::default()).unwrap();
    let monolithic = render_pdf_document_emitted(
        &document,
        &options,
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(chunked, monolithic);
    assert_eq!(chunked, render_pdf(source, &options).unwrap());
    let objects = Objects::new(&chunked);
    let figures = objects.figures();
    let width = |name| {
        let rect = figures.iter().find(|(alt, _)| *alt == name).unwrap().1;
        rect[2] - rect[0]
    };
    assert!(
        width("prose") > width("right") + 10.0,
        "one asset must retain context-specific widths"
    );
    assert_eq!(
        (1..objects.offsets.len())
            .filter(|&id| objects.dictionary(id).contains("/Subtype /Image"))
            .count(),
        1
    );
}

#[test]
fn ordinary_images_retain_their_natural_size() {
    let source = "Before [![small](image)](https://example.com/image) after.";
    let options = options(300.0, 220.0, png(16, 16));
    verify(source, &options, 1, 1.0);
    let pdf = render_pdf(source, &options).unwrap();
    let rect = Objects::new(&pdf).figures()[0].1;
    assert!((rect[2] - rect[0] - 12.0).abs() < 0.025);
    assert!((rect[3] - rect[1] - 12.0).abs() < 0.025);
}

#[test]
fn footnote_images_fit_their_smaller_text_and_page_measure() {
    let source =
        "Claim[^note].\n\n[^note]: Before [![note](image)](https://example.com/image) after.\n";
    verify(source, &options(160.0, 300.0, svg(1200, 20)), 1, 60.0);
}

#[test]
fn fitted_table_images_survive_page_breaks_and_both_emitters() {
    let mut source = String::from("| Left | Right |\n|---|---|\n");
    for index in 0..12 {
        source.push_str(&format!(
            "| [![left{index}](image)](https://example.com/image) | [![right{index}](image)](https://example.com/image) |\n"
        ));
    }
    let options = options(220.0, 160.0, png(1200, 20));
    verify(&source, &options, 24, 60.0);
    let document = parse_markdown(&source);
    let chunked = render_pdf(&source, &options).unwrap();
    assert!(Objects::new(&chunked).pages().len() > 1);
    let monolithic = render_pdf_document_emitted(
        &document,
        &options,
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(chunked, monolithic);
}
