//! Caret diagnostics renderer (bead 9wse.1).
//!
//! Every assertion logs `check=<id> subject=<…> outcome=PASS|FAIL` on stderr.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    CaretStyle, ColorMode, DiagnosticSeverity, ParseDiagnostic, SourceSpan, parse_markdown_spanned,
    render_caret, render_parse_diagnostic,
};

fn log_check(id: &str, subject: &str, outcome: &str) {
    eprintln!("check={id} subject={subject} outcome={outcome}");
}

fn assert_ok(id: &str, subject: &str, ok: bool, detail: &str) {
    if ok {
        log_check(id, subject, "PASS");
    } else {
        log_check(id, subject, "FAIL");
        panic!("{id} failed for `{subject}`: {detail}");
    }
}

fn plain() -> CaretStyle {
    CaretStyle::default()
}

#[test]
fn ascii_golden_block() {
    let src = "alpha\nfoo bar\nbaz\n";
    let span = SourceSpan::new(10, 13); // "bar"
    let got = render_caret(
        src,
        span,
        Some("doc.md"),
        "unused ident",
        DiagnosticSeverity::Warning,
        plain(),
    );
    let expected = "\
doc.md:2:5: warning: unused ident
1 | alpha
2 | foo bar
  |     ^^^
3 | baz
";
    assert_ok(
        "ascii-golden",
        "foo bar",
        got == expected,
        &format!("\n expected:\n{expected}\n got:\n{got}"),
    );
}

#[test]
fn cjk_column_is_two_wide() {
    let src = "a中b\n";
    // '中' starts at byte 1.
    let span = SourceSpan::new(1, src.find('b').unwrap());
    let got = render_caret(src, span, None, "wide", DiagnosticSeverity::Error, plain());
    assert_ok(
        "cjk-header-col",
        "a中b",
        got.starts_with("1:2: error: wide\n"),
        &got,
    );
    assert_ok(
        "cjk-caret-width",
        "a中b",
        got.contains(" | a中b\n") && got.contains(" |  ^^\n"),
        &got,
    );
}

#[test]
fn combining_mark_does_not_advance_column() {
    let src = "e\u{0301}x"; // é as e + combining acute
    let acute_end = "e\u{0301}".len();
    let span = SourceSpan::new(0, acute_end);
    let got = render_caret(
        src,
        span,
        None,
        "accent",
        DiagnosticSeverity::Warning,
        plain(),
    );
    assert_ok(
        "combining-col",
        "e\\u{0301}x",
        got.starts_with("1:1: warning: accent\n"),
        &got,
    );
    // One column of caret under the grapheme, then 'x'.
    assert_ok(
        "combining-caret",
        "e\\u{0301}x",
        got.contains(" | e\u{0301}x\n") && got.contains(" | ^\n"),
        &got,
    );
}

#[test]
fn empty_and_oob_spans_do_not_panic() {
    let cases = [
        ("", SourceSpan::new(0, 0)),
        ("hi", SourceSpan::new(9, 12)),
        ("hi", SourceSpan::new(2, 2)),
        ("hi", SourceSpan::new(12, 3)),
    ];
    for (i, (src, span)) in cases.iter().enumerate() {
        let got = render_caret(
            src,
            *span,
            Some("x.md"),
            "edge",
            DiagnosticSeverity::Warning,
            plain(),
        );
        assert_ok(
            &format!("edge-{i}"),
            src,
            got.contains("warning: edge") && got.contains('^'),
            &got,
        );
    }
}

#[test]
fn color_policy_matrix() {
    assert_ok(
        "never",
        "ColorMode::Never",
        !ColorMode::Never.enabled(true) && !ColorMode::Never.enabled(false),
        "Never must suppress color",
    );
    assert_ok(
        "always",
        "ColorMode::Always",
        ColorMode::Always.enabled(true) && ColorMode::Always.enabled(false),
        "Always must emit color",
    );
    assert_ok(
        "auto-tty",
        "ColorMode::Auto",
        ColorMode::Auto.enabled(true) && !ColorMode::Auto.enabled(false),
        "Auto follows TTY",
    );
    let src = "abc";
    let span = SourceSpan::new(0, 1);
    let colored = render_caret(
        src,
        span,
        None,
        "c",
        DiagnosticSeverity::Error,
        CaretStyle {
            color: true,
            columns: None,
            context_lines: 0,
        },
    );
    let plain_s = render_caret(
        src,
        span,
        None,
        "c",
        DiagnosticSeverity::Error,
        CaretStyle {
            color: false,
            columns: None,
            context_lines: 0,
        },
    );
    assert_ok(
        "ansi-present",
        "color=true",
        colored.contains('\u{1b}') && colored.contains("error"),
        &colored,
    );
    assert_ok(
        "ansi-absent",
        "color=false",
        !plain_s.contains('\u{1b}'),
        &plain_s,
    );
}

#[test]
fn narrow_terminal_elides() {
    let src = "abcdefghijklmnopqrstuvwxyz";
    let span = SourceSpan::new(0, 3);
    let got = render_caret(
        src,
        span,
        None,
        "long",
        DiagnosticSeverity::Warning,
        CaretStyle {
            color: false,
            columns: Some(16),
            context_lines: 0,
        },
    );
    assert_ok(
        "elide-ellipsis",
        "26-char line",
        got.contains('…') && got.contains("^^^"),
        &got,
    );
}

#[test]
fn parse_diagnostic_helper_renders_spanned_warning() {
    let src = "hello  world";
    let diag = ParseDiagnostic::warning(SourceSpan::new(7, 12), "double space");
    let got = render_parse_diagnostic(&diag, src, Some("n.md"), plain());
    assert_ok(
        "parse-diag",
        src,
        got.contains("n.md:1:8: warning: double space") && got.contains('^'),
        &got,
    );
}

#[test]
fn byte_golden_is_deterministic() {
    let src = "one\ntwo\n";
    let span = SourceSpan::new(4, 7);
    let a = render_caret(
        src,
        span,
        Some("t.md"),
        "x",
        DiagnosticSeverity::Error,
        plain(),
    );
    let b = render_caret(
        src,
        span,
        Some("t.md"),
        "x",
        DiagnosticSeverity::Error,
        plain(),
    );
    assert_ok("determinism", "two", a == b, "two renders differed");
}

#[test]
fn spanned_parser_output_can_be_rendered() {
    let src = "*[broken";
    let spanned = parse_markdown_spanned(src);
    let mut rendered = 0usize;
    for d in &spanned.diagnostics {
        let block = render_parse_diagnostic(d, src, Some("in.md"), plain());
        assert_ok(
            "spanned-diag",
            &d.message,
            block.contains("in.md:") && block.contains('^'),
            &block,
        );
        rendered += 1;
    }
    log_check(
        "spanned-count",
        &format!("n={rendered}"),
        if rendered > 0 { "PASS" } else { "SKIP" },
    );
}
