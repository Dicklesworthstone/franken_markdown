//! Scalar scanner oracle corpus for future SIMD implementations.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    classify_ascii_whitespace, find_any_special_byte, find_html_escape, find_html_text_escape,
    find_pdf_escape, scan_byte_candidates, scan_markdown_line, scan_table_or_fence_candidate,
};

fn naive_markdown_special(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|byte| {
        matches!(
            byte,
            b'\\'
                | b'\n'
                | b'\r'
                | b'\t'
                | b'#'
                | b'-'
                | b'='
                | b'*'
                | b'+' // `+` bullet-list marker, matching is_markdown_special_byte
                | b'_'
                | b'`'
                | b'~'
                | b'|'
                | b'['
                | b']'
                | b'('
                | b')'
                | b'<'
                | b'>'
                | b'!'
                | b'&'
                | b':'
                | b'@'
                | b'0'..=b'9'
        )
    })
}

fn naive_html_escape(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|byte| matches!(byte, b'&' | b'<' | b'>' | b'"'))
}

fn naive_html_text_escape(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|byte| matches!(byte, b'&' | b'<' | b'>'))
}

fn naive_pdf_escape(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|byte| matches!(byte, b'(' | b')' | b'\\' | b'\r' | b'\n'))
}

fn assert_byte_scanners(bytes: &[u8]) {
    assert_eq!(find_any_special_byte(bytes), naive_markdown_special(bytes));
    assert_eq!(find_html_text_escape(bytes), naive_html_text_escape(bytes));
    assert_eq!(find_html_escape(bytes), naive_html_escape(bytes));
    assert_eq!(find_pdf_escape(bytes), naive_pdf_escape(bytes));

    let combined = scan_byte_candidates(bytes);
    assert_eq!(
        combined.first_markdown_special,
        naive_markdown_special(bytes)
    );
    assert_eq!(combined.first_html_escape, naive_html_escape(bytes));
    assert_eq!(combined.first_pdf_escape, naive_pdf_escape(bytes));
}

#[test]
fn html_text_scanner_ignores_quotes_before_text_escapes() {
    let bytes = b"plain \"quoted\" then & escaped";

    assert_eq!(find_html_text_escape(bytes), Some(20));
    assert_eq!(find_html_escape(bytes), Some(6));
}

#[test]
fn byte_scanners_match_naive_oracles_for_core_corpus() {
    let cases: &[&[u8]] = &[
        b"",
        b"a",
        b"*",
        b"Title\n====\n",
        b"plain ascii words only",
        b"alpha **bold** `code` [link](dest)",
        b"<div class=\"x\">& escaped</div>",
        b"PDF literal (needs) \\ escaping\r\n",
        "\u{feff}# Title\r\n\t- tabbed\nemoji \u{1f680}".as_bytes(),
        b"| a | b |\n|---|:---:|\n| `x|y` | z |",
        b"```rust\nfn main() {}\n```",
    ];

    for case in cases {
        assert_byte_scanners(case);
    }
}

#[test]
fn byte_scanners_match_naive_oracles_for_all_alignment_offsets() {
    let mut backing = [b'.'; 160];
    let pattern = b"plain-prefix <html> markdown **x** pdf (x) tail";
    backing[40..40 + pattern.len()].copy_from_slice(pattern);

    for offset in 0..32 {
        let slice = &backing[offset..];
        assert_byte_scanners(slice);
    }
}

#[test]
fn byte_scanners_match_naive_oracles_for_large_generated_input() {
    let mut input = Vec::with_capacity(1_100_000);
    while input.len() < 1_048_576 {
        input.extend_from_slice(b"Paragraph with plain words and identifiers.\n");
        input.extend_from_slice(b"| alpha | beta |\n|---:|:---|\n| 123 | `x|y` |\n");
        input.extend_from_slice(b"<span data-x=\"1\">html</span> and pdf (literal) \\\n");
        input.extend_from_slice("utf8 rocket \u{1f680} and accented cafe\u{301}\n".as_bytes());
    }
    input.truncate(1_048_576);

    assert_byte_scanners(&input);
}

#[test]
fn whitespace_classifier_covers_empty_tabs_crlf_and_all_whitespace() {
    let empty = classify_ascii_whitespace(b"");
    assert_eq!(empty.first_ascii_whitespace, None);
    assert!(!empty.all_ascii_whitespace);

    let mixed = classify_ascii_whitespace(b"abc def\tghi\r\n");
    assert_eq!(mixed.first_ascii_whitespace, Some(3));
    assert!(mixed.contains_space);
    assert!(mixed.contains_tab);
    assert!(mixed.contains_cr);
    assert!(mixed.contains_lf);
    assert!(!mixed.all_ascii_whitespace);

    let all = classify_ascii_whitespace(b" \t\r\n ");
    assert_eq!(all.first_ascii_whitespace, Some(0));
    assert!(all.all_ascii_whitespace);
}

#[test]
fn markdown_line_scanner_is_conservative_for_parser_edges() {
    let bom_heading = scan_markdown_line("\u{feff}# title");
    assert!(!bom_heading.maybe_heading_marker);
    assert_eq!(bom_heading.first_special_byte, Some("\u{feff}".len()));

    let tabbed_ref = scan_markdown_line("\t[label]: /dest");
    assert!(tabbed_ref.maybe_reference);
    assert!(tabbed_ref.first_special_byte.is_some());

    let table = scan_table_or_fence_candidate("| a | `b|c` |");
    assert!(table.contains_pipe);
    assert!(table.contains_backtick);
    assert!(table.maybe_table_delimiter);
    assert!(!table.maybe_fence);

    let fence = scan_table_or_fence_candidate("   ~~~info");
    assert!(fence.contains_tilde);
    assert!(fence.maybe_fence);
}

#[test]
fn markdown_line_scanner_starter_flags_match_parser_block_precedence() {
    let blockquote = scan_markdown_line("   > quoted");
    assert!(blockquote.maybe_blockquote);

    let tab_indented_blockquote_text = scan_markdown_line(" \t> code, not quote");
    assert!(!tab_indented_blockquote_text.maybe_blockquote);

    let indented_blockquote_text = scan_markdown_line("    > code, not quote");
    assert!(!indented_blockquote_text.maybe_blockquote);

    let inline_greater_than = scan_markdown_line("a > b");
    assert!(!inline_greater_than.maybe_blockquote);
    assert!(inline_greater_than.first_special_byte.is_some());

    let unordered = scan_markdown_line("   - item");
    assert!(unordered.maybe_list_marker);

    let ordered = scan_markdown_line("123. ordered");
    assert!(ordered.maybe_list_marker);

    let setext_equals = scan_markdown_line("===");
    assert!(setext_equals.maybe_setext_underline);
    assert_eq!(setext_equals.first_special_byte, Some(0));

    let numeric_paragraph = scan_markdown_line("2026 report");
    assert!(!numeric_paragraph.maybe_list_marker);

    let indented_list_text = scan_markdown_line("    - code, not list");
    assert!(!indented_list_text.maybe_list_marker);
}
