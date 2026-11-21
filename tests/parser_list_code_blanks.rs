//! Internal indented-code blanks retain their bytes and do not separate list blocks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, reference_label_spans};
use franken_markdown::{
    Block, Document, HtmlOptions, List, Profile, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled, parse_markdown_with_profile,
};

fn document(source: &str) -> Document {
    let parsed = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, parsed);
    assert_eq!(parse_markdown_spanned(source).to_document(), parsed);
    assert_eq!(
        parse_markdown_spanned_profiled(source)
            .document
            .to_document(),
        parsed
    );
    for profile in [Profile::CommonMarkGfm, Profile::GfmPlus] {
        assert_eq!(parse_markdown_with_profile(source, Some(profile)), parsed);
    }
    parsed
}

fn list(block: &Block) -> &List {
    let Block::List(list) = block else {
        panic!("expected list, got {block:?}");
    };
    list
}

fn collect<'a>(blocks: &'a [Block], code: &mut Vec<&'a str>, tight: &mut Vec<bool>) {
    for block in blocks {
        match block {
            Block::CodeBlock { code: text, .. } => code.push(text),
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                collect(inner, code, tight);
            }
            Block::List(list) => {
                tight.push(list.tight);
                for item in &list.items {
                    collect(&item.blocks, code, tight);
                }
            }
            _ => {}
        }
    }
}

fn literals_and_tightness(parsed: &Document) -> (Vec<&str>, Vec<bool>) {
    let mut code = Vec::new();
    let mut tight = Vec::new();
    collect(&parsed.blocks, &mut code, &mut tight);
    (code, tight)
}

#[test]
fn internal_code_blanks_keep_spaces_tabs_and_tight_siblings() {
    for marker in ["-", "+", "*", "1.", "23)"] {
        let indent = " ".repeat(marker.len() + 5);
        for blank in ["", "  ", "\t", " \t ", "\t\t"] {
            let source =
                format!("{marker}     first\n{indent}{blank}\n{indent}second\n{marker} tail\n");
            let parsed = document(&source);
            let expected = format!("first\n{blank}\nsecond\n");
            assert_eq!(
                literals_and_tightness(&parsed),
                (vec![expected.as_str()], vec![true]),
                "{source:?}"
            );
            assert_eq!(list(&parsed.blocks[0]).items.len(), 2);
            let html =
                franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
            assert!(html.contains(&format!("<pre><code>{expected}</code></pre>")));
            assert!(html.contains("<li>tail</li>"), "{html}");
        }
    }
}

#[test]
fn only_the_structural_portion_of_a_blank_tab_is_consumed() {
    // Six columns belong to the list and code indentation. The second tab
    // straddles that boundary; its remaining two spaces precede a literal tab.
    let parsed = document("-     first\n\t\t\t\n      second\n- tail\n");
    assert_eq!(
        literals_and_tightness(&parsed),
        (vec!["first\n  \t\nsecond\n"], vec![true])
    );

    // A shorter blank still joins the two code chunks without inventing bytes.
    for blank in ["", " ", "   ", "\t", "    \t"] {
        let parsed = document(&format!("-     first\n{blank}\n      second\n- tail\n"));
        let expected = if blank == "    \t" {
            "first\n  \nsecond\n"
        } else {
            "first\n\nsecond\n"
        };
        assert_eq!(
            literals_and_tightness(&parsed),
            (vec![expected], vec![true])
        );
    }
}

fn quote(source: &str) -> String {
    source.lines().map(|line| format!("> {line}\n")).collect()
}

fn outer_list(source: &str) -> String {
    let mut lines = source.lines();
    let mut result = format!("- {}\n", lines.next().unwrap());
    for line in lines {
        result.push_str(&format!("  {line}\n"));
    }
    result.push_str("- outer tail\n");
    result
}

#[test]
fn nested_lists_and_explicit_quotes_retain_the_innermost_code_gap() {
    let source = "-     α\n      \t \n      ω\n- inner tail\n";
    for source in [
        outer_list(source),
        quote(source),
        outer_list(&quote(source)),
        quote(&outer_list(source)),
        outer_list(&quote(&outer_list(source))),
    ] {
        let parsed = document(&source);
        let (code, tight) = literals_and_tightness(&parsed);
        assert_eq!(code, ["α\n\t \nω\n"], "{source:?}");
        assert!(tight.iter().all(|value| *value), "{source:?}: {tight:?}");
    }
}

#[test]
fn a_missing_quote_marker_ends_quoted_code_instead_of_claiming_the_gap() {
    let parsed = document("- >     first\n        \n  >     second\n- tail\n");
    assert_eq!(
        literals_and_tightness(&parsed),
        (vec!["first\n", "second\n"], vec![false])
    );
    assert_eq!(list(&parsed.blocks[0]).items[0].blocks.len(), 2);

    let parsed = document("- >     first\n  >     \t \n  >     second\n- tail\n");
    assert_eq!(
        literals_and_tightness(&parsed),
        (vec!["first\n\t \nsecond\n"], vec![true])
    );
}

#[test]
fn real_item_and_block_separators_still_make_lists_loose() {
    for after in ["  paragraph", "  # heading", "  - nested", "- sibling"] {
        let source = format!("-     first\n      \t\n{after}\n- tail\n");
        let parsed = document(&source);
        let (code, tight) = literals_and_tightness(&parsed);
        assert_eq!(code, ["first\n"], "{source:?}");
        assert!(!tight[0], "{source:?}");
        let html = franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
        assert!(html.contains("<p>tail</p>"), "{html}");
    }

    // A gap inside the inner list can separate its blocks without loosening
    // the outer list, whose item still contains just one nested list.
    let parsed =
        document("- -     first\n        \t\n    paragraph\n  - inner tail\n- outer tail\n");
    assert_eq!(
        literals_and_tightness(&parsed),
        (vec!["first\n"], vec![true, false])
    );
}

#[test]
fn code_definition_lookalikes_stay_literal_and_real_unicode_spans_stay_original() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = concat!(
            "\u{feff}-     α\n      \t\n",
            "      [fake]: /wrong\n      [hidden](/never)\n",
            "- [good][réf]\n\n[réf]: /réel#β\n\n[visible](/real)\n"
        )
        .replace('\n', newline);
        let parsed = document(&source);
        assert_eq!(
            literals_and_tightness(&parsed),
            (
                vec!["α\n\t\n[fake]: /wrong\n[hidden](/never)\n"],
                vec![true]
            )
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 2, "{spans:?}");
        for (span, expected) in spans.iter().zip(["/réel#β", "/real"]) {
            assert_eq!(span.dest, expected);
            assert_eq!(&source[span.range.clone()], expected);
        }
        let labels = reference_label_spans(&source);
        assert_eq!(labels.len(), 2, "{labels:?}");
        for label in labels {
            assert_eq!(&source[label.label.unwrap()], "réf");
        }
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(
            spanned.blocks.last().unwrap().span.slice(&source),
            Some("[visible](/real)")
        );
        assert!(spanned.diagnostics.is_empty());
    }
}

#[test]
fn long_internal_blank_runs_remain_one_literal_code_block() {
    const BLANKS: usize = 4096;
    let source = format!(
        "-     first\n{}      last\n- tail\n      \t\n\noutside\n",
        "      \t\n".repeat(BLANKS)
    );
    let expected = format!("first\n{}last\n", "\t\n".repeat(BLANKS));
    let parsed = document(&source);
    assert_eq!(
        literals_and_tightness(&parsed),
        (vec![expected.as_str()], vec![true])
    );
    assert_eq!(parsed.blocks.len(), 2);
}
