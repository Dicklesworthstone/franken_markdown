//! Exercise actual SVG views/hit regions and exact, immutable note provenance.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::svg::{
    SvgOptions, render_svg, render_svg_with_diagnostics, render_svg_with_resources,
};
use franken_markdown::{
    Block, Document, EndnoteTarget, FontAssets, Inline, PreparedEndnotes, parse_markdown,
};

fn render(doc: &Document) -> (String, Vec<franken_markdown::svg::SvgWarning>) {
    let (bytes, _, warnings) = render_svg_with_diagnostics(doc, &SvgOptions::default());
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

fn anchors(svg: &str) -> Vec<&str> {
    svg.lines().filter(|line| line.starts_with("<a ")).collect()
}

fn named_link<'a>(svg: &'a str, label: &str) -> &'a str {
    anchors(svg)
        .into_iter()
        .find(|anchor| attr(anchor, "aria-label") == label)
        .unwrap()
}

fn has_link(svg: &str, label: &str) -> bool {
    anchors(svg)
        .iter()
        .any(|anchor| attr(anchor, "aria-label") == label)
}

fn view_top(svg: &str, id: &str) -> f64 {
    let view = svg
        .lines()
        .find(|line| line.starts_with("<view ") && attr(line, "id") == id)
        .unwrap();
    attr(view, "viewBox")
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

fn without_note_navigation(svg: &str) -> String {
    svg.lines()
        .filter(|line| {
            let value = line
                .strip_prefix("<view id=\"")
                .or_else(|| line.strip_prefix("<a href=\"#"));
            !value
                .and_then(|value| value.split_once('"'))
                .is_some_and(|(id, _)| {
                    id.strip_prefix("fmd-note-ref-")
                        .or_else(|| id.strip_prefix("fmd-note-"))
                        .is_some_and(|number| {
                            !number.is_empty()
                                && !number.starts_with('0')
                                && number.bytes().all(|byte| byte.is_ascii_digit())
                        })
                })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_unchanged_paint(svg: &str, legacy: &str) {
    let actual = without_note_navigation(svg);
    let expected = legacy.trim_end();
    assert!(
        actual == expected,
        "paint or preexisting navigation changed: actual {} bytes, expected {} bytes; first differing byte {:?}",
        actual.len(),
        expected.len(),
        actual
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
    );
}

fn assert_destinations_exist(svg: &str) {
    let views: Vec<_> = svg
        .lines()
        .filter(|line| line.starts_with("<view "))
        .map(|line| attr(line, "id"))
        .collect();
    let mut unique = views.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        views.len(),
        "duplicate emitted view identifiers"
    );
    for anchor in anchors(svg) {
        if let Some(id) = attr(anchor, "href").strip_prefix('#') {
            assert!(views.contains(&id), "dangling navigation: {anchor}");
        }
    }
}

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}
fn reference(id: &str) -> Inline {
    Inline::FootnoteRef { id: id.to_owned() }
}
fn paragraph(value: &str) -> Block {
    Block::Paragraph(vec![text(value)])
}
fn definition(id: &str, blocks: Vec<Block>) -> Block {
    Block::FootnoteDefinition {
        id: id.to_owned(),
        blocks,
    }
}
fn link(label: &str, destination: &str) -> Inline {
    Inline::Link {
        dest: destination.to_owned(),
        title: None,
        content: vec![text(label)],
    }
}

#[test]
fn svg_footnotes_resolve_first_use_order_source_links_and_actual_return_positions() {
    let doc = parse_markdown(concat!(
        "# Evidence\n\nFirst[^beta], repeated[^beta], then[^alpha].\n\n",
        "[Beta note](#fn-beta) [Beta return](#fnref-1).\n\n",
        "[^alpha]: Alpha body.\n\n[^beta]: Beta body with [evidence](https://example.com).\n"
    ));
    let (svg, warnings) = render(&doc);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(attr(named_link(&svg, "Beta note"), "href"), "#fmd-note-1");
    assert_eq!(
        attr(named_link(&svg, "Beta return"), "href"),
        "#fmd-note-ref-1"
    );
    let references: Vec<_> = anchors(&svg)
        .into_iter()
        .filter(|anchor| attr(anchor, "aria-label") == "Footnote 1")
        .collect();
    assert_eq!(references.len(), 2);
    assert!(
        references
            .iter()
            .all(|anchor| attr(anchor, "href") == "#fmd-note-1")
    );
    let first_y: f64 = attr(references[0], "y").parse().unwrap();
    assert!((first_y - view_top(&svg, "fmd-note-ref-1")).abs() <= 0.011);
    assert!(view_top(&svg, "fmd-note-1") > first_y);
    assert!(view_top(&svg, "fmd-note-2") > view_top(&svg, "fmd-note-1"));
    assert_eq!(
        attr(named_link(&svg, "Return to footnote 1 citation"), "href"),
        "#fmd-note-ref-1"
    );
    assert_destinations_exist(&svg);

    let legacy = String::from_utf8(render_svg(
        doc.with_endnotes().as_ref(),
        &SvgOptions::default(),
    ))
    .unwrap();
    assert_unchanged_paint(&svg, &legacy);
    assert_eq!(render(&doc).0, svg);
    let resource_bytes =
        render_svg_with_resources(&doc, &SvgOptions::default(), &FontAssets::default(), &[])
            .unwrap()
            .0;
    assert_eq!(resource_bytes, svg.as_bytes());
}

#[test]
fn svg_footnote_provenance_survives_moves_and_does_not_guess_from_visible_text() {
    let doc = Document {
        blocks: vec![
            Block::Paragraph(vec![
                text("literal ¹ and "),
                reference("fmd-fn-1"),
                text(" ¹"),
            ]),
            definition("fmd-fn-1", vec![paragraph("¹ author-provided body prefix")]),
        ],
    };
    let original = doc.clone();
    let prepared = PreparedEndnotes::new(&doc);
    assert_eq!(prepared.document(), doc.with_endnotes().as_ref());
    let mut before = Vec::new();
    prepared.for_each_inline_target(|inline, target| {
        before.push((std::ptr::from_ref(inline) as usize, target))
    });
    assert_eq!(before.len(), 2);
    assert_eq!(before[0].1, EndnoteTarget::Note(1));
    assert_eq!(before[1].1, EndnoteTarget::Reference(1));
    let moved = Box::new(prepared);
    let mut after = Vec::new();
    moved.for_each_inline_target(|inline, target| {
        after.push((std::ptr::from_ref(inline) as usize, target))
    });
    assert_eq!(before, after);
    assert_eq!(doc, original);
    assert_eq!(
        moved.aliases().collect::<Vec<_>>(),
        vec![
            ("fn-fmd-fn-1", EndnoteTarget::Note(1)),
            ("fnref-1", EndnoteTarget::Reference(1)),
        ]
    );
    let (svg, warnings) = render(&doc);
    assert!(warnings.is_empty());
    assert_eq!(
        anchors(&svg).len(),
        2,
        "literal superscripts acquired navigation"
    );
}

#[test]
fn svg_footnote_links_preserve_enclosing_links_and_return_to_the_first_nested_citation() {
    let doc = Document {
        blocks: vec![
            Block::Paragraph(vec![Inline::Link {
                dest: "https://example.com/owned".into(),
                title: None,
                content: vec![text("Source"), reference("n"), text(" remains linked")],
            }]),
            Block::Paragraph(vec![text("Later"), reference("n")]),
            definition("n", vec![paragraph("Note body")]),
        ],
    };
    let (svg, warnings) = render(&doc);
    assert!(warnings.is_empty(), "{warnings:?}");
    let generated: Vec<_> = anchors(&svg)
        .into_iter()
        .filter(|a| attr(a, "aria-label") == "Footnote 1")
        .collect();
    assert_eq!(
        generated.len(),
        1,
        "nested citation replaced its enclosing destination"
    );
    assert!(
        anchors(&svg)
            .iter()
            .any(|a| attr(a, "href") == "https://example.com/owned")
    );
    let later_y: f64 = attr(generated[0], "y").parse().unwrap();
    assert!(view_top(&svg, "fmd-note-ref-1") < later_y);
    assert_destinations_exist(&svg);
}

#[test]
fn svg_footnote_source_alias_collisions_are_inert_but_typed_links_remain_valid() {
    let doc = parse_markdown(concat!(
        "# fn-n\n\n## fnref-1\n\nClaim[^n].\n\n",
        "[Ambiguous note](#fn-n) [Ambiguous return](#fnref-1) [Real second heading](#fn-n-2).\n\n",
        "# fn-n\n\n[^n]: Note.\n"
    ));
    let (svg, warnings) = render(&doc);
    assert!(!has_link(&svg, "Ambiguous note"));
    assert!(!has_link(&svg, "Ambiguous return"));
    assert!(has_link(&svg, "Real second heading"));
    assert_eq!(attr(named_link(&svg, "Footnote 1"), "href"), "#fmd-note-1");
    assert_eq!(
        attr(named_link(&svg, "Return to footnote 1 citation"), "href"),
        "#fmd-note-ref-1"
    );
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.code == "svg_link_unresolved")
            .count(),
        1
    );
    assert_destinations_exist(&svg);
}

#[test]
fn svg_footnote_source_fragments_decode_once_and_never_target_private_svg_ids() {
    let doc = Document {
        blocks: vec![
            Block::Paragraph(vec![reference("%31"), text(" "), reference("1")]),
            Block::Paragraph(vec![
                link("Literal percent", "#fn-%2531"),
                text(" "),
                link("Decoded one", "#fn-%31"),
                text(" "),
                link("Private view", "#fmd-note-1"),
                text(" "),
                link("Malformed", "#fn-%GG"),
            ]),
            definition("%31", vec![paragraph("Literal-percent body")]),
            definition("1", vec![paragraph("Digit body")]),
        ],
    };
    let (svg, warnings) = render(&doc);
    assert_eq!(
        attr(named_link(&svg, "Literal percent"), "href"),
        "#fmd-note-1"
    );
    assert_eq!(attr(named_link(&svg, "Decoded one"), "href"), "#fmd-note-2");
    assert!(!has_link(&svg, "Private view"));
    assert!(!has_link(&svg, "Malformed"));
    assert!(
        warnings
            .iter()
            .any(|warning| warning.code == "svg_link_unsafe")
    );
    assert_destinations_exist(&svg);
}

#[test]
fn svg_footnote_cycles_rich_leading_blocks_and_orphan_notes_keep_complete_layout() {
    let mut doc = parse_markdown(concat!(
        "Claim[^figure].\n\n[^figure]: ![Plot](data:image/svg+xml;base64,",
        "PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSI0MCIgaGVpZ2h0PSIyMCI+",
        "PHJlY3Qgd2lkdGg9IjQwIiBoZWlnaHQ9IjIwIiBmaWxsPSJyZWQiLz48L3N2Zz4=)\n\n",
        "    Another[^table].\n\n[^table]: | Name | Value |\n    | --- | --- |\n    | Result | $x^2$ |\n\n",
        "    Back[^figure].\n\n    ```rust\n    let result = 42;\n    ```\n"
    ));
    doc.blocks.push(definition(
        "orphan",
        vec![
            Block::MathBlock("\\frac{1}{2}".into()),
            paragraph("Complete orphan"),
        ],
    ));
    doc.blocks
        .insert(1, Block::Paragraph(vec![link("Orphan body", "#fn-orphan")]));
    let (svg, warnings) = render(&doc);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(svg.contains("<image "));
    assert_eq!(attr(named_link(&svg, "Orphan body"), "href"), "#fmd-note-3");
    assert!(!has_link(&svg, "Return to footnote 3 citation"));
    assert!(view_top(&svg, "fmd-note-ref-2") > view_top(&svg, "fmd-note-1"));
    let legacy = String::from_utf8(render_svg(
        doc.with_endnotes().as_ref(),
        &SvgOptions::default(),
    ))
    .unwrap();
    assert_unchanged_paint(&svg, &legacy);
    assert_destinations_exist(&svg);
}

#[test]
fn svg_footnote_positions_are_recorded_at_paint_time_in_tables_and_nested_lists() {
    let doc = parse_markdown(concat!(
        "| First | Second |\n| --- | --- |\n| Citation[^n] | Other[^m] |\n\n",
        "- Nested[^n]\n\n[^n]: First note.\n\n[^m]: Second note.\n"
    ));
    let (svg, warnings) = render(&doc);
    assert!(warnings.is_empty(), "{warnings:?}");
    for number in 1..=2 {
        let citation = named_link(&svg, &format!("Footnote {number}"));
        let painted_y: f64 = attr(citation, "y").parse().unwrap();
        assert!((painted_y - view_top(&svg, &format!("fmd-note-ref-{number}"))).abs() <= 0.011);
    }
    assert_destinations_exist(&svg);
}

#[test]
fn svg_footnote_free_documents_and_explicit_legacy_preparation_remain_unchanged() {
    let doc =
        parse_markdown("# One\n\nText **bold** [One](#one) and [web](https://example.com).\n");
    let prepared = PreparedEndnotes::new(&doc);
    assert!(std::ptr::eq(prepared.document(), &doc));
    assert!(prepared.aliases().next().is_none());
    assert!(!prepared.navigation_limited());
    assert_eq!(render(&doc), render(prepared.document()));

    let doc = Document {
        blocks: vec![Block::Paragraph(vec![reference("missing"), text(" ¹")])],
    };
    let prepared = PreparedEndnotes::new(&doc);
    let mut generated = 0;
    prepared.for_each_inline_target(|_, _| generated += 1);
    assert_eq!(generated, 0);
    let (svg, warnings) = render(&doc);
    assert!(anchors(&svg).is_empty());
    assert!(warnings.is_empty());
}

#[test]
fn svg_footnote_metadata_limits_preserve_all_prepared_content() {
    let doc = Document {
        blocks: vec![
            Block::Paragraph((0..16_385).map(|_| reference("n")).collect()),
            definition("n", vec![paragraph("The complete final note")]),
        ],
    };
    let prepared = PreparedEndnotes::new(&doc);
    assert!(prepared.navigation_limited());
    assert_eq!(prepared.document(), doc.with_endnotes().as_ref());
    let mut count = 0;
    prepared.for_each_inline_target(|_, _| count += 1);
    assert_eq!(count, 16_384);
}
