//! Configured workspaces retain actual native appearance/resources, while
//! application controls remain outside the document's style/ID scope.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::fonts::{FontStyle, body_bytes};
use franken_markdown::text::Font;
use franken_markdown::{
    DarkModePolicy, FontAssetSlot, FontFamily, HtmlFontFormat, HtmlOptions, PdfImageAsset,
    parse_markdown, render_interactive_html,
};

fn render(source: &str, options: &HtmlOptions) -> String {
    render_interactive_html(&parse_markdown(source), source, options)
}

fn frame(html: &str) -> String {
    let iframe = html
        .split("<iframe data-fmd-styled-preview")
        .nth(1)
        .expect("configured frame");
    let encoded = iframe
        .split(" srcdoc=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    encoded
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn decode_base64(encoded: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let mut value = 0_u32;
    let mut bits = 0;
    for byte in encoded.bytes().take_while(|&b| b != b'=') {
        let n = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("unexpected base64 byte"),
        };
        value = (value << 6) | u32::from(n);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((value >> bits) as u8);
        }
    }
    result
}

fn fonts(document: &str, mime: &str) -> Vec<Vec<u8>> {
    document
        .split(&format!("data:{mime};base64,"))
        .skip(1)
        .map(|part| decode_base64(part.split('"').next().unwrap()))
        .collect()
}

#[test]
fn default_and_metadata_only_workspaces_keep_the_lightweight_shell() {
    let default = render("# Default", &HtmlOptions::default());
    let configured = render(
        "# Default",
        &HtmlOptions {
            title: Some("Name".into()),
            lang: Some("fr".into()),
            ..Default::default()
        },
    );
    for html in [&default, &configured] {
        assert!(!html.contains("id=\"fmd-presentation\""));
        assert!(!html.contains("<iframe data-fmd-styled-preview"));
        assert!(html.contains("<h1 id=\"default\">Default</h1>"));
    }
    assert_eq!(
        default.rsplit_once("<script>\n").unwrap().1,
        configured.rsplit_once("<script>\n").unwrap().1
    );
}

#[test]
fn document_css_ids_and_raw_text_closers_are_isolated_from_the_editor() {
    let css = "body { color: rebeccapurple } #fmd-editor { display: none } p::after { content: '</style><script>attack()</script>'; }";
    let html = render(
        "# fmd-editor\n\nSafe document",
        &HtmlOptions {
            custom_css: Some(css.into()),
            html_font_format: HtmlFontFormat::Ttf,
            ..Default::default()
        },
    );
    let parent_head = html.split("</head>").next().unwrap();
    assert!(!parent_head.contains("rebeccapurple"));
    assert!(!html.contains("<script>attack()"));
    assert!(html.contains("sandbox=\"allow-same-origin allow-modals\""));
    assert!(!html.contains("sandbox=\"allow-scripts"));
    let document = frame(&html);
    assert!(document.contains("#fmd-editor { display: none }"));
    assert!(document.contains("<\\/style><script>attack()"));
    assert!(document.contains("default-src 'none'"));
    assert!(document.contains("<h1 id=\"fmd-editor\">fmd-editor</h1>"));
    assert!(
        html.contains("\\u003c\\\\/style>"),
        "saved CSS cannot close its data block"
    );
}

#[test]
fn supplied_font_retains_glyphs_and_styles_absent_from_initial_text() {
    let bytes = body_bytes(FontFamily::Serif, FontStyle::Regular);
    let mut options = HtmlOptions {
        html_font_format: HtmlFontFormat::Ttf,
        ..Default::default()
    };
    options
        .font_assets
        .set_slot(FontAssetSlot::BodyRegular, bytes.to_vec())
        .unwrap();
    let actual = fonts(&frame(&render("Just A", &options)), "font/ttf");
    assert_eq!(
        actual.len(),
        5,
        "later bold, italic and code need their faces too"
    );
    assert_eq!(
        actual[0], bytes,
        "editing retains the complete admitted face"
    );
    let embedded = Font::parse(actual[0].clone()).unwrap();
    for ch in ['é', 'Ω', 'Ж'] {
        assert_ne!(
            embedded.glyph_index(ch),
            0,
            "later typed {ch} remains mapped"
        );
    }
}

#[test]
fn serif_scale_and_disabled_dark_mode_reach_the_document_theme() {
    let mut options = HtmlOptions {
        html_font_format: HtmlFontFormat::Ttf,
        ..Default::default()
    };
    options.theme.font = FontFamily::Serif;
    options.theme.dark_mode = DarkModePolicy::Disabled;
    options.theme.spacing.base_px = 20;
    let document = frame(&render("# Configured", &options));
    assert!(document.contains("--fmd-base: 20px"));
    assert!(!document.contains("@media (prefers-color-scheme: dark)"));
    assert_eq!(
        fonts(&document, "font/ttf")[0],
        body_bytes(FontFamily::Serif, FontStyle::Regular)
    );
}

#[test]
fn native_toc_respects_depth_and_persists_the_editing_policy() {
    let html = render(
        "# First\n\n## Second\n\n### Hidden from navigation",
        &HtmlOptions {
            toc: true,
            toc_depth: Some(2),
            html_font_format: HtmlFontFormat::Ttf,
            ..Default::default()
        },
    );
    let document = frame(&html);
    assert!(document.contains("<nav class=\"toc\">"));
    assert!(document.contains("href=\"#first\""));
    assert!(document.contains("href=\"#second\""));
    assert!(!document.contains("href=\"#hidden-from-navigation\""));
    assert!(html.contains("\"toc\":true,\"tocDepth\":2"));
}

#[test]
fn explicit_images_remain_available_after_edits_and_cannot_fetch_external_resources() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="2" height="1"/></svg>"#;
    let options = HtmlOptions {
        toc: true,
        html_font_format: HtmlFontFormat::Ttf,
        image_assets: vec![
            PdfImageAsset::new("chart.svg", svg.to_vec()),
            PdfImageAsset::new("unused.svg", svg.to_vec()),
        ],
        ..Default::default()
    };
    let html = render("![Chart](chart.svg)", &options);
    let document = frame(&html);
    assert!(document.contains("src=\"data:image/svg+xml;base64,"));
    assert!(document.contains("img-src data:"));
    assert!(html.contains("[\"unused.svg\",\"data:image/svg+xml;base64,"));
    assert_eq!(render("![Chart](chart.svg)", &options), html);
}

#[test]
fn configured_font_containers_use_the_requested_native_encoder() {
    for (format, mime, signature) in [
        (HtmlFontFormat::Woff1, "font/woff", b"wOFF"),
        (HtmlFontFormat::Woff2, "font/woff2", b"wOF2"),
    ] {
        let html = render(
            "A",
            &HtmlOptions {
                toc: true,
                html_font_format: format,
                ..Default::default()
            },
        );
        let actual = fonts(&frame(&html), mime);
        assert_eq!(actual.len(), 5);
        assert!(actual.iter().all(|font| font.starts_with(signature)));
    }
}

/// Opt-in real-browser proof for generated bytes; no browser package is a
/// renderer dependency. The configured host owns Playwright and Chromium.
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn configured_workspace_browser_lifecycle() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if std::env::var_os("FMD_PLAYWRIGHT_MODULE").is_none() {
        eprintln!("SKIP configured workspace browser proof: FMD_PLAYWRIGHT_MODULE unset");
        return;
    }
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="2" height="1"/></svg>"#;
    let options = HtmlOptions {
        title: Some("Configured workspace".into()),
        lang: Some("fr".into()),
        custom_css: Some(
            "body {font-family: \"FMD Body\"; font-size:20px; color:rgb(70,50,30)}\n\
             code {font-family: \"FMD Mono\"}\n\
             #fmd-editor {display:none}\n\
             @media (prefers-color-scheme: dark) {body {color:rgb(230,230,230)}}\n\
             p::after {content:\"</style><script>globalThis.cssExecuted=true</script>\"}"
                .into(),
        ),
        toc: true,
        toc_depth: Some(2),
        html_font_format: HtmlFontFormat::Ttf,
        image_assets: vec![
            PdfImageAsset::new("chart.svg", svg.to_vec()),
            PdfImageAsset::new("unused.svg", svg.to_vec()),
        ],
        ..Default::default()
    };
    let html = render(
        "# Original\n\nA plain initial paragraph.\n\n![Chart](chart.svg)\n\n\
         ~~~mermaid\ngraph TD\nA[Original] --> B[Native diagram]\n~~~\n",
        &options,
    );
    let mut child = Command::new("node")
        .arg("tests/support/interactive_presentation_browser_check.mjs")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node must be installed on the configured browser verification host");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(html.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "browser proof failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
}
