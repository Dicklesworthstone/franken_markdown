//! HTML block ownership must agree with reference collection and containers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn raw_fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(
        &parse_markdown(source).blocks,
        &HtmlOptions {
            allow_raw_html: true,
            ..HtmlOptions::default()
        },
    )
}

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.into())])
}

fn heading(text: &str) -> Block {
    Block::Heading {
        level: 1,
        inlines: vec![Inline::Text(text.into())],
    }
}

fn first_html(blocks: &[Block]) -> Option<&str> {
    for block in blocks {
        let found = match block {
            Block::HtmlBlock(raw) => Some(raw.as_str()),
            Block::BlockQuote(children)
            | Block::FootnoteDefinition {
                blocks: children, ..
            } => first_html(children),
            Block::List(list) => list.items.iter().find_map(|item| first_html(&item.blocks)),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

#[test]
fn complete_type_seven_tags_own_literal_markdown_until_a_blank_line() {
    for opener in [
        "<span>",
        "<custom-element>",
        "<widget disabled _x :ns.x-y=ok>",
        "<widget data-x = 'café > < β' title=\"quoted\"/>",
        "<widget src=path/to/file/>",
        "   <WIDGET-2\tattr=''>\t ",
        "</widget >",
        "</PRE>",
        "</script>",
    ] {
        let raw = format!(
            "{opener}\n# Literal heading\n- literal item\n```md\n[hidden]: /secret\n```\n</span>"
        );
        let source = format!("{raw}\n\n# Actual heading\n\n[hidden]\n");
        let document = parse_markdown(&source);
        assert_eq!(
            document.blocks,
            vec![
                Block::HtmlBlock(raw.clone()),
                heading("Actual heading"),
                paragraph("[hidden]")
            ],
            "{opener:?}"
        );
        assert!(destination_spans(&source).is_empty(), "{opener:?}");
        assert_eq!(parse_markdown_profiled(&source).document, document);
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
        assert!(raw_fragment(&source).starts_with(&format!("{raw}\n")));
    }
}

#[test]
fn type_seven_does_not_interrupt_or_close_an_open_paragraph() {
    for tag in ["<span>", "<custom a=x>", "</span>", "</pre>"] {
        let source = format!("Paragraph\n{tag}\n[hidden]: /secret\ncontinued\n\n[hidden]");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{tag}");
        assert!(matches!(document.blocks[0], Block::Paragraph(_)), "{tag}");
        assert_eq!(document.blocks[1], paragraph("[hidden]"));
        assert!(destination_spans(&source).is_empty(), "{tag}");
        assert_eq!(
            raw_fragment(&source),
            format!("<p>Paragraph\n{tag}\n[hidden]: /secret\ncontinued</p>\n<p>[hidden]</p>\n"),
            "{tag}"
        );

        let interrupted = format!("Paragraph\n{tag}\n# Actual heading\n");
        assert_eq!(
            parse_markdown(&interrupted).blocks[1],
            heading("Actual heading")
        );
    }
}

#[test]
fn type_seven_starts_after_completed_blocks_and_reference_definitions() {
    for prefix in [
        "# Heading\n",
        "```text\ncode\n```\n",
        "---\n",
        "[outside]: /accepted\n",
        "A | B\n--- | ---\nx | y\n",
    ] {
        let source =
            format!("{prefix}<span>\n# Literal heading\n[hidden]: /secret\n</span>\n\n[hidden]");
        let document = parse_markdown(&source);
        assert_eq!(
            first_html(&document.blocks),
            Some("<span>\n# Literal heading\n[hidden]: /secret\n</span>"),
            "{prefix:?}"
        );
        assert_eq!(document.blocks.last(), Some(&paragraph("[hidden]")));
        assert!(
            destination_spans(&source)
                .iter()
                .all(|span| span.dest != "/secret")
        );
    }
}

#[test]
fn quoted_listed_and_footnote_html_cannot_define_hidden_references() {
    for source in [
        "> <span>\n> # Literal heading\n> [hidden]: /secret\n> </span>\n[hidden]\n\n[hidden]: /outside",
        "- <span>\n  # Literal heading\n  [hidden]: /secret\n  </span>\n[hidden]\n\n[hidden]: /outside",
        "- > <span>\n  > # Literal heading\n  > [hidden]: /secret\n  > </span>\n[hidden]\n\n[hidden]: /outside",
        "[^n]: <span>\n    # Literal heading\n    [hidden]: /secret\n    </span>\n\n[hidden] [^n]\n\n[hidden]: /outside",
    ] {
        let document = parse_markdown(source);
        assert_eq!(
            first_html(&document.blocks),
            Some("<span>\n# Literal heading\n[hidden]: /secret\n</span>"),
            "{source}"
        );
        let spans = destination_spans(source);
        assert_eq!(spans.len(), 1, "{source}");
        assert_eq!(spans[0].dest, "/outside");
        assert_eq!(&source[spans[0].range.clone()], "/outside");
        assert!(raw_fragment(source).contains("<a href=\"/outside\">hidden</a>"));
        assert_eq!(parse_markdown_spanned(source).to_document(), document);
    }
}

#[test]
fn lazy_container_paragraphs_keep_type_seven_inline_and_definitions_literal() {
    for source in [
        "> Paragraph\n<span>\n[hidden]: /secret\ncontinued\n\n[hidden]",
        "- Paragraph\n<span>\n[hidden]: /secret\ncontinued\n\n[hidden]",
        "- > Paragraph\n<span>\n[hidden]: /secret\ncontinued\n\n[hidden]",
    ] {
        let document = parse_markdown(source);
        assert!(first_html(&document.blocks).is_none(), "{source}");
        assert_eq!(document.blocks.len(), 2, "{source}");
        assert_eq!(document.blocks[1], paragraph("[hidden]"));
        assert!(destination_spans(source).is_empty(), "{source}");
        assert!(raw_fragment(source).contains("Paragraph\n<span>\n[hidden]: /secret\ncontinued"));
    }
}

#[test]
fn literal_html_does_not_capture_unmarked_or_dedented_container_followers() {
    for source in [
        "> <span>\n> # Literal heading\n[real]: /outside\n\n[real]",
        "- <span>\n  # Literal heading\n[real]: /outside\n\n[real]",
        "> - <span>\n>   # Literal heading\n[real]: /outside\n\n[real]",
    ] {
        let document = parse_markdown(source);
        assert_eq!(document.blocks.len(), 2, "{source}");
        assert_eq!(
            first_html(&document.blocks),
            Some("<span>\n# Literal heading")
        );
        assert_eq!(destination_spans(source)[0].dest, "/outside");
        assert!(raw_fragment(source).contains("<p><a href=\"/outside\">real</a></p>"));
    }
}

#[test]
fn malformed_or_incomplete_tags_do_not_swallow_following_markdown() {
    for line in [
        "<widget title='unfinished>",
        "<widget a='x'b='y'>",
        "<widget a= >",
        "<widget a=`x`>",
        "<widget a=x=y>",
        "<widget/ >",
        "</widget a=x>",
        "<widget> trailing",
        "<widget><other>",
        "<widget\u{a0}a=x>",
        "<widget>\u{a0}",
        "<div.foo>",
        "<div/invalid>",
        "<div❤>",
        "<script/foo>",
        "<script\u{a0}type=x>",
        "<pre/>",
        "<style/>",
        "<textarea/>",
        "<!",
        "<!3invalid>",
    ] {
        let source = format!("{line}\n# Actual heading\n[real]: /outside\n\n[real]");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 3, "{line:?}: {:?}", document.blocks);
        assert!(
            matches!(document.blocks[0], Block::Paragraph(_)),
            "{line:?}"
        );
        assert_eq!(document.blocks[1], heading("Actual heading"), "{line:?}");
        assert_eq!(destination_spans(&source)[0].dest, "/outside", "{line:?}");
    }
}

#[test]
fn type_six_names_require_a_boundary_and_search_can_interrupt_paragraphs() {
    for line in [
        "<search",
        "</SEARCH>",
        "<div/>",
        "<div/><!--comment-->",
        "<table\tattr=",
    ] {
        let source = format!("Paragraph\n{line}\n# Literal heading\n[hidden]: /secret\n\n[hidden]");
        assert_eq!(
            parse_markdown(&source).blocks,
            vec![
                paragraph("Paragraph"),
                Block::HtmlBlock(format!("{line}\n# Literal heading\n[hidden]: /secret")),
                paragraph("[hidden]")
            ],
            "{line:?}"
        );
        assert!(destination_spans(&source).is_empty());
    }
}

#[test]
fn every_raw_text_block_ends_at_any_of_the_four_raw_text_closers() {
    for opener in ["<pre>", "<SCRIPT type=x>", "<style", "<textarea\t"] {
        for closer in ["</pre>", "</SCRIPT>", "</style>", "</textarea>"] {
            let raw = format!("{opener}\n\n# Literal heading\n[hidden]: /secret\n{closer}");
            let source =
                format!("Paragraph\n{raw}\n# Actual heading\n[real]: /outside\n\n[hidden] [real]");
            let document = parse_markdown(&source);
            assert_eq!(document.blocks.len(), 4, "{opener} / {closer}");
            assert_eq!(document.blocks[0], paragraph("Paragraph"));
            assert_eq!(document.blocks[1], Block::HtmlBlock(raw));
            assert_eq!(document.blocks[2], heading("Actual heading"));
            let spans = destination_spans(&source);
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].dest, "/outside");
        }
    }
}

#[test]
fn comments_declarations_processing_and_cdata_keep_their_own_end_markers() {
    for (opener, closer) in [
        ("<!--", "-->"),
        ("<?target", "?>"),
        ("<!DOCTYPE", ">"),
        ("<!lowercase", ">"),
        ("<![CDATA[", "]]>"),
    ] {
        let raw = format!("{opener}\n\n# Literal heading\n[hidden]: /secret\n{closer}");
        let source = format!("Paragraph\n{raw}\n# Actual heading\n\n[hidden]");
        assert_eq!(
            parse_markdown(&source).blocks,
            vec![
                paragraph("Paragraph"),
                Block::HtmlBlock(raw),
                heading("Actual heading"),
                paragraph("[hidden]")
            ],
            "{opener}"
        );
        assert!(destination_spans(&source).is_empty());
    }
}

#[test]
fn one_line_html_block_spans_and_raw_emission_have_no_paragraph_wrapper() {
    for raw in [
        r#"<a href="/bar\/)">"#,
        r#"<a href="&ouml;&ouml;.html">"#,
        "</custom>",
    ] {
        let source = format!("{raw}\n");
        let document = parse_markdown_spanned(&source);
        assert_eq!(document.blocks.len(), 1);
        assert_eq!(document.blocks[0].node, Block::HtmlBlock(raw.into()));
        assert_eq!(document.blocks[0].span.slice(&source), Some(raw));
        assert_eq!(raw_fragment(&source), source);
    }
}

#[test]
fn unicode_crlf_and_container_spans_cover_the_exact_owned_source() {
    // List source spans include the separator consumed by list collection,
    // even when their contents are ordinary text. HTML leaves and quotes stop
    // at their last nonblank line. Require those exact original byte ranges.
    let plain_list = "- plain\r\n\r\n[real]: /outside\r\n\r\n[real]";
    assert_eq!(
        parse_markdown_spanned(plain_list).blocks[0]
            .span
            .slice(plain_list),
        Some("- plain\r\n")
    );
    for (raw, trailing_separator) in [
        (
            "   <span title='café β'>\r\n# Literal heading\r\n[hidden]: /秘密\r\n</span>",
            "",
        ),
        (
            "> <span title='café β'>\r\n> # Literal heading\r\n> [hidden]: /秘密\r\n> </span>",
            "",
        ),
        (
            "- <span title='café β'>\r\n  # Literal heading\r\n  [hidden]: /秘密\r\n  </span>",
            "\r\n",
        ),
    ] {
        let source = format!("\u{feff}{raw}\r\n\r\n[real]: </réel#β>\r\n\r\n[real] [hidden]\r\n");
        let document = parse_markdown_spanned(&source);
        assert_eq!(document.blocks.len(), 2, "{raw}");
        let expected = format!("{raw}{trailing_separator}");
        assert_eq!(
            document.blocks[0].span.slice(&source),
            Some(expected.as_str())
        );
        assert_eq!(
            document.blocks[1].span.slice(&source),
            Some("[real] [hidden]")
        );
        assert_eq!(document.to_document(), parse_markdown(&source));
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1);
        assert_eq!(&source[spans[0].range.clone()], "</réel#β>");
        assert_eq!(&source[spans[0].fragment_range(&source).unwrap()], "β");
    }
}

#[test]
fn sanitizer_policy_still_applies_to_new_html_blocks_without_parsing_their_text() {
    let source = "<span onclick='evil()'>\n# Literal heading\n[hidden]: /secret\n<a href='javascript:evil()'>visible</a>\n</span>\n\n[hidden]";
    let document = parse_markdown(source);
    let safe = franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert!(safe.contains("# Literal heading"), "{safe}");
    assert!(safe.contains("[hidden]: /secret"), "{safe}");
    assert!(!safe.contains("onclick"), "{safe}");
    assert!(!safe.contains("javascript:"), "{safe}");
    assert!(!safe.contains("<h1"), "{safe}");
    assert!(!safe.contains("href=\"/secret\""), "{safe}");
    assert_eq!(document.blocks.last(), Some(&paragraph("[hidden]")));
}

#[test]
fn long_attribute_lists_and_unfinished_values_preserve_source_without_a_line_cap() {
    let attributes = " data-x='é > / β' disabled".repeat(4096);
    let opener = format!("<widget{attributes}>");
    let raw = format!("{opener}\n# Literal heading\n[hidden]: /secret");
    let source = format!("{raw}\n\n[hidden]");
    assert_eq!(
        parse_markdown(&source).blocks,
        vec![Block::HtmlBlock(raw), paragraph("[hidden]")]
    );
    assert!(destination_spans(&source).is_empty());

    let malformed = format!("<widget data-x='{}\n# Actual heading", "é".repeat(65_536));
    let document = parse_markdown(&malformed);
    assert_eq!(document.blocks.len(), 2);
    assert_eq!(document.blocks[1], heading("Actual heading"));
    assert_eq!(
        parse_markdown_spanned(&malformed).blocks[0]
            .span
            .slice(&malformed),
        malformed.lines().next()
    );
}

#[test]
fn reference_label_edits_exclude_type_seven_html_and_preserve_visible_text() {
    use franken_markdown::parse::{ReferenceLabelForm as Form, reference_label_spans};

    let prefix = "\u{feff}[Straße]: /target\r\n\r\n";
    let literal = "<span>\r\n# Literal heading\r\n[shown][STRASSE] [Straße][] [Straße] ![alt][strasse]\r\n[Straße]: /shadow\r\n[hidden]: /secret\r\n</span>";
    let visible = "[visible][STRASSE] [Straße][] [Straße] ![alt][strasse] [hidden]";
    for literal in [
        literal.to_string(),
        format!("> {}", literal.replace("\r\n", "\r\n> ")),
        format!("- {}", literal.replace("\r\n", "\r\n  ")),
    ] {
        let source = format!("{prefix}{literal}\r\n\r\n{visible}\r\n");
        let visible_start = prefix.len() + literal.len() + "\r\n\r\n".len();
        let definition_start = prefix.find("Straße").unwrap();
        let definition = definition_start..definition_start + "Straße".len();
        let full_start = visible_start + visible.find("STRASSE").unwrap();
        let full = full_start..full_start + "STRASSE".len();
        let collapsed_start = visible_start + visible.find("[Straße][]").unwrap() + 1;
        let collapsed = collapsed_start..collapsed_start + "Straße".len();
        let empty_start = visible_start + visible.find("[]").unwrap();
        let shortcut_start = visible_start + visible.find("[Straße] !").unwrap() + 1;
        let shortcut = shortcut_start..shortcut_start + "Straße".len();
        let insertion = shortcut.end + 1;
        let image_start = visible_start + visible.find("strasse").unwrap();
        let image = image_start..image_start + "strasse".len();

        let spans = reference_label_spans(&source);
        assert_eq!(
            spans
                .iter()
                .map(|span| (span.form, span.label.clone(), span.replacement.clone()))
                .collect::<Vec<_>>(),
            vec![
                (Form::Definition, Some(definition.clone()), Some(definition)),
                (Form::Full, Some(full.clone()), Some(full)),
                (
                    Form::Collapsed,
                    Some(collapsed),
                    Some(empty_start..empty_start + 2)
                ),
                (Form::Shortcut, Some(shortcut), Some(insertion..insertion)),
                (Form::Full, Some(image.clone()), Some(image)),
            ],
            "literal HTML cannot supply definitions or editable reference uses: {source}"
        );
        assert!(spans.iter().all(|span| span.key == "strasse"));

        // Apply the public edit forms in source order from right to left. The
        // visible link/image labels, destinations and literal HTML must survive.
        let document = parse_markdown(&source);
        let mut renamed = source.clone();
        for span in spans.iter().rev() {
            let replacement = match span.form {
                Form::Definition | Form::Full => "renamed",
                Form::Collapsed | Form::Shortcut => "[renamed]",
            };
            renamed.replace_range(span.replacement.clone().unwrap(), replacement);
        }
        assert_eq!(parse_markdown(&renamed), document);
        assert!(renamed.contains(&literal));
    }
}
