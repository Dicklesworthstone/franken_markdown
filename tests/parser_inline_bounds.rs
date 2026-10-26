//! Speculative inline parsing must preserve source and remain bounded on hostile input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{parse_markdown, parse_markdown_profiled, parse_markdown_spanned};

fn link(label: &str, dest: &str) -> Inline {
    Inline::Link {
        dest: dest.into(),
        title: None,
        content: vec![Inline::Text(label.into())],
    }
}

#[test]
fn a_long_url_suffix_keeps_balanced_parentheses_and_all_literal_text() {
    let suffix = ")".repeat(100_000);
    let source = format!("https://example.test/path(a(b)){suffix}");
    assert_eq!(
        parse_inlines(&source),
        vec![
            link(
                "https://example.test/path(a(b))",
                "https://example.test/path(a(b))",
            ),
            Inline::Text(suffix),
        ]
    );

    // The balance is counted across the entire URL, including interior close
    // parentheses, exactly as GFM's trailing-parenthesis rule requires.
    for (source, destination, tail) in [
        ("https://x.test/a)b)", "https://x.test/a)b", ")"),
        ("https://x.test/a(b))", "https://x.test/a(b)", ")"),
        ("https://x.test/(a(b))", "https://x.test/(a(b))", ""),
        ("https://x.test/(((a))", "https://x.test/(((a))", ""),
        ("www.x.test/a)))!", "http://www.x.test/a", ")))!"),
    ] {
        let label = destination.strip_prefix("http://").unwrap_or(destination);
        let mut expected = vec![link(label, destination)];
        if !tail.is_empty() {
            expected.push(Inline::Text(tail.into()));
        }
        assert_eq!(parse_inlines(source), expected, "{source:?}");
    }
}

#[test]
fn long_unmatched_delimiter_runs_preserve_the_entire_source() {
    // The former tokenizer recounted the same suffix from every '$' or '~'.
    // These inputs exercise the public parser at a size that made that work
    // quadratic; the regression has no host-speed-dependent timing assertion.
    for delimiter in ['$', '~'] {
        let source = format!("prefix {} suffix", delimiter.to_string().repeat(100_000));
        assert_eq!(parse_inlines(&source), vec![Inline::Text(source)]);
    }

    let source = format!("prefix {}end", "$value ".repeat(12_000));
    assert_eq!(parse_inlines(&source), vec![Inline::Text(source)]);
}

#[test]
fn repeated_unterminated_html_does_not_rescan_without_a_bound() {
    for opener in ["<!--", "<?", "<![CDATA[", "<!A"] {
        let source = format!("prefix {} suffix", opener.repeat(12_000));
        assert_eq!(parse_inlines(&source), vec![Inline::Text(source)]);
    }
}

#[test]
fn unmatched_code_span_lengths_and_link_destinations_are_bounded() {
    let mut source = String::from("prefix ");
    for ticks in 1..=512 {
        source.push_str(&"`".repeat(ticks));
        source.push('x');
    }
    assert_eq!(parse_inlines(&source), vec![Inline::Text(source)]);

    let source = format!("prefix {}end", "[x](".repeat(16_000));
    assert_eq!(parse_inlines(&source), vec![Inline::Text(source)]);
}

#[test]
fn exhausted_scans_roll_back_destinations_and_do_not_poison_later_paragraphs() {
    // Bracket matching succeeds before the repeated failed math scans. The
    // first link therefore has an accepted destination that must be revoked
    // when the outer paragraph falls back to its original literal source.
    let hostile = format!("[discard](/discard) {}end", "$value ".repeat(4096));
    let source = format!("{hostile}\n\n[good](/ok)\n\n{hostile}\n\n[good](/ok)");
    let parsed = parse_markdown(&source);
    assert_eq!(
        parsed.blocks,
        vec![
            Block::Paragraph(vec![Inline::Text(hostile.clone())]),
            Block::Paragraph(vec![link("good", "/ok")]),
            Block::Paragraph(vec![Inline::Text(hostile)]),
            Block::Paragraph(vec![link("good", "/ok")]),
        ]
    );
    assert_eq!(parse_markdown_profiled(&source).document, parsed);
    let spanned = parse_markdown_spanned(&source);
    assert_eq!(
        spanned
            .blocks
            .iter()
            .map(|block| &block.node)
            .collect::<Vec<_>>(),
        parsed.blocks.iter().collect::<Vec<_>>()
    );
    for block in &spanned.blocks {
        assert!(block.span.slice(&source).is_some());
    }
    let spans = destination_spans(&source);
    assert_eq!(spans.len(), 2);
    for span in spans {
        assert_eq!(span.dest, "/ok");
        assert_eq!(&source[span.range], "/ok");
    }
}

#[test]
fn bracket_preprocessing_uses_the_same_budget_as_inline_parsing() {
    let source = format!("[label {}end](/target)", "<?".repeat(12_000));
    assert_eq!(parse_inlines(&source), vec![Inline::Text(source.clone())]);
    assert!(destination_spans(&source).is_empty());
}

#[test]
fn ordinary_code_math_strike_links_and_raw_html_keep_their_structure() {
    for (source, expected) in [
        ("`literal`", vec![Inline::Code("literal".into())]),
        ("``a ` b``", vec![Inline::Code("a ` b".into())]),
        ("$x+1$", vec![Inline::Math("x+1".into())]),
        ("$$x+1$$", vec![Inline::DisplayMath("x+1".into())]),
        (
            "~~gone~~",
            vec![Inline::Strikethrough(vec![Inline::Text("gone".into())])],
        ),
        (
            "[**label**](/target)",
            vec![Inline::Link {
                dest: "/target".into(),
                title: None,
                content: vec![Inline::Strong(vec![Inline::Text("label".into())])],
            }],
        ),
        (
            "<a href=\"`\">`",
            vec![
                Inline::Html("<a href=\"`\">".into()),
                Inline::Text("`".into()),
            ],
        ),
        (
            "*<img src=\"foo\" title=\"*\"/>",
            vec![
                Inline::Text("*".into()),
                Inline::Html("<img src=\"foo\" title=\"*\"/>".into()),
            ],
        ),
        (
            "[foo <bar attr=\"](baz)\">",
            vec![
                Inline::Text("[foo ".into()),
                Inline::Html("<bar attr=\"](baz)\">".into()),
            ],
        ),
        (
            "<a href=\"foo  \nbar\">",
            vec![Inline::Html("<a href=\"foo  \nbar\">".into())],
        ),
        (
            "<a href=\"foo\\\nbar\">",
            vec![Inline::Html("<a href=\"foo\\\nbar\">".into())],
        ),
    ] {
        assert_eq!(parse_inlines(source), expected, "{source:?}");
    }
}

#[test]
fn many_valid_tokens_and_large_valid_html_attributes_do_not_exhaust_work() {
    let source = "[label](/target) `code` $x$ ~~gone~~ <b>bold</b> ".repeat(4096);
    let inlines = parse_inlines(&source);
    for predicate in [
        (|inline: &Inline| matches!(inline, Inline::Link { .. })) as fn(&Inline) -> bool,
        |inline| matches!(inline, Inline::Code(_)),
        |inline| matches!(inline, Inline::Math(_)),
        |inline| matches!(inline, Inline::Strikethrough(_)),
    ] {
        assert_eq!(
            inlines.iter().filter(|inline| predicate(inline)).count(),
            4096
        );
    }
    assert_eq!(
        inlines
            .iter()
            .filter(|inline| matches!(inline, Inline::Html(_)))
            .count(),
        8192
    );

    let tag = format!("<a title=\"{}\">", "a".repeat(128_000));
    assert_eq!(parse_inlines(&tag), vec![Inline::Html(tag)]);
}
