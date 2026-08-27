//! fmd verify — machine-readable render verification (beads yo83.1-3).
//!
//! Builds a stable-schema JSON report from the same layout+pagination pipeline
//! the PDF writer uses: per-page text runs, internal-anchor audit, render
//! warnings, and horizontal overflow findings, digested with FNV-1a 64 (a
//! non-cryptographic change detector — no crypto dependencies by doctrine).
//!
//! The JSON contract (schema version 1) is pinned by golden fixtures; any
//! shape change bumps [`SCHEMA_VERSION`].

use crate::ast::Document;
use crate::pdf::{
    audit_anchors, render_warnings, verification_text_layer, PdfOptions, RenderWarning,
    VerifyTextLayer,
};

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
            RenderWarning::MissingGlyphs { count, sample } => format!(
                "{count} character(s) have no glyph in any bundled face (sample: {sample})"
            ),
        };
        findings.push(VerifyFinding { code: warning.code(), detail });
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

    let verdict = if findings.is_empty() { "clean" } else { "findings" };
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

/// Serialize the report to JSON (including the digest). Deterministic.
#[must_use]
pub fn to_json(report: &VerifyReport) -> String {
    let body = to_json_body(report);
    let digest_hex = format!("{:016x}", report.digest);
    body.replace("\"digest\":\"\"", &format!("\"digest\":\"fnv1a64:{digest_hex}\""))
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
    if s.is_empty() {
        "0".to_string()
    } else {
        s
    }
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
    fn json_escape_covers_control_chars_and_quotes() {
        assert_eq!(json_escape_str("a\"b\\c\nd\te"), "a\\\"b\\\\c\\nd\\te");
        assert_eq!(json_escape_str("\u{1}"), "\\u0001");
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
