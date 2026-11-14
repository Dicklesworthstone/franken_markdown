//! Structural tab stops must not rewrite literal Markdown content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled,
};

fn document(source: &str) -> Document {
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document, "{source:?}");
    assert_eq!(
        parse_markdown_spanned(source).to_document(),
        document,
        "{source:?}"
    );
    assert_eq!(
        parse_markdown_spanned_profiled(source)
            .document
            .to_document(),
        document,
        "{source:?}"
    );
    document
}

fn collect_code<'a>(blocks: &'a [Block], output: &mut Vec<&'a str>) {
    for block in blocks {
        match block {
            Block::CodeBlock { code, .. } => output.push(code),
            Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => {
                collect_code(blocks, output);
            }
            Block::List(list) => {
                for item in &list.items {
                    collect_code(&item.blocks, output);
                }
            }
            _ => {}
        }
    }
}

fn code_blocks(document: &Document) -> Vec<&str> {
    let mut output = Vec::new();
    collect_code(&document.blocks, &mut output);
    output
}

#[test]
fn fenced_makefile_recipes_keep_tabs_in_the_ast_and_copyable_html() {
    for prefix in ["\t", "\t\t", " \t", "\t  \t"] {
        let body = format!("all:\n{prefix}cc main.c -o app\n");
        let source = format!("```\n{body}```\n");
        let parsed = document(&source);
        assert_eq!(code_blocks(&parsed), [body.as_str()]);
        let html =
            franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
        assert_eq!(html, format!("<pre><code>{body}</code></pre>\n"));
    }
}

#[test]
fn only_structurally_consumed_tab_columns_are_removed() {
    for (source, expected) in [
        ("  ```\n\tcommand\n  ```", "  command\n"),
        ("  ```\n\t\tcommand\n  ```", "  \tcommand\n"),
        ("   ```\n\t \tcommand\n   ```", "  \tcommand\n"),
        (" ```\n \tcommand\n ```", "\tcommand\n"),
        ("    \treturn x\n", "\treturn x\n"),
        ("\t\treturn x\n", "\treturn x\n"),
        (" \t\treturn x\n", "\treturn x\n"),
        ("   \t \tvalue\n", " \tvalue\n"),
    ] {
        assert_eq!(code_blocks(&document(source)), [expected], "{source:?}");
    }
}

#[test]
fn literal_tabs_survive_list_quote_and_footnote_containers() {
    for (source, expected) in [
        ("- ```\n  all:\n  \tcc main.c\n  ```", "all:\n\tcc main.c\n"),
        ("- ```\n\tcommand\n  ```", "  command\n"),
        ("> ```\n> \tcommand\n> ```", "\tcommand\n"),
        (">\t```\n>\t\tcommand\n>\t```", "\tcommand\n"),
        ("- > ```\n  > \tcommand\n  > ```", "\tcommand\n"),
        (
            "Use [^n].\n\n[^n]: ```\n    \tcc main.c\n    ```",
            "\tcc main.c\n",
        ),
        ("-\t\t\tcommand", "  \tcommand\n"),
    ] {
        assert_eq!(code_blocks(&document(source)), [expected], "{source:?}");
    }
}

#[test]
fn tab_only_literal_lines_and_line_endings_preserve_authored_bytes() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = "```\n\t\n \t \n\tend\n```\n".replace('\n', newline);
        assert_eq!(
            code_blocks(&document(&source)),
            ["\t\n \t \n\tend\n"],
            "{newline:?}"
        );
        let source = "- ```\n  \t\n  \tend\n  ```\n".replace('\n', newline);
        assert_eq!(code_blocks(&document(&source)), ["\t\n\tend\n"]);

        // An internal blank belongs to indented code when more code follows.
        // Preserve its tab beyond the four structural columns, while trailing
        // blank lines belong outside the code block.
        let source = "    first\n    \t\n    \tlast\n    \t\n\noutside\n".replace('\n', newline);
        let parsed = document(&source);
        assert_eq!(code_blocks(&parsed), ["first\n\t\n\tlast\n"]);
        assert!(matches!(
            parsed.blocks.last(),
            Some(Block::Paragraph(inlines))
                if inlines == &[Inline::Text("outside".into())]
        ));
    }
}

#[test]
fn multiline_inline_literals_and_unicode_destination_spans_stay_aligned() {
    let original = "before `left\n\tmiddle\n  \tright` [visible](/réel#heading)\n";
    for newline in ["\n", "\r\n", "\r"] {
        let source = original.replace('\n', newline);
        let parsed = document(&source);
        let Block::Paragraph(inlines) = &parsed.blocks[0] else {
            panic!("expected paragraph: {parsed:?}");
        };
        assert!(inlines.iter().any(
            |inline| matches!(inline, Inline::Code(text) if text == "left \tmiddle   \tright")
        ));
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{spans:?}");
        assert_eq!(spans[0].dest, "/réel#heading");
        assert_eq!(&source[spans[0].range.clone()], "/réel#heading");
        assert_eq!(
            parse_markdown_spanned(&source).blocks[0].span.slice(&source),
            Some(source.trim_end_matches(['\r', '\n']))
        );
    }
}

#[test]
fn nested_inline_code_keeps_tabs_without_promoting_its_hidden_link() {
    for source in [
        "> `α\n> \t[hidden](/wrong)\n> ω` [visible](/réel#heading)",
        "- `α\n  \t[hidden](/wrong)\n  ω` [visible](/réel#heading)",
        "Use [^n].\n\n[^n]: `α\n    \t[hidden](/wrong)\n    ω` [visible](/réel#heading)",
    ] {
        let parsed = document(source);
        let html =
            franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
        assert!(html.contains("\t[hidden](/wrong)"), "{html}");
        let spans = destination_spans(source);
        assert_eq!(spans.len(), 1, "{source:?}: {spans:?}");
        assert_eq!(&source[spans[0].range.clone()], "/réel#heading");
    }
}

#[test]
fn raw_html_and_inline_attributes_retain_literal_tabs() {
    let source = "<pre>\n\tliteral\n \tmore\n</pre>\n";
    let parsed = document(source);
    assert_eq!(
        parsed.blocks,
        [Block::HtmlBlock("<pre>\n\tliteral\n \tmore\n</pre>".into())]
    );
    let html = franken_markdown::html::render_fragment(
        &parsed.blocks,
        &HtmlOptions {
            allow_raw_html: true,
            ..HtmlOptions::default()
        },
    );
    assert_eq!(html, source);

    let source = "<span title=\"first\n\tsecond\">value</span> [go](/réel)";
    let parsed = document(source);
    let Block::Paragraph(inlines) = &parsed.blocks[0] else {
        panic!("expected inline HTML: {parsed:?}");
    };
    assert!(inlines.iter().any(
        |inline| matches!(inline, Inline::Html(text) if text == "<span title=\"first\n\tsecond\">")
    ));
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(&source[spans[0].range.clone()], "/réel");
}

#[test]
fn equal_normalized_lines_keep_distinct_literal_provenance_across_scopes() {
    let source = "```\n\tα\n```\n\n```\n    α\n```\n\n\
                  - ```\n  \tβ\n  ```\n\n\
                  > ```\n>   β\n> ```\n\n\
                  ```\n    α\n```\n";
    // Repeat the whole parse so allocator reuse cannot carry tab ownership
    // from a previous document or a finished nested container.
    for _ in 0..16 {
        let parsed = document(source);
        assert_eq!(
            code_blocks(&parsed),
            ["\tα\n", "    α\n", "\tβ\n", "  β\n", "    α\n"]
        );
    }
}
