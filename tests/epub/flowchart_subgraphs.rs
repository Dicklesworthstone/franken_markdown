use super::*;

#[test]
fn grouped_architecture_keeps_titles_and_native_figures_in_single_and_book_epubs() {
    use franken_markdown::BookInput;
    use franken_markdown::book::BookRenderer;

    let source = concat!(
        "# Architecture\n\n```mermaid\nflowchart TD\n",
        "subgraph Client[Client apps]\nUI[Browser]\nend\n",
        "subgraph Backend[Application]\nAPI[API]\n",
        "subgraph Data[\"Data & storage\"]\nCache[Cache]\nend\n",
        "API --> Cache\nend\nUI -->|request| API\n```\n",
    );
    let document = parse_markdown(source);
    let single = epub::render_epub(&document, &HtmlOptions::default()).unwrap();
    let book = BookRenderer::new(&[BookInput {
        path: "architecture.md".into(),
        source: source.into(),
    }])
    .unwrap()
    .render_epub()
    .unwrap();
    for archive in [single, book] {
        let chapter = entry_text(&archive, "OEBPS/chapter-1.xhtml");
        assert_well_formed_xml(&chapter);
        assert!(chapter.contains("class=\"fmd-diagram fmd-flowchart\""));
        assert_eq!(chapter.matches("class=\"fmd-subgraph-box\"").count(), 3);
        for label in [
            "Client apps",
            "Application",
            "Data &amp; storage",
            "Browser",
            "API",
            "Cache",
            "request",
        ] {
            assert!(
                chapter.contains(&format!(">{label}</text>")),
                "missing group content {label}"
            );
        }
        assert!(!chapter.contains("language-mermaid"));
        let manifest = entry_text(&archive, "OEBPS/content.opf");
        assert_well_formed_xml(&manifest);
        assert!(manifest.contains("properties=\"svg\""));
    }
}
