//! Document intelligence must describe destinations and headings that publication emits.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::doc_stats::{DocumentStats, compute_doc_stats};
use franken_markdown::{Block, Document, HtmlOptions, Inline, html, parse_markdown};

fn assert_broken_links(stats: &DocumentStats, labels: &[&str]) {
    let findings: Vec<_> = stats
        .findings
        .iter()
        .filter(|finding| finding.code == "broken_internal_anchor")
        .collect();
    assert_eq!(findings.len(), labels.len(), "{findings:?}");
    for label in labels {
        assert!(
            findings.iter().any(|finding| finding
                .message
                .starts_with(&format!("link '{label}' targets"))),
            "missing broken link {label:?}: {findings:?}"
        );
    }
}

fn assert_outline(stats: &DocumentStats, html: &str, expected: &[(u8, &str, &str)]) {
    let outline: Vec<_> = stats
        .outline
        .iter()
        .map(|heading| (heading.level, heading.text.as_str(), heading.slug.as_str()))
        .collect();
    assert_eq!(outline.as_slice(), expected);
    let mut previous = None;
    for (level, _, slug) in expected {
        let tag = format!("<h{level} id=\"{slug}\">");
        let position = html.find(&tag).expect("outline target must be published");
        assert!(
            previous.is_none_or(|previous| previous < position),
            "outline must follow published heading order: {html}"
        );
        previous = Some(position);
    }
}

#[test]
fn collision_safe_note_and_numeric_reference_anchors_resolve_repeated_citations() {
    let source = "# fn-alpha\n\n# fnref-1\n\n\
        Claim[^alpha], other[^alpha-2], again[^alpha].\n\n\
        [Actual](#fn-alpha-3) [Second](#fn-alpha-2) \
        [Return](#fnref-1-2) [Second return](#fnref-2) \
        [Fake](#fnref-alpha) [Bare](#alpha) [Repeat](#fnref-3)\n\n\
        [^alpha]: First.\n\n[^alpha-2]: Second.\n";
    let doc = parse_markdown(source);
    let stats = compute_doc_stats(source, &doc);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    assert_outline(
        &stats,
        &rendered,
        &[(1, "fn-alpha", "fn-alpha"), (1, "fnref-1", "fnref-1")],
    );
    for id in ["fn-alpha-3", "fn-alpha-2", "fnref-1-2", "fnref-2"] {
        assert_eq!(rendered.matches(&format!("id=\"{id}\"")).count(), 1);
    }
    assert!(!rendered.contains("id=\"fnref-3\""));
    assert_eq!(stats.structure.footnote_definitions, 2);
    assert_eq!(stats.structure.footnote_references, 3);
    assert_broken_links(&stats, &["Fake", "Bare", "Repeat"]);
}

#[test]
fn unused_note_headings_and_links_do_not_change_published_outline_or_lints() {
    let source = "# Start\n\n\
        [^unused]: ##### Dup\n\n    [Hidden broken](#missing).\n\n\
        ## Dup\n\n\
        [Body](#dup) [Phantom heading](#dup-2) [Hidden note](#fn-unused).\n";
    let doc = parse_markdown(source);
    let stats = compute_doc_stats(source, &doc);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    assert_outline(
        &stats,
        &rendered,
        &[(1, "Start", "start"), (2, "Dup", "dup")],
    );
    assert!(!rendered.contains("<h5"));
    assert!(!rendered.contains("id=\"fn-unused\""));
    assert!(!rendered.contains("Hidden broken"));
    assert_broken_links(&stats, &["Phantom heading", "Hidden note"]);
    assert!(stats.findings.iter().all(|finding| !matches!(
        finding.code,
        "heading_hierarchy_skip" | "duplicate_heading_slug"
    )));
    assert!(stats.findings.iter().any(|finding| {
        finding.code == "unreferenced_footnote_def" && finding.message.contains("[^unused]")
    }));

    // Source inventory and readability still account for the authored note.
    assert_eq!(stats.structure.headings_total, 3);
    assert_eq!(stats.structure.headings_by_level, [1, 1, 0, 0, 1, 0]);
    assert_eq!(stats.structure.paragraphs, 2);
    assert_eq!(stats.structure.links_internal_anchors, 4);
    assert_eq!(stats.words, 10);
}

fn heading(level: u8, text: &str) -> Block {
    Block::Heading {
        level,
        inlines: vec![Inline::Text(text.to_owned())],
    }
}

fn note(id: &str, blocks: Vec<Block>) -> Block {
    Block::FootnoteDefinition {
        id: id.to_owned(),
        blocks,
    }
}

fn reference(id: &str) -> Inline {
    Inline::FootnoteRef { id: id.to_owned() }
}

fn link(label: &str, anchor: &str) -> Inline {
    Inline::Link {
        dest: format!("#{anchor}"),
        title: None,
        content: vec![Inline::Text(label.to_owned())],
    }
}

#[test]
fn first_use_order_nested_definitions_and_cycles_match_published_navigation() {
    let doc = Document {
        blocks: vec![
            note(
                "a",
                vec![
                    heading(3, "Shared"),
                    Block::Paragraph(vec![reference("b"), reference("a")]),
                ],
            ),
            note(
                "b",
                vec![
                    heading(2, "Shared"),
                    Block::Paragraph(vec![reference("inner"), reference("a")]),
                ],
            ),
            note(
                "unused",
                vec![
                    heading(6, "Hidden"),
                    note(
                        "inner",
                        vec![
                            heading(4, "Shared"),
                            Block::Paragraph(vec![
                                reference("b"),
                                link("Nested broken", "missing-nested"),
                            ]),
                        ],
                    ),
                ],
            ),
            note(
                "a",
                vec![
                    heading(6, "Phantom"),
                    Block::Paragraph(vec![link("Hidden duplicate", "missing")]),
                ],
            ),
            heading(1, "Shared"),
            Block::Paragraph(vec![
                reference("b"),
                reference("a"),
                reference("b"),
                link("B heading", "shared-2"),
                link("A heading", "shared-3"),
                link("Inner heading", "shared-4"),
                link("Nested body", "fn-inner"),
                link("Nested return", "fnref-3"),
                link("Ghost heading", "hidden"),
                link("Duplicate heading", "phantom"),
            ]),
        ],
    };
    let original = doc.clone();
    let stats = compute_doc_stats("", &doc);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    assert_eq!(doc, original, "analysis preserves the caller's AST");
    assert_outline(
        &stats,
        &rendered,
        &[
            (1, "Shared", "shared"),
            (2, "Shared", "shared-2"),
            (3, "Shared", "shared-3"),
            (4, "Shared", "shared-4"),
        ],
    );
    for id in ["fn-b", "fn-a", "fn-inner", "fnref-1", "fnref-2", "fnref-3"] {
        assert_eq!(rendered.matches(&format!("id=\"{id}\"")).count(), 1);
    }
    assert!(!rendered.contains("<h6"));
    assert!(!rendered.contains("Hidden duplicate"));
    assert_broken_links(
        &stats,
        &["Nested broken", "Ghost heading", "Duplicate heading"],
    );
    assert!(
        stats
            .findings
            .iter()
            .all(|finding| finding.code != "heading_hierarchy_skip")
    );
    assert_eq!(
        stats
            .findings
            .iter()
            .filter(|finding| finding.code == "duplicate_heading_slug")
            .count(),
        3
    );
    assert_eq!(stats.structure.headings_total, 6);
    assert_eq!(stats.structure.footnote_definitions, 5);
    assert_eq!(stats.structure.footnote_references, 8);
    assert_eq!(stats.structure.links_internal_anchors, 9);
}

#[test]
fn unicode_note_fragments_resolve_actual_collision_ids_and_decode_once() {
    let source = "# fn-café\n\nClaim[^café].\n\n\
        [Valid](#fn-caf%C3%A9-2) [Return](#fnref-1) \
        [Double](#fn-caf%25C3%25A9-2) [Case](#fn-Caf%C3%A9-2) \
        [Invalid](#fn-caf%ff-2)\n\n[^café]: Evidence.\n";
    let doc = parse_markdown(source);
    let stats = compute_doc_stats(source, &doc);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    assert!(rendered.contains("<li id=\"fn-café-2\">"));
    assert!(rendered.contains("id=\"fnref-1\""));
    assert_broken_links(&stats, &["Double", "Case", "Invalid"]);
}
