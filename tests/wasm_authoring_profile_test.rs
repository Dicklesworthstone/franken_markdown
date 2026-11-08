//! End-to-end authoring dialect parity through the real browser ABI.
#![cfg(feature = "wasm-bindgen")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use franken_markdown::wasm_abi::*;
use franken_markdown::{HtmlOptions, PdfOptions, Profile, SvgOptions};

const SOURCE: &str = "# Profiles\n\nTerm\n: A **definition** with a [link](https://example.com).\n\n*Cross-line\n: emphasis*\n\n[Cross-line\n: link](https://example.com/target)\n\n`Cross-line\n: code`\n\n> Nested term\n> : Nested definition.\n\nA note[^n].\n\n[^n]: Note term\n    : A definition in the note.\n\n<script>alert('unsafe')</script>\n";

fn html(profile: Option<Profile>) -> FmdRenderResult {
    render_html_configured_profiled(
        SOURCE,
        None,
        None,
        None,
        Some(String::new()),
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
        None,
        profile.map(|value| value.as_str().to_owned()),
    )
    .unwrap()
}

fn pdf(profile: Option<Profile>, archival: &str) -> FmdRenderResult {
    render_pdf_configured_profiled(
        SOURCE,
        None,
        None,
        None,
        None,
        Some(1_700_000_000.0),
        false,
        false,
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
        None,
        None,
        false,
        None,
        None,
        false,
        None,
        None,
        false,
        vec![500.0, 700.0, 40.0, 30.0, 45.0, 35.0],
        Vec::new(),
        false,
        false,
        false,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        archival,
        false,
        None,
        profile.map(|value| value.as_str().to_owned()),
    )
    .unwrap()
}

fn svg(profile: Option<Profile>) -> FmdRenderResult {
    render_svg_configured_profiled(
        SOURCE,
        None,
        None,
        None,
        Some(420.0),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        profile.map(|value| value.as_str().to_owned()),
    )
    .unwrap()
}

fn epub(profile: Option<Profile>) -> FmdRenderResult {
    render_epub_configured_profiled(
        SOURCE,
        None,
        None,
        None,
        None,
        None,
        Some(String::new()),
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
        profile.map(|value| value.as_str().to_owned()),
    )
    .unwrap()
}

#[test]
fn html_profiles_match_native_structure_without_changing_html_safety() {
    for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
        let options = HtmlOptions {
            custom_css: Some(String::new()),
            profile,
            ..HtmlOptions::default()
        };
        let actual = html(profile);
        let expected = franken_markdown::render_html(SOURCE, &options).unwrap();
        assert_eq!(actual.bytes(), expected.as_bytes());
        assert_eq!(actual.source_length(), SOURCE.len());
        assert_eq!(
            expected.contains("<dl>"),
            profile != Some(Profile::CommonMarkGfm)
        );
        assert!(!expected.contains("<script>"));
        assert!(expected.contains("<strong>definition</strong>"));
        if profile == Some(Profile::CommonMarkGfm) {
            assert!(expected.contains("<em>Cross-line\n: emphasis</em>"));
            assert!(expected.contains("<code>Cross-line : code</code>"));
            assert!(
                expected.contains("href=\"https://example.com/target\">Cross-line\n: link</a>")
            );
        }
    }
    assert_eq!(html(None).bytes(), html(Some(Profile::GfmPlus)).bytes());
    assert_ne!(
        html(None).bytes(),
        html(Some(Profile::CommonMarkGfm)).bytes()
    );
}

#[test]
fn pdf_authoring_profile_and_archival_profile_remain_independent() {
    for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
        let mut options = PdfOptions {
            profile,
            metadata_epoch_seconds: Some(1_700_000_000),
            ..PdfOptions::default()
        };
        options.theme.page.size.width_pt = 500.0;
        options.theme.page.size.height_pt = 700.0;
        options.theme.page.margins.top_pt = 40.0;
        options.theme.page.margins.right_pt = 30.0;
        options.theme.page.margins.bottom_pt = 45.0;
        options.theme.page.margins.left_pt = 35.0;
        for (archival, settings) in [
            ("off", franken_markdown::PdfASettings::OFF),
            ("2b", franken_markdown::PdfASettings::a2b()),
        ] {
            let actual = pdf(profile, archival);
            let expected = franken_markdown::render_pdf_pdfa(SOURCE, &options, settings).unwrap();
            assert_eq!(
                actual.bytes(),
                expected,
                "profile={profile:?}, archival={archival}"
            );
            assert_eq!(actual.source_length(), SOURCE.len());
            let bytes = actual.bytes();
            let objects = String::from_utf8_lossy(&bytes);
            assert!(objects.contains("/MediaBox [0 0 500 700]"));
            assert_eq!(objects.contains("/OutputIntents"), archival == "2b");
        }
    }
    assert_eq!(
        pdf(None, "off").bytes(),
        pdf(Some(Profile::GfmPlus), "off").bytes()
    );
    assert_ne!(
        pdf(None, "off").bytes(),
        pdf(Some(Profile::CommonMarkGfm), "off").bytes()
    );
}

#[test]
fn svg_profiles_reach_the_shared_ast_and_keep_source_diagnostics() {
    for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
        let document = franken_markdown::parse_markdown_with_profile(SOURCE, profile);
        let document = franken_markdown::apply_profile(&document, profile);
        let expected = franken_markdown::render_svg(
            &document,
            &SvgOptions {
                max_width_pt: 420.0,
                ..SvgOptions::default()
            },
        );
        let actual = svg(profile);
        assert_eq!(actual.bytes(), expected);
        assert_eq!(actual.source_length(), SOURCE.len());
        assert_eq!(actual.diagnostics_json(), svg(None).diagnostics_json());
    }
    assert_eq!(svg(None).bytes(), svg(Some(Profile::GfmPlus)).bytes());
    assert_ne!(svg(None).bytes(), svg(Some(Profile::CommonMarkGfm)).bytes());
}

#[test]
fn epub_profiles_match_the_native_publication_without_dropping_source_text() {
    for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
        let document = franken_markdown::parse_markdown_with_profile(SOURCE, profile);
        let options = HtmlOptions {
            profile,
            custom_css: Some(String::new()),
            ..HtmlOptions::default()
        };
        let actual = epub(profile);
        assert_eq!(
            actual.bytes(),
            franken_markdown::render_epub(&document, &options).unwrap()
        );
        assert_eq!(actual.source_length(), SOURCE.len());
        assert_eq!(actual.diagnostics_json(), epub(None).diagnostics_json());
    }
    assert_eq!(epub(None).bytes(), epub(Some(Profile::GfmPlus)).bytes());
    assert_ne!(
        epub(None).bytes(),
        epub(Some(Profile::CommonMarkGfm)).bytes()
    );
}
