//! Optional monochrome emoji through real bundled Flow shaping, source
//! clusters, measured layout and the native browser-session wire.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::FontFamily;
use franken_markdown::dep_invalidation::{BrowserFlowSession, FlowAssetReuse};
use franken_markdown::flow_display::{FlowInlineStyle, FlowLayoutOptions, FlowTextRole};
use franken_markdown::fonts::BundledFlowFonts;

#[test]
fn unsupported_sequences_cannot_be_misrepresented_as_component_emoji() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for source in [
        "👩\u{200d}💻",
        "🏳\u{fe0f}\u{200d}🌈",
        "👍🏻",
        "🏻",
        "🇺🇸",
        "🇺",
        "1\u{fe0f}\u{20e3}",
        "1\u{20e3}",
        "\u{20e3}",
        "\u{200d}",
        "\u{fe0e}",
        "\u{fe0f}",
        "a\u{fe0f}",
        "©\u{fe0f}\u{fe0f}",
        "🚀\u{fe0f}",
        "😀\u{fe0e}",
        "⚠\u{fe00}",
        "⚠\u{e0100}",
        "🏴\u{e0067}\u{e0062}\u{e007f}",
        "😀\u{0301}",
        "العربية",
        "אב",
        "x\u{202e}y",
        "\u{1faff}",
    ] {
        assert!(
            fonts
                .shape(source, 14.0, FlowTextRole::Body, FlowInlineStyle::default())
                .is_err(),
            "unsupported sequence gained a misleading glyph decomposition: {source:?}"
        );
    }
    let mut browser =
        BrowserFlowSession::new("Current source", "sans", FlowLayoutOptions::default()).unwrap();
    let before = browser.snapshot_json(1, 1, 0, 2048, true).unwrap();
    for source in ["👩\u{200d}💻", "👍🏻", "🇺🇸", "a\u{fe0f}"] {
        assert_eq!(
            browser
                .replace_source(1, source, FlowAssetReuse::Invalidate)
                .unwrap_err()
                .code(),
            "LAYOUT_ERROR"
        );
        assert_eq!(browser.source(), "Current source");
        assert_eq!((browser.revision(), browser.layout_revision()), (1, 1));
        assert_eq!(
            before.split("\"cache\"").next(),
            browser
                .snapshot_json(1, 1, 0, 2048, true)
                .unwrap()
                .split("\"cache\"")
                .next()
        );
    }
}

#[cfg(not(feature = "emoji-face"))]
#[test]
fn lean_profile_has_no_emoji_bytes_or_new_presentation_sequences() {
    assert!(franken_markdown::fonts::emoji_bytes().is_none());
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for source in ["🚀", "✅", "😀", "⚠\u{fe0f}", "©\u{fe0f}", "©\u{fe0e}"] {
        assert!(
            fonts
                .shape(source, 14.0, FlowTextRole::Body, FlowInlineStyle::default())
                .is_err()
        );
        assert!(BrowserFlowSession::new(source, "sans", FlowLayoutOptions::default()).is_err());
    }
    for source in ["office AV café © ® ™", "e\u{0301} ∑"] {
        assert!(
            fonts
                .shape(source, 14.0, FlowTextRole::Body, FlowInlineStyle::default())
                .is_ok()
        );
    }
    let mut state =
        BrowserFlowSession::new("Preserved", "sans", FlowLayoutOptions::default()).unwrap();
    assert!(
        state
            .replace_source(1, "🚀", FlowAssetReuse::Invalidate)
            .is_err()
    );
    assert_eq!(state.source(), "Preserved");
    assert_eq!((state.revision(), state.layout_revision()), (1, 1));
}

#[cfg(feature = "emoji-face")]
mod enabled {
    use super::*;
    use franken_markdown::dep_invalidation::{FlowSession, FlowShapeCache, FlowShapeCacheLimits};
    use franken_markdown::display::{DisplayItem, DisplayList, DisplayRect, DisplayTextRun};
    use franken_markdown::flow_display::FlowDisplayLimits;
    use franken_markdown::fonts::FontStyle;
    use franken_markdown::text::{CaretAffinity, Font, FontId, outline::Segment};
    use franken_markdown::{SourceSpan, parse_markdown_spanned};
    use std::collections::{BTreeMap, BTreeSet};
    use std::fmt::Write;

    fn options(width: f32) -> FlowLayoutOptions {
        FlowLayoutOptions {
            viewport_width: width,
            ..FlowLayoutOptions::default()
        }
    }

    fn session(source: &str, width: f32) -> FlowSession {
        let mut fonts =
            FlowShapeCache::new(FontFamily::Sans, FlowShapeCacheLimits::default()).unwrap();
        FlowSession::new(
            source,
            8,
            FlowDisplayLimits::default(),
            options(width),
            |text, size, role, style| fonts.shape(text, size, role, style),
        )
        .unwrap()
    }

    fn texts(display: &DisplayList) -> Vec<&DisplayTextRun> {
        display
            .items()
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn emoji() -> (FontId, Font) {
        let bytes = franken_markdown::fonts::emoji_bytes().unwrap();
        (
            FontId::from_font_data(bytes),
            Font::parse(bytes.to_vec()).unwrap(),
        )
    }

    #[test]
    fn actual_curated_emoji_outlines_advances_and_metrics_reach_every_text_style() {
        let bytes = franken_markdown::fonts::emoji_bytes().unwrap();
        let (emoji_id, reference) = emoji();
        assert!(reference.has_glyf_outlines());
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            for (role, style) in [
                (FlowTextRole::Body, FlowInlineStyle::default()),
                (FlowTextRole::Heading(1), FlowInlineStyle::default()),
                (FlowTextRole::TableHeader, FlowInlineStyle::default()),
                (
                    FlowTextRole::Body,
                    FlowInlineStyle {
                        bold: true,
                        italic: true,
                        ..FlowInlineStyle::default()
                    },
                ),
                (FlowTextRole::Code, FlowInlineStyle::default()),
            ] {
                let source = "A 🚀✅⚠✨🎉📦🙂👍❤🔥 Z";
                let run = fonts.shape(source, 17.0, role, style).unwrap();
                assert_eq!(run.logical_text, source);
                assert_ne!(run.context.font_id, emoji_id);
                let metrics = run.vertical_metrics.unwrap();
                assert!(metrics.is_valid());
                for cluster in &run.clusters {
                    let part = &source[cluster.byte_range.clone()];
                    if part.is_ascii() {
                        assert_eq!(cluster.font_id, run.context.font_id);
                        continue;
                    }
                    assert_eq!(part.chars().count(), 1);
                    let ch = part.chars().next().unwrap();
                    assert_eq!(cluster.font_id, emoji_id);
                    assert_eq!(cluster.glyph_range.len(), 1);
                    let glyph = &run.glyphs[cluster.glyph_range.start];
                    let gid = reference.glyph_index(ch);
                    assert_ne!(gid, 0);
                    assert_eq!((glyph.font_id, glyph.glyph_id), (emoji_id, gid));
                    assert_eq!(fonts.font_bytes(glyph.font_id), Some(bytes));
                    assert!(
                        (glyph.x_advance
                            - f32::from(reference.advance_width(gid)) * 17.0
                                / f32::from(reference.units_per_em))
                        .abs()
                            < 0.001
                    );
                    let outline = reference.glyph_outline(gid).unwrap();
                    assert!(!outline.contours.is_empty());
                    for contour in outline.contours {
                        let check = |y: f64| {
                            let y = y * 17.0 / f64::from(reference.units_per_em);
                            assert!(y <= f64::from(metrics.ascent) + 0.001);
                            assert!(y >= -f64::from(metrics.descent) - 0.001);
                        };
                        check(contour.start.y);
                        for segment in contour.segments {
                            if let Segment::Quad { ctrl, .. } = segment {
                                check(ctrl.y);
                            }
                            check(segment.to().y);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn standardized_variants_use_real_presentation_faces_and_one_complete_cluster() {
        let (emoji_id, reference) = emoji();
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            for style in [
                FlowInlineStyle::default(),
                FlowInlineStyle {
                    code: true,
                    ..FlowInlineStyle::default()
                },
            ] {
                for base in ['©', '®', '™', '↩', '⚠', '✅', '❤', '👍', '📦'] {
                    let source = format!("{base}\u{fe0f}");
                    let run = fonts
                        .shape(&source, 17.0, FlowTextRole::Body, style)
                        .unwrap();
                    assert_eq!(run.logical_text, source);
                    assert_eq!(run.clusters.len(), 1);
                    assert_eq!(
                        run.glyphs.len(),
                        1,
                        "selector must not draw an independent format glyph"
                    );
                    assert_eq!(run.clusters[0].byte_range, 0..source.len());
                    assert_eq!(
                        run.clusters[0].utf16_range,
                        0..source.encode_utf16().count()
                    );
                    assert_eq!(run.glyphs[0].font_id, emoji_id);
                    assert_eq!(run.glyphs[0].glyph_id, reference.glyph_index(base));
                    assert_eq!(
                        run.caret_at_byte(base.len_utf8(), CaretAffinity::Leading)
                            .unwrap()
                            .byte_offset,
                        0
                    );
                    assert_eq!(
                        run.caret_at_byte(base.len_utf8(), CaretAffinity::Trailing)
                            .unwrap()
                            .byte_offset,
                        source.len()
                    );
                    if base.len_utf16() == 2 {
                        assert!(run.caret_at_utf16(1, CaretAffinity::Leading).is_none());
                    }
                }
                // Text selectors require actual text-face glyphs; selecting
                // text cannot silently redraw the monochrome emoji fallback.
                for base in ['©', '®', '™', '↩'] {
                    let ordinary = fonts
                        .shape(&base.to_string(), 17.0, FlowTextRole::Body, style)
                        .unwrap();
                    let source = format!("{base}\u{fe0e}");
                    let text = fonts
                        .shape(&source, 17.0, FlowTextRole::Body, style)
                        .unwrap();
                    assert_eq!(text.glyphs, ordinary.glyphs);
                    assert_eq!(text.total_advance, ordinary.total_advance);
                    assert_ne!(text.glyphs[0].font_id, emoji_id);
                    assert_eq!(text.clusters[0].byte_range, 0..source.len());
                }
                assert!(
                    fonts
                        .shape("🚑\u{fe0e}", 17.0, FlowTextRole::Body, style)
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn existing_symbols_ligatures_combining_and_cjk_keep_their_actual_faces() {
        let (emoji_id, _) = emoji();
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            let old = fonts
                .shape(
                    "office AV © ® ™ ↩",
                    14.0,
                    FlowTextRole::Body,
                    FlowInlineStyle::default(),
                )
                .unwrap();
            assert!(old.glyphs.iter().all(|glyph| glyph.font_id != emoji_id));
            for source in [
                "e\u{0301}©\u{fe0f}A\u{030a}",
                "a\u{0301}©\u{fe0e}e\u{0301}",
                "🚀e\u{0301}⚠\u{fe0f}",
                "a\u{0301}®\u{fe0f}b",
            ] {
                let run = fonts
                    .shape(source, 14.0, FlowTextRole::Body, FlowInlineStyle::default())
                    .unwrap();
                assert_eq!(run.logical_text, source);
                let mut byte = 0;
                for cluster in &run.clusters {
                    assert_eq!(cluster.byte_range.start, byte);
                    assert_eq!(
                        cluster.utf16_range.start,
                        source[..byte].encode_utf16().count()
                    );
                    byte = cluster.byte_range.end;
                    assert_eq!(
                        cluster.utf16_range.end,
                        source[..byte].encode_utf16().count()
                    );
                    assert_eq!(
                        run.hit_test(cluster.x_start + cluster.advance() * 0.25)
                            .caret
                            .byte_offset,
                        cluster.byte_range.start
                    );
                }
                assert_eq!(byte, source.len());
                assert!(run.vertical_metrics.unwrap().is_valid());
            }
            #[cfg(feature = "cjk-face")]
            {
                let cjk_id = FontId::from_font_data(franken_markdown::fonts::cjk_bytes().unwrap());
                let source = "中文©\u{fe0f}e\u{0301}かな🚀";
                let run = fonts
                    .shape(source, 14.0, FlowTextRole::Body, FlowInlineStyle::default())
                    .unwrap();
                assert_eq!(run.logical_text, source);
                for cluster in &run.clusters {
                    let part = &source[cluster.byte_range.clone()];
                    let expected = match part {
                        "中" | "文" | "か" | "な" => cjk_id,
                        "©\u{fe0f}" | "🚀" => emoji_id,
                        "e\u{0301}" => run.context.font_id,
                        _ => panic!("unexpected cluster {part:?}"),
                    };
                    assert_eq!(cluster.font_id, expected);
                }
            }
            let expected_primary = FontId::from_font_data(franken_markdown::fonts::body_bytes(
                family,
                FontStyle::Regular,
            ));
            assert_eq!(old.context.font_id, expected_primary);
        }
    }

    const SOURCE: &str = concat!(
        "\u{feff}# 🚀 Release\r\n\r\n",
        "A **✅ ready** and *⚠\u{fe0f} review* with [📦 package](/guide) and `🙂 code`.\r\n\r\n",
        "> ©\u{fe0f} Quote.\r\n\r\n- 🎉 Shipped.\r\n\r\n",
        "| Status | Result |\r\n| --- | --- |\r\n| ✅ | 👍\u{fe0f} Done |\r\n\r\n",
        "```text\r\n🚀\tlaunch\r\n```\r\n\r\n",
        "⚠\u{fe0f} $\\frac{1}{2}$ e\u{0301} after.\r\n"
    );

    #[test]
    fn emoji_reaches_rich_blocks_links_code_and_math_with_measured_baselines() {
        let state = session(SOURCE, 400.0);
        let (emoji_id, _) = emoji();
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        for block in parse_markdown_spanned(SOURCE).blocks() {
            assert!(
                texts(state.display())
                    .into_iter()
                    .filter(|text| text.source_span == block.span)
                    .any(|text| text
                        .font_run
                        .as_ref()
                        .unwrap()
                        .glyphs
                        .iter()
                        .any(|glyph| glyph.font_id == emoji_id)),
                "emoji disappeared from a supported block"
            );
        }
        for text in texts(state.display()) {
            let baseline = text
                .baseline_y
                .expect("text with native metrics needs a native baseline");
            let run = text.font_run.as_ref().unwrap();
            for glyph in &run.glyphs {
                let font = Font::parse(fonts.font_bytes(glyph.font_id).unwrap().to_vec()).unwrap();
                let scale = text.font_size / f32::from(font.units_per_em);
                if let Some(bbox) = font.glyph_bbox(glyph.glyph_id) {
                    assert!(
                        baseline - f32::from(bbox[3]) * scale - glyph.y_offset
                            >= text.bounds.y - 0.002
                    );
                    assert!(
                        baseline - f32::from(bbox[1]) * scale - glyph.y_offset
                            <= text.bounds.bottom() + 0.002
                    );
                }
            }
            assert!(text.bounds.right() <= 400.002);
        }
        let marker = texts(state.display())
            .into_iter()
            .find(|text| text.text == "•")
            .unwrap();
        let body = texts(state.display())
            .into_iter()
            .find(|text| text.text.contains("🎉"))
            .unwrap();
        assert_eq!(marker.baseline_y, body.baseline_y);
        assert!(
            state
                .display()
                .anchors()
                .any(|anchor| anchor.anchor_id == "/guide")
        );
        assert!(
            state
                .display()
                .items()
                .iter()
                .any(|item| matches!(item, DisplayItem::Path(_)))
        );
        assert!(state.pending_assets().is_empty());
        assert_eq!(state.source(), SOURCE);
    }

    #[test]
    fn wrapping_and_bounded_work_never_separate_base_and_presentation_selector() {
        let word = "©\u{fe0f}👍\u{fe0f}✅\u{fe0f}🚀";
        let source = word.repeat(24);
        let mut state = session(&source, 300.0);
        let before = state.display().clone();
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        state
            .reflow(options(42.0), |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })
            .unwrap();
        let lines = texts(state.display());
        assert!(lines.len() > 24);
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<String>(),
            source
        );
        for line in lines {
            assert!(!line.text.starts_with('\u{fe0f}'));
            assert!(line.bounds.right() <= 42.002);
            for cluster in &line.font_run.as_ref().unwrap().clusters {
                let part = &line.text[cluster.byte_range.clone()];
                assert!(matches!(
                    part,
                    "©\u{fe0f}" | "👍\u{fe0f}" | "✅\u{fe0f}" | "🚀"
                ));
            }
        }
        state
            .reflow(options(300.0), |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })
            .unwrap();
        assert_eq!(state.display().items(), before.items());
        assert_eq!(state.display().reading_order(), before.reading_order());
        let long = "©\u{fe0f} e\u{0301} 👍\u{fe0f} ready ".repeat(800);
        assert!(long.len() > FlowLayoutOptions::default().max_shape_bytes);
        let expected = session(&long, 80.0);
        for window in [64, 128] {
            let bounded = FlowSession::new(
                &long,
                3,
                FlowDisplayLimits::default(),
                FlowLayoutOptions {
                    max_shape_bytes: window,
                    ..options(80.0)
                },
                |text, size, role, style| {
                    assert!(text.len() <= window);
                    fonts.shape(text, size, role, style)
                },
            )
            .unwrap();
            assert_eq!(bounded.display().items(), expected.display().items());
            assert_eq!(
                bounded.display().reading_order(),
                expected.display().reading_order()
            );
        }
    }

    #[test]
    fn presentation_pairs_count_every_source_scalar_against_existing_budgets() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let source = "©\u{fe0f}".repeat(8193);
        assert!(source.len() < 64 * 1024);
        let error = fonts
            .shape(
                &source,
                14.0,
                FlowTextRole::Body,
                FlowInlineStyle::default(),
            )
            .unwrap_err();
        assert!(error.contains("scalar budget"));
        let error = fonts
            .shape(
                &"🚀".repeat(16 * 1024 + 1),
                14.0,
                FlowTextRole::Body,
                FlowInlineStyle::default(),
            )
            .unwrap_err();
        assert!(error.contains("byte budget"));
    }

    #[test]
    fn browser_source_edits_selection_and_outlines_preserve_supplementary_characters() {
        let source = "A👍\u{fe0f}©\u{fe0f}🚀Z";
        let mut state = BrowserFlowSession::new(source, "sans", options(400.0)).unwrap();
        let native = session(source, 400.0);
        let (id, reference) = emoji();
        let glyphs: Vec<_> = "👍©🚀"
            .chars()
            .map(|ch| reference.glyph_index(ch))
            .collect();
        let wire = state.glyph_outlines_json(id.0, &glyphs).unwrap();
        let contours: usize = glyphs
            .iter()
            .map(|gid| reference.glyph_outline(*gid).unwrap().contours.len())
            .sum();
        assert_eq!(wire.matches("[\"M\",").count(), contours);
        assert_eq!(
            state.font_bytes(id.0).unwrap(),
            franken_markdown::fonts::emoji_bytes().unwrap()
        );
        for (index, item) in native.display().items().iter().enumerate() {
            let DisplayItem::Text(text) = item else {
                continue;
            };
            for cluster in &text.font_run.as_ref().unwrap().clusters {
                let x = text.bounds.x + cluster.x_start + cluster.advance() * 0.25;
                let y = text.bounds.y + text.bounds.height / 2.0;
                let hit = state.hit_test_json(1, 1, x, y).unwrap();
                assert!(hit.contains(&format!("\"utf16Offset\":{}", cluster.utf16_range.start)));
                let selection = state
                    .select_item_json(
                        1,
                        1,
                        index,
                        cluster.utf16_range.start,
                        cluster.utf16_range.end,
                    )
                    .unwrap();
                assert!(selection.contains(&format!(
                    "\"text\":{}",
                    quoted(&text.text[cluster.byte_range.clone()])
                )));
            }
        }
        assert_eq!(
            state
                .copy_source(1, SourceSpan::new(0, source.len()))
                .unwrap(),
            source
        );
        assert_eq!(
            state
                .edit_utf16(1, 2, 3, "x", FlowAssetReuse::Invalidate)
                .unwrap_err()
                .code(),
            "INVALID_SELECTION"
        );
        assert_eq!(state.source(), source);
        state
            .edit_utf16(1, 1, 4, "🎉", FlowAssetReuse::Invalidate)
            .unwrap();
        let edited = "A🎉©\u{fe0f}🚀Z";
        assert_eq!(state.source(), edited);
        let rebuilt = BrowserFlowSession::new(edited, "sans", options(400.0)).unwrap();
        let reading = state
            .reading_json(2, state.layout_revision(), 0, 2048)
            .unwrap();
        assert!(reading.contains(edited));
        assert!(
            rebuilt
                .reading_json(1, 1, 0, 2048)
                .unwrap()
                .contains(edited)
        );
        if let Some(path) = std::env::var_os("FMD_FLOW_EMOJI_FIXTURE") {
            let mut fixture = BrowserFlowSession::new(SOURCE, "sans", options(400.0)).unwrap();
            let original = payload(&fixture, SOURCE);
            let updated_source = "Updated 🚀⚠\u{fe0f} e\u{0301}.\n\n📦 $x^2$ and **✅ ready**.";
            fixture
                .replace_source(1, updated_source, FlowAssetReuse::Invalidate)
                .unwrap();
            let updated = payload(&fixture, updated_source);
            std::fs::write(path, format!("{{\"schema\":\"fmd-flow-emoji-native-v1\",\"emojiFontId\":\"{}\",\"original\":{original},\"updated\":{updated}}}\n", id.0)).unwrap();
        }
    }

    fn quoted(value: &str) -> String {
        let mut out = String::from("\"");
        for ch in value.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                ch if ch.is_control() => write!(out, "\\u{:04x}", ch as u32).unwrap(),
                ch => out.push(ch),
            }
        }
        out.push('"');
        out
    }

    fn payload(state: &BrowserFlowSession, source: &str) -> String {
        let native = session(source, 400.0);
        assert!(native.display().items().len() < 256);
        let revision = state.revision();
        let layout = state.layout_revision();
        let snapshot = state
            .snapshot_json(revision, layout, 0, 2048, true)
            .unwrap();
        let viewport = state
            .viewport_json(
                revision,
                layout,
                DisplayRect::new(0.0, 0.0, 400.0, 600.0),
                0,
                256,
                true,
            )
            .unwrap();
        let reading = state.reading_json(revision, layout, 0, 2048).unwrap();
        let mut by_font: BTreeMap<u64, BTreeSet<u16>> = BTreeMap::new();
        for text in texts(native.display()) {
            let run = text.font_run.as_ref().unwrap();
            by_font.entry(run.context.font_id.0).or_default();
            for glyph in &run.glyphs {
                by_font
                    .entry(glyph.font_id.0)
                    .or_default()
                    .insert(glyph.glyph_id);
            }
        }
        let outlines: Vec<_> = by_font
            .into_iter()
            .map(|(font, ids)| {
                state
                    .glyph_outlines_json(font, &ids.into_iter().collect::<Vec<_>>())
                    .unwrap()
            })
            .collect();
        let (emoji_id, _) = emoji();
        let (index, text, cluster) = native
            .display()
            .items()
            .iter()
            .enumerate()
            .find_map(|(index, item)| {
                let DisplayItem::Text(text) = item else {
                    return None;
                };
                text.font_run
                    .as_ref()
                    .unwrap()
                    .clusters
                    .iter()
                    .find(|cluster| cluster.font_id == emoji_id)
                    .map(|cluster| (index, text, cluster))
            })
            .unwrap();
        let x = text.bounds.x + cluster.x_start + cluster.advance() * 0.25;
        let y = text.bounds.y + text.bounds.height / 2.0;
        let hit = state.hit_test_json(revision, layout, x, y).unwrap();
        let selection = state
            .select_item_json(
                revision,
                layout,
                index,
                cluster.utf16_range.start,
                cluster.utf16_range.end,
            )
            .unwrap();
        format!(
            "{{\"source\":{},\"snapshot\":{snapshot},\"viewport\":{viewport},\"viewportRequest\":{{\"x\":0,\"y\":0,\"width\":400,\"height\":600,\"afterIndex\":0,\"limit\":256,\"glyphs\":true}},\"reading\":{reading},\"glyphOutlines\":[{}],\"hits\":[{{\"x\":{x},\"y\":{y},\"response\":{hit}}}],\"selections\":[{{\"itemIndex\":{index},\"startUtf16\":{},\"endUtf16\":{},\"response\":{selection}}}]}}",
            quoted(source),
            outlines.join(","),
            cluster.utf16_range.start,
            cluster.utf16_range.end
        )
    }
}
