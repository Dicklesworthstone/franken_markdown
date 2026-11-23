//! Publication metadata must reach the real renderer through the exported ABI.
#![cfg(feature = "wasm-bindgen")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::wasm::{WasmRenderOptions, render_html};
use franken_markdown::wasm_abi::{
    FmdRenderResult, render_html_configured_metadata, render_html_configured_profiled,
};
use franken_markdown::{DarkModePolicy, Profile};

const SOURCE: &str = "---\nauthor: Source default\n---\n# Chapter\n\n[Term\n: definition](/target)\n\n## Detail\n\n![Badge](badge)\n";
const IMAGE: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red"/></svg>"#;

fn metadata(author: Option<&str>, profile: Option<&str>) -> FmdRenderResult {
    render_html_configured_metadata(
        SOURCE,
        Some("serif".into()),
        Some("disabled".into()),
        Some("  Publication  ".into()),
        Some(String::new()),
        false,
        Some(1.25),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec!["badge".into()],
        IMAGE.to_vec(),
        vec![IMAGE.len() as u32],
        Some("fr".into()),
        true,
        Some(2),
        None,
        None,
        profile.map(str::to_owned),
        author.map(str::to_owned),
    )
    .unwrap()
}

fn expected(
    author: Option<&str>,
    profile: Option<Profile>,
) -> franken_markdown::wasm::WasmRenderOutput {
    let options = WasmRenderOptions {
        title: Some("  Publication  ".into()),
        author: author.map(str::to_owned),
        custom_css: Some(String::new()),
        font_scale: Some(1.25),
        lang: Some("fr".into()),
        toc: true,
        toc_depth: Some(2),
        profile,
        ..WasmRenderOptions::default()
    }
    .with_font_name("serif")
    .unwrap()
    .with_dark_mode(DarkModePolicy::Disabled)
    .with_pdf_image_asset("badge", IMAGE.to_vec())
    .unwrap();
    render_html(SOURCE, &options).unwrap()
}

#[test]
fn html_metadata_binding_preserves_author_and_all_existing_render_settings() {
    for (profile_name, profile) in [
        (None, None),
        (Some("commonmark-gfm"), Some(Profile::CommonMarkGfm)),
        (Some("gfm-plus"), Some(Profile::GfmPlus)),
    ] {
        let author = "  Élodie & \"Co\" <script>alert(1)</script>\nSecond line  ";
        let actual = metadata(Some(author), profile_name);
        let expected = expected(Some(author), profile);
        assert_eq!(actual.bytes(), expected.bytes, "{profile_name:?}");
        assert_eq!(actual.source_length(), SOURCE.len());
        assert_eq!(actual.format(), "html");
        assert_eq!(actual.mime_type(), "text/html; charset=utf-8");
        assert_eq!(actual.diagnostics_json(), expected.diagnostics_json());
        let html = String::from_utf8(actual.bytes()).unwrap();
        assert!(html.contains("<meta name=\"author\" content=\"  Élodie &amp; &quot;Co&quot;"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("<html lang=\"fr\">"));
        assert!(html.contains("data:image/svg+xml;base64,"));
        assert!(!html.contains("content=\"Source default\""));
    }
}

#[test]
fn html_metadata_binding_keeps_empty_suppression_and_verbatim_whitespace() {
    let absent = metadata(None, None).bytes();
    assert_eq!(absent, expected(None, None).bytes);
    assert!(
        !String::from_utf8(absent.clone())
            .unwrap()
            .contains("<meta name=\"author\"")
    );
    assert_eq!(metadata(Some(""), None).bytes(), absent);
    let spaces = metadata(Some("  "), None).bytes();
    assert_eq!(spaces, expected(Some("  "), None).bytes);
    assert!(
        String::from_utf8(spaces)
            .unwrap()
            .contains("<meta name=\"author\" content=\"  \">")
    );
}

#[test]
fn existing_profiled_html_binding_retains_its_resolved_options_and_bytes() {
    for profile in [None, Some("commonmark-gfm"), Some("gfm-plus")] {
        let old = render_html_configured_profiled(
            SOURCE,
            Some("serif".into()),
            Some("disabled".into()),
            Some("  Publication  ".into()),
            Some(String::new()),
            false,
            Some(1.25),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec!["badge".into()],
            IMAGE.to_vec(),
            vec![IMAGE.len() as u32],
            Some("fr".into()),
            true,
            Some(2),
            None,
            None,
            profile.map(str::to_owned),
        )
        .unwrap();
        let new = metadata(None, profile);
        assert_eq!(old.bytes(), new.bytes());
        assert_eq!(old.diagnostics_json(), new.diagnostics_json());
        assert_eq!(old.source_length(), new.source_length());
    }
}
