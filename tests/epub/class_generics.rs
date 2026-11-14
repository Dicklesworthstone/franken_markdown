use super::*;

#[test]
fn generic_classes_keep_their_types_in_single_and_book_epubs() {
    use franken_markdown::BookInput;
    use franken_markdown::book::BookRenderer;

    let source = concat!(
        "# Types\n\n```mermaid\nclassDiagram\n",
        "class Store~T~ {\n~List~T~ items\n+read() List~List~T~~\n}\n",
        "Store --> Record: holds\n```\n",
    );
    let document = parse_markdown(source);
    let single = epub::render_epub(&document, &HtmlOptions::default()).unwrap();
    let book = BookRenderer::new(&[BookInput {
        path: "types.md".into(),
        source: source.into(),
    }])
    .unwrap()
    .render_epub()
    .unwrap();
    for archive in [single, book] {
        let chapter = entry_text(&archive, "OEBPS/chapter-1.xhtml");
        assert_well_formed_xml(&chapter);
        assert!(chapter.contains("class=\"fmd-diagram fmd-class\""));
        for text in [
            "Store&lt;T&gt;",
            "~List&lt;T&gt; items",
            "+read() List&lt;List&lt;T&gt;&gt;",
        ] {
            assert!(chapter.contains(text), "published type missing: {text}");
        }
        assert!(!chapter.contains("language-mermaid"));
        let manifest = entry_text(&archive, "OEBPS/content.opf");
        assert_well_formed_xml(&manifest);
        assert!(manifest.contains("properties=\"svg\""));
    }
}
