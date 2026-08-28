//! 9wse.2: caret renderer wired through parse / config / verify / math spans.
//!
//! `--json` contracts stay byte-identical (Display / `to_json` unchanged).
//! CLI `src/cli.rs` is reserved by another agent; these tests exercise the
//! library chokepoints the CLI should call. The CLI e2e is SKIP until that
//! file is free (9wse.3 covers verify human-mode flag wiring).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::caret::{
    render_byte_range, render_parse_diagnostics, span_of_line, style_for_stderr,
};
use franken_markdown::config::FmdConfig;
use franken_markdown::verify::{to_human, to_json, verify_pdf};
use franken_markdown::{
    CaretStyle, ColorMode, DiagnosticSeverity, PdfOptions, parse_markdown, parse_markdown_spanned,
};

fn log(id: &str, subject: &str, outcome: &str) {
    eprintln!("check={id} subject={subject} outcome={outcome}");
}

fn assert_ok(id: &str, subject: &str, ok: bool, detail: &str) {
    if ok {
        log(id, subject, "PASS");
    } else {
        log(id, subject, "FAIL");
        panic!("{id} `{subject}`: {detail}");
    }
}

fn plain() -> CaretStyle {
    CaretStyle::default()
}

#[test]
fn parse_unclosed_fence_emits_a_caret_block() {
    let src = "# Title\n\n```rust\nfn main() {}\n";
    let parsed = parse_markdown_spanned(src);
    assert_ok(
        "parse-diag-count",
        "unclosed-fence",
        parsed.diagnostics.len() == 1,
        &format!("got {}", parsed.diagnostics.len()),
    );
    let block = render_parse_diagnostics(src, Some("doc.md"), &parsed.diagnostics, plain());
    assert_ok(
        "parse-caret-file",
        "doc.md",
        block.contains("doc.md:"),
        &block,
    );
    assert_ok(
        "parse-caret-message",
        "unclosed",
        block.contains("unclosed fenced code block"),
        &block,
    );
    assert_ok("parse-caret-gutter", "^", block.contains('^'), &block);
}

#[test]
fn config_parse_error_caret_does_not_change_display() {
    let src = "font=sans\nfont=comic\n";
    let err = FmdConfig::parse(src).expect_err("comic is not a font");
    let display = err.to_string();
    assert_ok(
        "config-display-short",
        &display,
        display.starts_with("line 2:") && !display.contains('^'),
        "Display must stay the short JSON-safe message",
    );
    let caret = err
        .render_caret(src, Some("fmd.toml"), plain())
        .expect("parse errors render");
    assert_ok(
        "config-caret-line",
        "line 2",
        caret.contains("font=comic") && caret.contains('^'),
        &caret,
    );
}

#[test]
fn config_unknown_key_caret_points_at_the_line() {
    let src = "nope=1\n";
    let err = FmdConfig::parse(src).expect_err("unknown key");
    let caret = err.render_caret(src, None, plain()).unwrap();
    assert_ok(
        "config-unknown-key",
        "nope=1",
        caret.contains("nope=1") && caret.contains("unknown config key"),
        &caret,
    );
}

#[test]
fn verify_json_is_unchanged_when_human_formatter_exists() {
    let src = "# Hello\n\nA short paragraph.\n";
    let doc = parse_markdown(src);
    let report = verify_pdf(&doc, &PdfOptions::default()).expect("fonts");
    let json = to_json(&report);
    let json2 = to_json(&report);
    assert_ok(
        "verify-json-stable",
        "digest",
        json == json2 && json.contains("\"schema_version\""),
        "to_json must be deterministic",
    );
    assert_ok(
        "verify-json-no-caret",
        "json",
        !json.contains("-->") && !json.contains("^\n"),
        "JSON must not grow a caret block",
    );
    let human = to_human(&report, src, Some("hi.md"), plain());
    assert_ok(
        "verify-human-header",
        report.verdict,
        human.starts_with("fmd verify:"),
        &human,
    );
}

#[test]
fn verify_human_carets_unresolved_anchors() {
    let src = "See [missing](#no-such-heading).\n";
    let doc = parse_markdown(src);
    let report = verify_pdf(&doc, &PdfOptions::default()).expect("fonts");
    assert_ok(
        "verify-has-anchor-finding",
        "unresolved_anchor",
        report
            .findings
            .iter()
            .any(|f| f.code == "unresolved_anchor"),
        &format!("{:?}", report.findings),
    );
    let json = to_json(&report);
    assert_ok(
        "verify-json-finding-code",
        "unresolved_anchor",
        json.contains("unresolved_anchor"),
        &json,
    );
    let human = to_human(&report, src, Some("doc.md"), plain());
    assert_ok(
        "verify-human-caret",
        "doc.md",
        human.contains("doc.md:") && human.contains('^'),
        &human,
    );
}

#[test]
fn math_error_byte_range_uses_the_shared_renderer() {
    // fmd-math is a sibling crate; the root package does not depend on it.
    // MathError::span() is a byte range — this is the documented adapter.
    let src = r"\substack{a \\ b}";
    let start = 0usize;
    let end = src.find('{').unwrap_or(src.len());
    let block = render_byte_range(
        src,
        start,
        end,
        Some("eq.tex"),
        "`\\substack` is not yet supported; tier T2",
        DiagnosticSeverity::Error,
        plain(),
    );
    assert_ok(
        "math-caret-file",
        "eq.tex",
        block.contains("eq.tex:1:"),
        &block,
    );
    assert_ok(
        "math-caret-token",
        "substack",
        block.contains("substack") && block.contains('^'),
        &block,
    );
}

#[test]
fn span_of_line_covers_first_and_last_lines() {
    let src = "alpha\nbeta\ngamma";
    let l1 = span_of_line(src, 1);
    let l3 = span_of_line(src, 3);
    assert_ok(
        "span-line-1",
        "alpha",
        &src[l1.start..l1.end] == "alpha",
        &format!("{}..{}", l1.start, l1.end),
    );
    assert_ok(
        "span-line-3",
        "gamma",
        &src[l3.start..l3.end] == "gamma",
        &format!("{}..{}", l3.start, l3.end),
    );
}

#[test]
fn stderr_style_never_colors_when_mode_is_never() {
    let style = style_for_stderr(ColorMode::Never, true, Some(80));
    assert_ok(
        "style-never",
        "color=false",
        !style.color && style.columns == Some(80),
        &format!("{style:?}"),
    );
}

#[test]
fn cli_e2e_skipped_while_cli_rs_is_reserved() {
    log("cli-e2e", "src/cli.rs exclusive PlumWolf gk3v.3", "SKIP");
}
