//! Verification must not admit a layout that the same PDF request refuses.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::verify::{verify_pdf, verify_pdf_result};
use franken_markdown::{
    BaselineGrid, PdfOptions, RenderError, parse_markdown, render_pdf_document, render_pdf_reported,
};

fn tight_options() -> PdfOptions {
    let mut opts = PdfOptions {
        base_font_size: Some(24.0),
        strict_pagination: true,
        ..Default::default()
    };
    opts.theme.page.margins.top_pt = 360.0;
    opts.theme.page.margins.bottom_pt = 360.0;
    opts
}

#[test]
fn strict_verification_refuses_the_same_infeasible_paragraph_as_publication() {
    // Three lines in a 72pt body cannot split into fragments of at least two.
    let doc = parse_markdown("A  \nB  \nC");
    let opts = tight_options();
    let published = render_pdf_document(&doc, &opts).unwrap_err();
    let verified = verify_pdf_result(&doc, &opts).unwrap_err();
    assert!(matches!(verified, RenderError::Pagination(_)));
    assert_eq!(verified.code(), "pagination_infeasible");
    assert_eq!(verified.to_string(), published.to_string());
    assert!(verify_pdf(&doc, &opts).is_none());
    assert!(franken_markdown::pdf::verification_text_layer(&doc, &opts).is_none());
}

#[test]
fn best_effort_verification_retains_a_typed_fallback_finding() {
    let doc = parse_markdown("A  \nB  \nC");
    let mut opts = tight_options();
    opts.strict_pagination = false;
    opts.optimal_pagination = true;
    assert!(
        render_pdf_document(&doc, &opts)
            .unwrap()
            .starts_with(b"%PDF-")
    );
    let report = verify_pdf_result(&doc, &opts).unwrap();
    assert_eq!(report.verdict, "findings");
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "pagination_infeasible")
    );
    for text in ["A", "B", "C"] {
        assert!(
            report
                .pages
                .iter()
                .flat_map(|page| &page.runs)
                .any(|run| run.text == text)
        );
    }
}

#[test]
fn running_bands_are_subject_to_publication_admission() {
    let doc = parse_markdown("An ordinary body paragraph.");
    let mut opts = PdfOptions::default();
    opts.theme.page.margins.top_pt = 0.0;
    opts.running.header.left = Some("Document header".into());
    let published = render_pdf_document(&doc, &opts).unwrap_err();
    let verified = verify_pdf_result(&doc, &opts).unwrap_err();
    assert_eq!(verified.code(), published.code());
    assert_eq!(verified.to_string(), published.to_string());
    assert!(verify_pdf(&doc, &opts).is_none());
}

#[test]
fn incompatible_grid_options_retain_the_actual_error() {
    let doc = parse_markdown("A paragraph with incompatible layout options.");
    let mut opts = PdfOptions {
        fit_to_pages: Some(1),
        ..Default::default()
    };
    opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
    let published = render_pdf_document(&doc, &opts).unwrap_err();
    let verified = verify_pdf_result(&doc, &opts).unwrap_err();
    assert_eq!(verified.to_string(), published.to_string());
    assert!(
        verified
            .to_string()
            .contains("pdf_baseline_grid_incompatible")
    );
}

#[test]
fn valid_strict_grid_and_running_content_share_emitted_page_count() {
    let source = "# Verification\n\nA complete paragraph with a note.[^n]\n\n\
                  A second paragraph preserves body order.\n\n[^n]: A useful note.\n";
    let mut opts = PdfOptions {
        strict_pagination: true,
        ..Default::default()
    };
    opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
    opts.running.footer.center = Some("{page} / {pages}".into());
    let published = render_pdf_reported(source, &opts).unwrap();
    assert!(published.pagination.page_breaks.is_exact());
    let doc = parse_markdown(source);
    let verified = verify_pdf_result(&doc, &opts).unwrap();
    let page_marker = b"/Type /Page ";
    let emitted_pages = published
        .bytes
        .windows(page_marker.len())
        .filter(|window| *window == page_marker)
        .count();
    assert!(emitted_pages > 0);
    assert_eq!(verified.page_count, emitted_pages);
    assert_eq!(verify_pdf(&doc, &opts).unwrap(), verified);
    let layer = franken_markdown::pdf::verification_text_layer_result(&doc, &opts).unwrap();
    assert_eq!(layer.pages, verified.pages);
    let text: String = verified
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect();
    assert!(text.contains("A useful note."));
    let first = text
        .find("complete paragraph")
        .expect("first paragraph retained");
    let second = text
        .find("second paragraph")
        .expect("second paragraph retained");
    assert!(first < second);
}

#[test]
fn authoring_profile_is_applied_to_the_verified_document() {
    let source = "Term\n: definition content\n\nA reference.[^n]\n\n\
                  [^n]: Note term\n    : note definition\n";
    let doc = parse_markdown(source);
    let opts = PdfOptions {
        profile: Some(franken_markdown::Profile::CommonMarkGfm),
        ..Default::default()
    };
    let rendered = render_pdf_document(&doc, &opts).unwrap();
    let report = verify_pdf_result(&doc, &opts).unwrap();
    let layer = franken_markdown::pdf::verification_text_layer_result(&doc, &opts).unwrap();
    assert_eq!(layer.pages, report.pages);
    let text = report
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains(": definition content"), "{text}");
    assert!(text.contains(": note definition"), "{text}");
    let marker = b"/Type /Page ";
    assert_eq!(
        report.page_count,
        rendered
            .windows(marker.len())
            .filter(|window| *window == marker)
            .count()
    );
}
