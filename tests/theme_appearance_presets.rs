//! Named appearances must select complete palettes for native publications.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    FontFamily, HtmlOptions, PdfOptions, SystemAppearance, Theme, ThemeColors, render_html,
    render_pdf,
};

const SOURCE: &str = "# Appearance\n\nReadable text.\n\n```rust\nfn example() {}\n```\n";

#[test]
fn appearance_names_round_trip_and_reject_noncanonical_configuration() {
    for (name, appearance) in [
        ("auto", SystemAppearance::Auto),
        ("light", SystemAppearance::Light),
        ("dark", SystemAppearance::Dark),
        ("charcoal", SystemAppearance::Charcoal),
        ("high-contrast-light", SystemAppearance::HighContrastLight),
        ("high-contrast-dark", SystemAppearance::HighContrastDark),
    ] {
        assert_eq!(name.parse::<SystemAppearance>(), Ok(appearance));
        assert_eq!(appearance.as_str(), name);
    }
    for invalid in [
        "",
        "system",
        "Dark",
        " dark",
        "dark ",
        "high_contrast_dark",
        "high-contrast",
    ] {
        assert!(
            invalid.parse::<SystemAppearance>().is_err(),
            "accepted noncanonical appearance {invalid:?}"
        );
    }
}

#[test]
fn parsed_presets_materialize_the_document_and_syntax_palettes() {
    for (name, background, foreground, keyword) in [
        ("light", "#ffffff", "#1f2328", "#cf222e"),
        ("dark", "#0d1117", "#e6edf3", "#ff7b72"),
        ("charcoal", "#1a1d21", "#e6edf3", "#ff7b72"),
        ("high-contrast-light", "#ffffff", "#000000", "#000000"),
        ("high-contrast-dark", "#000000", "#ffffff", "#ffffff"),
    ] {
        let theme = Theme::for_appearance(name.parse().unwrap());
        for host_dark in [false, true] {
            for host_high_contrast in [false, true] {
                let colors = theme.effective_colors(host_dark, host_high_contrast);
                assert_eq!(colors.bg, background, "{name}");
                assert_eq!(colors.fg, foreground, "{name}");
            }
        }
        let html = render_html(
            SOURCE,
            &HtmlOptions {
                theme,
                ..HtmlOptions::default()
            },
        )
        .unwrap();
        assert!(html.contains(&format!("--fmd-bg: {background};")), "{name}");
        assert!(html.contains(&format!("--fmd-fg: {foreground};")), "{name}");
        assert!(
            html.contains(&format!(".tok-kw {{ color: {keyword}; }}")),
            "{name}"
        );
        assert!(
            !html.contains("@media (prefers-color-scheme: dark)"),
            "explicit appearance {name} must stay fixed"
        );
    }
}

#[test]
fn automatic_preset_preserves_existing_html_and_pdf_bytes() {
    let theme = Theme::for_appearance("auto".parse().unwrap());
    assert_eq!(theme, Theme::default());
    assert_eq!(
        render_html(SOURCE, &HtmlOptions::default()).unwrap(),
        render_html(
            SOURCE,
            &HtmlOptions {
                theme: theme.clone(),
                ..HtmlOptions::default()
            }
        )
        .unwrap()
    );
    assert_eq!(
        render_pdf(SOURCE, &PdfOptions::default()).unwrap(),
        render_pdf(
            SOURCE,
            &PdfOptions {
                theme,
                ..PdfOptions::default()
            }
        )
        .unwrap()
    );
}

#[test]
fn appearance_preferences_retain_custom_theme_tokens() {
    let custom = Theme {
        font: FontFamily::Serif,
        colors: ThemeColors {
            bg: "#faf7ef".to_string(),
            fg: "#123456".to_string(),
            ..ThemeColors::light()
        },
        dark_colors: ThemeColors {
            bg: "#111b2a".to_string(),
            fg: "#fedcba".to_string(),
            ..ThemeColors::dark()
        },
        ..Theme::default()
    }
    .with_larger_text();
    for appearance in [
        SystemAppearance::Auto,
        SystemAppearance::Light,
        SystemAppearance::Dark,
        SystemAppearance::Charcoal,
        SystemAppearance::HighContrastLight,
        SystemAppearance::HighContrastDark,
    ] {
        let selected = custom.clone().with_appearance(appearance);
        assert_eq!(selected.colors, custom.colors);
        assert_eq!(selected.dark_colors, custom.dark_colors);
        assert_eq!(selected.font, FontFamily::Serif);
        assert_eq!(selected.spacing, custom.spacing);
    }
}
