//! Reader-friendly CSS from the shared theme, without HTML's browser shell.
//!
//! Relative sizes preserve a reading system's own font-size controls. Concrete
//! color declarations also work in readers that do not support CSS variables.
//! Font resources are linked separately and may override only the families.

use std::borrow::Cow;

use franken_markdown::{
    CodeLigatures, DarkModePolicy, FontFamily, HtmlOptions, MonoFontFamily, SystemAppearance,
    Theme, ThemeColors, ThemeSpacing, TypeScale,
};

pub(super) fn stylesheet(opts: &HtmlOptions) -> Cow<'_, str> {
    match &opts.custom_css {
        Some(css) => Cow::Borrowed(css),
        None => Cow::Owned(default_css(&opts.theme)),
    }
}

fn default_css(theme: &Theme) -> String {
    let spacing = &theme.spacing;
    let defaults = ThemeSpacing::default();
    let sizes = TypeScale::default();
    // Generic families respect the reading system's installed fonts. Explicit
    // host faces replace these through the existing embedded-font stylesheet.
    let body_font = match theme.font {
        FontFamily::Sans => "sans-serif",
        FontFamily::Serif => "serif",
    };
    let mono_font = match theme.mono_font {
        MonoFontFamily::Documentation => "monospace",
    };
    let base = if spacing.base_px == 0 {
        defaults.base_px
    } else {
        spacing.base_px
    };
    let base_percent = number(f32::from(base) / f32::from(defaults.base_px) * 100.0);
    let leading = number(
        if spacing.line_height.is_finite() && spacing.line_height > 0.0 {
            spacing.line_height
        } else {
            defaults.line_height
        },
    );
    let pad_y = number(nonnegative(
        spacing.table_cell_padding_y_em,
        defaults.table_cell_padding_y_em,
    ));
    let pad_x = number(nonnegative(
        spacing.table_cell_padding_x_em,
        defaults.table_cell_padding_x_em,
    ));
    let radius = number(f32::from(spacing.radius_px) / f32::from(defaults.base_px));
    let code_size = number(sizes.code / sizes.body);
    let table_size = number(sizes.table / sizes.body);
    let mut css = format!(
        "body{{font-family:{body_font};font-size:{base_percent}%;line-height:{leading};margin:5%;overflow-wrap:break-word;}}\n\
         h1,h2,h3,h4,h5,h6{{line-height:1.25;page-break-after:avoid;break-after:avoid;}}\n"
    );
    for (index, size) in sizes.h.iter().enumerate() {
        css.push_str(&format!(
            "h{}{{font-size:{}em;}}\n",
            index + 1,
            number(size / sizes.body)
        ));
    }
    css.push_str(&format!(
        "pre,code,kbd,samp{{font-family:{mono_font};font-size:{code_size}em;}}\n\
         pre{{white-space:pre-wrap;overflow-wrap:break-word;border:1px solid;padding:0.5em;border-radius:{radius}em;}}\n\
         pre code{{font-size:1em;background:transparent;}}\n\
         table{{border-collapse:collapse;font-size:{table_size}em;max-width:100%;}}\n\
         th,td{{border:1px solid;padding:{pad_y}em {pad_x}em;vertical-align:top;}}\n\
         blockquote{{margin-left:0;padding-left:1em;border-left:0.25em solid;}}\n\
         img,svg{{max-width:100%;height:auto;}}\n\
         img{{border-radius:{radius}em;}}\n\
         hr{{border:0;border-top:1px solid;}}\n\
         .table-wrap{{overflow-x:auto;}}\n"
    ));
    let ligatures = match theme.code_ligatures {
        CodeLigatures::Off => {
            "font-variant-ligatures:none;font-feature-settings:\"liga\" 0,\"calt\" 0,\"dlig\" 0;"
        }
        CodeLigatures::Conservative => {
            "font-variant-ligatures:common-ligatures contextual;font-feature-settings:\"liga\" 1,\"calt\" 1,\"dlig\" 0;"
        }
        CodeLigatures::All => {
            "font-variant-ligatures:common-ligatures discretionary-ligatures contextual;font-feature-settings:\"liga\" 1,\"calt\" 1,\"dlig\" 1;"
        }
    };
    css.push_str(&format!("pre,code,kbd,samp{{{ligatures}}}\n"));

    // A fixed appearance takes precedence over the host media query. Auto
    // follows the reader only when automatic dark mode is enabled; a disabled
    // policy keeps the configured base palette even in a dark reading system.
    let automatic =
        theme.appearance == SystemAppearance::Auto && theme.dark_mode == DarkModePolicy::Auto;
    let dark = theme.appearance.is_dark();
    css.push_str(if automatic {
        ":root{color-scheme:light dark;}\n"
    } else if dark {
        ":root{color-scheme:dark;}\n"
    } else {
        ":root{color-scheme:light;}\n"
    });
    palette(
        &mut css,
        theme.effective_colors(false, false),
        dark,
        theme.appearance.is_high_contrast(),
    );
    if automatic {
        css.push_str("@media (prefers-color-scheme: dark){\n");
        palette(&mut css, &theme.dark_colors, true, false);
        css.push_str("}\n");
    }
    css
}

fn palette(css: &mut String, colors: &ThemeColors, dark: bool, high_contrast: bool) {
    let fallback = if dark {
        ThemeColors::dark()
    } else {
        ThemeColors::light()
    };
    let fg = color(&colors.fg, &fallback.fg);
    let bg = color(&colors.bg, &fallback.bg);
    let muted = color(&colors.fg_muted, &fallback.fg_muted);
    let subtle = color(&colors.bg_subtle, &fallback.bg_subtle);
    let border = color(&colors.border, &fallback.border);
    let border_muted = color(&colors.border_muted, &fallback.border_muted);
    let code = color(&colors.code_bg, &fallback.code_bg);
    let stripe = color(&colors.stripe, &fallback.stripe);
    let quote = color(&colors.quote_fg, &fallback.quote_fg);
    let quote_bar = color(&colors.quote_bar, &fallback.quote_bar);
    let accent = color(&colors.accent, &fallback.accent);
    let selection_bg = color(&colors.selection_bg, &fallback.selection_bg);
    let selection_fg = color(&colors.selection_fg, &fallback.selection_fg);
    let match_bg = color(&colors.search_match_bg, &fallback.search_match_bg);
    let match_fg = color(&colors.search_match_fg, &fallback.search_match_fg);
    css.push_str(&format!(
        "body{{color:{fg};background-color:{bg};}}\n\
         a{{color:{accent};}}\n\
         h5,h6,.footnotes{{color:{muted};}}\n\
         pre,code,kbd,samp{{background-color:{code};}}\n\
         pre{{border-color:{border_muted};}}\n\
         pre code{{background-color:transparent;}}\n\
         th,td{{border-color:{border};}}\n\
         thead th{{background-color:{subtle};}}\n\
         tbody tr:nth-child(even){{background-color:{stripe};}}\n\
         blockquote{{color:{quote};border-left-color:{quote_bar};}}\n\
         hr{{border-color:{border};}}\n\
         ::selection{{color:{selection_fg};background-color:{selection_bg};}}\n\
         mark{{color:{match_fg};background-color:{match_bg};}}\n"
    ));
    if high_contrast {
        css.push_str(".tok-kw,.tok-ty,.tok-fn,.tok-st,.tok-nu,.tok-cm,.tok-op,.tok-pn{color:inherit;}\n.tok-kw{font-weight:bold;}\n.tok-cm{font-style:italic;}\n");
    } else {
        let (kw, ty, fun, string, number, comment) = if dark {
            (
                "#ff7b72", "#ffa657", "#d2a8ff", "#a5d6ff", "#79c0ff", "#8b949e",
            )
        } else {
            (
                "#cf222e", "#953800", "#6639ba", "#0a3069", "#0550ae", "#6e7781",
            )
        };
        css.push_str(&format!(
            ".tok-kw{{color:{kw};}}\n.tok-ty{{color:{ty};}}\n.tok-fn{{color:{fun};}}\n\
             .tok-st{{color:{string};}}\n.tok-nu,.tok-op{{color:{number};}}\n\
             .tok-cm{{color:{comment};font-style:italic;}}\n.tok-pn{{color:inherit;}}\n"
        ));
    }
}

/// A color field cannot introduce declarations, selectors, escapes, or URLs.
/// Support named/hex colors and numeric CSS color functions; reject the entire
/// value on failure rather than turning hostile input into a different token.
fn color<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return fallback;
    }
    if let Some(hex) = value.strip_prefix('#') {
        return if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            value
        } else {
            fallback
        };
    }
    if value.bytes().all(|b| b.is_ascii_alphabetic()) {
        return value;
    }
    if let Some((name, tail)) = value.split_once('(') {
        let function = [
            "rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color",
        ]
        .iter()
        .any(|allowed| name.eq_ignore_ascii_case(allowed));
        if function
            && let Some(parameters) = tail.strip_suffix(')')
            && parameters.bytes().any(|b| b.is_ascii_digit())
            && parameters.bytes().all(|b| {
                // Letters allow hue units, scientific notation, color-space
                // names and `none`. Nested functions and escapes stay out.
                b.is_ascii_alphanumeric()
                    || matches!(b, b' ' | b'\t' | b',' | b'.' | b'%' | b'/' | b'+' | b'-')
            })
        {
            return value;
        }
    }
    fallback
}

fn nonnegative(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        fallback
    }
}

fn number(value: f32) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let mut result = format!("{value:.3}");
    while result.ends_with('0') {
        result.pop();
    }
    if result.ends_with('.') {
        result.pop();
    }
    result
}
