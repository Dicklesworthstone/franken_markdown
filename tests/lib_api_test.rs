//! Public crate-root API coverage (`src/lib.rs`), bead grn.2.8.
//!
//! Real inputs, no mocks: font-slot validation uses the REAL bundled TrueType
//! fonts (and genuinely-empty byte slices for the rejection path), and the parse
//! entry points run on real Markdown.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::{Document, FontAssetSlot, FontAssets, FontFamily, PdfImageAsset, parse};

fn real_font(slot: FontAssetSlot) -> &'static [u8] {
    match slot {
        FontAssetSlot::BodyRegular => fonts::body_bytes(FontFamily::Sans, FontStyle::Regular),
        FontAssetSlot::BodyBold => fonts::body_bytes(FontFamily::Sans, FontStyle::Bold),
        FontAssetSlot::BodyItalic => fonts::body_bytes(FontFamily::Sans, FontStyle::Italic),
        FontAssetSlot::BodyBoldItalic => fonts::body_bytes(FontFamily::Sans, FontStyle::BoldItalic),
        FontAssetSlot::MonoRegular => fonts::mono_bytes(FontStyle::Regular),
    }
}

const ALL_SLOTS: [FontAssetSlot; 5] = [
    FontAssetSlot::BodyRegular,
    FontAssetSlot::BodyBold,
    FontAssetSlot::BodyItalic,
    FontAssetSlot::BodyBoldItalic,
    FontAssetSlot::MonoRegular,
];

#[test]
fn font_asset_slot_parses_every_documented_spelling() {
    let cases = [
        ("body-regular", FontAssetSlot::BodyRegular),
        ("body_regular", FontAssetSlot::BodyRegular),
        ("regular", FontAssetSlot::BodyRegular),
        ("body-bold", FontAssetSlot::BodyBold),
        ("bold", FontAssetSlot::BodyBold),
        ("body-italic", FontAssetSlot::BodyItalic),
        ("italic", FontAssetSlot::BodyItalic),
        ("body-bold-italic", FontAssetSlot::BodyBoldItalic),
        ("bold_italic", FontAssetSlot::BodyBoldItalic),
        ("mono-regular", FontAssetSlot::MonoRegular),
        ("mono", FontAssetSlot::MonoRegular),
        ("code", FontAssetSlot::MonoRegular),
        ("  BODY-Bold  ", FontAssetSlot::BodyBold), // trimmed + case-insensitive
    ];
    for (spelling, want) in cases {
        assert_eq!(
            FontAssetSlot::parse(spelling),
            Some(want),
            "spelling {spelling:?}"
        );
    }
    assert_eq!(FontAssetSlot::parse("nonsense"), None);
}

#[test]
fn font_asset_slot_as_str_round_trips_through_parse() {
    for slot in ALL_SLOTS {
        let s = slot.as_str();
        assert_eq!(FontAssetSlot::parse(s), Some(slot), "as_str {s}");
    }
    // Spot-check the exact stable spellings.
    assert_eq!(FontAssetSlot::BodyBoldItalic.as_str(), "body-bold-italic");
    assert_eq!(FontAssetSlot::MonoRegular.as_str(), "mono-regular");
}

#[test]
fn font_assets_is_empty_tracks_population() {
    let mut assets = FontAssets::default();
    assert!(assets.is_empty());
    assets
        .set_slot(
            FontAssetSlot::BodyRegular,
            real_font(FontAssetSlot::BodyRegular).to_vec(),
        )
        .unwrap();
    assert!(!assets.is_empty());
}

#[test]
fn with_slot_and_set_slot_accept_every_real_bundled_face() {
    // Exercises every match arm of set_slot with REAL, subsettable font bytes.
    for slot in ALL_SLOTS {
        let assets = FontAssets::default()
            .with_slot(slot, real_font(slot).to_vec())
            .unwrap_or_else(|e| panic!("real bundled font rejected for {slot:?}: {e}"));
        assert!(!assets.is_empty());
        // The populated assets must pass full validation.
        assets.validate().unwrap();
    }
}

#[test]
fn empty_font_bytes_are_rejected_with_a_named_slot() {
    let err = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, Vec::<u8>::new())
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    assert!(err.to_string().contains("body-regular"), "got {err}");
    assert!(err.to_string().contains("empty"), "got {err}");
}

#[test]
fn directly_constructed_assets_validate_each_populated_slot() {
    // A caller who builds FontAssets by hand still gets validation, and an empty
    // slice in any slot is rejected (covers the validate() loop + rejection).
    let bad = FontAssets {
        body_regular: Some(real_font(FontAssetSlot::BodyRegular).to_vec()),
        mono_regular: Some(Vec::new()),
        ..FontAssets::default()
    };
    let err = bad.validate().unwrap_err();
    assert!(err.to_string().contains("mono-regular"), "got {err}");

    let good = FontAssets {
        body_italic: Some(real_font(FontAssetSlot::BodyItalic).to_vec()),
        ..FontAssets::default()
    };
    good.validate().unwrap();
}

#[test]
fn non_font_bytes_are_rejected_as_unsupported_truetype() {
    // A real, deliberately-bogus byte string is not a parseable font.
    let err = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, b"not a font at all".to_vec())
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    assert!(err.to_string().contains("body-regular"), "got {err}");
}

#[test]
fn pdf_image_asset_new_keeps_destination_and_bytes() {
    let asset = PdfImageAsset::new("images/chart.png", vec![1u8, 2, 3]);
    assert_eq!(asset.destination, "images/chart.png");
    assert_eq!(asset.bytes, vec![1, 2, 3]);
}

#[test]
fn parse_entry_point_builds_a_document_ast() {
    let doc: Document = parse("# Title\n\nbody paragraph\n");
    // The bare `parse` alias must produce the same non-empty AST as parse_markdown.
    assert_eq!(
        doc,
        franken_markdown::parse_markdown("# Title\n\nbody paragraph\n")
    );
    assert!(!doc.blocks.is_empty());
}
