//! Strikethrough shares inline-token boundaries with emphasis and links.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    HtmlOptions, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}

fn strike(content: Vec<Inline>) -> Inline {
    Inline::Strikethrough(content)
}

fn link(label: &str, destination: &str) -> Inline {
    Inline::Link {
        dest: destination.to_owned(),
        title: None,
        content: vec![text(label)],
    }
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn code_escapes_entities_and_math_cannot_close_a_strike() {
    for (source, content) in [
        (
            "~~use `~~` literally~~",
            vec![text("use "), Inline::Code("~~".into()), text(" literally")],
        ),
        (
            "~~use ``a ` ~~ b`` literally~~",
            vec![
                text("use "),
                Inline::Code("a ` ~~ b".into()),
                text(" literally"),
            ],
        ),
        (r"~~a \~~ b~~", vec![text("a ~~ b")]),
        ("~~a &#126;&#126; b~~", vec![text("a ~~ b")]),
        ("~~word&#32;~~", vec![text("word ")]),
        (
            "~~formula $x~~y$ end~~",
            vec![text("formula "), Inline::Math("x~~y".into()), text(" end")],
        ),
    ] {
        assert_eq!(parse_inlines(source), vec![strike(content)], "{source:?}");
    }
    assert_eq!(
        fragment("~~use `~~` literally~~"),
        "<p><del>use <code>~~</code> literally</del></p>\n"
    );
}

#[test]
fn links_and_images_keep_their_complete_destinations_and_titles() {
    let source = "~~[label](/x~~y \"a ~~ title\") tail~~";
    assert_eq!(
        parse_inlines(source),
        vec![strike(vec![
            Inline::Link {
                dest: "/x~~y".into(),
                title: Some("a ~~ title".into()),
                content: vec![text("label")],
            },
            text(" tail"),
        ])],
    );
    assert_eq!(
        fragment(source),
        "<p><del><a href=\"/x~~y\" title=\"a ~~ title\">label</a> tail</del></p>\n",
    );
    assert_eq!(
        parse_inlines("~~![alt~~text](image~~name.png) end~~"),
        vec![strike(vec![
            Inline::Image {
                dest: "image~~name.png".into(),
                title: None,
                alt: "alt~~text".into(),
            },
            text(" end"),
        ])],
    );
}

#[test]
fn a_label_boundary_prevents_crossing_strike_delimiters() {
    assert_eq!(
        parse_inlines("~~[label~~](/target)"),
        vec![text("~~"), link("label~~", "/target")],
    );
    assert_eq!(
        parse_inlines("[~~label](/target)~~"),
        vec![link("~~label", "/target"), text("~~")],
    );
    assert_eq!(
        fragment("~~[label~~](/target)"),
        "<p>~~<a href=\"/target\">label~~</a></p>\n",
    );
    let source = "[ref]: /target~~value\n\n~~[label][ref] end~~";
    assert_eq!(
        parse_markdown(source).blocks,
        vec![Block::Paragraph(vec![strike(vec![
            link("label", "/target~~value"),
            text(" end"),
        ])])],
    );
}

#[test]
fn raw_html_attributes_and_angle_autolinks_are_opaque() {
    let tag = "<a href=\"/target~~value\" title=\"~~\">";
    let source = format!("~~{tag}label</a>~~");
    assert_eq!(
        parse_inlines(&source),
        vec![strike(vec![
            Inline::Html(tag.into()),
            text("label"),
            Inline::Html("</a>".into())
        ])],
    );
    assert_eq!(
        parse_inlines("~~<https://example.test/a~~b> end~~"),
        vec![strike(vec![
            link("https://example.test/a~~b", "https://example.test/a~~b"),
            text(" end")
        ])],
    );
    let untrusted = fragment("~~[unsafe](javascript:alert(1)) <script>alert(1)</script>~~");
    assert!(!untrusted.contains("href=\"javascript:"));
    assert!(!untrusted.contains("<script>"));
}

#[test]
fn nested_and_overlapping_emphasis_uses_one_delimiter_stack() {
    for (source, expected) in [
        ("~~a ~~b~~ c~~", "<p><del>a <del>b</del> c</del></p>\n"),
        (
            "~~a **b** c~~",
            "<p><del>a <strong>b</strong> c</del></p>\n",
        ),
        (
            "**a ~~b~~ c**",
            "<p><strong>a <del>b</del> c</strong></p>\n",
        ),
        (
            "*before ~~middle* after~~",
            "<p><em>before ~~middle</em> after~~</p>\n",
        ),
        (
            "~~before *middle~~ after*",
            "<p><del>before *middle</del> after*</p>\n",
        ),
    ] {
        assert_eq!(fragment(source), expected, "{source:?}");
    }
}

#[test]
fn strike_requires_complete_double_tilde_runs_and_nonwhitespace_content() {
    for source in [
        "~~~word~~~",
        "~~~~word~~~~",
        "~~\tword~~",
        "~~word\t~~",
        "~~\nword~~",
        "~~word\n~~",
        "~~\u{00a0}word~~",
        "~~word\u{00a0}~~",
    ] {
        assert!(
            !parse_inlines(source)
                .iter()
                .any(|node| matches!(node, Inline::Strikethrough(_))),
            "{source:?}"
        );
    }
    assert_eq!(
        parse_inlines("a~~b~~c"),
        vec![text("a"), strike(vec![text("b")]), text("c")]
    );
    assert_eq!(
        fragment("This will ~~~not~~~ strike."),
        "<p>This will ~~~not~~~ strike.</p>\n"
    );
}

#[test]
fn bare_urls_and_email_addresses_survive_strikethrough_tokenization() {
    for (label, destination) in [
        ("https://example.test", "https://example.test"),
        ("www.example.test", "http://www.example.test"),
        ("support@example.test", "mailto:support@example.test"),
    ] {
        for prefix in ["", "see "] {
            let source = format!("~~{prefix}{label}~~");
            let mut content = Vec::new();
            if !prefix.is_empty() {
                content.push(text(prefix));
            }
            content.push(link(label, destination));
            assert_eq!(parse_inlines(&source), vec![strike(content)], "{source:?}");
        }
    }
    assert_eq!(
        parse_inlines("~~https://example.test/a~~~b~~ https://outside.test/a~~b"),
        vec![
            strike(vec![link(
                "https://example.test/a~~~b",
                "https://example.test/a~~~b"
            )]),
            text(" "),
            link("https://outside.test/a~~b", "https://outside.test/a~~b")
        ],
    );
}

#[test]
fn consumed_openers_and_decoded_text_cannot_rewrite_url_boundaries() {
    assert_eq!(
        parse_inlines("*~~old* https://example.test/a~~b"),
        vec![
            Inline::Emphasis(vec![text("~~old")]),
            text(" "),
            link("https://example.test/a~~b", "https://example.test/a~~b")
        ],
    );
    for source in ["~~&#104;ttps://example.test~~", "~~www&#46;example.test~~"] {
        assert!(
            matches!(&parse_inlines(source)[..], [Inline::Strikethrough(content)] if matches!(&content[..], [Inline::Text(_)])),
            "{source:?}"
        );
    }
    assert_eq!(
        fragment("[~~https://label.test~~](/chosen)"),
        "<p><a href=\"/chosen\"><del>https://label.test</del></a></p>\n",
    );
}

#[test]
fn destination_authority_and_all_parse_entrypoints_agree() {
    let source = "## ~~use `~~` literally~~\n\n> ~~é [link](/target~~path#h) `[/wrong](/ignored)`~~\n\n- ~~[label~~](/other)\n\n| value |\n| --- |\n| ~~`~~`~~ |";
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), document);
    assert!(
        spanned
            .blocks
            .iter()
            .all(|block| block.span.slice(source).is_some())
    );
    let spans = destination_spans(source);
    assert_eq!(
        spans
            .iter()
            .map(|span| span.dest.as_str())
            .collect::<Vec<_>>(),
        ["/target~~path#h", "/other"]
    );
    for span in spans {
        assert_eq!(&source[span.range], span.dest);
    }
}

#[test]
fn long_runs_and_many_struck_urls_remain_bounded_and_complete() {
    let source = format!("prefix {} suffix", "~".repeat(100_000));
    assert_eq!(parse_inlines(&source), vec![text(&source)]);
    let source = "~~https://example.test~~ ".repeat(2048);
    let inlines = parse_inlines(&source);
    assert_eq!(
        inlines
            .iter()
            .filter(|node| matches!(node, Inline::Strikethrough(_)))
            .count(),
        2048
    );
    for node in &inlines {
        if let Inline::Strikethrough(content) = node {
            assert_eq!(
                content,
                &[link("https://example.test", "https://example.test")]
            );
        }
    }
}

#[test]
fn formatted_autolinks_keep_interior_delimiters_and_trim_only_suffixes() {
    for (source, expected) in [
        (
            "~~https://example.test/a~~b~~",
            strike(vec![link(
                "https://example.test/a~~b",
                "https://example.test/a~~b",
            )]),
        ),
        (
            "**https://example.test/a*b**",
            Inline::Strong(vec![link(
                "https://example.test/a*b",
                "https://example.test/a*b",
            )]),
        ),
        (
            "_https://example.test/a_b_",
            Inline::Emphasis(vec![link(
                "https://example.test/a_b",
                "https://example.test/a_b",
            )]),
        ),
    ] {
        assert_eq!(parse_inlines(source), vec![expected], "{source:?}");
    }
    assert_eq!(
        parse_inlines("(~~https://example.test/a(b)~~)"),
        vec![
            text("("),
            strike(vec![link(
                "https://example.test/a(b)",
                "https://example.test/a(b)"
            )]),
            text(")")
        ],
    );
    // GFM's URL suffix rule does not depend on having a matching opener.
    assert_eq!(
        parse_inlines("https://example.test/a~~b~~"),
        vec![
            link("https://example.test/a~~b", "https://example.test/a~~b"),
            text("~~")
        ]
    );
    let suffix = ")~".repeat(20_000);
    let source = format!("https://example.test/a(b){suffix}");
    assert_eq!(
        parse_inlines(&source),
        vec![
            link("https://example.test/a(b)", "https://example.test/a(b)"),
            text(&suffix)
        ]
    );
}
