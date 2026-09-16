//! Focused tests for font context, fallback chains, clustering, and
//! bounded shaping-context checkpoints (FCB-075.A).

#![forbid(unsafe_code)]

use franken_markdown::font_context::{
    checkpoint_context, classify_char, restore_context, segment_clusters, CharClass,
    CheckpointError, FallbackChain, FontRunIdentity,
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
    assert_eq!(classify_char(' '), CharClass::Whitespace);
    assert_eq!(classify_char('!'), CharClass::Other);
}

#[test]
fn cluster_segmentation_groups_combining_marks() {
    // "café" — the é is e + combining acute (2 codepoints, 1 cluster).
    let text = "cafe\u{0301}";
    let clusters = segment_clusters(text);
    // "caf" is 3 clusters + "é" combining = 1 cluster → 4 total.
    assert_eq!(clusters.len(), 4);
    // The last cluster includes the combining mark.
    assert_eq!(clusters.last().unwrap().end, text.len());
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
}

#[test]
fn checkpoint_round_trips_field_for_field() {
    let runs = vec![
        make_font("Inter", 14),
        make_font("NotoSansCJK", 14),
    ];
    let clusters = vec![];
    let blob = franken_markdown::font_context::checkpoint_context(&runs, &clusters)
        .expect("bounded checkpoint succeeds");
    let restored =
        franken_markdown::font_context::restore_context(&blob).expect("round-trip succeeds");
    assert_eq!(restored.version, 1);
    assert_eq!(restored.run_identities.len(), runs.len());
    assert_eq!(restored.run_identities[0].family, "Inter");
    assert_eq!(restored.run_identities[1].family, "NotoSansCJK");
}

#[test]
fn oversized_checkpoint_is_refused() {
    // Create a context with a very long family name to exceed the cap.
    let huge_family = "F".repeat(4096);
    let runs = vec![make_font(&huge_family, 14)];
    let result = franken_markdown::font_context::checkpoint_context(&runs, &[]);
    assert_eq!(result.unwrap_err(), CheckpointError::TooLarge);
}

#[test]
fn unknown_checkpoint_version_is_refused() {
    let bad = [0xFF, 0xFF, 0xFF, 0xFF];
    assert_eq!(
        franken_markdown::font_context::restore_context(&bad),
        Err(CheckpointError::UnknownVersion)
    );
}
