//! Shared theme/style model tests. Tests may unwrap for clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    DarkModePolicy, HtmlOptions, PageSize, Theme, ThemeColors, ThemeSpacing, render_html,
};

#[test]
fn theme_config_json_exposes_stable_wasm_and_cli_contract() {
    let json = Theme::default().to_config_json();

    assert!(json.starts_with('{'));
    assert!(json.ends_with('}'));
    assert!(json.contains("\"font\":\"sans\""));
    assert!(json.contains("\"mono_font\":\"documentation\""));
    assert!(json.contains("\"code_theme\":\"github\""));
    assert!(json.contains("\"dark_mode\":\"auto\""));
    assert!(json.contains("\"base_px\":16"));
    assert!(json.contains("\"max_width_px\":760"));
    assert!(json.contains("\"page\""));
    assert!(json.contains("\"name\":\"letter\""));
}

#[test]
fn theme_config_json_escapes_public_page_size_name() {
    let theme = Theme {
        page: franken_markdown::PageStyle {
            size: PageSize {
                name: "letter\"x\n\u{0001}",
                width_pt: 612.0,
                height_pt: 792.0,
            },
            ..franken_markdown::PageStyle::default()
        },
        ..Theme::default()
    };

    let json = theme.to_config_json();
    assert!(
        json.contains("\"name\":\"letter\\\"x\\n \""),
        "page size name must be JSON-escaped: {json}"
    );
    assert!(
        !json.contains("\"name\":\"letter\"x"),
        "raw quotes would break theme JSON: {json}"
    );
}

#[test]
fn serif_theme_keeps_the_high_quality_serif_stack() {
    let opts = HtmlOptions {
        theme: Theme::serif(),
        ..HtmlOptions::default()
    };
    let html = render_html("# Title", &opts).unwrap();

    assert!(html.contains("Source Serif 4"));
    assert!(html.contains("Newsreader"));
}

#[test]
fn typed_color_and_spacing_tokens_drive_default_css() {
    let theme = Theme {
        colors: ThemeColors {
            accent: "#cc3355".to_string(),
            code_bg: "#f0f7ff".to_string(),
            ..ThemeColors::light()
        },
        spacing: ThemeSpacing {
            max_width_px: 680,
            line_height: 1.62,
            radius_px: 7,
            table_cell_padding_y_em: 0.6,
            table_cell_padding_x_em: 0.9,
            ..ThemeSpacing::default()
        },
        ..Theme::default()
    };
    let html = render_html(
        "# Styled",
        &HtmlOptions {
            theme,
            ..HtmlOptions::default()
        },
    )
    .unwrap();

    assert!(html.contains("--fmd-accent: #cc3355;"));
    assert!(html.contains("--fmd-code-bg: #f0f7ff;"));
    assert!(html.contains("--fmd-measure: 680px;"));
    assert!(html.contains("--fmd-line-height: 1.62;"));
    assert!(html.contains("--fmd-radius: 7px;"));
    assert!(html.contains("--fmd-table-pad-y: 0.6em;"));
    assert!(html.contains("--fmd-table-pad-x: 0.9em;"));
}

#[test]
fn dark_mode_policy_can_emit_light_only_css() {
    let html = render_html(
        "# Light",
        &HtmlOptions {
            theme: Theme::default().with_dark_mode(DarkModePolicy::Disabled),
            ..HtmlOptions::default()
        },
    )
    .unwrap();

    assert!(!html.contains("@media (prefers-color-scheme: dark)"));
}

// --- grn.2.8: small-module coverage for the theme model ---------------------

#[test]
fn dark_mode_policy_as_str_covers_both_variants() {
    assert_eq!(DarkModePolicy::Auto.as_str(), "auto");
    assert_eq!(DarkModePolicy::Disabled.as_str(), "disabled");
}

#[test]
fn theme_colors_default_is_the_light_palette() {
    let def = ThemeColors::default();
    let light = ThemeColors::light();
    assert_eq!(def, light);
    // And the dark palette is genuinely different.
    assert_ne!(def.bg, ThemeColors::dark().bg);
}

#[test]
fn theme_sans_constructor_equals_default_and_serif_differs() {
    assert_eq!(Theme::sans().font, franken_markdown::FontFamily::Sans);
    assert_eq!(Theme::sans(), Theme::default());
    assert_ne!(Theme::serif().font, Theme::sans().font);
}

#[test]
fn pdf_options_type_scale_defaults_reproduce_legacy_ladder() {
    let opts = franken_markdown::PdfOptions::default();
    let scale = opts.type_scale();
    assert_eq!(scale.h, [24.0, 19.0, 16.0, 13.5, 12.0, 11.0]);
    assert_eq!(scale.body, 11.0);
    assert_eq!(scale.code, 9.5);
    assert_eq!(scale.table, 10.0);
}

#[test]
fn pdf_options_type_scale_applies_overrides() {
    let mut opts = franken_markdown::PdfOptions {
        base_font_size: Some(16.5),
        ..Default::default()
    };
    let scale = opts.type_scale();
    assert_eq!(scale.body, 16.5);
    assert!((scale.h[0] - 36.0).abs() < 1e-4);
    // Explicit heading ratio switches to the geometric ladder.
    opts.heading_scale = Some(1.333);
    let geometric = opts.type_scale();
    for pair in geometric.h.windows(2) {
        assert!(pair[0] > pair[1]);
    }
    // Table override respects the documented floor.
    opts.table_font_size = Some(1.0);
    assert_eq!(opts.type_scale().table, 5.0);
}

#[test]
fn wasm_typography_builders_set_fields_and_default_stays_none() {
    use franken_markdown::wasm::WasmRenderOptions;
    let base = WasmRenderOptions::sans();
    assert_eq!(base.base_font_size, None);
    assert_eq!(base.heading_scale, None);
    assert_eq!(base.table_font_size, None);
    let tuned = WasmRenderOptions::serif()
        .with_base_font_size(13.0)
        .with_heading_scale(1.25)
        .with_table_font_size(8.5);
    assert_eq!(tuned.base_font_size, Some(13.0));
    assert_eq!(tuned.heading_scale, Some(1.25));
    assert_eq!(tuned.table_font_size, Some(8.5));
}

#[test]
fn typography_overrides_render_deterministically_and_differ_from_default() {
    use franken_markdown::{PdfOptions, render_pdf_document};
    let doc = franken_markdown::parse_markdown("# Title\n\nbody text\n");
    let render = |opts: &PdfOptions| render_pdf_document(&doc, opts).expect("render");
    let default = render(&PdfOptions::default());
    assert_eq!(
        default,
        render(&PdfOptions::default()),
        "same options must be byte-deterministic"
    );
    let bigger = PdfOptions {
        base_font_size: Some(20.0),
        ..Default::default()
    };
    let enlarged = render(&bigger);
    assert_ne!(
        default, enlarged,
        "a larger base font must move the layout tree"
    );
}

#[test]
fn typography_table_override_changes_render_output() {
    // Guards 45d2.5 acceptance: PdfOptions.table_font_size must actually
    // reach PDF table layout, not just exist as an inert field. Dense grid
    // maximizes cell-size influence on column allocation and wrapping.
    use franken_markdown::{PdfOptions, render_pdf_document};
    let md = "| A | B | C |\n|:--|:--|:--|\n| alpha | beta | gamma |\n";
    let doc = franken_markdown::parse_markdown(md);
    let render = |opts: &PdfOptions| render_pdf_document(&doc, opts).expect("render");
    let default = render(&PdfOptions::default());
    assert_eq!(default, render(&PdfOptions::default()));
    let bigger = PdfOptions {
        table_font_size: Some(14.0),
        ..Default::default()
    };
    assert_ne!(
        default,
        render(&bigger),
        "table_font_size must reach layout_table_uncached"
    );
}

#[test]
fn typography_table_override_affects_dense_adaptive_table() {
    use franken_markdown::{PdfOptions, render_pdf_document};
    let md = "| Col1 | Col2 | Col3 | Col4 | Col5 | Col6 |\n|:--|:--|:--|:--|:--|:--|\n| very long content alpha | beta | gamma | delta | epsilon | zeta |\n";
    let doc = franken_markdown::parse_markdown(md);
    let render = |opts: &PdfOptions| render_pdf_document(&doc, opts).expect("render");
    let default = render(&PdfOptions::default());
    let custom = PdfOptions {
        table_font_size: Some(12.0),
        ..Default::default()
    };
    assert_ne!(
        default,
        render(&custom),
        "table_font_size override must propagate through adaptive table font scaling"
    );
}
