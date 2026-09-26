#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::validation::{AnchorKind, ReferenceKind, analyze_document_links, check_book_links};
use franken_markdown::{Block, Document, Inline, parse_markdown, parse_markdown_spanned};

fn link(destination: &str) -> Inline {
    Inline::Link { dest: destination.into(), title: None, content: vec![Inline::Text("go".into())] }
}
fn heading(title: &str) -> Block { Block::Heading { level: 2, inlines: vec![Inline::Text(title.into())] } }
fn note(id: &str, blocks: Vec<Block>) -> Block { Block::FootnoteDefinition { id: id.into(), blocks } }
fn reference(id: &str) -> Inline { Inline::FootnoteRef { id: id.into() } }

#[test]
fn standalone_checks_do_not_invent_a_filesystem_workspace() {
    let doc = parse_markdown("# Target\n\n[self](#target) [file](missing.md#target) [web](https://example.test/) [download](manual.zip)");
    let report = analyze_document_links(&doc).unwrap();
    assert_eq!((report.references.len(), report.external, report.unchecked), (1, 1, 2));
    assert!(report.references[0].finding.is_none());
    assert_eq!(report.references[0].target_block_index, Some(0));
    assert_eq!(report, analyze_document_links(&doc).unwrap());
}

#[test]
fn canonical_collision_ids_match_real_html_and_book_validation() {
    let doc = Document { blocks: vec![
        heading("Same"), heading("Same"), heading("Same 2"), heading("Same"),
        Block::Paragraph(vec![link("#same"), link("#same-2"), link("#same-2-2"), link("#same-3"), link("#absent")]),
    ] };
    let report = analyze_document_links(&doc).unwrap();
    let html = franken_markdown::html::render_fragment(&doc.blocks, &Default::default());
    for anchor in &report.anchors {
        assert_eq!(anchor.kind, AnchorKind::Heading);
        assert_eq!(anchor.occurrences, 1);
        assert!(html.contains(&format!("id=\"{}\"", anchor.id)), "{}", anchor.id);
    }
    assert_eq!(report.references.iter().map(|r| r.target_block_index).collect::<Vec<_>>(), [Some(0), Some(1), Some(2), Some(3), None]);
    let book = franken_markdown::Book { chapters: vec![franken_markdown::BookChapter {
        path: "document.md".into(), out_name: "document.html".into(), title: "Document".into(), frontmatter: None, doc,
    }] };
    let book_report = check_book_links(&book).unwrap();
    assert_eq!(report.references.iter().filter_map(|r| r.finding.clone()).collect::<Vec<_>>(), book_report.chapters[0].findings);
}

#[test]
fn invalid_encoded_and_empty_fragments_are_distinct() {
    let doc = Document { blocks: vec![heading("Here"), Block::Paragraph(vec![
        link("#%68ere"), link("?mode=read#here"), link("#%2568ere"), link("#%zz"),
        link("#%ff"), link("#%00"), link("#"), link(""), link("?mode=read"),
    ])] };
    let report = analyze_document_links(&doc).unwrap();
    let codes: Vec<_> = report.references.iter().map(|r| r.finding.as_ref().map(|f| f.code)).collect();
    assert_eq!(codes, [None, None, Some("missing_anchor"), Some("invalid_fragment"), Some("invalid_fragment"), Some("invalid_fragment"), None, None, None]);
    assert_eq!(report.references[0].target_block_index, Some(0));
    assert_eq!(report.references[6].target_block_index, None);
}

#[test]
fn nested_references_keep_actual_top_level_source_ownership() {
    let source = "# Target\n\n> [quote](#missing)\n\n- [list](#target)\n\n| Column |\n| --- |\n| [cell](#absent) |\n";
    let spanned = parse_markdown_spanned(source);
    let report = analyze_document_links(&spanned.to_document()).unwrap();
    assert_eq!(report.references.len(), 3);
    for (reference, expected) in report.references.iter().zip(["quote", "list", "cell"]) {
        let block = &spanned.blocks[reference.block_index];
        assert!(block.span.slice(source).unwrap().contains(expected));
        assert!(reference.block_index > 0);
    }
    assert_eq!(report.references[1].target_block_index, Some(0));
}

#[test]
fn note_queue_preserves_definition_and_reference_owners_through_cycles() {
    let doc = Document { blocks: vec![
        heading("Dup"),
        note("a", vec![heading("Dup"), Block::Paragraph(vec![reference("b")])]),
        Block::Paragraph(vec![reference("b"), reference("a"), link("#dup-4"), link("#fn-a"), link("#fnref-2")]),
        heading("Dup"),
        note("b", vec![heading("Dup"), Block::Paragraph(vec![reference("a")])]),
        note("hidden", vec![heading("Hidden"), Block::Paragraph(vec![link("#missing")])]),
    ] };
    let report = analyze_document_links(&doc).unwrap();
    assert_eq!(report.references.len(), 7);
    assert!(report.references.iter().all(|r| r.finding.is_none()));
    assert_eq!(report.references.iter().map(|r| r.block_index).collect::<Vec<_>>(), [2, 2, 2, 2, 2, 4, 1]);
    assert_eq!(report.references.iter().map(|r| r.target_block_index).collect::<Vec<_>>(), [Some(4), Some(1), Some(1), Some(1), Some(2), Some(1), Some(4)]);
    assert!(!report.anchors.iter().any(|anchor| anchor.id == "hidden"));
}

#[test]
fn colliding_note_ids_are_not_misrepresented_as_resolved_fragment_links() {
    let doc = Document { blocks: vec![heading("fn-a"), Block::Paragraph(vec![reference("a"), reference("missing"), link("#fn-a")]), note("a", vec![])] };
    let report = analyze_document_links(&doc).unwrap();
    assert_eq!(report.references[0].kind, ReferenceKind::Footnote);
    assert_eq!(report.references[0].target_block_index, Some(2));
    assert_eq!(report.references[1].finding.as_ref().unwrap().code, "missing_footnote");
    assert_eq!(report.references[2].finding.as_ref().unwrap().code, "ambiguous_anchor");
    assert_eq!(report.references[2].target_block_index, None);
    assert_eq!(report.anchors.iter().find(|a| a.id == "fn-a").unwrap().occurrences, 2);
}

#[test]
fn first_note_definition_wins_without_mining_unused_examples() {
    let doc = Document { blocks: vec![
        Block::Paragraph(vec![reference("a")]), note("a", vec![heading("Present")]),
        note("a", vec![Block::Paragraph(vec![link("#bad")])]),
        note("unused", vec![Block::Paragraph(vec![link("#missing")])]),
        Block::CodeBlock { lang: None, code: "[code](#missing)".into() },
        Block::Paragraph(vec![Inline::Code("[code](#bad)".into()), Inline::Image { dest: "missing.png".into(), title: None, alt: "[alt](#bad)".into() }]),
    ] };
    let report = analyze_document_links(&doc).unwrap();
    assert_eq!(report.references.len(), 1);
    assert_eq!(report.references[0].target_block_index, Some(1));
    assert!(report.references[0].finding.is_none());
}

#[test]
fn admission_failure_never_returns_partial_clean_results() {
    let mut deep = heading("Deep");
    for _ in 0..130 { deep = Block::BlockQuote(vec![deep]); }
    assert!(analyze_document_links(&Document { blocks: vec![deep] }).is_err());
    let excessive = Document { blocks: vec![Block::Paragraph((0..4097).map(|_| link("#missing")).collect())] };
    assert!(analyze_document_links(&excessive).is_err());
    let large_destination = Document { blocks: vec![Block::Paragraph(vec![link(&format!("#{}", "x".repeat(8192)))])] };
    assert!(analyze_document_links(&large_destination).is_err());
    assert!(analyze_document_links(&Document { blocks: vec![] }).unwrap().references.is_empty());
}
