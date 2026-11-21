#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    accessibility_audit, accessibility_audit_profiled, accessibility_audit_profiled_inner,
    document_stats, document_stats_profiled, document_stats_profiled_inner,
    parse_authoring_profile,
};
use crate::Profile;

#[test]
fn authoring_profile_admission_preserves_omission_and_rejects_unknown_values() {
    assert_eq!(parse_authoring_profile(None), Ok(None));
    assert_eq!(
        parse_authoring_profile(Some("commonmark-gfm")),
        Ok(Some(Profile::CommonMarkGfm))
    );
    assert_eq!(
        parse_authoring_profile(Some("gfm-plus")),
        Ok(Some(Profile::GfmPlus))
    );
    // No unknown spelling, accidental PDF/A selector or blank explicit value
    // may silently select the extension-enabled default.
    for value in ["", " ", "GFM-PLUS", "gfm", "default", "2b", "gfm-plus "] {
        assert!(parse_authoring_profile(Some(value)).is_err(), "{value:?}");
        // Exercise the same native helpers used by the exported bindings.
        // Invalid admission must happen before parsing or PDF layout, without
        // constructing a JsValue on a native test host.
        let expected = Err("profile must be commonmark-gfm or gfm-plus");
        assert_eq!(
            document_stats_profiled_inner("source", Some(value)),
            expected
        );
        assert_eq!(
            accessibility_audit_profiled_inner("source", Some(value)),
            expected
        );
    }
}

#[test]
fn omitted_analysis_profiles_preserve_the_exact_legacy_reports() {
    let source = "# Report\n\nTerm\n: Definition with [here](https://example.com).\n\n<!-- pagebreak -->\n\n## Evidence\n\nClaim[^n].\n\n[^n]: Supporting note.\n";
    assert_eq!(
        document_stats_profiled(source, None).unwrap(),
        document_stats(source)
    );
    assert_eq!(
        accessibility_audit_profiled(source, None).unwrap(),
        accessibility_audit(source).unwrap()
    );
    assert_eq!(
        accessibility_audit_profiled(source, Some("gfm-plus".into())).unwrap(),
        accessibility_audit(source).unwrap()
    );
}

#[test]
fn profiled_stats_parse_cross_line_delimiters_before_counting_structure() {
    let source = "[Term\n: definition](/target)\n";
    let commonmark = document_stats_profiled(source, Some("commonmark-gfm".into())).unwrap();
    let plus = document_stats_profiled(source, Some("gfm-plus".into())).unwrap();
    assert!(commonmark.contains("\"paragraphs\":1,"), "{commonmark}");
    assert!(commonmark.contains("\"links_total\":1,"), "{commonmark}");
    assert!(plus.contains("\"paragraphs\":0,"), "{plus}");
    assert!(plus.contains("\"links_total\":0,"), "{plus}");
    assert_eq!(plus, document_stats(source));
    for report in [&commonmark, &plus] {
        assert!(report.contains(&format!("\"bytes\":{},", source.len())));
    }
}

#[test]
fn profiled_stats_apply_the_authoring_page_break_transform() {
    let source = "Before.\n\n<!-- pagebreak -->\n\nAfter.\n";
    let plus = document_stats_profiled(source, Some("gfm-plus".into())).unwrap();
    // The print directive contributes no visible characters. Byte/line totals
    // still describe the complete source, including its comment spelling.
    let visible = crate::parse_markdown("Before.\n\nAfter.\n");
    assert_eq!(plus, crate::compute_doc_stats(source, &visible).to_json());
    assert!(plus.contains("\"characters\":13,"), "{plus}");
    // Omitting the new option retains the historical source-AST statistics.
    assert_eq!(
        document_stats_profiled(source, None).unwrap(),
        document_stats(source)
    );
    assert_ne!(plus, document_stats(source));
}

#[test]
fn profiled_accessibility_respects_code_spans_crossing_colon_lines() {
    let source = "`Term\n: ![](missing.png)`\n";
    let commonmark = accessibility_audit_profiled(source, Some("commonmark-gfm".into())).unwrap();
    let plus = accessibility_audit_profiled(source, Some("gfm-plus".into())).unwrap();
    assert!(
        !commonmark.contains("\"code\":\"missing_alt_text\""),
        "literal code must not be audited as an image: {commonmark}"
    );
    assert!(
        plus.contains("\"code\":\"missing_alt_text\""),
        "the extension grammar gives the definition its own image: {plus}"
    );
}

#[test]
fn profiled_accessibility_uses_the_matching_pdf_page_break_policy() {
    let source = "# First\n\nBefore.\n\n<!-- pagebreak -->\n\n# Second\n\nAfter.\n";
    let commonmark = accessibility_audit_profiled(source, Some("commonmark-gfm".into())).unwrap();
    let plus = accessibility_audit_profiled(source, Some("gfm-plus".into())).unwrap();
    assert!(commonmark.contains("\"page_count\":1,"), "{commonmark}");
    assert!(plus.contains("\"page_count\":2,"), "{plus}");
    assert_eq!(plus, accessibility_audit(source).unwrap());
    assert_eq!(
        commonmark,
        accessibility_audit_profiled(source, Some("commonmark-gfm".into())).unwrap()
    );
}
