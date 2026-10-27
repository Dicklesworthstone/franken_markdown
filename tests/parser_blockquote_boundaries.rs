//! Blockquote ownership follows the current block, not its last physical line.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.to_owned())])
}

fn quote(body: &str) -> String {
    body.lines().map(|line| format!("> {line}\n")).collect()
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn unquoted_prose_after_fenced_code_remains_a_separate_paragraph() {
    for fence in ["```", "~~~"] {
        for close in ["", &format!("> {fence}\n")] {
            let source = format!("> {fence}\n> literal\n{close}outside\n");
            assert_eq!(
                parse_markdown(&source).blocks,
                vec![
                    Block::BlockQuote(vec![Block::CodeBlock {
                        lang: None,
                        code: "literal\n".into(),
                    }]),
                    paragraph("outside"),
                ],
                "{source:?}"
            );
            assert_eq!(
                fragment(&source),
                "<blockquote>\n<pre><code>literal\n</code></pre>\n</blockquote>\n<p>outside</p>\n",
                "{source:?}"
            );
        }
    }
}

#[test]
fn nonparagraph_quote_blocks_cannot_capture_outside_text() {
    for body in [
        "# Heading",
        "Title\n===",
        "    indented code",
        "<!-- comment\nbody\n-->",
        "<!-- comment\nbody",
        "<pre>\nbody\n</pre>",
        "<pre>\nbody",
        "<div>\nbody",
        "$$\nx + y\n$$",
        "$$\nx + y",
        "| A | B |\n| --- | --- |\n| a | b |",
        "[^note]: Note body",
    ] {
        let source = format!("{}outside\n", quote(body));
        let document = parse_markdown(&source);
        assert_eq!(
            document.blocks,
            vec![
                Block::BlockQuote(parse_markdown(body).blocks),
                paragraph("outside"),
            ],
            "{source:?}"
        );
        assert_eq!(parse_markdown_profiled(&source).document, document);
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
    }
}

#[test]
fn nested_quotes_and_lists_keep_their_actual_leaf_boundary() {
    for body in [
        "> ```\n> literal",
        "> <!-- comment\n> body",
        "> Title\n> ===",
        "- ```\n  literal",
        "1. ~~~\n   literal",
        "- # Heading",
        "- [ ] # Heading",
        "- [x] ```\n  literal",
        "- Title\n  ===",
        "- > <pre>\n  > literal",
        "> - ```\n>   literal",
    ] {
        let source = format!("{}outside\n", quote(body));
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{source:?}: {document:?}");
        assert_eq!(document.blocks[1], paragraph("outside"), "{source:?}");
        assert_eq!(
            document.blocks[0],
            Block::BlockQuote(parse_markdown(body).blocks),
            "{source:?}"
        );
    }
}

#[test]
fn ending_an_inner_quote_keeps_explicitly_marked_outer_content() {
    let source = "> > ```\n> > literal\n> outside inner\noutside outer\n";
    assert_eq!(
        parse_markdown(source).blocks,
        vec![Block::BlockQuote(vec![
            Block::BlockQuote(vec![Block::CodeBlock {
                lang: None,
                code: "literal\n".into(),
            }]),
            Block::Paragraph(vec![
                Inline::Text("outside inner".into()),
                Inline::SoftBreak,
                Inline::Text("outside outer".into()),
            ]),
        ])]
    );
}

#[test]
fn genuine_lazy_paragraphs_keep_hanging_and_nested_continuations() {
    for source in [
        "> first\ncontinued\n",
        "> first\n>     hanging\ncontinued\n",
        "> first\n    hanging\ncontinued\n",
        "> - first\ncontinued\n",
        "> 1. first\ncontinued\n",
        "> > first\ncontinued\n",
        "> - > first\ncontinued\n",
        "> > - first\ncontinued\n",
        "> ```\n> code\n> ```\n> first\ncontinued\n",
        "> <!-- comment\n> -->\n> first\ncontinued\n",
        "> Title\n> ===\n> first\ncontinued\n",
    ] {
        let document = parse_markdown(source);
        assert_eq!(document.blocks.len(), 1, "{source:?}: {document:?}");
        let rendered = fragment(source);
        assert!(
            rendered.find("continued").unwrap() < rendered.rfind("</blockquote>").unwrap(),
            "{source:?}: {rendered}"
        );
    }
}

#[test]
fn lazy_markers_stay_paragraph_text_after_container_stripping() {
    for (source, expected) in [
        (
            "> first\n=\ncontinued\n",
            "<blockquote>\n<p>first\n=\ncontinued</p>\n</blockquote>\n",
        ),
        (
            "> first\n    - hanging\ncontinued\n",
            "<blockquote>\n<p>first\n- hanging\ncontinued</p>\n</blockquote>\n",
        ),
    ] {
        assert_eq!(fragment(source), expected, "{source:?}");
    }
}

#[test]
fn unquoted_fences_start_new_blocks_instead_of_closing_quoted_code() {
    let source = "> ```\n> literal\n```\noutside\n";
    assert_eq!(
        parse_markdown(source).blocks,
        vec![
            Block::BlockQuote(vec![Block::CodeBlock {
                lang: None,
                code: "literal\n".into(),
            }]),
            Block::CodeBlock {
                lang: None,
                code: "outside\n".into(),
            },
        ]
    );
}

#[test]
fn quote_fences_do_not_steal_real_reference_definitions() {
    for body in [
        "```\n[fake]: /literal",
        "~~~\n[fake]: /literal",
        "<pre>\n[fake]: /literal",
    ] {
        let source = format!("{}[real]: /target\n\n[fake] [real]\n", quote(body));
        let rendered = fragment(&source);
        assert!(
            rendered.contains("[fake]: /literal"),
            "{source:?}: {rendered}"
        );
        assert!(
            rendered.contains("</blockquote>\n<p>[fake] <a href=\"/target\">real</a></p>"),
            "{source:?}: {rendered}"
        );
        assert!(!rendered.contains("href=\"/literal\""), "{source:?}");
        let destinations = franken_markdown::parse::destination_spans(&source);
        assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
        assert_eq!(destinations[0].dest, "/target");
        assert_eq!(&source[destinations[0].range.clone()], "/target");
    }
}

#[test]
fn ordinary_lazy_reference_shaped_lines_still_remain_content() {
    let source = "> first\n[not-a-definition]: /literal\n\n[not-a-definition]\n";
    let rendered = fragment(source);
    assert!(rendered.contains("first\n[not-a-definition]: /literal</p>\n</blockquote>"));
    assert!(rendered.ends_with("<p>[not-a-definition]</p>\n"));
    assert!(franken_markdown::parse::destination_spans(source).is_empty());
}

#[test]
fn source_spans_and_destinations_follow_unicode_crlf_quote_boundaries() {
    let source = "> ```\r\n> café [literal](/no)\r\n[real](/réal#heading)\r\n";
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.blocks.len(), 2, "{spanned:?}");
    assert_eq!(
        spanned.blocks[0].span.slice(source),
        Some("> ```\r\n> café [literal](/no)")
    );
    assert_eq!(
        spanned.blocks[1].span.slice(source),
        Some("[real](/réal#heading)")
    );
    assert_eq!(spanned.to_document(), parse_markdown(source));
    let destinations = franken_markdown::parse::destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/réal#heading");
    assert_eq!(&source[destinations[0].range.clone()], "/réal#heading");
}

#[test]
fn tab_expansion_and_lazy_marker_copies_keep_exact_destination_ranges() {
    let source = ">\tfirst\n    - [link](/réal)\ncontinued\n";
    let destinations = franken_markdown::parse::destination_spans(source);
    assert_eq!(destinations.len(), 1, "{destinations:?}");
    assert_eq!(destinations[0].dest, "/réal");
    assert_eq!(&source[destinations[0].range.clone()], "/réal");
    assert_eq!(
        parse_markdown_spanned(source).to_document(),
        parse_markdown(source)
    );
    assert!(fragment(source).contains("- <a href=\"/r%C3%A9al\">link</a>\ncontinued"));
}

#[test]
fn repeated_quote_boundaries_keep_alternating_code_and_prose() {
    let source = "> ```\n> literal\noutside\n".repeat(1024);
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 2048);
    for pair in document.blocks.chunks_exact(2) {
        assert_eq!(
            pair[0],
            Block::BlockQuote(vec![Block::CodeBlock {
                lang: None,
                code: "literal\n".into(),
            }]),
        );
        assert_eq!(pair[1], paragraph("outside"));
    }
}
