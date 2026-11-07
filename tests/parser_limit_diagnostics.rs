//! Resource fallbacks retain source text and report the actual affected bytes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    DiagnosticSeverity, SpannedDocument, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled,
};

fn work_limited() -> String {
    // Rejected outer links repeatedly reconsider the same inner candidates.
    // This is under the inline-cache admission size, and reaches the work
    // limit before the independent recursion limit.
    format!("{}leaf{}", "[".repeat(32), "](/target)".repeat(32))
}

fn checked(source: &str) -> SpannedDocument {
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), parse_markdown(source));
    assert_eq!(
        spanned.to_document(),
        parse_markdown_profiled(source).document
    );
    assert_eq!(spanned, parse_markdown_spanned_profiled(source).document);
    assert_eq!(spanned.source_len, source.len());
    for diagnostic in &spanned.diagnostics {
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert!(!diagnostic.span.is_empty());
        assert!(diagnostic.span.slice(source).is_some());
    }
    spanned
}

#[test]
fn cached_fallbacks_report_each_occurrence_and_leave_normal_paragraphs_clean() {
    let hostile = format!("café {}", work_limited());
    let clean = "[a normal link](/ok)";
    let source = format!(
        "前置\0\r\n\r\n{hostile}\r\n\r\n{clean}\r\n\r\n{hostile}\r\n\r\n{clean}\r\n\r\n{hostile}\r\n\r\n{clean}\r\n\r\n{hostile}"
    );
    let parsed = checked(&source);
    let expected: Vec<_> = source
        .match_indices(&hostile)
        .map(|(start, _)| start..start + hostile.len())
        .collect();
    assert_eq!(parsed.diagnostics.len(), expected.len());
    for (diagnostic, range) in parsed.diagnostics.iter().zip(expected) {
        assert_eq!(diagnostic.code, "inline_work_limit");
        assert_eq!(diagnostic.span.start, range.start);
        assert_eq!(diagnostic.span.end, range.end);
        assert_eq!(diagnostic.span.slice(&source), Some(hostile.as_str()));
        assert!(diagnostic.message.contains("rendered as literal text"));
    }
    let destinations = franken_markdown::parse::destination_spans(&source);
    assert_eq!(destinations.len(), 3);
    assert!(destinations.iter().all(|span| span.dest == "/ok"));
}

#[test]
fn copied_container_text_retains_the_original_unicode_and_crlf_envelope() {
    let hostile = work_limited();
    for source in [
        format!("intro α\r\n\r\n> - café {hostile}\r\n>   tail β\r\n\r\n[good](/ok)\r\n"),
        format!("intro α\r\n\r\n> -\tcafé {hostile}\r\n>   tail β\r\n\r\n[good](/ok)\r\n"),
        format!("intro α\r\n\r\n[^note]: café {hostile}\r\n    tail β\r\n\r\n[good](/ok)\r\n"),
        format!("intro α\r\n\r\nTerm\r\n: café {hostile}\r\n  tail β\r\n\r\n[good](/ok)\r\n"),
    ] {
        let parsed = checked(&source);
        assert_eq!(parsed.diagnostics.len(), 1, "{source:?}");
        let diagnostic = &parsed.diagnostics[0];
        assert_eq!(diagnostic.code, "inline_work_limit");
        assert_eq!(diagnostic.span.start, source.find("café").unwrap());
        assert_eq!(
            diagnostic.span.end,
            source.find("tail β").unwrap() + "tail β".len()
        );
        // This is the real source envelope, including stripped container
        // prefixes and original CRLF bytes, never a synthesized joined string.
        assert!(diagnostic.span.slice(&source).unwrap().contains("\r\n"));
        assert!(!diagnostic.span.slice(&source).unwrap().contains("[good]"));
    }
}

#[test]
fn headings_table_cells_and_task_items_report_only_their_affected_inline_run() {
    let hostile = format!("café {}", work_limited());
    for source in [
        format!("# {hostile}\n\n[good](/ok)"),
        format!("| head | other |\n| --- | --- |\n| {hostile} | [good](/ok) |"),
        format!("- [x] {hostile}\n- [good](/ok)"),
    ] {
        let parsed = checked(&source);
        assert_eq!(parsed.diagnostics.len(), 1, "{source:?}");
        let diagnostic = &parsed.diagnostics[0];
        assert_eq!(diagnostic.code, "inline_work_limit");
        assert_eq!(diagnostic.span.slice(&source), Some(hostile.as_str()));
    }
}

#[test]
fn inline_depth_is_distinct_from_speculative_work_exhaustion() {
    // Plain leading text increases the work budget without adding recursive
    // scans. Nested image labels can therefore reach the actual depth guard.
    let source = format!(
        "{}{}leaf{}",
        "text ".repeat(1024),
        "![".repeat(80),
        "](/image)".repeat(80),
    );
    let parsed = checked(&source);
    assert_eq!(parsed.diagnostics.len(), 1);
    let diagnostic = &parsed.diagnostics[0];
    assert_eq!(diagnostic.code, "inline_depth_limit");
    assert_eq!(diagnostic.span.slice(&source), Some(source.as_str()));
    assert!(diagnostic.message.contains("rendered as literal text"));
}

#[test]
fn block_depth_fallback_reports_the_remaining_source_and_recovers_afterward() {
    let deep = format!("{}leaf β", "> ".repeat(132));
    let source = format!("intro α\r\n\r\n{deep}\r\n\r\n[good](/ok)");
    let parsed = checked(&source);
    assert_eq!(parsed.diagnostics.len(), 1);
    let diagnostic = &parsed.diagnostics[0];
    assert_eq!(diagnostic.code, "block_depth_limit");
    assert_eq!(diagnostic.span.slice(&source), Some("> > > leaf β"));
    assert_eq!(
        diagnostic.span.end,
        source.find("leaf β").unwrap() + "leaf β".len()
    );
    assert!(diagnostic.message.contains("rendered as one paragraph"));
    let destinations = franken_markdown::parse::destination_spans(&source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/ok");
}

#[test]
fn old_and_new_diagnostics_are_source_ordered_without_false_literal_warnings() {
    let hostile = work_limited();
    let source = format!("{hostile}\n\n[broken]:\n\n```rust\nunclosed");
    let parsed = checked(&source);
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        ["inline_work_limit", "malformed_reference", "unclosed_fence"]
    );
    for source in [
        format!("```\n{hostile}\n```"),
        format!("`{hostile}`"),
        format!("<!-- {hostile} -->"),
        "[**ordinary**](/target) and ~~formatting~~".to_owned(),
    ] {
        assert!(checked(&source).diagnostics.is_empty(), "{source:?}");
    }
}
