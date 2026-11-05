//! Blank-separated list blocks retain their paragraph boundaries at every legal indent.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, Document, HtmlOptions, List, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned,
};

fn document(source: &str) -> Document {
    let parsed = parse_markdown(source);
    assert_eq!(
        parse_markdown_profiled(source).document,
        parsed,
        "{source:?}"
    );
    assert_eq!(
        parse_markdown_spanned(source).to_document(),
        parsed,
        "{source:?}"
    );
    parsed
}

fn list(block: &Block) -> &List {
    let Block::List(list) = block else {
        panic!("expected a list, got {block:?}");
    };
    list
}

fn fragment(document: &Document) -> String {
    franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default())
}

#[test]
fn extra_paragraph_indentation_does_not_merge_blank_separated_blocks() {
    // CommonMark 5.3: two direct blocks separated by a blank make a list loose.
    // The paragraph may start up to three columns beyond the content column.
    for (marker, content_indent, open, close) in [
        ("-", 2, "<ul>", "</ul>"),
        ("1.", 3, "<ol>", "</ol>"),
        ("23.", 4, "<ol start=\"23\">", "</ol>"),
    ] {
        for extra_indent in 0..=3 {
            let indent = " ".repeat(content_indent + extra_indent);
            let source = format!("{marker} alpha\n\n{indent}beta\n");
            let parsed = document(&source);
            let parsed_list = list(&parsed.blocks[0]);
            assert!(!parsed_list.tight, "{source:?}: {parsed:?}");
            assert_eq!(parsed_list.items[0].blocks.len(), 2, "{source:?}");
            assert_eq!(
                fragment(&parsed),
                format!("{open}\n<li>\n<p>alpha</p>\n<p>beta</p>\n</li>\n{close}\n"),
                "{source:?}"
            );
        }
    }
}

#[test]
fn tabs_and_indented_code_keep_their_actual_block_kind() {
    for indent in ["\t", " \t", "  \t", "   \t"] {
        let source = format!("- alpha\n\n{indent}beta\n");
        let parsed = document(&source);
        assert!(!list(&parsed.blocks[0]).tight, "{source:?}");
        assert_eq!(
            fragment(&parsed),
            "<ul>\n<li>\n<p>alpha</p>\n<p>beta</p>\n</li>\n</ul>\n",
            "{source:?}"
        );
    }

    for (indent, code) in [("      ", "beta"), ("\t  ", "beta"), ("\t\t", "  beta")] {
        let source = format!("- alpha\n\n{indent}beta\n");
        let parsed = document(&source);
        let parsed_list = list(&parsed.blocks[0]);
        assert!(!parsed_list.tight, "{source:?}");
        assert_eq!(
            parsed_list.items[0].blocks[1],
            Block::CodeBlock {
                lang: None,
                code: format!("{code}\n"),
            },
            "{source:?}"
        );
        assert_eq!(
            fragment(&parsed),
            format!("<ul>\n<li>\n<p>alpha</p>\n<pre><code>{code}\n</code></pre>\n</li>\n</ul>\n"),
            "{source:?}"
        );
    }
}

#[test]
fn fenced_code_after_a_blank_preserves_the_preceding_paragraph() {
    for columns in 2..=5 {
        let indent = " ".repeat(columns);
        for fence in ["```", "~~~"] {
            let source = format!("- alpha\n\n{indent}{fence}\n{indent}code\n{indent}{fence}\n");
            let parsed = document(&source);
            assert!(!list(&parsed.blocks[0]).tight, "{source:?}");
            assert_eq!(
                fragment(&parsed),
                "<ul>\n<li>\n<p>alpha</p>\n<pre><code>code\n</code></pre>\n</li>\n</ul>\n",
                "{source:?}"
            );
        }
    }
}

#[test]
fn one_blank_separated_item_loosens_all_items_in_the_same_list() {
    let source = "- alpha\n\n    beta\n- gamma\n";
    let parsed = document(source);
    assert!(!list(&parsed.blocks[0]).tight);
    assert_eq!(
        fragment(&parsed),
        "<ul>\n<li>\n<p>alpha</p>\n<p>beta</p>\n</li>\n<li>\n<p>gamma</p>\n</li>\n</ul>\n"
    );

    let task = document("- [x] alpha\n\n    beta\n");
    let task_list = list(&task.blocks[0]);
    assert!(!task_list.tight);
    assert_eq!(task_list.items[0].task, Some(true));
    assert_eq!(
        fragment(&task),
        "<ul>\n<li class=\"task\"><input type=\"checkbox\" disabled checked> <p>alpha</p>\n<p>beta</p>\n</li>\n</ul>\n"
    );
}

#[test]
fn nested_paragraph_spacing_loosens_only_the_list_that_owns_it() {
    for columns in 4..=7 {
        let source = format!("- alpha\n  - beta\n\n{}gamma\n", " ".repeat(columns));
        let parsed = document(&source);
        let outer = list(&parsed.blocks[0]);
        assert!(
            outer.tight,
            "nested paragraphs must not loosen their outer list: {source:?}"
        );
        let inner = list(&outer.items[0].blocks[1]);
        assert!(!inner.tight, "{source:?}");
        assert_eq!(
            fragment(&parsed),
            "<ul>\n<li>alpha\n<ul>\n<li>\n<p>beta</p>\n<p>gamma</p>\n</li>\n</ul>\n</li>\n</ul>\n",
            "{source:?}"
        );
    }

    // A blank before the nested list separates two blocks of the outer item.
    let parsed = document("- alpha\n\n    - beta\n");
    let outer = list(&parsed.blocks[0]);
    assert!(!outer.tight);
    assert!(list(&outer.items[0].blocks[1]).tight);
    assert_eq!(
        fragment(&parsed),
        "<ul>\n<li>\n<p>alpha</p>\n<ul>\n<li>beta</li>\n</ul>\n</li>\n</ul>\n"
    );
}

#[test]
fn quote_blocks_and_quoted_lists_keep_their_own_paragraph_boundaries() {
    for columns in 2..=5 {
        let source = format!("- alpha\n\n{}> beta\n", " ".repeat(columns));
        let parsed = document(&source);
        assert!(!list(&parsed.blocks[0]).tight, "{source:?}");
        assert_eq!(
            fragment(&parsed),
            "<ul>\n<li>\n<p>alpha</p>\n<blockquote>\n<p>beta</p>\n</blockquote>\n</li>\n</ul>\n",
            "{source:?}"
        );
    }

    let quoted_list = document("> - alpha\n>\n>     beta\n");
    assert_eq!(
        fragment(&quoted_list),
        "<blockquote>\n<ul>\n<li>\n<p>alpha</p>\n<p>beta</p>\n</li>\n</ul>\n</blockquote>\n"
    );

    // A blank inside the item's only quote is not a gap between direct blocks.
    let nested_quote = document("- > alpha\n  >\n  > beta\n");
    assert!(list(&nested_quote.blocks[0]).tight);
    assert_eq!(
        fragment(&nested_quote),
        "<ul>\n<li>\n<blockquote>\n<p>alpha</p>\n<p>beta</p>\n</blockquote>\n</li>\n</ul>\n"
    );
}

#[test]
fn lists_inside_footnotes_retain_blank_separated_paragraphs() {
    let source = "A note[^note].\n\n[^note]:\n    - alpha\n\n        beta\n";
    let parsed = document(source);
    let Block::FootnoteDefinition { blocks, .. } = &parsed.blocks[1] else {
        panic!("missing footnote definition: {parsed:?}");
    };
    assert!(!list(&blocks[0]).tight);
    assert_eq!(
        fragment(&parsed),
        concat!(
            "<p>A note<sup class=\"footnote-ref\" id=\"fnref-1\"><a href=\"#fn-note\">1</a></sup>.</p>\n",
            "<section class=\"footnotes\">\n<ol>\n<li id=\"fn-note\"><ul>\n<li>\n",
            "<p>alpha</p>\n<p>beta</p>\n</li>\n</ul>\n",
            " <a class=\"footnote-backref\" href=\"#fnref-1\">↩</a>\n</li>\n</ol>\n</section>\n"
        )
    );
}

#[test]
fn unicode_crlf_continuations_keep_exact_destination_and_block_spans() {
    let source = "- café\r\n\r\n\t[réad](/target)\r\n\r\noutside\r\n";
    let parsed = document(source);
    assert!(!list(&parsed.blocks[0]).tight);
    assert_eq!(
        fragment(&parsed),
        "<ul>\n<li>\n<p>café</p>\n<p><a href=\"/target\">réad</a></p>\n</li>\n</ul>\n<p>outside</p>\n"
    );
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/target");
    assert_eq!(&source[spans[0].range.clone()], "/target");
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.blocks.len(), 2);
    assert_eq!(spanned.blocks[1].span.slice(source), Some("outside"));
}

#[test]
fn continuation_lines_trailing_blanks_and_literal_blanks_do_not_loosen_lists() {
    for indent in ["  ", "   ", "    ", "     ", "      ", "\t"] {
        let source = format!("- alpha\n{indent}beta\n\n");
        let parsed = document(&source);
        assert!(list(&parsed.blocks[0]).tight, "{source:?}");
        assert_eq!(
            fragment(&parsed),
            "<ul>\n<li>alpha\nbeta</li>\n</ul>\n",
            "{source:?}"
        );
    }

    let outside = document("- alpha\n\noutside\n");
    assert!(list(&outside.blocks[0]).tight);
    assert_eq!(
        fragment(&outside),
        "<ul>\n<li>alpha</li>\n</ul>\n<p>outside</p>\n"
    );

    let code = document("- ```\n  x\n\n  y\n  ```\n");
    assert!(list(&code.blocks[0]).tight);
    assert_eq!(
        fragment(&code),
        "<ul>\n<li>\n<pre><code>x\n\ny\n</code></pre>\n</li>\n</ul>\n"
    );
}
