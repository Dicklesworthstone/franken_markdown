//! Literal NUL is replaced before grammar recognition, without changing source
//! coordinates used by editors, includes, or reference renames.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{
    destination_spans, parse_inlines, reference_label_spans, split_frontmatter,
};
use franken_markdown::{
    Block, HtmlOptions, Inline, PdfOptions, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled, render_html, render_pdf,
};

#[test]
fn literal_null_becomes_replacement_text_in_every_parser_entrypoint() {
    let expected = vec![Inline::Text("a\u{fffd}b".into())];
    assert_eq!(parse_inlines("a\0b"), expected);
    assert_eq!(parse_markdown("a\0b").blocks, [Block::Paragraph(expected)]);

    for source in [
        "# a\0b\n\nPlain\0 text.\ncontinued\0.\n",
        "`a\0b` and **bold\0** and $x\0y$.\n",
        "```text\0info\n  a\0b\n```\n\n    code\0\n",
        "> Quote\0\n>\n> - Item\0\n>   continuation\0\n",
        "| A\0 | B |\n| --- | --- |\n| cell\0 | `code\0` |\n",
        "<div title='a\0b'>\n<span>text\0</span>\n</div>\n",
        "$$\nx\0y\n$$\n\nTerm\0\n: Definition\0\n",
        "[^n\0]: note\0\n\nUse[^n\u{fffd}].\n",
        "[id\0]: /a\0#part\0 \"title\0\"\n\n[text][id\u{fffd}]\n",
    ] {
        let expected = parse_markdown(&source.replace('\0', "\u{fffd}"));
        assert_eq!(parse_markdown(source), expected, "{source:?}");
        assert_eq!(parse_markdown_profiled(source).document, expected);
        assert_eq!(parse_markdown_spanned(source).to_document(), expected);
        assert_eq!(
            parse_markdown_spanned_profiled(source)
                .document
                .to_document(),
            expected
        );
    }
}

#[test]
fn replacement_precedes_link_recognition_but_tokens_keep_original_byte_ranges() {
    let source = "é\0 [x](a\0é&#35;frag\0二) [y](#\0) [z](\0#)\n";
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 3);
    for (span, raw, destination, fragment) in [
        (
            &spans[0],
            "a\0é&#35;frag\0二",
            "a\u{fffd}é#frag\u{fffd}二",
            "frag\0二",
        ),
        (&spans[1], "#\0", "#\u{fffd}", "\0"),
        (&spans[2], "\0#", "\u{fffd}#", ""),
    ] {
        assert_eq!(&source[span.range.clone()], raw);
        assert_eq!(span.dest, destination);
        assert_eq!(&source[span.fragment_range(source).unwrap()], fragment);
    }
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("href=\"a%EF%BF%BD%C3%A9#frag%EF%BF%BD%E4%BA%8C\""));
    assert!(!html.contains('\0'));
}

#[test]
fn reference_identity_is_normalized_while_rename_locations_are_original() {
    let source = "[标\0]: /a\0#节\r\n\r\n[正文][标\u{fffd}] [标\0][] [标\u{fffd}]\r\n";
    let spans = reference_label_spans(source);
    assert_eq!(spans.len(), 4);
    for (span, raw) in spans
        .iter()
        .zip(["标\0", "标\u{fffd}", "标\0", "标\u{fffd}"])
    {
        assert_eq!(span.key, "标\u{fffd}");
        assert_eq!(&source[span.label.clone().unwrap()], raw);
        assert!(source.get(span.replacement.clone().unwrap()).is_some());
    }
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(&source[destinations[0].range.clone()], "/a\0#节");
    assert_eq!(destinations[0].dest, "/a\u{fffd}#节");
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert_eq!(html.matches("href=\"/a%EF%BF%BD#%E8%8A%82\"").count(), 3);
}

#[test]
fn block_and_diagnostic_spans_address_original_bom_metadata_unicode_and_crlf() {
    let source = "\u{feff}---\r\ntitle: A\0B\r\n---\r\n# é\0\r\n\r\n[go](a\0#节)\r\n\r\n```text\r\nunfinished\0\r\n";
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.source_len, source.len());
    assert_eq!(spanned.blocks.len(), 3);
    assert_eq!(spanned.blocks[0].span.slice(source), Some("# é\0"));
    assert_eq!(spanned.blocks[1].span.slice(source), Some("[go](a\0#节)"));
    assert_eq!(spanned.diagnostics.len(), 1);
    assert_eq!(
        spanned.diagnostics[0].span.slice(source),
        Some("```text\r\nunfinished\0\r\n")
    );
    assert!(spanned.provenance_tree().is_ok());
    assert_eq!(parse_markdown_spanned_profiled(source).document, spanned);
    let destinations = destination_spans(source);
    assert_eq!(&source[destinations[0].range.clone()], "a\0#节");
}

#[test]
fn encoded_and_literal_backslash_zero_in_code_are_not_rewritten() {
    let source = "`\\0 %00 &#0;`\n\n```text\n\\0 %00 &#0;\n```\n";
    assert_eq!(
        parse_inlines("`\\0 %00 &#0;`"),
        [Inline::Code("\\0 %00 &#0;".into())]
    );
    assert_eq!(
        parse_markdown(source).blocks[1],
        Block::CodeBlock {
            lang: Some("text".into()),
            code: "\\0 %00 &#0;\n".into()
        }
    );
}

#[test]
fn parsed_null_source_exports_as_the_same_valid_document_as_replacement_text() {
    let source = "# Title\0\n\nVisible\0 text and `code\0`.\n\n<span>HTML\0</span>\n";
    let expected = source.replace('\0', "\u{fffd}");
    for allow_raw_html in [false, true] {
        let options = HtmlOptions {
            allow_raw_html,
            ..HtmlOptions::default()
        };
        let html = render_html(source, &options).unwrap();
        assert!(!html.contains('\0'));
        assert_eq!(html, render_html(&expected, &options).unwrap());
    }
    let document = parse_markdown(source);
    let expected_document = parse_markdown(&expected);
    assert_eq!(
        franken_markdown::epub::render_epub(&document, &HtmlOptions::default()).unwrap(),
        franken_markdown::epub::render_epub(&expected_document, &HtmlOptions::default()).unwrap()
    );
    let options = PdfOptions::default();
    assert_eq!(
        render_pdf(source, &options).unwrap(),
        render_pdf(&expected, &options).unwrap()
    );
}

#[test]
fn null_before_multiline_code_does_not_shift_include_source_ownership() {
    use franken_markdown::transclude::{ResolveResult, expand_includes};
    let source = "é\0 Example `\n{{#include missing.md}}\n`\n\n{{#include actual.md}}\n";
    let resolve = |path: &str, _origin: &str| -> ResolveResult {
        assert_eq!(path, "actual.md", "code-owned include must remain literal");
        Ok(Some(("ACTUAL\n".into(), "/actual.md".into())))
    };
    assert_eq!(
        expand_includes(source, &resolve).unwrap(),
        source.replace("{{#include actual.md}}", "ACTUAL")
    );
}

#[test]
fn frontmatter_values_normalize_without_rewriting_the_borrowed_source_tail() {
    for newline in ["\n", "\r\n", "\r"] {
        let tail = format!("# Body\0 text{newline}");
        let source = format!(
            "\u{feff}---{newline}title: 'A\0B'{newline}author: >-{newline}  First\0{newline}  Last{newline}lang: en\0US{newline}odd\0key: ignored{newline}toc: true{newline}toc_depth: 3{newline}---{newline}{tail}"
        );
        let (metadata, body) = split_frontmatter(&source);
        let metadata = metadata.unwrap();
        assert_eq!(metadata.title.as_deref(), Some("A\u{fffd}B"));
        assert_eq!(metadata.author.as_deref(), Some("First\u{fffd} Last"));
        assert_eq!(metadata.lang.as_deref(), Some("en\u{fffd}US"));
        assert_eq!(metadata.unknown_keys, ["odd\u{fffd}key"]);
        assert_eq!(metadata.toc, Some(true));
        assert_eq!(metadata.toc_depth, Some(3));
        assert_eq!(body, tail);
        assert_eq!(body.as_ptr(), source[source.len() - tail.len()..].as_ptr());
        assert_eq!(
            Some(metadata),
            split_frontmatter(&source.replace('\0', "\u{fffd}")).0
        );
    }
}

#[test]
fn browser_and_book_metadata_and_epub_exports_share_source_null_replacement() {
    let source = "---\ntitle: T\0itle\nauthor: Auth\0or\nlang: en\n---\n# Body\0\n\nText.\n";
    let expected = source.replace('\0', "\u{fffd}");
    let metadata = franken_markdown::wasm::source_metadata(source);
    assert_eq!(metadata.title.as_deref(), Some("T\u{fffd}itle"));
    assert_eq!(metadata.author.as_deref(), Some("Auth\u{fffd}or"));
    assert_eq!(metadata, franken_markdown::wasm::source_metadata(&expected));
    assert!(!metadata.to_json().contains("\\u0000"));

    let book = franken_markdown::build_book(&[franken_markdown::BookInput {
        path: "chapter.md".into(),
        source: source.into(),
    }])
    .unwrap();
    assert_eq!(book.chapters[0].title, "T\u{fffd}itle");
    assert_eq!(
        book.chapters[0]
            .frontmatter
            .as_ref()
            .unwrap()
            .author
            .as_deref(),
        Some("Auth\u{fffd}or")
    );

    let options = HtmlOptions {
        title: metadata.title,
        lang: metadata.lang,
        ..HtmlOptions::default()
    };
    let document = parse_markdown(source);
    let archive = franken_markdown::epub::render_epub(&document, &options).unwrap();
    assert_eq!(
        archive,
        franken_markdown::epub::render_epub(&parse_markdown(&expected), &options).unwrap()
    );
    assert!(
        !franken_markdown::render_html_document(&document, &options)
            .unwrap()
            .contains('\0')
    );
}
