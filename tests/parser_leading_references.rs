//! Leading reference definitions are removed from an existing paragraph. They
//! do not grant its remaining source a second opportunity to start a block.
//!
//! The HTML expectations below were checked against CommonMark.js 0.31.2
//! (tag 0.31.2, cb2c2303d3550ec6ef28ceb2841f148e8761eebf), in addition to
//! CommonMark 0.31.2 examples 208, 215 and 216. Keep blank-separated controls:
//! removing the paragraph-boundary requirement would change literal code.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, reference_label_spans};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled,
};

fn document(source: &str) -> Document {
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    assert_eq!(
        parse_markdown_spanned_profiled(source)
            .document
            .to_document(),
        document
    );
    document
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&document(source).blocks, &HtmlOptions::default())
}

#[test]
fn indented_continuations_after_leading_definitions_keep_inline_markup() {
    for indent in ["    ", "        ", "\t", " \t"] {
        let source = format!("[r]: /target\n{indent}**visible** [r]\n");
        assert_eq!(
            fragment(&source),
            "<p><strong>visible</strong> <a href=\"/target\">r</a></p>\n",
            "{source:?}"
        );
    }
    for (source, expected) in [
        (
            "> [r]: /target\n>     **visible** [r]\n",
            "<blockquote>\n<p><strong>visible</strong> <a href=\"/target\">r</a></p>\n</blockquote>\n",
        ),
        (
            "- [r]: /target\n      **visible** [r]\n",
            "<ul>\n<li><strong>visible</strong> <a href=\"/target\">r</a></li>\n</ul>\n",
        ),
        (
            "> - [r]: /target\n>       **visible** [r]\n",
            "<blockquote>\n<ul>\n<li><strong>visible</strong> <a href=\"/target\">r</a></li>\n</ul>\n</blockquote>\n",
        ),
    ] {
        assert_eq!(fragment(source), expected, "{source:?}");
    }
}

#[test]
fn non_one_ordered_markers_after_definitions_cannot_restart_block_parsing() {
    for marker in ["2.", "9)", "0002.", "999999999."] {
        let source =
            format!("[r]: /real\n{marker} [visible](/literal)\n[fake]: /phantom\n\n[fake]\n");
        assert_eq!(
            fragment(&source),
            format!(
                "<p>{marker} <a href=\"/literal\">visible</a>\n[fake]: /phantom</p>\n<p>[fake]</p>\n"
            ),
            "{source:?}"
        );
        assert_eq!(
            destination_spans(&source)
                .iter()
                .map(|span| span.dest.as_str())
                .collect::<Vec<_>>(),
            ["/real", "/literal"]
        );
    }
}

#[test]
fn nested_paragraph_ownership_prevents_phantom_reference_destinations() {
    let source = "> > [r]: /real\n> >     **visible** [r]\n> [fake]: /phantom\n\n[fake]\n";
    assert_eq!(
        fragment(source),
        "<blockquote>\n<blockquote>\n<p><strong>visible</strong> <a href=\"/real\">r</a>\n[fake]: /phantom</p>\n</blockquote>\n</blockquote>\n<p>[fake]</p>\n"
    );
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/real");
    assert_eq!(reference_label_spans(source).len(), 2);
}

#[test]
fn consecutive_multiline_definitions_preserve_their_shared_paragraph() {
    let source = "[r]: /first\n  \"Title\"\n[s]:\n/second\n    **visible** [r] [s]\n";
    assert_eq!(
        fragment(source),
        "<p><strong>visible</strong> <a href=\"/first\" title=\"Title\">r</a> <a href=\"/second\">s</a></p>\n"
    );
    let source = "[r]: /first\n    first [r]\n    [r]: /phantom\n\n[r]: /later\n\n[r]\n";
    assert_eq!(
        fragment(source),
        "<p>first <a href=\"/first\">r</a>\n<a href=\"/first\">r</a>: /phantom</p>\n<p><a href=\"/first\">r</a></p>\n"
    );
    assert_eq!(
        destination_spans(source)
            .iter()
            .map(|span| span.dest.as_str())
            .collect::<Vec<_>>(),
        ["/first", "/later"]
    );
}

#[test]
fn indented_leading_definitions_and_titles_keep_reference_authority() {
    for indent in ["    ", "        ", "\t"] {
        let source = format!("[r]: /first\n{indent}\"Title\"\n{indent}[s]: /second\n\n[r] [s]\n");
        assert_eq!(
            fragment(&source),
            "<p><a href=\"/first\" title=\"Title\">r</a> <a href=\"/second\">s</a></p>\n"
        );
        let destinations = destination_spans(&source);
        assert_eq!(destinations.len(), 2);
        assert_eq!(&source[destinations[1].range.clone()], "/second");
    }
    for source in [
        "> [r]: /first\n    [s]: /second\n\n[s]\n",
        "> [r]: /first\n>     [s]: /second\n\n[s]\n",
    ] {
        assert_eq!(
            fragment(source),
            "<blockquote>\n</blockquote>\n<p><a href=\"/second\">s</a></p>\n"
        );
    }
}

#[test]
fn real_blank_boundaries_still_admit_code_and_ordered_lists() {
    let source = "[r]: /target\n\n    **literal** [r]\n";
    assert_eq!(
        document(source).blocks,
        [Block::CodeBlock {
            lang: None,
            code: "**literal** [r]\n".into(),
        }]
    );
    let source = "[r]: /target\n\n2. [r]\n";
    assert_eq!(
        fragment(source),
        "<ol start=\"2\">\n<li><a href=\"/target\">r</a></li>\n</ol>\n"
    );
    let source = "[r]: /target\n1. [r]\n";
    assert_eq!(
        fragment(source),
        "<ol>\n<li><a href=\"/target\">r</a></li>\n</ol>\n"
    );
}

#[test]
fn setext_heading_requires_visible_text_after_the_reference_prefix() {
    for (source, expected) in [
        ("[\nfoo\n]: /url\nbar\n", "<p>bar</p>\n"),
        (
            "[foo]: /url\nbar\n===\n[foo]\n",
            "<h1 id=\"bar\">bar</h1>\n<p><a href=\"/url\">foo</a></p>\n",
        ),
        (
            "[foo]: /url\n===\n[foo]\n",
            "<p>===\n<a href=\"/url\">foo</a></p>\n",
        ),
        (
            "[foo]: /url\n===\n[fake]: /phantom\n\n[fake]\n",
            "<p>===\n[fake]: /phantom</p>\n<p>[fake]</p>\n",
        ),
    ] {
        assert_eq!(fragment(source), expected, "{source:?}");
    }
}

#[test]
fn continued_paragraph_keeps_original_crlf_unicode_source_ranges() {
    let source =
        "\u{feff}[r]: /réal\r\n\t**café** [link](/café) [r]\r\n[fake]: /phantom\r\n\r\n[fake]\r\n";
    let parsed = document(source);
    assert_eq!(parsed.blocks.len(), 2);
    assert!(
        matches!(&parsed.blocks[0], Block::Paragraph(inlines) if matches!(inlines[0], Inline::Strong(_)))
    );
    let spanned = parse_markdown_spanned(source);
    assert_eq!(
        spanned.blocks[0].span.slice(source),
        Some("\t**café** [link](/café) [r]\r\n[fake]: /phantom")
    );
    assert_eq!(spanned.blocks[1].span.slice(source), Some("[fake]"));
    assert!(spanned.diagnostics.is_empty());
    assert_eq!(
        destination_spans(source)
            .iter()
            .map(|span| (&source[span.range.clone()], span.dest.as_str()))
            .collect::<Vec<_>>(),
        [("/réal", "/réal"), ("/café", "/café")]
    );
}
