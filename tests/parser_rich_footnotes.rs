//! Rich Markdown footnotes preserve their semantic blocks through public APIs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::flow_display::{DisplayBlock, ResumableFlowDisplay};

#[test]
fn plain_note_continuations_keep_literal_indentation_beyond_the_container() {
    let document = parse_markdown("[^1]: hello\n        extra\n");
    let Block::FootnoteDefinition { blocks, .. } = &document.blocks[0] else {
        panic!("expected footnote definition");
    };
    let Block::Paragraph(inlines) = &blocks[0] else {
        panic!("expected paragraph body");
    };
    let text: String = inlines
        .iter()
        .filter_map(|inline| match inline {
            Inline::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(text.contains("    extra"));
}
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
    render_html,
};

#[test]
fn live_reader_preserves_rich_note_blocks_formatting_and_asset_requests() {
    // The same public-API assertions as the existing live-flow regression that
    // exposed the parser's former single-paragraph note implementation.
    let source = "Use [^rich].\n\n[^rich]: **Bold** é中🙂\n\n    ```rust\n    let x = 1;\n    ```\n\n    | H |\n    | --- |\n    | V |\n\n    - item\n\n    ![caption](note.png)\n";
    let mut engine = ResumableFlowDisplay::new(source, 1);
    engine.process_all().unwrap();
    assert!(engine.blocks().iter().any(|block| matches!(block,
        DisplayBlock::CodeBlock { language, source } if language.as_deref() == Some("rust") && source.contains("let x = 1;"))));
    assert!(
        engine
            .blocks()
            .iter()
            .any(|block| matches!(block, DisplayBlock::TableHeader { .. }))
    );
    assert!(
        engine
            .blocks()
            .iter()
            .any(|block| matches!(block, DisplayBlock::ListItem { .. }))
    );
    assert_eq!(engine.unresolved_assets().len(), 1);
    assert_eq!(engine.unresolved_assets()[0].url, "note.png");
    assert_eq!(engine.unresolved_assets()[0].alt_text, "caption");
    let body = engine
        .blocks()
        .iter()
        .position(|block| {
            matches!(block,
        DisplayBlock::Paragraph { text } if text == "Bold é中🙂")
        })
        .unwrap();
    assert!(
        engine
            .inline_runs_for_block(body)
            .unwrap()
            .iter()
            .any(|run| run.style.bold)
    );
    for index in 0..engine.blocks().len() {
        assert!(
            engine
                .source_span_for_block(index)
                .unwrap()
                .slice(source)
                .is_some()
        );
    }
}

#[test]
fn compact_nested_footnote_markers_terminate_without_losing_the_leaf() {
    let source = format!("{}leaf", "[^a]: ".repeat(1024));
    let mut document = parse_markdown(&source);
    let mut depth = 0;
    let mut found_leaf = false;
    while let Some(block) = document.blocks.pop() {
        match block {
            Block::FootnoteDefinition { blocks, .. } => {
                depth += 1;
                document.blocks = blocks;
            }
            Block::Paragraph(inlines) => {
                found_leaf |= inlines
                    .iter()
                    .any(|inline| matches!(inline, Inline::Text(text) if text.contains("leaf")));
            }
            other => panic!("unexpected note content: {other:?}"),
        }
    }
    assert!(
        depth < 1024,
        "excessive nesting must flatten into literal content"
    );
    assert!(found_leaf);
}

#[test]
fn rich_footnote_bodies_keep_semantic_blocks_and_complete_spans() {
    let definition = "[^rich]: **Bold** é中🙂\n\n    ```rust\n    let x = 1;\n    ```\n\n    | H |\n    | --- |\n    | V |\n\n    - item\n\n    ![caption](note.png)";
    let source = format!("Use [^rich].\n\n{definition}\n\nOutside.");
    let document = crate::parse_markdown(&source);
    assert_eq!(document.blocks.len(), 3);
    let Block::FootnoteDefinition { blocks, .. } = &document.blocks[1] else {
        panic!("expected rich footnote: {document:?}");
    };
    assert_eq!(blocks.len(), 5);
    assert!(
        matches!(&blocks[0], Block::Paragraph(inlines) if matches!(&inlines[0], Inline::Strong(_)))
    );
    assert!(
        matches!(&blocks[1], Block::CodeBlock { lang, code } if lang.as_deref() == Some("rust") && code == "let x = 1;\n")
    );
    assert!(matches!(&blocks[2], Block::Table(table) if table.rows.len() == 1));
    assert!(matches!(&blocks[3], Block::List(list) if list.items.len() == 1));
    assert!(
        matches!(&blocks[4], Block::Paragraph(inlines) if matches!(&inlines[0], Inline::Image { dest, .. } if dest == "note.png"))
    );
    let spanned = crate::parse_markdown_spanned(&source);
    assert_eq!(spanned.blocks.len(), 3);
    assert_eq!(spanned.blocks[1].span.slice(&source), Some(definition));
    assert_eq!(spanned.blocks[2].span.slice(&source), Some("Outside."));
    assert_eq!(crate::parse_markdown_profiled(&source).document, document);

    let html = crate::render_html(&source, &crate::HtmlOptions::default()).unwrap();
    let notes_start = html.find("<section class=\"footnotes\">").unwrap();
    assert!(html[notes_start..].contains("<pre><code class=\"language-rust\">"));
    assert!(html[notes_start..].contains("<table>"));
    assert!(html[notes_start..].contains("<img src=\"note.png\" alt=\"caption\">"));
    assert!(html[..notes_start].contains("<p>Outside.</p>"));
}

#[test]
fn footnote_references_resolve_globally_without_harvesting_literal_code() {
    let source = "[first] [guide] [fake] [indented]\n\n[^note]: [first]: /first\n\n    [guide]: /guide\n\n    ```text\n    [fake]: /fake\n    ```\n\n        [indented]: /indented\n\n    Read [first] and [guide].";
    let document = crate::parse_markdown(source);
    let Block::Paragraph(inlines) = &document.blocks[0] else {
        panic!("body");
    };
    let destinations: Vec<_> = inlines
        .iter()
        .filter_map(|inline| match inline {
            Inline::Link { dest, .. } => Some(dest.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(destinations, ["/first", "/guide"]);
    let Block::FootnoteDefinition { blocks, .. } = &document.blocks[1] else {
        panic!("note");
    };
    assert_eq!(blocks.len(), 3);
    assert!(matches!(&blocks[0], Block::CodeBlock { code, .. } if code == "[fake]: /fake\n"));
    assert!(
        matches!(&blocks[1], Block::CodeBlock { code, .. } if code == "[indented]: /indented\n")
    );
}

#[test]
fn footnote_markers_inside_open_paragraphs_cannot_define_phantom_references() {
    let source = "ordinary paragraph\n[^n]: [ref]: /secret\n\n[ref]";
    let document = crate::parse_markdown(source);
    assert!(
        document
            .blocks
            .iter()
            .all(|block| !matches!(block, Block::FootnoteDefinition { .. }))
    );
    assert_eq!(
        document.blocks[1],
        Block::Paragraph(vec![Inline::Text("[ref]".into())])
    );
}

#[test]
fn rich_footnotes_preserve_fence_lines_and_tab_indented_continuations() {
    let source = "[^code]: ```rust\n\tlet x = 1;\n\t```\n\n\tSecond paragraph.\n\nOutside.";
    let document = crate::parse_markdown(source);
    let Block::FootnoteDefinition { blocks, .. } = &document.blocks[0] else {
        panic!("note");
    };
    assert_eq!(blocks.len(), 2);
    assert!(matches!(&blocks[0], Block::CodeBlock { code, .. } if code == "let x = 1;\n"));
    assert_eq!(
        blocks[1],
        Block::Paragraph(vec![Inline::Text("Second paragraph.".into())])
    );
    let spanned = crate::parse_markdown_spanned(source);
    assert_eq!(spanned.blocks[1].span.slice(source), Some("Outside."));
}

#[test]
fn footnote_body_block_openers_are_not_flattened_into_plain_paragraphs() {
    let definition_list = crate::parse_markdown("[^n]: Term\n    : Meaning");
    let Block::FootnoteDefinition { blocks, .. } = &definition_list.blocks[0] else {
        panic!("note");
    };
    assert!(matches!(&blocks[0], Block::DefinitionList(items) if items.len() == 1));
    let ordered_list = crate::parse_markdown("[^n]: 2. item\n    3. next");
    let Block::FootnoteDefinition { blocks, .. } = &ordered_list.blocks[0] else {
        panic!("note");
    };
    assert!(
        matches!(&blocks[0], Block::List(list) if list.ordered && list.start == 2 && list.items.len() == 2)
    );
}
