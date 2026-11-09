//! Native outlines and resolved hit regions from the public SVG entry points.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::svg::{
    SvgOptions, SvgWarning, render_svg, render_svg_with_diagnostics, render_svg_with_report,
    render_svg_with_resources,
};
use franken_markdown::{Block, Document, FontAssets, Inline, parse_markdown};

fn render(source: &str, options: &SvgOptions) -> (String, Vec<SvgWarning>) {
    let (bytes, _, warnings) = render_svg_with_diagnostics(&parse_markdown(source), options);
    (String::from_utf8(bytes).unwrap(), warnings)
}

fn attr<'a>(element: &'a str, name: &str) -> &'a str {
    element
        .split_once(&format!("{name}=\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}

fn outline(svg: &str) -> Vec<(&str, &str)> {
    svg.lines()
        .filter(|line| line.starts_with("<a href=\"#fmd-heading-"))
        .map(|line| (attr(line, "href"), attr(line, "aria-label")))
        .collect()
}

fn views(svg: &str) -> Vec<(&str, f64)> {
    let mut views: Vec<_> = svg
        .lines()
        .filter(|line| line.starts_with("<view id=\"fmd-heading-"))
        .map(|line| {
            (
                attr(line, "id"),
                attr(line, "viewBox")
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .parse()
                    .unwrap(),
            )
        })
        .collect();
    views.sort_by_key(|(id, _)| *id);
    views
}

fn assert_link_geometry(svg: &str) {
    let root = svg
        .lines()
        .find(|line| line.starts_with("<svg "))
        .unwrap();
    let root_box: Vec<f64> = attr(root, "viewBox")
        .split_whitespace()
        .map(|part| part.parse().unwrap())
        .collect();
    let destinations = views(svg);
    for line in svg
        .lines()
        .filter(|line| line.starts_with("<a href=\"#fmd-heading-"))
    {
        assert!(
            destinations
                .iter()
                .any(|(id, _)| *id == &attr(line, "href")[1..])
        );
        assert!(line.contains("tabindex=\"0\"") && line.contains("pointer-events=\"all\""));
        let rectangle = line.split_once("<rect ").unwrap().1;
        let x: f64 = attr(rectangle, "x").parse().unwrap();
        let y: f64 = attr(rectangle, "y").parse().unwrap();
        let width: f64 = attr(rectangle, "width").parse().unwrap();
        let height: f64 = attr(rectangle, "height").parse().unwrap();
        assert!(x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0);
        assert!(x + width <= root_box[2] + 0.02 && y + height <= root_box[3] + 0.02);
    }
    assert!(!svg.contains("<text") && !svg.contains("<foreignObject") && !svg.contains("<script"));
}

#[test]
fn every_shared_marker_spelling_creates_a_complete_native_outline() {
    for marker in ["[TOC]", "[[TOC]]", "[[_TOC_]]", "[__toc__]"] {
        let (svg, warnings) = render(
            &format!("{marker}\n\n# Alpha\n\n## Beta\n"),
            &SvgOptions::default(),
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            outline(&svg),
            [("#fmd-heading-0", "Alpha"), ("#fmd-heading-1", "Beta")]
        );
        assert_link_geometry(&svg);
    }
}

#[test]
fn marker_depth_overrides_the_option_at_each_explicit_location() {
    let (svg, warnings) = render(
        "[TOC:1]\n\n# Alpha\n\n## Beta\n\n### Gamma\n\n[TOC]\n",
        &SvgOptions {
            toc: true,
            toc_depth: Some(2),
            ..SvgOptions::default()
        },
    );
    assert!(warnings.is_empty());
    assert_eq!(
        outline(&svg),
        [
            ("#fmd-heading-0", "Alpha"),
            ("#fmd-heading-0", "Alpha"),
            ("#fmd-heading-1", "Beta"),
        ]
    );
    assert_link_geometry(&svg);
}

#[test]
fn typed_rows_keep_duplicate_raw_html_and_note_collisions_separate() {
    let (svg, warnings) = render(
        "[TOC]\n\n<h2>Topic</h2>\n\n# Topic\n\n> ## Topic\n\n- ### fn-evidence\n\n[ambiguous](#fn-evidence) cite[^evidence].\n\n[^evidence]: ## Only in notes\n",
        &SvgOptions::default(),
    );
    assert_eq!(
        outline(&svg),
        [
            ("#fmd-heading-0", "Topic"),
            ("#fmd-heading-1", "Topic"),
            ("#fmd-heading-2", "Topic"),
            ("#fmd-heading-3", "fn-evidence"),
        ]
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.code == "svg_link_unresolved")
    );
    assert!(!svg.contains("aria-label=\"ambiguous\""));
    assert!(
        !outline(&svg)
            .iter()
            .any(|(_, label)| label.contains("Only in notes") || *label == "Notes")
    );
    assert!(svg.contains("href=\"#fmd-note-1\"") && svg.contains("href=\"#fmd-note-ref-1\""));
    let destinations = views(&svg);
    assert!(destinations.windows(2).all(|pair| pair[0].1 < pair[1].1));
    assert_link_geometry(&svg);
}

#[test]
fn nested_body_markers_own_placement_while_note_markers_do_not_suppress_automatic_toc() {
    let options = SvgOptions {
        toc: true,
        ..SvgOptions::default()
    };
    for marker in ["> [TOC]", "- [TOC]"] {
        let (svg, warnings) = render(&format!("# Body\n\n{marker}\n"), &options);
        assert!(warnings.is_empty());
        assert_eq!(outline(&svg), [("#fmd-heading-0", "Body")]);
        let link = svg
            .lines()
            .find(|line| line.starts_with("<a href=\"#fmd-heading-"))
            .unwrap();
        let y: f64 = attr(link.split_once("<rect ").unwrap().1, "y")
            .parse()
            .unwrap();
        assert!(
            y > views(&svg)[0].1,
            "the explicit marker stays below the heading"
        );
    }
    let (svg, warnings) = render(
        "Intro[^n].\n\n# Body\n\n[^n]: [TOC]\n\n    ## Private\n",
        &options,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        outline(&svg),
        [("#fmd-heading-0", "Body"), ("#fmd-heading-0", "Body")]
    );
    assert_eq!(views(&svg).len(), 1);
    assert_link_geometry(&svg);
}

#[test]
fn automatic_outline_never_uses_generated_or_note_only_headings() {
    let source = "Body[^n].\n\n[^n]: ## Note heading\n";
    let document = parse_markdown(source);
    assert_eq!(
        render_svg(&document, &SvgOptions::default()),
        render_svg(
            &document,
            &SvgOptions {
                toc: true,
                ..SvgOptions::default()
            }
        )
    );
    assert_eq!(
        render_svg(&parse_markdown("[TOC]"), &SvgOptions::default()),
        render_svg(&Document::default(), &SvgOptions::default())
    );
    let (svg, warnings) = render(
        "`[TOC]`\n\n**[TOC]**\n\nPrefix [TOC]\n\n```text\n[TOC]\n```\n\n# Body\n",
        &SvgOptions::default(),
    );
    assert!(warnings.is_empty());
    assert!(outline(&svg).is_empty());
}

#[test]
fn empty_headings_and_formatted_titles_have_exact_distinct_destinations() {
    let document = Document {
        blocks: vec![
            Block::Paragraph(vec![Inline::Text("[TOC]".into())]),
            Block::Heading {
                level: 1,
                inlines: vec![],
            },
            Block::Heading {
                level: 2,
                inlines: vec![],
            },
            Block::Heading {
                level: 2,
                inlines: vec![
                    Inline::Strong(vec![Inline::Text("A & ".into())]),
                    Inline::Link {
                        dest: "https://example.com".into(),
                        title: None,
                        content: vec![Inline::Code("B".into())],
                    },
                ],
            },
        ],
    };
    let result = render_svg_with_diagnostics(&document, &SvgOptions::default());
    assert!(result.2.is_empty());
    let svg = String::from_utf8(result.0).unwrap();
    assert_eq!(
        outline(&svg),
        [
            ("#fmd-heading-0", "Section"),
            ("#fmd-heading-1", "Section"),
            ("#fmd-heading-2", "A &amp; B"),
        ]
    );
    assert_eq!(svg.matches("href=\"https://example.com\"").count(), 1);
    assert_link_geometry(&svg);
}

#[test]
fn narrow_long_outlines_wrap_and_all_entry_points_remain_deterministic() {
    let title =
        "A long multibyte résumé title with several words and aVeryLongUnbrokenSourceToken";
    let document = parse_markdown(&format!("[TOC]\n\n# {title}\n\n###### Deep\n"));
    let options = SvgOptions {
        max_width_pt: 144.0,
        ..SvgOptions::default()
    };
    let result = render_svg_with_diagnostics(&document, &options);
    assert!(result.2.is_empty(), "{:?}", result.2);
    assert_eq!(
        result,
        render_svg_with_diagnostics(&document.clone(), &options)
    );
    assert_eq!(result.0, render_svg(&document, &options));
    assert_eq!(
        (result.0.clone(), result.1),
        render_svg_with_report(&document, &options)
    );
    assert_eq!(
        result,
        render_svg_with_resources(&document, &options, &FontAssets::default(), &[]).unwrap()
    );
    let svg = String::from_utf8(result.0).unwrap();
    assert!(
        outline(&svg)
            .iter()
            .filter(|(_, label)| *label == title)
            .count()
            > 1
    );
    assert!(outline(&svg).iter().any(|(_, label)| *label == "Deep"));
    assert_link_geometry(&svg);
}
