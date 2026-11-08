//! Authoring dialects own source syntax before inline parsing and source edits.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{
    ReferenceLabelForm, destination_spans, destination_spans_with_profile,
    reference_label_spans_with_profile,
};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, PdfASettings, PdfEmitOptions, PdfOptions, Profile,
    parse_markdown, parse_markdown_profiled_with_profile,
    parse_markdown_spanned_profiled_with_profile, parse_markdown_spanned_with_profile,
    parse_markdown_with_profile, render_html, render_pdf, render_pdf_document, render_pdf_emitted,
    render_pdf_pdfa, render_pdf_reported,
};

const COMMONMARK: Option<Profile> = Some(Profile::CommonMarkGfm);

fn paragraph(lines: &[&str]) -> Block {
    let mut inlines = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            inlines.push(Inline::SoftBreak);
        }
        inlines.push(Inline::Text((*line).to_owned()));
    }
    Block::Paragraph(inlines)
}

#[test]
fn commonmark_keeps_colon_continuations_in_the_original_paragraph() {
    let source = "First\n: value one\nSecond\n: value two\n";
    let document = parse_markdown_with_profile(source, COMMONMARK);
    assert_eq!(
        document.blocks,
        [paragraph(&[
            "First",
            ": value one",
            "Second",
            ": value two"
        ])]
    );
    let options = HtmlOptions {
        profile: COMMONMARK,
        ..HtmlOptions::default()
    };
    assert!(
        render_html(source, &options)
            .unwrap()
            .contains("<p>First\n: value one\nSecond\n: value two</p>")
    );
}

#[test]
fn commonmark_resolves_inline_delimiters_across_colon_lines() {
    let options = HtmlOptions {
        profile: COMMONMARK,
        ..HtmlOptions::default()
    };
    for (source, expected) in [
        (
            "*Term\n: definition*\n",
            "<p><em>Term\n: definition</em></p>",
        ),
        (
            "**Term\n: definition**\n",
            "<p><strong>Term\n: definition</strong></p>",
        ),
        (
            "`Term\n: definition`\n",
            "<p><code>Term : definition</code></p>",
        ),
        (
            "[Term\n: definition](/target)\n",
            "<p><a href=\"/target\">Term\n: definition</a></p>",
        ),
        (
            "[Term\n: definition][target]\n\n[target]: /target\n",
            "<p><a href=\"/target\">Term\n: definition</a></p>",
        ),
        (
            "[Term\n: definition][]\n\n[Term : definition]: /target\n",
            "<p><a href=\"/target\">Term\n: definition</a></p>",
        ),
    ] {
        let document = parse_markdown_with_profile(source, COMMONMARK);
        assert_eq!(document.blocks.len(), 1, "{source:?}");
        assert!(
            render_html(source, &options).unwrap().contains(expected),
            "{source:?}"
        );
    }
}

#[test]
fn the_selected_grammar_reaches_quotes_lists_and_rich_footnotes() {
    let source = "> *Term\n> : definition*\n\n- [Term\n  : definition](/target)\n\n[^n]: `Term\n    : definition`\n";
    let document = parse_markdown_with_profile(source, COMMONMARK);
    let Block::BlockQuote(quote) = &document.blocks[0] else {
        panic!("expected quote");
    };
    assert_eq!(
        quote,
        &[Block::Paragraph(vec![Inline::Emphasis(vec![
            Inline::Text("Term".into()),
            Inline::SoftBreak,
            Inline::Text(": definition".into()),
        ])])]
    );
    let Block::List(list) = &document.blocks[1] else {
        panic!("expected list");
    };
    assert_eq!(
        list.items[0].blocks,
        [Block::Paragraph(vec![Inline::Link {
            dest: "/target".into(),
            title: None,
            content: vec![
                Inline::Text("Term".into()),
                Inline::SoftBreak,
                Inline::Text(": definition".into()),
            ],
        }])]
    );
    let Block::FootnoteDefinition { blocks, .. } = &document.blocks[2] else {
        panic!("expected footnote");
    };
    assert_eq!(
        blocks,
        &[Block::Paragraph(vec![Inline::Code(
            "Term : definition".into()
        )])]
    );
}

#[test]
fn default_and_gfm_plus_preserve_the_existing_extension_ast() {
    let source = "First\n: value one\nSecond\n: value two\n\n> Term\n> : definition\n\n[^n]: Term\n    : definition\n";
    let expected = parse_markdown(source);
    assert!(matches!(&expected.blocks[0], Block::DefinitionList(items) if items.len() == 2));
    for profile in [None, Some(Profile::GfmPlus)] {
        assert_eq!(parse_markdown_with_profile(source, profile), expected);
        assert_eq!(
            parse_markdown_profiled_with_profile(source, profile).document,
            expected
        );
        assert_eq!(
            parse_markdown_spanned_with_profile(source, profile).to_document(),
            expected
        );
        assert_eq!(
            parse_markdown_spanned_profiled_with_profile(source, profile)
                .document
                .to_document(),
            expected
        );
    }
}

#[test]
fn profiled_and_spanned_parsers_keep_original_unicode_and_crlf_ranges() {
    let source = "\u{feff}---\r\ntitle: Profiles\r\n---\r\n*É中\r\n: definition*\r\n\r\nTail.\r\n";
    let expected = parse_markdown_with_profile(source, COMMONMARK);
    let parsed = parse_markdown_spanned_with_profile(source, COMMONMARK);
    assert_eq!(parsed.source_len, source.len());
    assert_eq!(parsed.to_document(), expected);
    assert_eq!(
        parsed.blocks[0].span.slice(source),
        Some("*É中\r\n: definition*")
    );
    assert_eq!(parsed.blocks[1].span.slice(source), Some("Tail."));
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(
        parse_markdown_profiled_with_profile(source, COMMONMARK).document,
        expected
    );
    assert_eq!(
        parse_markdown_spanned_profiled_with_profile(source, COMMONMARK).document,
        parsed
    );
}

#[test]
fn malformed_reference_diagnostics_follow_the_selected_container_grammar() {
    let source = "[top]:\n: Definition.\n\n> [quote]:\n> : Definition.\n\n- [list]:\n  : Definition.\n\n[^n]: [note]:\n    : Definition.\n";
    assert!(
        parse_markdown_spanned_with_profile(source, None)
            .diagnostics
            .is_empty()
    );
    let parsed = parse_markdown_spanned_with_profile(source, COMMONMARK);
    assert_eq!(
        parsed.to_document(),
        parse_markdown_with_profile(source, COMMONMARK)
    );
    let warnings: Vec<_> = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.code, "malformed_reference");
            diagnostic.span.slice(source).unwrap()
        })
        .collect();
    assert_eq!(warnings, ["[top]:", "[quote]:", "[list]:", "[note]:"]);
    assert_eq!(
        parse_markdown_spanned_profiled_with_profile(source, COMMONMARK).document,
        parsed
    );
}

#[test]
fn destination_tracking_uses_the_same_dialect_as_the_rendered_links_and_images() {
    let source = "[Term\n: definition](<chapter.md#part> \"title\")\n\n![Image\n: alternative](figure.png)\n";
    assert!(destination_spans(source).is_empty());
    let spans = destination_spans_with_profile(source, COMMONMARK);
    let tokens: Vec<_> = spans
        .iter()
        .map(|span| (&source[span.range.clone()], span.dest.as_str()))
        .collect();
    assert_eq!(
        tokens,
        [
            ("<chapter.md#part>", "chapter.md#part"),
            ("figure.png", "figure.png")
        ]
    );
}

#[test]
fn reference_label_tracking_retains_uses_crossing_colon_lines() {
    let source = "[Term\n: definition][target]\n\n[target]: chapter.md\n";
    let spans = reference_label_spans_with_profile(source, COMMONMARK);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].form, ReferenceLabelForm::Full);
    assert_eq!(spans[1].form, ReferenceLabelForm::Definition);
    for span in spans {
        assert_eq!(span.key, "target");
        assert_eq!(&source[span.label.unwrap()], "target");
    }
}

#[test]
fn pdf_source_entrypoints_parse_with_the_requested_authoring_profile() {
    let source = "*Term\n: definition*\n";
    let expected = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Emphasis(vec![
            Inline::Text("Term".into()),
            Inline::SoftBreak,
            Inline::Text(": definition".into()),
        ])])],
    };
    let options = PdfOptions {
        profile: COMMONMARK,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let bytes = render_pdf_document(&expected, &options).unwrap();
    assert_eq!(render_pdf(source, &options).unwrap(), bytes);
    assert_eq!(
        render_pdf_emitted(source, &options, PdfEmitOptions::default()).unwrap(),
        bytes
    );
    assert_eq!(render_pdf_reported(source, &options).unwrap().bytes, bytes);
    assert_eq!(
        render_pdf_pdfa(source, &options, PdfASettings::OFF).unwrap(),
        bytes
    );
}
