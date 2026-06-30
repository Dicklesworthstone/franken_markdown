//! Shared theme/style model tests. Tests may unwrap for clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    DarkModePolicy, HtmlOptions, Theme, ThemeColors, ThemeSpacing, render_html,
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
