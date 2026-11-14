//! List markers and blank lines must belong to the container they actually fit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, reference_label_spans};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, List, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned,
};

fn document(source: &str) -> Document {
    let parsed = parse_markdown(source);
    assert_eq!(
        parse_markdown_profiled(source).document,
        parsed,
        "{source:?}"
    );
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), parsed, "{source:?}");
    assert!(spanned.diagnostics.is_empty(), "{source:?}");
    parsed
}

fn list(block: &Block) -> &List {
    let Block::List(list) = block else {
        panic!("expected list, got {block:?}");
    };
    list
}

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.into())])
}

fn fragment(document: &Document) -> String {
    franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default())
}

#[test]
fn insufficiently_indented_numbered_markers_are_siblings() {
    // CommonMark 5.3 / examples 310-313: sibling items need not align; the
    // previous item's required content indentation determines membership.
    for (marker, start, content_indent) in [("1.", 1, 3), ("23)", 23, 4)] {
        for indent in 0..content_indent {
            let delimiter = marker.chars().last().unwrap();
            let source = format!(
                "{marker} first\n{}42{delimiter} second\n",
                " ".repeat(indent)
            );
            let parsed = document(&source);
            assert_eq!(parsed.blocks.len(), 1, "{source:?}");
            let numbered = list(&parsed.blocks[0]);
            assert!(numbered.ordered && numbered.tight, "{source:?}");
            assert_eq!(numbered.start, start, "{source:?}");
            assert_eq!(numbered.items.len(), 2, "{source:?}");
            assert_eq!(numbered.items[0].blocks, [paragraph("first")]);
            assert_eq!(numbered.items[1].blocks, [paragraph("second")]);
        }
    }
}

#[test]
fn sibling_marker_types_and_empty_items_start_their_own_lists() {
    // Verified against upstream cmark as well as the specification's sibling
    // and marker-type rules. None of these lines interrupts an item paragraph
    // at its content indentation, so neither start-at-1 nor nonempty applies.
    for (source, expected) in [
        (
            "- first\n2. second\n",
            "<ul>\n<li>first</li>\n</ul>\n<ol start=\"2\">\n<li>second</li>\n</ol>\n",
        ),
        (
            "- first\n 0) second\n",
            "<ul>\n<li>first</li>\n</ul>\n<ol start=\"0\">\n<li>second</li>\n</ol>\n",
        ),
        (
            "1. first\n-\n",
            "<ol>\n<li>first</li>\n</ol>\n<ul>\n<li></li>\n</ul>\n",
        ),
        (
            "- first\n+\n",
            "<ul>\n<li>first</li>\n</ul>\n<ul>\n<li></li>\n</ul>\n",
        ),
        (
            "1. first\n 2) second\n",
            "<ol>\n<li>first</li>\n</ol>\n<ol start=\"2\">\n<li>second</li>\n</ol>\n",
        ),
    ] {
        for newline in ["\n", "\r\n", "\r"] {
            let source = source.replace('\n', newline);
            assert_eq!(fragment(&document(&source)), expected, "{source:?}");
        }
    }

    for source in [
        "- first\n  2. second\n",
        "1. first\n   2. second\n",
        "- first\n1234567890. second\n",
        "- first\n2.second\n",
    ] {
        let parsed = document(source);
        let outer = list(&parsed.blocks[0]);
        assert_eq!(parsed.blocks.len(), 1, "{source:?}");
        assert_eq!(outer.items.len(), 1, "{source:?}");
        assert_eq!(outer.items[0].blocks.len(), 1, "{source:?}");
        assert!(matches!(outer.items[0].blocks[0], Block::Paragraph(_)));
    }
}

#[test]
fn unmarked_quotes_end_before_new_ordered_or_empty_list_items() {
    for source in ["> first\n2. second\n", "> - first\n2. second\n"] {
        let parsed = document(source);
        assert_eq!(parsed.blocks.len(), 2, "{source:?}");
        assert!(matches!(parsed.blocks[0], Block::BlockQuote(_)));
        let numbered = list(&parsed.blocks[1]);
        assert!(numbered.ordered && numbered.tight);
        assert_eq!(numbered.start, 2);
        assert_eq!(numbered.items[0].blocks, [paragraph("second")]);
    }
    assert_eq!(
        fragment(&document("> first\n-\n")),
        "<blockquote>\n<p>first</p>\n</blockquote>\n<ul>\n<li></li>\n</ul>\n"
    );
    for source in ["> first\n> 2. second\n", "> first\n    2. second\n"] {
        assert_eq!(
            fragment(&document(source)),
            "<blockquote>\n<p>first\n2. second</p>\n</blockquote>\n",
            "{source:?}"
        );
    }
}

#[test]
fn a_gap_after_a_sublist_uses_the_following_blocks_actual_owner() {
    // The child marker may use any of its three optional indentation columns.
    // Dedenting below its content returns to a direct parent block; staying
    // inside the child loosens only that child (CommonMark 5.3).
    for marker_indent in 2..=5 {
        let child_content = marker_indent + 2;
        for next_indent in 2..=child_content + 2 {
            let source = format!(
                "- parent\n{}- child\n\n{}tail\n",
                " ".repeat(marker_indent),
                " ".repeat(next_indent)
            );
            let parsed = document(&source);
            let outer = list(&parsed.blocks[0]);
            let blocks = &outer.items[0].blocks;
            let inner = list(&blocks[1]);
            if next_indent < child_content {
                assert!(!outer.tight && inner.tight, "{source:?}");
                assert_eq!(blocks.len(), 3, "{source:?}");
                let expected = if next_indent >= 6 {
                    Block::CodeBlock {
                        lang: None,
                        code: format!("{}tail\n", " ".repeat(next_indent - 6)),
                    }
                } else {
                    paragraph("tail")
                };
                assert_eq!(blocks[2], expected, "{source:?}");
                assert_eq!(inner.items[0].blocks, [paragraph("child")]);
            } else {
                assert!(outer.tight && !inner.tight, "{source:?}");
                assert_eq!(blocks.len(), 2, "{source:?}");
                assert_eq!(
                    inner.items[0].blocks,
                    [paragraph("child"), paragraph("tail")],
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn blank_separators_between_child_items_do_not_loosen_the_parent() {
    for blank in ["\n", "\n\n"] {
        let source = format!("- parent\n  - first\n{blank}  - second\n");
        let parsed = document(&source);
        let outer = list(&parsed.blocks[0]);
        let inner = list(&outer.items[0].blocks[1]);
        assert!(outer.tight && !inner.tight, "{source:?}");
        assert_eq!(inner.items.len(), 2);
        assert_eq!(
            fragment(&parsed),
            concat!(
                "<ul>\n<li>parent\n<ul>\n<li>\n<p>first</p>\n</li>\n",
                "<li>\n<p>second</p>\n</li>\n</ul>\n</li>\n</ul>\n"
            )
        );
    }
    for marker in ["+", "2."] {
        let source = format!("- parent\n  - first\n\n  {marker} second\n");
        let parsed = document(&source);
        let outer = list(&parsed.blocks[0]);
        assert!(!outer.tight, "{source:?}");
        assert_eq!(outer.items[0].blocks.len(), 3, "{source:?}");
        assert!(list(&outer.items[0].blocks[1]).tight);
        assert!(list(&outer.items[0].blocks[2]).tight);
        assert!(fragment(&parsed).contains("<p>parent</p>"));
    }
}

#[test]
fn sibling_boundaries_preserve_reference_ownership_and_unicode_source_spans() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = "- café\n2. [réf]: /target\n3. [réf]\n".replace('\n', newline);
        let parsed = document(&source);
        assert_eq!(parsed.blocks.len(), 2, "{source:?}");
        let numbered = list(&parsed.blocks[1]);
        assert_eq!(numbered.start, 2);
        assert_eq!(numbered.items.len(), 2);
        assert!(numbered.items[0].blocks.is_empty());
        assert_eq!(
            numbered.items[1].blocks,
            [Block::Paragraph(vec![Inline::Link {
                dest: "/target".into(),
                title: None,
                content: vec![Inline::Text("réf".into())],
            }])]
        );
        let destinations = destination_spans(&source);
        assert_eq!(destinations.len(), 1);
        assert_eq!(&source[destinations[0].range.clone()], "/target");
        let labels = reference_label_spans(&source);
        assert_eq!(labels.len(), 2);
        for label in labels {
            assert_eq!(&source[label.label.unwrap()], "réf");
        }
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(spanned.blocks[0].span.slice(&source), Some("- café"));
        let second = format!("2. [réf]: /target{newline}3. [réf]");
        assert_eq!(spanned.blocks[1].span.slice(&source), Some(second.as_str()));
    }
}

#[test]
fn nested_container_rendering_uses_the_same_sibling_boundaries() {
    let source = "> - parent\n>   - first\n>   2. second\n>\n>    tail\n";
    let parsed = document(source);
    let Block::BlockQuote(quoted) = &parsed.blocks[0] else {
        panic!("quote");
    };
    let outer = list(&quoted[0]);
    assert!(!outer.tight);
    assert_eq!(outer.items[0].blocks.len(), 4);
    assert!(!list(&outer.items[0].blocks[1]).ordered);
    assert_eq!(list(&outer.items[0].blocks[2]).start, 2);
    assert_eq!(outer.items[0].blocks[3], paragraph("tail"));
    let native = franken_markdown::render_html_document(&parsed, &HtmlOptions::default()).unwrap();
    let browser = franken_markdown::wasm::render_html(
        source,
        &franken_markdown::wasm::WasmRenderOptions::default(),
    )
    .unwrap();
    assert_eq!(browser.bytes, native.as_bytes());
    assert!(browser.diagnostics.is_empty());
}

#[test]
fn published_pdf_keeps_separate_sibling_list_structure() {
    let parsed = document("- first\n2. second\n");
    let pdf =
        franken_markdown::render_pdf_document(&parsed, &franken_markdown::PdfOptions::default())
            .unwrap();
    let syntax = String::from_utf8_lossy(&pdf);
    // Inspect the emitted structure dictionaries, not a reconstructed AST or
    // renderer helper: both items must belong to their distinct list owners.
    assert_eq!(syntax.matches("/S /L ").count(), 2);
    assert_eq!(syntax.matches("/S /LI ").count(), 2);
}
