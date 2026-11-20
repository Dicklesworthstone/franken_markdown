//! Structural tab stops retain their original column through nested containers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, reference_label_spans};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled,
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
    assert_eq!(
        parse_markdown_spanned_profiled(source)
            .document
            .to_document(),
        parsed,
        "{source:?}"
    );
    parsed
}

fn leaf(mut blocks: &[Block]) -> &Block {
    loop {
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        match &blocks[0] {
            Block::BlockQuote(inner) => blocks = inner,
            Block::List(list) => {
                assert_eq!(list.items.len(), 1);
                blocks = &list.items[0].blocks;
            }
            block => return block,
        }
    }
}

fn text(value: &str) -> Vec<Inline> {
    vec![Inline::Text(value.into())]
}

fn paragraph(value: &str) -> Block {
    Block::Paragraph(text(value))
}

#[test]
fn tab_separators_keep_headings_and_inline_markup_live() {
    for prefix in ["> \t", "- \t", "1. \t", "> -\t", "- >\t", "1. >\t"] {
        let source = format!("{prefix}# **Heading** [doc](/target)\n");
        assert_eq!(
            leaf(&document(&source).blocks),
            &Block::Heading {
                level: 1,
                inlines: vec![
                    Inline::Strong(text("Heading")),
                    Inline::Text(" ".into()),
                    Inline::Link {
                        dest: "/target".into(),
                        title: None,
                        content: text("doc"),
                    },
                ],
            },
            "{source:?}"
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{source:?}: {spans:?}");
        assert_eq!(&source[spans[0].range.clone()], "/target");
    }
}

#[test]
fn tabbed_list_padding_sets_the_real_continuation_indent() {
    for marker in ["-", "+", "*", "1.", "23)"] {
        let source = format!("{marker}\titem\n\n  outside\n");
        let parsed = document(&source);
        assert_eq!(parsed.blocks.len(), 2, "{source:?}: {parsed:?}");
        let Block::List(list) = &parsed.blocks[0] else {
            panic!("expected list: {parsed:?}");
        };
        assert!(list.tight, "{source:?}");
        assert_eq!(list.items[0].blocks, [paragraph("item")]);
        assert_eq!(parsed.blocks[1], paragraph("outside"));

        let source = format!("{marker}\titem\n\n\tinside\n");
        let parsed = document(&source);
        assert_eq!(parsed.blocks.len(), 1, "{source:?}: {parsed:?}");
        let Block::List(list) = &parsed.blocks[0] else {
            panic!("expected list: {parsed:?}");
        };
        assert!(!list.tight, "{source:?}");
        assert_eq!(
            list.items[0].blocks,
            [paragraph("item"), paragraph("inside")]
        );
    }
}

#[test]
fn nested_list_siblings_use_physical_tab_columns() {
    let source = ">  - foo\n>    - bar\n> \t - baz\n";
    let parsed = document(source);
    let Block::BlockQuote(quoted) = &parsed.blocks[0] else {
        panic!("expected quote: {parsed:?}");
    };
    let Block::List(outer) = &quoted[0] else {
        panic!("expected outer list: {quoted:?}");
    };
    assert_eq!(outer.items.len(), 1);
    assert_eq!(outer.items[0].blocks[0], paragraph("foo"));
    let Block::List(inner) = &outer.items[0].blocks[1] else {
        panic!("expected inner list: {outer:?}");
    };
    assert_eq!(inner.items.len(), 2);
    assert_eq!(inner.items[0].blocks, [paragraph("bar")]);
    assert_eq!(inner.items[1].blocks, [paragraph("baz")]);

    // A continuation may begin after more than three physical columns: the
    // enclosing list strips its columns before this nested marker is parsed.
    let source = "- outer\n\n    -\titem\n\n      outside\n";
    let parsed = document(source);
    let Block::List(outer) = &parsed.blocks[0] else {
        panic!("expected outer list: {parsed:?}");
    };
    assert_eq!(outer.items[0].blocks.len(), 3, "{parsed:?}");
    assert_eq!(
        outer.items[0].blocks[2],
        Block::CodeBlock {
            lang: None,
            code: "outside\n".into()
        }
    );
}

#[test]
fn nested_indented_code_keeps_only_unconsumed_tab_columns() {
    // Tab stops belong to the original physical line, including every quote
    // and list marker; removing a container never resets a later tab's phase.
    for (source, expected) in [
        (">   \tvalue\n", "  value\n"),
        ("-   \tvalue\n", "  value\n"),
        ("1.   \tvalue\n", " value\n"),
        ("> >\t\tvalue\n", "value\n"),
        ("- >\t\tvalue\n", "value\n"),
        ("1. >\t\tvalue\n", "   value\n"),
        ("> - >\t\tvalue\n", "  value\n"),
    ] {
        assert_eq!(
            leaf(&document(source).blocks),
            &Block::CodeBlock {
                lang: None,
                code: expected.into(),
            },
            "{source:?}"
        );
    }
}

#[test]
fn literal_code_keeps_marker_shaped_prefixes_and_tabs_byte_for_byte() {
    let body = "> \t# heading\n- \t[hidden](/never)\n  2.\tvalue\n\t-\tcode\n>>\ttext\n";
    for source in [
        format!("```\n{body}```\n"),
        format!(
            "> ```\n{}> ```\n",
            body.lines()
                .map(|line| format!("> {line}\n"))
                .collect::<String>()
        ),
        format!(
            "- ```\n{}  ```\n",
            body.lines()
                .map(|line| format!("  {line}\n"))
                .collect::<String>()
        ),
    ] {
        let parsed = document(&source);
        assert_eq!(
            leaf(&parsed.blocks),
            &Block::CodeBlock {
                lang: None,
                code: body.into()
            },
            "{source:?}"
        );
        assert!(destination_spans(&source).is_empty());
        let html = franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
        assert!(html.contains("- \t[hidden](/never)"));
        assert!(!html.contains("href=\"/never\""));
    }
}

#[test]
fn tabbed_reference_definitions_keep_unicode_source_coordinates() {
    for prefix in ["> \t", "- \t", "1. \t", "> -\t", "- >\t"] {
        for newline in ["\n", "\r\n", "\r"] {
            let source =
                format!("\u{feff}{prefix}[réad]: /réal#β{newline}{newline}[réad]{newline}");
            let parsed = document(&source);
            assert!(
                matches!(parsed.blocks.last(), Some(Block::Paragraph(inlines))
                if matches!(&inlines[0], Inline::Link { dest, .. } if dest == "/réal#β")),
                "{source:?}: {parsed:?}"
            );
            let spans = destination_spans(&source);
            assert_eq!(spans.len(), 1, "{source:?}: {spans:?}");
            assert_eq!(&source[spans[0].range.clone()], "/réal#β");
            let labels = reference_label_spans(&source);
            assert_eq!(labels.len(), 2, "{source:?}: {labels:?}");
            for label in labels {
                assert_eq!(&source[label.label.unwrap()], "réad");
            }
            let spanned = parse_markdown_spanned(&source);
            assert_eq!(
                spanned.blocks.last().unwrap().span.slice(&source),
                Some("[réad]")
            );
            assert!(
                spanned.diagnostics.is_empty(),
                "{source:?}: {:?}",
                spanned.diagnostics
            );
        }
    }
}

#[test]
fn genuinely_indented_reference_text_stays_literal() {
    for prefix in [">   \t", "-   \t", "1.   \t", "> >\t\t"] {
        let source = format!("{prefix}[hidden]: /never\n\n[hidden]\n");
        let parsed = document(&source);
        assert!(
            matches!(leaf(&parsed.blocks[..1]), Block::CodeBlock { code, .. } if code.contains("[hidden]: /never"))
        );
        assert_eq!(parsed.blocks.last(), Some(&paragraph("[hidden]")));
        assert!(destination_spans(&source).is_empty());
        assert!(reference_label_spans(&source).is_empty());
    }
}

#[test]
fn include_scanning_uses_the_same_tabbed_fence_container_boundary() {
    use franken_markdown::transclude::{ResolveResult, expand_includes};
    use std::cell::Cell;

    for (source, expected) in [
        // The list's content begins at column four. This two-space include
        // lies outside its unclosed fence and must remain an active include.
        ("-\t```\n  {{#include actual.md}}\n", "-\t```\nACTUAL\n"),
        // Four columns do belong to the fence, including a literal directive.
        (
            "-\t```\n\t{{#include missing.md}}\n\t```\n\n{{#include actual.md}}\n",
            "-\t```\n\t{{#include missing.md}}\n\t```\n\nACTUAL\n",
        ),
    ] {
        let calls = Cell::new(0);
        let resolve = |path: &str, origin: &str| -> ResolveResult {
            assert_eq!(
                path, "actual.md",
                "literal code must not resolve: {source:?}"
            );
            assert_eq!(origin, "<input>");
            calls.set(calls.get() + 1);
            Ok(Some(("ACTUAL".into(), "actual.md".into())))
        };
        assert_eq!(
            expand_includes(source, &resolve).unwrap(),
            expected,
            "{source:?}"
        );
        assert_eq!(calls.get(), 1, "{source:?}");
    }
}
