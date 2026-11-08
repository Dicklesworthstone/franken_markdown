//! Native execution of the same typed options and tokens used by the WASM ABI.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::{PdfOptions, Theme, render_pdf, wasm};

const SOURCE: &str = "First line.  \nSecond line.  \nThird line.\n\n| Heading | Value |\n| --- | --- |\n| Alpha | Beta |\n";

#[test]
fn spacing_tokens_reach_native_pdf_without_changing_html() {
    let mut options = wasm::WasmRenderOptions::default();
    options
        .apply_typography_tokens(Some(
            "theme-spacing,line-height=2,table-padding-x=1.25,table-padding-y=0.75",
        ))
        .unwrap();
    assert!(options.use_theme_spacing);
    assert_eq!(options.pdf_line_height, Some(2.0));
    assert_eq!(options.pdf_table_cell_padding_x_em, Some(1.25));
    assert_eq!(options.pdf_table_cell_padding_y_em, Some(0.75));
    let mut theme = Theme::default();
    theme.spacing.line_height = 2.0;
    theme.spacing.table_cell_padding_x_em = 1.25;
    theme.spacing.table_cell_padding_y_em = 0.75;
    let expected = render_pdf(
        SOURCE,
        &PdfOptions {
            theme,
            use_theme_spacing: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(wasm::render_pdf(SOURCE, &options).unwrap().bytes, expected);
    assert_ne!(
        expected,
        render_pdf(SOURCE, &PdfOptions::default()).unwrap()
    );
    assert_eq!(
        wasm::render_html(SOURCE, &options).unwrap().bytes,
        wasm::render_html(SOURCE, &wasm::WasmRenderOptions::default())
            .unwrap()
            .bytes,
        "PDF-only token overrides must not modify HTML presentation",
    );
}

#[test]
fn shared_theme_and_explicit_pdf_overrides_keep_font_scaling() {
    let mut theme = Theme::default();
    theme.spacing.line_height = 1.25;
    let mut options = wasm::WasmRenderOptions {
        theme: theme.clone(),
        font_scale: Some(1.25),
        use_theme_spacing: true,
        ..Default::default()
    };
    options
        .apply_typography_tokens(Some("table-padding-x=0,table-padding-y=0"))
        .unwrap();
    theme = theme.with_font_scale(franken_markdown::FontScale::from_factor(1.25));
    theme.spacing.table_cell_padding_x_em = 0.0;
    theme.spacing.table_cell_padding_y_em = 0.0;
    let expected = render_pdf(
        SOURCE,
        &PdfOptions {
            theme,
            use_theme_spacing: true,
            base_font_size: Some(13.75),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(wasm::render_pdf(SOURCE, &options).unwrap().bytes, expected);
}

#[test]
fn every_numeric_spacing_token_enables_literal_spacing_and_admits_host_precision() {
    for valid in [
        "line-height=1",
        "line-height=4",
        "table-padding-x=0",
        "table-padding-x=8",
        "table-padding-y=0",
        "table-padding-y=8",
        "line-height=2e0",
    ] {
        let mut options = wasm::WasmRenderOptions::default();
        options.apply_typography_tokens(Some(valid)).unwrap();
        assert!(options.use_theme_spacing, "{valid}");
    }
    for invalid in [
        "line-height=0.9999999999999999",
        "line-height=4.000000000000001",
        "line-height=NaN",
        "line-height=inf",
        "line-height=",
        "line-height=0x2",
        "table-padding-x=-0.1",
        "table-padding-y=8.000000000000002",
        "table-padding-y=Infinity",
        "unknown=2",
    ] {
        let mut options = wasm::WasmRenderOptions::default();
        assert!(
            options.apply_typography_tokens(Some(invalid)).is_err(),
            "{invalid}"
        );
        assert!(
            !options.use_theme_spacing,
            "invalid first token must not enable spacing: {invalid}"
        );
    }
}

#[test]
fn absent_settings_preserve_legacy_output_and_default_token_uses_the_shared_theme() {
    let expected = render_pdf(SOURCE, &PdfOptions::default()).unwrap();
    for tokens in [None, Some(""), Some(" , ")] {
        let mut options = wasm::WasmRenderOptions::default();
        options.apply_typography_tokens(tokens).unwrap();
        assert_eq!(wasm::render_pdf(SOURCE, &options).unwrap().bytes, expected);
    }
    let mut options = wasm::WasmRenderOptions::default();
    options
        .apply_typography_tokens(Some("theme-spacing"))
        .unwrap();
    assert_eq!(
        wasm::render_pdf(SOURCE, &options).unwrap().bytes,
        render_pdf(
            SOURCE,
            &PdfOptions {
                use_theme_spacing: true,
                ..Default::default()
            }
        )
        .unwrap(),
    );
}
