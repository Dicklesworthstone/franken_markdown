//! Comprehensive tests for font context, fallback chains, clustering,
//! bounded checkpoints, color/bitmap resources, and exact logical selection (FCB-075.A).

#![forbid(unsafe_code)]

use franken_markdown::font_context::{
    checkpoint_context, classify_char, restore_context, segment_clusters, select_logical_range,
    BitmapGlyphResource, CharClass, CheckpointError, ColorGlyphError, ColorGlyphResource,
    FallbackChain, FontRunIdentity, MAX_BITMAP_GLYPH_DIMENSION, MAX_CHECKPOINT_BYTES,
    MAX_COLOR_GLYPH_DIMENSION,
};

fn make_font(family: &str, size: u16) -> FontRunIdentity {
    FontRunIdentity::new(family, false, false, size)
}

#[test]
fn char_classification_identifies_scripts() {
    assert_eq!(classify_char('a'), CharClass::Alphanumeric);
    assert_eq!(classify_char('日'), CharClass::Cjk);
    assert_eq!(classify_char('ع'), CharClass::Rtl);
    assert_eq!(classify_char('😀'), CharClass::Emoji);
    assert_eq!(classify_char('\u{0301}'), CharClass::Combining);
    assert_eq!(classify_char('\t'), CharClass::Tab);
    assert_eq!(classify_char(' '), CharClass::Whitespace);
    assert_eq!(classify_char('!'), CharClass::Other);
}

#[test]
fn font_run_identity_deterministic_font_id() {
    let font1 = FontRunIdentity::new("Inter", false, false, 14);
    let font2 = FontRunIdentity::new("Inter", false, false, 14);
    let font_bold = FontRunIdentity::new("Inter", true, false, 14);
    let font_cjk = FontRunIdentity::new("NotoSansCJK", false, false, 14);

    assert_eq!(font1.font_id(), font2.font_id());
    assert_ne!(font1.font_id(), font_bold.font_id());
    assert_ne!(font1.font_id(), font_cjk.font_id());
}

#[test]
fn cluster_segmentation_groups_combining_marks() {
    // "café" — the é is e + combining acute (2 codepoints, 1 cluster).
    let text = "cafe\u{0301}";
    let clusters = segment_clusters(text);
    assert_eq!(clusters.len(), 4);
    let last = clusters.last().expect("last cluster");
    assert_eq!(last.end, text.len());
    assert_eq!(last.char_count, 2);
}

#[test]
fn cluster_segmentation_identifies_rtl_and_emoji() {
    let text = "hello عربى 😀";
    let clusters = segment_clusters(text);
    let rtl_cluster = clusters.iter().find(|c| c.is_rtl);
    assert!(rtl_cluster.is_some(), "Arabic text should produce RTL cluster");
    let emoji_cluster = clusters.iter().find(|c| c.is_emoji);
    assert!(emoji_cluster.is_some(), "emoji should produce emoji cluster");
}

#[test]
fn fallback_chain_resolves_in_order() {
    let primary = make_font("Inter", 14);
    let cjk = make_font("NotoSansCJK", 14);
    let emoji = make_font("NotoColorEmoji", 14);
    let mut chain = FallbackChain::single(primary.clone());
    chain.push_fallback(cjk.clone());
    chain.push_fallback(emoji.clone());

    let covers_latin = |f: &FontRunIdentity, _c: char| f.family == "Inter";
    let covers_cjk = |f: &FontRunIdentity, _c: char| f.family == "NotoSansCJK";
    let covers_emoji = |f: &FontRunIdentity, _c: char| f.family == "NotoColorEmoji";

    assert_eq!(
        chain.resolve('a', &covers_latin).map(|f| &f.family),
        Some(&"Inter".to_owned())
    );
    assert_eq!(
        chain.resolve('日', &covers_cjk).map(|f| &f.family),
        Some(&"NotoSansCJK".to_owned())
    );
    assert_eq!(
        chain.resolve('😀', &covers_emoji).map(|f| &f.family),
        Some(&"NotoColorEmoji".to_owned())
    );
    assert_eq!(chain.resolve('§', &|_, _| false), None);
}

#[test]
fn corpus_cjk_ideographs_and_hangul() {
    let text = "漢字とハングル한국어";
    let clusters = segment_clusters(text);
    assert!(!clusters.is_empty());
    for cluster in &clusters {
        assert!(cluster.is_cjk, "CJK character expected: {:?}", cluster);
        assert_eq!(cluster.char_count, 1);
        assert!(!cluster.is_rtl);
        assert!(!cluster.is_emoji);
    }
}

#[test]
fn corpus_rtl_arabic_and_hebrew() {
    let arabic = "السلام عليكم";
    let clusters_ar = segment_clusters(arabic);
    let rtl_clusters: Vec<_> = clusters_ar.iter().filter(|c| c.is_rtl).collect();
    assert!(!rtl_clusters.is_empty(), "Arabic text must produce RTL clusters");

    let hebrew = "שלום עולם";
    let clusters_he = segment_clusters(hebrew);
    let rtl_clusters_he: Vec<_> = clusters_he.iter().filter(|c| c.is_rtl).collect();
    assert!(!rtl_clusters_he.is_empty(), "Hebrew text must produce RTL clusters");
}

#[test]
fn corpus_joining_characters_and_zwnj() {
    // Arabic cursive with zero-width non-joiner (ZWNJ \u{200C})
    let text = "می‌خواهم";
    let clusters = segment_clusters(text);
    let rtl_count = clusters.iter().filter(|c| c.is_rtl).count();
    assert!(rtl_count > 0, "Persian text with ZWNJ must contain RTL clusters");
}

#[test]
fn corpus_combining_marks_multi_stacking() {
    // Base 'o' + combining diaeresis \u{0308} + combining acute \u{0301}
    let text = "o\u{0308}\u{0301}";
    let clusters = segment_clusters(text);
    assert_eq!(clusters.len(), 1, "Stacked combining marks must form 1 cluster");
    assert_eq!(clusters[0].char_count, 3);
    assert_eq!(clusters[0].end, text.len());
}

#[test]
fn corpus_emoji_zwj_and_skin_tones() {
    // Single emoji, skin tone modifier, and compound ZWJ
    let text = "😀 👍🏽";
    let clusters = segment_clusters(text);
    let emojis: Vec<_> = clusters.iter().filter(|c| c.is_emoji).collect();
    assert_eq!(emojis.len(), 2, "Expected 2 emoji clusters");
    assert_eq!(emojis[0].char_count, 1);
    assert!(emojis[1].char_count >= 2, "Thumbs up with skin tone has >= 2 codepoints");
}

#[test]
fn corpus_tab_stop_segmentation() {
    let text = "col1\tcol2\t\tcol3";
    let clusters = segment_clusters(text);
    let tabs: Vec<_> = clusters.iter().filter(|c| c.is_tab).collect();
    assert_eq!(tabs.len(), 3, "Expected 3 tab clusters");
    for tab in tabs {
        assert_eq!(tab.byte_len(), 1);
        assert_eq!(tab.char_count, 1);
    }
}

#[test]
fn corpus_ligature_detection() {
    let text = "fn test() -> bool { a == b && c != d && x => y }";
    let clusters = segment_clusters(text);
    let ligatures: Vec<_> = clusters.iter().filter(|c| c.is_ligature).collect();
    assert!(!ligatures.is_empty(), "Programming ligatures should be detected");

    // Check specific ligatures
    let has_arrow = ligatures.iter().any(|c| &text[c.start..c.end] == "->");
    let has_eq = ligatures.iter().any(|c| &text[c.start..c.end] == "==");
    let has_ne = ligatures.iter().any(|c| &text[c.start..c.end] == "!=");
    let has_fat_arrow = ligatures.iter().any(|c| &text[c.start..c.end] == "=>");

    assert!(has_arrow, "Expected '->' ligature");
    assert!(has_eq, "Expected '==' ligature");
    assert!(has_ne, "Expected '!=' ligature");
    assert!(has_fat_arrow, "Expected '=>' ligature");
}

#[test]
fn color_glyph_pixels_and_sampling() {
    let width = 2u32;
    let height = 2u32;
    // 2x2 RGBA image (16 bytes)
    let pixels = vec![
        255, 0, 0, 255,   // (0, 0) Red
        0, 255, 0, 255,   // (1, 0) Green
        0, 0, 255, 255,   // (0, 1) Blue
        255, 255, 0, 255, // (1, 1) Yellow
    ];

    let glyph = ColorGlyphResource::try_new_rgba('😀', width, height, pixels)
        .expect("valid color glyph");

    assert_eq!(glyph.pixel_at(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(glyph.pixel_at(1, 0), Some([0, 255, 0, 255]));
    assert_eq!(glyph.pixel_at(0, 1), Some([0, 0, 255, 255]));
    assert_eq!(glyph.pixel_at(1, 1), Some([255, 255, 0, 255]));
    assert_eq!(glyph.pixel_at(2, 2), None);
}

#[test]
fn color_glyph_fallback_placeholder() {
    let fallback = ColorGlyphResource::fallback_placeholder('🚀', 32);
    assert_eq!(fallback.width, 32);
    assert_eq!(fallback.height, 32);
    assert_eq!(fallback.pixels.len(), 32 * 32 * 4);
    assert_eq!(fallback.codepoint, '🚀');

    // Border pixels must be non-zero / distinct magenta
    let border_px = fallback.pixel_at(0, 0).expect("border pixel");
    assert_eq!(border_px, [220, 40, 220, 255]);

    // Interior pixels
    let interior_px = fallback.pixel_at(16, 16).expect("interior pixel");
    assert_eq!(interior_px, [120, 120, 120, 80]);
}

#[test]
fn bitmap_glyph_bits_and_sampling() {
    let width = 8u32;
    let height = 2u32;
    // 8x2 = 16 bits = 2 bytes
    let bits = vec![0b10101010, 0b01010101];

    let glyph = BitmapGlyphResource::try_new_1bpp('A', width, height, bits)
        .expect("valid bitmap glyph");

    assert_eq!(glyph.bit_at(0, 0), Some(true));
    assert_eq!(glyph.bit_at(1, 0), Some(false));
    assert_eq!(glyph.bit_at(0, 1), Some(false));
    assert_eq!(glyph.bit_at(1, 1), Some(true));
    assert_eq!(glyph.bit_at(8, 0), None);
}

#[test]
fn exact_logical_selection_across_scripts() {
    let text = "Latin 日本語 عربى 😀 cafe\u{0301}";
    let clusters = segment_clusters(text);

    // Select "日本語"
    let cjk_start = text.find("日本語").expect("find cjk");
    let cjk_end = cjk_start + "日本語".len();
    let sel_cjk = select_logical_range(&clusters, cjk_start..cjk_end)
        .expect("cjk selection");
    assert_eq!(sel_cjk.byte_range, cjk_start..cjk_end);
    assert!(sel_cjk.has_cjk);
    assert!(!sel_cjk.has_rtl);
    assert_eq!(sel_cjk.char_count, 3);

    // Select spanning combining mark partially: must snap to full cluster
    let cafe_start = text.find("cafe").expect("find cafe");
    // Request up to 'e' without the combining mark: selection snaps to include the combining mark!
    let sel_combining = select_logical_range(&clusters, cafe_start..text.len() - 1)
        .expect("combining selection");
    assert_eq!(sel_combining.byte_range.end, text.len());
}

#[test]
fn negative_control_color_glyph_dimension_too_large() {
    let huge_dim = MAX_COLOR_GLYPH_DIMENSION + 1;
    let result = ColorGlyphResource::try_new_rgba('X', huge_dim, 10, vec![]);
    assert_eq!(
        result,
        Err(ColorGlyphError::DimensionTooLarge {
            dimension: huge_dim,
            max_allowed: MAX_COLOR_GLYPH_DIMENSION,
        })
    );
}

#[test]
fn negative_control_color_glyph_mismatched_buffer() {
    let result = ColorGlyphResource::try_new_rgba('X', 4, 4, vec![0u8; 10]);
    assert_eq!(
        result,
        Err(ColorGlyphError::BufferLengthMismatch {
            expected: 64,
            actual: 10,
        })
    );
}

#[test]
fn negative_control_color_glyph_zero_dimension() {
    let result = ColorGlyphResource::try_new_rgba('X', 0, 10, vec![]);
    assert_eq!(result, Err(ColorGlyphError::ZeroDimension));
}

#[test]
fn negative_control_bitmap_glyph_caps() {
    let huge_dim = MAX_BITMAP_GLYPH_DIMENSION + 1;
    let result = BitmapGlyphResource::try_new_1bpp('X', huge_dim, 10, vec![]);
    assert_eq!(
        result,
        Err(ColorGlyphError::DimensionTooLarge {
            dimension: huge_dim,
            max_allowed: MAX_BITMAP_GLYPH_DIMENSION,
        })
    );
}

#[test]
fn checkpoint_round_trips_field_for_field() {
    let runs = vec![
        make_font("Inter", 14),
        make_font("NotoSansCJK", 14),
    ];
    let clusters = segment_clusters("hello 世界");
    let blob = checkpoint_context(&runs, &clusters).expect("bounded checkpoint succeeds");
    let restored = restore_context(&blob).expect("round-trip succeeds");
    assert_eq!(restored.version, 2);
    assert_eq!(restored.run_identities.len(), runs.len());
    assert_eq!(restored.run_identities[0].family, "Inter");
    assert_eq!(restored.run_identities[1].family, "NotoSansCJK");
    assert_eq!(restored.cluster_offsets.len(), clusters.len());
}

#[test]
fn oversized_checkpoint_is_refused() {
    let huge_family = "F".repeat(MAX_CHECKPOINT_BYTES);
    let runs = vec![make_font(&huge_family, 14)];
    let result = checkpoint_context(&runs, &[]);
    assert_eq!(result.unwrap_err(), CheckpointError::TooLarge);
}

#[test]
fn unknown_checkpoint_version_is_refused() {
    let bad = [0xFF, 0xFF, 0xFF, 0xFF];
    assert_eq!(
        restore_context(&bad),
        Err(CheckpointError::UnknownVersion)
    );
}
