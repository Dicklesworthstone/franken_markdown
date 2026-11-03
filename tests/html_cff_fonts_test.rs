//! Checked HTML rendering of real CFF1 OpenType host fonts, including native
//! OpenType and both WOFF containers. The fixture is test-only and OFL licensed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::ops::Range;

use franken_markdown::text::Font;
use franken_markdown::{
    FontAssetSlot, FontAssets, HtmlFontFormat, HtmlOptions, RenderError, parse_markdown,
    render_html, render_html_document, render_interactive_html,
};

const CFF: &[u8] = include_bytes!("fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");
const DOC: &str = "Regular café Ω Ж. **Bold café Ω Ж.** *Italic café Ω Ж.* \
    ***Bold italic café Ω Ж.*** `Mono café Ω Ж.`";
const SLOTS: [FontAssetSlot; 5] = [
    FontAssetSlot::BodyRegular,
    FontAssetSlot::BodyBold,
    FontAssetSlot::BodyItalic,
    FontAssetSlot::BodyBoldItalic,
    FontAssetSlot::MonoRegular,
];

fn options(format: HtmlFontFormat) -> HtmlOptions {
    let mut font_assets = FontAssets::default();
    for slot in SLOTS {
        font_assets.set_slot(slot, CFF.to_vec()).unwrap();
    }
    HtmlOptions {
        font_assets,
        html_font_format: format,
        ..HtmlOptions::default()
    }
}

fn decode_base64(encoded: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let mut value = 0u32;
    let mut bits = 0;
    for byte in encoded.bytes().take_while(|&byte| byte != b'=') {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("unexpected base64 byte"),
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((value >> bits) as u8);
        }
    }
    result
}

fn fonts(html: &str, mime: &str) -> Vec<Vec<u8>> {
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

/// WOFF1 has no public decoder. Its standard table directory plus the public
/// zlib decoder lets us compare all preserved table bytes independently.
fn woff1_tables(bytes: &[u8]) -> BTreeMap<[u8; 4], Vec<u8>> {
    assert_eq!(&bytes[..4], b"wOFF");
    assert_eq!(&bytes[4..8], b"OTTO");
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

fn assert_real_cff_subset(bytes: &[u8], original: &Font) {
    assert_eq!(&bytes[..4], b"OTTO");
    assert!(
        bytes.len() < CFF.len() / 2,
        "HTML must subset the host face"
    );
    let subset = Font::parse(bytes.to_vec()).unwrap();
    assert!(!subset.has_glyf_outlines());
    assert!(subset.num_glyphs < original.num_glyphs);
    for ch in ['c', 'a', 'f', 'é', 'Ω', 'Ж'] {
        let old = original.glyph_index(ch);
        let new = subset.glyph_index(ch);
        assert_ne!(new, 0, "missing U+{:04X}", u32::from(ch));
        assert_eq!(subset.advance_width(new), original.advance_width(old));
        assert_eq!(
            subset.cff_outline(new).unwrap(),
            original.cff_outline(old).unwrap()
        );
    }
}

#[test]
fn all_five_cff_slots_preserve_native_opentype_payloads_and_css_identity() {
    let options = options(HtmlFontFormat::Ttf);
    let html = render_html(DOC, &options).unwrap();
    assert_eq!(
        html,
        render_html_document(&parse_markdown(DOC), &options).unwrap()
    );
    assert_eq!(html, render_html(DOC, &options).unwrap());
    assert!(!html.contains("data:font/ttf"));
    assert_eq!(html.matches("format(\"opentype\")").count(), 5);
    let expected = [
        ("FMD Body", "normal", "400"),
        ("FMD Body", "normal", "700"),
        ("FMD Body", "italic", "400"),
        ("FMD Body", "italic", "700"),
        ("FMD Mono", "normal", "400"),
    ];
    let rules: Vec<_> = html.split("@font-face {\n").skip(1).collect();
    assert_eq!(rules.len(), 5);
    for (rule, (family, style, weight)) in rules.iter().zip(expected) {
        let rule = rule.split_once('}').unwrap().0;
        assert!(rule.contains(&format!("font-family: \"{family}\"")));
        assert!(rule.contains(&format!("font-style: {style};")));
        assert!(rule.contains(&format!("font-weight: {weight};")));
    }
    let faces = fonts(&html, "font/otf");
    assert_eq!(faces.len(), 5);
    let original = Font::parse(CFF.to_vec()).unwrap();
    for bytes in faces {
        assert_real_cff_subset(&bytes, &original);
    }
}

#[test]
fn cff_woff_containers_keep_otto_flavor_and_exact_subset_tables() {
    let raw = fonts(
        &render_html(DOC, &options(HtmlFontFormat::Ttf)).unwrap(),
        "font/otf",
    );
    assert_eq!(raw.len(), 5);
    let original = Font::parse(CFF.to_vec()).unwrap();
    for (format, mime, hint) in [
        (HtmlFontFormat::Woff1, "font/woff", "woff"),
        (HtmlFontFormat::Woff2, "font/woff2", "woff2"),
    ] {
        let options = options(format);
        let html = render_html(DOC, &options).unwrap();
        assert_eq!(html, render_html(DOC, &options).unwrap());
        assert!(!html.contains("data:font/otf") && !html.contains("data:font/ttf"));
        assert_eq!(html.matches(&format!("format(\"{hint}\")")).count(), 5);
        let wrapped = fonts(&html, mime);
        assert_eq!(wrapped.len(), raw.len());
        for (container, sfnt) in wrapped.iter().zip(&raw) {
            assert_eq!(&container[4..8], b"OTTO");
            let actual = if format == HtmlFontFormat::Woff1 {
                woff1_tables(container)
            } else {
                assert_eq!(&container[..4], b"wOF2");
                let decoded = franken_markdown::woff2::decode_woff2(container).unwrap();
                assert_real_cff_subset(&decoded, &original);
                sfnt_tables(&decoded)
            };
            let expected = sfnt_tables(sfnt);
            assert_eq!(
                actual.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>()
            );
            for (tag, mut table) in actual {
                let mut expected = expected[&tag].clone();
                // WOFF2 reconstructs sfnt offsets and repairs checkSumAdjustment.
                if &tag == b"head" {
                    table[8..12].fill(0);
                    expected[8..12].fill(0);
                }
                assert_eq!(table, expected, "table {tag:?}");
            }
        }
    }
}

#[test]
fn cff_and_bundled_truetype_have_matching_per_face_mime_and_format_hints() {
    let options = HtmlOptions {
        font_assets: FontAssets::default()
            .with_slot(FontAssetSlot::BodyRegular, CFF.to_vec())
            .unwrap(),
        html_font_format: HtmlFontFormat::Ttf,
        ..HtmlOptions::default()
    };
    let html = render_html(DOC, &options).unwrap();
    let cff = fonts(&html, "font/otf");
    let ttf = fonts(&html, "font/ttf");
    assert_eq!(cff.len(), 1);
    assert_eq!(ttf.len(), 4);
    assert_eq!(html.matches("format(\"opentype\")").count(), 1);
    assert_eq!(html.matches("format(\"truetype\")").count(), 4);
    assert!(!Font::parse(cff[0].clone()).unwrap().has_glyf_outlines());
    assert!(
        ttf.into_iter()
            .all(|bytes| Font::parse(bytes).unwrap().has_glyf_outlines())
    );
}

#[test]
fn editable_html_retains_complete_cff_faces_with_opentype_mime() {
    let options = options(HtmlFontFormat::Ttf);
    let html = render_interactive_html(&parse_markdown("A"), "A", &options);
    let iframe = html
        .split_once("<iframe data-fmd-styled-preview")
        .unwrap()
        .1;
    let srcdoc = iframe
        .split_once(" srcdoc=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    let document = srcdoc
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let faces = fonts(&document, "font/otf");
    assert_eq!(faces.len(), 5);
    assert!(faces.iter().all(|bytes| bytes == CFF));
    assert_eq!(document.matches("format(\"opentype\")").count(), 5);
}

fn index_entry(bytes: &[u8], at: usize, index: usize) -> (Range<usize>, usize) {
    let count = usize::from(u16_at(bytes, at));
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
fn checked_html_refuses_an_unsupported_used_glyph_in_every_font_container() {
    let original = Font::parse(CFF.to_vec()).unwrap();
    let mut bytes = original.try_subset(&['A', 'Ω']).unwrap().bytes;
    let gid = Font::parse(bytes.clone()).unwrap().glyph_index('Ω');
    let record = (0..usize::from(u16_at(&bytes, 4)))
        .map(|index| 12 + index * 16)
        .find(|&at| &bytes[at..at + 4] == b"CFF ")
        .unwrap();
    let table = u32_at(&bytes, record + 8) as usize;
    let cff = &bytes[table..];
    let (_, next) = index_entry(cff, usize::from(cff[2]), 0);
    let (top, _) = index_entry(cff, next, 0);
    let charstrings = cff[top]
        .windows(6)
        .find_map(|window| (window[0] == 29 && window[5] == 17).then(|| u32_at(window, 1) as usize))
        .unwrap();
    let (glyph, _) = index_entry(cff, charstrings, usize::from(gid));
    assert!(glyph.len() >= 2);
    // Preserve all structures and .notdef; introduce deterministic-tier
    // refusal only in Omega's Type2 charstring (escaped random operator).
    bytes[table + glyph.start..table + glyph.start + 2].copy_from_slice(&[12, 23]);
    let assets = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, bytes)
        .unwrap();
    for format in [
        HtmlFontFormat::Ttf,
        HtmlFontFormat::Woff1,
        HtmlFontFormat::Woff2,
    ] {
        let options = HtmlOptions {
            font_assets: assets.clone(),
            html_font_format: format,
            ..HtmlOptions::default()
        };
        render_html("A", &options)
            .expect("an unused unsupported glyph must not reject ordinary text");
        let error = render_html_document(&parse_markdown("Ω"), &options).unwrap_err();
        let RenderError::InvalidInput(message) = error else {
            panic!("unsupported used CFF glyph must be an input error: {error}");
        };
        assert!(
            message.starts_with("html_cff_font:") && message.contains("UnsupportedOperator"),
            "{message}"
        );
        assert!(message.contains(&format!("glyph Some({gid})")), "{message}");
    }
}
