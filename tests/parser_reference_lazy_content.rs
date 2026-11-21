//! Removing invisible references must not reclassify admitted lazy prose as code.
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

fn inner_paragraph(mut blocks: &[Block]) -> &[Inline] {
    loop {
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        match &blocks[0] {
            Block::BlockQuote(inner) => blocks = inner,
            Block::List(list) => {
                assert_eq!(list.items.len(), 1);
                blocks = &list.items[0].blocks;
            }
            Block::Paragraph(inlines) => return inlines,
            other => panic!("admitted lazy prose became a block: {other:?}"),
        }
    }
}

fn link(label: &str, dest: &str) -> Inline {
    Inline::Link {
        dest: dest.into(),
        title: None,
        content: vec![Inline::Text(label.into())],
    }
}

#[test]
fn lazy_prose_after_a_hidden_definition_still_resolves_inline_markup() {
    for source in [
        "> [r]: /target\n    **visible** [r]\n",
        "> [r]: /target\n\t**visible** [r]\n",
        "1234. [r]: /target\n    **visible** [r]\n",
        "> - [r]: /target\n    **visible** [r]\n",
        "> > [r]: /target\n    **visible** [r]\n",
    ] {
        assert_eq!(
            inner_paragraph(&document(source).blocks),
            [
                Inline::Strong(vec![Inline::Text("visible".into())]),
                Inline::Text(" ".into()),
                link("r", "/target"),
            ],
            "{source:?}"
        );
    }
}

#[test]
fn lazy_setext_markers_do_not_become_code_after_reference_removal() {
    for opening in [">", "-", "1234.", "> -", "> >"] {
        for marker in ["=", "==", "==="] {
            let source = format!("{opening} [r]: /target\n{marker}\n");
            assert_eq!(
                inner_paragraph(&document(&source).blocks),
                [Inline::Text(marker.into())],
                "{source:?}"
            );
        }
    }
}

#[test]
fn preserved_lazy_markers_cannot_define_phantom_reference_targets() {
    for (opening, continuation) in [
        (">", "> "),
        ("1234.", "      "),
        ("> -", ">   "),
        ("> >", "> > "),
    ] {
        for marker in ["# heading", "~~~", "===", "> quote"] {
            let source = format!(
                "{opening} [r]: /real\n    {marker}\n{continuation}[fake]: /phantom\n\n[fake]\n"
            );
            let parsed = document(&source);
            assert_eq!(parsed.blocks.len(), 2, "{source:?}: {parsed:?}");
            assert_eq!(
                inner_paragraph(&parsed.blocks[..1]),
                [
                    Inline::Text(marker.into()),
                    Inline::SoftBreak,
                    Inline::Text("[fake]: /phantom".into()),
                ],
                "{source:?}"
            );
            assert_eq!(
                parsed.blocks[1],
                Block::Paragraph(vec![Inline::Text("[fake]".into())]),
                "{source:?}"
            );
            let destinations = destination_spans(&source);
            assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
            assert_eq!(destinations[0].dest, "/real");
            assert_eq!(reference_label_spans(&source).len(), 1);
        }
    }
}

#[test]
fn lazy_footnote_lookalikes_stay_inline_content_after_reference_removal() {
    let source = "> [r]: /real\n    [^note]: text\n> [fake]: /phantom\n\n[fake]\n";
    let parsed = document(source);
    assert_eq!(
        inner_paragraph(&parsed.blocks[..1]),
        [
            Inline::FootnoteRef { id: "note".into() },
            Inline::Text(": text".into()),
            Inline::SoftBreak,
            Inline::Text("[fake]: /phantom".into()),
        ]
    );
    assert_eq!(
        parsed.blocks[1],
        Block::Paragraph(vec![Inline::Text("[fake]".into())])
    );
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/real");
}

#[test]
fn only_a_real_blank_boundary_allows_code_after_a_reference_prefix() {
    for (source, code) in [
        (
            "> [r]: /real\n>     [hidden](/literal)\n> [fake]: /actual\n\n[fake]\n",
            false,
        ),
        (
            "- [r]: /real\n      [hidden](/literal)\n  [fake]: /actual\n\n[fake]\n",
            false,
        ),
        (
            "> [r]: /real\n>\n>     [hidden](/literal)\n> [fake]: /actual\n\n[fake]\n",
            true,
        ),
        (
            "- [r]: /real\n\n      [hidden](/literal)\n  [fake]: /actual\n\n[fake]\n",
            true,
        ),
    ] {
        let parsed = document(source);
        let blocks = match &parsed.blocks[0] {
            Block::BlockQuote(blocks) => blocks,
            Block::List(list) => &list.items[0].blocks,
            other => panic!("expected container: {other:?}"),
        };
        let expected = if code {
            Block::CodeBlock {
                lang: None,
                code: "[hidden](/literal)\n".into(),
            }
        } else {
            Block::Paragraph(vec![
                link("hidden", "/literal"),
                Inline::SoftBreak,
                Inline::Text("[fake]: /actual".into()),
            ])
        };
        assert_eq!(blocks, &[expected], "{source:?}");
        assert_eq!(
            parsed.blocks[1],
            Block::Paragraph(vec![if code {
                link("fake", "/actual")
            } else {
                Inline::Text("[fake]".into())
            }])
        );
        let destinations = destination_spans(source);
        assert_eq!(
            destinations
                .iter()
                .map(|span| span.dest.as_str())
                .collect::<Vec<_>>(),
            ["/real", if code { "/actual" } else { "/literal" }]
        );
    }
}

#[test]
fn lazy_reference_content_keeps_original_unicode_crlf_and_destination_ranges() {
    let source =
        "\u{feff}> - [r]: /réal\r\n\t# [visible](/café)\r\n>   [fake]: /phantom\r\n\r\n[fake]\r\n";
    let parsed = document(source);
    assert_eq!(
        inner_paragraph(&parsed.blocks[..1]),
        [
            Inline::Text("# ".into()),
            link("visible", "/café"),
            Inline::SoftBreak,
            Inline::Text("[fake]: /phantom".into()),
        ]
    );
    let spanned = parse_markdown_spanned(source);
    assert_eq!(
        spanned.blocks[0].span.slice(source),
        Some("> - [r]: /réal\r\n\t# [visible](/café)\r\n>   [fake]: /phantom")
    );
    assert_eq!(spanned.blocks[1].span.slice(source), Some("[fake]"));
    assert!(spanned.diagnostics.is_empty());
    let destinations = destination_spans(source);
    assert_eq!(
        destinations
            .iter()
            .map(|span| (&source[span.range.clone()], span.dest.as_str()))
            .collect::<Vec<_>>(),
        [("/réal", "/réal"), ("/café", "/café")]
    );
    let html = franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
    assert!(html.contains("# <a href=\"/caf%C3%A9\">visible</a>\n[fake]: /phantom"));
    assert!(html.ends_with("<p>[fake]</p>\n"));
}
