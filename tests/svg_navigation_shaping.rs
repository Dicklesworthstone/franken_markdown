//! Navigation metadata must preserve the independently specified font's shape
//! while retaining each source destination in the emitted hit-test geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::svg::{SvgOptions, SvgReport, SvgWarning, render_svg_with_resources};
use franken_markdown::{Block, Document, FontAssets, Inline};

const FONT: &[u8] = include_bytes!("../fmd-font/fonts/test-shaping/FmdShaping.ttf");

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}

fn link(target: &str, content: Vec<Inline>) -> Inline {
    Inline::Link {
        dest: target.to_owned(),
        title: None,
        content,
    }
}

fn linked(target: &str, value: &str) -> Inline {
    link(target, vec![text(value)])
}

fn render(
    inlines: Vec<Inline>,
    narrow: bool,
    custom_font: bool,
) -> (String, SvgReport, Vec<SvgWarning>) {
    let mut options = SvgOptions::default();
    if narrow {
        options.max_width_pt = 144.0;
        options.theme.page.margins.left_pt = 36.0;
        options.theme.page.margins.right_pt = 36.0;
    }
    let fonts = FontAssets {
        body_regular: custom_font.then(|| FONT.to_vec()),
        ..FontAssets::default()
    };
    let (bytes, report, warnings) = render_svg_with_resources(
        &Document {
            blocks: vec![Block::Paragraph(inlines)],
        },
        &options,
        &fonts,
        &[],
    )
    .unwrap();
    (String::from_utf8(bytes).unwrap(), report, warnings)
}

fn paint(svg: &str) -> String {
    svg.lines()
        .filter(|line| !line.starts_with("<a ") && !line.starts_with("<view "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn anchors(svg: &str) -> Vec<&str> {
    svg.lines().filter(|line| line.starts_with("<a ")).collect()
}

fn attr<'a>(element: &'a str, name: &str) -> &'a str {
    element
        .split_once(&format!(" {name}=\""))
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
}

fn interval(anchor: &str) -> (f64, f64) {
    let start: f64 = attr(anchor, "x").parse().unwrap();
    let width: f64 = attr(anchor, "width").parse().unwrap();
    (start, start + width)
}

fn close(actual: f64, expected: f64) {
    // Hit regions round outwards to 0.01pt, independently of glyph rounding.
    assert!((actual - expected).abs() <= 0.011, "{actual} != {expected}");
}

#[test]
fn adjacent_links_keep_the_fixture_pair_adjustment_and_their_own_targets() {
    // This fixture's saved HarfBuzz reference has a/b advances 460/500 units.
    // At the default 11pt size, b must start 5.06pt after a, not 5.5pt.
    let actual = render(
        vec![
            linked("https://a.example", "a"),
            linked("https://b.example", "b"),
        ],
        false,
        true,
    );
    let whole = render(vec![linked("https://whole.example", "ab")], false, true);
    assert!(actual.2.is_empty(), "{:?}", actual.2);
    assert_eq!(paint(&actual.0), paint(&whole.0));
    assert_eq!(actual.1, whole.1);
    assert_eq!(actual.1.glyphs_drawn, 2);
    let regions = anchors(&actual.0);
    assert_eq!(regions.len(), 2);
    assert_eq!(attr(regions[0], "href"), "https://a.example");
    assert_eq!(attr(regions[1], "href"), "https://b.example");
    let first = interval(regions[0]);
    let second = interval(regions[1]);
    close(first.1 - first.0, 5.06);
    close(second.0 - first.0, 5.06);
    close(second.1 - first.0, 10.56);
}

#[test]
fn a_cross_link_ligature_is_painted_once_with_two_distinct_hit_regions() {
    // fi is one 500-unit glyph in the fixture, not two 500-unit glyphs.
    let actual = render(
        vec![
            linked("https://f.example", "f"),
            linked("https://i.example", "i"),
        ],
        false,
        true,
    );
    let whole = render(vec![linked("https://whole.example", "fi")], false, true);
    assert!(actual.2.is_empty(), "{:?}", actual.2);
    assert_eq!(paint(&actual.0), paint(&whole.0));
    assert_eq!(actual.1.glyphs_drawn, 1);
    let regions = anchors(&actual.0);
    assert_eq!(regions.len(), 2);
    assert_eq!(attr(regions[0], "href"), "https://f.example");
    assert_eq!(attr(regions[1], "href"), "https://i.example");
    let first = interval(regions[0]);
    let second = interval(regions[1]);
    close(first.1 - first.0, 2.75);
    close(second.0, first.1);
    close(second.1 - first.0, 5.5);
}

#[test]
fn a_navigation_boundary_cannot_detach_a_combining_mark_or_steal_its_target() {
    let actual = render(
        vec![
            linked("https://base.example", "Cafe"),
            linked("https://mark.example", "\u{0301}"),
            linked("https://tail.example", "ine"),
        ],
        false,
        false,
    );
    let whole = render(
        vec![linked("https://whole.example", "Cafe\u{0301}ine")],
        false,
        false,
    );
    assert_eq!(actual.2, whole.2);
    assert_eq!(paint(&actual.0), paint(&whole.0));
    assert_eq!(actual.1, whole.1);
    let regions = anchors(&actual.0);
    assert_eq!(regions.len(), 3);
    for (region, target) in regions.iter().zip([
        "https://base.example",
        "https://mark.example",
        "https://tail.example",
    ]) {
        assert_eq!(attr(region, "href"), target);
        let (left, right) = interval(region);
        assert!(right > left);
    }
    close(interval(regions[0]).1, interval(regions[1]).0);
    close(interval(regions[1]).1, interval(regions[2]).0);
}

#[test]
fn emergency_wrapping_preserves_cross_owner_clusters_and_each_destination() {
    let mut inlines = Vec::new();
    for _ in 0..60 {
        inlines.push(linked("https://f.example", "f"));
        inlines.push(linked("https://i.example", "i"));
    }
    let actual = render(inlines, true, true);
    let whole = render(
        vec![linked("https://whole.example", &"fi".repeat(60))],
        true,
        true,
    );
    assert!(actual.2.is_empty(), "{:?}", actual.2);
    assert_eq!(paint(&actual.0), paint(&whole.0));
    assert_eq!(actual.1.glyphs_drawn, 60);
    let regions = anchors(&actual.0);
    assert_eq!(regions.len(), 120);
    for pair in regions.chunks_exact(2) {
        assert_eq!(attr(pair[0], "href"), "https://f.example");
        assert_eq!(attr(pair[1], "href"), "https://i.example");
        assert_eq!(attr(pair[0], "y"), attr(pair[1], "y"));
        close(interval(pair[0]).1, interval(pair[1]).0);
        assert!(interval(pair[0]).0 >= 35.99);
        assert!(interval(pair[1]).1 <= 108.01);
    }
    let mut rows: Vec<_> = regions.iter().map(|line| attr(line, "y")).collect();
    rows.sort_unstable();
    rows.dedup();
    assert!(rows.len() >= 4);
}

#[test]
fn authored_breaks_keep_hidden_and_selected_hint_ownership_separate() {
    for (prefix, suffix, selected) in [
        ("f".to_owned(), "i".to_owned(), false),
        ("ab".repeat(6), "ab".repeat(6), true),
    ] {
        let actual = render(
            vec![
                linked("https://prefix.example", &prefix),
                linked("https://hint.example", "\u{00ad}"),
                linked("https://suffix.example", &suffix),
            ],
            true,
            true,
        );
        let source = format!("{prefix}\u{00ad}{suffix}");
        let whole = render(vec![linked("https://whole.example", &source)], true, true);
        assert_eq!(actual.2, whole.2);
        assert_eq!(paint(&actual.0), paint(&whole.0));
        assert_eq!(actual.1, whole.1);
        let regions = anchors(&actual.0);
        assert_eq!(
            regions
                .iter()
                .filter(|line| attr(line, "href") == "https://hint.example")
                .count(),
            usize::from(selected)
        );
        assert!(
            regions
                .iter()
                .any(|line| attr(line, "href") == "https://prefix.example")
        );
        assert!(
            regions
                .iter()
                .any(|line| attr(line, "href") == "https://suffix.example")
        );
    }
}

#[test]
fn unsafe_inner_links_do_not_inherit_a_coalesced_outer_destination() {
    let actual = render(
        vec![link(
            "https://outer.example",
            vec![text("a"), linked("javascript:bad", "b"), text("fi")],
        )],
        false,
        true,
    );
    let whole = render(vec![linked("https://whole.example", "abfi")], false, true);
    assert_eq!(paint(&actual.0), paint(&whole.0));
    assert_eq!(actual.1.glyphs_drawn, 3);
    assert_eq!(
        actual
            .2
            .iter()
            .filter(|warning| warning.code == "svg_link_unsafe")
            .count(),
        1
    );
    let regions = anchors(&actual.0);
    assert_eq!(regions.len(), 2);
    assert!(
        regions
            .iter()
            .all(|line| attr(line, "href") == "https://outer.example")
    );
    let first = interval(regions[0]);
    let last = interval(regions[1]);
    close(first.1 - first.0, 5.06);
    close(last.0 - first.0, 10.56);
    close(last.1 - last.0, 5.5);
    assert!(!actual.0.contains("javascript"));
}

#[test]
fn navigation_coalescing_retains_the_existing_paragraph_work_limit() {
    let inlines = (0..8193)
        .map(|index| {
            linked(
                if index % 2 == 0 {
                    "https://a.example"
                } else {
                    "https://b.example"
                },
                "a",
            )
        })
        .collect();
    let (_, report, warnings) = render(inlines, false, true);
    assert_eq!(report.glyphs_drawn, 8193);
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.code == "svg_paragraph_limit")
            .count(),
        1
    );
}
