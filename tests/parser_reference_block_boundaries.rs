//! Invisible reference definitions still terminate the surrounding source blocks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{Block, HtmlOptions, Inline, List, parse_markdown, parse_markdown_spanned};

fn list(block: &Block) -> &List {
    let Block::List(list) = block else {
        panic!("expected list, got {block:?}");
    };
    list
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn boundary_definitions_preserve_distinct_lists_and_ordered_restarts() {
    // CommonMark's reference implementation keeps these as two lists. Merely
    // removing the hidden definition merges them and replaces the restart 7 by 2.
    for separator in [
        "[r]: /target\n",
        "[r]: /target\n[s]: /other\n",
        "[r]: /target\n  \"Title\"\n",
    ] {
        for marker in ["-", "1."] {
            let second = if marker == "1." { "7." } else { "-" };
            let source = format!("{marker} first\n\n{separator}\n{second} [second][r]\n");
            let doc = parse_markdown(&source);
            assert_eq!(doc.blocks.len(), 2, "{source:?}: {doc:?}");
            let first = list(&doc.blocks[0]);
            let last = list(&doc.blocks[1]);
            assert!(first.tight && last.tight, "{source:?}");
            assert_eq!(first.items.len(), 1);
            assert_eq!(last.items.len(), 1);
            assert_eq!(last.start, if marker == "1." { 7 } else { 1 });
            let Block::Paragraph(inlines) = &last.items[0].blocks[0] else {
                panic!("paragraph");
            };
            assert!(matches!(&inlines[..], [Inline::Link { dest, .. }] if dest == "/target"));
            if marker == "1." {
                assert!(fragment(&source).contains("</ol>\n<ol start=\"7\">"));
            } else {
                assert!(fragment(&source).contains("</ul>\n<ul>"));
            }
        }
    }
}

#[test]
fn reference_boundaries_are_preserved_inside_quotes_lists_and_notes() {
    let source = "> - first\n>\n> [r]: /target\n>\n> - second\n";
    let doc = parse_markdown(source);
    let Block::BlockQuote(blocks) = &doc.blocks[0] else {
        panic!("quote");
    };
    assert_eq!(blocks.len(), 2);
    assert_eq!(list(&blocks[0]).items.len(), 1);
    assert_eq!(list(&blocks[1]).items.len(), 1);
    assert!(list(&blocks[0]).tight && list(&blocks[1]).tight);

    let source = "- outer\n  - first\n\n  [r]: /target\n\n  - second\n";
    let doc = parse_markdown(source);
    let blocks = &list(&doc.blocks[0]).items[0].blocks;
    assert_eq!(blocks.len(), 3);
    assert_eq!(list(&blocks[1]).items.len(), 1);
    assert_eq!(list(&blocks[2]).items.len(), 1);

    let source = "[^note]: - first\n\n    [r]: /target\n\n    - second\n\nUse[^note] and [r].\n";
    let doc = parse_markdown(source);
    let Block::FootnoteDefinition { blocks, .. } = &doc.blocks[0] else {
        panic!("footnote");
    };
    assert_eq!(blocks.len(), 2);
    assert_eq!(list(&blocks[0]).items.len(), 1);
    assert_eq!(list(&blocks[1]).items.len(), 1);
    assert!(fragment(source).contains("href=\"/target\""));
}

#[test]
fn hidden_definitions_do_not_concatenate_indented_code_blocks() {
    let source = "    before\n\n[r]: /target\n\n    after\n";
    assert_eq!(
        parse_markdown(source).blocks,
        vec![
            Block::CodeBlock {
                lang: None,
                code: "before\n".into()
            },
            Block::CodeBlock {
                lang: None,
                code: "after\n".into()
            },
        ]
    );
    assert_eq!(fragment(source).matches("<pre>").count(), 2);
}

#[test]
fn definitions_owned_by_an_item_do_not_split_the_containing_list() {
    // Official CommonMark example 317: definition belongs to the middle item.
    let source = "- a\n- b\n\n  [ref]: /url\n- d\n";
    let doc = parse_markdown(source);
    assert_eq!(doc.blocks.len(), 1);
    let outer = list(&doc.blocks[0]);
    assert_eq!(outer.items.len(), 3);
    assert!(!outer.tight);
    assert_eq!(
        fragment(source),
        "<ul>\n<li>\n<p>a</p>\n</li>\n<li>\n<p>b</p>\n</li>\n<li>\n<p>d</p>\n</li>\n</ul>\n"
    );
}

#[test]
fn literal_and_invalid_definitions_keep_their_existing_block_ownership() {
    let source = "- ```\n  [r]: /literal\n  ```\n- [r]\n";
    let doc = parse_markdown(source);
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(list(&doc.blocks[0]).items.len(), 2);
    assert!(destination_spans(source).is_empty());
    assert!(fragment(source).contains("[r]: /literal"));
    let source = "- first\n\n[r]: /target trailing junk\n\n- second\n";
    let doc = parse_markdown(source);
    assert_eq!(doc.blocks.len(), 3);
    assert!(matches!(doc.blocks[1], Block::Paragraph(_)));
    assert!(destination_spans(source).is_empty());
}

#[test]
fn separated_lists_keep_exact_unicode_crlf_spans_and_reference_tokens() {
    let source = "\u{feff}1. café\r\n\r\n[r]: /méta \"Title\"\r\n\r\n7. [β][r]\r\n";
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), parse_markdown(source));
    assert_eq!(spanned.blocks.len(), 2);
    assert_eq!(spanned.blocks[0].span.start, '\u{feff}'.len_utf8());
    assert!(spanned.blocks[0].span.end <= source.find("[r]:").unwrap());
    assert_eq!(spanned.blocks[1].span.slice(source), Some("7. [β][r]"));
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/méta");
    assert_eq!(&source[destinations[0].range.clone()], "/méta");
}
