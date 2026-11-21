//! Empty list items close before a blank without capturing later source blocks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, reference_label_spans};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, List, PdfOptions, parse_markdown,
    parse_markdown_profiled, parse_markdown_spanned, render_html_document, render_pdf_document,
};

fn document(source: &str) -> Document {
    let document = parse_markdown(source);
    assert_eq!(
        parse_markdown_profiled(source).document,
        document,
        "{source:?}"
    );
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), document, "{source:?}");
    assert!(spanned.diagnostics.is_empty(), "{source:?}");
    document
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
fn blank_after_any_empty_item_keeps_following_blocks_outside_the_list() {
    // CommonMark 5.2: a list item can begin with at most one blank line,
    // including its marker line. This applies to every item, not just the first.
    for marker in ["-", "*", "+", "1.", "7)"] {
        for prefix in [String::new(), format!("{marker} first\n")] {
            for blank in ["\n", " \t\n", "\n\n"] {
                for indent in 0..=7 {
                    let source =
                        format!("{prefix}{marker}\n{blank}{}outside\n", " ".repeat(indent));
                    let parsed = document(&source);
                    assert_eq!(parsed.blocks.len(), 2, "{source:?}: {parsed:?}");
                    let outer = list(&parsed.blocks[0]);
                    assert!(outer.tight, "{source:?}");
                    assert_eq!(outer.items.len(), 1 + usize::from(!prefix.is_empty()));
                    assert!(outer.items.last().unwrap().blocks.is_empty(), "{source:?}");
                    let expected = if indent < 4 {
                        paragraph("outside")
                    } else {
                        Block::CodeBlock {
                            lang: None,
                            code: format!("{}outside\n", " ".repeat(indent - 4)),
                        }
                    };
                    assert_eq!(parsed.blocks[1], expected, "{source:?}");
                }
            }
        }
    }
}

#[test]
fn blank_separated_siblings_still_form_one_loose_list_after_empty_items() {
    // Once the empty item closes, the following sibling may use any of the
    // three optional marker-indentation columns, including its old content column.
    for (marker, next) in [("-", "-"), ("*", "*"), ("1.", "2."), ("7)", "8)")] {
        for prefix in [String::new(), format!("{marker} first\n")] {
            for indent in 0..=3 {
                let source = format!("{prefix}{marker}\n\n{}{next} next\n", " ".repeat(indent));
                let parsed = document(&source);
                assert_eq!(parsed.blocks.len(), 1, "{source:?}: {parsed:?}");
                let outer = list(&parsed.blocks[0]);
                assert!(!outer.tight, "{source:?}");
                assert_eq!(outer.items.len(), 2 + usize::from(!prefix.is_empty()));
                assert!(outer.items[outer.items.len() - 2].blocks.is_empty());
                assert_eq!(outer.items.last().unwrap().blocks, [paragraph("next")]);
                assert!(fragment(&parsed).contains("<p>next</p>"));
            }
        }
    }

    let parsed = document("-\n\n-\n\n- last\n");
    let outer = list(&parsed.blocks[0]);
    assert_eq!(parsed.blocks.len(), 1);
    assert_eq!(outer.items.len(), 3);
    assert!(outer.items[0].blocks.is_empty());
    assert!(outer.items[1].blocks.is_empty());
    assert!(!outer.tight);
}

#[test]
fn blank_after_an_empty_item_does_not_reinterpret_other_block_openers() {
    for (tail, expected) in [
        ("  > quote\n", Block::BlockQuote(vec![paragraph("quote")])),
        (
            "  ```\ncode\n```\n",
            Block::CodeBlock {
                lang: None,
                code: "code\n".into(),
            },
        ),
        (
            "    - literal\n",
            Block::CodeBlock {
                lang: None,
                code: "- literal\n".into(),
            },
        ),
        ("  * * *\n", Block::ThematicBreak),
    ] {
        let source = format!("- first\n-\n\n{tail}");
        let parsed = document(&source);
        assert_eq!(parsed.blocks.len(), 2, "{source:?}: {parsed:?}");
        assert!(list(&parsed.blocks[0]).tight);
        assert_eq!(parsed.blocks[1], expected, "{source:?}");
    }
    for tail in ["+ separate", "2. separate", "  + separate"] {
        let source = format!("- first\n-\n\n{tail}\n");
        let parsed = document(&source);
        assert_eq!(parsed.blocks.len(), 2, "{source:?}");
        assert!(list(&parsed.blocks[0]).tight);
        let following = list(&parsed.blocks[1]);
        assert!(following.tight);
        assert_eq!(following.items.len(), 1);
        assert_eq!(following.items[0].blocks, [paragraph("separate")]);
    }
}

#[test]
fn code_after_an_empty_item_cannot_define_a_phantom_reference() {
    for marker in ["-", "1.", "7)"] {
        let source = format!("{marker} first\n{marker}\n\n    [réf]: /literal\n\nUse [réf].\n");
        for newline in ["\n", "\r\n", "\r"] {
            let source = source.replace('\n', newline);
            let parsed = document(&source);
            assert_eq!(parsed.blocks.len(), 3, "{source:?}: {parsed:?}");
            assert!(list(&parsed.blocks[0]).items[1].blocks.is_empty());
            assert_eq!(
                parsed.blocks[1],
                Block::CodeBlock {
                    lang: None,
                    code: "[réf]: /literal\n".into(),
                }
            );
            assert_eq!(parsed.blocks[2], paragraph("Use [réf]."));
            assert!(destination_spans(&source).is_empty(), "{source:?}");
            assert!(reference_label_spans(&source).is_empty(), "{source:?}");
            let (open, close) = match marker {
                "-" => ("<ul>", "</ul>"),
                "1." => ("<ol>", "</ol>"),
                _ => ("<ol start=\"7\">", "</ol>"),
            };
            assert_eq!(
                fragment(&parsed),
                format!(
                    "{open}\n<li>first</li>\n<li></li>\n{close}\n<pre><code>[réf]: /literal\n</code></pre>\n<p>Use [réf].</p>\n"
                )
            );
        }
    }
}

#[test]
fn following_real_definitions_resolve_without_hiding_literal_reference_source() {
    let source = "- first\n-\n\n    [ref]: /literal\n\n[ref]: /real\n\nUse [ref].\n";
    let parsed = document(source);
    let rendered = fragment(&parsed);
    assert!(rendered.contains("<pre><code>[ref]: /literal\n</code></pre>"));
    assert!(rendered.contains("Use <a href=\"/real\">ref</a>."));
    assert!(!rendered.contains("href=\"/literal\""));
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/real");
    assert_eq!(&source[destinations[0].range.clone()], "/real");
    assert_eq!(reference_label_spans(source).len(), 2);
}

#[test]
fn nested_containers_do_not_capture_code_after_an_empty_inner_item() {
    let body = "* first\n*\n\n    [réf]: /literal\n\nUse [réf].\n";
    for source in [
        body.lines()
            .map(|line| format!("> {line}\n"))
            .collect::<String>(),
        format!(
            "- parent\n{}",
            body.lines()
                .map(|line| format!("  {line}\n"))
                .collect::<String>()
        ),
        format!(
            "[^note]:\n{}\nRead[^note].\n",
            body.lines()
                .map(|line| format!("    {line}\n"))
                .collect::<String>()
        ),
    ] {
        let parsed = document(&source);
        let rendered = fragment(&parsed);
        assert!(
            rendered.contains("<pre><code>[réf]: /literal\n</code></pre>"),
            "{source:?}: {rendered}"
        );
        assert!(rendered.contains("Use [réf]."), "{source:?}: {rendered}");
        assert!(!rendered.contains("href=\"/literal\""));
        assert!(destination_spans(&source).is_empty(), "{source:?}");
        assert!(reference_label_spans(&source).is_empty(), "{source:?}");
    }
}

#[test]
fn a_blank_after_an_empty_child_uses_the_next_blocks_real_container() {
    for gap in ["\n", "\n\n", " \t\n\n"] {
        for indent in 2..=5 {
            let source = format!(
                "- parent\n  - first\n  -\n{gap}{}tail\n",
                " ".repeat(indent)
            );
            let parsed = document(&source);
            assert_eq!(parsed.blocks.len(), 1, "{source:?}");
            let outer = list(&parsed.blocks[0]);
            assert!(
                !outer.tight,
                "direct parent paragraph must loosen parent: {source:?}"
            );
            let blocks = &outer.items[0].blocks;
            assert_eq!(blocks.len(), 3, "{source:?}: {blocks:?}");
            let inner = list(&blocks[1]);
            assert!(inner.tight);
            assert_eq!(inner.items.len(), 2);
            assert!(inner.items[1].blocks.is_empty());
            assert_eq!(blocks[2], paragraph("tail"));
        }

        for indent in 2..=5 {
            let source = format!(
                "- parent\n  - first\n  -\n{gap}{}- next\n",
                " ".repeat(indent)
            );
            let parsed = document(&source);
            assert_eq!(parsed.blocks.len(), 1);
            let outer = list(&parsed.blocks[0]);
            assert!(
                outer.tight,
                "child sibling must not loosen parent: {source:?}"
            );
            let blocks = &outer.items[0].blocks;
            assert_eq!(blocks.len(), 2, "{source:?}: {blocks:?}");
            let inner = list(&blocks[1]);
            assert!(!inner.tight);
            assert_eq!(inner.items.len(), 3);
            assert!(inner.items[1].blocks.is_empty());
            assert_eq!(inner.items[2].blocks, [paragraph("next")]);
        }
    }
}

#[test]
fn empty_item_boundaries_preserve_unicode_crlf_source_spans() {
    let source = "intro α\r\n\r\n- café\r\n-\r\n \t\r\n    [réf]: /literal\r\n\r\nUse [réf].\r\n";
    let parsed = parse_markdown_spanned(source);
    assert_eq!(parsed.to_document(), document(source));
    assert_eq!(parsed.source_len, source.len());
    let spans: Vec<_> = parsed
        .blocks
        .iter()
        .map(|block| block.span.slice(source).unwrap())
        .collect();
    assert_eq!(
        spans,
        [
            "intro α",
            "- café\r\n-",
            "    [réf]: /literal",
            "Use [réf]."
        ]
    );
}

#[test]
fn nonempty_items_and_immediate_continuations_keep_their_content() {
    for source in ["-\n  text\n", "- first\n-\n  text\n"] {
        let parsed = document(source);
        assert_eq!(parsed.blocks.len(), 1);
        let outer = list(&parsed.blocks[0]);
        assert!(outer.tight);
        assert_eq!(outer.items.last().unwrap().blocks, [paragraph("text")]);
    }
    for source in [
        "- text\n\n  tail\n",
        "- first\n- text\n\n  tail\n",
        "-\n  text\n\n  tail\n",
    ] {
        let parsed = document(source);
        assert_eq!(parsed.blocks.len(), 1);
        let outer = list(&parsed.blocks[0]);
        assert!(!outer.tight);
        assert_eq!(
            outer.items.last().unwrap().blocks,
            [paragraph("text"), paragraph("tail")]
        );
    }

    let task = document("- [x]\n\n  tail\n");
    let task_list = list(&task.blocks[0]);
    assert!(!task_list.tight);
    assert_eq!(task_list.items[0].task, Some(true));
    assert_eq!(task_list.items[0].blocks, [paragraph("tail")]);

    let code = document("- text\n\n      alpha\n\n\n      beta\n");
    let code_list = list(&code.blocks[0]);
    assert!(!code_list.tight);
    assert_eq!(
        code_list.items[0].blocks,
        [
            paragraph("text"),
            Block::CodeBlock {
                lang: None,
                code: "alpha\n\n\nbeta\n".into(),
            }
        ]
    );
}

#[test]
fn native_and_browser_exports_retain_the_literal_code_and_unlinked_reference() {
    let source = "- first\n-\n\n    [ref]: /literal\n\nUse [ref].\n";
    let parsed = document(source);
    let html = render_html_document(&parsed, &HtmlOptions::default()).unwrap();
    assert!(html.contains("<pre><code>[ref]: /literal\n</code></pre>"));
    assert!(html.contains("<p>Use [ref].</p>"));
    let browser_options = franken_markdown::wasm::WasmRenderOptions::default();
    let browser_html = franken_markdown::wasm::render_html(source, &browser_options).unwrap();
    assert_eq!(browser_html.bytes, html.as_bytes());
    assert!(browser_html.diagnostics.is_empty());

    let options = PdfOptions::default();
    let report = franken_markdown::verify::verify_pdf_result(&parsed, &options).unwrap();
    let text = report
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    // Run boundaries and line wrapping may separate words. Discard only
    // whitespace while requiring every literal punctuation/label character.
    let reading: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    assert!(
        reading.contains("[ref]:/literal"),
        "missing source code: {text}"
    );
    assert!(
        reading.contains("Use[ref]."),
        "changed unresolved reference: {text}"
    );
    let pdf = render_pdf_document(&parsed, &options).unwrap();
    assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Link"));
    let browser_pdf = franken_markdown::wasm::render_pdf(source, &browser_options).unwrap();
    assert_eq!(browser_pdf.bytes, pdf);
}
