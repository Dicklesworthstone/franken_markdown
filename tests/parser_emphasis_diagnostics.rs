//! Bounded emphasis keeps its established AST and exposes recoverable degradation.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, DiagnosticSeverity, Inline, SpannedDocument, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled,
};

fn depth_limited() -> String {
    // One pair would require 1000 wrappers. The existing depth guard keeps
    // 999 wrappers and leaves two stars at each end. It also fits the cache.
    let stars = "*".repeat(2000);
    format!("{stars}x{stars}")
}

fn work_limited() -> String {
    // Nine independent pairs stay below the nesting limit individually, but
    // together require more than the existing 4096 delimiter-walk steps.
    let stars = "*".repeat(600);
    vec![format!("{stars}x{stars}"); 9].join(" ")
}

fn checked(source: &str) -> SpannedDocument {
    let parsed = parse_markdown_spanned(source);
    assert_eq!(parsed.to_document(), parse_markdown(source));
    assert_eq!(
        parsed.to_document(),
        parse_markdown_profiled(source).document
    );
    assert_eq!(parsed, parse_markdown_spanned_profiled(source).document);
    assert_eq!(parsed.source_len, source.len());
    parsed
}

fn assert_warning(parsed: &SpannedDocument, code: &str) {
    assert_eq!(parsed.diagnostics.len(), 1);
    let diagnostic = &parsed.diagnostics[0];
    assert_eq!(diagnostic.code, code);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    assert!(diagnostic.message.contains("delimiters"));
    assert!(
        !diagnostic
            .message
            .contains("this inline run is rendered as literal text")
    );
}

fn emphasis_shape(inlines: &[Inline]) -> (usize, usize, String) {
    // Walk iteratively so the test itself does not add a deep recursive walk.
    let mut pending: Vec<_> = inlines.iter().rev().map(|node| (node, 0)).collect();
    let mut wrappers = 0;
    let mut deepest = 0;
    let mut text = String::new();
    while let Some((node, depth)) = pending.pop() {
        match node {
            Inline::Strong(children)
            | Inline::Emphasis(children)
            | Inline::Strikethrough(children) => {
                wrappers += 1;
                deepest = deepest.max(depth + 1);
                pending.extend(children.iter().rev().map(|child| (child, depth + 1)));
            }
            Inline::Text(value) => text.push_str(value),
            _ => {}
        }
    }
    (wrappers, deepest, text)
}

#[test]
fn depth_capped_emphasis_is_reported_without_changing_its_bounded_tree() {
    let source = depth_limited();
    let document = parse_markdown(&source);
    let [Block::Paragraph(inlines)] = document.blocks.as_slice() else {
        panic!("expected the emphasis paragraph");
    };
    assert_eq!(emphasis_shape(inlines), (999, 999, "**x**".into()));

    let parsed = checked(&source);
    assert_warning(&parsed, "emphasis_depth_limit");
    assert_eq!(
        parsed.diagnostics[0].span.slice(&source),
        Some(source.as_str())
    );
}

#[test]
fn work_capped_emphasis_is_distinct_and_retains_already_resolved_styles() {
    let source = work_limited();
    let document = parse_markdown(&source);
    let [Block::Paragraph(inlines)] = document.blocks.as_slice() else {
        panic!("expected the emphasis paragraph");
    };
    let (wrappers, deepest, text) = emphasis_shape(inlines);
    assert_eq!(wrappers, 2048);
    assert_eq!(deepest, 300);
    assert_eq!(text.matches('x').count(), 9);
    assert!(text.contains('*'), "unresolved source must remain literal");

    let parsed = checked(&source);
    assert_warning(&parsed, "emphasis_work_limit");
    assert_eq!(
        parsed.diagnostics[0].span.slice(&source),
        Some(source.as_str())
    );
}

#[test]
fn cached_emphasis_limits_report_each_original_unicode_and_crlf_range() {
    let limited = format!("café {}", depth_limited());
    let source =
        format!("前置\0\r\n\r\n{limited}\r\n\r\n[good](/ok)\r\n\r\n{limited}\r\n\r\n{limited}");
    let parsed = checked(&source);
    let occurrences: Vec<_> = source.match_indices(&limited).collect();
    assert_eq!(parsed.diagnostics.len(), occurrences.len());
    assert_eq!(occurrences.len(), 3);
    for (diagnostic, (start, _)) in parsed.diagnostics.iter().zip(occurrences) {
        assert_eq!(diagnostic.code, "emphasis_depth_limit");
        assert_eq!(diagnostic.span.start, start);
        assert_eq!(diagnostic.span.end, start + limited.len());
        assert_eq!(diagnostic.span.slice(&source), Some(limited.as_str()));
    }
    let destinations = destination_spans(&source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/ok");
}

#[test]
fn container_limits_keep_original_source_envelopes_and_later_link_ownership() {
    let limited = depth_limited();
    for source in [
        format!("intro α\0\r\n\r\n> - café {limited}\r\n>   tail β\r\n\r\n[good](/ok)"),
        format!("intro α\0\r\n\r\n> -\tcafé {limited}\r\n>   tail β\r\n\r\n[good](/ok)"),
        format!("intro α\0\r\n\r\n[^note]: café {limited}\r\n    tail β\r\n\r\n[good](/ok)"),
    ] {
        let parsed = checked(&source);
        assert_warning(&parsed, "emphasis_depth_limit");
        let span = parsed.diagnostics[0].span;
        assert_eq!(span.start, source.find("café").unwrap());
        assert_eq!(span.end, source.find("tail β").unwrap() + "tail β".len());
        assert!(span.slice(&source).unwrap().contains("\r\n"));
        assert!(!span.slice(&source).unwrap().contains("[good]"));
        let destinations = destination_spans(&source);
        assert_eq!(destinations.len(), 1);
        assert_eq!(destinations[0].dest, "/ok");
    }
}

#[test]
fn accepted_link_labels_report_limits_without_revoking_the_link() {
    let source = format!("[{}](/kept)", depth_limited());
    let parsed = checked(&source);
    assert_warning(&parsed, "emphasis_depth_limit");
    assert_eq!(
        parsed.diagnostics[0].span.slice(&source),
        Some(source.as_str())
    );
    let destinations = destination_spans(&source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/kept");
    assert!(matches!(
        &parsed.blocks[0].node,
        Block::Paragraph(inlines)
            if matches!(&inlines[..], [Inline::Link { dest, .. }] if dest == "/kept")
    ));
}

#[test]
fn rejected_link_speculation_cannot_report_an_emphasis_limit_for_a_bare_url() {
    let url = format!("https://example.test/{}/end", depth_limited());
    for (source, shortcuts) in [
        (format!("[{url} [inner](/inside)](/rejected)"), 0),
        (
            format!("[{url} [inner](/inside)][outer]\n\n[outer]: /rejected"),
            1,
        ),
    ] {
        let parsed = checked(&source);
        assert!(parsed.diagnostics.is_empty());
        let document = parsed.into_document();
        let [Block::Paragraph(inlines)] = document.blocks.as_slice() else {
            panic!("expected a paragraph containing the bare URL and inner link");
        };
        assert!(
            inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Link { dest, .. } if dest == &url))
        );
        assert!(
            inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Link { dest, .. } if dest == "/inside"))
        );
        // Rejecting the containing link leaves [outer] available as a valid
        // shortcut. It must not acquire the failed outer label's contents.
        let trailing_links: Vec<_> = inlines
            .iter()
            .filter_map(|inline| match inline {
                Inline::Link { dest, content, .. } if dest == "/rejected" => Some(content),
                _ => None,
            })
            .collect();
        assert_eq!(trailing_links.len(), shortcuts);
        for content in trailing_links {
            assert_eq!(content, &[Inline::Text("outer".into())]);
        }
    }
}

#[test]
fn later_tokenization_exhaustion_supersedes_a_nonfatal_emphasis_limit() {
    let source = format!(
        "[{}](/discard) {}end",
        depth_limited(),
        "$value ".repeat(10_000)
    );
    let parsed = checked(&source);
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(parsed.diagnostics[0].code, "inline_work_limit");
    assert!(
        parsed.diagnostics[0]
            .message
            .contains("rendered as literal text")
    );
    assert_eq!(
        parsed.diagnostics[0].span.slice(&source),
        Some(source.as_str())
    );
    assert!(matches!(
        &parsed.blocks[0].node,
        Block::Paragraph(inlines)
            if matches!(&inlines[..], [Inline::Text(text)] if text == &source)
    ));
    assert!(destination_spans(&source).is_empty());
}

#[test]
fn complete_emphasis_literal_delimiters_and_opaque_code_do_not_warn() {
    let limited = depth_limited();
    let complete = "*".repeat(1998);
    for source in [
        format!("{complete}x{complete}"),
        format!("{complete}unmatched"),
        format!("\u{60}{limited}\u{60}"),
        format!("~~~text\n{limited}\n~~~"),
        format!("before <!-- {limited} --> after"),
        "**ordinary** and *emphasis* and ~~strike~~".into(),
    ] {
        assert!(checked(&source).diagnostics.is_empty());
    }
}

#[test]
fn browser_outputs_preserve_the_emphasis_code_and_original_source_range() {
    use franken_markdown::wasm::{WasmRenderOptions, render_html, render_pdf};

    let limited = work_limited();
    let source = format!("café\r\n\r\n{limited}\r\n\r\nA final paragraph.");
    let parsed = checked(&source);
    assert_warning(&parsed, "emphasis_work_limit");
    let expected = &parsed.diagnostics[0];
    for output in [
        render_html(&source, &WasmRenderOptions::default()).unwrap(),
        render_pdf(&source, &WasmRenderOptions::default()).unwrap(),
    ] {
        let actual = output
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == Some(expected.code))
            .expect("emphasis degradation must reach the browser host");
        assert_eq!(actual.start, expected.span.start);
        assert_eq!(actual.end, expected.span.end);
        assert_eq!(actual.message, expected.message);
        assert_eq!(actual.severity, "warning");
        assert!(
            output
                .diagnostics_json()
                .contains("\"code\":\"emphasis_work_limit\"")
        );
    }
}

#[cfg(feature = "cli")]
#[test]
fn native_verification_reports_emphasis_degradation_with_source_offsets() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let limited = work_limited();
    let source = format!("café\r\n\r\n{limited}\r\n\r\nA final paragraph.");
    let parsed = checked(&source);
    assert_warning(&parsed, "emphasis_work_limit");
    let span = parsed.diagnostics[0].span;
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "verify", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("\"code\":\"emphasis_work_limit\""),
        "{stdout}"
    );
    assert!(stdout.contains(&format!("source bytes {}..{}", span.start, span.end)));
    assert!(stdout.contains("A final paragraph."));
    assert!(output.stderr.is_empty());
}
