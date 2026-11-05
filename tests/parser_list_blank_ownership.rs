//! Blank separators belong to live blocks, not list-shaped text in earlier literals.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, Document, HtmlOptions, List, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned,
};

fn list(block: &Block) -> &List {
    let Block::List(list) = block else {
        panic!("expected a list, got {block:?}");
    };
    list
}

fn assert_html(source: &str, expected: &str, tight: bool) -> Document {
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
    assert_eq!(list(&parsed.blocks[0]).tight, tight, "{source:?}");
    let options = HtmlOptions {
        allow_raw_html: true,
        ..HtmlOptions::default()
    };
    assert_eq!(
        franken_markdown::html::render_fragment(&parsed.blocks, &options),
        expected,
        "{source:?}"
    );
    parsed
}

#[test]
fn list_markers_in_fences_do_not_claim_a_later_paragraph_gap() {
    // CommonMark 5.3 / example 324: a blank after the closed fence separates
    // direct blocks of the item, regardless of the literal code's contents.
    for fence in ["```", "~~~"] {
        for literal in ["- literal", "1. literal"] {
            for columns in 3..=5 {
                let source = format!(
                    "- {fence}\n  {literal}\n  {fence}\n\n{}beta\n",
                    " ".repeat(columns)
                );
                let expected = format!(
                    "<ul>\n<li>\n<pre><code>{literal}\n</code></pre>\n<p>beta</p>\n</li>\n</ul>\n"
                );
                assert_html(&source, &expected, false);
            }
        }
    }
}

#[test]
fn list_markers_in_html_do_not_claim_a_later_paragraph_gap() {
    for (open, close) in [("<pre>", "</pre>"), ("<div>", "</div>"), ("<!--", "-->")] {
        let source = format!("- {open}\n  - literal\n  {close}\n\n    beta\n");
        let expected =
            format!("<ul>\n<li>\n{open}\n- literal\n{close}\n<p>beta</p>\n</li>\n</ul>\n");
        assert_html(&source, &expected, false);
    }
}

#[test]
fn indented_code_markers_stay_literal_after_the_code_block_closes() {
    let parsed = assert_html(
        "-     - literal\n  # heading\n\n    beta\n",
        concat!(
            "<ul>\n<li>\n<pre><code>- literal\n</code></pre>\n",
            "<h1 id=\"heading\">heading</h1>\n<p>beta</p>\n</li>\n</ul>\n"
        ),
        false,
    );
    assert_eq!(
        list(&parsed.blocks[0]).items[0].blocks[0],
        Block::CodeBlock {
            lang: None,
            code: "- literal\n".to_string(),
        }
    );
}

#[test]
fn literal_blanks_and_adjacent_blocks_keep_the_list_tight() {
    assert_html(
        "- ```\n  - literal\n\n  still code\n  ```\n- tail\n",
        concat!(
            "<ul>\n<li>\n<pre><code>- literal\n\nstill code\n</code></pre>\n",
            "</li>\n<li>tail</li>\n</ul>\n"
        ),
        true,
    );
    assert_html(
        "- ```\n  - literal\n  ```\n    beta\n",
        "<ul>\n<li>\n<pre><code>- literal\n</code></pre>\nbeta</li>\n</ul>\n",
        true,
    );
}

#[test]
fn live_nested_lists_keep_ownership_of_their_own_gaps() {
    for columns in 4..=7 {
        let source = format!("- alpha\n  - beta\n\n{}gamma\n", " ".repeat(columns));
        let parsed = assert_html(
            &source,
            concat!(
                "<ul>\n<li>alpha\n<ul>\n<li>\n<p>beta</p>\n<p>gamma</p>\n",
                "</li>\n</ul>\n</li>\n</ul>\n"
            ),
            true,
        );
        assert!(!list(&list(&parsed.blocks[0]).items[0].blocks[1]).tight);
    }

    // The outer item's direct child is a list; the quote is inside that list.
    let parsed = assert_html(
        "- - > alpha\n\n      beta\n",
        concat!(
            "<ul>\n<li>\n<ul>\n<li>\n<blockquote>\n<p>alpha</p>\n</blockquote>\n",
            "<p>beta</p>\n</li>\n</ul>\n</li>\n</ul>\n"
        ),
        true,
    );
    assert!(!list(&list(&parsed.blocks[0]).items[0].blocks[0]).tight);
}

#[test]
fn a_list_inside_a_quote_cannot_claim_a_gap_outside_that_quote() {
    assert_html(
        "- > - alpha\n\n    beta\n",
        concat!(
            "<ul>\n<li>\n<blockquote>\n<ul>\n<li>alpha</li>\n</ul>\n</blockquote>\n",
            "<p>beta</p>\n</li>\n</ul>\n"
        ),
        false,
    );

    // An explicit quote marker keeps the gap inside the quote and inner list.
    assert_html(
        "- > - alpha\n  >\n  >     beta\n",
        concat!(
            "<ul>\n<li>\n<blockquote>\n<ul>\n<li>\n<p>alpha</p>\n<p>beta</p>\n",
            "</li>\n</ul>\n</blockquote>\n</li>\n</ul>\n"
        ),
        true,
    );
}

#[test]
fn closed_nested_lists_do_not_claim_later_direct_block_gaps() {
    for (block, html) in [
        ("  # heading\n", "<h1 id=\"heading\">heading</h1>\n"),
        ("  ```\n  code\n  ```\n", "<pre><code>code\n</code></pre>\n"),
    ] {
        let source = format!("- alpha\n  - beta\n{block}\n    gamma\n");
        let expected = format!(
            "<ul>\n<li>\n<p>alpha</p>\n<ul>\n<li>beta</li>\n</ul>\n{html}<p>gamma</p>\n</li>\n</ul>\n"
        );
        let parsed = assert_html(&source, &expected, false);
        assert!(list(&list(&parsed.blocks[0]).items[0].blocks[1]).tight);
    }
}
