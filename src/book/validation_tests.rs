    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::book::{BookInput, build_book};

    fn file(path: &str, source: &str) -> BookInput { BookInput { path: path.into(), source: source.into() } }
    fn link(dest: &str) -> Inline { Inline::Link { dest: dest.into(), title: None, content: vec![Inline::Text("go".into())] } }
    fn note(id: &str, blocks: Vec<Block>) -> Block { Block::FootnoteDefinition { id: id.into(), blocks } }
    fn heading(text: &str) -> Block { Block::Heading { level: 2, inlines: vec![Inline::Text(text.into())] } }
    fn one(blocks: Vec<Block>) -> Book {
        let mut book = build_book(&[file("a.md", "")]).unwrap();
        book.chapters[0].doc = Document { blocks }; book
    }
    fn codes(report: &LinkReport) -> Vec<&str> {
        report.chapters.iter().flat_map(|chapter| chapter.findings.iter().map(|finding| finding.code)).collect()
    }

    #[test]
    fn forward_nested_links_queries_and_duplicate_heading_ids_match_publication() {
        let book = build_book(&[
            file("guide/start.md", "# Start\n\n[forward](../end.md?mode=read#same-2) [self](#start) [query](?mode=read#start) [root](/end.md#same)\n\n[web](https://example.test/) [download](manual.zip)"),
            file("end.md", "# Same\n\n# Same\n"),
        ]).unwrap();
        let report = check_book_links(&book).unwrap();
        assert_eq!(report.finding_count(), 0);
        assert_eq!((report.chapters[0].checked, report.chapters[0].external, report.chapters[0].unchecked), (4, 1, 1));
        assert_eq!(report, check_book_links(&book).unwrap());
    }

    #[test]
    fn missing_chapters_anchors_and_invalid_fragments_are_distinct() {
        let book = one(vec![heading("Here"), Block::Paragraph(vec![
            link("gone.md#x"), link("#not-here"), link("#%zz"), link("../escape.md"), link("bad%zz.md"), link("#%00"),
        ])]);
        let report = check_book_links(&book).unwrap();
        assert_eq!(codes(&report), ["missing_chapter", "missing_anchor", "invalid_fragment", "invalid_local_destination", "invalid_local_destination", "invalid_fragment"]);
        assert_eq!(report.chapters[0].checked, 6);
        assert_eq!(report.chapters[0].findings[0].destination, "gone.md#x");
    }

    #[test]
    fn percent_decoding_is_once_and_output_page_links_are_recognized() {
        let book = build_book(&[
            file("guide/start.md", "[space](../a%20b.md#%68ello) [percent](../a%2520b.md#hello) [output](./a~20b.html#hello)"),
            file("a b.md", "# Hello"), file("a%20b.md", "# Hello"),
        ]).unwrap();
        let report = check_book_links(&book).unwrap();
        assert_eq!(report.finding_count(), 0); assert_eq!(report.chapters[0].checked, 3);
        assert_eq!(decode("a+b"), Some("a+b".into()));
        assert_eq!(decode("%E4%B8%AD"), Some("中".into()));
        assert_eq!(decode("%ff"), None);
    }

    #[test]
    fn referenced_notes_follow_emission_order_and_cycles_terminate() {
        let book = one(vec![
            heading("Dup"),
            note("a", vec![heading("Dup"), Block::Paragraph(vec![Inline::FootnoteRef { id: "b".into() }])]),
            Block::Paragraph(vec![Inline::FootnoteRef { id: "b".into() }, Inline::FootnoteRef { id: "a".into() }, link("#dup-4"), link("#fn-a"), link("#fnref-2")]),
            heading("Dup"),
            note("b", vec![heading("Dup"), Block::Paragraph(vec![Inline::FootnoteRef { id: "a".into() }])]),
            note("hidden", vec![heading("Hidden"), Block::Paragraph(vec![link("gone.md")])]),
        ]);
        let report = check_book_links(&book).unwrap();
        assert_eq!(report.finding_count(), 0); assert_eq!(report.chapters[0].checked, 7);
        let nav = navigation(&book.chapters[0].doc);
        assert_eq!(nav.order, ["b", "a"]); assert!(!nav.anchors.contains_key("hidden"));
        let html = crate::html::render_fragment(&book.chapters[0].doc.blocks, &crate::HtmlOptions::default());
        for anchor in nav.anchors.keys() { assert!(html.contains(&format!("id=\"{anchor}\"")), "missing {anchor}"); }
    }

    #[test]
    fn undefined_notes_and_colliding_note_heading_ids_are_reported() {
        let book = one(vec![heading("fn-a"), Block::Paragraph(vec![
            Inline::FootnoteRef { id: "a".into() }, Inline::FootnoteRef { id: "missing".into() }, link("#fn-a"),
        ]), note("a", vec![])]);
        assert_eq!(codes(&check_book_links(&book).unwrap()), ["missing_footnote", "ambiguous_anchor"]);
    }

    #[test]
    fn first_note_definition_wins_and_unreferenced_content_is_not_checked() {
        let book = one(vec![Block::Paragraph(vec![Inline::FootnoteRef { id: "a".into() }]),
            note("a", vec![heading("Present")]), note("a", vec![Block::Paragraph(vec![link("bad.md")])]),
            note("unused", vec![Block::Paragraph(vec![link("missing.md")])]),
            Block::CodeBlock { lang: None, code: "[code](missing.md)".into() },
            Block::Paragraph(vec![Inline::Image { dest: "missing.png".into(), title: None, alt: "[alt](bad.md)".into() }]),
        ]);
        let report = check_book_links(&book).unwrap();
        assert_eq!(report.finding_count(), 0); assert_eq!(report.chapters[0].checked, 1);
    }

    #[test]
    fn nested_container_links_are_checked_without_treating_code_as_links() {
        let book = build_book(&[file("a.md", "# Target\n\n> [quote](#target)\n\n- **[list](#missing)**\n\n| column |\n| --- |\n| [cell](absent.md) |\n\n`[code](absent.md)`")]).unwrap();
        let report = check_book_links(&book).unwrap();
        assert_eq!(codes(&report), ["missing_anchor", "missing_chapter"]);
        assert_eq!(report.chapters[0].checked, 3);
    }

    #[test]
    fn resource_expansion_is_checked_in_the_actual_publishing_chapter() {
        let renderer = BookRenderer::from_sources(&[file("book.md", "# Book\n\n{{#include parts.md}}\n")],
            &[file("parts.md", "[broken](#absent)\n")]).unwrap();
        let report = renderer.validate_links().unwrap();
        assert_eq!(report.chapters.len(), 1); assert_eq!(report.chapters[0].path, "book.md");
        assert_eq!(codes(&report), ["missing_anchor"]);
        assert!(BookRenderer::from_sources(&[file("book.md", "{{#include missing.md}}")], &[]).is_err());
    }

    #[test]
    fn explicit_skip_counts_do_not_claim_remote_or_download_validation() {
        let book = one(vec![Block::Paragraph(vec![link("https://example.test/nope"), link("//example.test/x"),
            link("mailto:a@example.test"), link("asset.zip"), link("other.html"), link(""), link("#")])]);
        let report = check_book_links(&book).unwrap();
        assert_eq!((report.chapters[0].checked, report.chapters[0].external, report.chapters[0].unchecked), (2, 3, 2));
    }

    #[test]
    fn malformed_manual_books_fail_instead_of_issuing_a_clean_report() {
        assert!(check_book_links(&Book { chapters: vec![] }).is_err());
        let mut book = one(vec![]); book.chapters[0].out_name = "javascript:bad".into();
        assert!(check_book_links(&book).is_err());
        let mut book = one(vec![]); book.chapters.push(book.chapters[0].clone());
        assert!(check_book_links(&book).is_err());
    }

    #[test]
    fn deep_or_oversized_reports_fail_without_partial_success() {
        let mut block = heading("deep");
        for _ in 0..130 { block = Block::BlockQuote(vec![block]); }
        assert!(check_book_links(&one(vec![block])).is_err());
        let references = (0..MAX_FINDINGS + 1).map(|_| link("missing.md")).collect();
        assert!(check_book_links(&one(vec![Block::Paragraph(references)])).is_err());
        let mut out = Json(String::new());
        assert!(out.raw(&"x".repeat(MAX_REPORT_BYTES + 1)).is_err()); assert!(out.0.is_empty());
    }

    #[test]
    fn serialization_is_total_deterministic_and_has_honest_totals() {
        let report = LinkReport { chapters: vec![ChapterLinks { path: "a.md".into(), checked: 1, external: 2, unchecked: 3,
            findings: vec![LinkFinding { code: "missing_anchor", destination: "#\"\\\0中".into(), message: "Missing." }] }] };
        let json = report.to_json().unwrap();
        assert!(json.contains("#\\\"\\\\\\u0000中"));
        assert!(json.ends_with("\"summary\":{\"chapters\":1,\"checked\":1,\"external\":2,\"unchecked\":3,\"findings\":1}}"));
        assert_eq!(json, report.to_json().unwrap());
        let mut huge = report.clone(); huge.chapters[0].checked = usize::MAX; huge.chapters.push(huge.chapters[0].clone());
        assert!(huge.to_json().is_err());
    }
