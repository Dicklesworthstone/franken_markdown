//! Font-oracle coordinates, retained measurement, and enclosing PDF geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

const FIXTURE: &[u8] = include_bytes!("../../fmd-font/fonts/test-shaping/FmdShaping.ttf");

fn options() -> PdfOptions {
    PdfOptions {
        font_assets: FontAssets {
            body_regular: Some(FIXTURE.to_vec()),
            body_bold: Some(FIXTURE.to_vec()),
            body_italic: Some(FIXTURE.to_vec()),
            body_bold_italic: Some(FIXTURE.to_vec()),
            mono_regular: Some(FIXTURE.to_vec()),
            ..FontAssets::default()
        },
        base_font_size: Some(10.0),
        ..PdfOptions::default()
    }
}

fn near(left: f32, right: f32) {
    assert!((left - right).abs() < 0.002, "{left} != {right}");
}

#[test]
fn pdf_positioning_matches_saved_harfbuzz_mark_and_mark_to_mark_coordinates() {
    let faces = Faces::load(&options()).unwrap();
    let face = faces.face(F_BODY);
    let size = font_size_of(10.0);
    for (text, ids, positions, width) in [
        (
            "a\u{0301}b",
            vec![2, 7, 3],
            vec![(0.0, 0.0), (1.5, 5.0), (5.0, 0.0)],
            10.0,
        ),
        (
            "a\u{0301}\u{0307}",
            vec![2, 7, 8],
            vec![(0.0, 0.0), (1.5, 5.0), (1.5, 6.5)],
            5.0,
        ),
    ] {
        let shaped = shape_run(face, text);
        let run = shaped.positioned.as_ref().unwrap();
        assert_eq!(shaped.glyphs, ids);
        assert_eq!(shaped.text, text);
        assert!(shaped.requires_actual_text);
        near(run.width(size).to_points_f32(), width);
        assert_eq!(face.shaped_width(text, size), run.width(size));
        let mut pen = LayoutUnit::ZERO;
        for (glyph, (x, y)) in run.glyphs.iter().zip(positions) {
            near((pen + run.measure(glyph.offset_x, size)).to_points_f32(), x);
            near(run.measure(glyph.offset_y, size).to_points_f32(), y);
            pen += run.measure(glyph.advance, size);
        }
    }
}

#[test]
fn pdf_positioning_cache_retains_exact_source_offsets_and_actual_face() {
    let faces = Faces::load(&options()).unwrap();
    let face = faces.face(F_BODY);
    let first = shape_run(face, "a\u{0301}\u{0307}");
    let second = shape_run(face, "a\u{0301}\u{0307}");
    assert!(std::sync::Arc::ptr_eq(
        first.positioned.as_ref().unwrap(),
        second.positioned.as_ref().unwrap()
    ));
    let different = shape_run(face, "a\u{0301}");
    assert_ne!(
        first.positioned.as_ref().unwrap().glyphs,
        different.positioned.as_ref().unwrap().glyphs
    );
    let other_face = shape_run(faces.face(F_BOLD), "a\u{0301}\u{0307}");
    assert_eq!(first.positioned, other_face.positioned);
    assert!(!std::sync::Arc::ptr_eq(
        first.positioned.as_ref().unwrap(),
        other_face.positioned.as_ref().unwrap()
    ));
}

#[test]
fn pdf_long_merged_marked_runs_keep_per_word_admission_and_width() {
    let faces = Faces::load(&options()).unwrap();
    let face = faces.face(F_BODY);
    let size = font_size_of(10.0);
    let word = "a\u{0301}\u{0307}b";
    let source = std::iter::repeat_n(word, 1100)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(source.chars().count() > 4096);
    let shaped = shape_run(face, &source);
    assert!(shaped.positioned.is_some());
    let expected = face.shaped_width(word, size).milli_points() * 1100
        + face.shaped_width(" ", size).milli_points() * 1099;
    assert_eq!(face.shaped_width(&source, size).milli_points(), expected);
    assert_eq!(shaped.text, source);
    let mut warnings = Vec::new();
    text_positioning::prepare(face, &source).warnings(&mut warnings);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn pdf_positioning_refuses_unsupported_and_bounded_words_without_partial_source() {
    let faces = Faces::load(&options()).unwrap();
    let face = faces.face(F_BODY);
    for (source, code) in [
        ("a\u{034f}".to_owned(), "text_shaping_unsupported"),
        (format!("a{}", "\u{0301}".repeat(65)), "text_shaping_limit"),
        (
            format!("{}a\u{0301}", "b".repeat(4096)),
            "text_shaping_limit",
        ),
    ] {
        let prepared = text_positioning::prepare(face, &source);
        assert!(prepared.run.is_none());
        let mut warnings = Vec::new();
        prepared.warnings(&mut warnings);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code(), code);
        let shaped = shape_run(face, &source);
        assert_eq!(shaped.text, source);
        assert_eq!(shaped.glyphs.len(), source.chars().count());
        assert!(shaped.positioned.is_none());
    }
    let mixed = "a\u{0301}\u{0307} a\u{034f} a\u{0301}b";
    let shaped = shape_run(face, mixed);
    assert!(
        shaped.positioned.is_some(),
        "one refused word must not disable another word"
    );
    assert_eq!(shaped.text, mixed);
    assert!(
        shaped.glyphs.contains(&0),
        "the missing glyph fallback stays visible"
    );
}

#[test]
fn pdf_positioned_ink_sizes_paragraphs_table_rows_and_code_before_pagination() {
    let opts = options();
    let faces = Faces::load(&opts).unwrap();
    let source = "# a\u{0301}\u{0307}\n\na\u{0301}\u{0307}  \na\u{0301}\u{0307}\n\n```\na\u{0301}\u{0307}\na\u{0301}\u{0307}\n```\n\n| a\u{0301}\u{0307} |\n| --- |\n| a\u{0301}\u{0307} |\n\n> a\u{0301}\u{0307}\n";
    let page = PageGeom::from_theme(&opts.theme);
    let lines = layout(&crate::parse_markdown(source).blocks, &opts, &faces, page);
    let mut kinds = Vec::new();
    for line in &lines {
        if line.segs.iter().any(|seg| seg.text.contains('\u{0307}')) {
            near(line.text_ink.0, line.size * 1.05);
            near(line.text_ink.1, 0.0);
            assert!(line_leading(line) > line.size * 1.32);
            kinds.push(line.flow.kind);
        }
    }
    for kind in [
        FlowKind::Heading,
        FlowKind::Paragraph,
        FlowKind::Code,
        FlowKind::TableHeader,
        FlowKind::TableRow,
    ] {
        assert!(kinds.contains(&kind), "missing ink-bearing {kind:?}");
    }
    for page_lines in paginate_lines_for_options(&lines, page, &opts) {
        for placed in page_lines {
            assert!(placed.y + placed.line.text_ink.0 <= page.top_y() + 0.002);
            assert!(placed.y - placed.line.text_ink.1 >= page.bottom - 0.002);
        }
    }
}

#[test]
fn pdf_positioned_glyph_painter_respects_microtype_scale_and_svg_transform() {
    let faces = Faces::load(&options()).unwrap();
    let face = faces.face(F_BODY);
    let run = text_positioning::prepare(face, "a\u{0301}\u{0307}")
        .run
        .unwrap();
    let map: Vec<_> = (0..face.font.num_glyphs).collect();
    let mut text = String::new();
    text_positioning::append_text(&mut text, &run, &map, F_BODY, 10.0, 17.0, 30.0, 15, None);
    assert!(
        text.contains("1.015 0 0 1 18.52 36.50 Tm <0008> Tj"),
        "{text}"
    );
    let mut rotated = String::new();
    text_positioning::append_glyphs(
        &mut rotated,
        &run,
        &map,
        SvgTextMatrix {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            x: 17.0,
            y: 30.0,
            size: 10.0,
        },
        0,
        0,
    );
    assert!(
        rotated.contains("0 1 -1 0 10.50 31.50 Tm <0008> Tj"),
        "{rotated}"
    );
    let mut diagonal = String::new();
    let diagonal_component = std::f32::consts::FRAC_1_SQRT_2;
    text_positioning::append_glyphs(
        &mut diagonal,
        &run,
        &map,
        SvgTextMatrix {
            a: diagonal_component,
            b: diagonal_component,
            c: -diagonal_component,
            d: diagonal_component,
            x: 17.0,
            y: 30.0,
            size: 10.0,
        },
        0,
        0,
    );
    assert!(
        diagonal.contains("0.707107 0.707107 -0.707107 0.707107"),
        "{diagonal}"
    );
}

#[test]
fn pdf_ascii_and_complete_canonical_composites_keep_legacy_shaping() {
    let faces = Faces::load(&PdfOptions::default()).unwrap();
    for slot in [F_BODY, F_BOLD, F_ITALIC, F_BOLDITALIC, F_MONO] {
        let face = faces.face(slot);
        for source in [
            "ordinary office AVATAR",
            "Café",
            "Cafe\u{0301}",
            "A\u{030a}ngstro\u{0308}m",
        ] {
            let shaped = shape_run(face, source);
            assert!(shaped.positioned.is_none(), "{slot} {source:?}");
            assert_eq!(shaped.text, source);
        }
    }
}

#[test]
fn pdf_positioned_spacing_counts_clusters_and_never_splits_mark_stacks() {
    let faces = Faces::load(&options()).unwrap();
    let face = faces.face(F_BODY);
    for (source, letters, words) in [
        ("a\u{0301}\u{0307}", 0, 0),
        ("fia\u{0301}\u{0307}b", 2, 0),
        ("fia\u{0301}\u{0307} b", 3, 1),
    ] {
        let run = text_positioning::prepare(face, source).run.unwrap();
        assert_eq!(run.spacing_gaps(), (letters, words), "{source:?}");
        for (index, glyph) in run.glyphs.iter().enumerate() {
            if matches!(glyph.id, 2 | 7)
                && run
                    .glyphs
                    .get(index + 1)
                    .is_some_and(|next| matches!(next.id, 7 | 8))
            {
                assert!(!glyph.cluster_end, "no spacing before an attached mark");
            }
        }
    }
}
