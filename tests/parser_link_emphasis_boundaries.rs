//! Link/image brackets retain their punctuation role when their labels parse.
//!
//! Exact expectations were checked against CommonMark.js 0.31.2. An isolated
//! label cannot replace the source brackets with paragraph-edge whitespace:
//! doing so changes delimiter flanking and the rule of three, including the
//! literal asterisks/underscores retained in image descriptions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans_with_profile;
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, Profile, parse_markdown_profiled_with_profile,
    parse_markdown_spanned_profiled_with_profile, parse_markdown_spanned_with_profile,
    parse_markdown_with_profile,
};

const PROFILES: [Option<Profile>; 3] = [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)];

fn document(source: &str, profile: Option<Profile>) -> Document {
    let parsed = parse_markdown_with_profile(source, profile);
    assert_eq!(
        parse_markdown_profiled_with_profile(source, profile).document,
        parsed
    );
    assert_eq!(
        parse_markdown_spanned_with_profile(source, profile).to_document(),
        parsed
    );
    assert_eq!(
        parse_markdown_spanned_profiled_with_profile(source, profile)
            .document
            .to_document(),
        parsed
    );
    parsed
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&document(source, None).blocks, &HtmlOptions::default())
}

const LITERAL_LABELS: [(&str, &str); 7] = [
    (r"**a\!\**", "**a!**"),
    (r#"__"a_"#, "__\"a_"),
    ("*_a**", "*_a**"),
    ("_«β__", "_«β__"),
    ("**é»*", "**é»*"),
    (r"*/u\``_**", "*/u``_**"),
    (r#"__a/u**"_"#, "__a/u**\"_"),
];

#[test]
fn link_labels_keep_literal_markers_at_both_punctuation_boundaries() {
    for profile in PROFILES {
        for (label, text) in LITERAL_LABELS {
            let definition = format!("\n\n[{label}]: /target\n");
            for source in [
                format!("[{label}](/target)"),
                format!("[{label}][r]\n\n[r]: /target\n"),
                format!("[{label}][]{definition}"),
                format!("[{label}]{definition}"),
            ] {
                assert_eq!(
                    document(&source, profile).blocks,
                    [Block::Paragraph(vec![Inline::Link {
                        dest: "/target".into(),
                        title: None,
                        content: vec![Inline::Text(text.into())],
                    }])],
                    "{source:?}"
                );
                let destinations = destination_spans_with_profile(&source, profile);
                assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
                assert_eq!(&source[destinations[0].range.clone()], "/target");
            }
        }
    }
}

#[test]
fn image_descriptions_do_not_lose_unmatched_emphasis_markers() {
    for profile in PROFILES {
        for (label, alt) in LITERAL_LABELS {
            for source in [
                format!("![{label}](/image)"),
                format!("![{label}][r]\n\n[r]: /image\n"),
                format!("![{label}][]\n\n[{label}]: /image\n"),
                format!("![{label}]\n\n[{label}]: /image\n"),
            ] {
                assert_eq!(
                    document(&source, profile).blocks,
                    [Block::Paragraph(vec![Inline::Image {
                        dest: "/image".into(),
                        title: None,
                        alt: alt.into(),
                    }])],
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn rendered_link_and_image_text_retains_exact_source_punctuation() {
    assert_eq!(
        fragment(r"[**a\!\**](/target)"),
        "<p><a href=\"/target\">**a!**</a></p>\n"
    );
    assert_eq!(
        fragment(r#"![__a/u**"_](/image)"#),
        "<p><img src=\"/image\" alt=\"__a/u**&quot;_\"></p>\n"
    );
    assert_eq!(
        fragment(r"[![*_a**](/image)](/target)"),
        "<p><a href=\"/target\"><img src=\"/image\" alt=\"*_a**\"></a></p>\n"
    );
}

#[test]
fn genuine_emphasis_whitespace_and_code_boundaries_keep_their_own_rules() {
    for (source, expected) in [
        (r"**a\!\**", "<p>*<em>a!*</em></p>\n"),
        (
            r"[ **a\!\** ](/target)",
            "<p><a href=\"/target\"> *<em>a!*</em> </a></p>\n",
        ),
        (
            "[__foo_](/target)",
            "<p><a href=\"/target\">_<em>foo</em></a></p>\n",
        ),
        (
            "[***foo***](/target)",
            "<p><a href=\"/target\"><em><strong>foo</strong></em></a></p>\n",
        ),
        (
            "[**é** and *β*](/target)",
            "<p><a href=\"/target\"><strong>é</strong> and <em>β</em></a></p>\n",
        ),
        (
            r"[`**a\!\**`](/target)",
            "<p><a href=\"/target\"><code>**a\\!\\**</code></a></p>\n",
        ),
        (
            "[~~foo~~](/target)",
            "<p><a href=\"/target\"><del>foo</del></a></p>\n",
        ),
        (r"[**a\!\**][missing]", "<p>[**a!**][missing]</p>\n"),
    ] {
        assert_eq!(fragment(source), expected, "{source:?}");
    }
}

#[test]
fn nested_containers_and_line_endings_keep_original_unicode_destinations() {
    for profile in PROFILES {
        for newline in ["\n", "\r\n", "\r"] {
            let source = "> - [**é»*](/réel)\n>   ![_«β__](/image)\n".replace('\n', newline);
            let parsed = document(&source, profile);
            let html =
                franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
            assert!(html.contains(">**é»*</a>"), "{html}");
            assert!(html.contains("alt=\"_«β__\""), "{html}");
            let destinations = destination_spans_with_profile(&source, profile);
            assert_eq!(destinations.len(), 2, "{source:?}: {destinations:?}");
            assert_eq!(&source[destinations[0].range.clone()], "/réel");
            assert_eq!(&source[destinations[1].range.clone()], "/image");
        }
    }
}
