//! Accessibility findings in fmd verify (bead jqls).
//!
//! The verify pipeline already audits anchors, warnings, and overflow; these
//! tests pin the additive accessibility findings: missing alt text, heading
//! level jumps, generic link text, and headerless tables — plus their absence
//! on a clean document.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfOptions, parse_markdown, verify::verify_pdf};

fn codes(md: &str) -> Vec<&'static str> {
    let doc = parse_markdown(md);
    let report = verify_pdf(&doc, &PdfOptions::default()).expect("verify renders");
    report.findings.iter().map(|f| f.code).collect()
}

#[test]
fn missing_alt_text_flagged() {
    let found = codes("# Doc\n\n![](images/chart.png)\n");
    assert!(found.contains(&"missing_alt_text"), "got {found:?}");
}

#[test]
fn nonempty_alt_is_clean_of_alt_findings() {
    let found = codes("# Doc\n\n![Chart of revenue](images/chart.png)\n");
    assert!(!found.contains(&"missing_alt_text"), "got {found:?}");
}

#[test]
fn heading_level_jump_flagged() {
    let found = codes("# Top\n\n### Skipped a level\n");
    assert!(found.contains(&"heading_level_skip"), "got {found:?}");
}

#[test]
fn sequential_headings_clean() {
    let found = codes("# Top\n\n## Next\n\n### Third\n");
    assert!(!found.contains(&"heading_level_skip"), "got {found:?}");
}

#[test]
fn generic_link_text_flagged() {
    let found = codes("# Doc\n\n[click here](https://example.com) for details.\n");
    assert!(found.contains(&"generic_link_text"), "got {found:?}");
}

#[test]
fn descriptive_link_text_clean() {
    let found = codes("# Doc\n\n[the release notes](https://example.com) for details.\n");
    assert!(!found.contains(&"generic_link_text"), "got {found:?}");
}

#[test]
fn headerless_table_flagged() {
    let found = codes("# Doc\n\n|   |   |\n|---|---|\n| 1 | 2 |\n");
    assert!(found.contains(&"table_missing_header"), "got {found:?}");
}

#[test]
fn table_with_header_clean() {
    let found = codes("# Doc\n\n| Name | Value |\n|---|---|\n| a | 1 |\n");
    assert!(!found.contains(&"table_missing_header"), "got {found:?}");
}

#[test]
fn clean_document_has_no_a11y_findings() {
    let found = codes(
        "# Title\n\n## Section\n\nBody with [the docs](https://example.com) and ![alt](a.png).\n\n| A | B |\n|---|---|\n| 1 | 2 |\n",
    );
    for code in [
        "missing_alt_text",
        "heading_level_skip",
        "generic_link_text",
        "table_missing_header",
    ] {
        assert!(!found.contains(&code), "unexpected {code} in {found:?}");
    }
}

#[test]
fn filter_a11y_keeps_only_a11y_codes() {
    let doc = parse_markdown("# Top\n\n### Jump\n\n![ ](x.png)\n\n[missing](#nowhere)\n");
    let report = verify_pdf(&doc, &PdfOptions::default()).expect("verify");
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "unresolved_anchor")
    );
    let filtered = franken_markdown::verify::filter_a11y(report);
    assert!(filtered.findings.iter().all(|f| matches!(
        f.code,
        "missing_alt_text" | "heading_level_skip" | "generic_link_text" | "table_missing_header"
    )));
    assert!(
        filtered
            .findings
            .iter()
            .any(|f| f.code == "missing_alt_text")
    );
    assert!(
        filtered
            .findings
            .iter()
            .any(|f| f.code == "heading_level_skip")
    );
    assert_eq!(filtered.verdict, "findings");
}

#[test]
fn math_in_table_header_is_clean() {
    let found = codes("# Doc\n\n| $f(x)$ | $g(x)$ |\n|---|---|\n| 1 | 2 |\n");
    assert!(!found.contains(&"table_missing_header"), "got {found:?}");
}

#[test]
fn footnote_definitions_are_audited_for_a11y() {
    let found = codes(
        "# Doc\n\nSee note[^1].\n\n[^1]: ![](missing.png) and [click here](https://example.com)\n",
    );
    assert!(found.contains(&"missing_alt_text"), "got {found:?}");
    assert!(found.contains(&"generic_link_text"), "got {found:?}");
}
