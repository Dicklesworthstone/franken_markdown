#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use franken_markdown::{Document, parse_markdown};

#[test]
fn outline_inventory_follows_body_order_and_excludes_prepared_notes() {
    let source = parse_markdown(
        "[TOC]\n\n# First[^n]\n\n> ## Quoted\n\n- ### Listed\n\n<h2>HTML</h2>\n\n[^n]: ## Note heading\n",
    );
    let prepared = PreparedEndnotes::new(&source);
    let mut navigation = Navigation::default();
    navigation.prepare_endnotes(&prepared);
    let mut contents = Contents::new(&SvgOptions::default());
    contents.prepare(&prepared, &navigation);
    assert!(contents.active());
    assert!(!contents.limited());
    assert_eq!(
        contents
            .entries
            .iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        ["First", "Quoted", "Listed", "HTML"]
    );
    assert!(prepared.body_blocks().len() < prepared.document().blocks.len());
    assert!(!contents.automatic);
}

#[test]
fn complete_outline_limits_fall_back_without_partial_rows() {
    for blocks in [
        (0..=MAX_HEADINGS)
            .map(|_| Block::Heading {
                level: 1,
                inlines: vec![],
            })
            .collect(),
        vec![Block::Heading {
            level: 1,
            inlines: vec![Inline::Text("x".repeat(MAX_TITLE_BYTES + 1))],
        }],
        (0..17)
            .map(|_| Block::Heading {
                level: 1,
                inlines: vec![Inline::Text("x".repeat(MAX_TITLE_BYTES))],
            })
            .collect(),
    ] {
        let source = Document { blocks };
        let prepared = PreparedEndnotes::new(&source);
        let mut contents = Contents::new(&SvgOptions {
            toc: true,
            ..SvgOptions::default()
        });
        contents.prepare(&prepared, &Navigation::default());
        assert!(contents.limited());
        assert!(!contents.active());
        assert!(!contents.automatic);
        assert!(contents.entries.is_empty());
        assert_eq!(prepared.body_blocks().len(), source.blocks.len());
    }
}

#[test]
fn distinct_empty_headings_have_distinct_transient_targets() {
    let source = Document {
        blocks: vec![
            Block::Heading {
                level: 1,
                inlines: vec![],
            },
            Block::Heading {
                level: 2,
                inlines: vec![],
            },
        ],
    };
    let prepared = PreparedEndnotes::new(&source);
    let mut contents = Contents::new(&SvgOptions {
        toc: true,
        ..SvgOptions::default()
    });
    contents.prepare(&prepared, &Navigation::default());
    assert_eq!(contents.entries.len(), 2);
    assert_ne!(contents.entries[0].target, contents.entries[1].target);
    assert_eq!(contents.entries[0].title, "Section");
    assert!(contents.take_automatic(contents.entries[0].target));
    assert!(!contents.take_automatic(contents.entries[1].target));
}

#[test]
fn repeated_outline_limits_keep_markers_visible_and_prior_forward_links_live() {
    let source = parse_markdown("[TOC]\n\n[TOC]\n\n[TOC]\n\n# First\n\n## Second\n");
    let mut poster = Poster::new(&SvgOptions::default());
    // The same production admission path, with a small remaining row budget:
    // the first outline fits exactly, the two later markers must stay visible.
    poster.contents.remaining_rows = 2;
    let height = poster.layout(&source);
    assert!(poster.contents.expansion_limited);
    assert!(poster.contents.active());
    let (bytes, report, warnings) = poster.emit(height);
    let svg = String::from_utf8(bytes).unwrap();
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.code == "svg_toc_limit")
            .count(),
        1
    );
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.code == "svg_link_unresolved")
    );
    assert!(
        svg.contains("<a href=\"#fmd-heading-0\"") && svg.contains("<a href=\"#fmd-heading-1\"")
    );
    assert!(
        svg.contains("<view id=\"fmd-heading-0\"") && svg.contains("<view id=\"fmd-heading-1\"")
    );
    let ordinary = parse_markdown("[TOC]\n\n# First\n\n## Second\n");
    let (_, baseline) =
        super::super::render_svg_with_report(&ordinary, &SvgOptions::default());
    assert_eq!(
        report.glyphs_drawn,
        baseline.glyphs_drawn + 2 * "[TOC]".len()
    );
}

#[test]
fn expanded_text_budget_admits_whole_filtered_outlines_only() {
    let source = parse_markdown("# First\n\n## Second\n");
    let prepared = PreparedEndnotes::new(&source);
    let mut contents = Contents::new(&SvgOptions {
        toc: true,
        ..SvgOptions::default()
    });
    contents.prepare(&prepared, &Navigation::default());
    contents.remaining_text = "First".len();
    assert!(contents.reserve(Some(1)));
    assert_eq!(contents.remaining_text, 0);
    let rows = contents.remaining_rows;
    assert!(!contents.reserve(None));
    assert_eq!(
        contents.remaining_rows, rows,
        "a failed reservation paints no partial rows"
    );
    assert!(
        contents.active(),
        "previous targets must still bind at paint time"
    );
}
