//! Independent acceptance for the existing optional horizontal CJK face in
//! Flow: real glyphs, safe line boundaries, exact source, and browser carets.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::FontFamily;
use franken_markdown::flow_display::{FlowInlineStyle, FlowLayoutOptions, FlowTextRole};
use franken_markdown::fonts::BundledFlowFonts;

#[test]
fn unsupported_scripts_marks_variants_emoji_and_missing_glyphs_still_refuse() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for source in [
        "אב",
        "العربية",
        "한",
        "\u{20000}",
        "👩\u{200d}💻",
        "a\u{200d}b",
        "x\u{202e}y",
        "か\u{3099}",
        "\u{3099}か",
        "中\u{302a}",
        "\u{3031}",
        "\u{3032}",
        "\u{3033}",
        "\u{3034}",
        "\u{3035}",
        "\u{303b}",
        "中\u{fe00}",
        "中\u{e0100}",
        "ｶ",
        "ｶ\u{ff9e}",
        "\u{0301}a",
        "A\u{0301}\u{0308}",
        "\u{0378}",
    ] {
        assert!(
            fonts
                .shape(source, 14.0, FlowTextRole::Body, FlowInlineStyle::default())
                .is_err(),
            "optional CJK must not silently admit unsupported shaping: {source:?}"
        );
    }
}

#[test]
fn latin_long_words_and_insufficient_context_still_refuse_without_replacing_source() {
    use franken_markdown::dep_invalidation::{FlowAssetReuse, FlowSession, FlowSessionError};
    use franken_markdown::flow_display::{FlowDisplayLimits, FlowLayoutError};
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let mut state = FlowSession::new(
        "Before.",
        8,
        FlowDisplayLimits::default(),
        FlowLayoutOptions {
            viewport_width: 600.0,
            max_shape_bytes: 64,
            ..FlowLayoutOptions::default()
        },
        |text, size, role, style| fonts.shape(text, size, role, style),
    )
    .unwrap();
    let before = state.display().clone();
    for source in ["unbroken".repeat(200), "a ".repeat(2000)] {
        let error = state
            .replace_source(
                1,
                &source,
                FlowAssetReuse::Invalidate,
                |text, size, role, style| fonts.shape(text, size, role, style),
            )
            .unwrap_err();
        assert!(
            matches!(
                error,
                FlowSessionError::Layout(FlowLayoutError::BudgetExceeded(_))
            ),
            "CJK continuation must not invent Latin shaping context: {error}"
        );
        assert_eq!(state.source(), "Before.");
        assert_eq!(state.revision(), 1);
        assert_eq!(state.layout_revision(), 1);
        assert_eq!(state.display().items(), before.items());
        assert_eq!(state.display().reading_order(), before.reading_order());
    }
}

#[cfg(not(feature = "cjk-face"))]
#[test]
fn lean_flow_refuses_cjk_without_replacing_the_last_successful_document() {
    use franken_markdown::dep_invalidation::{BrowserFlowSession, FlowAssetReuse};
    assert!(franken_markdown::fonts::cjk_bytes().is_none());
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for source in ["中文", "東京", "かな", "カナ", "Ａ！", "中\u{3000}文"] {
        assert!(
            fonts
                .shape(source, 14.0, FlowTextRole::Body, FlowInlineStyle::default())
                .is_err()
        );
        assert!(BrowserFlowSession::new(source, "sans", FlowLayoutOptions::default()).is_err());
    }
    let mut state = BrowserFlowSession::new("café", "sans", FlowLayoutOptions::default()).unwrap();
    let before = state.snapshot_json(1, 1, 0, 2048, true).unwrap();
    assert!(
        state
            .replace_source(1, "東京", FlowAssetReuse::Invalidate)
            .is_err()
    );
    assert_eq!(state.source(), "café");
    assert_eq!(state.revision(), 1);
    assert_eq!(state.layout_revision(), 1);
    let after = state.snapshot_json(1, 1, 0, 2048, true).unwrap();
    assert_eq!(
        before.split("\"cache\"").next(),
        after.split("\"cache\"").next()
    );
}

#[cfg(feature = "cjk-face")]
mod enabled {
    use super::*;
    use franken_markdown::dep_invalidation::{
        BrowserFlowSession, FlowAssetReuse, FlowSession, FlowShapeCache, FlowShapeCacheLimits,
    };
    use franken_markdown::display::{DisplayItem, DisplayList, DisplayRect, DisplayTextRun};
    use franken_markdown::flow_display::FlowDisplayLimits;
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

    fn fonts() -> FlowShapeCache {
        FlowShapeCache::new(FontFamily::Sans, FlowShapeCacheLimits::default()).unwrap()
    }

    fn session(source: &str, width: f32) -> FlowSession {
        let mut fonts = fonts();
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

    #[test]
    fn mapped_han_kana_punctuation_and_space_use_real_optional_font_geometry() {
        let bytes = franken_markdown::fonts::cjk_bytes().unwrap();
        let reference = Font::parse(bytes.to_vec()).unwrap();
        let cjk_id = FontId::from_font_data(bytes);
        assert!(reference.has_glyf_outlines());
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            for style in [
                FlowInlineStyle::default(),
                FlowInlineStyle {
                    bold: true,
                    italic: true,
                    ..FlowInlineStyle::default()
                },
                FlowInlineStyle {
                    code: true,
                    ..FlowInlineStyle::default()
                },
            ] {
                let source = "A 中文東京かなカナ龍齉龘鬱「」。！？Ａ！中\u{3000}文 B";
                let run = fonts
                    .shape(source, 17.0, FlowTextRole::Body, style)
                    .unwrap();
                let primary =
                    Font::parse(fonts.font_bytes(run.context.font_id).unwrap().to_vec()).unwrap();
                assert_ne!(
                    run.context.font_id, cjk_id,
                    "Latin/code retains its selected primary face"
                );
                assert_eq!(run.logical_text, source);
                assert!(run.vertical_metrics.unwrap().is_valid());
                for cluster in &run.clusters {
                    let part = &source[cluster.byte_range.clone()];
                    if part.is_ascii() {
                        assert_ne!(cluster.font_id, cjk_id);
                        continue;
                    }
                    assert_eq!(
                        part.chars().count(),
                        1,
                        "this simple mapped CJK profile uses atomic scalar glyphs"
                    );
                    let ch = part.chars().next().unwrap();
                    let expected = reference.glyph_index(ch);
                    assert_ne!(expected, 0, "fixture requires a real mapped CJK character");
                    assert_eq!(
                        primary.glyph_index(ch),
                        0,
                        "fixture must exercise fallback rather than primary coverage"
                    );
                    assert_eq!(cluster.font_id, cjk_id);
                    assert_eq!(cluster.glyph_range.len(), 1);
                    let glyph = &run.glyphs[cluster.glyph_range.start];
                    assert_eq!(glyph.font_id, cjk_id);
                    assert_eq!(glyph.glyph_id, expected);
                    assert_eq!(fonts.font_bytes(glyph.font_id), Some(bytes));
                    let advance = f32::from(reference.advance_width(expected)) * 17.0
                        / f32::from(reference.units_per_em);
                    assert!((glyph.x_advance - advance).abs() < 0.001);
                    let outline = reference.glyph_outline(expected).unwrap();
                    assert_eq!(outline.contours.is_empty(), ch == '\u{3000}');
                    for contour in outline.contours {
                        for point in std::iter::once(contour.start)
                            .chain(contour.segments.iter().map(Segment::to))
                        {
                            assert!(point.x.is_finite() && point.y.is_finite());
                            let y = point.y * 17.0 / f64::from(reference.units_per_em);
                            let metrics = run.vertical_metrics.unwrap();
                            assert!(y <= f64::from(metrics.ascent) + 0.001);
                            assert!(y >= -f64::from(metrics.descent) - 0.001);
                        }
                    }
                }
            }
        }
        // A selected Unicode block is not a promise of a glyph at every scalar.
        assert_eq!(reference.glyph_index('\u{3040}'), 0);
        assert!(
            BundledFlowFonts::new(FontFamily::Sans)
                .unwrap()
                .shape(
                    "\u{3040}",
                    14.0,
                    FlowTextRole::Body,
                    FlowInlineStyle::default()
                )
                .is_err()
        );
    }

    #[test]
    fn mixed_latin_accents_and_cjk_preserve_original_cluster_and_caret_domains() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        for source in [
            "A中文éB",
            "中文 e\u{0301} 東京",
            "e\u{0301}中文",
            "中\u{3000}文",
        ] {
            let run = fonts
                .shape(source, 14.0, FlowTextRole::Body, FlowInlineStyle::default())
                .unwrap();
            assert_eq!(run.logical_text, source);
            let mut end = 0;
            for cluster in &run.clusters {
                assert_eq!(cluster.byte_range.start, end);
                assert_eq!(
                    cluster.utf16_range.start,
                    source[..cluster.byte_range.start].encode_utf16().count()
                );
                assert_eq!(
                    cluster.utf16_range.end,
                    source[..cluster.byte_range.end].encode_utf16().count()
                );
                let hit = run.hit_test(cluster.x_start + cluster.advance() * 0.25);
                assert_eq!(hit.caret.byte_offset, cluster.byte_range.start);
                assert_eq!(hit.caret.utf16_offset, cluster.utf16_range.start);
                for byte in cluster.byte_range.clone() {
                    if !source.is_char_boundary(byte) {
                        assert!(run.caret_at_byte(byte, CaretAffinity::Leading).is_none());
                    }
                }
                end = cluster.byte_range.end;
            }
            assert_eq!(end, source.len());
            for byte in source
                .char_indices()
                .map(|(byte, _)| byte)
                .chain([source.len()])
            {
                let caret = run.caret_at_byte(byte, CaretAffinity::Leading).unwrap();
                assert!(
                    run.clusters
                        .iter()
                        .any(|cluster| cluster.byte_range.start == caret.byte_offset
                            || cluster.byte_range.end == caret.byte_offset)
                );
                assert_eq!(
                    caret.utf16_offset,
                    source[..caret.byte_offset].encode_utf16().count()
                );
            }
        }
    }

    const SOURCE: &str = concat!(
        "\u{feff}# 中文 Guide\r\n\r\n",
        "English **中文** and *日本語* with [リンク](/guide) and `代码`.\r\n\r\n",
        "> 中文 quote.\r\n\r\n- 日本語 list.\r\n\r\n",
        "| 项目 | 値 |\r\n| --- | --- |\r\n| 内容 | 確認 |\r\n\r\n",
        "```text\r\n中文 = カナ\r\n```\r\n\r\n",
        "中文 $\\frac{1}{2}$ after.\r\n"
    );

    #[test]
    fn cjk_reaches_headings_lists_quotes_tables_code_and_math_without_losing_source() {
        let state = session(SOURCE, 300.0);
        let parsed = parse_markdown_spanned(SOURCE);
        let cjk_id = FontId::from_font_data(franken_markdown::fonts::cjk_bytes().unwrap());
        for block in parsed.blocks() {
            let owned: Vec<_> = texts(state.display())
                .into_iter()
                .filter(|text| text.source_span == block.span)
                .collect();
            assert!(
                owned.iter().any(|text| text
                    .font_run
                    .as_ref()
                    .unwrap()
                    .glyphs
                    .iter()
                    .any(|glyph| glyph.font_id == cjk_id)),
                "CJK was lost in a supported block"
            );
            assert_eq!(block.span.slice(state.source()), block.span.slice(SOURCE));
        }
        assert!(
            state
                .display()
                .items()
                .iter()
                .any(|item| matches!(item, DisplayItem::Path(_)))
        );
        assert!(
            state
                .display()
                .anchors()
                .any(|anchor| anchor.anchor_id == "/guide")
        );
        assert!(
            texts(state.display())
                .iter()
                .all(|text| text.bounds.right() <= 300.001)
        );
        assert!(state.pending_assets().is_empty());
        assert_eq!(state.source(), SOURCE);
        let mut browser = BrowserFlowSession::new(SOURCE, "sans", options(300.0)).unwrap();
        let before = browser.snapshot_json(1, 1, 0, 2048, true).unwrap();
        assert_eq!(
            browser
                .copy_source(1, SourceSpan::new(0, SOURCE.len()))
                .unwrap(),
            SOURCE
        );
        assert!(
            browser
                .replace_source(1, "中文 👩\u{200d}💻", FlowAssetReuse::Invalidate)
                .is_err()
        );
        assert_eq!(browser.source(), SOURCE);
        assert_eq!(browser.revision(), 1);
        assert_eq!(browser.layout_revision(), 1);
        assert_eq!(
            before.split("\"cache\"").next(),
            browser
                .snapshot_json(1, 1, 0, 2048, true)
                .unwrap()
                .split("\"cache\"")
                .next()
        );
    }

    #[test]
    fn narrow_cjk_reflow_and_byte_edits_retain_every_character_and_restore_geometry() {
        let source = "中文東京日本語かなカナ。".repeat(30);
        let mut state = session(&source, 400.0);
        let expected = state.display().clone();
        let mut fonts = fonts();
        state
            .reflow(options(55.0), |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })
            .unwrap();
        assert_eq!(
            texts(state.display())
                .iter()
                .map(|text| text.text.as_str())
                .collect::<String>(),
            source
        );
        assert_eq!(state.display().reading_order()[0].text, source);
        assert!(
            texts(state.display())
                .iter()
                .all(|text| text.bounds.right() <= 55.001)
        );
        assert!(state.display().total_bounds().height > expected.total_bounds().height);
        state
            .reflow(options(400.0), |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })
            .unwrap();
        assert_eq!(state.display().items(), expected.items());
        assert_eq!(state.display().reading_order(), expected.reading_order());
        state
            .edit(
                1,
                SourceSpan::new(0, "中文".len()),
                "日本語",
                FlowAssetReuse::Invalidate,
                |text, size, role, style| fonts.shape(text, size, role, style),
            )
            .unwrap();
        let edited = format!("日本語{}", &source["中文".len()..]);
        assert_eq!(state.source(), edited);
        assert_eq!(
            state.display().items(),
            session(&edited, 400.0).display().items()
        );
        let before = state.display().clone();
        assert!(
            state
                .edit(
                    2,
                    SourceSpan::new(1, 2),
                    "x",
                    FlowAssetReuse::Invalidate,
                    |text, size, role, style| fonts.shape(text, size, role, style)
                )
                .is_err()
        );
        assert_eq!(state.source(), edited);
        assert_eq!(state.display().items(), before.items());
        assert_eq!(state.revision(), 2);
    }

    #[test]
    fn long_unspaced_cjk_and_mixed_math_keep_identical_geometry_across_shaping_windows() {
        let word = "中文東京かなカナ日本語";
        let prose = word.repeat(600);
        let table = format!("| ID | 説明 |\n| --- | --- |\n| 7 | {} |", word.repeat(600));
        let mixed = format!("{}$x^2$", word.repeat(30)).repeat(20);
        for (name, source, width) in [
            ("prose", prose, 72.0),
            ("table", table, 125.0),
            ("mixed math", mixed, 85.0),
        ] {
            assert!(source.len() > FlowLayoutOptions::default().max_shape_bytes);
            let mut fonts = fonts();
            let expected = FlowSession::new(
                &source,
                64,
                FlowDisplayLimits::default(),
                FlowLayoutOptions {
                    max_shape_bytes: source.len(),
                    ..options(width)
                },
                |text, size, role, style| fonts.shape(text, size, role, style),
            )
            .unwrap();
            if name == "mixed math" {
                assert!(
                    expected
                        .display()
                        .items()
                        .iter()
                        .any(|item| matches!(item, DisplayItem::Path(_)))
                );
            }
            for window in [64, 128, FlowLayoutOptions::default().max_shape_bytes] {
                let mut calls = 0;
                let actual = FlowSession::new(
                    &source,
                    3,
                    FlowDisplayLimits::default(),
                    FlowLayoutOptions {
                        max_shape_bytes: window,
                        ..options(width)
                    },
                    |text, size, role, style| {
                        assert!(
                            text.len() <= window,
                            "{name}: per-call shaping limit escaped"
                        );
                        calls += 1;
                        fonts.shape(text, size, role, style)
                    },
                )
                .unwrap_or_else(|error| panic!("{name}, window={window}: {error}"));
                assert!(calls > 0);
                assert_eq!(actual.source(), source);
                assert!(
                    actual.display().reading_order() == expected.display().reading_order(),
                    "{name}, window={window}: work boundary changed reading text or geometry"
                );
                assert_eq!(
                    actual.display().items().len(),
                    expected.display().items().len(),
                    "{name}, window={window}: work boundary changed final fragments"
                );
                for (index, (actual, expected)) in actual
                    .display()
                    .items()
                    .iter()
                    .zip(expected.display().items())
                    .enumerate()
                {
                    assert_eq!(
                        actual, expected,
                        "{name}, window={window}, item={index}: actual glyph/path placement changed"
                    );
                }
                assert_eq!(
                    actual.display().total_bounds(),
                    expected.display().total_bounds()
                );
            }
        }
    }

    #[test]
    fn cjk_wraps_preserve_opening_closing_and_nonstarter_pairs() {
        // Explicit independent punctuation controls, not expected results
        // generated by the production line-boundary helper.
        let source =
            "中文（東京）かな、カナ。『漢字』【語文】「中文」あぁ語カァ語々語ー日本語！".repeat(5);
        let opens = "（［｛〈《「『【〔〖〘〚";
        let closes =
            "、。，．？！：；）］｝〉》」』】〕〗〙〛々ーぁぃぅぇぉゃゅょっァィゥェォャュョッ";
        for width in [45.0, 66.0, 95.0] {
            let state = session(&source, width);
            let lines = texts(state.display());
            assert!(lines.len() > 2, "fixture must exercise actual wraps");
            assert_eq!(
                lines
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<String>(),
                source
            );
            for pair in lines.windows(2) {
                let left = pair[0].text.chars().last().unwrap();
                let right = pair[1].text.chars().next().unwrap();
                assert!(
                    !opens.contains(left),
                    "opening punctuation stranded at line end: {left}"
                );
                assert!(
                    !closes.contains(right),
                    "closing/nonstarter punctuation stranded at line start: {right}"
                );
                assert!(pair[0].bounds.bottom() <= pair[1].bounds.y + 0.001);
            }
            assert!(
                lines
                    .iter()
                    .all(|line| line.bounds.right() <= width + 0.001)
            );
        }
    }

    #[test]
    fn cjk_text_code_tables_and_markers_share_measured_baselines_and_contain_real_ink() {
        let source = concat!(
            "# 中文 title\n\n",
            "中文 **office** *italic* e\u{0301} 語文。\n\n",
            "- 中文 **office** marker.\n\n",
            "7. 東京 e\u{0301} marker.\n\n",
            "| 中文 | Latin |\n| --- | --- |\n| 語文 e\u{0301} | office |\n\n",
            "```text\n中文\tcode\n東京 second\n```"
        );
        let cjk_id = FontId::from_font_data(franken_markdown::fonts::cjk_bytes().unwrap());
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            let state = FlowSession::new(
                source,
                8,
                FlowDisplayLimits::default(),
                options(300.0),
                |text, size, role, style| fonts.shape(text, size, role, style),
            )
            .unwrap();
            let all = texts(state.display());
            let mut decoded = BTreeMap::<u64, Font>::new();
            let mut cjk_lines = 0;
            for text in &all {
                let run = text.font_run.as_ref().unwrap();
                let has_cjk = run.glyphs.iter().any(|glyph| glyph.font_id == cjk_id);
                let marker = text.text == "•" || text.text == "7.";
                if !(has_cjk || marker) {
                    continue;
                }
                cjk_lines += usize::from(has_cjk);
                let baseline = text
                    .baseline_y
                    .expect("actual mixed-face line owns its baseline");
                assert!(baseline >= text.bounds.y && baseline <= text.bounds.bottom());
                let metrics = run.vertical_metrics.unwrap();
                assert!(metrics.is_valid());
                assert!(text.bounds.height + 0.001 >= metrics.ascent + metrics.descent);
                for cluster in &run.clusters {
                    let mut pen = cluster.x_start;
                    for glyph in &run.glyphs[cluster.glyph_range.clone()] {
                        let font = decoded.entry(glyph.font_id.0).or_insert_with(|| {
                            Font::parse(fonts.font_bytes(glyph.font_id).unwrap().to_vec()).unwrap()
                        });
                        let scale = f64::from(text.font_size) / f64::from(font.units_per_em);
                        let outline = font.glyph_outline(glyph.glyph_id).unwrap();
                        let check = |point: franken_markdown::text::outline::Point| {
                            let x =
                                f64::from(text.bounds.x + pen + glyph.x_offset) + point.x * scale;
                            let y = f64::from(baseline - glyph.y_offset) - point.y * scale;
                            assert!(
                                y >= f64::from(text.bounds.y) - 0.002,
                                "glyph ink escapes line top"
                            );
                            assert!(
                                y <= f64::from(text.bounds.bottom()) + 0.002,
                                "glyph ink escapes line bottom"
                            );
                            if glyph.font_id == cjk_id {
                                assert!(x >= f64::from(text.bounds.x) - 0.002);
                                assert!(x <= f64::from(text.bounds.right()) + 0.002);
                            }
                        };
                        for contour in &outline.contours {
                            check(contour.start);
                            for segment in &contour.segments {
                                match *segment {
                                    Segment::Line { to } => check(to),
                                    Segment::Quad { ctrl, to } => {
                                        check(ctrl);
                                        check(to);
                                    }
                                }
                            }
                        }
                        pen += glyph.x_advance;
                    }
                }
                if marker {
                    let body = all
                        .iter()
                        .copied()
                        .filter(|candidate| {
                            candidate.source_span == text.source_span && candidate.text != text.text
                        })
                        .min_by(|a, b| a.bounds.y.total_cmp(&b.bounds.y))
                        .unwrap();
                    assert_eq!(
                        text.baseline_y, body.baseline_y,
                        "list marker must share its first body's actual baseline"
                    );
                    assert!(text.bounds.right() < body.bounds.x);
                }
            }
            assert!(
                cjk_lines >= 8,
                "heading/prose/lists/table/code must all exercise actual CJK"
            );
            assert_eq!(all.iter().filter(|text| text.text == "•").count(), 1);
            assert_eq!(all.iter().filter(|text| text.text == "7.").count(), 1);
            assert_eq!(state.source(), source);
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
        let layout_revision = state.layout_revision();
        let snapshot = state
            .snapshot_json(revision, layout_revision, 0, 2048, true)
            .unwrap();
        let viewport = state
            .viewport_json(
                revision,
                layout_revision,
                DisplayRect::new(0.0, 0.0, 400.0, 600.0),
                0,
                256,
                true,
            )
            .unwrap();
        let reading = state
            .reading_json(revision, layout_revision, 0, 2048)
            .unwrap();
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
        let cjk_id = FontId::from_font_data(franken_markdown::fonts::cjk_bytes().unwrap());
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
                    .find(|cluster| cluster.font_id == cjk_id)
                    .map(|cluster| (index, text, cluster))
            })
            .unwrap();
        let x = text.bounds.x + cluster.x_start + cluster.advance() * 0.25;
        let y = text.bounds.y + text.bounds.height / 2.0;
        let hit = state
            .hit_test_json(revision, layout_revision, x, y)
            .unwrap();
        let selection = state
            .select_item_json(
                revision,
                layout_revision,
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

    #[test]
    fn native_browser_exports_actual_cjk_outlines_reading_and_fragment_carets() {
        let source = "A中文éB";
        let state = BrowserFlowSession::new(source, "sans", options(400.0)).unwrap();
        let native = session(source, 400.0);
        let reference =
            Font::parse(franken_markdown::fonts::cjk_bytes().unwrap().to_vec()).unwrap();
        let id = FontId::from_font_data(franken_markdown::fonts::cjk_bytes().unwrap());
        let glyphs: Vec<_> = "中文".chars().map(|ch| reference.glyph_index(ch)).collect();
        let wire = state.glyph_outlines_json(id.0, &glyphs).unwrap();
        assert_eq!(
            state.font_bytes(id.0).unwrap(),
            franken_markdown::fonts::cjk_bytes().unwrap()
        );
        assert!(wire.contains(&format!("\"fontId\":\"{}\"", id.0)));
        let expected: Vec<_> = glyphs
            .iter()
            .map(|id| reference.glyph_outline(*id).unwrap())
            .collect();
        assert_eq!(
            wire.matches("[\"M\",").count(),
            expected
                .iter()
                .map(|outline| outline.contours.len())
                .sum::<usize>()
        );
        let quads = expected
            .iter()
            .flat_map(|outline| &outline.contours)
            .flat_map(|contour| &contour.segments)
            .filter(|segment| matches!(segment, Segment::Quad { .. }))
            .count();
        assert_eq!(wire.matches("[\"Q\",").count(), quads);
        assert!(wire.contains("[\"L\",") || wire.contains("[\"Q\","));
        for (index, item) in native.display().items().iter().enumerate() {
            let DisplayItem::Text(text) = item else {
                continue;
            };
            let run = text.font_run.as_ref().unwrap();
            for cluster in &run.clusters {
                let x = text.bounds.x + cluster.x_start + cluster.advance() * 0.25;
                let y = text.bounds.y + text.bounds.height / 2.0;
                let hit = state.hit_test_json(1, 1, x, y).unwrap();
                assert!(hit.contains("\"coordinateSpace\":\"fragment-utf16\""));
                assert!(hit.contains(&format!("\"byteOffset\":{}", cluster.byte_range.start)));
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
        assert!(state.reading_json(1, 1, 0, 2048).unwrap().contains(source));
        assert!(
            state
                .pending_assets_json(1, 0, 2048)
                .unwrap()
                .contains("\"requests\":[]")
        );
        if let Some(path) = std::env::var_os("FMD_FLOW_CJK_FIXTURE") {
            let mut fixture = BrowserFlowSession::new(SOURCE, "sans", options(400.0)).unwrap();
            let original = payload(&fixture, SOURCE);
            let updated_source = "Updated 中文、かなカナ。\n\n日本語 $x^2$ and **bold**.";
            fixture
                .replace_source(1, updated_source, FlowAssetReuse::Invalidate)
                .unwrap();
            let updated = payload(&fixture, updated_source);
            std::fs::write(path, format!("{{\"schema\":\"fmd-flow-cjk-native-v1\",\"original\":{original},\"updated\":{updated}}}\n")).unwrap();
        }
    }
}
