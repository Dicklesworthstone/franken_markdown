//! fmd verify — machine-readable render verification (beads yo83.1-3).
//!
//! Builds a stable-schema JSON report from the same layout+pagination pipeline
//! the PDF writer uses: per-page text runs, internal-anchor audit, render
//! warnings, and horizontal overflow findings, digested with FNV-1a 64 (a
//! non-cryptographic change detector — no crypto dependencies by doctrine).
//!
//! The JSON contract (schema version 1) is pinned by golden fixtures; any
//! shape change bumps [`SCHEMA_VERSION`].

use crate::PdfOptions;
use crate::ast::Document;
use crate::caret::{CaretStyle, render_caret};
use crate::pdf::{RenderWarning, audit_anchors, render_warnings, verification_text_layer};
use crate::span::{DiagnosticSeverity, SourceSpan};

/// Report schema version (bump on any breaking shape change).
pub const SCHEMA_VERSION: &str = "1";

/// One actionable problem found during verification.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyFinding {
    /// Stable machine code (the render-warning code, or a verify-specific one
    /// such as `unresolved_anchor` / `overflow`).
    pub code: &'static str,
    /// Human-readable detail.
    pub detail: String,
}

/// The full verification report.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyReport {
    pub schema_version: &'static str,
    /// Which render surface was verified (v1: "pdf" only — the text layer is
    /// PDF-specific; HTML verification carries anchors/warnings/digest).
    pub target: &'static str,
    pub page_count: usize,
    /// FNV-1a 64 of the canonical report body (everything except this field).
    pub digest: u64,
    pub anchors_resolved: usize,
    /// Unresolved internal anchor targets (deduplicated, document order).
    pub anchors_unresolved: Vec<String>,
    /// All findings: unresolved anchors, render warnings, overflow runs.
    pub findings: Vec<VerifyFinding>,
    /// "clean" when findings is empty, otherwise "findings".
    pub verdict: &'static str,
    /// The text layer itself (per-page runs), for programmatic consumers.
    pub pages: Vec<crate::pdf::VerifyPage>,
}

/// Verify a parsed document against the PDF rendering pipeline. Returns `None`
/// when font loading fails (never panics).
#[must_use]
pub fn verify_pdf(doc: &Document, opts: &PdfOptions) -> Option<VerifyReport> {
    let layer = verification_text_layer(doc, opts)?;
    let audit = audit_anchors(doc);
    let warnings = render_warnings(doc, opts);

    let mut findings = Vec::new();
    for target in &audit.unresolved {
        findings.push(VerifyFinding {
            code: "unresolved_anchor",
            detail: format!("internal link target #{target} matches no heading"),
        });
    }
    for warning in &warnings {
        let detail = match warning {
            RenderWarning::UnresolvedImage(dest) => {
                format!("image {dest} had no matching asset; rendered as alt text")
            }
            RenderWarning::UnsupportedImage(dest) => {
                format!("image asset {dest} could not be decoded; rendered as alt text")
            }
            RenderWarning::MissingGlyphs { count, sample } => {
                format!("{count} character(s) have no glyph in any bundled face (sample: {sample})")
            }
            RenderWarning::FontWeightIgnoredStatic { slot, weight } => {
                format!("{slot} font-weight {weight} ignored on a static face")
            }
        };
        findings.push(VerifyFinding {
            code: warning.code(),
            detail,
        });
    }
    for page in &layer.pages {
        for run in &page.runs {
            if let Some(overshoot) = run.overshoot {
                findings.push(VerifyFinding {
                    code: "overflow",
                    detail: format!(
                        "page {} run exceeds the right margin by {:.3}pt: {}",
                        page.number, overshoot, run.text
                    ),
                });
            }
        }
    }

    findings.extend(audit_accessibility(doc));

    let verdict = if findings.is_empty() {
        "clean"
    } else {
        "findings"
    };
    let mut report = VerifyReport {
        schema_version: SCHEMA_VERSION,
        target: "pdf",
        page_count: layer.page_count,
        digest: 0,
        anchors_resolved: audit.resolved,
        anchors_unresolved: audit.unresolved,
        findings,
        verdict,
        pages: layer.pages,
    };
    // Digest covers the canonical body (every field except the digest itself),
    // so any content, anchor, warning, or overflow change moves it.
    let body = to_json_body(&report);
    report.digest = fnv1a64(body.as_bytes());
    Some(report)
}

/// Accessibility audit (bead jqls): authoring-time findings for the render
/// surfaces — missing alt text, heading-level jumps, generic link text,
/// tables without a header row. Codes are stable and additive; severity is
/// warning-class (verdict wording unchanged).
fn audit_accessibility(doc: &Document) -> Vec<VerifyFinding> {
    let mut out = Vec::new();
    audit_accessibility_blocks(&doc.blocks, &mut out, &mut None);
    out
}

fn audit_accessibility_blocks(
    blocks: &[crate::ast::Block],
    out: &mut Vec<VerifyFinding>,
    last_heading_level: &mut Option<u8>,
) {
    for block in blocks {
        match block {
            crate::ast::Block::Heading { level, inlines } => {
                if let Some(prev) = *last_heading_level
                    && *level > prev + 1
                {
                    out.push(VerifyFinding {
                        code: "heading_level_skip",
                        detail: format!(
                            "heading level jumps from H{prev} to H{level}: {}",
                            plain_inline_text(inlines)
                        ),
                    });
                }
                *last_heading_level = Some(*level);
                audit_accessibility_inlines(inlines, out);
            }
            crate::ast::Block::Paragraph(inlines) => audit_accessibility_inlines(inlines, out),
            crate::ast::Block::BlockQuote(inner) => {
                audit_accessibility_blocks(inner, out, last_heading_level);
            }
            crate::ast::Block::List(list) => {
                for item in &list.items {
                    audit_accessibility_blocks(&item.blocks, out, last_heading_level);
                }
            }
            crate::ast::Block::Table(table) => {
                let header_empty = table
                    .head
                    .iter()
                    .all(|cell| plain_inline_text(cell).trim().is_empty());
                if header_empty {
                    out.push(VerifyFinding {
                        code: "table_missing_header",
                        detail: "table has no header row text (screen readers lose column scope)"
                            .to_string(),
                    });
                }
                for cell in &table.head {
                    audit_accessibility_inlines(cell, out);
                }
                for row in &table.rows {
                    for cell in row {
                        audit_accessibility_inlines(cell, out);
                    }
                }
            }
            crate::ast::Block::DefinitionList(items) => {
                for item in items {
                    for term in &item.terms {
                        audit_accessibility_inlines(term, out);
                    }
                    for def in &item.definitions {
                        audit_accessibility_inlines(def, out);
                    }
                }
            }
            crate::ast::Block::CodeBlock { .. }
            | crate::ast::Block::ThematicBreak
            | crate::ast::Block::HtmlBlock(_)
            | crate::ast::Block::MathBlock(_)
            | crate::ast::Block::FootnoteDefinition { .. } => {}
        }
    }
}

fn audit_accessibility_inlines(inlines: &[crate::ast::Inline], out: &mut Vec<VerifyFinding>) {
    for inl in inlines {
        match inl {
            crate::ast::Inline::Image { alt, dest, .. } => {
                if alt.trim().is_empty() {
                    out.push(VerifyFinding {
                        code: "missing_alt_text",
                        detail: format!("image {dest} has empty alt text"),
                    });
                }
            }
            crate::ast::Inline::Link { content, .. } => {
                let text = plain_inline_text(content);
                let normalized = text.trim().to_ascii_lowercase();
                if matches!(
                    normalized.as_str(),
                    "click here" | "here" | "link" | "read more" | "learn more" | "this"
                ) {
                    out.push(VerifyFinding {
                        code: "generic_link_text",
                        detail: format!(
                            "link text \"{}\" is meaningless out of context",
                            text.trim()
                        ),
                    });
                }
                audit_accessibility_inlines(content, out);
            }
            crate::ast::Inline::Emphasis(children)
            | crate::ast::Inline::Strong(children)
            | crate::ast::Inline::Strikethrough(children) => {
                audit_accessibility_inlines(children, out);
            }
            crate::ast::Inline::Text(_)
            | crate::ast::Inline::Code(_)
            | crate::ast::Inline::SoftBreak
            | crate::ast::Inline::HardBreak
            | crate::ast::Inline::Html(_)
            | crate::ast::Inline::FootnoteRef { .. }
            | crate::ast::Inline::Math(_)
            | crate::ast::Inline::DisplayMath(_) => {}
        }
    }
}

fn plain_inline_text(inlines: &[crate::ast::Inline]) -> String {
    let mut out = String::new();
    for inl in inlines {
        match inl {
            crate::ast::Inline::Text(t) | crate::ast::Inline::Code(t) => out.push_str(t),
            crate::ast::Inline::Emphasis(c)
            | crate::ast::Inline::Strong(c)
            | crate::ast::Inline::Strikethrough(c) => out.push_str(&plain_inline_text(c)),
            crate::ast::Inline::Link { content, .. } => out.push_str(&plain_inline_text(content)),
            crate::ast::Inline::Image { alt, .. } => out.push_str(alt),
            crate::ast::Inline::SoftBreak | crate::ast::Inline::HardBreak => out.push(' '),
            crate::ast::Inline::Html(_)
            | crate::ast::Inline::FootnoteRef { .. }
            | crate::ast::Inline::Math(_)
            | crate::ast::Inline::DisplayMath(_) => {}
        }
    }
    out
}

/// Human-mode verify report: caret blocks for findings that map back into
/// `source`, plus a one-line summary. JSON consumers must keep using
/// [`to_json`] — this never writes to stdout and does not change the JSON
/// schema.
#[must_use]
pub fn to_human(
    report: &VerifyReport,
    source: &str,
    file: Option<&str>,
    style: CaretStyle,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "fmd verify: {} ({} page(s), {} finding(s))\n",
        report.verdict,
        report.page_count,
        report.findings.len()
    ));
    for finding in &report.findings {
        out.push('\n');
        if let Some(span) = finding_span(source, finding) {
            out.push_str(&render_caret(
                source,
                span,
                file,
                &format!("{}: {}", finding.code, finding.detail),
                DiagnosticSeverity::Error,
                style,
            ));
        } else {
            out.push_str(&format!("error: {}: {}\n", finding.code, finding.detail));
        }
    }
    out
}

fn finding_span(source: &str, finding: &VerifyFinding) -> Option<SourceSpan> {
    // Overflow details are `… by {n:.3}pt: {run.text}`. Split on the first
    // `pt: ` so a colon inside the run text is kept as part of the needle.
    if finding.code == "overflow" {
        if let Some((_, text)) = finding.detail.split_once("pt: ") {
            return find_source_span(source, text).or_else(|| {
                // Dictionary hyphenation appends a synthetic `-` that is not in
                // the Markdown; strip it so the caret can still find the run.
                text.strip_suffix('-')
                    .filter(|stem| !stem.is_empty())
                    .and_then(|stem| find_source_span(source, stem))
            });
        }
    }
    if finding.code == "unresolved_anchor" {
        // Detail is `internal link target #{id} matches no heading`. Do not
        // split on the first `#` then take the next word — an empty fragment
        // (`[x](#)`) produced the id "matches", and a bare `#{id}` search
        // highlights ATX headings.
        const PREFIX: &str = "internal link target #";
        const SUFFIX: &str = " matches no heading";
        let rest = finding.detail.strip_prefix(PREFIX)?;
        let id = rest.strip_suffix(SUFFIX).unwrap_or(rest);
        if id.is_empty() {
            return None;
        }
        return find_source_span(source, &format!("](#{id})"));
    }
    None
}

fn find_source_span(source: &str, needle: &str) -> Option<SourceSpan> {
    if needle.is_empty() {
        return None;
    }
    let start = source.find(needle)?;
    Some(SourceSpan::new(start, start.saturating_add(needle.len())))
}

/// Serialize the report to JSON (including the digest). Deterministic.
#[must_use]
pub fn to_json(report: &VerifyReport) -> String {
    let body = to_json_body(report);
    let digest_hex = format!("{:016x}", report.digest);
    body.replace(
        "\"digest\":\"\"",
        &format!("\"digest\":\"fnv1a64:{digest_hex}\""),
    )
}

fn to_json_body(report: &VerifyReport) -> String {
    let mut s = String::with_capacity(2048);
    s.push('{');
    s.push_str(&format!("\"schema_version\":\"{}\",", SCHEMA_VERSION));
    s.push_str(&format!("\"target\":\"{}\",", report.target));
    s.push_str(&format!("\"verdict\":\"{}\",", report.verdict));
    s.push_str(&format!("\"page_count\":{},", report.page_count));
    s.push_str("\"digest\":\"\",");
    s.push_str(&format!(
        "\"anchors\":{{\"resolved\":{},\"unresolved\":[{}]}},",
        report.anchors_resolved,
        report
            .anchors_unresolved
            .iter()
            .map(|t| format!("\"{}\"", json_escape_str(t)))
            .collect::<Vec<_>>()
            .join(",")
    ));
    s.push_str("\"findings\":[");
    let items: Vec<String> = report
        .findings
        .iter()
        .map(|f| {
            format!(
                "{{\"code\":\"{}\",\"detail\":\"{}\"}}",
                f.code,
                json_escape_str(&f.detail)
            )
        })
        .collect();
    s.push_str(&items.join(","));
    s.push_str("],");
    // Text layer: kept last (largest) so human readers of the JSON see the
    // verdict and findings first.
    s.push_str("\"pages\":[");
    let page_items: Vec<String> = report
        .pages
        .iter()
        .map(|page| {
            let run_items: Vec<String> = page
                .runs
                .iter()
                .map(|run| {
                    format!(
                        "{{\"text\":\"{}\",\"x\":{},\"y\":{},\"size\":{},\"kind\":\"{}\"}}",
                        json_escape_str(&run.text),
                        json_num(run.x),
                        json_num(run.y),
                        json_num(run.size),
                        run.kind
                    )
                })
                .collect();
            format!(
                "{{\"number\":{},\"runs\":[{}]}}",
                page.number,
                run_items.join(",")
            )
        })
        .collect();
    s.push_str(&page_items.join(","));
    s.push_str("]}");
    s
}

fn json_escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn json_num(value: f32) -> String {
    // Mirrors config::json_num: non-finite folds to 0 so the report always
    // parses; trailing zeros trimmed for stable, readable numbers.
    if !value.is_finite() {
        return "0".to_string();
    }
    let mut s = format!("{value:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() { "0".to_string() } else { s }
}

fn fnv1a64(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn digest_moves_when_findings_change() {
        let mut a = VerifyReport {
            schema_version: SCHEMA_VERSION,
            target: "pdf",
            page_count: 1,
            digest: 0,
            anchors_resolved: 0,
            anchors_unresolved: Vec::new(),
            findings: vec![VerifyFinding {
                code: "overflow",
                detail: "page 1 run exceeds the right margin by 1.000pt: x".to_string(),
            }],
            verdict: "findings",
            pages: Vec::new(),
        };
        let body_a = to_json_body(&a);
        a.findings.clear();
        a.verdict = "clean";
        let body_b = to_json_body(&a);
        assert_ne!(fnv1a64(body_a.as_bytes()), fnv1a64(body_b.as_bytes()));
    }

    #[test]
    fn unresolved_anchor_span_does_not_invent_id_matches() {
        let empty = VerifyFinding {
            code: "unresolved_anchor",
            detail: "internal link target # matches no heading".to_string(),
        };
        assert_eq!(
            finding_span("see [x](#) and [y](#matches)", &empty),
            None,
            "empty fragment must not highlight the word 'matches'"
        );
        let nope = VerifyFinding {
            code: "unresolved_anchor",
            detail: "internal link target #nope matches no heading".to_string(),
        };
        let src = "see [x](#t) and [bad](#nope)\n";
        let span = finding_span(src, &nope).expect("span");
        assert_eq!(&src[span.start..span.end], "](#nope)");
        let heading_only = VerifyFinding {
            code: "unresolved_anchor",
            detail: "internal link target #T matches no heading".to_string(),
        };
        assert_eq!(
            finding_span("# T\n\nsee [x](#missing)\n", &heading_only),
            None,
            "must not highlight an ATX heading that happens to match the id"
        );
    }

    #[test]
    fn overflow_span_strips_synthetic_hyphen() {
        let finding = VerifyFinding {
            code: "overflow",
            detail: "page 1 run exceeds the right margin by 1.250pt: hyphenation-".to_string(),
        };
        let src = "see hyphenation in the source\n";
        let span = finding_span(src, &finding).expect("span");
        assert_eq!(&src[span.start..span.end], "hyphenation");
    }

    #[test]
    fn json_escape_covers_control_chars_and_quotes() {
        assert_eq!(json_escape_str("a\"b\\c\nd\te"), "a\\\"b\\\\c\\nd\\te");
        assert_eq!(json_escape_str("\u{1}"), "\\u0001");
    }

    #[test]
    fn schema_shape_is_pinned_end_to_end() {
        // A tiny document exercising every report section: heading (anchor
        // target), a matching link, and a run on page 1. The JSON shape is
        // the contract — key order and structure are asserted, and the
        // digest is stable for identical inputs.
        let doc = crate::parse_markdown("# T\n\nsee [x](#t) and [bad](#nope)\n");
        let opts = PdfOptions::default();
        let report = verify_pdf(&doc, &opts).expect("verify runs");
        let json = to_json(&report);
        for key in [
            "\"schema_version\":\"1\"",
            "\"target\":\"pdf\"",
            "\"verdict\":\"findings\"",
            "\"anchors\":{\"resolved\":1,\"unresolved\":[\"nope\"]}",
            "\"code\":\"unresolved_anchor\"",
            "\"pages\":[{\"number\":1,",
            "\"kind\":\"heading\"",
        ] {
            assert!(json.contains(key), "missing {key} in:\n{json}");
        }
        assert!(json.contains("fnv1a64:"), "digest hex present");
        // Deterministic: same document, same JSON.
        let again = verify_pdf(&doc, &opts).expect("re-verify");
        assert_eq!(to_json(&again), json);
    }

    #[test]
    fn json_num_trims_trailing_zeros() {
        assert_eq!(json_num(72.0), "72");
        assert_eq!(json_num(72.5), "72.5");
        assert_eq!(json_num(f32::NAN), "0");
    }

    #[test]
    fn to_json_splices_digest_and_is_valid_json_shape() {
        let mut report = VerifyReport {
            schema_version: SCHEMA_VERSION,
            target: "pdf",
            page_count: 1,
            digest: 0xdead_beef_cafe_f00d,
            anchors_resolved: 2,
            anchors_unresolved: vec!["nope".to_string()],
            findings: Vec::new(),
            verdict: "clean",
            pages: Vec::new(),
        };
        report.digest = fnv1a64(to_json_body(&report).as_bytes());
        let json = to_json(&report);
        assert!(json.contains("\"digest\":\"fnv1a64:"), "digest spliced");
        assert!(json.starts_with('{') && json.ends_with('}'));
        assert!(!json.contains("\"digest\":\"\""), "no empty digest slot");
    }
}
