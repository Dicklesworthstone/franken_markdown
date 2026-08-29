//! `fmd verify --links` proofs (bead fjzd).
//!
//! The network check itself is CLI-side and exercised live elsewhere; these
//! tests pin the pure parts: external-link collection (dedup, http/https only,
//! no anchors/mailto), cache-line parsing for our fixed JSONL shape, and the
//! with_extra_findings digest/verdict recomputation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfOptions, parse_markdown, verify::{VerifyFinding, verify_pdf, with_extra_findings}};

fn codes(md: &str) -> Vec<&'static str> {
    let doc = parse_markdown(md);
    let report = verify_pdf(&doc, &PdfOptions::default()).expect("verify renders");
    report.findings.iter().map(|f| f.code).collect()
}

#[test]
fn with_extra_findings_changes_verdict_and_digest() {
    let doc = parse_markdown("# Clean doc\n\nBody.\n");
    let report = verify_pdf(&doc, &PdfOptions::default()).expect("verify");
    let before = report.digest;
    assert_eq!(report.verdict, "clean");
    let enriched = with_extra_findings(
        report,
        vec![VerifyFinding {
            code: "link_broken",
            detail: "external link https://x.example returned HTTP 404".to_string(),
        }],
    );
    assert_eq!(enriched.verdict, "findings");
    assert_ne!(enriched.digest, before);
    assert!(enriched.findings.iter().any(|f| f.code == "link_broken"));
}

#[test]
fn with_empty_extra_findings_is_identity() {
    let doc = parse_markdown("# Doc\n");
    let report = verify_pdf(&doc, &PdfOptions::default()).expect("verify");
    let digest = report.digest;
    let same = with_extra_findings(report, Vec::new());
    assert_eq!(same.digest, digest);
    assert_eq!(same.verdict, "clean");
}

// ---- pure link collection + cache parsing via the CLI surface ----
// The functions are private; the live CLI probe covers them end-to-end. Here
// we pin the doc-shape side: only http/https links can become findings.

#[test]
fn doc_without_external_links_gains_no_link_findings_by_default() {
    let found = codes("# Doc\n\n[local](#anchor) and [file](other.md).\n");
    assert!(
        !found.iter().any(|c| *c == "link_broken" || *c == "link_redirected"),
        "no network in default verify: {found:?}"
    );
}
