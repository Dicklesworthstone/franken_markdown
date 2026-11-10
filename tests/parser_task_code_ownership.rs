//! Code indentation takes precedence over checkbox-shaped list content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, List, PdfOptions, parse_markdown,
    parse_markdown_profiled, parse_markdown_spanned, render_pdf,
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
        panic!("expected a list: {block:?}");
    };
    list
}

fn code(text: &str) -> Block {
    Block::CodeBlock {
        lang: None,
        code: format!("{text}\n"),
    }
}

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.into())])
}

fn fragment(document: &Document) -> String {
    franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default())
}

fn first_container_code(block: &Block) -> &str {
    match block {
        Block::CodeBlock { code, .. } => code,
        Block::List(list) => {
            assert_eq!(list.items[0].task, None);
            first_container_code(&list.items[0].blocks[0])
        }
        Block::BlockQuote(blocks) => first_container_code(&blocks[0]),
        other => panic!("expected a container with literal code: {other:?}"),
    }
}

#[test]
fn checkbox_shaped_tokens_in_indented_code_remain_literal() {
    // CommonMark 5.2: five or more columns after a list marker leave four or
    // more content columns, so the first block is code, even for a GFM token.
    for marker in ["-", "+", "*", "1.", "23)"] {
        for checkbox in ["[ ]", "[x]", "[X]"] {
            for spaces in 5..=8 {
                let literal = format!("{}{checkbox} literal", " ".repeat(spaces - 5));
                let source = format!("{marker}{}{checkbox} literal\n", " ".repeat(spaces));
                let parsed = document(&source);
                let list = list(&parsed.blocks[0]);
                assert_eq!(list.items.len(), 1, "{source:?}");
                assert_eq!(list.items[0].task, None, "{source:?}");
                assert_eq!(list.items[0].blocks, vec![code(&literal)], "{source:?}");
                let rendered = fragment(&parsed);
                assert!(rendered.contains(&format!("<pre><code>{literal}\n</code></pre>")));
                assert!(!rendered.contains("type=\"checkbox\""), "{source:?}");
            }
        }
    }
}

#[test]
fn literal_checkboxes_do_not_turn_code_into_lazy_paragraphs() {
    for marker in ["-", "1.", "23)"] {
        let source = format!("{marker}     [x] literal\noutside\n");
        let parsed = document(&source);
        assert_eq!(parsed.blocks.len(), 2, "{source:?}: {parsed:?}");
        assert_eq!(
            list(&parsed.blocks[0]).items[0].blocks,
            vec![code("[x] literal")]
        );
        assert_eq!(parsed.blocks[1], paragraph("outside"));
    }
}

#[test]
fn checkbox_shaped_code_cannot_define_references_or_editable_links() {
    for body in [
        "-     [x] [hidden]: /secret",
        "> -     [x] [hidden]: /secret",
        "- -     [x] [hidden]: /secret",
        "Use [^note].\n\n[^note]:\n    -     [x] [hidden]: /secret",
    ] {
        let source = format!("{body}\n\n[hidden]\n");
        let parsed = document(&source);
        assert_eq!(
            parsed.blocks.last(),
            Some(&paragraph("[hidden]")),
            "{source:?}"
        );
        assert!(destination_spans(&source).is_empty(), "{source:?}");
        let rendered = fragment(&parsed);
        assert!(rendered.contains("[x] [hidden]: /secret"), "{source:?}");
        assert!(!rendered.contains("href=\"/secret\""), "{source:?}");
    }
}

#[test]
fn unambiguous_mixed_tab_code_cannot_define_references() {
    // Two tabs, or a tab followed by three spaces, span at least four
    // columns regardless of the original container's physical tab phase.
    // This checks code/source ownership without pinning the parser's existing
    // treatment of the exact residual indentation inside nested containers.
    for container in ["", "> ", "- "] {
        for marker in ["-", "1.", "23)"] {
            for padding in [" \t\t", " \t   ", "  \t\t"] {
                let source =
                    format!("{container}{marker}{padding}[x] [hidden]: /secret\n\n[hidden]\n");
                let parsed = document(&source);
                assert!(
                    first_container_code(&parsed.blocks[0]).contains("[x] [hidden]: /secret"),
                    "{source:?}"
                );
                assert_eq!(
                    parsed.blocks.last(),
                    Some(&paragraph("[hidden]")),
                    "{source:?}"
                );
                assert!(destination_spans(&source).is_empty(), "{source:?}");
                let html = fragment(&parsed);
                assert!(html.contains("<pre><code>"), "{source:?}");
                assert!(html.contains("[x] [hidden]: /secret"), "{source:?}");
                assert!(!html.contains("type=\"checkbox\""), "{source:?}");
                assert!(!html.contains("href=\"/secret\""), "{source:?}");
            }
        }
    }
}

#[test]
fn mixed_tab_code_keeps_real_reference_ranges_and_closes_lazy_continuation() {
    for padding in [" \t\t", " \t   ", "  \t\t"] {
        let source = format!(
            "\u{feff}-{padding}[x] [réad]: /hidden\r\noutside\r\n\r\n[réad]: /réal#β\r\n\r\n[réad]\r\n"
        );
        let parsed = document(&source);
        assert!(first_container_code(&parsed.blocks[0]).contains("[x] [réad]: /hidden"));
        assert_eq!(parsed.blocks[1], paragraph("outside"), "{source:?}");
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{source:?}: {spans:?}");
        assert_eq!(spans[0].dest, "/réal#β");
        assert_eq!(&source[spans[0].range.clone()], "/réal#β");
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(
            spanned.blocks.last().unwrap().span.slice(&source),
            Some("[réad]")
        );
        assert!(!fragment(&parsed).contains("href=\"/hidden\""));
    }
}

#[test]
fn mixed_tab_code_stays_literal_in_pdf_and_svg() {
    let source = "- \t\t[x] [hidden]: /secret\n\n[hidden]\n";
    let parsed = document(source);
    assert!(first_container_code(&parsed.blocks[0]).contains("[x] [hidden]: /secret"));
    let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(pdf_text.contains("/S /Code"));
    assert!(!pdf_text.contains("/URI"));
    let svg =
        franken_markdown::svg::render_svg(&parsed, &franken_markdown::svg::SvgOptions::default());
    assert!(!String::from_utf8_lossy(&svg).contains("href=\"/secret\""));
}

#[test]
fn valid_references_after_literal_code_keep_original_unicode_crlf_ranges() {
    let source = "\u{feff}-     [x] [réad]: /hidden\r\n\r\n[réad]: /réal#β\r\n\r\n[réad]\r\n";
    let parsed = document(source);
    assert_eq!(
        list(&parsed.blocks[0]).items[0].blocks,
        vec![code("[x] [réad]: /hidden")]
    );
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(spans[0].dest, "/réal#β");
    assert_eq!(&source[spans[0].range.clone()], "/réal#β");
    let spanned = parse_markdown_spanned(source);
    assert_eq!(
        spanned.blocks.last().unwrap().span.slice(source),
        Some("[réad]")
    );
    assert!(!fragment(&parsed).contains("href=\"/hidden\""));
}

#[test]
fn existing_expanded_tab_code_indentation_also_keeps_literal_checkboxes() {
    for (marker, leading_code_spaces) in [("-", 2), ("1.", 1), ("12.", 0)] {
        let source = format!("{marker}\t\t[x] literal\n");
        let parsed = document(&source);
        assert_eq!(list(&parsed.blocks[0]).items[0].task, None);
        assert_eq!(
            list(&parsed.blocks[0]).items[0].blocks,
            vec![code(&format!(
                "{}[x] literal",
                " ".repeat(leading_code_spaces)
            ))],
            "{source:?}"
        );
    }
}

#[test]
fn nested_tab_padding_keeps_its_existing_container_boundaries() {
    // A nested marker does not start at the physical source column zero.
    // Task recognition must not reinterpret that marker's padding locally.
    for source in [
        "- -\tfoo\n\n    continued\n",
        "> -\tfoo\n>\n>   continued\n",
    ] {
        let parsed = document(source);
        let inner = match &parsed.blocks[0] {
            Block::List(outer) => list(&outer.items[0].blocks[0]),
            Block::BlockQuote(blocks) => list(&blocks[0]),
            other => panic!("expected container: {other:?}"),
        };
        assert!(!inner.tight, "{source:?}");
        assert_eq!(
            inner.items[0].blocks,
            vec![paragraph("foo"), paragraph("continued")],
            "{source:?}"
        );
    }
}

#[test]
fn real_tasks_and_empty_items_still_follow_their_normal_rules() {
    for padding in [" ", "  ", "   ", "    ", "\t", " \t", "  \t", "\t "] {
        let source = format!("-{padding}[x] done\ncontinued\n");
        let parsed = document(&source);
        assert_eq!(parsed.blocks.len(), 1, "{source:?}");
        let item = &list(&parsed.blocks[0]).items[0];
        assert_eq!(item.task, Some(true), "{source:?}");
        assert_eq!(
            item.blocks,
            vec![Block::Paragraph(vec![
                Inline::Text("done".into()),
                Inline::SoftBreak,
                Inline::Text("continued".into()),
            ])]
        );
    }
    for padding in ["\t", " \t", "\t\t", "     "] {
        let source = format!("-{padding}\n- tail\n");
        let parsed = document(&source);
        assert_eq!(list(&parsed.blocks[0]).items.len(), 2, "{source:?}");
        assert_eq!(list(&parsed.blocks[0]).items[0].task, None);
        assert!(list(&parsed.blocks[0]).items[0].blocks.is_empty());
    }
}

#[test]
fn code_keeps_internal_tabs_and_does_not_grant_inline_destination_authority() {
    let source = "-\t\t[x]\t[hidden](/secret)\n\n[visible](/réal)\n";
    let parsed = document(source);
    assert_eq!(list(&parsed.blocks[0]).items[0].task, None);
    assert_eq!(
        list(&parsed.blocks[0]).items[0].blocks,
        vec![code("  [x]\t[hidden](/secret)")]
    );
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/réal");
    assert_eq!(&source[spans[0].range.clone()], "/réal");
}

#[test]
fn pdf_and_svg_preserve_the_same_literal_code_as_an_explicit_fence() {
    let source = "-     [x] [hidden]: /secret\n\n[hidden]\n";
    let fenced = "- ```\n  [x] [hidden]: /secret\n  ```\n\n[hidden]\n";
    let parsed = document(source);
    assert_eq!(parsed, document(fenced));
    let pdf_options = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let pdf = render_pdf(source, &pdf_options).unwrap();
    assert_eq!(pdf, render_pdf(fenced, &pdf_options).unwrap());
    assert!(!String::from_utf8_lossy(&pdf).contains("/URI"));
    let svg_options = franken_markdown::svg::SvgOptions::default();
    let svg = franken_markdown::svg::render_svg(&parsed, &svg_options);
    assert_eq!(
        svg,
        franken_markdown::svg::render_svg(&document(fenced), &svg_options)
    );
    assert!(!String::from_utf8_lossy(&svg).contains("href=\"/secret\""));
}
