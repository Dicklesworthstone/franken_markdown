#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::book::{BookInput, BookRenderer};
use crate::{HtmlFontFormat, PdfImageAsset, PdfOptions, PdfRunningBand, PdfRunningContent};

fn settings(tokens: Option<&str>) -> PdfSettings {
    PdfSettings::new(tokens, None, None, None, None, None, false, None,
        Vec::new(), false, false, false).unwrap()
}

fn book() -> BookRenderer {
    BookRenderer::new(&[
        BookInput { path: "a.md".into(), source: "# First\n\n[Next](b.md#second). Note[^n].\n\n[^n]: First note.\n".into() },
        BookInput { path: "b.md".into(), source: "# Second\n\nThe paragraph engine selects measured line breaks for the complete book.\n\n```rust\nlet answer = 42;\n```\n\n| Key | Value |\n| --- | --- |\n| A | B |\n".into() },
    ]).unwrap()
}

#[test]
fn individual_typography_switches_match_independently_configured_native_pdf() {
    for (token, mode) in [("homogeneous", 0), ("antiriver", 1), ("pareto", 2), ("optimal-pagination", 3)] {
        let mut book = book();
        settings(Some(token)).apply(book.options_mut());
        let mut native = PdfOptions::default();
        match mode {
            0 => native.gradual_demerits = true,
            1 => native.river_penalty = true,
            2 => native.pareto_line_breaking = true,
            _ => native.optimal_pagination = true,
        }
        let actual = book.render_pdf().unwrap();
        assert_eq!(actual, crate::book::render_book_pdf(book.book(), &native).unwrap(), "{token}");
        assert_eq!(actual, book.render_pdf().unwrap(), "determinism: {token}");
    }
}

#[test]
fn full_configuration_with_running_chrome_uses_the_native_book_pipeline() {
    let mut book = book();
    let source = book.book().chapters[0].doc.clone();
    let options = book.options_mut();
    options.title = Some("Publication".into());
    options.author = Some("Writer".into());
    options.toc = true;
    options.page_numbers = true;
    PdfSettings::new(
        Some("homogeneous,antiriver,pareto,optimal-pagination,protrusion"),
        Some(12.0), Some(1.3), Some(9.0), Some(3.0), None, true, Some(0.0),
        vec!["{title}".into(), "".into(), "{author}".into(), "".into(), "{page}/{pages}".into(), "{date}".into()],
        true, true, true,
    ).unwrap().apply(book.options_mut());
    let native = PdfOptions {
        title: Some("Publication".into()), author: Some("Writer".into()),
        toc: true, page_numbers: true, toc_depth: Some(3),
        base_font_size: Some(12.0), heading_scale: Some(1.3), table_font_size: Some(9.0),
        gradual_demerits: true, river_penalty: true, pareto_line_breaking: true,
        optimal_pagination: true, code_line_numbers: true, metadata_epoch_seconds: Some(0),
        microtype: crate::layout::MicrotypeOptions::CONSERVATIVE,
        running: PdfRunningContent {
            header: PdfRunningBand { left: Some("{title}".into()), right: Some("{author}".into()), rule: true, ..Default::default() },
            footer: PdfRunningBand { center: Some("{page}/{pages}".into()), right: Some("{date}".into()), rule: true, ..Default::default() },
            skip_first_page: true,
        },
        ..Default::default()
    };
    let actual = book.render_pdf().unwrap();
    assert_eq!(actual, crate::book::render_book_pdf(book.book(), &native).unwrap());
    assert_eq!(actual, book.render_pdf().unwrap());
    assert_eq!(book.book().chapters[0].doc, source);
}

#[test]
fn resetting_the_profile_preserves_metadata_theme_assets_and_source() {
    let mut options = WasmRenderOptions {
        title: Some("Title".into()), author: Some("Author".into()),
        custom_css: Some("body{}".into()), toc: true, page_numbers: true,
        font_scale: Some(1.25), html_font_format: HtmlFontFormat::Ttf,
        pdf_image_assets: vec![PdfImageAsset { destination: "image.png".into(), bytes: vec![1, 2, 3] }],
        ..WasmRenderOptions::default()
    };
    let bytes = options.pdf_image_assets[0].bytes.as_ptr();
    settings(Some("homogeneous,antiriver,pareto,optimal-pagination,expansion")).apply(&mut options);
    assert!(options.gradual_demerits && options.river_penalty && options.pareto_line_breaking && options.optimal_pagination);
    assert!(options.microtype.max_expansion_per_mille > 0);
    settings(None).apply(&mut options);
    assert!(!options.gradual_demerits && !options.river_penalty && !options.pareto_line_breaking && !options.optimal_pagination);
    assert_eq!(options.microtype.max_expansion_per_mille, 0);
    assert_eq!(options.pdf_image_assets[0].bytes.as_ptr(), bytes);
    assert_eq!(options.title.as_deref(), Some("Title"));
    assert_eq!(options.author.as_deref(), Some("Author"));
    assert_eq!(options.custom_css.as_deref(), Some("body{}"));
    assert_eq!(options.font_scale, Some(1.25));
    assert_eq!(options.html_font_format, HtmlFontFormat::Ttf);
    assert!(options.toc && options.page_numbers);
}

#[test]
fn invalid_tokens_do_not_partially_mutate_the_last_valid_profile() {
    let mut options = WasmRenderOptions::default();
    settings(Some("antiriver")).apply(&mut options);
    let before = format!("{options:?}");
    for token in ["homogeneous,unknown", "pareto,unknown", "optimal-pagination,unknown"] {
        let result = PdfSettings::new(Some(token), None, None, None, None, None,
            false, None, Vec::new(), false, false, false);
        assert!(result.is_err());
        assert_eq!(format!("{options:?}"), before);
    }
    let long = " ".repeat(257);
    assert!(PdfSettings::new(Some(&long), None, None, None, None, None,
        false, None, Vec::new(), false, false, false).is_err());
}

#[test]
fn numeric_admission_precedes_lossy_casts() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX, 5.999999999, 24.000000001] {
        assert!(size(Some(value), "baseFontSize", 6.0, 24.0).is_err());
    }
    assert_eq!(size(Some(6.0), "baseFontSize", 6.0, 24.0).unwrap(), Some(6.0));
    assert_eq!(size(Some(24.0), "baseFontSize", 6.0, 24.0).unwrap(), Some(24.0));
    for value in [f64::NAN, f64::INFINITY, -1.0, 0.0, 0.5, 4_294_967_296.0] {
        assert!(integer(Some(value), "fitToPages", 1, u64::from(u32::MAX)).is_err());
    }
    assert_eq!(integer(Some(4_294_967_295.0), "fitToPages", 1, u64::from(u32::MAX)).unwrap(), Some(u64::from(u32::MAX)));
    assert!(integer(Some(9_007_199_254_740_992.0), "epoch", 0, 9_007_199_254_740_991).is_err());
}

#[test]
fn band_admission_counts_utf8_bytes_and_keeps_exact_slot_order() {
    for slots in [vec!["one".into()], vec!["".into(); 7], vec!["é".repeat(2049); 6]] {
        assert!(PdfSettings::new(None, None, None, None, None, None,
            false, None, slots, false, false, false).is_err());
    }
    let settings = PdfSettings::new(None, None, None, None, None, None,
        false, None, vec!["é".repeat(2048), "".into(), "right".into(), "left".into(), "middle".into(), "end".into()],
        true, false, true).unwrap();
    let mut options = WasmRenderOptions::default();
    settings.apply(&mut options);
    assert_eq!(options.running.header.left.as_ref().unwrap().len(), 4096);
    assert_eq!(options.running.header.center, None);
    assert_eq!(options.running.header.right.as_deref(), Some("right"));
    assert_eq!(options.running.footer.center.as_deref(), Some("middle"));
    assert!(options.running.header.rule && options.running.skip_first_page);
    assert!(!options.running.footer.rule);
}

#[test]
fn default_profile_remains_byte_identical() {
    let mut book = book();
    let before = book.render_pdf().unwrap();
    settings(None).apply(book.options_mut());
    assert_eq!(book.render_pdf().unwrap(), before);
}
