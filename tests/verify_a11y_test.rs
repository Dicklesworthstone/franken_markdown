//! Accessibility findings in fmd verify (bead jqls).
//!
//! The verify pipeline already audits anchors, warnings, and overflow; these
//! tests pin the additive accessibility findings: missing alt text, heading
//! level jumps, generic link text, and headerless tables — plus their absence
//! on a clean document.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::verify::{VerifyReport, filter_a11y, to_json, verify_pdf, verify_pdf_result};
use franken_markdown::{PdfOptions, Profile, parse_markdown, parse_markdown_with_profile};

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

fn a11y_report(source: &str, options: &PdfOptions) -> VerifyReport {
    let document = parse_markdown_with_profile(source, options.profile);
    filter_a11y(verify_pdf_result(&document, options).expect("verify renders"))
}

#[test]
fn supported_html_reports_the_same_accessibility_findings_as_markdown() {
    let html = "<h1>Top</h1>\n\n<h3>Skipped a level</h3>\n\n\
                <p><a href='https://example.com'>click <strong>here</strong></a> \
                <img src='missing.png' alt=''></p>\n\n\
                <table><tr><th></th><th></th></tr>\
                <tr><td>1</td><td>2</td></tr></table>\n";
    let markdown = "# Top\n\n### Skipped a level\n\n\
                    [click **here**](https://example.com) ![](missing.png)\n\n\
                    |   |   |\n|---|---|\n| 1 | 2 |\n";
    let options = PdfOptions::default();
    let expected = a11y_report(markdown, &options);
    assert_eq!(
        expected
            .findings
            .iter()
            .map(|finding| finding.code)
            .collect::<Vec<_>>(),
        [
            "heading_level_skip",
            "generic_link_text",
            "missing_alt_text",
            "table_missing_header",
        ]
    );
    let actual = a11y_report(html, &options);
    assert_eq!(actual.findings, expected.findings);
    assert_eq!(actual.verdict, "findings");
    assert!(actual.pages.iter().any(|page| {
        page.runs
            .iter()
            .any(|run| run.text.contains("Skipped a level"))
    }));
}

#[test]
fn inline_html_in_quotes_lists_and_notes_is_audited_once() {
    let source = "# Body\n\n\
                  > Quote <a href='https://example.com'>click here</a> \
                  <img src='quote.png' alt=''>\n\n\
                  - List <a href='https://example.com'>read more</a> \
                  <img src='list.png' alt=''>\n\n\
                  Claim.[^n] Repeated claim.[^n]\n\n\
                  [^n]: Note <a href='https://example.com'>learn more</a> \
                  <img src='note.png' alt=''>\n";
    for footnote_backlinks in [false, true] {
        let options = PdfOptions {
            footnote_backlinks,
            ..Default::default()
        };
        let report = a11y_report(source, &options);
        assert_eq!(report.findings.len(), 6, "{:?}", report.findings);
        let image_details: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.code == "missing_alt_text")
            .map(|finding| finding.detail.as_str())
            .collect();
        assert_eq!(
            image_details,
            [
                "image quote.png has empty alt text",
                "image list.png has empty alt text",
                "image note.png has empty alt text",
            ]
        );
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.code == "generic_link_text")
                .count(),
            3
        );
    }
}

#[test]
fn html_note_headings_keep_their_source_scope() {
    // An uncited note is published in the fallback Notes section. Its first
    // heading does not inherit the generated H2 or the body's H1 context.
    let source = "# Body\n\nBody text.\n\n\
                  [^appendix]: <h5>Appendix heading</h5><h6>Appendix detail</h6>\
                  <p><a href='https://example.com'>click here</a></p>\n";
    let report = a11y_report(source, &PdfOptions::default());
    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    assert_eq!(report.findings[0].code, "generic_link_text");
    let text = report
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Notes"), "{text}");
    assert!(text.contains("Appendix heading"), "{text}");
}

#[test]
fn literal_html_in_code_comments_and_escaped_text_is_not_audited() {
    let sources = [
        "# Top\n\n```html\n<h3>Hidden heading</h3>\n\
         <a href='https://example.com'>click here</a>\n<img src='code.png'>\n```\n",
        "# Top\n\nLiteral `<img src='code.png'>` and \
         `<a href='https://example.com'>click here</a>`.\n",
        "# Top\n\n&lt;h3&gt;Hidden heading&lt;/h3&gt; \
         &lt;img src='escaped.png'&gt; \
         &lt;a href='https://example.com'&gt;click here&lt;/a&gt;\n",
        "# Top\n\n\\<img src='escaped.png'> and \
         \\<a href='https://example.com'>click here\\</a>.\n",
        "# Top\n\n<!-- <h3>Hidden heading</h3>\n\
         <a href='https://example.com'>click here</a><img src='comment.png'> -->\n",
        "# Top\n\n<pre><code>&lt;h3&gt;Hidden heading&lt;/h3&gt; \
         &lt;a href='https://example.com'&gt;click here&lt;/a&gt; \
         &lt;img src='code.png'&gt;</code></pre>\n",
    ];
    for source in sources {
        let report = a11y_report(source, &PdfOptions::default());
        assert!(
            report.findings.is_empty(),
            "{source}: {:?}",
            report.findings
        );
        assert_eq!(report.verdict, "clean");
    }
}

#[test]
fn profiled_html_audits_preserve_source_and_deterministic_reports() {
    let source = "# Top\n\nBefore.\n\n<!-- pagebreak -->\n\n\
                  <h3>After</h3><p><a href='https://example.com'>click here</a> \
                  <img src='missing.png' alt=''></p>\n";
    for (profile, expected_pages) in [
        (None, 2),
        (Some(Profile::CommonMarkGfm), 1),
        (Some(Profile::GfmPlus), 2),
    ] {
        let document = parse_markdown_with_profile(source, profile);
        let original = document.clone();
        let options = PdfOptions {
            profile,
            ..Default::default()
        };
        let report = filter_a11y(verify_pdf_result(&document, &options).unwrap());
        assert_eq!(report.page_count, expected_pages);
        assert_eq!(
            report
                .findings
                .iter()
                .map(|finding| finding.code)
                .collect::<Vec<_>>(),
            [
                "heading_level_skip",
                "generic_link_text",
                "missing_alt_text"
            ]
        );
        let repeated = filter_a11y(verify_pdf_result(&document, &options).unwrap());
        assert_eq!(to_json(&report), to_json(&repeated));
        assert_eq!(document, original);
    }
}
