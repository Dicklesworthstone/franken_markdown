//! Actual SVG syntax paint and preservation of source-shaped output geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::Tok;
use franken_markdown::svg::{
    SvgOptions, SvgReport, SvgWarning, render_svg, render_svg_with_diagnostics,
    render_svg_with_report, render_svg_with_resources,
};
use franken_markdown::{
    Block, Document, FontAssetSlot, FontAssets, SystemAppearance, Theme, parse_markdown,
};

fn render(document: &Document, options: &SvgOptions) -> (String, SvgReport, Vec<SvgWarning>) {
    let (bytes, report, warnings) = render_svg_with_diagnostics(document, options);
    (String::from_utf8(bytes).unwrap(), report, warnings)
}

/// Mask only glyph fill values. Every path, glyph identity, transform, panel,
/// page dimension, link and ordering byte must otherwise remain unchanged.
fn without_glyph_ink(svg: &str) -> String {
    let mut output = String::new();
    for line in svg.lines() {
        if line.starts_with("<use href=\"#g") {
            let (before, rest) = line.split_once(" fill=\"").unwrap();
            let (_, after) = rest.split_once('"').unwrap();
            output.push_str(before);
            output.push_str(" fill=\"INK\"");
            output.push_str(after);
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

fn code_document(code: &str, language: Option<&str>) -> Document {
    Document {
        blocks: vec![Block::CodeBlock {
            lang: language.map(str::to_owned),
            code: code.to_owned(),
        }],
    }
}

#[test]
fn supported_fences_emit_shared_token_colors_without_any_geometry_change() {
    let source = "fn main() {\n\tlet count: u32 = 42;\n\tlet text = \"résumé\"; // comment\n}\n";
    let mut options = SvgOptions {
        max_width_pt: 144.0,
        ..SvgOptions::default()
    };
    options.theme.page.margins.left_pt = 36.0;
    options.theme.page.margins.right_pt = 36.0;
    let colored = render(&code_document(source, Some("rust")), &options);
    let plain = render(&code_document(source, None), &options);
    assert!(colored.2.is_empty(), "{:?}", colored.2);
    assert_eq!(colored.1, plain.1);
    assert_eq!(colored.2, plain.2);
    assert_eq!(without_glyph_ink(&colored.0), without_glyph_ink(&plain.0));
    assert_ne!(colored.0, plain.0);
    let palette = options.theme.effective_colors(false, false);
    for token in [
        Tok::Keyword,
        Tok::Type,
        Tok::Func,
        Tok::Str,
        Tok::Number,
        Tok::Comment,
    ] {
        assert!(
            colored.0.contains(&format!(
                "fill=\"{}\"",
                options.theme.code_color(palette, token)
            )),
            "{token:?}"
        );
    }
    assert!(
        !colored.0.contains("<text")
            && !colored.0.contains("<foreignObject")
            && !colored.0.contains("<script")
    );
}

#[test]
fn nested_and_note_fences_keep_multiline_lexing_and_all_source_geometry() {
    let source = "# Code\n\n> ```javascript\n> const text = `résumé\n> \tsecond line`;\n> /* comment\n> continued */\n> ```\n\n- ```javascript\n  let answer = 42;\n  ```\n\nA note[^n].\n\n[^n]: Code below.\n\n    ```javascript\n    const value = \"done\";\n    ```\n";
    let options = SvgOptions::default();
    let colored = render(&parse_markdown(source), &options);
    let plain = render(
        &parse_markdown(&source.replace("```javascript", "```unknown-language")),
        &options,
    );
    assert!(colored.2.is_empty(), "{:?}", colored.2);
    assert_eq!(colored.1, plain.1);
    assert_eq!(colored.2, plain.2);
    assert_eq!(without_glyph_ink(&colored.0), without_glyph_ink(&plain.0));
    assert_ne!(colored.0, plain.0);
    assert!(colored.0.contains("href=\"#fmd-note-1\""));
}

#[test]
fn every_public_api_and_custom_font_use_the_same_deterministic_code_paint() {
    let document = code_document(
        "let café = \"e\u{0301}\tfi\";\n/* comment */\n",
        Some("rust"),
    );
    for appearance in [
        SystemAppearance::Light,
        SystemAppearance::Dark,
        SystemAppearance::HighContrastDark,
    ] {
        let options = SvgOptions {
            theme: Theme {
                appearance,
                ..Theme::default()
            },
            ..SvgOptions::default()
        };
        let result = render_svg_with_diagnostics(&document, &options);
        assert_eq!(result, render_svg_with_diagnostics(&document, &options));
        assert_eq!(result.0, render_svg(&document, &options));
        assert_eq!(
            (result.0.clone(), result.1),
            render_svg_with_report(&document, &options)
        );
        assert_eq!(
            result,
            render_svg_with_resources(&document, &options, &FontAssets::default(), &[]).unwrap()
        );
        if appearance == SystemAppearance::HighContrastDark {
            assert_eq!(
                result,
                render_svg_with_diagnostics(
                    &code_document("let café = \"e\u{0301}\tfi\";\n/* comment */\n", None),
                    &options
                )
            );
        }
    }
    let fonts = FontAssets::default()
        .with_slot(
            FontAssetSlot::MonoRegular,
            franken_markdown::fonts::body_bytes(
                franken_markdown::FontFamily::Serif,
                franken_markdown::fonts::FontStyle::Regular,
            )
            .to_vec(),
        )
        .unwrap();
    let options = SvgOptions::default();
    let result = render_svg_with_resources(&document, &options, &fonts, &[]).unwrap();
    let plain = render_svg_with_resources(
        &code_document("let café = \"e\u{0301}\tfi\";\n/* comment */\n", None),
        &options,
        &fonts,
        &[],
    )
    .unwrap();
    assert_eq!(result.1, plain.1);
    assert_eq!(result.2, plain.2);
    assert_eq!(
        without_glyph_ink(std::str::from_utf8(&result.0).unwrap()),
        without_glyph_ink(std::str::from_utf8(&plain.0).unwrap())
    );
}

#[test]
fn hostile_language_metadata_falls_back_once_and_unknown_languages_stay_plain() {
    let source = "fn main() { let x = 42; }\n";
    let options = SvgOptions::default();
    let plain = render(&code_document(source, None), &options);
    assert_eq!(
        plain,
        render(&code_document(source, Some("unknown-language")), &options)
    );
    let long_language = "rust".repeat(65);
    let refused = render(&code_document(source, Some(&long_language)), &options);
    assert_eq!(refused.0, plain.0);
    assert_eq!(refused.1, plain.1);
    assert_eq!(refused.2.len(), 1);
    assert_eq!(refused.2[0].code, "svg_code_highlight_limit");
}

#[test]
fn diagrams_keep_priority_and_unrecognized_diagrams_keep_complete_code() {
    let source = "graph LR\n A[Start] --> B[End]\n";
    let options = SvgOptions::default();
    let diagram = render(&code_document(source, Some("mermaid")), &options);
    assert_eq!(diagram, render(&code_document(source, None), &options));
    assert!(diagram.2.is_empty());
    assert!(diagram.0.contains("data:image/svg+xml;base64,"));
    let unsupported = "classDiagram\n class Member {\n +String name\n }\n";
    let colored = render(&code_document(unsupported, Some("mermaid")), &options);
    let plain = render(
        &code_document(unsupported, Some("unknown-language")),
        &options,
    );
    assert_eq!(colored.1, plain.1);
    assert_eq!(colored.2, plain.2);
    assert_eq!(without_glyph_ink(&colored.0), without_glyph_ink(&plain.0));
    assert!(!colored.0.contains("data:image/svg+xml;base64,"));
}
