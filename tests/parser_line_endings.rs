//! LF, CRLF and lone CR have one grammar while spans retain original bytes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{
    ReferenceLabelForm, destination_spans, parse_inlines, reference_label_spans, split_frontmatter,
};
use franken_markdown::{
    Block, HtmlOptions, Inline, PdfOptions, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled, render_html, render_pdf,
};

fn endings(source: &str, pattern: &[&str]) -> String {
    let mut index = 0;
    let mut output = String::new();
    for ch in source.chars() {
        if ch == '\n' {
            output.push_str(pattern[index % pattern.len()]);
            index += 1;
        } else {
            output.push(ch);
        }
    }
    output
}

// A lone CR followed directly by LF is one CRLF, not two line endings. This
// cycle preserves blank lines even when adjacent source breaks use mixed forms.
const PATTERNS: &[&[&str]] = &[&["\r"], &["\r\n"], &["\r\n", "\n", "\r"]];

fn assert_document_parity(lf: &str) {
    let expected = parse_markdown(lf);
    let html = render_html(lf, &HtmlOptions::default()).unwrap();
    for pattern in PATTERNS {
        let source = endings(lf, pattern);
        assert_eq!(parse_markdown(&source), expected, "{source:?}");
        assert_eq!(parse_markdown_profiled(&source).document, expected);
        assert_eq!(parse_markdown_spanned(&source).to_document(), expected);
        assert_eq!(
            parse_markdown_spanned_profiled(&source)
                .document
                .to_document(),
            expected
        );
        assert_eq!(render_html(&source, &HtmlOptions::default()).unwrap(), html);
    }
}

#[test]
fn carriage_returns_cannot_fold_an_entire_document_into_its_first_heading() {
    let source = "# Title\r\rFirst paragraph.\r\r- alpha\r- beta\r";
    let document = parse_markdown(source);
    assert_eq!(document.blocks.len(), 3);
    assert_eq!(
        document.blocks[0],
        Block::Heading {
            level: 1,
            inlines: vec![Inline::Text("Title".into())]
        }
    );
    assert!(matches!(&document.blocks[1], Block::Paragraph(_)));
    assert!(matches!(&document.blocks[2], Block::List(list) if list.items.len() == 2));
    assert_document_parity("# Title\n\nFirst paragraph.\n\n- alpha\n- beta\n");
}

#[test]
fn mixed_endings_preserve_block_boundaries_across_the_supported_grammar() {
    for source in [
        "# Heading\n\nParagraph\ncontinued.\n\nTitle\n=====\n\n---\n",
        "> quoted\n> - one\n>   - nested\n>\n> continued\n\noutside\n",
        "- first\n\n  second paragraph\n\n- [x] task\n  2. child\n\noutside\n",
        "| Name | Value |\n| :--- | ---: |\n| `a|b` | right |\n\noutside\n",
        "<div>\n[ref]: /literal\n</div>\n\n[ref]\n",
        "$$\nx + y\n$$\n\nAfter math.\n",
        "[^note]: first\n\n    second paragraph\n\nUse[^note].\n",
    ] {
        assert_document_parity(source);
    }
}

#[test]
fn fenced_and_indented_code_normalize_line_breaks_without_publishing_definitions() {
    for source in [
        "```text\n[secret]: /hidden\n[^n]: Hidden note\n\n```\n\n[secret] [^n]\n",
        "    [secret]: /hidden\n    **literal**\n\n    last\n\n[secret]\n",
        "- ```text\n  [secret]: /hidden\n  ```\n\n[secret]\n",
        "> ~~~text\n> [secret]: /hidden\n> ~~~\n\n[secret]\n",
    ] {
        assert_document_parity(source);
        for pattern in PATTERNS {
            assert!(destination_spans(&endings(source, pattern)).is_empty());
        }
    }
    let document = parse_markdown("```\ra\r\nb\nc\r```\r");
    assert!(matches!(&document.blocks[..], [Block::CodeBlock { code, .. }] if code == "a\nb\nc\n"));
}

#[test]
fn tab_expansion_uses_the_same_logical_lines_and_keeps_source_authority() {
    let source = "\tcode\n\tsecond\n\n-\t[link](/target)\n\tcontinued\n\n>\tquoted\n";
    assert_document_parity(source);
    for pattern in PATTERNS {
        let source = endings(source, pattern);
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1);
        assert_eq!(&source[spans[0].range.clone()], "/target");
    }
}

#[test]
fn references_and_multiline_titles_resolve_without_consuming_following_prose() {
    assert_document_parity(
        "[Long\n Ref]:\n  </right#part>\n  \"two\n  lines\"\n\n[shown][long ref] ![image][long ref]\n\nAfter.\n",
    );
    assert_document_parity("[ref]: /right\n  \"unterminated\n\n[ref]\n");
    assert_document_parity("> [ref]: /right\n>\n> [ref]\n\n- [other]: /other\n\n[other]\n");
}

#[test]
fn frontmatter_uses_shared_endings_and_returns_an_exact_original_body_slice() {
    let lf = "\u{feff}---\ntitle: >-\n  First\n  second\nauthor: |-\n  Jane\n  Team\n---\n# Body\n\nVisible.\n";
    let expected = split_frontmatter(lf).0.unwrap();
    for pattern in PATTERNS {
        let source = endings(lf, pattern);
        let (metadata, body) = split_frontmatter(&source);
        assert_eq!(metadata, Some(expected.clone()));
        let start = source.find("# Body").unwrap();
        assert_eq!(body, &source[start..]);
        assert_eq!(
            parse_markdown(&source),
            parse_markdown("# Body\n\nVisible.\n")
        );
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(spanned.blocks[0].span.start, start);
        assert_eq!(spanned.blocks[0].span.slice(&source), Some("# Body"));
    }
}

#[test]
fn invalid_or_unclosed_frontmatter_retains_every_original_byte() {
    for lf in [
        "\u{feff}---\ntitle: unclosed\n",
        "---\n# only a comment\n---\n# Body\n",
        "---\ntitle: |\n    first\n  partial dedent\n---\n# Body\n",
        "---\n=empty key\n---\n# Body\n",
    ] {
        for pattern in PATTERNS {
            let source = endings(lf, pattern);
            let (metadata, body) = split_frontmatter(&source);
            assert!(metadata.is_none(), "{source:?}");
            assert_eq!(body, source);
            assert_eq!(parse_markdown(&source), parse_markdown(lf));
        }
    }
}

#[test]
fn top_level_spans_include_exact_mixed_terminators_between_owned_lines() {
    let source =
        "\u{feff}# é\r\n\rParagraph\ncontinued\r\r- one\r\n- two\n\n```text\rcode\r\n```\r";
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.source_len, source.len());
    let slices: Vec<_> = spanned
        .blocks
        .iter()
        .map(|block| block.span.slice(source).unwrap())
        .collect();
    assert_eq!(
        slices,
        [
            "# é",
            "Paragraph\ncontinued",
            "- one\r\n- two\n",
            "```text\rcode\r\n```"
        ]
    );
    // A consumed blank line after a list includes the preceding terminator in
    // its source span. Keep the existing LF ownership contract unchanged.
    let lf = "- one\n- two\n\n```text\ncode\n```\n";
    assert_eq!(
        parse_markdown_spanned(lf).blocks[0].span.slice(lf),
        Some("- one\n- two\n")
    );
    assert_eq!(spanned.to_document(), parse_markdown(source));
    for block in &spanned.blocks {
        assert!(source.is_char_boundary(block.span.start));
        assert!(source.is_char_boundary(block.span.end));
    }
}

#[test]
fn diagnostics_keep_original_byte_ranges_after_carriage_returns() {
    let source = "\u{feff}# é\r\n\r[bad]:\r\r```text\r\nunterminated\r";
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.diagnostics.len(), 2);
    assert_eq!(spanned.diagnostics[0].span.slice(source), Some("[bad]:"));
    assert_eq!(
        spanned.diagnostics[1].span.slice(source),
        Some("```text\r\nunterminated\r")
    );
}

#[test]
fn destination_tokens_and_edits_retain_original_unicode_and_line_ending_bytes() {
    let lf = "\u{feff}---\ntitle: Meta\n---\n[Réf]: </a#frag>\n\n# é\n\n> [Réf] [réf][] ![image][RÉF]\n> [direct](<images/café.png#début>)\n\n- nested [link](/target#later)\n";
    for pattern in PATTERNS {
        let source = endings(lf, pattern);
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 3);
        assert_eq!(
            spans
                .iter()
                .map(|span| &source[span.range.clone()])
                .collect::<Vec<_>>(),
            ["</a#frag>", "<images/café.png#début>", "/target#later"]
        );
        assert_eq!(
            spans
                .iter()
                .map(|span| &source[span.fragment_range(&source).unwrap()])
                .collect::<Vec<_>>(),
            ["frag", "début", "later"]
        );
        let mut edited = source.clone();
        let range = spans[2].range.clone();
        edited.replace_range(range.clone(), "/updated#next");
        assert_eq!(
            edited,
            format!(
                "{}/updated#next{}",
                &source[..range.start],
                &source[range.end..]
            )
        );
        assert_eq!(destination_spans(&edited)[2].dest, "/updated#next");
    }
}

#[test]
fn reference_label_edits_preserve_visible_content_and_original_newlines() {
    let lf = "\u{feff}> [Réf]: /right\n>\n> [Réf] [réf][] ![image][RÉF]\n\n- [shown][réf]\n";
    for pattern in PATTERNS {
        let source = endings(lf, pattern);
        let spans = reference_label_spans(&source);
        assert_eq!(spans.len(), 5);
        let mut edited = source.clone();
        for span in spans.iter().rev() {
            assert_eq!(span.key, "réf");
            assert!(matches!(
                &source[span.label.clone().unwrap()],
                "Réf" | "réf" | "RÉF"
            ));
            let replacement = match span.form {
                ReferenceLabelForm::Definition | ReferenceLabelForm::Full => "renamed",
                ReferenceLabelForm::Collapsed | ReferenceLabelForm::Shortcut => "[renamed]",
            };
            edited.replace_range(span.replacement.clone().unwrap(), replacement);
        }
        assert_eq!(parse_markdown(&edited), parse_markdown(&source));
        assert_eq!(
            edited
                .chars()
                .filter(|ch| *ch == '\r' || *ch == '\n')
                .collect::<String>(),
            source
                .chars()
                .filter(|ch| *ch == '\r' || *ch == '\n')
                .collect::<String>()
        );
    }
}

#[test]
fn direct_inline_parsing_normalizes_soft_hard_code_and_link_line_endings() {
    for source in [
        "first\nsecond",
        "first  \nsecond",
        "first\\\nsecond",
        "first\\\\\nsecond",
        "first&#92;\nsecond",
        "`one\ntwo`",
        "**one\ntwo**",
        "[one\ntwo](/target)",
        "[x](/target \"one\ntwo\")",
        "[x](<one\ntwo>)",
        "first\n",
        "\n\n",
    ] {
        let expected = parse_inlines(source);
        for pattern in PATTERNS {
            assert_eq!(
                parse_inlines(&endings(source, pattern)),
                expected,
                "{source:?}"
            );
        }
    }
}

#[test]
fn blank_and_unterminated_line_shapes_do_not_create_phantom_blocks() {
    for source in [
        "",
        "\n",
        "\n\n",
        "alpha",
        "alpha\n",
        "alpha\n\n",
        "alpha\n\nbeta",
    ] {
        assert_document_parity(source);
    }
    assert_eq!(parse_markdown("alpha\r\n").blocks.len(), 1);
    assert_eq!(parse_markdown("alpha\r\nbeta").blocks.len(), 1);
    assert_eq!(parse_markdown("alpha\r\r\nbeta").blocks.len(), 2);
    assert_eq!(parse_inlines("\r\n"), vec![Inline::SoftBreak]);
    assert_eq!(
        parse_inlines("\r\r\n"),
        vec![Inline::SoftBreak, Inline::SoftBreak]
    );
    assert_eq!(
        parse_inlines("\n\r"),
        vec![Inline::SoftBreak, Inline::SoftBreak]
    );
}

#[test]
fn equivalent_line_endings_produce_identical_native_pdf_bytes() {
    let source = "# Stable output\n\nA paragraph with **emphasis** and [link](/right).\n\n- first\n- second\n\n```text\nline one\nline two\n```\n";
    let options = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let expected = render_pdf(source, &options).unwrap();
    for pattern in PATTERNS {
        assert_eq!(
            render_pdf(&endings(source, pattern), &options).unwrap(),
            expected
        );
    }
}
