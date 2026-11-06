//! Whitespace inside a link label is display content, not a paragraph ending.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{
    ReferenceLabelForm, destination_spans, parse_inlines, reference_label_spans,
};
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn emphasis(value: &str) -> Inline {
    Inline::Emphasis(vec![text(value)])
}

fn link(content: Vec<Inline>) -> Inline {
    Inline::Link {
        dest: "/target".into(),
        title: None,
        content,
    }
}

fn image(alt: &str) -> Inline {
    Inline::Image {
        dest: "/target".into(),
        title: None,
        alt: alt.into(),
    }
}

fn assert_paragraph(source: &str, expected: Vec<Inline>, expected_html: &str) {
    let document = parse_markdown(source);
    assert_eq!(
        document.blocks,
        vec![Block::Paragraph(expected)],
        "{source:?}"
    );
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    assert_eq!(
        franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default()),
        expected_html,
        "{source:?}"
    );
}

#[test]
fn inline_links_preserve_display_spaces_with_or_without_markup() {
    for (label, content, rendered) in [
        ("plain ", vec![text("plain ")], "plain "),
        (
            "*café*  ",
            vec![emphasis("café"), text("  ")],
            "<em>café</em>  ",
        ),
        ("entity &amp; ", vec![text("entity & ")], "entity &amp; "),
        ("escaped \\* ", vec![text("escaped * ")], "escaped * "),
        (
            "`code` ",
            vec![Inline::Code("code".into()), text(" ")],
            "<code>code</code> ",
        ),
        (
            "*café*\t ",
            vec![emphasis("café"), text("\t ")],
            "<em>café</em>\t ",
        ),
    ] {
        let source = format!("[{label}](/target)");
        let expected = vec![link(content)];
        assert_eq!(parse_inlines(&source), expected, "{source:?}");
        assert_paragraph(
            &source,
            expected,
            &format!("<p><a href=\"/target\">{rendered}</a></p>\n"),
        );
    }
}

#[test]
fn formatted_link_labels_do_not_merge_with_the_following_word() {
    assert_paragraph(
        "[*one* ](/target)two",
        vec![link(vec![emphasis("one"), text(" ")]), text("two")],
        "<p><a href=\"/target\"><em>one</em> </a>two</p>\n",
    );
    assert_paragraph(
        "[entity &amp; ](/target)next",
        vec![link(vec![text("entity & ")]), text("next")],
        "<p><a href=\"/target\">entity &amp; </a>next</p>\n",
    );
}

#[test]
fn images_preserve_formatted_and_nested_alt_text_spaces() {
    for (label, alt, rendered_alt) in [
        ("plain ", "plain ", "plain "),
        ("*café*  ", "café  ", "café  "),
        ("&amp;&#32; ", "&  ", "&amp;  "),
        ("[linked](/inside) ", "linked ", "linked "),
        ("![nested ](child.png) ", "nested  ", "nested  "),
    ] {
        let source = format!("![{label}](/target)");
        assert_paragraph(
            &source,
            vec![image(alt)],
            &format!("<p><img src=\"/target\" alt=\"{rendered_alt}\"></p>\n"),
        );
    }
}

#[test]
fn reference_lookup_normalization_preserves_link_and_image_display_spaces() {
    for (suffix, definition, form, token) in [
        ("[ref]", "ref", ReferenceLabelForm::Full, "ref"),
        ("[]", "*café*", ReferenceLabelForm::Collapsed, " *café*  "),
        ("", "*café*", ReferenceLabelForm::Shortcut, " *café*  "),
    ] {
        for is_image in [false, true] {
            let prefix = if is_image { "!" } else { "" };
            let source = format!("{prefix}[ *café*  ]{suffix}\n\n[{definition}]: /target\n");
            let (expected, rendered) = if is_image {
                (image(" café  "), r#"<img src="/target" alt=" café  ">"#)
            } else {
                (
                    link(vec![text(" "), emphasis("café"), text("  ")]),
                    "<a href=\"/target\"> <em>café</em>  </a>",
                )
            };
            assert_paragraph(&source, vec![expected], &format!("<p>{rendered}</p>\n"));
            let spans = reference_label_spans(&source);
            assert_eq!(spans.len(), 2, "{source:?}: {spans:?}");
            assert_eq!(spans[0].form, form);
            assert_eq!(&source[spans[0].label.clone().unwrap()], token);
            assert_eq!(spans[1].form, ReferenceLabelForm::Definition);
            assert_eq!(&source[spans[1].label.clone().unwrap()], definition);
        }
    }
}

#[test]
fn line_break_delimiters_remain_distinct_from_spaces_at_the_label_end() {
    for (separator, line_break, rendered_break) in [
        ("  \n  ", Inline::HardBreak, "<br>\n"),
        ("\\\n  ", Inline::HardBreak, "<br>\n"),
        ("\n  ", Inline::SoftBreak, "\n"),
    ] {
        let source = format!("[*first*{separator}last  ](/target)");
        assert_paragraph(
            &source,
            vec![link(vec![emphasis("first"), line_break, text("last  ")])],
            &format!("<p><a href=\"/target\"><em>first</em>{rendered_break}last  </a></p>\n"),
        );
    }
}

#[test]
fn paragraph_end_trimming_preserves_label_spaces_and_decoded_space_entities() {
    assert_paragraph(
        "[*styled* ](/target)  ",
        vec![link(vec![emphasis("styled"), text(" ")])],
        "<p><a href=\"/target\"><em>styled</em> </a></p>\n",
    );
    assert_paragraph(
        "[*styled* ](/target)&#32;  ",
        vec![link(vec![emphasis("styled"), text(" ")]), text(" ")],
        "<p><a href=\"/target\"><em>styled</em> </a> </p>\n",
    );
    assert_paragraph(
        "entity &amp;  ",
        vec![text("entity &")],
        "<p>entity &amp;</p>\n",
    );
}

#[test]
fn preserved_unicode_label_spaces_leave_editable_destinations_exact() {
    let source = "> [*café*  ](<docs/café.md#part>) ![**图**  ](图.png)\r\n>\r\n> [*réf*  ][ref]\r\n\r\n[ref]: <réf.md>\r\n";
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    let rendered =
        franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert!(rendered.contains("<em>café</em>  </a>"));
    assert!(rendered.contains("alt=\"图  \""));
    assert!(rendered.contains("<em>réf</em>  </a>"));
    let spans = destination_spans(source);
    assert_eq!(
        spans
            .iter()
            .map(|span| (&source[span.range.clone()], span.dest.as_str()))
            .collect::<Vec<_>>(),
        [
            ("<docs/café.md#part>", "docs/café.md#part"),
            ("图.png", "图.png"),
            ("<réf.md>", "réf.md"),
        ]
    );
}
