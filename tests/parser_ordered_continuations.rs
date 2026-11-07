//! Container indentation must not manufacture paragraph boundaries or references.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{ReferenceLabelForm, destination_spans, reference_label_spans};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, List, PdfOptions, parse_markdown,
    parse_markdown_profiled, parse_markdown_spanned, render_html_document, render_pdf_document,
};

fn document(source: &str) -> Document {
    let parsed = parse_markdown(source);
    assert_eq!(
        parse_markdown_profiled(source).document,
        parsed,
        "{source:?}"
    );
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), parsed, "{source:?}");
    assert!(spanned.diagnostics.is_empty(), "{source:?}");
    parsed
}

fn list(block: &Block) -> &List {
    let Block::List(list) = block else {
        panic!("expected list, got {block:?}");
    };
    list
}

fn fragment(document: &Document) -> String {
    franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default())
}

#[test]
fn non_one_markers_continue_an_open_paragraph_at_every_list_indent() {
    // CommonMark 0.31.2 §5.2: an ordered list may interrupt a paragraph only
    // when its start number is 1. Official examples 304/305 demonstrate the
    // contrast. Wrapping that paragraph in an item does not change the rule.
    for (outer, indent) in [("-", 2), ("1.", 3), ("23)", 4)] {
        for extra in 0..=3 {
            for marker in ["0.", "2.", "7)", "1986.", "999999999)"] {
                let source = format!(
                    "{outer} before\n{}{marker} continuation\n",
                    " ".repeat(indent + extra)
                );
                let parsed = document(&source);
                assert_eq!(parsed.blocks.len(), 1, "{source:?}");
                let outer = list(&parsed.blocks[0]);
                assert!(outer.tight, "{source:?}");
                assert_eq!(outer.items.len(), 1, "{source:?}");
                assert_eq!(
                    outer.items[0].blocks,
                    [Block::Paragraph(vec![
                        Inline::Text("before".into()),
                        Inline::SoftBreak,
                        Inline::Text(format!("{marker} continuation")),
                    ])],
                    "{source:?}"
                );
                assert!(
                    fragment(&parsed).contains(&format!("<li>before\n{marker} continuation</li>")),
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn numbered_continuations_cannot_hide_source_or_define_phantom_references() {
    let body = "- café\n  7. [réf]: /phantom\n\nUse [réf].\n";
    let quoted = body
        .lines()
        .map(|line| format!("> {line}\n"))
        .collect::<String>();
    let nested = format!(
        "- parent\n{}",
        body.lines()
            .map(|line| format!("  {line}\n"))
            .collect::<String>()
    );
    let note = format!(
        "[^note]:\n{}\nRead[^note].\n",
        body.lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>()
    );
    let task = body.replacen("- café", "- [x] café", 1);
    for source in [body.to_owned(), quoted, nested, note, task] {
        for newline in ["\n", "\r\n", "\r"] {
            let source = source.replace('\n', newline);
            let parsed = document(&source);
            let rendered = fragment(&parsed);
            assert!(
                rendered.contains("7. [réf]: /phantom"),
                "{source:?}: {rendered}"
            );
            assert!(rendered.contains("Use [réf]."), "{source:?}: {rendered}");
            assert!(!rendered.contains("href=\"/phantom\""), "{source:?}");
            assert!(destination_spans(&source).is_empty(), "{source:?}");
            assert!(reference_label_spans(&source).is_empty(), "{source:?}");
        }
    }
}

#[test]
fn a_numbered_continuation_cannot_shadow_a_later_real_definition() {
    let source = "- before\n  7. [ref]: /phantom\n\n[ref]: /real\n\nUse [ref].\n";
    let parsed = document(source);
    let rendered = fragment(&parsed);
    assert!(rendered.contains("7. <a href=\"/real\">ref</a>: /phantom"));
    assert!(rendered.contains("Use <a href=\"/real\">ref</a>."));
    assert!(!rendered.contains("href=\"/phantom\""));
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/real");
    assert_eq!(&source[destinations[0].range.clone()], "/real");
    let definitions: Vec<_> = reference_label_spans(source)
        .into_iter()
        .filter(|span| span.form == ReferenceLabelForm::Definition)
        .collect();
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        definitions[0].label,
        Some(source.find("[ref]: /real").unwrap() + 1..source.find("[ref]: /real").unwrap() + 4)
    );
}

#[test]
fn real_nested_numbered_items_with_continuations_remain_one_tight_list() {
    let source = "- parent\n  1. first\n     continuation\n  2. second\n     continuation too\n  3. third\n- sibling\n";
    let parsed = document(source);
    let outer = list(&parsed.blocks[0]);
    assert!(outer.tight);
    assert_eq!(outer.items.len(), 2);
    assert_eq!(outer.items[0].blocks.len(), 2);
    let inner = list(&outer.items[0].blocks[1]);
    assert!(inner.ordered && inner.tight);
    assert_eq!(inner.start, 1);
    assert_eq!(inner.items.len(), 3);
    assert_eq!(
        fragment(&parsed),
        "<ul>\n<li>parent\n<ol>\n<li>first\ncontinuation</li>\n<li>second\ncontinuation too</li>\n<li>third</li>\n</ol>\n</li>\n<li>sibling</li>\n</ul>\n"
    );
}

#[test]
fn non_one_lists_still_start_at_actual_block_boundaries() {
    for (source, loose) in [
        ("1. parent\n\n   3. child\n   4. next\n", true),
        ("- # Heading\n  3. child\n  4. next\n", false),
        ("- ```\n  literal\n  ```\n  3. child\n  4. next\n", false),
    ] {
        let parsed = document(source);
        let outer = list(&parsed.blocks[0]);
        assert_eq!(outer.tight, !loose, "{source:?}");
        let inner = list(&outer.items[0].blocks[1]);
        assert!(inner.ordered && inner.tight, "{source:?}");
        assert_eq!(inner.start, 3, "{source:?}");
        assert_eq!(inner.items.len(), 2, "{source:?}");
    }

    let source = "- before\n\n  7. [ref]: /target\n  8. [ref]\n";
    let parsed = document(source);
    assert!(fragment(&parsed).contains("<a href=\"/target\">ref</a>"));
    assert_eq!(destination_spans(source).len(), 1);
    assert_eq!(destination_spans(source)[0].dest, "/target");
}

#[test]
fn unicode_crlf_tabs_retain_source_ranges_without_phantom_edit_targets() {
    let source =
        "> - café\r\n> \t7. [réf]: /phantom\r\n>\r\n> Use [réf].\r\n\r\n[outside](/real)\r\n";
    let parsed = document(source);
    assert!(fragment(&parsed).contains("7. [réf]: /phantom"));
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.blocks.len(), 2);
    let quote_end = source.find("\r\n\r\n").unwrap();
    assert_eq!(
        spanned.blocks[0].span.slice(source),
        Some(&source[..quote_end])
    );
    assert_eq!(
        spanned.blocks[1].span.slice(source),
        Some("[outside](/real)")
    );
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/real");
    assert_eq!(&source[destinations[0].range.clone()], "/real");
    assert!(reference_label_spans(source).is_empty());
}

#[test]
fn native_and_browser_outputs_keep_the_numbered_source_without_a_link_annotation() {
    let source = "- Important\n  7. [ref]: /phantom\n\nUse [ref].\n";
    let parsed = document(source);
    let html = render_html_document(&parsed, &HtmlOptions::default()).unwrap();
    assert!(html.contains("7. [ref]: /phantom"));
    let browser_options = franken_markdown::wasm::WasmRenderOptions::default();
    let browser_html = franken_markdown::wasm::render_html(source, &browser_options).unwrap();
    assert_eq!(browser_html.bytes, html.as_bytes());
    assert!(browser_html.diagnostics.is_empty());

    let options = PdfOptions::default();
    let report = franken_markdown::verify::verify_pdf_result(&parsed, &options).unwrap();
    let text = report
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("/phantom"), "missing visible source: {text}");
    let pdf = render_pdf_document(&parsed, &options).unwrap();
    assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Link"));
    let browser_pdf = franken_markdown::wasm::render_pdf(source, &browser_options).unwrap();
    assert_eq!(browser_pdf.bytes, pdf);
}
