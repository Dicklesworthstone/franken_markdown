//! Exercise theme choices through real, decoded EPUB archives and the CLI.

use super::{entry_text, parse_central_directory};
use franken_markdown::{
    BookInput, CodeLigatures, DarkModePolicy, FontAssetSlot, FontAssets, FontFamily, FontScale,
    HtmlOptions, SystemAppearance, Theme, build_book, parse_markdown, render_epub,
};

const SOURCE: &str = "# Theme\n\n## Second\n\n### Third\n\n#### Fourth\n\n##### Fifth\n\n###### Sixth\n\nBody and `code`.\n\n> Quoted text.\n\n| A | B |\n| - | - |\n| One | Two |\n\n```rust\nlet value = 42;\n```\n";

fn publications(opts: &HtmlOptions) -> [Vec<u8>; 2] {
    let book = build_book(&[
        BookInput {
            path: "first.md".into(),
            source: SOURCE.into(),
        },
        BookInput {
            path: "second.md".into(),
            source: "# Last\n\nLast chapter.\n".into(),
        },
    ])
    .unwrap();
    [
        render_epub(&parse_markdown(SOURCE), opts).unwrap(),
        franken_markdown::epub::render_book_epub(&book, opts).unwrap(),
    ]
}

fn identifier(archive: &[u8]) -> String {
    let opf = entry_text(archive, "OEBPS/content.opf");
    opf.split_once("<dc:identifier id=\"bookid\">")
        .unwrap()
        .1
        .split_once("</dc:identifier>")
        .unwrap()
        .0
        .to_string()
}

#[test]
fn selected_family_and_uniform_scale_change_both_publications_and_their_identity() {
    let default = HtmlOptions::default();
    let originals = publications(&default);
    for (family, scale) in [(FontFamily::Serif, 1.0), (FontFamily::Sans, 2.0)] {
        let opts = HtmlOptions {
            theme: Theme::default()
                .with_font(family)
                .with_font_scale(FontScale::from_factor(scale)),
            ..HtmlOptions::default()
        };
        let changed = publications(&opts);
        assert_eq!(
            entry_text(&changed[0], "OEBPS/style.css"),
            entry_text(&changed[1], "OEBPS/style.css")
        );
        for (index, archive) in changed.iter().enumerate() {
            assert_ne!(archive, &originals[index]);
            assert_ne!(identifier(archive), identifier(&originals[index]));
            assert_eq!(
                entry_text(archive, "OEBPS/chapter-1.xhtml"),
                entry_text(&originals[index], "OEBPS/chapter-1.xhtml")
            );
            let css = entry_text(archive, "OEBPS/style.css");
            assert!(css.contains(if family == FontFamily::Serif {
                "body{font-family:serif;"
            } else {
                "body{font-family:sans-serif;"
            }));
            assert!(css.contains(if scale == 2.0 {
                "font-size:200%;"
            } else {
                "font-size:100%;"
            }));
            // All sizes inherit the body's scale exactly once. In particular,
            // pre > code cannot apply the code/body ratio a second time.
            for rule in [
                "h1{font-size:2.182em;}",
                "h2{font-size:1.727em;}",
                "h3{font-size:1.455em;}",
                "h4{font-size:1.227em;}",
                "h5{font-size:1.091em;}",
                "h6{font-size:1em;}",
                "pre,code,kbd,samp{font-family:monospace;font-size:0.864em;}",
                "pre code{font-size:1em;",
                "table{border-collapse:collapse;font-size:0.909em;",
            ] {
                assert!(css.contains(rule), "missing {rule}");
            }
        }
        assert_eq!(
            changed,
            publications(&opts),
            "theme-dependent output remains deterministic"
        );
    }
}

#[test]
fn navigation_and_every_chapter_link_the_same_theme_without_embedded_fonts() {
    for archive in publications(&HtmlOptions::default()) {
        let entries = parse_central_directory(&archive);
        assert!(
            !entries
                .iter()
                .any(|entry| entry.name.starts_with("OEBPS/fonts/"))
        );
        for entry in entries
            .iter()
            .filter(|entry| entry.name.ends_with(".xhtml"))
        {
            let page = entry_text(&archive, &entry.name);
            assert_eq!(
                page.matches("href=\"style.css\"").count(),
                1,
                "{}",
                entry.name
            );
            assert!(page.find("href=\"style.css\"").unwrap() < page.find("</head>").unwrap());
        }
    }
}

#[test]
fn theme_spacing_and_color_tokens_reach_the_reader_stylesheet() {
    let mut opts = HtmlOptions::default();
    opts.theme.dark_mode = DarkModePolicy::Disabled;
    opts.theme.spacing.line_height = 1.9;
    opts.theme.spacing.table_cell_padding_y_em = 0.75;
    opts.theme.spacing.table_cell_padding_x_em = 1.25;
    opts.theme.spacing.radius_px = 12;
    opts.theme.colors.fg = "#123456".into();
    opts.theme.colors.bg = "rgb(240 241 242 / 95%)".into();
    opts.theme.colors.accent = "hsl(220deg 80% 40%)".into();
    opts.theme.colors.quote_bar = "navy".into();
    opts.theme.colors.stripe = "#abc".into();
    for archive in publications(&opts) {
        let css = entry_text(&archive, "OEBPS/style.css");
        for value in [
            "line-height:1.9;",
            "padding:0.75em 1.25em;",
            "border-radius:0.75em;",
            "body{color:#123456;background-color:rgb(240 241 242 / 95%);}",
            "a{color:hsl(220deg 80% 40%);}",
            "border-left-color:navy;",
            "background-color:#abc;",
        ] {
            assert!(css.contains(value), "missing {value}");
        }
        assert!(!css.contains("@media"));
        assert!(
            !css.contains("var("),
            "reader compatibility does not require CSS custom properties"
        );
    }
}

#[test]
fn automatic_and_explicit_appearances_use_the_correct_light_and_dark_palettes() {
    let mut opts = HtmlOptions::default();
    opts.theme.colors.bg = "#fefefe".into();
    opts.theme.dark_colors.bg = "#121212".into();
    let auto = publications(&opts);
    for archive in auto {
        let css = entry_text(&archive, "OEBPS/style.css");
        let (light, dark) = css
            .split_once("@media (prefers-color-scheme: dark){")
            .unwrap();
        assert!(light.contains("background-color:#fefefe;"));
        assert!(dark.contains("background-color:#121212;"));
        assert!(light.contains(".tok-kw{color:#cf222e;}"));
        assert!(dark.contains(".tok-kw{color:#ff7b72;}"));
    }
    for (appearance, background, scheme) in [
        (SystemAppearance::Dark, "#121212", "dark"),
        (SystemAppearance::Light, "#fefefe", "light"),
    ] {
        opts.theme.appearance = appearance;
        for archive in publications(&opts) {
            let css = entry_text(&archive, "OEBPS/style.css");
            assert!(
                !css.contains("@media"),
                "an explicit appearance stays fixed"
            );
            assert!(css.contains(&format!("background-color:{background};")));
            assert!(css.contains(&format!("color-scheme:{scheme};")));
        }
    }
    for theme in [Theme::high_contrast_light(), Theme::high_contrast_dark()] {
        opts.theme = theme;
        for archive in publications(&opts) {
            let css = entry_text(&archive, "OEBPS/style.css");
            assert!(
                !css.contains(".tok-kw{color:"),
                "token hues cannot weaken high contrast"
            );
            assert!(css.contains(".tok-kw{font-weight:bold;}"));
            assert!(css.contains(".tok-cm{font-style:italic;}"));
        }
    }
}

#[test]
fn malformed_theme_values_cannot_inject_css_or_nonfinite_sizes() {
    let mut opts = HtmlOptions::default();
    opts.theme.colors.fg = "red;}@import url(https://example.invalid/a);/*".into();
    opts.theme.colors.bg = "url(https://example.invalid/b)".into();
    opts.theme.colors.border = "</style><script>bad()</script>".into();
    opts.theme.colors.accent = "\\72 ed".into();
    opts.theme.dark_colors.bg = "\";background-image:url(//example.invalid/c)".into();
    opts.theme.spacing.line_height = f32::NAN;
    opts.theme.spacing.table_cell_padding_y_em = f32::INFINITY;
    opts.theme.spacing.table_cell_padding_x_em = -1.0;
    opts.theme.spacing.base_px = 0;
    for archive in publications(&opts) {
        let css = entry_text(&archive, "OEBPS/style.css");
        for forbidden in [
            "@import",
            "url(",
            "example.invalid",
            "<script",
            "</style",
            "\\72",
            "NaN",
            "inf",
            "font-size:0%",
        ] {
            assert!(!css.contains(forbidden), "unsafe value: {forbidden}");
        }
        assert!(css.contains("body{color:#1f2328;background-color:#ffffff;}"));
        assert!(css.contains("line-height:1.7;"));
        assert!(css.contains("padding:0.55em 0.85em;"));
    }
}

#[test]
fn custom_css_is_verbatim_and_replaces_theme_font_scale_and_palette() {
    for custom in [
        "",
        "/* Author CSS */\nbody { font-family: fantasy; color: navy; }\n.fmd h1 { font-size: 3em; }\n",
    ] {
        let mut opts = HtmlOptions {
            custom_css: Some(custom.into()),
            ..HtmlOptions::default()
        };
        let first = publications(&opts);
        opts.theme = Theme::charcoal()
            .with_font(FontFamily::Serif)
            .with_font_scale(FontScale::from_factor(2.0));
        let changed = publications(&opts);
        assert_eq!(
            first, changed,
            "explicit CSS controls the entire stylesheet"
        );
        for archive in first {
            assert_eq!(entry_text(&archive, "OEBPS/style.css"), custom);
            assert!(entry_text(&archive, "OEBPS/chapter-1.xhtml").contains("<main class=\"fmd\">"));
        }
    }
}

#[test]
fn embedded_host_fonts_preserve_typographic_scale_and_win_the_default_family_cascade() {
    let opts = HtmlOptions {
        theme: Theme::serif().with_font_scale(FontScale::from_factor(2.0)),
        font_assets: FontAssets::default()
            .with_slot(
                FontAssetSlot::BodyRegular,
                franken_markdown::fonts::body_bytes(
                    FontFamily::Sans,
                    franken_markdown::fonts::FontStyle::Regular,
                )
                .to_vec(),
            )
            .unwrap(),
        ..HtmlOptions::default()
    };
    for archive in publications(&opts) {
        let css = entry_text(&archive, "OEBPS/style.css");
        assert!(css.contains("body{font-family:serif;font-size:200%;"));
        let fonts = entry_text(&archive, "OEBPS/embedded-fonts.css");
        assert!(fonts.contains("body{font-family:\"FmdEpubBody\",\"FmdEpubSymbols\",serif;}"));
        assert!(
            !fonts.contains("font-size:"),
            "font embedding must not reset the text scale"
        );
        for entry in parse_central_directory(&archive)
            .iter()
            .filter(|entry| entry.name.ends_with(".xhtml"))
        {
            let page = entry_text(&archive, &entry.name);
            assert!(
                page.find("href=\"style.css\"").unwrap()
                    < page.find("href=\"embedded-fonts.css\"").unwrap()
            );
        }
    }
}

#[test]
fn code_ligature_preferences_are_exported() {
    for (policy, declaration) in [
        (CodeLigatures::Off, "font-variant-ligatures:none;"),
        (
            CodeLigatures::Conservative,
            "font-variant-ligatures:common-ligatures contextual;",
        ),
        (
            CodeLigatures::All,
            "font-variant-ligatures:common-ligatures discretionary-ligatures contextual;",
        ),
    ] {
        let opts = HtmlOptions {
            theme: Theme::default().with_code_ligatures(policy),
            ..HtmlOptions::default()
        };
        for archive in publications(&opts) {
            assert!(entry_text(&archive, "OEBPS/style.css").contains(declaration));
        }
    }
}

#[cfg(feature = "cli")]
#[test]
fn cli_epub_font_and_font_scale_flags_change_the_actual_archive() {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};
    let dir = std::env::temp_dir().join(format!(
        "fmd-epub-theme-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let mut archives = Vec::new();
    for (name, font, scale, expected) in [
        (
            "sans",
            "sans",
            "1",
            "body{font-family:sans-serif;font-size:100%;",
        ),
        (
            "serif",
            "serif",
            "1",
            "body{font-family:serif;font-size:100%;",
        ),
        (
            "large",
            "sans",
            "2",
            "body{font-family:sans-serif;font-size:200%;",
        ),
    ] {
        let path = dir.join(format!("{name}.epub"));
        let output = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args([
                "--no-config",
                "--text",
                SOURCE,
                "--to",
                "epub",
                "--font",
                font,
                "--font-scale",
                scale,
                "--out",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let archive = std::fs::read(&path).unwrap();
        assert!(entry_text(&archive, "OEBPS/style.css").contains(expected));
        archives.push(archive);
        std::fs::remove_file(path).unwrap();
    }
    std::fs::remove_dir(dir).unwrap();
    assert_ne!(archives[0], archives[1]);
    assert_ne!(archives[0], archives[2]);
    assert_ne!(identifier(&archives[0]), identifier(&archives[1]));
    assert_ne!(identifier(&archives[0]), identifier(&archives[2]));
}
