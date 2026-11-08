//! Book findings must describe the chapter-resolved PDF actually emitted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer, render_book_pdf_reported};
use franken_markdown::{
    PdfASettings, PdfOptions, PdfPaginationStrategy, PdfParagraphVariantStatus, RenderWarning,
};

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn small_page(height: f32) -> PdfOptions {
    let mut options = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 400.0;
    options.theme.page.size.height_pt = height + 72.0;
    options.theme.page.margins = franken_markdown::theme::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options
}

// Follow the emitted xref and inspect actual annotation dictionaries. Outline
// destinations or delimiter-like bytes inside font streams cannot satisfy this.
fn assert_bound_navigation(bytes: &[u8], minimum_links: usize) {
    let start = bytes
        .windows(9)
        .rposition(|part| part == b"startxref")
        .unwrap();
    let offset: usize = std::str::from_utf8(&bytes[start + 9..])
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut lines = std::str::from_utf8(&bytes[offset..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let count: usize = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(lines.next().unwrap().trim(), "0000000000 65535 f");
    let mut positions: Vec<usize> = lines
        .take(count - 1)
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            assert_eq!(&fields[1..], &["00000", "n"]);
            fields[0].parse().unwrap()
        })
        .collect();
    positions.push(offset);
    let objects: Vec<_> = positions
        .windows(2)
        .map(|range| {
            let object = &bytes[range[0]..range[1]];
            let end = object
                .windows(7)
                .position(|part| part == b"stream\n")
                .unwrap_or(object.len());
            String::from_utf8_lossy(&object[..end])
        })
        .collect();
    let mut linked = 0;
    for object in objects
        .iter()
        .filter(|object| object.contains("/Subtype /Link"))
    {
        assert!(
            !object.contains("/S /URI"),
            "chapter links must become internal destinations"
        );
        let destination = object
            .split_once("/Dest [")
            .expect("bound annotation")
            .1
            .split(']')
            .next()
            .unwrap();
        let values: Vec<_> = destination.split_whitespace().collect();
        assert_eq!(values.len(), 7);
        assert_eq!(&values[1..5], &["0", "R", "/XYZ", "null"]);
        assert_eq!(values[6], "null");
        let page: usize = values[0].parse().unwrap();
        let top: f32 = values[5].parse().unwrap();
        assert!(objects[page - 1].contains("/Type /Page "));
        assert!(top.is_finite() && top > 0.0);
        linked += 1;
    }
    assert!(linked >= minimum_links);
    assert!(!bytes.windows(17).any(|part| part == b"fmd-book-pdf-nav/"));
}

#[test]
fn findings_use_chapter_assets_and_published_notes_without_changing_navigation_or_bytes() {
    let mut book = BookRenderer::new(&[
        input(
            "guide/one.md",
            "# One\n\n[Next](../two.md#two). Note[^n].\n\n![good](good.svg)\n\n![broken](broken.png)\n\n![missing](missing.png)\n\n\\fmnNotACommand is ordinary text; $\\fmnNotACommand{x}$ is a math fallback. \u{13000}\n\n[^n]: Published note.\n\n[^unused]: ![orphan](orphan.png)\n",
        ),
        input("two.md", "# Two\n\n[Back](guide/one.md#one)."),
    ])
    .unwrap();
    book.set_image(
        "guide/good.svg",
        br#"<svg width="20" height="10"><rect width="20" height="10"/></svg>"#.to_vec(),
    )
    .unwrap();
    book.set_image("guide/broken.png", vec![0, 1, 2, 3]).unwrap();
    let report = book.render_pdf_reported().unwrap();
    assert_eq!(report.bytes, book.render_pdf().unwrap());
    assert_bound_navigation(&report.bytes, 2);
    assert!(report.warnings.iter().any(|warning| {
        matches!(warning, RenderWarning::UnsupportedImage(path) if path == "guide/broken.png")
    }));
    assert!(report.warnings.iter().any(|warning| {
        matches!(warning, RenderWarning::UnresolvedImage(path) if path == "missing.png")
    }));
    for code in ["missing_glyphs", "math_fallback"] {
        assert!(report.warnings.iter().any(|warning| warning.code() == code));
    }
    // Unreferenced notes remain complete in the PDF's endnote fallback. Their
    // degraded figures therefore need the same finding as body figures.
    assert!(report.warnings.iter().any(|warning| {
        matches!(warning, RenderWarning::UnresolvedImage(path) if path == "orphan.png")
    }));
    assert!(
        report
            .warnings
            .iter()
            .all(|warning| !warning.message().contains("good.svg"))
    );
}

#[test]
fn final_pagination_fallback_and_paragraph_limits_survive_the_book_pipeline() {
    let book = BookRenderer::new(&[input("one.md", "A  \nB  \nC")]).unwrap();
    let options = small_page(40.0);
    let report = render_book_pdf_reported(book.book(), &options).unwrap();
    assert_eq!(
        report.pagination.page_breaks.strategy,
        PdfPaginationStrategy::Legacy
    );
    assert!(report.pagination.page_breaks.fallback.is_some());
    let warnings: Vec<_> = report
        .warnings
        .iter()
        .filter_map(|warning| match warning {
            RenderWarning::PaginationFallback { strategy, reason } => Some((strategy, reason)),
            _ => None,
        })
        .collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!(*warnings[0].0, report.pagination.page_breaks.strategy);
    assert_eq!(
        Some(warnings[0].1),
        report.pagination.page_breaks.fallback.as_ref()
    );
    assert!(
        String::from_utf8_lossy(&report.bytes)
            .matches("/Contents ")
            .count()
            > 1
    );
    assert_eq!(
        report.bytes,
        book.render_pdf_with_options(&options, PdfASettings::OFF)
            .unwrap()
    );

    let mut exact = small_page(100.0);
    exact.gradual_demerits = true;
    let report = book
        .render_pdf_with_options_reported(&exact, PdfASettings::OFF)
        .unwrap();
    assert!(report.pagination.page_breaks.is_exact());
    assert_eq!(
        report.pagination.paragraph_variants.status,
        PdfParagraphVariantStatus::UnsupportedTypography
    );
    assert_eq!(
        report
            .warnings
            .iter()
            .filter(|warning| warning.code() == "paragraph_pagination_limited")
            .count(),
        1
    );
}

#[test]
fn strict_reports_refuse_the_same_failure_and_export_options_are_local() {
    let book = BookRenderer::new(&[input("one.md", "A  \nB  \nC")]).unwrap();
    let original = book.render_pdf().unwrap();
    let mut options = small_page(40.0);
    options.strict_pagination = true;
    let error = book
        .render_pdf_with_options_reported(&options, PdfASettings::OFF)
        .unwrap_err();
    assert_eq!(error.code(), "pagination_infeasible");
    assert_eq!(
        error.code(),
        book.render_pdf_with_options(&options, PdfASettings::OFF)
            .unwrap_err()
            .code()
    );
    assert_eq!(book.render_pdf().unwrap(), original);
}

#[test]
fn archival_and_fitted_reports_keep_the_actual_writer_provenance() {
    let book = BookRenderer::new(&[input(
        "one.md",
        "A[^n]  \nB  \nC\n\n[^n]: Note.",
    )])
    .unwrap();
    let mut options = small_page(40.0);
    options.fit_to_pages = Some(1);
    for settings in [
        PdfASettings::OFF,
        PdfASettings::a2b(),
        PdfASettings::a2b_strict(),
    ] {
        let report = book
            .render_pdf_with_options_reported(&options, settings)
            .unwrap();
        assert_eq!(
            report.bytes,
            book.render_pdf_with_options(&options, settings).unwrap()
        );
        let fallback = report.warnings.iter().find_map(|warning| match warning {
            RenderWarning::PaginationFallback { strategy, reason } => Some((*strategy, reason)),
            _ => None,
        });
        assert_eq!(
            fallback.map(|(_, reason)| reason),
            report.pagination.page_breaks.fallback.as_ref()
        );
        if let Some((strategy, _)) = fallback {
            assert_eq!(strategy, report.pagination.page_breaks.strategy);
        }
        if settings.mode.is_a2b() {
            assert!(String::from_utf8_lossy(&report.bytes).contains("/OutputIntents"));
        }
    }
}

#[test]
fn clean_audited_books_have_no_fabricated_render_findings() {
    let book = BookRenderer::new(&[input("one.md", "# One\n\nPlain body.")]).unwrap();
    let report = book.render_pdf_reported().unwrap();
    assert!(report.warnings.is_empty());
    assert!(!report.pagination.page_breaks.requested_optimal);
    assert_eq!(report.bytes, book.render_pdf().unwrap());
    assert_eq!(
        book.render_pdf_pdfa_reported(PdfASettings::OFF).unwrap(),
        report
    );
}

#[cfg(feature = "wasm-book")]
#[test]
fn retained_reported_abi_uses_the_owned_result_and_truthful_unlocated_findings() {
    let source = "# One\n\n![missing](missing.png)";
    let book = franken_markdown::book::browser::FmdBook::new(
        vec!["one.md".into()],
        vec![source.into()],
    )
    .unwrap();
    let ordinary = book.render_pdf_reported().unwrap();
    assert_eq!(ordinary.format(), "pdf");
    assert_eq!(ordinary.mime_type(), "application/pdf");
    assert_eq!(ordinary.source_length(), source.len());
    assert_eq!(ordinary.bytes(), book.render_pdf().unwrap());
    let findings = ordinary.diagnostics_json();
    assert!(findings.contains("\"code\":\"unresolved_image\""));
    assert!(findings.contains("\"start\":0,\"end\":0"));
    let geometry = vec![540.0, 720.0, 36.0, 30.0, 24.0, 18.0];
    let page = book.render_pdf_with_page_reported(geometry.clone()).unwrap();
    assert_eq!(
        page.bytes(),
        book.render_pdf_with_page(geometry.clone()).unwrap()
    );
    assert_eq!(page.diagnostics_json(), findings);
    let archival = book
        .render_pdf_archival_reported("2b", true, geometry.clone())
        .unwrap();
    assert_eq!(
        archival.bytes(),
        book.render_pdf_archival("2b", true, geometry).unwrap()
    );
    assert_eq!(archival.diagnostics_json(), findings);
    assert_eq!(book.source_revision(), 0);
}

#[cfg(feature = "wasm-bindgen")]
#[test]
fn one_shot_book_abi_preserves_findings_and_chapter_navigation() {
    let first = "# One\n\n[Next](two.md#two). ![missing](missing.png)";
    let second = "# Two\n\n[Back](one.md#one)";
    let result = franken_markdown::wasm_abi::render_book_pdf(
        vec!["one.md".into(), "two.md".into()],
        vec![first.into(), second.into()],
        None,
        None,
        None,
        None,
        None,
        false,
    )
    .unwrap();
    assert_eq!(result.format(), "book-pdf");
    assert_eq!(result.source_length(), first.len() + second.len());
    assert!(
        result
            .diagnostics_json()
            .contains("\"code\":\"unresolved_image\"")
    );
    assert_bound_navigation(&result.bytes(), 2);
}
