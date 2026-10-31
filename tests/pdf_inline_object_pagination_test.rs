//! Inline objects retain their source, painted geometry and ownership when
//! actual paragraph/page alternatives are selected.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    BaselineGrid, PdfEmitOptions, PdfImageAsset, PdfOptions, PdfParagraphVariantStatus,
    parse_markdown, render_pdf, render_pdf_document_reported, render_pdf_reported,
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
        for line in content
            .lines()
            .filter(|line| line.contains("BT /F") && !line.contains("3 Tr"))
        {
            let fields: Vec<_> = line.split_whitespace().collect();
            for (index, field) in fields.iter().enumerate() {
                if *field == "Tf" {
                    let size: f32 = fields[index - 1].parse().unwrap();
                    assert_eq!(size, expected, "object optimization cannot shrink text");
                    count += 1;
                }
            }
        }
    }
    assert!(count > 0);
}

type Rect = [f64; 4];
type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

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

fn tagged_boxes(pdf: &[u8], tag: &str) -> Vec<Rect> {
    let objects = Objects::new(pdf);
    let key = format!("/Type /StructElem /S /{tag} ");
    (1..objects.offsets.len())
        .filter_map(|id| {
            let dictionary = objects.dictionary(id);
            dictionary
                .contains(&key)
                .then(|| rectangle(dictionary, "/BBox ["))
        })
        .collect()
}

fn link_boxes(pdf: &[u8], destination: &str) -> Vec<Rect> {
    let objects = Objects::new(pdf);
    let key = format!("/URI ({destination})");
    (1..objects.offsets.len())
        .filter_map(|id| {
            let dictionary = objects.dictionary(id);
            dictionary
                .contains(&key)
                .then(|| rectangle(dictionary, "/Rect ["))
        })
        .collect()
}

fn object_ancestry(pdf: &[u8], tag: &str) -> Vec<Vec<String>> {
    let objects = Objects::new(pdf);
    let key = format!("/Type /StructElem /S /{tag} ");
    let mut result = Vec::new();
    for id in 1..objects.offsets.len() {
        let dictionary = objects.dictionary(id);
        if !dictionary.contains(&key) {
            continue;
        }
        let mut parent = integer_after(dictionary, "/P ");
        let mut ancestry = Vec::new();
        while parent != 0 {
            let dictionary = objects.dictionary(parent);
            let tag = dictionary
                .split_once("/S /")
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap();
            ancestry.push(tag.to_string());
            if tag == "Document" {
                break;
            }
            parent = integer_after(dictionary, "/P ");
            assert!(
                ancestry.len() < objects.offsets.len(),
                "acyclic object ancestry"
            );
        }
        result.push(ancestry);
    }
    result
}

// Inline math emits absolute paths. The control polygon conservatively bounds
// every Bezier, independently of the producer's formula metrics and /BBox.
fn formula_paint(pdf: &[u8]) -> Vec<Rect> {
    let mut result = Vec::new();
    for stream in body_streams(pdf) {
        for part in stream.split("/Formula <</MCID ").skip(1) {
            let outlines = part.split_once("/ActualText ").unwrap().0;
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            let mut points = 0;
            for line in outlines.lines() {
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
            assert!(points > 10, "formula must paint real vector outlines");
            result.push(bounds);
        }
    }
    result
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
    let points = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
        .map(|(x, y)| (a * x + c * y + e, b * x + d * y + f));
    [
        points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max),
        points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max),
    ]
}

// Fixtures use either a raster Do or a filled SVG rectangle. Follow the actual
// drawing matrices instead of trusting only the Figure's tagged box.
fn image_paint(pdf: &[u8]) -> Vec<Rect> {
    let mut result = Vec::new();
    for stream in body_streams(pdf) {
        for part in stream.split("/Figure <</MCID ").skip(1) {
            let body = part.split_once("EMC").unwrap().0;
            let fields: Vec<_> = body.split_whitespace().collect();
            let mut matrix = IDENTITY;
            let mut stack = Vec::new();
            for (index, field) in fields.iter().enumerate() {
                match *field {
                    "q" => stack.push(matrix),
                    "Q" => matrix = stack.pop().unwrap(),
                    "cm" => {
                        let next = std::array::from_fn(|offset| {
                            fields[index - 6 + offset].parse().unwrap()
                        });
                        matrix = multiply(matrix, next);
                    }
                    "Do" if fields[index - 1].starts_with("/Im") => {
                        result.push(transformed_rect(matrix, [0.0, 0.0, 1.0, 1.0]))
                    }
                    "re" if fields.get(index + 1) == Some(&"f") => {
                        let rect = std::array::from_fn(|offset| {
                            fields[index - 4 + offset].parse().unwrap()
                        });
                        result.push(transformed_rect(matrix, rect));
                    }
                    _ => {}
                }
            }
            assert!(stack.is_empty());
        }
    }
    result
}

fn inside(ink: Rect, frame: Rect) {
    assert!(
        ink[2] > ink[0] && ink[3] > ink[1],
        "nonempty painted object: {ink:?}"
    );
    assert!(
        ink[0] >= frame[0] - 0.025
            && ink[1] >= frame[1] - 0.025
            && ink[2] <= frame[2] + 0.025
            && ink[3] <= frame[3] + 0.025,
        "painted object {ink:?} must fit {frame:?}"
    );
}

fn same_dimensions(before: &[Rect], after: &[Rect]) {
    assert_eq!(
        before.len(),
        after.len(),
        "retain every object exactly once"
    );
    for (before, after) in before.iter().zip(after) {
        for axis in 0..2 {
            let old = before[axis + 2] - before[axis];
            let new = after[axis + 2] - after[axis];
            assert!(
                (old - new).abs() < 0.025,
                "object must keep its scale: {before:?} -> {after:?}"
            );
        }
    }
}

fn literal(text: &str) -> String {
    format!(
        "({})",
        text.replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
    )
}

fn assert_formula_sources(pdf: &[u8], sources: &[&str]) {
    let streams = body_streams(pdf).concat();
    let objects = Objects::new(pdf);
    let actual: Vec<_> = (1..objects.offsets.len())
        .map(|id| objects.dictionary(id))
        .filter(|dictionary| dictionary.contains("/Type /StructElem /S /Formula "))
        .collect();
    assert_eq!(actual.len(), sources.len());
    assert_eq!(streams.matches("/ActualText ").count(), sources.len());
    for (dictionary, source) in actual.into_iter().zip(sources) {
        let source = literal(source);
        assert!(
            dictionary.contains(&format!("/Alt {source}")),
            "complete tagged math source: {dictionary}"
        );
        assert!(
            streams.contains(&format!("/ActualText {source}")),
            "complete selectable math source: {source}"
        );
    }
}

fn badge() -> PdfImageAsset {
    PdfImageAsset::new("image", br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#125abc"/></svg>"##.to_vec())
}

fn raster_badge() -> PdfImageAsset {
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
    let mut header = 20u32.to_be_bytes().to_vec();
    header.extend_from_slice(&20u32.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut bytes, b"IHDR", &header);
    let row: Vec<u8> = std::iter::once(0)
        .chain([18, 90, 188].into_iter().cycle().take(60))
        .collect();
    chunk(
        &mut bytes,
        b"IDAT",
        &franken_markdown::compress::zlib_compress(&row.repeat(20)),
    );
    chunk(&mut bytes, b"IEND", &[]);
    PdfImageAsset::new("image", bytes)
}

fn verify_geometry(before: &[u8], after: &[u8], opts: &PdfOptions) {
    let frame = [
        36.0,
        36.0,
        f64::from(opts.theme.page.size.width_pt - 36.0),
        f64::from(opts.theme.page.size.height_pt - 36.0),
    ];
    for (paint, boxes) in [
        (formula_paint(after), tagged_boxes(after, "Formula")),
        (image_paint(after), tagged_boxes(after, "Figure")),
    ] {
        assert_eq!(paint.len(), boxes.len());
        for (ink, tagged) in paint.into_iter().zip(boxes) {
            inside(ink, frame);
            inside(ink, tagged);
        }
    }
    same_dimensions(&formula_paint(before), &formula_paint(after));
    same_dimensions(&image_paint(before), &image_paint(after));
    let image_ink = image_paint(after);
    let links = link_boxes(after, "https://example.com/image");
    assert_eq!(links.len(), image_ink.len());
    for (ink, link) in image_ink.into_iter().zip(links) {
        inside(ink, link);
        inside(link, ink);
    }
}

#[test]
fn inline_math_can_select_a_shorter_shape_without_shrinking_the_formula() {
    let source = format!("{} $x$", prose());
    let opts = options(440.0, 196.0);
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
    assert_eq!(prose_pages(&after.bytes).concat(), compact(&prose()));
    assert_body_size(&after.bytes, 11.0);
    assert_formula_sources(&after.bytes, &["x"]);
    verify_geometry(&before, &after.bytes, &opts);
    assert_eq!(
        object_ancestry(&after.bytes, "Formula"),
        vec![vec!["P", "Document"]]
    );
    let mono = render_pdf_document_reported(
        &parse_markdown(&source),
        &opts,
        PdfEmitOptions::monolithic(),
    )
    .unwrap();
    assert_eq!(after, mono);
}

#[test]
fn linked_inline_image_can_select_a_shorter_shape_and_keep_its_paint_and_link() {
    let source = format!("[![badge](image)](https://example.com/image) {}", prose());
    for asset in [badge(), raster_badge()] {
        let raster = asset.bytes.starts_with(b"\x89PNG");
        let mut opts = options(440.0, 198.0);
        opts.image_assets = vec![asset];
        let mut original = opts.clone();
        original.optimal_pagination = false;
        let before = render_pdf(&source, &original).unwrap();
        let after = render_pdf_reported(&source, &opts).unwrap();
        assert!(after.pagination.page_breaks.is_exact());
        assert_eq!(after.pagination.paragraph_variants.selected_alternatives, 1);
        assert_eq!(painted_pages(&before).len(), 2);
        assert_eq!(painted_pages(&after.bytes).len(), 1);
        assert_eq!(prose_pages(&after.bytes).concat(), compact(&prose()));
        assert_body_size(&after.bytes, 11.0);
        assert_eq!(image_paint(&after.bytes).len(), 1);
        assert!(String::from_utf8_lossy(&after.bytes).contains("/Alt (badge)"));
        verify_geometry(&before, &after.bytes, &opts);
        assert_eq!(
            object_ancestry(&after.bytes, "Figure"),
            vec![vec!["P", "Document"]]
        );
        if raster {
            assert!(String::from_utf8_lossy(&after.bytes).contains("/Subtype /Image"));
            assert!(body_streams(&after.bytes).concat().contains(" Do"));
        }
    }
}

#[test]
fn nested_list_and_quote_keep_selected_formula_and_image_ownership() {
    let source = format!(
        "> - {} [$x$](https://example.com/formula) [![badge](image)](https://example.com/image)",
        prose()
    );
    let mut opts = options(482.0, 202.0);
    opts.image_assets = vec![badge()];
    opts.strict_pagination = true;
    let mut original = opts.clone();
    original.strict_pagination = false;
    original.optimal_pagination = false;
    let before = render_pdf(&source, &original).unwrap();
    let after = render_pdf_reported(&source, &opts).unwrap();
    assert!(after.pagination.page_breaks.is_exact());
    assert_eq!(painted_pages(&before).len(), 2);
    assert_eq!(painted_pages(&after.bytes).len(), 1);
    assert_eq!(after.pagination.paragraph_variants.selected_alternatives, 1);
    assert_eq!(
        prose_pages(&after.bytes).concat(),
        format!("•{}", compact(&prose()))
    );
    assert_body_size(&after.bytes, 11.0);
    assert_formula_sources(&after.bytes, &["x"]);
    verify_geometry(&before, &after.bytes, &opts);
    let expected = vec![vec!["P", "LBody", "LI", "L", "BlockQuote", "Document"]];
    assert_eq!(object_ancestry(&after.bytes, "Formula"), expected);
    assert_eq!(object_ancestry(&after.bytes, "Figure"), expected);
    for ink in formula_paint(&after.bytes)
        .into_iter()
        .chain(image_paint(&after.bytes))
    {
        inside(
            ink,
            [
                73.0,
                36.0,
                f64::from(opts.theme.page.size.width_pt - 36.0),
                f64::from(opts.theme.page.size.height_pt - 36.0),
            ],
        );
    }
    assert_eq!(
        link_boxes(&after.bytes, "https://example.com/formula").len(),
        1
    );
}

#[test]
fn selected_mixed_height_objects_retain_grid_and_numbered_note_placement() {
    let source = format!(
        "[$\\frac{{a+b}}{{c+d}}$](https://example.com/formula) [![badge](image)](https://example.com/image) {}[^one]\n\n[^one]: Citation retained.\n",
        prose()
    );
    let mut opts = options(440.0, 452.0);
    opts.image_assets = vec![badge()];
    opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
    opts.strict_pagination = true;
    let mut original = opts.clone();
    original.strict_pagination = false;
    original.optimal_pagination = false;
    let before = render_pdf(&source, &original).unwrap();
    let after = render_pdf_reported(&source, &opts).unwrap();
    assert!(after.pagination.page_breaks.is_exact());
    assert_eq!(painted_pages(&before).len(), 2);
    assert_eq!(painted_pages(&after.bytes).len(), 1);
    assert_eq!(after.pagination.paragraph_variants.selected_alternatives, 1);
    let visible = prose_pages(&after.bytes).concat();
    assert!(visible.contains(&compact(&prose())));
    assert_eq!(visible.matches("Citationretained.").count(), 1);
    assert_formula_sources(&after.bytes, &["\\frac{a+b}{c+d}"]);
    verify_geometry(&before, &after.bytes, &opts);
    let top = opts.theme.page.size.height_pt - 36.0;
    let mut count = 0;
    for stream in body_streams(&after.bytes) {
        for line in stream
            .lines()
            .filter(|line| line.contains(" Tm ") && !line.contains("3 Tr"))
        {
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
                "selected body baseline remains on-grid: {line}"
            );
            count += 1;
        }
    }
    assert!(count >= 12);
    let mono = render_pdf_document_reported(
        &parse_markdown(&source),
        &opts,
        PdfEmitOptions::monolithic(),
    )
    .unwrap();
    assert_eq!(after, mono);
}

#[test]
fn inline_display_formula_keeps_display_ink_when_a_shorter_shape_is_selected() {
    let formula = r"\sum_{i=1}^{n} i";
    let source = prose().replacen("The ", &format!("The $${formula}$$ "), 1);
    let opts = options(440.0, 214.0);
    let mut original = opts.clone();
    original.optimal_pagination = false;
    let before = render_pdf(&source, &original).unwrap();
    let after = render_pdf_reported(&source, &opts).unwrap();
    assert!(after.pagination.page_breaks.is_exact());
    assert_eq!(painted_pages(&before).len(), 2);
    assert_eq!(painted_pages(&after.bytes).len(), 1);
    assert_eq!(after.pagination.paragraph_variants.selected_alternatives, 1);
    assert_eq!(prose_pages(&after.bytes).concat(), compact(&prose()));
    assert_body_size(&after.bytes, 11.0);
    assert_formula_sources(&after.bytes, &[formula]);
    verify_geometry(&before, &after.bytes, &opts);
    let ink = formula_paint(&after.bytes);
    assert_eq!(ink.len(), 1);
    assert!(
        ink[0][3] - ink[0][1] > 20.0,
        "display limits retain tall geometry"
    );
}

#[test]
fn selected_wide_formula_keeps_its_existing_containing_width_cap() {
    let formula = (0..40)
        .map(|index| format!("x_{{{index}}}"))
        .collect::<Vec<_>>()
        .join("+");
    let source = format!("${formula}$ {}", prose());
    let opts = options(440.0, 212.0);
    let mut original = opts.clone();
    original.optimal_pagination = false;
    let before = render_pdf(&source, &original).unwrap();
    let after = render_pdf_reported(&source, &opts).unwrap();
    assert!(after.pagination.page_breaks.is_exact());
    assert_eq!(painted_pages(&before).len(), 2);
    assert_eq!(painted_pages(&after.bytes).len(), 1);
    assert_eq!(after.pagination.paragraph_variants.selected_alternatives, 1);
    assert_eq!(prose_pages(&after.bytes).concat(), compact(&prose()));
    assert_body_size(&after.bytes, 11.0);
    assert_formula_sources(&after.bytes, &[&formula]);
    verify_geometry(&before, &after.bytes, &opts);
    let ink = formula_paint(&after.bytes);
    assert_eq!(ink.len(), 1);
    assert!(
        ink[0][2] - ink[0][0] > 400.0,
        "fitted source still occupies its original measure"
    );
}

#[test]
fn object_paragraphs_keep_their_baseline_shape_when_glyph_expansion_is_enabled() {
    for (source, formula) in [
        (
            format!("[![badge](image)](https://example.com/image) {}", prose()),
            None,
        ),
        (format!("$x$ {}", prose()), Some("x")),
        (prose().replacen("The ", "The $$x$$ ", 1), Some("x")),
    ] {
        let mut opts = options(440.0, 1000.0);
        opts.image_assets = vec![badge()];
        opts.microtype = franken_markdown::layout::MicrotypeOptions::CONSERVATIVE;
        opts.strict_pagination = true;
        let mut original = opts.clone();
        original.optimal_pagination = false;
        original.strict_pagination = false;
        let before = render_pdf(&source, &original).unwrap();
        let after = render_pdf_reported(&source, &opts).unwrap();
        assert!(after.pagination.page_breaks.is_exact());
        assert_eq!(after.pagination.paragraph_variants.attempted_paragraphs, 0);
        assert_eq!(after.pagination.paragraph_variants.offered_paragraphs, 0);
        assert_eq!(after.pagination.paragraph_variants.selected_alternatives, 0);
        assert_eq!(prose_pages(&after.bytes).concat(), compact(&prose()));
        assert_body_size(&after.bytes, 11.0);
        if let Some(formula) = formula {
            assert_formula_sources(&after.bytes, &[formula]);
        }
        verify_geometry(&before, &after.bytes, &opts);
    }
}
