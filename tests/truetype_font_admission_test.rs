//! Checked publishers must not turn malformed TrueType outlines into a
//! successful document with missing ink. The small original collection face
//! has independent geometric fixtures; only one used glyph is damaged here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::{Font, SubsetErrorKind};
use franken_markdown::wasm::{self, WasmRenderOptions};
use franken_markdown::{
    FontAssetSlot, FontAssets, FontEmbedding, HtmlOptions, PdfEmitOptions, PdfOptions, RenderError,
    SvgOptions, parse_markdown, render_epub, render_html, render_html_document_to_writer,
    render_pdf, render_pdf_document_emitted, render_pdf_document_reported,
};

const TTF: &[u8] = include_bytes!("../fmd-font/fonts/test-collection/FmdCollectionTrueType.ttf");
const SHAPING: &[u8] = include_bytes!("../fmd-font/fonts/test-shaping/FmdShaping.ttf");

fn table(bytes: &[u8], tag: &[u8; 4]) -> (usize, usize) {
    let count = usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap()));
    (0..count)
        .find_map(|index| {
            let record = 12 + 16 * index;
            (&bytes[record..record + 4] == tag).then(|| {
                (
                    u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize,
                    u32::from_be_bytes(bytes[record + 12..record + 16].try_into().unwrap())
                        as usize,
                )
            })
        })
        .expect("fixture contains table")
}

fn damage_loca(bytes: &mut [u8], gid: u16) {
    let (head, _) = table(bytes, b"head");
    let (loca, _) = table(bytes, b"loca");
    let (_, glyf_len) = table(bytes, b"glyf");
    let index = usize::from(gid) + 1;
    if u16::from_be_bytes(bytes[head + 50..head + 52].try_into().unwrap()) == 0 {
        assert!(glyf_len < usize::from(u16::MAX) * 2);
        bytes[loca + 2 * index..loca + 2 * index + 2].copy_from_slice(&u16::MAX.to_be_bytes());
    } else {
        bytes[loca + 4 * index..loca + 4 * index + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    }
}

fn invalid_outline(composite: bool) -> Vec<u8> {
    let mut bytes = TTF.to_vec();
    let font = Font::parse(bytes.clone()).unwrap();
    let gid = font.glyph_index('日');
    assert_eq!(gid, 3);
    if composite {
        let (glyf, _) = table(&bytes, b"glyf");
        let (loca, _) = table(&bytes, b"loca");
        let at = glyf
            + usize::from(u16::from_be_bytes(
                bytes[loca + 6..loca + 8].try_into().unwrap(),
            )) * 2;
        // A complete composite record whose child is outside numGlyphs.
        // The legacy subsetter replaces that child with empty .notdef.
        bytes[at..at + 2].copy_from_slice(&(-1i16).to_be_bytes());
        bytes[at + 10..at + 12].copy_from_slice(&3u16.to_be_bytes());
        bytes[at + 12..at + 14].copy_from_slice(&u16::MAX.to_be_bytes());
        bytes[at + 14..at + 18].fill(0);
    } else {
        damage_loca(&mut bytes, gid);
    }
    let font = Font::parse(bytes.clone()).unwrap();
    assert!(
        font.try_subset_glyphs(&[], &[]).is_ok(),
        ".notdef is intact"
    );
    assert!(font.glyph_outline(gid).is_err());
    assert!(font.try_subset(&['日']).is_err());
    bytes
}

fn assets(slot: FontAssetSlot, bytes: &[u8]) -> FontAssets {
    FontAssets::default().with_slot(slot, bytes).unwrap()
}

fn assert_font_error<T>(result: franken_markdown::Result<T>, glyph: Option<u16>) {
    let error = result.err().expect("invalid font must not publish output");
    assert!(matches!(error, RenderError::InvalidInput(_)), "{error}");
    let message = error.to_string();
    assert!(message.contains("font subset"), "{message}");
    if let Some(gid) = glyph {
        assert!(message.contains(&format!("glyph Some({gid})")), "{message}");
    }
}

#[test]
fn damaged_fixtures_expose_the_legacy_silent_ink_loss() {
    for composite in [false, true] {
        let font = Font::parse(invalid_outline(composite)).unwrap();
        let error = font.try_subset(&['日']).unwrap_err();
        assert_eq!(error.kind, SubsetErrorKind::Malformed);
        assert_eq!(error.glyph, Some(3));
        // This deliberately compatible Option API stays tolerant. Its returned
        // font maps the requested character to an empty outline, which is why
        // a checked publication must use the existing strict API instead.
        let subset = Font::parse(font.subset(&['日']).unwrap()).unwrap();
        let outline = subset.glyph_outline(subset.glyph_index('日')).unwrap();
        assert!(outline.contours.is_empty());
    }
}

#[test]
fn used_invalid_outlines_are_refused_in_all_slots_and_formats() {
    for composite in [false, true] {
        let bytes = invalid_outline(composite);
        for (slot, source) in [
            (FontAssetSlot::BodyRegular, "日"),
            (FontAssetSlot::BodyBold, "**日**"),
            (FontAssetSlot::BodyItalic, "*日*"),
            (FontAssetSlot::BodyBoldItalic, "***日***"),
            (FontAssetSlot::MonoRegular, "`日`"),
        ] {
            let fonts = assets(slot, &bytes);
            let document = parse_markdown(source);
            let html = HtmlOptions {
                font_assets: fonts.clone(),
                ..HtmlOptions::default()
            };
            assert_font_error(render_html(source, &html), Some(3));
            assert_font_error(render_epub(&document, &html), Some(3));
            assert_font_error(
                render_pdf(
                    source,
                    &PdfOptions {
                        font_assets: fonts.clone(),
                        ..PdfOptions::default()
                    },
                ),
                Some(3),
            );
            assert_font_error(
                franken_markdown::svg::render_svg_with_resources(
                    &document,
                    &SvgOptions::default(),
                    &fonts,
                    &[],
                ),
                Some(3),
            );
        }
    }
}

#[test]
fn unused_bad_glyphs_do_not_change_the_rendered_bytes() {
    let source = "A B";
    let document = parse_markdown(source);
    let good = assets(FontAssetSlot::BodyRegular, TTF);
    let bad = assets(FontAssetSlot::BodyRegular, &invalid_outline(false));
    let html = |fonts| HtmlOptions {
        font_assets: fonts,
        ..HtmlOptions::default()
    };
    let pdf = |fonts| PdfOptions {
        font_assets: fonts,
        ..PdfOptions::default()
    };
    assert_eq!(
        render_html(source, &html(bad.clone())).unwrap(),
        render_html(source, &html(good.clone())).unwrap()
    );
    assert_eq!(
        render_epub(&document, &html(bad.clone())).unwrap(),
        render_epub(&document, &html(good.clone())).unwrap()
    );
    assert_eq!(
        render_pdf(source, &pdf(bad.clone())).unwrap(),
        render_pdf(source, &pdf(good.clone())).unwrap()
    );
    assert_eq!(
        franken_markdown::svg::render_svg_with_resources(
            &document,
            &SvgOptions::default(),
            &bad,
            &[]
        )
        .unwrap(),
        franken_markdown::svg::render_svg_with_resources(
            &document,
            &SvgOptions::default(),
            &good,
            &[]
        )
        .unwrap(),
    );
}

#[test]
fn html_refuses_before_writing_and_pdf_emission_modes_share_the_error() {
    let fonts = assets(FontAssetSlot::BodyRegular, &invalid_outline(false));
    let document = parse_markdown("日");
    let html = HtmlOptions {
        font_assets: fonts.clone(),
        ..HtmlOptions::default()
    };
    let mut sink = b"existing caller data".to_vec();
    assert_font_error(
        render_html_document_to_writer(&document, &html, &mut sink),
        Some(3),
    );
    assert_eq!(sink, b"existing caller data");
    let pdf = PdfOptions {
        font_assets: fonts,
        ..PdfOptions::default()
    };
    for emit in [PdfEmitOptions::default(), PdfEmitOptions::monolithic()] {
        assert_font_error(render_pdf_document_emitted(&document, &pdf, emit), Some(3));
        assert_font_error(render_pdf_document_reported(&document, &pdf, emit), Some(3));
    }
}

#[test]
fn invalid_metrics_and_notdef_fail_before_layout_even_with_full_fonts() {
    for notdef in [false, true] {
        let mut bytes = TTF.to_vec();
        if notdef {
            damage_loca(&mut bytes, 0);
        } else {
            let (head, _) = table(&bytes, b"head");
            bytes[head + 18..head + 20].copy_from_slice(&0u16.to_be_bytes());
        }
        let font = Font::parse(bytes.clone()).unwrap();
        assert!(font.try_subset_glyphs(&[], &[]).is_err());
        assert_font_error(
            FontAssets::default().with_slot(FontAssetSlot::BodyRegular, bytes.clone()),
            None,
        );
        // Public fields must not bypass the admission used by with_slot.
        let fonts = FontAssets {
            body_regular: Some(bytes),
            ..FontAssets::default()
        };
        let document = parse_markdown("A");
        for embedding in [FontEmbedding::Subset, FontEmbedding::Full] {
            let html = HtmlOptions {
                font_assets: fonts.clone(),
                font_embedding: embedding,
                ..HtmlOptions::default()
            };
            assert_font_error(render_html("A", &html), None);
            assert_font_error(render_epub(&document, &html), None);
        }
        assert_font_error(
            render_pdf(
                "A",
                &PdfOptions {
                    font_assets: fonts.clone(),
                    ..PdfOptions::default()
                },
            ),
            None,
        );
        assert_font_error(
            franken_markdown::svg::render_svg_with_resources(
                &document,
                &SvgOptions::default(),
                &fonts,
                &[],
            ),
            None,
        );
    }
}

#[test]
fn shaped_ligature_glyphs_are_validated_even_without_a_direct_cmap_entry() {
    let mut bytes = SHAPING.to_vec();
    let font = Font::parse(bytes.clone()).unwrap();
    let source_glyphs = [font.glyph_index('f'), font.glyph_index('i')];
    let shaped = font.gsub_ligatures().substitute(&source_glyphs);
    assert_eq!(shaped.len(), 1, "fixture forms the fi ligature");
    let gid = shaped[0];
    assert!(!source_glyphs.contains(&gid));
    damage_loca(&mut bytes, gid);
    let fonts = assets(FontAssetSlot::BodyRegular, &bytes);
    let document = parse_markdown("fi");
    assert_font_error(
        render_pdf(
            "fi",
            &PdfOptions {
                font_assets: fonts.clone(),
                ..PdfOptions::default()
            },
        ),
        Some(gid),
    );
    assert_font_error(
        franken_markdown::svg::render_svg_with_resources(
            &document,
            &SvgOptions::default(),
            &fonts,
            &[],
        ),
        Some(gid),
    );
}

#[test]
fn browser_rust_facades_preserve_the_checked_refusal() {
    let options = WasmRenderOptions {
        font_assets: assets(FontAssetSlot::BodyRegular, &invalid_outline(false)),
        ..WasmRenderOptions::default()
    };
    assert_font_error(wasm::render_html("日", &options), Some(3));
    assert_font_error(wasm::render_pdf("日", &options), Some(3));
}

#[test]
fn intact_full_embedding_and_infallible_helpers_keep_their_compatibility() {
    let options = HtmlOptions {
        font_assets: assets(FontAssetSlot::BodyRegular, &invalid_outline(false)),
        font_embedding: FontEmbedding::Full,
        ..HtmlOptions::default()
    };
    let document = parse_markdown("A B");
    assert!(render_html("A B", &options).is_ok());
    assert!(render_epub(&document, &options).is_ok());
    let compact = HtmlOptions {
        font_embedding: FontEmbedding::Subset,
        ..options
    };
    let damaged_document = parse_markdown("日");
    // Callers choosing the old infallible API retain its original fallback.
    assert!(!franken_markdown::html::render(&damaged_document, &compact).is_empty());
}
