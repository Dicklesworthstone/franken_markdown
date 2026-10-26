//! Pagination diagnostics must describe the PDF that was actually emitted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pagination::height::HeightPaginationError;
use franken_markdown::{
    PdfEmitOptions, PdfOptions, PdfPaginationFailure, PdfPaginationStrategy,
    PdfParagraphVariantStatus, RenderWarning, parse_markdown, render_pdf,
    render_pdf_document_reported, render_pdf_reported, render_warnings,
};
use std::collections::BTreeMap;

fn options(content_height: f32) -> PdfOptions {
    let mut opts = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = 400.0;
    opts.theme.page.size.height_pt = content_height + 72.0;
    opts.theme.page.margins = franken_markdown::theme::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    opts
}

/// Follow the emitted page references and decode only their content streams.
/// This does not consult layout, pagination, or verification helper results.
fn page_contents(pdf: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(pdf)
        .split("/Contents ")
        .skip(1)
        .map(|reference| {
            let number = reference.split_whitespace().next().unwrap();
            let header = format!("\n{number} 0 obj\n");
            let offset = pdf
                .windows(header.len())
                .position(|part| part == header.as_bytes())
                .unwrap()
                + header.len();
            let object = &pdf[offset..];
            let start = object
                .windows(8)
                .position(|part| part == b"\nstream\n")
                .unwrap();
            let dictionary = std::str::from_utf8(&object[..start]).unwrap();
            let length: usize = dictionary
                .split_once("/Length ")
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let stream = &object[start + 8..start + 8 + length];
            if dictionary.contains("/FlateDecode") {
                String::from_utf8(franken_markdown::zlib_decompress(stream, 1 << 24).unwrap())
                    .unwrap()
            } else {
                std::str::from_utf8(stream).unwrap().to_string()
            }
        })
        .collect()
}

/// Recover the one-face, single-character-line fixture through its actual
/// ToUnicode map and painted hex strings. A present font map alone cannot pass.
fn painted_letters(pdf: &[u8]) -> String {
    let map: BTreeMap<u16, char> = String::from_utf8_lossy(pdf)
        .lines()
        .filter_map(|line| {
            let (id, unicode) = line.strip_prefix('<')?.split_once("> <")?;
            let unicode = u32::from_str_radix(unicode.strip_suffix('>')?, 16).ok()?;
            Some((u16::from_str_radix(id, 16).ok()?, char::from_u32(unicode)?))
        })
        .collect();
    let mut painted = String::new();
    for content in page_contents(pdf) {
        for line in content.lines().filter(|line| line.contains(" TJ")) {
            let array = line.split_once('[').expect("TJ glyph array").1;
            for tail in array.split('<').skip(1) {
                let hex = tail.split('>').next().unwrap();
                for glyph in hex.as_bytes().chunks_exact(4) {
                    let id = u16::from_str_radix(std::str::from_utf8(glyph).unwrap(), 16).unwrap();
                    painted.push(*map.get(&id).expect("painted glyph must be selectable"));
                }
            }
        }
    }
    painted
}

#[test]
fn report_api_preserves_default_bytes_and_distinguishes_no_request() {
    let opts = PdfOptions::default();
    let result = render_pdf_reported("A", &opts).unwrap();
    assert_eq!(result.bytes, render_pdf("A", &opts).unwrap());
    assert!(!result.pagination.page_breaks.requested_optimal);
    assert_eq!(
        result.pagination.page_breaks.strategy,
        PdfPaginationStrategy::Greedy
    );
    assert!(result.pagination.page_breaks.fallback.is_none());
    assert_eq!(
        result.pagination.paragraph_variants.status,
        PdfParagraphVariantStatus::NotRequested
    );
    assert_eq!(painted_letters(&result.bytes), "A");
}

#[test]
fn exact_success_and_infeasible_fallback_report_the_actual_emitted_pages() {
    let source = "A  \nB  \nC";
    let doc = parse_markdown(source);
    let exact = render_pdf_reported(source, &options(100.0)).unwrap();
    assert!(exact.pagination.page_breaks.is_exact());
    assert_eq!(page_contents(&exact.bytes).len(), 1);
    assert_eq!(painted_letters(&exact.bytes), "ABC");

    // Three lines cannot split into two fragments of at least two lines, and
    // cannot fit together in this body area. The legacy pager preserves them.
    let opts = options(40.0);
    let fallback = render_pdf_reported(source, &opts).unwrap();
    assert_eq!(
        fallback.pagination.page_breaks.strategy,
        PdfPaginationStrategy::Legacy
    );
    assert!(matches!(
        fallback.pagination.page_breaks.fallback,
        Some(PdfPaginationFailure::Planner(
            HeightPaginationError::NoFeasibleLayout { .. }
        ))
    ));
    assert!(page_contents(&fallback.bytes).len() > 1);
    assert_eq!(painted_letters(&fallback.bytes), "ABC");
    assert_eq!(fallback.bytes, render_pdf(source, &opts).unwrap());
    let monolithic =
        render_pdf_document_reported(&doc, &opts, PdfEmitOptions::monolithic()).unwrap();
    assert_eq!(fallback, monolithic);
    let warning = render_warnings(&doc, &opts)
        .into_iter()
        .find(|warning| matches!(warning, RenderWarning::PaginationFallback { .. }))
        .unwrap();
    assert_eq!(warning.code(), "pagination_infeasible");
    assert!(warning.message().contains("legacy"));
}

#[test]
fn resource_budget_failure_is_not_mislabeled_as_infeasibility() {
    let opts = options(10_001.0);
    let rendered = render_pdf_reported("A", &opts).unwrap();
    assert_eq!(
        rendered.pagination.page_breaks.fallback,
        Some(PdfPaginationFailure::Planner(
            HeightPaginationError::BudgetExceeded("page capacity")
        ))
    );
    assert_eq!(
        rendered.pagination.page_breaks.strategy,
        PdfPaginationStrategy::Legacy
    );
    assert_eq!(painted_letters(&rendered.bytes), "A");
    assert!(
        render_warnings(&parse_markdown("A"), &opts)
            .iter()
            .any(|warning| warning.code() == "pagination_budget_exceeded")
    );
}

#[test]
fn footnote_fallback_and_browser_warning_match_the_public_render() {
    let source = "A[^n]  \nB  \nC\n\n[^n]: N";
    let opts = options(40.0);
    let rendered = render_pdf_reported(source, &opts).unwrap();
    assert_eq!(
        rendered.pagination.page_breaks.strategy,
        PdfPaginationStrategy::Greedy
    );
    assert!(matches!(
        rendered.pagination.page_breaks.fallback,
        Some(PdfPaginationFailure::Planner(
            HeightPaginationError::NoFeasibleLayout { .. }
        ))
    ));
    let warning = render_warnings(&parse_markdown(source), &opts)
        .into_iter()
        .find(|warning| matches!(warning, RenderWarning::PaginationFallback { .. }))
        .unwrap();
    assert!(matches!(&warning, RenderWarning::PaginationFallback {
        strategy: PdfPaginationStrategy::Greedy, reason,
    } if Some(reason) == rendered.pagination.page_breaks.fallback.as_ref()));
    let browser_opts = franken_markdown::wasm::WasmRenderOptions {
        theme: opts.theme,
        optimal_pagination: true,
        ..franken_markdown::wasm::WasmRenderOptions::default()
    };
    let browser = franken_markdown::wasm::render_pdf(source, &browser_opts).unwrap();
    assert_eq!(browser.bytes, rendered.bytes);
    assert!(
        browser
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == warning.message())
    );
}

#[test]
fn paragraph_limits_are_separate_from_exact_page_boundaries() {
    let source = ["word"; 600].join(" ");
    let opts = options(700.0);
    let rendered = render_pdf_reported(&source, &opts).unwrap();
    assert!(rendered.pagination.page_breaks.is_exact());
    assert!(rendered.pagination.paragraph_variants.paragraph_item_limit);
    assert_eq!(rendered.pagination.paragraph_variants.limited_paragraphs, 1);
    assert!(
        render_warnings(&parse_markdown(&source), &opts)
            .iter()
            .any(|warning| warning.code() == "paragraph_pagination_limited")
    );

    let mut opts = options(100.0);
    opts.gradual_demerits = true;
    let rendered = render_pdf_reported("A", &opts).unwrap();
    assert!(rendered.pagination.page_breaks.is_exact());
    assert_eq!(
        rendered.pagination.paragraph_variants.status,
        PdfParagraphVariantStatus::UnsupportedTypography
    );
}

#[test]
fn warning_preflight_uses_fitted_geometry_and_prepared_notes() {
    let source = "A  \nB  \nC";
    let mut opts = options(40.0);
    opts.fit_to_pages = Some(1);
    let rendered = render_pdf_reported(source, &opts).unwrap();
    let warning = render_warnings(&parse_markdown(source), &opts)
        .into_iter()
        .find(|warning| matches!(warning, RenderWarning::PaginationFallback { .. }));
    match (rendered.pagination.page_breaks.fallback.as_ref(), warning) {
        (None, None) => {}
        (Some(expected), Some(RenderWarning::PaginationFallback { reason, strategy })) => {
            assert_eq!(expected, &reason);
            assert_eq!(strategy, rendered.pagination.page_breaks.strategy);
        }
        other => panic!("preflight and final fitted render diverged: {other:?}"),
    }
    assert_eq!(painted_letters(&rendered.bytes), "ABC");
}
