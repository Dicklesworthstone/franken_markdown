//! Variable metrics reach actual PDF and HTML embedded font programs.
//! Expectations come from the independent saved FontTools reference, never
//! from calling the same instancer again to manufacture the expected result.
//! Both public Rust facades run on the native test host: this is API parity,
//! not generated wasm32 or browser execution.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::text::Font;
use franken_markdown::wasm::{self, WasmRenderOptions};
use franken_markdown::{
    FontAssetSlot, FontAssets, FontEmbedding, HtmlFontFormat, HtmlOptions, PdfOptions,
    render_html, render_pdf,
};

const FIXTURES: [(&str, &[u8]); 2] = [
    (
        "FmdMetricsVF",
        include_bytes!("../fmd-font/fonts/test-variable/FmdMetricsVF.ttf"),
    ),
    (
        "FmdMappedMetricsVF",
        include_bytes!("../fmd-font/fonts/test-variable/FmdMappedMetricsVF.ttf"),
    ),
];
const REFERENCE: &str =
    include_str!("../fmd-font/fonts/test-variable/metric-variations-reference.tsv");
const TEXT: &str = "AB AB";
const EPOCH: u64 = 1_700_000_000;

fn assets(bytes: &[u8], weight: u16) -> FontAssets {
    FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, bytes.to_vec())
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyRegular, weight)
        .unwrap()
}

fn oracle(name: &str, weight: u16, table: &str, row: &str, column: usize) -> i32 {
    let weight = weight.to_string();
    REFERENCE
        .lines()
        .find_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() == 6
                && fields[0] == name
                && fields[1] == weight
                && fields[2] == table
                && fields[3] == row
            {
                Some(fields[column].parse().unwrap())
            } else {
                None
            }
        })
        .expect("metric is present in the independent reference")
}

fn assert_metrics(font: &Font, original: &Font, name: &str, weight: u16, space: bool) {
    assert!(font.axes().is_empty(), "embedded font must be a static instance");
    assert_eq!(font.units_per_em, 1000, "fixture units");
    assert_eq!(
        [i32::from(font.ascent), i32::from(font.descent), i32::from(font.line_gap)],
        [
            oracle(name, weight, "hhea", "4", 5),
            oracle(name, weight, "hhea", "6", 5),
            oracle(name, weight, "hhea", "8", 5),
        ],
        "{name} weight {weight}: MVAR face metrics",
    );
    for ch in ['A', 'B'].into_iter().chain(space.then_some(' ')) {
        let original_gid = original.glyph_index(ch);
        assert_ne!(original_gid, 0);
        let gid = font.glyph_index(ch);
        assert_ne!(gid, 0, "{name} weight {weight}: missing {ch:?}");
        let row = original_gid.to_string();
        assert_eq!(
            [i32::from(font.advance_width(gid)), i32::from(font.left_side_bearing(gid))],
            [
                oracle(name, weight, "hmtx", &row, 4),
                oracle(name, weight, "hmtx", &row, 5),
            ],
            "{name} weight {weight}: HVAR metrics for {ch:?}",
        );
    }
}

// Follow real xref offsets. Binary font bytes must never be split on apparent
// obj/endobj delimiters, and physical object order need not equal object IDs.
fn objects(pdf: &[u8]) -> BTreeMap<usize, &[u8]> {
    let marker = b"startxref\n";
    let at = pdf.windows(marker.len()).rposition(|s| s == marker).unwrap();
    let xref: usize = std::str::from_utf8(&pdf[at + marker.len()..])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut lines = std::str::from_utf8(&pdf[xref..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let count: usize = lines.next().unwrap().strip_prefix("0 ").unwrap().parse().unwrap();
    let mut offsets: Vec<_> = lines
        .take(count)
        .enumerate()
        .filter_map(|(id, line)| {
            let mut words = line.split_whitespace();
            let offset = words.next().unwrap().parse::<usize>().unwrap();
            assert_eq!(words.next(), Some(if id == 0 { "65535" } else { "00000" }));
            assert_eq!(words.next(), Some(if id == 0 { "f" } else { "n" }));
            (id != 0).then_some((offset, id))
        })
        .collect();
    assert_eq!(offsets.len(), count - 1);
    offsets.sort_unstable();
    assert!(offsets.windows(2).all(|pair| pair[0].0 < pair[1].0));
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

fn field<'a>(dict: &'a str, key: &str) -> &'a str {
    dict.split_once(key)
        .unwrap()
        .1
        .trim_start_matches([' ', '['])
        .split_whitespace()
        .next()
        .unwrap()
}

fn reference(dict: &str, key: &str) -> usize {
    field(dict, key).parse().unwrap()
}

fn stream(object: &[u8]) -> Vec<u8> {
    let dict = dictionary(object);
    let marker = b"\nstream\n";
    let start = object.windows(marker.len()).position(|s| s == marker).unwrap()
        + marker.len();
    let bytes = &object[start..start + reference(dict, "/Length ")];
    if dict.contains("/FlateDecode") {
        franken_markdown::zlib_decompress(bytes, 16 * 1024 * 1024).unwrap()
    } else {
        bytes.to_vec()
    }
}

#[test]
fn native_and_wasm_safe_pdf_apis_embed_varied_cid_widths_and_face_metrics() {
    for (name, bytes) in FIXTURES {
        let original = Font::parse(bytes.to_vec()).unwrap();
        for weight in [100, 650, 900] {
            let assets = assets(bytes, weight);
            let options = PdfOptions {
                font_assets: assets.clone(),
                metadata_epoch_seconds: Some(EPOCH),
                ..Default::default()
            };
            let pdf = render_pdf(TEXT, &options).unwrap();
            let host = wasm::render_pdf(
                TEXT,
                &WasmRenderOptions {
                    font_assets: assets,
                    metadata_epoch_seconds: Some(EPOCH),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(pdf, host.bytes, "{name} weight {weight}: native API parity");

            let objects = objects(&pdf);
            let page = objects
                .values()
                .map(|object| dictionary(object))
                .find(|dict| dict.starts_with("<< /Type /Page "))
                .unwrap();
            let type0 = dictionary(objects[&reference(page, "/F1 ")]);
            assert!(type0.contains("/Subtype /Type0 "));
            let cid = dictionary(objects[&reference(type0, "/DescendantFonts ")]);
            assert!(cid.contains("/Subtype /CIDFontType2 "));
            assert!(cid.contains("/CIDToGIDMap /Identity"));
            let descriptor = dictionary(objects[&reference(cid, "/FontDescriptor ")]);
            let program = stream(objects[&reference(descriptor, "/FontFile2 ")]);
            let font = Font::parse(program).unwrap();
            // PDF can paint interword space as a positioning adjustment, so
            // the subset is not required to retain a space cmap entry.
            assert_metrics(&font, &original, name, weight, false);
            for (key, offset) in [("/Ascent ", "4"), ("/Descent ", "6")] {
                assert_eq!(
                    field(descriptor, key).parse::<i32>().unwrap(),
                    oracle(name, weight, "hhea", offset, 5),
                    "{name} weight {weight}: PDF descriptor {key}",
                );
            }
            let widths: Vec<u32> = cid
                .split_once("/W [0 [")
                .unwrap()
                .1
                .split_once(']')
                .unwrap()
                .0
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            assert_eq!(widths.len(), usize::from(font.num_glyphs));
            for ch in ['A', 'B'] {
                let gid = usize::from(font.glyph_index(ch));
                let row = original.glyph_index(ch).to_string();
                assert_eq!(
                    i32::try_from(widths[gid]).unwrap(),
                    oracle(name, weight, "hmtx", &row, 4),
                    "{name} weight {weight}: CID width for {ch}",
                );
            }
        }
    }
}

fn decode_base64(encoded: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut value = 0u32;
    let mut bits = 0;
    for ch in encoded.bytes().take_while(|&ch| ch != b'=') {
        let digit = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("unexpected base64 byte"),
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((value >> bits) as u8);
        }
    }
    bytes
}

#[test]
fn html_subset_and_full_fonts_preserve_variable_metrics_through_both_host_apis() {
    for (name, bytes) in FIXTURES {
        let original = Font::parse(bytes.to_vec()).unwrap();
        for weight in [100, 650, 900] {
            for embedding in [FontEmbedding::Subset, FontEmbedding::Full] {
                let assets = assets(bytes, weight);
                let html = render_html(
                    TEXT,
                    &HtmlOptions {
                        font_assets: assets.clone(),
                        html_font_format: HtmlFontFormat::Ttf,
                        font_embedding: embedding,
                        ..Default::default()
                    },
                )
                .unwrap();
                let host = wasm::render_html(
                    TEXT,
                    &WasmRenderOptions {
                        font_assets: assets,
                        html_font_format: HtmlFontFormat::Ttf,
                        font_embedding: embedding,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(host.html(), Some(html.as_str()), "{name} weight {weight}");
                let payloads: Vec<_> = html
                    .split("data:font/ttf;base64,")
                    .skip(1)
                    .map(|part| decode_base64(part.split('"').next().unwrap()))
                    .collect();
                assert_eq!(payloads.len(), 1, "only the regular face is used");
                let font = Font::parse(payloads.into_iter().next().unwrap()).unwrap();
                assert_metrics(&font, &original, name, weight, embedding == FontEmbedding::Full);
            }
        }
    }
}
