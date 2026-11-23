//! Page fitting must start from the admitted dimensions and retain all paint.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use franken_markdown::{
    PageMargins, PdfEmitOptions, PdfOptions, PdfPageEmission, parse_markdown, render_pdf,
    render_pdf_document_emitted, render_pdf_reported,
};

fn hard_lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("ROW{index:02}"))
        .collect::<Vec<_>>()
        .join("  \n")
}

fn small_options() -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.page.size.width_pt = 240.0;
    options.theme.page.size.height_pt = 144.0;
    options.theme.page.margins = PageMargins {
        top_pt: 12.0,
        right_pt: 24.0,
        bottom_pt: 6.0,
        left_pt: 0.0,
    };
    options
}

fn glyphs(contents: &[String]) -> Vec<u16> {
    contents
        .iter()
        .flat_map(|content| {
            pdf_content::operations(content)
                .into_iter()
                .filter(|operation| matches!(operation.operator, "Tj" | "TJ"))
                .flat_map(|operation| pdf_content::glyph_ids(&operation))
        })
        .collect()
}

fn positions(contents: &[String]) -> Vec<[f32; 2]> {
    contents
        .iter()
        .flat_map(|content| {
            pdf_content::operations(content)
                .into_iter()
                .filter(|operation| operation.operator == "Tm")
                .map(|operation| {
                    [
                        operation.operands[4].parse().unwrap(),
                        operation.operands[5].parse().unwrap(),
                    ]
                })
        })
        .collect()
}

fn font_sizes(contents: &[String]) -> Vec<f32> {
    contents
        .iter()
        .flat_map(|content| {
            pdf_content::operations(content)
                .into_iter()
                .filter(|operation| operation.operator == "Tf")
                .map(|operation| operation.operands[1].parse().unwrap())
        })
        .collect()
}

#[test]
fn fitting_keeps_compact_asymmetric_margins_instead_of_growing_them() {
    let source = hard_lines(13);
    for strict in [false, true] {
        let mut options = small_options();
        options.strict_pagination = strict;
        let ordinary = render_pdf(&source, &options).unwrap();
        let original_content = pdf_content::page_content_streams(&ordinary);
        assert!(original_content.len() > 1, "the source needs fitting");

        options.fit_to_pages = Some(1);
        let fitted = render_pdf_reported(&source, &options).unwrap();
        let content = pdf_content::page_content_streams(&fitted.bytes);
        assert_eq!(content.len(), 1, "the authored body can fit all 13 rows");
        if strict {
            assert!(fitted.pagination.page_breaks.is_exact());
        }
        assert_eq!(glyphs(&content), glyphs(&original_content));
        let positions = positions(&content);
        assert_eq!(positions.len(), 13, "every hard line is still painted");
        for [x, y] in positions {
            assert_eq!(x, 0.0, "the authored zero left margin must survive fitting");
            assert!((6.0..=132.0).contains(&y), "body baseline {y}");
        }
        assert!(
            font_sizes(&content)
                .iter()
                .all(|size| (6.0..11.0).contains(size))
        );
        let monolithic = render_pdf_document_emitted(
            &parse_markdown(&source),
            &options,
            PdfEmitOptions {
                emission: PdfPageEmission::Monolithic,
                ..PdfEmitOptions::default()
            },
        )
        .unwrap();
        assert_eq!(fitted.bytes, monolithic);
        assert_eq!(fitted.bytes, render_pdf(&source, &options).unwrap());
    }
}

#[test]
fn fitting_measures_the_reachable_lower_endpoint_before_missing_its_budget() {
    let source = hard_lines(10);
    let mut options = PdfOptions {
        base_font_size: Some(24.0),
        use_theme_spacing: true,
        ..PdfOptions::default()
    };
    options.theme.spacing.line_height = 2.0;
    options.theme.page.size.width_pt = 400.0;
    options.theme.page.size.height_pt = 312.0;
    options.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    let original = render_pdf(&source, &options).unwrap();
    let original_content = pdf_content::page_content_streams(&original);
    assert!(original_content.len() > 1);

    // Ten 24pt line boxes occupy the full 240pt body. A bisection probe even
    // slightly above 12pt type cannot fit; the reachable endpoint itself can.
    let mut explicit = options.clone();
    explicit.base_font_size = Some(12.0);
    let expected = render_pdf(&source, &explicit).unwrap();
    assert_eq!(pdf_content::page_content_streams(&expected).len(), 1);
    options.fit_to_pages = Some(1);
    let fitted = render_pdf(&source, &options).unwrap();
    assert_eq!(fitted, expected);
    let content = pdf_content::page_content_streams(&fitted);
    assert_eq!(glyphs(&content), glyphs(&original_content));
    assert_eq!(positions(&content).len(), 10);
    assert!(font_sizes(&content).iter().all(|size| *size == 12.0));
}

#[test]
fn fitting_uses_sanitized_font_overrides_without_panicking() {
    let source = format!(
        "{}\n\n| Label | Value |\n| --- | --- |\n| TABLE | CONTENT |\n",
        hard_lines(13)
    );
    for (raw_base, raw_table, base, table) in [
        (f32::NAN, 10.0, 11.0, 10.0),
        (f32::INFINITY, 10.0, 11.0, 10.0),
        (f32::NEG_INFINITY, 10.0, 11.0, 10.0),
        (11.0, f32::NAN, 11.0, 10.0),
        (11.0, f32::INFINITY, 11.0, 10.0),
        (48.0, 48.0, 24.0, 24.0),
    ] {
        let mut raw = small_options();
        raw.base_font_size = Some(raw_base);
        raw.table_font_size = Some(raw_table);
        raw.fit_to_pages = Some(1);
        let mut admitted = raw.clone();
        admitted.base_font_size = Some(base);
        admitted.table_font_size = Some(table);
        assert_eq!(
            render_pdf(&source, &raw).unwrap(),
            render_pdf(&source, &admitted).unwrap(),
            "fitting should use the same admitted type as ordinary layout: {raw_base}, {raw_table}"
        );
    }
}

#[test]
fn overlarge_font_overrides_can_shrink_from_their_actual_rendered_size() {
    let source = hard_lines(7);
    let mut options = small_options();
    options.base_font_size = Some(48.0);
    options.table_font_size = Some(48.0);
    assert!(pdf_content::page_content_streams(&render_pdf(&source, &options).unwrap()).len() > 1);
    options.fit_to_pages = Some(1);
    let pdf = render_pdf(&source, &options).unwrap();
    let content = pdf_content::page_content_streams(&pdf);
    assert_eq!(content.len(), 1);
    assert_eq!(positions(&content).len(), 7);
    assert!(
        font_sizes(&content)
            .iter()
            .all(|size| (12.0..24.0).contains(size))
    );
}

#[test]
fn fitting_uses_the_sanitized_page_margins() {
    let source = hard_lines(13);
    for (top, admitted_top, admitted_bottom) in [
        (-1.0, 72.0, 6.0),
        (f32::NAN, 72.0, 6.0),
        (f32::INFINITY, 72.0, 6.0),
        (1000.0, 104.0, 0.0),
    ] {
        let mut raw = small_options();
        raw.theme.page.margins.top_pt = top;
        raw.fit_to_pages = Some(1);
        // The ordinary renderer resolves invalid margins to their 72pt
        // defaults and clamps excessive margins to retain a 40pt body.
        let mut admitted = raw.clone();
        admitted.theme.page.margins.top_pt = admitted_top;
        admitted.theme.page.margins.bottom_pt = admitted_bottom;
        assert_eq!(
            render_pdf(&source, &raw).unwrap(),
            render_pdf(&source, &admitted).unwrap(),
            "fitting must use the original rendered page geometry: {top}"
        );
    }
}

#[test]
fn disabled_or_already_satisfied_fitting_preserves_original_bytes() {
    let source = hard_lines(4);
    let mut options = small_options();
    let original = render_pdf(&source, &options).unwrap();
    assert_eq!(pdf_content::page_content_streams(&original).len(), 1);
    for target in [0, 1, 2] {
        options.fit_to_pages = Some(target);
        assert_eq!(render_pdf(&source, &options).unwrap(), original);
    }
}
