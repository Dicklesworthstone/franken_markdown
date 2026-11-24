//! Public SVG output checks for citation-bearing financial statements.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use franken_markdown::parse_markdown;
use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::theme::{FontFamily, Theme};

fn render(source: &str, options: &SvgOptions) -> String {
    let (bytes, report, warnings) = render_svg_with_diagnostics(&parse_markdown(source), options);
    assert_eq!(report.glyphs_missing, 0);
    assert!(warnings.is_empty(), "{warnings:?}");
    String::from_utf8(bytes).unwrap()
}

fn attribute<'a>(element: &'a str, name: &str) -> &'a str {
    let key = format!("{name}=\"");
    let start = element
        .match_indices(&key)
        .find(|(offset, _)| *offset == 0 || element.as_bytes()[offset - 1].is_ascii_whitespace())
        .unwrap()
        .0
        + key.len();
    element[start..].split_once('"').unwrap().0
}

fn glyph_paths(svg: &str) -> impl Iterator<Item = (&str, &str)> {
    svg.lines()
        .filter(|line| line.starts_with("<path id=\"g"))
        .map(|line| (attribute(line, "id"), attribute(line, "d")))
}

/// Find glyph instances by their actual outline, without depending on a
/// document's generated glyph IDs or the renderer's decimal classifier.
fn matching_origins(svg: &str, paths: &BTreeSet<String>) -> Vec<(f64, f64)> {
    let ids: BTreeSet<_> = glyph_paths(svg)
        .filter(|(_, path)| paths.contains(*path))
        .map(|(id, _)| format!("#{id}"))
        .collect();
    svg.lines()
        .filter(|line| line.starts_with("<use ") && ids.contains(attribute(line, "href")))
        .map(|line| {
            let transform = attribute(line, "transform");
            let coordinates = transform
                .strip_prefix("translate(")
                .unwrap()
                .split_once(')')
                .unwrap()
                .0;
            let (x, y) = coordinates.split_once(' ').unwrap();
            (x.parse().unwrap(), y.parse().unwrap())
        })
        .collect()
}

#[test]
fn emitted_svg_aligns_cited_amounts_and_keeps_distinct_note_and_value_links() {
    let source = "| Account | Amount |\n| --- | ---. |\n\
        | Receipts | 1,234.5[^audit] |\n\
        | Expenses | (12.50)[^tax][^audit] |\n\
        | Balance | **1,221.00[^audit]** |\n\
        | Yield | *2.375%[^tax]* |\n\
        | Source | [7.00[^audit]](https://example.com/statement) |\n\n\
        [^audit]: Audited amount\n\
        [^tax]: Tax basis\n";
    for family in [FontFamily::Sans, FontFamily::Serif] {
        let options = SvgOptions {
            theme: Theme::default().with_font(family),
            ..Default::default()
        };
        let svg = render(source, &options);
        assert_eq!(svg, render(source, &options));

        // Each standalone witness supplies one real glyph outline. Compare
        // the decimal origins serialized in the complete publication, across
        // regular, bold, italic, and linked text with different suffix widths.
        let dot_paths: BTreeSet<String> = [".", "**.**", "*.*"]
            .iter()
            .flat_map(|source| {
                let witness = render(source, &options);
                glyph_paths(&witness)
                    .map(|(_, path)| path.to_owned())
                    .collect::<Vec<_>>()
            })
            .collect();
        let origins = matching_origins(&svg, &dot_paths);
        assert_eq!(origins.len(), 5);
        assert!(
            origins
                .iter()
                .all(|(x, _)| (x - origins[0].0).abs() <= 0.01),
            "{family:?}: {origins:?}"
        );
        assert!(origins.windows(2).all(|rows| rows[0].1 < rows[1].1));
        assert!(svg.contains("href=\"https://example.com/statement\""));
        for number in [1, 2] {
            assert!(svg.contains(&format!("<view id=\"fmd-note-{number}\"")));
            assert!(svg.contains(&format!("href=\"#fmd-note-{number}\"")));
            assert!(svg.contains(&format!("href=\"#fmd-note-ref-{number}\"")));
        }
        let note_links: Vec<_> = svg
            .lines()
            .filter(|line| line.starts_with("<a href=\"#fmd-note-1\""))
            .collect();
        assert_eq!(note_links.len(), 3);
        for line in note_links {
            let rect = line.split_once("<rect ").unwrap().1;
            let x: f64 = attribute(rect, "x").parse().unwrap();
            let width: f64 = attribute(rect, "width").parse().unwrap();
            assert!(x > origins[0].0, "citation hit target includes the amount");
            assert!(width > 0.0 && width < 20.0);
        }
    }
}

#[test]
fn wrapped_cited_svg_cells_keep_complete_glyphs_and_note_targets() {
    let source = "| Amount |\n| ---. |\n\
        | 123456789.0123456789123456789[^a][^b] |\n\n\
        [^a]: First evidence\n\
        [^b]: Second evidence\n";
    let mut theme = Theme::default();
    theme.page.margins.left_pt = 20.0;
    theme.page.margins.right_pt = 20.0;
    let options = SvgOptions {
        theme,
        max_width_pt: 144.0,
        ..Default::default()
    };
    let actual = render(source, &options);
    let right_aligned = render(&source.replace("---.", "---:"), &options);
    // The decimal allocator may reserve a wider column, but its wrapped
    // fallback must preserve every glyph in the ordinary right-aligned cell.
    let glyphs = |svg: &str| {
        svg.lines()
            .filter(|line| line.starts_with("<use href=\"#g"))
            .map(|line| attribute(line, "href").to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(glyphs(&actual), glyphs(&right_aligned));
    let table_bottom: f64 = actual
        .lines()
        .filter(|line| line.starts_with("<line ") && attribute(line, "stroke-width") == "0.75")
        .nth(1)
        .map(|line| attribute(line, "y1").parse().unwrap())
        .unwrap();
    let table_baselines: BTreeSet<_> = actual
        .lines()
        .filter(|line| line.starts_with("<use href=\"#g"))
        .filter_map(|line| {
            let position = attribute(line, "transform")
                .strip_prefix("translate(")?
                .split_once(')')?
                .0;
            let (_, y) = position.split_once(' ')?;
            (y.parse::<f64>().unwrap() < table_bottom).then(|| y.to_owned())
        })
        .collect();
    assert!(
        table_baselines.len() >= 3,
        "header plus multiple amount lines"
    );
    assert!(actual.contains("href=\"#fmd-note-1\""));
    assert!(actual.contains("href=\"#fmd-note-2\""));
    for line in actual.lines().filter(|line| {
        line.starts_with("<a href=\"#fmd-note-1\"") || line.starts_with("<a href=\"#fmd-note-2\"")
    }) {
        let rect = line.split_once("<rect ").unwrap().1;
        let x: f64 = attribute(rect, "x").parse().unwrap();
        let width: f64 = attribute(rect, "width").parse().unwrap();
        assert!(x >= 20.0 && x + width <= 124.01);
    }
}
