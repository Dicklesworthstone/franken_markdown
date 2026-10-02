//! Exercise the real SVG tokenizer, painter and serialized link metadata.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{
    Block, Document, Inline, Op, Poster, RStyle, SvgOptions, render_svg,
    render_svg_with_diagnostics, render_svg_with_report, render_svg_with_resources,
};
use super::*;

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}
fn link(dest: &str, content: Vec<Inline>) -> Inline {
    Inline::Link {
        dest: dest.to_owned(),
        title: None,
        content,
    }
}
fn heading(value: &str) -> Block {
    Block::Heading {
        level: 2,
        inlines: vec![text(value)],
    }
}
fn finish(nav: Navigation) -> (String, Vec<SvgWarning>) {
    let mut out = String::new();
    let mut warnings = Vec::new();
    nav.emit(&mut out, 612.0, 792.0, &mut warnings);
    (out, warnings)
}
fn paint(nav: &mut Navigation, id: usize) {
    nav.record(Some(id), Some((10.0, 20.0, 30.0, 40.0)), &mut None);
}
fn anchors(svg: &str) -> Vec<&str> {
    svg.lines().filter(|line| line.starts_with("<a ")).collect()
}
fn count(warnings: &[SvgWarning], code: &str) -> usize {
    warnings
        .iter()
        .filter(|warning| warning.code == code)
        .count()
}

#[test]
fn svg_links_admit_only_explicit_schemes_and_heading_fragments() {
    for raw in [
        "https://example.com",
        "HTTP://example.com/path?a=1&b=2",
        "mailto:a@example.com",
        "https://example.com/a%2Fb",
        "https://example.com/%E2%82%AC",
        "#",
        "#chapter-2",
        "#%63hapter-2",
    ] {
        assert!(target(raw).is_some(), "safe target rejected: {raw:?}");
    }
    for raw in [
        "",
        "relative.md",
        "/etc/passwd",
        "//example.com",
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "java\tscript:alert(1)",
        "data:text/html,hello",
        "vbscript:msgbox(1)",
        "file:///tmp/a",
        "blob:https://example.com/id",
        "custom:thing",
        "http:example.com",
        "https://",
        "https://?x",
        "https://example.com/a b",
        "https://example.com/\\x",
        "https://example.com/%0aevil",
        "mailto:a@example.com?subject=x%0D%0Abcc:y@example.com",
        "https://example.com/%",
        "https://example.com/%GG",
        "https://example.com/%FF",
        "https://example.com/\0",
        "https://example.com/\u{ffff}",
        "https://example.com/\u{1ffff}",
    ] {
        assert!(target(raw).is_none(), "unsafe target admitted: {raw:?}");
    }
    assert_eq!(
        target(" https://example.com "),
        Some(Target::External("https://example.com".into()))
    );
    assert_eq!(
        target("#%63hapter-2"),
        Some(Target::Fragment("chapter-2".into()))
    );
    assert_eq!(
        target("#%2563hapter-2"),
        Some(Target::Fragment("%63hapter-2".into()))
    );
    assert!(target(&"x".repeat(targets::MAX_TARGET_BYTES + 1)).is_none());
}

#[test]
fn svg_links_metadata_is_bounded_xml_safe_and_not_executed() {
    let label = targets::text_label(&format!("é\0\n\u{ffff}{}", "Ω".repeat(400)));
    assert_eq!(label.chars().count(), 256);
    assert!(label.starts_with("é  \u{fffd}"));
    let mut nav = Navigation::default();
    let id = nav.intern(
        "https://example.com/?a=1&b=2",
        Some("\"><script>x</script>&\0"),
        &[text("Read <this> & \"that\"\0")],
    );
    paint(&mut nav, id);
    let (out, warnings) = finish(nav);
    assert!(warnings.is_empty());
    assert_eq!(anchors(&out).len(), 1);
    assert!(out.contains("href=\"https://example.com/?a=1&amp;b=2\""));
    assert!(out.contains("aria-label=\"Read &lt;this&gt; &amp; &quot;that&quot; \""));
    assert!(out.contains("<title>&quot;&gt;&lt;script&gt;x&lt;/script&gt;&amp; </title>"));
    assert!(!out.contains("<script") && !out.contains('\0') && !out.contains("onclick="));
    assert!(out.contains("tabindex=\"0\"") && out.contains("pointer-events=\"all\""));
}

#[test]
fn svg_links_measurement_interns_stably_without_paint_or_diagnostics() {
    let mut nav = Navigation::default();
    let content = [text("same link")];
    let first = nav.intern("https://example.com", Some("title"), &content);
    for _ in 0..20 {
        assert_eq!(
            nav.intern("https://example.com", Some("title"), &content),
            first
        );
    }
    assert_ne!(
        nav.intern("https://example.com", Some("other"), &content),
        first
    );
    assert_eq!(nav.intern("javascript:bad", None, &content), UNSAFE);
    nav.heading(&[text("A heading")], 48.0);
    assert_eq!(nav.links.len(), 2);
    assert!(nav.regions.is_empty());
    let (out, warnings) = finish(nav);
    assert!(out.is_empty() && warnings.is_empty());
}

#[test]
fn svg_links_source_labels_keep_rich_text_without_inventing_word_spaces() {
    let inlines = [
        text("pre"),
        Inline::Strong(vec![text("fix")]),
        Inline::Code("_code".into()),
        Inline::SoftBreak,
        Inline::Math("x^2".into()),
        Inline::Image {
            dest: "x".into(),
            alt: " diagram".into(),
            title: None,
        },
        Inline::FootnoteRef { id: "n".into() },
    ];
    assert_eq!(inline_label(&inlines), "prefix_code x^2 diagram[^n]");
    assert_eq!(
        heading_slug(&inlines).as_deref(),
        Some("prefix-code-x2-diagram")
    );
    assert_eq!(heading_slug(&[text("你好 !")]).as_deref(), Some("section"));
    assert_eq!(
        heading_slug(&[text(" A__B---C  D ")]).as_deref(),
        Some("a-b-c-d")
    );
}

#[test]
fn svg_links_hit_regions_merge_only_adjacent_same_link_runs_in_one_line() {
    let mut nav = Navigation::default();
    let a = nav.intern("https://a.example", None, &[text("a")]);
    let b = nav.intern("https://b.example", None, &[text("b")]);
    let mut previous = None;
    nav.record(Some(a), Some((0.0, 10.0, 10.0, 20.0)), &mut previous);
    nav.record(Some(a), Some((13.0, 5.0, 25.0, 22.0)), &mut previous);
    assert_eq!(nav.regions.len(), 1);
    assert_eq!(nav.regions[0].rect.left, 0.0);
    assert_eq!(nav.regions[0].rect.right, 25.0);
    assert_eq!(nav.regions[0].rect.top, 5.0);
    assert_eq!(nav.regions[0].rect.bottom, 22.0);
    nav.record(None, None, &mut previous);
    nav.record(Some(a), Some((45.0, 10.0, 50.0, 20.0)), &mut previous);
    nav.record(Some(b), Some((50.0, 10.0, 60.0, 20.0)), &mut previous);
    nav.record(Some(a), Some((60.0, 10.0, 70.0, 20.0)), &mut previous);
    assert_eq!(nav.regions.len(), 4);
    previous = None; // New physical line / independently aligned table cell.
    nav.record(Some(a), Some((0.0, 30.0, 20.0, 40.0)), &mut previous);
    assert_eq!(nav.regions.len(), 5);
    assert_eq!(anchors(&finish(nav).0).len(), 5);
}

#[test]
fn svg_links_regions_clip_to_the_poster_and_thin_regions_remain_hittable() {
    let mut nav = Navigation::default();
    let id = nav.intern("https://example.com", None, &[text("link")]);
    nav.record(Some(id), Some((-10.0, -20.0, 700.0, 900.0)), &mut None);
    nav.record(Some(id), Some((20.001, 30.001, 20.002, 30.002)), &mut None);
    nav.record(Some(id), Some((f64::NAN, 0.0, 20.0, 20.0)), &mut None);
    nav.record(Some(id), Some((20.0, 0.0, 20.0, 20.0)), &mut None);
    let (out, warnings) = finish(nav);
    assert_eq!(anchors(&out).len(), 2);
    assert!(out.contains("x=\"0.00\" y=\"0.00\" width=\"612.00\" height=\"792.00\""));
    assert!(out.contains("width=\"0.01\" height=\"0.01\""));
    assert_eq!(count(&warnings, "svg_link_geometry"), 1);
}

#[test]
fn svg_links_forward_heading_fragments_and_overview_have_resolved_native_views() {
    let mut nav = Navigation::default();
    let later = nav.intern("#%6cater", None, &[text("go forward")]);
    let top = nav.intern("#", None, &[text("overview")]);
    paint(&mut nav, later);
    paint(&mut nav, top);
    nav.heading(&[text("Earlier")], 48.0);
    nav.heading(&[text("Later")], 300.0);
    let (out, warnings) = finish(nav);
    assert!(warnings.is_empty());
    assert!(out.contains("<view id=\"fmd-heading-1\" viewBox=\"0 300.00 612.00 792.00\""));
    assert!(out.contains("<view id=\"fmd-top\" viewBox=\"0 0.00 612.00 792.00\""));
    assert!(out.contains("<a href=\"#fmd-heading-1\"") && out.contains("<a href=\"#fmd-top\""));
    assert!(
        !out.contains("id=\"fmd-heading-0\""),
        "unreferenced headings need no emitted view"
    );
}

#[test]
fn svg_links_heading_collisions_match_html_suffixes_without_glyph_id_collisions() {
    let mut nav = Navigation::default();
    for (i, source) in [
        "Topic", "Topic-2", "Topic", "Topic", "g0", "i0", "!!!", "???",
    ]
    .iter()
    .enumerate()
    {
        nav.heading(&[text(source)], 20.0 * i as f64);
    }
    for (i, slug) in [
        "topic",
        "topic-2",
        "topic-3",
        "topic-4",
        "g0",
        "i0",
        "section",
        "section-2",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(nav.headings[*slug].id, i);
        let id = nav.intern(&format!("#{slug}"), None, &[text(slug)]);
        paint(&mut nav, id);
    }
    let (out, warnings) = finish(nav);
    assert!(warnings.is_empty());
    let all = anchors(&out);
    assert_eq!(all.len(), 8);
    assert!(
        all.iter()
            .all(|anchor| anchor.starts_with("<a href=\"#fmd-heading-"))
    );
    assert!(!out.contains("id=\"g0\"") && !out.contains("id=\"i0\""));
}

#[test]
fn svg_links_unknown_or_reserved_fragments_never_activate_raw_ids_or_view_specs() {
    let mut nav = Navigation::default();
    for raw in [
        "#unknown",
        "#g0",
        "#i0",
        "#fmd-heading-0",
        "#svgView(viewBox(0,0,1,1))",
    ] {
        let id = nav.intern(raw, None, &[text("visible")]);
        paint(&mut nav, id);
    }
    nav.heading(&[text("Known")], 30.0);
    let (out, warnings) = finish(nav);
    assert!(out.is_empty());
    assert_eq!(count(&warnings, "svg_link_unresolved"), 1);
    assert!(!warnings[0].message.contains("unknown"));
}

#[test]
fn svg_links_budget_failures_are_atomic_and_do_not_renumber_later_headings() {
    let mut nav = Navigation::default();
    nav.heading(&[text("First")], 10.0);
    nav.heading(&[text(&"x".repeat(1025))], 20.0);
    nav.heading(&[text("Later")], 30.0);
    assert_eq!(nav.headings.len(), 1);
    let good = nav.intern("#first", None, &[text("first")]);
    let missing = nav.intern("#later", None, &[text("later")]);
    paint(&mut nav, good);
    paint(&mut nav, missing);
    nav.metadata_bytes = MAX_METADATA_BYTES;
    assert_eq!(
        nav.intern("#first", None, &[text("first")]),
        good,
        "existing metadata stays usable"
    );
    let limited = nav.intern("https://new.example", None, &[text("new")]);
    assert_eq!(limited, LIMITED);
    paint(&mut nav, limited);
    let (out, warnings) = finish(nav);
    assert_eq!(anchors(&out).len(), 1);
    assert_eq!(count(&warnings, "svg_anchor_limit"), 1);
    assert_eq!(count(&warnings, "svg_link_limit"), 1);
    assert_eq!(count(&warnings, "svg_link_unresolved"), 1);
    let mut out = "original".to_owned();
    let mut remaining = 4;
    assert!(!append_bounded(&mut out, "<a/>", &mut 3));
    assert_eq!(out, "original");
    assert!(append_bounded(&mut out, "<a/>", &mut remaining));
    assert_eq!(remaining, 0);
}

#[test]
fn svg_links_region_budget_retains_existing_regions_and_emits_one_diagnostic() {
    let mut nav = Navigation::default();
    let id = nav.intern("https://example.com", None, &[text("link")]);
    for _ in 0..MAX_REGIONS + 3 {
        paint(&mut nav, id);
    }
    assert_eq!(nav.regions.len(), MAX_REGIONS);
    let (out, warnings) = finish(nav);
    assert_eq!(anchors(&out).len(), MAX_REGIONS);
    assert_eq!(count(&warnings, "svg_link_limit"), 1);
    assert!(out.len() <= MAX_OUTPUT_BYTES);
}

#[test]
fn svg_links_real_shaped_lines_keep_link_identity_through_styles_and_emergency_wrap() {
    let mut p = Poster::new(&SvgOptions::default());
    let inlines = [link(
        "https://example.com",
        vec![
            text("AV ffi before "),
            Inline::Strong(vec![text("bold ")]),
            Inline::Emphasis(vec![text("e\u{301} italic ")]),
            Inline::Code("two  spaces ".into()),
            text(&"abcdefgh".repeat(20)),
        ],
    )];
    let mut pieces = Vec::new();
    p.flatten(&inlines, RStyle::BODY, &mut pieces);
    let lines = p.wrap(&pieces, 11.0, 60.0);
    assert!(lines.len() > 5);
    for (i, line) in lines.iter().enumerate() {
        let first = line.first().unwrap().style.link;
        assert!(first.is_some() && line.iter().all(|run| run.style.link == first));
        let baseline = 30.0 + i as f64 * 30.0;
        p.draw_words(line, 10.0, baseline, 11.0);
        let nav = p.navigation.borrow();
        let region = nav.regions.last().unwrap();
        assert!((region.rect.left - 10.0).abs() < 1e-8);
        assert!((region.rect.right - 10.0 - p.words_width(line, 11.0)).abs() < 1e-8);
    }
    assert_eq!(p.navigation.borrow().regions.len(), lines.len());
    assert!(p.ops.iter().any(|op| matches!(op, Op::Glyph { .. })));
}

#[test]
fn svg_links_unsafe_nested_ast_links_cannot_inherit_an_outer_destination() {
    let mut p = Poster::new(&SvgOptions::default());
    let inlines = [link(
        "https://outer.example",
        vec![
            text("before"),
            link("javascript:bad", vec![text("middle")]),
            text("after"),
        ],
    )];
    let mut pieces = Vec::new();
    p.flatten(&inlines, RStyle::BODY, &mut pieces);
    let lines = p.wrap(&pieces, 11.0, 400.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].len(), 3);
    p.draw_words(&lines[0], 0.0, 30.0, 11.0);
    let nav = p.navigation.borrow();
    assert_eq!(nav.regions.len(), 2);
    assert_eq!(nav.regions[0].rect.right, lines[0][0].w);
    assert_eq!(nav.regions[1].rect.left, lines[0][0].w + lines[0][1].w);
    drop(nav);
    let (bytes, report, warnings) = p.emit(100.0);
    let out = String::from_utf8(bytes).unwrap();
    assert_eq!(anchors(&out).len(), 2);
    assert_eq!(count(&warnings, "svg_link_unsafe"), 1);
    assert!(report.glyphs_drawn >= 16);
    assert!(!out.contains("javascript"));
}

#[test]
fn svg_links_linked_math_uses_its_actual_height_and_keeps_formula_glyphs() {
    let mut p = Poster::new(&SvgOptions::default());
    let mut pieces = Vec::new();
    p.flatten(
        &[link(
            "https://example.com/math",
            vec![Inline::Math("\\frac{1}{x^2}".into())],
        )],
        RStyle::BODY,
        &mut pieces,
    );
    let lines = p.wrap(&pieces, 11.0, 400.0);
    let word = &lines[0][0];
    let formula = word
        .formula
        .as_ref()
        .expect("shared TeX engine typesets linked formulas");
    let above = formula.formula.ascent * formula.size;
    let below = formula.formula.descent * formula.size;
    p.draw_words(&lines[0], 12.0, 100.0, 11.0);
    let nav = p.navigation.borrow();
    assert_eq!(nav.regions.len(), 1);
    let rect = nav.regions[0].rect;
    assert!(rect.top <= 100.0 - above && rect.bottom >= 100.0 + below);
    assert!((rect.right - rect.left - word.w).abs() < 1e-8);
    assert!(p.ops.iter().any(|op| matches!(op, Op::Glyph { .. })));
}

#[test]
fn svg_links_image_and_fallback_alt_content_both_keep_destinations() {
    let asset = franken_markdown::PdfImageAsset::new("diagram.svg",
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"120\"><rect width=\"40\" height=\"120\" fill=\"red\"/></svg>".to_vec());
    let mut p = Poster::new(&SvgOptions::default())
        .with_resources(&franken_markdown::FontAssets::default(), &[asset])
        .unwrap();
    let make = |dest: &str| {
        link(
            "https://example.com/image",
            vec![Inline::Image {
                dest: dest.into(),
                title: None,
                alt: "diagram".into(),
            }],
        )
    };
    let mut pieces = Vec::new();
    p.flatten(
        &[make("diagram.svg"), text(" "), make("missing.png")],
        RStyle::BODY,
        &mut pieces,
    );
    let lines = p.wrap(&pieces, 11.0, 400.0);
    let image = lines[0][0]
        .image
        .as_ref()
        .expect("host image resource was resolved");
    let image_height = image.height;
    let image_width = image.width;
    p.draw_words(&lines[0], 20.0, 200.0, 11.0);
    let nav = p.navigation.borrow();
    // Equal link label/target may merge adjacent image and alt-text fragments.
    assert!(nav.regions[0].rect.top <= 200.0 - image_height);
    assert!(nav.regions[0].rect.right >= 20.0 + image_width);
    drop(nav);
    let (bytes, _, warnings) = p.emit(300.0);
    let out = String::from_utf8(bytes).unwrap();
    assert!(out.contains("<use href=\"#i0\""));
    assert!(
        anchors(&out)
            .iter()
            .all(|a| a.contains("https://example.com/image"))
    );
    assert!(!anchors(&out).is_empty());
    assert!(
        warnings
            .iter()
            .any(|warning| warning.code.starts_with("svg_image_"))
    );
}

#[test]
fn svg_links_all_public_entrypoints_and_endnotes_keep_navigation_deterministically() {
    let doc = Document {
        blocks: vec![
            Block::Paragraph(vec![
                link("#later", vec![text("go forward")]),
                Inline::FootnoteRef { id: "n".into() },
            ]),
            heading("Later"),
            Block::FootnoteDefinition {
                id: "n".into(),
                blocks: vec![Block::Paragraph(vec![link(
                    "https://example.com/note",
                    vec![text("note link")],
                )])],
            },
        ],
    };
    let opts = SvgOptions::default();
    let result = render_svg_with_diagnostics(&doc, &opts);
    assert_eq!(result, render_svg_with_diagnostics(&doc, &opts));
    assert_eq!(
        result,
        render_svg_with_diagnostics(&doc.with_endnotes(), &opts)
    );
    assert_eq!(result.0, render_svg(&doc, &opts));
    assert_eq!(
        (result.0.clone(), result.1),
        render_svg_with_report(&doc, &opts)
    );
    assert_eq!(
        result,
        render_svg_with_resources(&doc, &opts, &franken_markdown::FontAssets::default(), &[])
            .unwrap()
    );
    let out = String::from_utf8(result.0).unwrap();
    assert!(out.contains("<view id=\"fmd-heading-0\""));
    assert!(out.contains("<a href=\"#fmd-heading-0\""));
    assert!(out.contains("<a href=\"https://example.com/note\""));
    assert!(result.2.is_empty());
}

#[test]
fn svg_links_link_free_output_does_not_emit_navigation_or_change_glyph_geometry() {
    let doc = Document {
        blocks: vec![heading("No links"), Block::Paragraph(vec![text("body")])],
    };
    let mut p = Poster::new(&SvgOptions::default());
    let left = p.content_left();
    let right = p.content_right();
    for block in &doc.blocks {
        p.block(block, left, right, false);
    }
    assert_eq!(p.navigation.borrow().headings.len(), 1);
    let height = p.y + p.margin_bottom;
    // Clearing metadata cannot alter the actual prepared/painted operations.
    *p.navigation.borrow_mut() = Navigation::default();
    assert_eq!(
        p.emit(height),
        render_svg_with_diagnostics(&doc, &SvgOptions::default())
    );
}
