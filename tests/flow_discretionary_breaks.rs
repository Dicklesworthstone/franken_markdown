//! Authored line breaks through the public Flow and browser APIs. Visible
//! glyphs are checked against real bundled-font shaping while reading text,
//! source clusters, and browser selections retain the original controls.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::dep_invalidation::BrowserFlowSession;
use franken_markdown::display::{DisplayItem, DisplayList, DisplayTextRun, VectorShapeType};
use franken_markdown::flow_display::{
    FlowInlineStyle, FlowLayoutError, FlowLayoutOptions, FlowTextRole, ResumableFlowDisplay,
};
use franken_markdown::fonts::BundledFlowFonts;
use franken_markdown::text::{CaretAffinity, OwnedTextRun};
use franken_markdown::{FontFamily, SourceSpan};

const SHY: char = '\u{00ad}';
const ZWSP: char = '\u{200b}';

fn options(width: f32) -> FlowLayoutOptions {
    FlowLayoutOptions {
        viewport_width: width,
        ..FlowLayoutOptions::default()
    }
}

fn engine(source: &str) -> ResumableFlowDisplay {
    let mut engine = ResumableFlowDisplay::new(source, 2);
    engine.process_all().unwrap();
    engine
}

fn render(source: &str, width: f32, styled: bool) -> DisplayList {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let engine = engine(source);
    if styled {
        fonts.render(&engine, options(width)).unwrap()
    } else {
        engine
            .to_shaped_display_list(options(width), |text, size, role| {
                fonts.shape(text, size, role, FlowInlineStyle::default())
            })
            .unwrap()
    }
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

fn copied_text(display: &DisplayList) -> String {
    texts(display).iter().map(|text| text.text.as_str()).collect()
}

fn without_hints(source: &str) -> String {
    source.chars().filter(|ch| !matches!(*ch, SHY | ZWSP)).collect()
}

fn shaped(visible: &str, style: FlowInlineStyle) -> OwnedTextRun {
    BundledFlowFonts::new(FontFamily::Sans)
        .unwrap()
        .shape(visible, 14.0, FlowTextRole::Body, style)
        .unwrap()
}

fn assert_visible(run: &OwnedTextRun, expected: &OwnedTextRun) {
    assert_eq!(run.context, expected.context);
    assert_eq!(run.vertical_metrics, expected.vertical_metrics);
    assert_eq!(run.total_advance, expected.total_advance);
    assert_eq!(run.glyphs.len(), expected.glyphs.len());
    for (actual, expected) in run.glyphs.iter().zip(&expected.glyphs) {
        // Source clusters may absorb invisible controls. Font identities and
        // actual positioned glyphs must still be precisely the visible text.
        assert_eq!(actual.font_id, expected.font_id);
        assert_eq!(actual.glyph_id, expected.glyph_id);
        assert_eq!(actual.x_advance, expected.x_advance);
        assert_eq!(actual.y_advance, expected.y_advance);
        assert_eq!(actual.x_offset, expected.x_offset);
        assert_eq!(actual.y_offset, expected.y_offset);
    }
}

fn assert_source_clusters(text: &DisplayTextRun) {
    let run = text.font_run.as_ref().unwrap();
    assert_eq!(run.logical_text, text.text);
    let mut clusters: Vec<_> = run.clusters.iter().collect();
    clusters.sort_by_key(|cluster| cluster.byte_range.start);
    let mut end = 0;
    for cluster in clusters {
        assert_eq!(cluster.byte_range.start, end);
        assert!(cluster.byte_range.end > end);
        assert!(text.text.get(cluster.byte_range.clone()).is_some());
        assert_eq!(
            cluster.utf16_range.start,
            text.text[..cluster.byte_range.start].encode_utf16().count(),
        );
        assert_eq!(
            cluster.utf16_range.end,
            text.text[..cluster.byte_range.end].encode_utf16().count(),
        );
        assert!(run.glyphs.get(cluster.glyph_range.clone()).is_some());
        if cluster.advance() > 0.0 {
            let hit = run.hit_test(cluster.x_start + cluster.advance() * 0.25);
            assert_eq!(hit.caret.byte_offset, cluster.byte_range.start);
            assert_eq!(hit.caret.utf16_offset, cluster.utf16_range.start);
        }
        end = cluster.byte_range.end;
    }
    assert_eq!(end, text.text.len(), "every authored byte keeps an owner");
    for byte in 0..=text.text.len() {
        let caret = run.caret_at_byte(byte, CaretAffinity::Leading);
        if !text.text.is_char_boundary(byte) {
            assert!(caret.is_none(), "no caret inside a UTF-8 scalar");
            continue;
        }
        let caret = caret.unwrap();
        assert_eq!(
            caret.utf16_offset,
            text.text[..caret.byte_offset].encode_utf16().count(),
        );
        assert_eq!(
            run.caret_at_utf16(
                text.text[..byte].encode_utf16().count(),
                CaretAffinity::Leading,
            ),
            Some(caret),
        );
    }
}

fn assert_hyphen_owned_by_hint(text: &DisplayTextRun) {
    let run = text.font_run.as_ref().unwrap();
    let glyph = run.glyphs.last().unwrap();
    let cluster = &run.clusters[glyph.cluster_index];
    assert!(text.text[cluster.byte_range.clone()].contains(SHY));
}

#[test]
fn unselected_hints_preserve_real_glyphs_ligatures_marks_and_exact_source() {
    let mut sources = vec![
        "co\u{00ad}operate",
        "of\u{200b}fice",
        "\u{00ad}é\u{200b}lan\u{00ad}",
        "a\u{00ad}\u{0301}bc",
        "a\u{200b}\u{0301}bc",
        "ban\u{200b}\u{00ad}\u{200b}ana",
    ];
    if cfg!(feature = "emoji-face") {
        sources.push("é\u{00ad}😀\u{200b}a");
    }
    for source in sources {
        for styled in [false, true] {
            let display = render(source, 800.0, styled);
            let lines = texts(&display);
            assert_eq!(lines.len(), 1, "{source:?}");
            assert_eq!(lines[0].text, source);
            assert_eq!(display.reading_order()[0].text, source);
            assert_visible(
                lines[0].font_run.as_ref().unwrap(),
                &shaped(&without_hints(source), FlowInlineStyle::default()),
            );
            assert_source_clusters(lines[0]);
        }
    }
}

#[test]
fn selected_soft_hyphens_paint_hyphens_and_zero_width_breaks_do_not() {
    for (hints, visible) in [
        ("\u{00ad}", "banana-"),
        ("\u{200b}", "banana"),
        ("\u{200b}\u{00ad}\u{200b}", "banana-"),
    ] {
        let source = format!("banana{hints}land");
        let expected = shaped(visible, FlowInlineStyle::default());
        for styled in [false, true] {
            let display = render(&source, expected.total_advance + 0.01, styled);
            let lines = texts(&display);
            assert_eq!(lines.len(), 2, "{source:?}");
            assert_eq!(lines[0].text, format!("banana{hints}"));
            assert_eq!(lines[1].text, "land");
            assert_eq!(copied_text(&display), source);
            assert_eq!(display.reading_order()[0].text, source);
            assert_visible(lines[0].font_run.as_ref().unwrap(), &expected);
            assert_visible(
                lines[1].font_run.as_ref().unwrap(),
                &shaped("land", FlowInlineStyle::default()),
            );
            for line in lines {
                assert_source_clusters(line);
            }
            if hints.contains(SHY) {
                assert_hyphen_owned_by_hint(texts(&display)[0]);
            }
        }
    }
}

#[test]
fn authored_break_inside_a_ligature_is_reshaped_at_its_selected_boundary() {
    let source = "f\u{00ad}igree";
    let expected = shaped("f-", FlowInlineStyle::default());
    let display = render(source, expected.total_advance + 0.01, true);
    let lines = texts(&display);
    assert_eq!(lines[0].text, "f\u{00ad}");
    assert_visible(lines[0].font_run.as_ref().unwrap(), &expected);
    assert_hyphen_owned_by_hint(lines[0]);
    assert_eq!(copied_text(&display), source);
    for line in lines {
        assert_source_clusters(line);
    }
}

#[test]
fn styled_links_and_mixed_math_keep_their_real_fragments_at_authored_breaks() {
    let bold = FlowInlineStyle {
        bold: true,
        ..FlowInlineStyle::default()
    };
    let expected = shaped("banana-", bold);
    let display = render(
        "[**banana\u{00ad}**land](#target)",
        expected.total_advance + 0.01,
        true,
    );
    let lines = texts(&display);
    assert_eq!(lines[0].text, "banana\u{00ad}");
    assert_eq!(lines[1].text, "land");
    assert!(lines[1].bounds.y > lines[0].bounds.y);
    assert_visible(lines[0].font_run.as_ref().unwrap(), &expected);
    assert_hyphen_owned_by_hint(lines[0]);
    assert_eq!(copied_text(&display), "banana\u{00ad}land");
    let links: Vec<_> = display
        .anchors()
        .filter(|anchor| !anchor.is_heading)
        .collect();
    assert_eq!(links.len(), 2);
    for (link, text) in links.iter().zip(&lines) {
        assert_eq!(link.anchor_id, "#target");
        assert_eq!(link.bounds, text.bounds);
        assert_source_clusters(text);
    }

    let expected = shaped("banana-", FlowInlineStyle::default());
    let display = render(
        "banana\u{00ad}land $x$",
        expected.total_advance + 0.01,
        true,
    );
    assert!(
        display
            .items()
            .iter()
            .any(|item| matches!(item, DisplayItem::Path(_)))
    );
    let lines = texts(&display);
    assert_eq!(lines[0].text, "banana\u{00ad}");
    assert_visible(lines[0].font_run.as_ref().unwrap(), &expected);
    assert_hyphen_owned_by_hint(lines[0]);
    assert_eq!(copied_text(&display), "banana\u{00ad}land ");
    assert_eq!(display.reading_order()[0].text, "banana\u{00ad}land x");
    for line in lines {
        assert!(line.math_fallback.is_none());
        assert_source_clusters(line);
    }
}

#[test]
fn an_isolated_linked_soft_hyphen_keeps_its_own_face_and_navigation() {
    let source = "banana[**\u{00ad}**](#break)land";
    let bold = FlowInlineStyle {
        bold: true,
        ..FlowInlineStyle::default()
    };
    let hyphen = shaped("-", bold);
    let width = shaped("banana", FlowInlineStyle::default()).total_advance
        + hyphen.total_advance
        + 0.01;
    let narrow = render(source, width, true);
    let fragments = texts(&narrow);
    let selected = fragments.iter().find(|text| text.text.contains(SHY)).unwrap();
    assert_eq!(selected.text, "\u{00ad}");
    assert_visible(selected.font_run.as_ref().unwrap(), &hyphen);
    assert_hyphen_owned_by_hint(selected);
    assert_eq!(copied_text(&narrow), "banana\u{00ad}land");
    let links: Vec<_> = narrow.anchors().filter(|anchor| !anchor.is_heading).collect();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].anchor_id, "#break");
    assert_eq!(links[0].bounds, selected.bounds);
    let before = fragments.iter().find(|text| text.text == "banana").unwrap();
    let after = fragments.iter().find(|text| text.text == "land").unwrap();
    assert_eq!(selected.bounds.y, before.bounds.y);
    assert!(after.bounds.y > selected.bounds.y);
    for fragment in fragments {
        assert_source_clusters(fragment);
    }

    let wide = render(source, 800.0, true);
    assert_eq!(copied_text(&wide), "banana\u{00ad}land");
    for fragment in texts(&wide) {
        let run = fragment.font_run.as_ref().unwrap();
        if fragment.text.contains(SHY) {
            assert_eq!(run.total_advance, 0.0);
            assert!(run.glyphs.iter().all(|glyph| glyph.x_advance == 0.0));
        } else {
            assert_visible(run, &shaped(&fragment.text, FlowInlineStyle::default()));
        }
        assert_source_clusters(fragment);
    }
}

#[test]
fn hints_before_combining_marks_never_separate_the_mark_from_its_base() {
    for hint in [SHY, ZWSP] {
        let source = format!("ca{hint}\u{0301}fe");
        let expected = shaped("ca\u{0301}", FlowInlineStyle::default());
        let display = render(&source, expected.total_advance + 0.01, true);
        assert_eq!(copied_text(&display), source);
        let first = texts(&display)[0];
        assert_eq!(first.text, format!("ca{hint}\u{0301}"));
        assert_visible(first.font_run.as_ref().unwrap(), &expected);
        for line in texts(&display) {
            assert_source_clusters(line);
            assert!(!line.text.starts_with('\u{0301}'));
        }
    }
}

#[test]
fn leading_trailing_and_hint_only_text_never_invent_visible_hyphens() {
    for source in ["\u{00ad}bananaland\u{00ad}", "\u{200b}bananaland\u{200b}"] {
        let display = render(
            source,
            shaped("banana", FlowInlineStyle::default()).total_advance,
            true,
        );
        assert_eq!(copied_text(&display), source);
        for text in texts(&display) {
            assert_visible(
                text.font_run.as_ref().unwrap(),
                &shaped(&without_hints(&text.text), FlowInlineStyle::default()),
            );
            assert_source_clusters(text);
        }
    }
    let source = "\u{00ad}\u{200b}\u{00ad}";
    let display = render(source, 30.0, true);
    assert_eq!(copied_text(&display), source);
    assert_eq!(display.reading_order()[0].text, source);
    let space = shaped(" ", FlowInlineStyle::default());
    for text in texts(&display) {
        let run = text.font_run.as_ref().unwrap();
        assert_eq!(run.total_advance, 0.0);
        for glyph in &run.glyphs {
            assert_eq!(glyph.x_advance, 0.0);
            assert_eq!(glyph.glyph_id, space.glyphs[0].glyph_id);
            assert_eq!(glyph.font_id, space.glyphs[0].font_id);
        }
        assert_source_clusters(text);
    }
}

#[test]
fn wide_table_grid_uses_visible_widths_without_losing_authored_cell_text() {
    let source = "| Te\u{00ad}rm | Meaning |\n| --- | --- |\n| **co\u{00ad}operate** | plain\u{200b}text |";
    let actual = render(source, 600.0, true);
    let expected = render(&without_hints(source), 600.0, true);
    let borders = |display: &DisplayList| {
        display
            .items()
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Vector(vector) if vector.shape == VectorShapeType::TableBorder => {
                    Some(vector.bounds)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(!borders(&actual).is_empty());
    assert_eq!(borders(&actual), borders(&expected));
    let actual_text = texts(&actual);
    let expected_text = texts(&expected);
    assert_eq!(actual_text.len(), expected_text.len());
    for (actual, expected) in actual_text.into_iter().zip(expected_text) {
        assert_eq!(actual.bounds, expected.bounds);
        assert_visible(
            actual.font_run.as_ref().unwrap(),
            expected.font_run.as_ref().unwrap(),
        );
        assert_source_clusters(actual);
    }
}

#[test]
fn bounded_hint_separated_spans_match_whole_context_in_prose_math_and_tables() {
    let content = "banana\u{00ad}land\u{200b}".repeat(80);
    for source in [
        content.clone(),
        format!("**{content}**"),
        format!("{content} $x$ {content}"),
        format!("| Key | Details |\n| --- | --- |\n| value | {content} |"),
    ] {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let engine = engine(&source);
        let mut bounded = options(100.0);
        bounded.max_shape_bytes = 96;
        let mut longest_call = 0;
        let actual = engine
            .to_styled_display_list(bounded, |text, size, role, style| {
                longest_call = longest_call.max(text.len());
                assert!(text.len() <= 96, "a work window exceeded its host budget");
                assert!(!text.contains([SHY, ZWSP]), "prose controls leaked to the host");
                fonts.shape(text, size, role, style)
            })
            .unwrap();
        assert!(longest_call > 0);
        let expected = fonts.render(&engine, options(100.0)).unwrap();
        assert_eq!(actual.items(), expected.items());
        assert_eq!(actual.reading_order(), expected.reading_order());
        for text in texts(&actual) {
            assert_source_clusters(text);
        }
    }
}

#[test]
fn browser_selections_copy_original_hints_and_hit_tests_use_source_utf16() {
    let narrow = shaped("banana-", FlowInlineStyle::default()).total_advance + 0.01;
    let mut sources = vec![
        ("é\u{00ad}lan\u{200b}ce", 800.0),
        ("a\u{200b}\u{0301}bc", 800.0),
        ("banana\u{00ad}land", narrow),
    ];
    if cfg!(feature = "emoji-face") {
        sources.push(("é\u{00ad}😀\u{200b}a", 800.0));
    }
    for (source, width) in sources {
        let state = BrowserFlowSession::new(source, "sans", options(width)).unwrap();
        let display = render(source, width, true);
        assert_eq!(
            state.copy_source(1, SourceSpan::new(0, source.len())).unwrap(),
            source,
        );
        assert!(state.reading_json(1, 1, 0, 16).unwrap().contains(source));
        for (index, item) in display.items().iter().enumerate() {
            let DisplayItem::Text(text) = item else { continue };
            let run = text.font_run.as_ref().unwrap();
            assert_source_clusters(text);
            let selected = state
                .select_item_json(1, 1, index, 0, text.text.encode_utf16().count())
                .unwrap();
            assert!(selected.contains(&format!("\"text\":\"{}\"", text.text)));
            for cluster in &run.clusters {
                let hit = state
                    .hit_test_json(
                        1,
                        1,
                        text.bounds.x + cluster.x_start + cluster.advance() * 0.25,
                        text.bounds.y + text.bounds.height * 0.5,
                    )
                    .unwrap();
                assert!(hit.contains("\"coordinateSpace\":\"fragment-utf16\""));
                assert!(hit.contains(&format!("\"byteOffset\":{}", cluster.byte_range.start)));
                assert!(hit.contains(&format!("\"utf16Offset\":{}", cluster.utf16_range.start)));
            }
            for (byte, ch) in text.text.char_indices() {
                let start = text.text[..byte].encode_utf16().count();
                if matches!(ch, SHY | ZWSP) {
                    let selected = state
                        .select_item_json(1, 1, index, start, start + 1)
                        .unwrap();
                    assert!(selected.contains(&format!("\"text\":\"{ch}\"")));
                } else if ch.len_utf16() == 2 {
                    assert!(state.select_item_json(1, 1, index, start, start + 1).is_err());
                    assert!(run.caret_at_utf16(start + 1, CaretAffinity::Leading).is_none());
                }
            }
        }
    }
}

#[test]
fn code_and_unrenderable_tex_keep_literal_control_bytes_at_the_host_boundary() {
    for source in [
        "`literal\u{200b}code`",
        "```text\nliteral\u{200b}code\n```",
        "$\\unsupported{literal\u{200b}tex}$",
    ] {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut saw_literal = false;
        let result = engine(source).to_styled_display_list(
            options(800.0),
            |text, size, role, style| {
                saw_literal |= text.contains(ZWSP);
                fonts.shape(text, size, role, style)
            },
        );
        assert!(saw_literal, "literal bytes were projected away: {source:?}");
        assert!(matches!(result, Err(FlowLayoutError::Shaping(_))));
    }
}

#[test]
fn malformed_projected_host_runs_are_rejected_before_source_remapping() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for malformed in 0..3 {
        let result = engine("é\u{00ad}long\u{200b}word").to_styled_display_list(
            options(800.0),
            |text, size, role, style| {
                let mut run = fonts.shape(text, size, role, style)?;
                match malformed {
                    0 => run.clusters[0].byte_range.end = usize::MAX,
                    1 => run.clusters[0].utf16_range.end = usize::MAX,
                    _ => run.clusters[0].glyph_range.end = usize::MAX,
                }
                Ok(run)
            },
        );
        assert!(matches!(result, Err(FlowLayoutError::InvalidShapedRun)));
    }
}

#[test]
fn hints_after_spaces_retain_word_breaks_even_in_their_own_styled_span() {
    let expected = shaped("foo ", FlowInlineStyle::default());
    let width = expected.total_advance + 0.01;
    for hint in [SHY, ZWSP] {
        for prefix in [format!("foo {hint}"), format!("foo **{hint}**")] {
            for suffix in ["barbaz", "barbaz $x$"] {
                let source = format!("{prefix}{suffix}");
                for styled in [false, true] {
                    let display = render(&source, width, styled);
                    let all = texts(&display);
                    let first_y = all[0].bounds.y;
                    let first: Vec<_> = all
                        .iter()
                        .copied()
                        .take_while(|text| text.bounds.y == first_y)
                        .collect();
                    assert_eq!(
                        first.iter().map(|text| text.text.as_str()).collect::<String>(),
                        format!("foo {hint}"),
                    );
                    let visible: Vec<_> = first
                        .iter()
                        .copied()
                        .filter(|text| !without_hints(&text.text).is_empty())
                        .collect();
                    assert_eq!(visible.len(), 1);
                    assert_visible(visible[0].font_run.as_ref().unwrap(), &expected);
                    for text in first {
                        if without_hints(&text.text).is_empty() {
                            let run = text.font_run.as_ref().unwrap();
                            assert_eq!(run.total_advance, 0.0);
                            assert!(run.glyphs.iter().all(|glyph| glyph.x_advance == 0.0));
                        }
                        assert_source_clusters(text);
                    }
                    assert!(all.iter().any(|text| text.bounds.y > first_y));
                }
            }
            let repeated = format!("{prefix}barbaz ").repeat(48);
            let source = format!("{repeated}$x$ {repeated}");
            let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
            let engine = engine(&source);
            let mut bounded = options(width);
            bounded.max_shape_bytes = 64;
            let actual = fonts.render(&engine, bounded).unwrap();
            let whole = fonts.render(&engine, options(width)).unwrap();
            assert_eq!(actual.items(), whole.items());
            assert_eq!(actual.reading_order(), whole.reading_order());
        }
    }
}

#[test]
fn documents_without_hints_keep_plain_ligatures_and_math_fallback_placement() {
    let plain = render("of`f`ice", 800.0, false);
    let expected = shaped("office", FlowInlineStyle::default());
    let lines = texts(&plain);
    assert_eq!(lines.len(), 1, "unstyled code boundaries must still coalesce");
    assert_eq!(lines[0].text, "office");
    assert_visible(lines[0].font_run.as_ref().unwrap(), &expected);

    // Both API callbacks deliberately choose the same font. Unsupported TeX
    // must retain its established code-size fallback in either API.
    let source = "Before $\\unsupported{office}$ after.";
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let engine = engine(source);
    for width in [90.0, 800.0] {
        let plain = engine
            .to_shaped_display_list(options(width), |text, size, role| {
                fonts.shape(text, size, role, FlowInlineStyle::default())
            })
            .unwrap();
        let styled = engine
            .to_styled_display_list(options(width), |text, size, role, _style| {
                fonts.shape(text, size, role, FlowInlineStyle::default())
            })
            .unwrap();
        assert_eq!(plain.items(), styled.items());
        assert_eq!(plain.reading_order(), styled.reading_order());
        let fallback: Vec<_> = texts(&plain)
            .into_iter()
            .filter(|text| text.math_fallback.is_some())
            .collect();
        assert!(!fallback.is_empty());
        assert!(fallback.iter().all(|text| text.font_size == options(width).code_size));
    }
}

#[test]
fn a_hyphen_that_does_not_fit_cannot_destroy_a_fitting_unbroken_ligature() {
    let expected = shaped("fi", FlowInlineStyle::default());
    let hyphenated = shaped("f-", FlowInlineStyle::default());
    assert_eq!(expected.clusters.len(), 1, "fixture needs the real fi ligature");
    assert!(
        expected.total_advance + 0.01 < hyphenated.total_advance,
        "fixture needs fi to fit where its hyphenated prefix does not",
    );
    let width = expected.total_advance + 0.01;
    for suffix in ["", " $x$"] {
        let source = format!("f\u{00ad}ifi{suffix}");
        for styled in [false, true] {
            let display = render(&source, width, styled);
            let first = texts(&display)[0];
            assert_eq!(first.text, "f\u{00ad}i");
            assert_visible(first.font_run.as_ref().unwrap(), &expected);
            assert_source_clusters(first);
            for text in texts(&display) {
                assert!(text.bounds.width <= width);
            }
            assert_eq!(
                copied_text(&display),
                if suffix.is_empty() { "f\u{00ad}ifi" } else { "f\u{00ad}ifi " },
            );
        }
    }
}

#[test]
fn narrow_table_cells_choose_a_real_hyphen_or_a_fitting_unbroken_ligature() {
    let hyphenated = shaped("a-", FlowInlineStyle::default());
    let tail = shaped("b", FlowInlineStyle::default());
    let unbroken = shaped("ab", FlowInlineStyle::default());
    let minimum = hyphenated.total_advance.max(tail.total_advance);
    assert!(minimum < unbroken.total_advance, "fixture needs a genuine split");
    let hyphen_width = (minimum + unbroken.total_advance) * 0.5;
    let ligature = shaped("fi", FlowInlineStyle::default());
    assert!(
        ligature.total_advance + 0.01
            < shaped("f-", FlowInlineStyle::default()).total_advance
    );
    for (cell, first_source, first_visible, tail, width, selected) in [
        ("a\u{00ad}b", "a\u{00ad}", "a-", "b", hyphen_width, true),
        (
            "f\u{00ad}ifi",
            "f\u{00ad}i",
            "fi",
            "fi",
            ligature.total_advance + 0.01,
            false,
        ),
    ] {
        // The established table geometry reserves four points on each side.
        // An empty header leaves the actual cell content responsible for the
        // intrinsic minimum, rather than a wider heading masking a refusal.
        let viewport = width + 8.0;
        let source = format!("| |\n| --- |\n| {cell} |");
        for styled in [false, true] {
            let display = render(&source, viewport, styled);
            let lines = texts(&display);
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0].text, first_source);
            assert_eq!(lines[1].text, tail);
            assert!(lines[1].bounds.y > lines[0].bounds.y);
            assert_visible(
                lines[0].font_run.as_ref().unwrap(),
                &shaped(first_visible, FlowInlineStyle::default()),
            );
            assert_visible(
                lines[1].font_run.as_ref().unwrap(),
                &shaped(tail, FlowInlineStyle::default()),
            );
            assert_eq!(copied_text(&display), cell);
            if selected {
                assert_hyphen_owned_by_hint(lines[0]);
            }
            for line in lines {
                assert!(line.bounds.x >= 4.0);
                assert!(line.bounds.right() <= viewport - 4.0 + 0.001);
                assert_source_clusters(line);
            }
            let borders: Vec<_> = display
                .items()
                .iter()
                .filter_map(|item| match item {
                    DisplayItem::Vector(vector)
                        if vector.shape == VectorShapeType::TableBorder =>
                    {
                        Some(vector.bounds)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(borders.len(), 2);
            assert!(borders.iter().all(|bounds| bounds.width == viewport));
        }
    }
}
