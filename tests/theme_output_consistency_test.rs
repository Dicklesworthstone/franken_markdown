//! Appearance is an output contract, including the page beneath the glyphs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, DarkModePolicy, HtmlOptions, PdfEmitOptions, PdfOptions, SvgOptions, SystemAppearance,
    Theme, parse_markdown, render_html, render_pdf, render_pdf_document_emitted, render_svg,
};

const SOURCE: &str = "# Appearance\n\nReadable text and $x^2$.\n\n```rust\nfn example() {\n    // comment\n    let text = \"value\";\n    let answer = 42;\n}\n```\n\n$$x^2 + y^2$$\n";

/// Follow actual page references and decode their content streams. The test
/// never consults the renderer's palette or page-placement helpers.
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
                String::from_utf8(stream.to_vec()).unwrap()
            }
        })
        .collect()
}

#[test]
fn explicit_html_appearance_controls_both_page_and_code_without_host_override() {
    for (theme, background, foreground, keyword) in [
        (
            Theme::default().with_appearance(SystemAppearance::Dark),
            "#0d1117",
            "#e6edf3",
            "#ff7b72",
        ),
        (Theme::charcoal(), "#1a1d21", "#e6edf3", "#ff7b72"),
        (Theme::high_contrast_dark(), "#000000", "#ffffff", "#ffffff"),
        (
            Theme::high_contrast_light(),
            "#ffffff",
            "#000000",
            "#000000",
        ),
        (
            Theme::default().with_appearance(SystemAppearance::Light),
            "#ffffff",
            "#1f2328",
            "#cf222e",
        ),
    ] {
        // Explicit appearance wins even when automatic dark CSS is disabled.
        for policy in [DarkModePolicy::Auto, DarkModePolicy::Disabled] {
            let html = render_html(
                SOURCE,
                &HtmlOptions {
                    theme: theme.clone().with_dark_mode(policy),
                    ..HtmlOptions::default()
                },
            )
            .unwrap();
            assert!(html.contains(&format!("--fmd-bg: {background};")));
            assert!(html.contains(&format!("--fmd-fg: {foreground};")));
            assert!(html.contains(&format!(".tok-kw {{ color: {keyword}; }}")));
            assert!(
                !html.contains("@media (prefers-color-scheme: dark)"),
                "a fixed {:?} appearance must not change with the browser host",
                theme.appearance
            );
        }
    }
}

#[test]
fn automatic_html_keeps_independent_light_and_dark_palettes() {
    let html = render_html(SOURCE, &HtmlOptions::default()).unwrap();
    assert!(html.contains("--fmd-bg: #ffffff;"));
    assert!(html.contains("--fmd-bg: #0d1117;"));
    assert!(html.contains(".tok-kw { color: #cf222e; }"));
    assert!(html.contains(".tok-kw { color: #ff7b72; }"));
    assert!(html.contains("@media (prefers-color-scheme: dark)"));

    let html = render_html(
        SOURCE,
        &HtmlOptions {
            theme: Theme::default().with_dark_mode(DarkModePolicy::Disabled),
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    assert!(html.contains(".tok-kw { color: #cf222e; }"));
    assert!(!html.contains(".tok-kw { color: #ff7b72; }"));
    assert!(!html.contains("@media (prefers-color-scheme: dark)"));
}

#[test]
fn dark_pdf_paints_every_physical_page_before_text_and_matches_streaming() {
    let chapter = parse_markdown(SOURCE);
    let mut document = chapter.clone();
    document.blocks.push(Block::PageBreak);
    document.blocks.extend(chapter.blocks);
    for (theme, page_color, text_color) in [
        (
            Theme::default().with_appearance(SystemAppearance::Dark),
            "0.051 0.067 0.090",
            "0.902 0.929 0.953",
        ),
        (
            Theme::high_contrast_dark(),
            "0.000 0.000 0.000",
            "1.000 1.000 1.000",
        ),
    ] {
        let opts = PdfOptions {
            theme,
            ..PdfOptions::default()
        };
        let bytes =
            render_pdf_document_emitted(&document, &opts, PdfEmitOptions::default()).unwrap();
        assert_eq!(
            bytes,
            render_pdf_document_emitted(&document, &opts, PdfEmitOptions::monolithic()).unwrap()
        );
        let pages = page_contents(&bytes);
        assert_eq!(pages.len(), 2);
        for content in pages {
            assert!(content.starts_with("/Artifact BMC\n"));
            let background_end = content.find("EMC\n").unwrap();
            let background = &content[..background_end];
            assert!(background.contains(&format!("{page_color} rg ")));
            assert!(
                background.contains("0.00 0.00 612.00 792.00 re f"),
                "the page fill must cover the media box including margins"
            );
            assert!(content[background_end..].contains(&format!("{text_color} rg")));
            assert!(
                !content.contains("0.122 0.137 0.157 rg"),
                "display math must use the effective foreground too"
            );
            assert!(background_end < content.find("BT ").unwrap());
        }
    }
}

#[test]
fn pdf_code_tokens_follow_dark_and_high_contrast_appearance() {
    for (theme, keyword, string, number) in [
        (
            Theme::charcoal(),
            "1.000 0.482 0.447",
            "0.647 0.839 1.000",
            "0.475 0.753 1.000",
        ),
        (
            Theme::high_contrast_dark(),
            "1.000 1.000 1.000",
            "1.000 1.000 1.000",
            "1.000 1.000 1.000",
        ),
    ] {
        let bytes = render_pdf(
            SOURCE,
            &PdfOptions {
                theme,
                ..PdfOptions::default()
            },
        )
        .unwrap();
        let contents = page_contents(&bytes).concat();
        for expected in [keyword, string, number] {
            assert!(
                contents.contains(&format!("{expected} rg")),
                "missing {expected}"
            );
        }
        for light_token in [
            "0.812 0.133 0.180",
            "0.039 0.188 0.412",
            "0.020 0.314 0.682",
        ] {
            assert!(
                !contents.contains(&format!("{light_token} rg")),
                "light token {light_token} escaped onto a dark code panel"
            );
        }
    }
}

#[test]
fn custom_dark_code_panels_do_not_require_an_appearance_enum_override() {
    let mut theme = Theme::default().with_dark_mode(DarkModePolicy::Disabled);
    theme.colors.bg = "#123".to_string();
    theme.colors.code_bg = "#234".to_string();
    theme.colors.fg = "#fff".to_string();
    let html = render_html(
        SOURCE,
        &HtmlOptions {
            theme: theme.clone(),
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    assert!(html.contains(".tok-kw { color: #ff7b72; }"));
    let pdf = render_pdf(
        SOURCE,
        &PdfOptions {
            theme: theme.clone(),
            ..PdfOptions::default()
        },
    )
    .unwrap();
    let contents = page_contents(&pdf).concat();
    assert!(contents.contains("0.067 0.133 0.200 rg"));
    assert!(contents.contains("1.000 0.482 0.447 rg"));
    let svg = String::from_utf8(render_svg(
        &parse_markdown("Readable text"),
        &SvgOptions {
            theme,
            ..SvgOptions::default()
        },
    ))
    .unwrap();
    assert!(svg.contains("fill=\"#123\""));
    assert!(svg.contains("fill=\"#fff\""));
}

#[test]
fn static_default_remains_light_and_explicit_white_background_is_equivalent() {
    let opts = PdfOptions::default();
    let ordinary = render_pdf(SOURCE, &opts).unwrap();
    let mut explicit = opts.clone();
    explicit.theme.appearance = SystemAppearance::Light;
    assert_eq!(ordinary, render_pdf(SOURCE, &explicit).unwrap());
    explicit.theme.colors.bg = "#fff".to_string();
    assert_eq!(ordinary, render_pdf(SOURCE, &explicit).unwrap());
    let contents = page_contents(&ordinary).concat();
    assert!(
        !contents.contains("0.00 0.00 612.00 792.00 re f"),
        "ordinary white pages should not add redundant paint"
    );
}

#[test]
fn high_contrast_token_colors_cannot_escape_the_stylesheet() {
    let mut theme = Theme::high_contrast_dark();
    theme.dark_colors.fg = "#fff;}</style><script>alert(1)</script>".to_string();
    let html = render_html(
        SOURCE,
        &HtmlOptions {
            theme,
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    assert_eq!(html.matches("</style>").count(), 1);
    assert!(!html.contains("<script>"));
    assert!(!html.contains("#fff;}"));
}

#[test]
fn css_backgrounds_outside_the_pdf_hex_vocabulary_keep_the_previous_white_page() {
    let ordinary = render_pdf(SOURCE, &PdfOptions::default()).unwrap();
    for background in [
        "white",
        "rgb(240 241 242 / 95%)",
        "rgba(0, 0, 0, 0)",
        "#00000000",
        "#fff0",
        "transparent",
        "not-a-color",
    ] {
        let mut options = PdfOptions::default();
        options.theme.colors.bg = background.to_string();
        assert_eq!(
            ordinary,
            render_pdf(SOURCE, &options).unwrap(),
            "unsupported PDF background {background:?} must not become opaque black"
        );
    }
}
