#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::display::AccessibleReadingRole;
use crate::text::{Direction, FontId, FontOrigin, TextRunContext};

// Deterministic geometry oracle, not a claim about real font rendering.
fn shape(text: &str, size: f32, _: FlowTextRole, _: FlowInlineStyle) -> Result<OwnedTextRun, String> {
    let mut run = OwnedTextRun {
        context: TextRunContext {
            font_id: FontId::new(1), font_size: size, script: *b"latn", language: *b"dflt",
            direction: Direction::LeftToRight, font_origin: FontOrigin::BundledFace,
        },
        logical_text: text.into(), clusters: Vec::new(), glyphs: Vec::new(), total_advance: 0.0,
    };
    let mut chars = text.char_indices().peekable();
    let mut utf16 = 0;
    while let Some((byte, ch)) = chars.next() {
        let mut end = byte + ch.len_utf8();
        if ch == 'f' && chars.peek().is_some_and(|(_, ch)| *ch == 'i') {
            let (byte, ch) = chars.next().unwrap();
            end = byte + ch.len_utf8();
        }
        while chars.peek().is_some_and(|(_, ch)| ('\u{0300}'..='\u{036f}').contains(ch)) {
            let (byte, ch) = chars.next().unwrap();
            end = byte + ch.len_utf8();
        }
        let index = run.clusters.len();
        let units = text[byte..end].encode_utf16().count();
        run.clusters.push(TextCluster {
            cluster_index: index, byte_range: byte..end, utf16_range: utf16..utf16 + units,
            glyph_range: index..index + 1, x_start: run.total_advance,
            x_end: run.total_advance + 10.0, font_id: FontId::new(1),
        });
        run.glyphs.push(RunGlyph {
            glyph_id: 1, font_id: FontId::new(1), cluster_index: index,
            x_advance: 10.0, y_advance: 0.0, x_offset: 0.0, y_offset: 0.0,
        });
        run.total_advance += 10.0;
        utf16 += units;
    }
    Ok(run)
}

fn engine(source: &str, batch: usize) -> ResumableFlowDisplay {
    let mut engine = ResumableFlowDisplay::new(source, batch);
    engine.process_all().unwrap();
    engine
}

fn options(width: f32) -> FlowLayoutOptions {
    FlowLayoutOptions { viewport_width: width, ..FlowLayoutOptions::default() }
}

fn texts(list: &DisplayList) -> Vec<&DisplayTextRun> {
    list.items().iter().filter_map(|item| match item {
        DisplayItem::Text(text) => Some(text), _ => None,
    }).collect()
}

fn cells(list: &DisplayList) -> Vec<(String, f32, f32, f32, FontId, SourceSpan)> {
    let mut out = Vec::new();
    for text in texts(list) {
        let run = text.font_run.as_ref().unwrap();
        let mut logical: Vec<_> = run.clusters.iter().collect();
        logical.sort_by_key(|cluster| cluster.byte_range.start);
        for cluster in logical {
            out.push((
                run.logical_text[cluster.byte_range.clone()].to_owned(),
                text.bounds.x + cluster.x_start, text.bounds.y,
                cluster.advance(), cluster.font_id, text.source_span,
            ));
        }
    }
    out
}

fn validate_fragments(list: &DisplayList) {
    for text in texts(list) {
        let run = text.font_run.as_ref().unwrap();
        assert_eq!(run.logical_text, text.text);
        assert_eq!(run.total_advance, text.bounds.width);
        let mut logical: Vec<_> = run.clusters.iter().collect();
        logical.sort_by_key(|cluster| cluster.byte_range.start);
        let (mut byte, mut utf16, mut glyphs) = (0, 0, 0);
        for cluster in logical {
            assert_eq!(cluster.byte_range.start, byte);
            assert_eq!(cluster.utf16_range.start, utf16);
            assert_eq!(&run.clusters[cluster.cluster_index], cluster);
            byte = cluster.byte_range.end;
            utf16 += text.text[cluster.byte_range.clone()].encode_utf16().count();
            assert_eq!(cluster.utf16_range.end, utf16);
            for glyph in &run.glyphs[cluster.glyph_range.clone()] {
                assert_eq!(glyph.cluster_index, cluster.cluster_index);
                assert_eq!(glyph.font_id, cluster.font_id);
                glyphs += 1;
            }
        }
        assert_eq!(byte, text.text.len());
        assert_eq!(glyphs, run.glyphs.len());
        assert!(run.hit_test(0.0).caret.byte_offset <= text.text.len());
    }
}

#[test]
fn whole_fence_lexer_preserves_multiline_state_and_wrapped_geometry() {
    let source = "# Before\n\n```rust\n/* first\nlet hidden = 9; */\nlet café = \"fi a\u{0301} 😀\";\n\t// tail\n```\n\n[after](#before)";
    let engine = engine(source, 1);
    let plain = engine.to_styled_display_list(options(80.0), shape).unwrap();
    let colored = engine.to_highlighted_display_list(options(80.0), shape).unwrap();
    assert_eq!(plain.reading_order(), colored.reading_order());
    assert_eq!(cells(&plain), cells(&colored));
    assert_eq!(plain.anchors().collect::<Vec<_>>(), colored.anchors().collect::<Vec<_>>());
    let comments: String = texts(&colored).iter().filter(|text| text.color_role == "tok-cm")
        .map(|text| text.text.as_str()).collect();
    assert!(comments.contains("let hidden = 9; */"));
    assert!(texts(&colored).iter().any(|text| text.color_role == "tok-kw" && text.text == "let"));
    assert!(texts(&colored).iter().any(|text| text.color_role == "tok-st"));
    validate_fragments(&colored);
}

#[test]
fn unknown_and_missing_languages_preserve_the_entire_original_output() {
    for source in ["plain **text**", "```not-a-language\nfn f() {}\n```", "```\nfn f() {}\n```", "```rust\n\n\n```"] {
        let engine = engine(source, 2);
        assert_eq!(
            engine.to_highlighted_display_list(options(120.0), shape).unwrap(),
            engine.to_styled_display_list(options(120.0), shape).unwrap(),
        );
    }
}

#[test]
fn fence_identity_does_not_depend_on_searching_repeated_source_text() {
    let source = "> ```rust\n> let x = 7;\n> ```\n>\n> ```text\n> let x = 7;\n> ```\n\n```rust\nlet x = 7;\n```";
    let engine = engine(source, 1);
    let list = engine.to_highlighted_display_list(options(240.0), shape).unwrap();
    let keywords = texts(&list).into_iter().filter(|run| run.color_role == "tok-kw").count();
    assert_eq!(keywords, 2);
    assert_eq!(list.reading_order().iter().filter(|node| node.role == AccessibleReadingRole::CodeBlock).count(), 3);
    assert_eq!(cells(&list), cells(&engine.to_styled_display_list(options(240.0), shape).unwrap()));
}

#[test]
fn span_admission_precedes_shaping_and_final_item_budget_is_atomic() {
    let engine = engine("```rust\nlet x = 1;\n```", 8);
    let original = engine.to_display_list();
    let mut calls = 0;
    let result = engine.to_highlighted_display_list(FlowLayoutOptions {
        max_items: 2, ..options(200.0)
    }, |text, size, role, style| {
        calls += 1;
        shape(text, size, role, style)
    });
    assert!(matches!(result, Err(FlowLayoutError::BudgetExceeded("syntax source bytes per fence"))));
    assert_eq!(calls, 0);
    // Exercise post-lexing admission independently of the conservative ingress guard.
    let plain = engine.to_styled_display_list(options(200.0), shape).unwrap();
    assert!(matches!(paint(engine.blocks(), plain, 2), Err(FlowLayoutError::BudgetExceeded("highlighted display items"))));
    assert_eq!(engine.to_display_list(), original);
    assert!(engine.to_highlighted_display_list(options(200.0), shape).is_ok());
}

#[test]
fn shaping_failures_propagate_and_success_does_not_add_shaper_calls() {
    let engine = engine("```rust\nlet x = 1;\n```", 8);
    assert!(matches!(engine.to_highlighted_display_list(options(200.0), |_, _, _, _| Err("no face".into())), Err(FlowLayoutError::Shaping(_))));
    let mut plain_calls = Vec::new();
    engine.to_styled_display_list(options(80.0), |text, size, role, style| {
        plain_calls.push(text.to_owned()); shape(text, size, role, style)
    }).unwrap();
    let mut color_calls = Vec::new();
    engine.to_highlighted_display_list(options(80.0), |text, size, role, style| {
        color_calls.push(text.to_owned()); shape(text, size, role, style)
    }).unwrap();
    assert_eq!(plain_calls, color_calls);
}

#[test]
fn highlighting_is_independent_of_source_batches_and_measurement_windows() {
    let source = format!("```rust\n{}\n```", "let café = 42; ".repeat(100));
    let expected = engine(&source, 128).to_highlighted_display_list(options(90.0), shape).unwrap();
    for batch in [1, 3, 128] {
        let engine = engine(&source, batch);
        let actual = engine.to_highlighted_display_list(FlowLayoutOptions {
            max_shape_bytes: 64, ..options(90.0)
        }, shape).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn token_boundaries_never_split_a_host_ligature() {
    let engine = engine("```rust\nlet x=1;\n```", 8);
    let indivisible = |text: &str, size, role, style| {
        let mut run = shape(text, size, role, style)?;
        if !text.is_empty() {
            run.clusters.truncate(1);
            run.clusters[0].byte_range = 0..text.len();
            run.clusters[0].utf16_range = 0..text.encode_utf16().count();
            run.glyphs.truncate(1);
            run.total_advance = 10.0;
        }
        Ok::<_, String>(run)
    };
    let list = engine.to_highlighted_display_list(options(200.0), indivisible).unwrap();
    assert_eq!(texts(&list).len(), 1);
    assert_eq!(texts(&list)[0].text, "let x=1;");
    assert_eq!(texts(&list)[0].color_role, "tok-kw");
    validate_fragments(&list);
}

#[test]
fn rtl_cluster_positions_and_fragment_local_indices_remain_exact() {
    let engine = engine("```rust\nlet x=42;\n```", 8);
    let rtl = |text: &str, size, role, style| {
        let mut run = shape(text, size, role, style)?;
        run.context.direction = Direction::RightToLeft;
        for cluster in &mut run.clusters {
            let start = cluster.x_start;
            cluster.x_start = run.total_advance - cluster.x_end;
            cluster.x_end = run.total_advance - start;
        }
        Ok::<_, String>(run)
    };
    let plain = engine.to_styled_display_list(options(200.0), rtl).unwrap();
    let colored = engine.to_highlighted_display_list(options(200.0), rtl).unwrap();
    assert_eq!(cells(&plain), cells(&colored));
    validate_fragments(&colored);
    assert!(texts(&colored).iter().all(|text| text.font_run.as_ref().unwrap().context.direction == Direction::RightToLeft));
}

#[test]
fn real_bundled_faces_keep_fonts_glyphs_and_positioned_marks() {
    let fonts = crate::fonts::BundledFlowFonts::new(crate::theme::FontFamily::Sans).unwrap();
    let engine = engine("```rust\nlet x = \"a\u{0301} fi\";\n```", 8);
    let shape = |text: &str, size, role, style| fonts.shape(text, size, role, style);
    let plain = engine.to_styled_display_list(options(500.0), shape).unwrap();
    let colored = engine.to_highlighted_display_list(options(500.0), shape).unwrap();
    assert_eq!(plain.reading_order(), colored.reading_order());
    let a = cells(&plain);
    let b = cells(&colored);
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(&b) {
        assert_eq!((&a.0, a.4, a.5), (&b.0, b.4, b.5));
        assert!((a.1 - b.1).abs() < 0.001);
        assert_eq!(a.2, b.2);
        assert!((a.3 - b.3).abs() < 0.001);
    }
    let glyphs = |list: &DisplayList| {
        texts(list).iter().flat_map(|text| text.font_run.as_ref().unwrap().glyphs.iter())
            .map(|glyph| (glyph.glyph_id, glyph.font_id, glyph.x_advance, glyph.y_advance, glyph.x_offset, glyph.y_offset))
            .collect::<Vec<_>>()
    };
    assert_eq!(glyphs(&plain), glyphs(&colored));
    validate_fragments(&colored);
}
