//! Full embedding preserves the real font's cmap and layout program for the
//! browser, while compact subsets remain the default publication contract.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::text::Font;
use franken_markdown::{
    FontAssetSlot, FontAssets, FontEmbedding, FontFamily, HtmlFontFormat, HtmlOptions, RenderError,
    render_html,
};

const SOURCE_SANS: &[u8] = include_bytes!("fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");
const TEXT: &str = "office AVATAR z\u{0301}\u{0307}";

fn options(bytes: &[u8], format: HtmlFontFormat) -> HtmlOptions {
    HtmlOptions {
        font_assets: FontAssets::default()
            .with_slot(FontAssetSlot::BodyRegular, bytes.to_vec())
            .unwrap(),
        font_embedding: FontEmbedding::Full,
        html_font_format: format,
        ..HtmlOptions::default()
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

fn payloads(html: &str, mime: &str) -> Vec<Vec<u8>> {
    html.split(&format!("data:{mime};base64,"))
        .skip(1)
        .map(|part| decode_base64(part.split('"').next().unwrap()))
        .collect()
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn sfnt_tables(bytes: &[u8]) -> BTreeMap<[u8; 4], Vec<u8>> {
    (0..usize::from(u16_at(bytes, 4)))
        .map(|index| {
            let at = 12 + index * 16;
            let start = u32_at(bytes, at + 8) as usize;
            let len = u32_at(bytes, at + 12) as usize;
            (
                bytes[at..at + 4].try_into().unwrap(),
                bytes[start..start + len].to_vec(),
            )
        })
        .collect()
}

fn woff1_tables(bytes: &[u8]) -> BTreeMap<[u8; 4], Vec<u8>> {
    assert_eq!(&bytes[..4], b"wOFF");
    assert_eq!(u32_at(bytes, 8) as usize, bytes.len());
    (0..usize::from(u16_at(bytes, 12)))
        .map(|index| {
            let at = 44 + index * 20;
            let start = u32_at(bytes, at + 4) as usize;
            let stored_len = u32_at(bytes, at + 8) as usize;
            let raw_len = u32_at(bytes, at + 12) as usize;
            let payload = &bytes[start..start + stored_len];
            let table = if stored_len < raw_len {
                franken_markdown::zlib_decompress(payload, raw_len).unwrap()
            } else {
                assert_eq!(stored_len, raw_len);
                payload.to_vec()
            };
            assert_eq!(table.len(), raw_len);
            (bytes[at..at + 4].try_into().unwrap(), table)
        })
        .collect()
}

fn assert_layout_program(actual: &[u8], original: &[u8]) {
    // These real faces have nontrivial cmap, substitution, positioning and
    // glyph-class data; merely retaining requested Unicode glyphs is not enough.
    let original_tables = sfnt_tables(original);
    let actual_tables = sfnt_tables(actual);
    for tag in [*b"cmap", *b"GSUB", *b"GPOS", *b"GDEF"] {
        assert!(!original_tables[&tag].is_empty(), "fixture {tag:?}");
        assert_eq!(actual_tables[&tag], original_tables[&tag], "{tag:?}");
    }
    let original_font = Font::parse(original.to_vec()).unwrap();
    let actual_font = Font::parse(actual.to_vec()).unwrap();
    assert_eq!(actual_font.num_glyphs, original_font.num_glyphs);
    // Not present in TEXT: canonical normalization and later browser content
    // must still be able to select these original cmap glyph identities.
    for ch in ['é', 'ź', 'Ω', 'Ж'] {
        let original_gid = original_font.glyph_index(ch);
        assert_ne!(original_gid, 0, "fixture U+{:04X}", u32::from(ch));
        assert_eq!(actual_font.glyph_index(ch), original_gid);
    }
}

#[test]
fn full_static_faces_preserve_the_complete_program_in_every_semantic_slot() {
    let document = format!("{TEXT} **{TEXT}** *{TEXT}* ***{TEXT}*** `{TEXT}`");
    for (source, mime, hint) in [
        (SOURCE_SANS, "font/otf", "opentype"),
        (
            fonts::body_bytes(FontFamily::Sans, FontStyle::Regular),
            "font/ttf",
            "truetype",
        ),
    ] {
        let mut opts = options(source, HtmlFontFormat::Ttf);
        for slot in FontAssetSlot::ALL {
            opts.font_assets.set_slot(slot, source.to_vec()).unwrap();
        }
        let assets = opts.font_assets.clone();
        let html = render_html(&document, &opts).unwrap();
        let fonts = payloads(&html, mime);
        assert_eq!(fonts.len(), 5);
        assert_eq!(html.matches(&format!("format(\"{hint}\")")).count(), 5);
        for bytes in fonts {
            assert_eq!(bytes, source);
            assert_layout_program(&bytes, source);
        }
        assert_eq!(html, render_html(&document, &opts).unwrap());
        assert_eq!(opts.font_assets, assets);
    }
}

#[test]
fn full_woff_containers_preserve_every_source_table_for_both_outline_formats() {
    for source in [
        SOURCE_SANS,
        fonts::body_bytes(FontFamily::Sans, FontStyle::Regular),
    ] {
        let expected = sfnt_tables(source);
        for (format, mime, hint) in [
            (HtmlFontFormat::Woff1, "font/woff", "woff"),
            (HtmlFontFormat::Woff2, "font/woff2", "woff2"),
        ] {
            let opts = options(source, format);
            let html = render_html(TEXT, &opts).unwrap();
            assert_eq!(html, render_html(TEXT, &opts).unwrap());
            assert!(html.contains(&format!("format(\"{hint}\")")));
            let fonts = payloads(&html, mime);
            assert_eq!(fonts.len(), 1, "unused style slots need no font payload");
            assert_eq!(&fonts[0][4..8], &source[..4]);
            let actual = if format == HtmlFontFormat::Woff1 {
                woff1_tables(&fonts[0])
            } else {
                let bytes = franken_markdown::woff2::decode_woff2(&fonts[0]).unwrap();
                assert_layout_program(&bytes, source);
                sfnt_tables(&bytes)
            };
            assert_eq!(
                actual.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>()
            );
            for (tag, mut table) in actual {
                let mut source_table = expected[&tag].clone();
                // Container decoding can rebuild sfnt offsets/checksums.
                if &tag == b"head" {
                    table[8..12].fill(0);
                    source_table[8..12].fill(0);
                }
                assert_eq!(table, source_table, "{format:?} table {tag:?}");
            }
        }
    }
}

#[test]
fn full_variable_faces_use_the_existing_pinned_static_instances() {
    let source = franken_markdown::text::variable_triangle_fixture();
    let parsed = Font::parse(source.clone()).unwrap();
    let mut opts = options(&source, HtmlFontFormat::Ttf);
    opts.font_assets
        .set_slot_weight(FontAssetSlot::BodyRegular, 300)
        .unwrap();
    opts.font_assets
        .set_slot_weight(FontAssetSlot::BodyBold, 800)
        .unwrap();
    let assets = opts.font_assets.clone();
    let html = render_html("A **A**", &opts).unwrap();
    let fonts = payloads(&html, "font/ttf");
    assert_eq!(fonts.len(), 2);
    assert_ne!(fonts[0], fonts[1]);
    for (bytes, weight) in fonts.iter().zip([300., 800.]) {
        assert_eq!(bytes, parsed.instance(weight).unwrap().as_sfnt());
        assert!(
            Font::parse(bytes.clone())
                .unwrap()
                .instance_bounds(*b"wght")
                .is_none()
        );
    }
    assert_eq!(opts.font_assets, assets);
}

#[test]
fn full_embedding_delegates_used_glyph_programs_but_keeps_structural_admission() {
    // Independently located by FontTools in the pinned fixture: introduce an
    // operation outside the compact subsetter's deterministic Type2 tier.
    const OMEGA: usize = 106_951;
    assert_eq!(&SOURCE_SANS[OMEGA..OMEGA + 4], &[0xf7, 0x34, 0x8b, 0xcf]);
    let mut source = SOURCE_SANS.to_vec();
    source[OMEGA..OMEGA + 2].copy_from_slice(&[12, 23]);
    let opts = options(&source, HtmlFontFormat::Ttf);
    let mut compact = opts.clone();
    compact.font_embedding = FontEmbedding::Subset;
    let error = render_html("Ω", &compact).unwrap_err();
    assert!(error.to_string().contains("UnsupportedOperator"));
    let html = render_html("Ω", &opts).unwrap();
    assert_eq!(payloads(&html, "font/otf"), [source]);

    for invalid in [Vec::new(), b"not a font".to_vec()] {
        let mut invalid_opts = opts.clone();
        invalid_opts.font_assets.body_regular = Some(invalid);
        assert!(matches!(
            render_html("A", &invalid_opts),
            Err(RenderError::InvalidInput(_))
        ));
    }
}

#[test]
fn subset_is_still_default_and_custom_css_still_replaces_embedded_faces() {
    let mut opts = options(SOURCE_SANS, HtmlFontFormat::Ttf);
    opts.font_embedding = FontEmbedding::default();
    assert_eq!(opts.font_embedding, FontEmbedding::Subset);
    let default = render_html(TEXT, &opts).unwrap();
    opts.font_embedding = FontEmbedding::Subset;
    assert_eq!(default, render_html(TEXT, &opts).unwrap());
    let compact = payloads(&default, "font/otf");
    assert_eq!(compact.len(), 1);
    assert!(compact[0].len() < SOURCE_SANS.len());

    opts.custom_css = Some("body { color: darkslateblue; }".into());
    let compact_css = render_html(TEXT, &opts).unwrap();
    opts.font_embedding = FontEmbedding::Full;
    let full_css = render_html(TEXT, &opts).unwrap();
    assert_eq!(compact_css, full_css);
    assert!(!full_css.contains("@font-face"));
}
