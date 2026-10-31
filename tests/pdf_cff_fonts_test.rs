//! Public PDF contracts for host-supplied CFF1 OpenType faces. The pinned OFL
//! fixture exercises real cubic outlines, subroutines, ligatures and Unicode;
//! these checks inspect emitted font programs and painted CIDs, not just text.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use franken_markdown::pdf::PdfEmitOptions;
use franken_markdown::text::Font;
use franken_markdown::{
    FontAssetSlot, FontAssets, PdfOptions, RenderError, parse_markdown, render_pdf,
    render_pdf_document_emitted,
};

const CFF: &[u8] = include_bytes!("fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");

fn options() -> PdfOptions {
    PdfOptions {
        font_assets: FontAssets {
            body_regular: Some(CFF.to_vec()),
            body_bold: Some(CFF.to_vec()),
            body_italic: Some(CFF.to_vec()),
            body_bold_italic: Some(CFF.to_vec()),
            mono_regular: Some(CFF.to_vec()),
            ..FontAssets::default()
        },
        page_numbers: false,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

/// Use xref offsets so an arbitrary binary font stream cannot be mistaken for
/// an object boundary. Object numbers need not match physical write order.
fn objects(pdf: &[u8]) -> BTreeMap<usize, &[u8]> {
    let marker = b"startxref\n";
    let at = pdf
        .windows(marker.len())
        .rposition(|s| s == marker)
        .unwrap();
    let xref: usize = std::str::from_utf8(&pdf[at + marker.len()..])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut lines = std::str::from_utf8(&pdf[xref..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let count: usize = lines
        .next()
        .unwrap()
        .strip_prefix("0 ")
        .unwrap()
        .parse()
        .unwrap();
    let mut offsets: Vec<_> = lines
        .take(count)
        .enumerate()
        .filter_map(|(id, line)| {
            let mut words = line.split_whitespace();
            let offset = words.next().unwrap().parse::<usize>().unwrap();
            let _generation = words.next().unwrap();
            (words.next() == Some("n")).then_some((offset, id))
        })
        .collect();
    offsets.sort_unstable();
    offsets
        .iter()
        .enumerate()
        .map(|(index, &(offset, id))| {
            let end = offsets.get(index + 1).map_or(xref, |entry| entry.0);
            let object = &pdf[offset..end];
            let header = format!("{id} 0 obj\n");
            assert!(object.starts_with(header.as_bytes()));
            (id, &object[header.len()..])
        })
        .collect()
}

fn dictionary(object: &[u8]) -> &str {
    let end = object.iter().position(|&byte| byte == b'\n').unwrap();
    std::str::from_utf8(&object[..end]).unwrap()
}

fn reference(dictionary: &str, key: &str) -> usize {
    dictionary
        .split_once(key)
        .unwrap()
        .1
        .trim_start_matches([' ', '['])
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn stream(object: &[u8]) -> Vec<u8> {
    let dict = dictionary(object);
    let length = reference(dict, "/Length ");
    let marker = b"\nstream\n";
    let start = object
        .windows(marker.len())
        .position(|s| s == marker)
        .unwrap()
        + marker.len();
    let bytes = &object[start..start + length];
    if dict.contains("/FlateDecode") {
        franken_markdown::zlib_decompress(bytes, 16 * 1024 * 1024).unwrap()
    } else {
        bytes.to_vec()
    }
}

fn content(objects: &BTreeMap<usize, &[u8]>) -> String {
    let mut result = String::new();
    for object in objects.values() {
        let dict = dictionary(object);
        if dict.starts_with("<< /Type /Page ") {
            let bytes = stream(objects[&reference(dict, "/Contents ")]);
            result.push_str(std::str::from_utf8(&bytes).unwrap());
        }
    }
    result
}

fn painted_cids(content: &str) -> BTreeSet<u16> {
    content
        .split("BT ")
        .skip(1)
        .flat_map(|text| {
            text.split_once(" ET")
                .unwrap()
                .0
                .split('<')
                .skip(1)
                .flat_map(|s| {
                    s.split_once('>')
                        .unwrap()
                        .0
                        .as_bytes()
                        .chunks_exact(4)
                        .map(|code| {
                            u16::from_str_radix(std::str::from_utf8(code).unwrap(), 16).unwrap()
                        })
                })
        })
        .collect()
}

fn unicode_map(cmap: &[u8]) -> BTreeMap<u16, String> {
    let text = std::str::from_utf8(cmap).unwrap();
    let mut entries = BTreeMap::new();
    for chunk in text.split("beginbfchar\n").skip(1) {
        for line in chunk.split_once("endbfchar").unwrap().0.lines() {
            let (cid, unicode) = line.split_once("> <").unwrap();
            let cid = u16::from_str_radix(cid.trim_start_matches('<'), 16).unwrap();
            let units: Vec<_> = unicode
                .trim_end_matches('>')
                .as_bytes()
                .chunks_exact(4)
                .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
                .collect();
            assert!(
                entries
                    .insert(cid, String::from_utf16(&units).unwrap())
                    .is_none()
            );
        }
    }
    entries
}

struct EmbeddedCff {
    font: Font,
    unicode: BTreeMap<u16, String>,
}

/// Follow the real PDF resource graph, checking width and bounding-box units
/// against the embedded subset, whose GIDs are the painted Identity-H CIDs.
fn cff_faces(objects: &BTreeMap<usize, &[u8]>) -> Vec<EmbeddedCff> {
    objects
        .values()
        .filter_map(|object| {
            let type0 = dictionary(object);
            if !type0.contains("/Subtype /Type0 ") {
                return None;
            }
            let cid = dictionary(objects[&reference(type0, "/DescendantFonts ")]);
            if !cid.contains("/Subtype /CIDFontType0 ") {
                return None;
            }
            assert!(type0.contains("/Encoding /Identity-H "));
            assert!(!cid.contains("/CIDToGIDMap"));
            let descriptor = dictionary(objects[&reference(cid, "/FontDescriptor ")]);
            assert!(!descriptor.contains("/FontFile2"));
            let fontfile = objects[&reference(descriptor, "/FontFile3 ")];
            assert!(dictionary(fontfile).contains("/Subtype /OpenType "));
            let bytes = stream(fontfile);
            assert_eq!(&bytes[..4], b"OTTO");
            assert!(bytes.len() < CFF.len() / 2, "host face must be subset");
            let font = Font::parse(bytes).unwrap();
            assert!(!font.has_glyf_outlines());
            let widths: Vec<u32> = cid
                .split_once("/W [0 [")
                .unwrap()
                .1
                .split_once(']')
                .unwrap()
                .0
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            assert_eq!(widths.len(), usize::from(font.num_glyphs));
            let upm = i32::from(font.units_per_em);
            for (gid, &width) in widths.iter().enumerate() {
                assert_eq!(
                    width,
                    u32::from(font.advance_width(gid as u16)) * 1000 / upm as u32
                );
            }
            let declared: Vec<i32> = descriptor
                .split_once("/FontBBox [")
                .unwrap()
                .1
                .split_once(']')
                .unwrap()
                .0
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            let bounds = font.font_bbox().unwrap();
            assert_eq!(declared.len(), 4);
            for (index, &limit) in bounds.iter().enumerate() {
                let scaled = i32::from(limit) * 1000;
                let expected = if index < 2 {
                    scaled.div_euclid(upm)
                } else {
                    (scaled + upm - 1).div_euclid(upm)
                };
                assert_eq!(declared[index], expected);
            }
            // Bounds enclose every retained cubic, including .notdef.
            for gid in 0..font.num_glyphs {
                if let Some(ink) = font.cff_glyph_bbox(gid).unwrap() {
                    assert!(bounds[0] <= ink[0] && bounds[1] <= ink[1]);
                    assert!(bounds[2] >= ink[2] && bounds[3] >= ink[3]);
                }
            }
            let unicode = unicode_map(&stream(objects[&reference(type0, "/ToUnicode ")]));
            Some(EmbeddedCff { font, unicode })
        })
        .collect()
}

#[test]
fn cff_host_fonts_render_all_five_slots_with_ligatures_and_exact_writer_parity() {
    let source = "# CFF document\n\nRegular office affine café Ελληνικά Кириллица.\n\n\
        **Bold office** and *italic café* and ***bold italic affine***.\n\n\
        `mono office` and [link café](https://example.com).\n";
    let opts = options();
    let doc = parse_markdown(source);
    let chunked = render_pdf(source, &opts).unwrap();
    let monolithic =
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap();
    assert_eq!(chunked, monolithic);
    assert_eq!(chunked, render_pdf(source, &opts).unwrap());
    let objects = objects(&chunked);
    let faces = cff_faces(&objects);
    assert_eq!(faces.len(), 5, "regular, bold, italic, bold-italic, mono");
    assert!(
        faces
            .iter()
            .all(|face| face.font.num_glyphs > 1 && !face.unicode.is_empty())
    );
    assert!(
        faces
            .iter()
            .flat_map(|face| face.unicode.values())
            .any(|text| text == "ff")
    );
    let content = content(&objects);
    assert!(painted_cids(&content).iter().any(|&gid| gid != 0));
    assert!(
        !content.contains("/F6 "),
        "supported prose must not require a symbol fallback"
    );
}

#[test]
fn cff_identity_cids_above_255_preserve_unicode_advances_and_cubic_outlines() {
    let source_font = Font::parse(CFF.to_vec()).unwrap();
    let mut source_gids = BTreeSet::new();
    let mut chars: Vec<_> = (0x100..0x700)
        .filter_map(char::from_u32)
        .filter(|&ch| {
            let gid = source_font.glyph_index(ch);
            ch.is_alphabetic() && gid != 0 && source_gids.insert(gid)
        })
        .take(300)
        .collect();
    assert_eq!(chars.len(), 300);
    chars.push('\u{1f748}'); // Supplementary-plane glyph: two UTF-16 units.
    assert_ne!(source_font.glyph_index('\u{1f748}'), 0);
    let source = chars
        .iter()
        .map(char::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let pdf = render_pdf(&source, &options()).unwrap();
    let objects = objects(&pdf);
    let faces = cff_faces(&objects);
    assert_eq!(faces.len(), 1);
    let subset = &faces[0];
    assert!(subset.font.num_glyphs > 256);
    let painted = painted_cids(&content(&objects));
    assert!(painted.iter().any(|&cid| cid > 255));
    for ch in chars {
        let source_gid = source_font.glyph_index(ch);
        let cid = subset.font.glyph_index(ch);
        assert_ne!(cid, 0, "missing U+{:04X}", u32::from(ch));
        assert!(
            painted.contains(&cid),
            "U+{:04X} was mapped but not painted",
            u32::from(ch)
        );
        assert_eq!(subset.unicode[&cid], ch.to_string());
        assert_eq!(
            subset.font.advance_width(cid),
            source_font.advance_width(source_gid)
        );
        assert_eq!(
            subset.font.cff_outline(cid).unwrap(),
            source_font.cff_outline(source_gid).unwrap()
        );
    }
}

#[test]
fn cff_body_and_bundled_truetype_styles_keep_distinct_embedding_formats() {
    let mut opts = options();
    opts.font_assets = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, CFF.to_vec())
        .unwrap();
    let pdf = render_pdf(
        "CFF body and **TrueType bold** with `TrueType code`.",
        &opts,
    )
    .unwrap();
    let objects = objects(&pdf);
    assert_eq!(cff_faces(&objects).len(), 1);
    let true_type: Vec<_> = objects
        .values()
        .map(|object| dictionary(object))
        .filter(|dict| dict.contains("/Subtype /CIDFontType2 "))
        .collect();
    assert_eq!(true_type.len(), 2);
    for cid in true_type {
        assert!(cid.contains("/CIDToGIDMap /Identity"));
        let descriptor = dictionary(objects[&reference(cid, "/FontDescriptor ")]);
        assert!(!descriptor.contains("/FontFile3"));
        let program = stream(objects[&reference(descriptor, "/FontFile2 ")]);
        assert!(Font::parse(program).unwrap().has_glyf_outlines());
    }
}

fn table_range(bytes: &[u8], tag: &[u8; 4]) -> Range<usize> {
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap());
    let record = (0..usize::from(count))
        .map(|index| 12 + index * 16)
        .find(|&at| &bytes[at..at + 4] == tag)
        .unwrap();
    let start = u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize;
    let len = u32::from_be_bytes(bytes[record + 12..record + 16].try_into().unwrap()) as usize;
    start..start + len
}

/// INDEX entry and next INDEX position, independent of offset byte width.
fn index_entry(bytes: &[u8], at: usize, index: usize) -> (Range<usize>, usize) {
    let count = usize::from(u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap()));
    assert!(index < count);
    let width = usize::from(bytes[at + 2]);
    let offset = |i: usize| {
        bytes[at + 3 + i * width..at + 3 + (i + 1) * width]
            .iter()
            .fold(0usize, |value, &byte| value * 256 + usize::from(byte))
    };
    let data = at + 3 + (count + 1) * width - 1;
    (
        data + offset(index)..data + offset(index + 1),
        data + offset(count),
    )
}

#[test]
fn malformed_used_cff_glyph_fails_explicitly_after_structural_font_acceptance() {
    // Start from the public subset API so the fixture remains a small valid
    // host font. Only A's Type2 program is changed; .notdef remains renderable.
    let original = Font::parse(CFF.to_vec()).unwrap();
    let mut bytes = original.try_subset(&['A']).unwrap().bytes;
    let font = Font::parse(bytes.clone()).unwrap();
    let gid = font.glyph_index('A');
    let table = table_range(&bytes, b"CFF ");
    let cff = &bytes[table.clone()];
    let (_, next) = index_entry(cff, usize::from(cff[2]), 0);
    let (top, _) = index_entry(cff, next, 0);
    // The deterministic subset DICT uses signed 32-bit integer operands.
    // CharStrings is operator 17 preceded by its offset operand.
    let charstrings = cff[top]
        .windows(6)
        .find_map(|window| {
            (window[0] == 29 && window[5] == 17)
                .then(|| u32::from_be_bytes(window[1..5].try_into().unwrap()) as usize)
        })
        .unwrap();
    let (glyph, _) = index_entry(cff, charstrings, usize::from(gid));
    assert!(glyph.len() >= 2);
    // Type2 random is deliberately outside the deterministic supported tier.
    let start = table.start + glyph.start;
    bytes[start..start + 2].copy_from_slice(&[12, 23]);
    let assets = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, bytes)
        .unwrap();
    let opts = PdfOptions {
        font_assets: assets,
        ..options()
    };
    for emit in [PdfEmitOptions::default(), PdfEmitOptions::monolithic()] {
        let error = render_pdf_document_emitted(&parse_markdown("A"), &opts, emit).unwrap_err();
        let RenderError::InvalidInput(message) = error else {
            panic!("CFF failure must be an explicit input error: {error}");
        };
        assert!(
            message.contains("CFF") && message.contains("UnsupportedOperator"),
            "{message}"
        );
        assert!(message.contains(&format!("glyph Some({gid})")), "{message}");
    }
}
