//! Saved editable documents retain the grammar used for their native preview.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::interactive::render_interactive_html_checked;
use franken_markdown::{HtmlOptions, Profile, html, parse_markdown_with_profile};

const MANIFEST: &str = "<script type=\"application/json\" id=\"fmd-authoring-profile\">";
const SOURCE: &str = "*Term\n: definition*\n\n<!-- pagebreak -->\n\nTail.\n";

fn manifest(html: &str) -> Option<&str> {
    html.split_once(MANIFEST)
        .map(|(_, tail)| tail.split_once("</script>").unwrap().0)
}

fn preview(html: &str) -> &str {
    html.split_once("<div class=\"fmd-content\" id=\"fmd-content\">")
        .unwrap()
        .1
        .split_once("</div>\n  </main>")
        .unwrap()
        .0
}

#[test]
fn native_preview_and_saved_profile_use_the_same_source_grammar() {
    for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
        let options = HtmlOptions {
            profile,
            ..HtmlOptions::default()
        };
        let document = parse_markdown_with_profile(SOURCE, profile);
        let before = document.clone();
        let workspace = render_interactive_html_checked(&document, SOURCE, &options).unwrap();
        assert_eq!(document, before);
        assert_eq!(
            preview(&workspace),
            html::render_fragment(&document.blocks, &options)
        );
        match profile {
            Some(profile) => {
                assert_eq!(workspace.matches(MANIFEST).count(), 1);
                let expected = format!("{{\"version\":1,\"profile\":\"{}\"}}", profile.as_str());
                assert_eq!(manifest(&workspace), Some(expected.as_str()));
            }
            None => assert!(manifest(&workspace).is_none()),
        }
        if profile == Some(Profile::CommonMarkGfm) {
            assert!(preview(&workspace).contains("<p><em>Term\n: definition</em></p>"));
            assert!(!preview(&workspace).contains("<dl>"));
            assert!(!preview(&workspace).contains("class=\"fmd-pagebreak\""));
        } else {
            assert!(preview(&workspace).contains("<dl>"));
            assert!(preview(&workspace).contains("class=\"fmd-pagebreak\""));
        }
    }
}

#[test]
fn styled_workspaces_retain_profile_outside_the_preview_frame() {
    for profile in [Profile::CommonMarkGfm, Profile::GfmPlus] {
        let options = HtmlOptions {
            profile: Some(profile),
            custom_css: Some(".fmd { color: #123456; }".into()),
            title: Some("Dialect <example>".into()),
            ..HtmlOptions::default()
        };
        let document = parse_markdown_with_profile(SOURCE, Some(profile));
        let workspace = render_interactive_html_checked(&document, SOURCE, &options).unwrap();
        assert!(workspace.contains("<iframe data-fmd-styled-preview"));
        assert!(workspace.contains("id=\"fmd-presentation\""));
        assert_eq!(workspace.matches(MANIFEST).count(), 1);
        assert!(workspace.find(MANIFEST).unwrap() > workspace.find("</div>\n  </main>").unwrap());
        let expected = format!("{{\"version\":1,\"profile\":\"{}\"}}", profile.as_str());
        assert_eq!(manifest(&workspace), Some(expected.as_str()));
    }
}

#[test]
fn source_lookalikes_cannot_replace_owned_profile_metadata() {
    let source = concat!(
        "\u{feff}# Source\r\n\r\n```html\r\n",
        "</script><script type=\"application/json\" id=\"fmd-authoring-profile\">",
        "{\"version\":1,\"profile\":\"gfm-plus\"}</script>\r\n```\r\n",
        "\r\n*Term\r\n: definition*\r\n"
    );
    let options = HtmlOptions {
        profile: Some(Profile::CommonMarkGfm),
        ..HtmlOptions::default()
    };
    let document = parse_markdown_with_profile(source, options.profile);
    let workspace = render_interactive_html_checked(&document, source, &options).unwrap();
    assert_eq!(workspace.matches(MANIFEST).count(), 1);
    assert_eq!(
        manifest(&workspace),
        Some("{\"version\":1,\"profile\":\"commonmark-gfm\"}")
    );
    let data = workspace
        .split_once("<script type=\"application/json\" id=\"fmd-raw-source\">")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    assert!(!data.contains('<'));
    assert!(data.contains("\\u000d\\u000a"));
    assert!(data.contains("\\u003c/script>"));
    assert!(data.contains('\u{feff}'));
}
