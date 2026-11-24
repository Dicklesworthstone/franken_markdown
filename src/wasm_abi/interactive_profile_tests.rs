//! Native-host execution of the additive editable-workspace WASM binding.
//! Error admission is covered by authoring_profile_tests without constructing
//! JsValue errors on non-WASM hosts; these tests exercise successful rendering.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    FmdRenderResult, WasmRenderOptions, render_interactive_html_configured,
    render_interactive_html_configured_advanced, render_interactive_html_configured_profiled,
};
use crate::{HtmlFontFormat, Profile};

const SOURCE: &str = "# Grammar\n\n*Term\n: definition*\n\n<!-- pagebreak -->\n\nTail.\n";
const MANIFEST: &str = "<script type=\"application/json\" id=\"fmd-authoring-profile\">";

fn profiled(source: &str, profile: Option<&str>) -> FmdRenderResult {
    render_interactive_html_configured_profiled(
        source,
        None,
        None,
        None,
        None,
        false,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        false,
        None,
        None,
        profile.map(str::to_owned),
    )
    .unwrap()
}

fn html(result: &FmdRenderResult) -> String {
    String::from_utf8(result.bytes()).unwrap()
}

#[test]
fn profiled_workspace_uses_native_source_grammar_and_retains_its_manifest() {
    for profile in [Profile::CommonMarkGfm, Profile::GfmPlus] {
        let result = profiled(SOURCE, Some(profile.as_str()));
        assert_eq!(result.format(), "interactive-html");
        assert_eq!(result.mime_type(), "text/html; charset=utf-8");
        assert_eq!(result.extension(), "html");
        assert_eq!(result.source_length(), SOURCE.len());
        assert_eq!(result.diagnostics_json(), "[]");
        let output = html(&result);
        assert!(output.contains("<iframe data-fmd-styled-preview"));
        assert_eq!(output.matches(MANIFEST).count(), 1);
        assert!(output.contains(&format!(
            "{MANIFEST}{{\"version\":1,\"profile\":\"{}\"}}</script>",
            profile.as_str()
        )));
        if profile == Profile::CommonMarkGfm {
            assert!(output.contains(
                "&lt;p&gt;&lt;em&gt;Term\n: definition&lt;/em&gt;&lt;/p&gt;"
            ));
            assert!(!output.contains("&lt;dl&gt;"));
            assert!(!output.contains("&lt;div class=&quot;fmd-pagebreak&quot;"));
        } else {
            assert!(output.contains("&lt;dl&gt;"));
            assert!(output.contains("&lt;dt&gt;*Term&lt;/dt&gt;"));
            assert!(output.contains("&lt;dd&gt;definition*&lt;/dd&gt;"));
            assert!(output.contains("&lt;div class=&quot;fmd-pagebreak&quot;"));
        }
    }
}

#[test]
fn omitted_profile_preserves_advanced_bytes_and_existing_binding_signatures() {
    let advanced = render_interactive_html_configured_advanced(
        SOURCE,
        None,
        None,
        None,
        None,
        false,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        false,
        None,
        None,
    )
    .unwrap();
    let omitted = profiled(SOURCE, None);
    assert_eq!(omitted.bytes(), advanced.bytes());
    let output = html(&omitted);
    assert!(output.contains("<iframe data-fmd-styled-preview"));
    assert!(!output.contains(MANIFEST));
    assert!(output.contains("&lt;dl&gt;"));

    let simple =
        render_interactive_html_configured(SOURCE, None, None, None, None, None).unwrap();
    let expected = crate::interactive::render_interactive_html_checked(
        &crate::parse_markdown(SOURCE),
        SOURCE,
        &crate::HtmlOptions::default(),
    )
    .unwrap();
    assert_eq!(html(&simple), expected);
    assert!(!html(&simple).contains("<iframe data-fmd-styled-preview"));
}

#[test]
fn profiled_workspace_keeps_styles_navigation_and_explicit_image_bindings() {
    let source = format!("{SOURCE}\n![Plot](plot.svg)\n");
    let image = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="2" height="1" fill="red"/></svg>"#;
    let css = ".fmd { color: #123456; }";
    for profile in [Profile::CommonMarkGfm, Profile::GfmPlus] {
        let result = render_interactive_html_configured_profiled(
            &source,
            None,
            None,
            Some("  Profile <owner>  ".into()),
            Some(css.into()),
            false,
            Some(1.25),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec!["plot.svg".into()],
            image.to_vec(),
            vec![u32::try_from(image.len()).unwrap()],
            Some("fr".into()),
            true,
            Some(2),
            Some("ttf".into()),
            Some(profile.as_str().into()),
        )
        .unwrap();
        let options = WasmRenderOptions {
            profile: Some(profile),
            title: Some("  Profile <owner>  ".into()),
            custom_css: Some(css.into()),
            font_scale: Some(1.25),
            lang: Some("fr".into()),
            toc: true,
            toc_depth: Some(2),
            html_font_format: HtmlFontFormat::Ttf,
            ..WasmRenderOptions::default()
        }
        .with_pdf_image_asset("plot.svg", image.to_vec())
        .unwrap();
        let document = crate::parse_markdown_with_profile(&source, Some(profile));
        let expected = crate::interactive::render_interactive_html_advanced(
            &document,
            &source,
            &options.html_options(),
        );
        let output = html(&result);
        assert_eq!(output, expected);
        assert!(output.contains("<html lang=\"fr\">"));
        assert!(output.contains("<title>  Profile &lt;owner&gt;  </title>"));
        assert!(output.contains("id=\"fmd-image-assets\">[[\"plot.svg\",\"data:image/svg+xml;base64,"));
        assert!(output.contains("\"toc\":true,\"tocDepth\":2"));
        assert!(output.contains(css));
        assert_eq!(result.source_length(), source.len());
    }
}

#[test]
fn profiled_workspace_preserves_source_controls_and_inert_manifest_lookalikes() {
    let source = concat!(
        "\u{feff}# Source\r\n\r\n\x60\x60\x60html\r\n",
        "</script><script type=\"application/json\" id=\"fmd-authoring-profile\">",
        "{\"version\":1,\"profile\":\"gfm-plus\"}</script>\r\n\x60\x60\x60\r\n",
        "\r\n*Term\r\n: definition*\r\n\0\t\u{2028}\u{2029}"
    );
    let result = profiled(source, Some("commonmark-gfm"));
    let output = html(&result);
    assert_eq!(result.source_length(), source.len());
    assert_eq!(output.matches(MANIFEST).count(), 1);
    assert!(output.contains(&format!(
        "{MANIFEST}{{\"version\":1,\"profile\":\"commonmark-gfm\"}}</script>"
    )));
    let raw = output
        .split_once("<script type=\"application/json\" id=\"fmd-raw-source\">")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    assert!(!raw.contains('<'));
    assert!(raw.contains("\\u003c/script>"));
    assert!(raw.contains("\\u000d\\u000a"));
    assert!(raw.contains("\\u0000\\u0009\\u2028\\u2029"));
    assert!(raw.contains('\u{feff}'));
}
