//! Multiline references have a source-character label limit, not a line limit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn link(label: &str, destination: &str, title: Option<&str>) -> Inline {
    Inline::Link {
        dest: destination.into(),
        title: title.map(str::to_owned),
        content: vec![text(label)],
    }
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

fn title_lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("line {index}: café &amp; \\* &quot;quoted&quot; \\(β\\)"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn decoded_title_lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("line {index}: café & * \"quoted\" (β)"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn long_titles_keep_all_lines_and_decode_entities_and_escapes() {
    let raw = title_lines(48);
    let decoded = decoded_title_lines(48);
    for (open, close) in [('"', '"'), ('\'', '\''), ('(', ')')] {
        for separator in [" ", "\n"] {
            let source =
                format!("[id]: </réf\\(x\\)#section>{separator}{open}{raw}{close}\n\nRead [id].\n");
            let document = parse_markdown(&source);
            assert_eq!(
                document.blocks,
                vec![Block::Paragraph(vec![
                    text("Read "),
                    link("id", "/réf(x)#section", Some(&decoded)),
                    text("."),
                ])],
                "{open} title with separator {separator:?}"
            );
            assert_eq!(parse_markdown_profiled(&source).document, document);
            let spanned = parse_markdown_spanned(&source);
            assert_eq!(spanned.to_document(), document);
            assert_eq!(spanned.blocks[0].span.slice(&source), Some("Read [id]."));
            let spans = destination_spans(&source);
            assert_eq!(spans.len(), 1);
            assert_eq!(&source[spans[0].range.clone()], "</réf\\(x\\)#section>");
            assert_eq!(spans[0].dest, "/réf(x)#section");
            assert_eq!(
                &source[spans[0].fragment_range(&source).unwrap()],
                "section"
            );
            let html = fragment(&source);
            assert!(html.contains("line 47: café &amp; * &quot;quoted&quot; (β)"));
            assert!(!html.contains("[id]:"));
        }
    }
}

#[test]
fn multiline_labels_accept_exactly_999_source_characters() {
    // 500 physical lines, but only 999 label characters (including line endings).
    let label = format!("{}x", "é\n".repeat(499));
    assert_eq!(label.chars().count(), 999);
    let normalized = label.replace('\n', " ");
    let source = format!("[{label}]: /valid\n\n[visible][{normalized}]\n");
    let document = parse_markdown(&source);
    assert_eq!(
        document.blocks,
        vec![Block::Paragraph(vec![link("visible", "/valid", None)])]
    );
    assert_eq!(parse_markdown_spanned(&source).to_document(), document);
    let spans = destination_spans(&source);
    assert_eq!(spans.len(), 1);
    assert_eq!(&source[spans[0].range.clone()], "/valid");
}

#[test]
fn long_multiline_labels_keep_the_character_limit_and_visible_invalid_source() {
    let label = format!("{}xy", "é\n".repeat(499));
    assert_eq!(label.chars().count(), 1000);
    let source = format!("[{label}]: /invalid\n\nAfter.\n");
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 2);
    let Block::Paragraph(inlines) = &document.blocks[0] else {
        panic!("invalid definition must stay visible");
    };
    assert!(matches!(inlines.last(), Some(Inline::Text(last)) if last == "xy]: /invalid"));
    assert_eq!(document.blocks[1], Block::Paragraph(vec![text("After.")]));
    assert!(destination_spans(&source).is_empty());
    assert_eq!(parse_markdown_spanned(&source).to_document(), document);
}

#[test]
fn long_definitions_in_containers_preserve_lazy_content_and_first_definition() {
    let raw = title_lines(24);
    let decoded = decoded_title_lines(24);
    let definition = format!("[id]: /first \"{raw}\"");
    for prefix in ["> ", "- "] {
        let continuation = if prefix == "> " { "> " } else { "  " };
        let mut source = String::new();
        for (index, line) in definition.lines().enumerate() {
            source.push_str(if index == 0 { prefix } else { continuation });
            source.push_str(line);
            source.push('\n');
        }
        source.push_str("continued [id]\n\n[id]: /later\n\noutside [id]\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{prefix:?}: {document:?}");
        let inner = match &document.blocks[0] {
            Block::BlockQuote(blocks) => blocks,
            Block::List(list) => {
                assert_eq!(list.items.len(), 1);
                &list.items[0].blocks
            }
            other => panic!("expected original container: {other:?}"),
        };
        assert_eq!(
            inner,
            &[Block::Paragraph(vec![
                text("continued "),
                link("id", "/first", Some(&decoded)),
            ])]
        );
        assert_eq!(
            document.blocks[1],
            Block::Paragraph(vec![text("outside "), link("id", "/first", Some(&decoded)),])
        );
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
        let spans = destination_spans(&source);
        assert_eq!(
            spans
                .iter()
                .map(|span| span.dest.as_str())
                .collect::<Vec<_>>(),
            ["/first", "/later"]
        );
    }
}

#[test]
fn unicode_crlf_labels_keep_exact_destination_ranges_inside_containers() {
    let label = (0..16)
        .map(|index| format!("réf{index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let normalized = label.replace('\n', " ");
    for prefix in ["", "> ", "- "] {
        let mut source = String::from("\u{feff}");
        let definition = format!("[{label}]:\n</café\\(β\\)?q=1&amp;x=2&#35;ancêtre>");
        for (index, line) in definition.lines().enumerate() {
            source.push_str(if index == 0 || prefix != "- " {
                prefix
            } else {
                "  "
            });
            source.push_str(line);
            source.push_str("\r\n");
        }
        source.push_str(&format!("\r\n[visible][{normalized}]\r\n"));
        let document = parse_markdown(&source);
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
        assert_eq!(parse_markdown_profiled(&source).document, document);
        assert!(
            matches!(
                document.blocks.last(),
                Some(Block::Paragraph(inlines))
                    if inlines == &[link("visible", "/café(β)?q=1&x=2#ancêtre", None)]
            ),
            "{prefix:?}: {document:?}"
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{prefix:?}");
        assert_eq!(
            &source[spans[0].range.clone()],
            "</café\\(β\\)?q=1&amp;x=2&#35;ancêtre>"
        );
        assert_eq!(
            &source[spans[0].fragment_range(&source).unwrap()],
            "ancêtre"
        );
    }
}

#[test]
fn malformed_long_titles_never_hide_source_or_shadow_a_later_definition() {
    let body = "long title line\n".repeat(40);
    for ending in ["unterminated", "done\" trailing junk", "\nblank break\""] {
        let source = format!("[id]: /invalid \"{body}{ending}\n\n[id]: /valid\n\nUse [id].\n");
        let document = parse_markdown(&source);
        assert!(matches!(
            document.blocks.last(),
            Some(Block::Paragraph(inlines))
                if inlines == &[text("Use "), link("id", "/valid", None), text(".")]
        ));
        assert!(fragment(&source).contains("/invalid"));
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{ending:?}: {spans:?}");
        assert_eq!(spans[0].dest, "/valid");
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
    }
}

#[test]
fn malformed_next_line_long_titles_keep_the_valid_destination_fallback() {
    let body = "long title line\n".repeat(40);
    for ending in ["unterminated", "done\" trailing junk", "\nblank break\""] {
        let source = format!("[id]: /valid\n\"{body}{ending}\n\nUse [id].\n");
        let document = parse_markdown(&source);
        assert!(matches!(
            document.blocks.last(),
            Some(Block::Paragraph(inlines))
                if inlines == &[text("Use "), link("id", "/valid", None), text(".")]
        ));
        assert!(fragment(&source).contains("long title line"));
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].dest, "/valid");
        assert_eq!(&source[spans[0].range.clone()], "/valid");
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
    }
}

#[test]
fn long_reference_definitions_preserve_separate_ordered_lists() {
    let source = format!(
        "1. before\n\n[id]: /target \"{}\"\n\n7. [after][id]\n",
        title_lines(24)
    );
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 2);
    for (block, start) in document.blocks.iter().zip([1, 7]) {
        let Block::List(list) = block else {
            panic!("expected ordered list");
        };
        assert!(list.ordered && list.tight);
        assert_eq!(list.start, start);
        assert_eq!(list.items.len(), 1);
    }
    assert!(fragment(&source).contains("</ol>\n<ol start=\"7\">"));
}

#[test]
fn long_reference_looking_code_stays_literal_and_cannot_define_links() {
    let definition = format!("[id]: /literal \"{}\"", title_lines(24));
    for source in [
        format!("```\n{definition}\n```\n\n[id]\n"),
        format!(
            "- ```\n  {}\n  ```\n\n[id]\n",
            definition.replace('\n', "\n  ")
        ),
    ] {
        let document = parse_markdown(&source);
        assert_eq!(
            document.blocks.last(),
            Some(&Block::Paragraph(vec![text("[id]")]))
        );
        assert!(destination_spans(&source).is_empty());
        assert!(fragment(&source).contains("/literal"));
    }
}

#[test]
fn long_unclosed_titles_and_many_malformed_starters_remain_bounded() {
    let title = "ordinary text\n".repeat(20_000);
    let source = format!("[id]: /valid \"{title}last\"\n\n[id]\n");
    assert_eq!(
        parse_markdown(&source).blocks,
        vec![Block::Paragraph(vec![link(
            "id",
            "/valid",
            Some(&format!("{title}last")),
        )])]
    );

    // One large candidate and many overlapping rejected candidates exercise
    // different failure paths. No valid reference may be invented on fallback.
    for source in [
        format!(
            "[id]: /invalid \"{}\n\nTail.\n",
            "ordinary text\n".repeat(20_000)
        ),
        format!(
            "{}\nTail.\n",
            "# boundary\n[broken\n[id]: /invalid \"unfinished\n".repeat(4_000)
        ),
    ] {
        let document = parse_markdown(&source);
        assert_eq!(
            document.blocks.last(),
            Some(&Block::Paragraph(vec![text("Tail.")]))
        );
        assert!(destination_spans(&source).is_empty());
        assert_eq!(parse_markdown_profiled(&source).document, document);
    }
}
