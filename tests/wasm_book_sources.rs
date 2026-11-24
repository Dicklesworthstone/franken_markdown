//! One-shot PDF source roles and options must reach the real book renderer.
#![cfg(feature = "wasm-bindgen")]
#![allow(clippy::expect_used)]

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::wasm_abi::{
    FmdRenderResult, render_book_pdf_configured_page, render_book_pdf_configured_sources,
};
use franken_markdown::{PdfImageAsset, PdfRunningImage, PdfRunningImagePosition, Profile};

const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;

fn file(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn capture() -> (Vec<BookInput>, Vec<BookInput>) {
    (
        vec![
            file(
                "guide/one.md",
                "---\ntitle: One\nlang: de-DE\n---\n# One\n\n{{#include ../parts/shared.md}}\n\n[Next](../two.md#two)\n",
            ),
            file("two.md", "# Two\n\n[Previous](guide/one.md#one)\n"),
        ],
        vec![
            file(
                "parts/shared.md",
                "## Included\n\nShared *prose* with a citation[^n], [next](../two.md#two), and ![Chart](chart.svg).\n\n| Item | Value |\n| --- | --- |\n| Count | 42 |\n\n\x60\x60\x60rust\nlet answer = 42;\n\x60\x60\x60\n\n{{#include rows.txt:2:3}}\n\n[^n]: A source-local note.\n",
            ),
            file(
                "parts/rows.txt",
                "omitted\nsecond line\nthird line\nomitted\n",
            ),
        ],
    )
}

fn geometry() -> Vec<f64> {
    vec![512.0, 420.0, 60.0, 36.0, 60.0, 36.0]
}

fn publish(
    chapters: &[BookInput],
    resources: &[BookInput],
    expand: bool,
    profile: Option<&str>,
    presentation: bool,
) -> FmdRenderResult {
    render_book_pdf_configured_sources(
        chapters.iter().map(|file| file.path.clone()).collect(),
        chapters.iter().map(|file| file.source.clone()).collect(),
        presentation.then(|| "serif".into()),
        None,
        presentation.then(|| "Manual".into()),
        presentation.then(|| "  Élodie 中  ".into()),
        presentation.then_some(0.0),
        false,
        presentation,
        if presentation {
            vec!["parts/chart.svg".into(), "brand.svg".into()]
        } else {
            vec![]
        },
        if presentation {
            [SVG, SVG].concat()
        } else {
            vec![]
        },
        if presentation {
            vec![SVG.len() as u32; 2]
        } else {
            vec![]
        },
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        presentation.then_some(12.0),
        presentation.then_some(1.3),
        presentation.then_some(10.0),
        presentation,
        presentation.then_some(1.0),
        presentation.then(|| "en".into()),
        presentation,
        presentation.then_some(3),
        None,
        presentation,
        if presentation { geometry() } else { vec![] },
        if presentation {
            ["{title}", "", "{author}", "", "{page} / {pages}", "{date}"]
                .map(String::from)
                .to_vec()
        } else {
            vec![]
        },
        presentation,
        presentation,
        false,
        if presentation {
            vec!["brand.svg".into(), String::new()]
        } else {
            vec![]
        },
        if presentation { vec![1, 0] } else { vec![] },
        if presentation { vec![20, 0] } else { vec![] },
        presentation.then(|| "footnote-backlinks".into()),
        if presentation { 13.125 } else { 0.0 },
        profile.map(str::to_string),
        resources.iter().map(|file| file.path.clone()).collect(),
        resources.iter().map(|file| file.source.clone()).collect(),
        expand,
    )
    .expect("source-aware PDF")
}

// Configure through independent public fields, not the binding's token or
// argument helpers, so dropped/shifted settings change the expected PDF.
fn configure(renderer: &mut BookRenderer) {
    let options = renderer.options_mut();
    options.theme = franken_markdown::Theme::serif();
    options.theme.page =
        franken_markdown::PageStyle::from_points([512.0, 420.0], [60.0, 36.0, 60.0, 36.0])
            .expect("page geometry");
    options.theme.page.baseline_grid =
        Some(franken_markdown::BaselineGrid::from_points(13.125).expect("body grid"));
    options.title = Some("Manual".into());
    options.author = Some("  Élodie 中  ".into());
    options.metadata_epoch_seconds = Some(0);
    options.code_line_numbers = true;
    options.base_font_size = Some(12.0);
    options.heading_scale = Some(1.3);
    options.table_font_size = Some(10.0);
    options.page_numbers = true;
    options.font_scale = Some(1.0);
    options.lang = Some("en".into());
    options.toc = true;
    options.toc_depth = Some(3);
    options.microtype = franken_markdown::layout::MicrotypeOptions::CONSERVATIVE;
    options.footnote_backlinks = true;
    options.pdf_image_assets = ["parts/chart.svg", "brand.svg"]
        .map(|destination| PdfImageAsset {
            destination: destination.into(),
            bytes: SVG.to_vec(),
        })
        .to_vec();
    options.running.header.left = Some("{title}".into());
    options.running.header.right = Some("{author}".into());
    options.running.header.rule = true;
    options.running.header.image = Some(PdfRunningImage {
        dest: "brand.svg".into(),
        position: PdfRunningImagePosition::Right,
        height_pt: Some(20),
    });
    options.running.footer.center = Some("{page} / {pages}".into());
    options.running.footer.right = Some("{date}".into());
    options.running.footer.rule = true;
}

#[test]
fn source_aware_pdf_keeps_nested_resources_out_of_chapters_and_preserves_configuration() {
    let (chapters, resources) = capture();
    for (name, profile) in [
        (None, None),
        (Some("gfm-plus"), Some(Profile::GfmPlus)),
        (Some("commonmark-gfm"), Some(Profile::CommonMarkGfm)),
    ] {
        let actual = publish(&chapters, &resources, true, name, true);
        let mut expected = BookRenderer::from_sources_with_profile(&chapters, &resources, profile)
            .expect("native book with separate resources");
        configure(&mut expected);
        assert_eq!(expected.book().chapters.len(), 2);
        assert_eq!(actual.bytes(), expected.render_pdf().expect("native PDF"));
        assert_eq!(actual.format(), "book-pdf");
        assert_eq!(actual.mime_type(), "application/pdf");
        assert_eq!(actual.extension(), "pdf");
        assert_eq!(
            actual.source_length(),
            chapters
                .iter()
                .chain(&resources)
                .map(|file| file.source.len())
                .sum::<usize>()
        );
        assert_eq!(actual.source_length(), expected.source_length());
        assert!(String::from_utf8_lossy(&actual.bytes()).contains("/MediaBox [0 0 512 420]"));
        assert_eq!(
            actual.bytes(),
            publish(&chapters, &resources, true, name, true).bytes()
        );
    }

    // Independently written expansion: resource-relative links/images still
    // address the original file's directory, and .txt selector tails disappear.
    let expanded = [
        file(
            "guide/one.md",
            "---\ntitle: One\nlang: de-DE\n---\n# One\n\n## Included\n\nShared *prose* with a citation[^n], [next](../two.md#two), and ![Chart](../parts/chart.svg).\n\n| Item | Value |\n| --- | --- |\n| Count | 42 |\n\n\x60\x60\x60rust\nlet answer = 42;\n\x60\x60\x60\n\nsecond line\nthird line\n\n[^n]: A source-local note.\n\n[Next](../two.md#two)\n",
        ),
        chapters[1].clone(),
    ];
    let mut expected = BookRenderer::new(&expanded).expect("pre-expanded book");
    configure(&mut expected);
    assert_eq!(
        publish(&chapters, &resources, true, None, true).bytes(),
        expected.render_pdf().expect("pre-expanded PDF")
    );
    assert!(chapters[0].source.contains("{{#include"));
    assert!(resources[1].source.starts_with("omitted"));
}

#[test]
fn parse_only_pdf_keeps_literal_directives_without_attempting_resolution() {
    let chapters = [file("one.md", "# One\n\n{{#include missing.md}}\n")];
    assert!(BookRenderer::from_sources(&chapters, &[]).is_err());
    for (name, profile) in [
        (None, None),
        (Some("gfm-plus"), Some(Profile::GfmPlus)),
        (Some("commonmark-gfm"), Some(Profile::CommonMarkGfm)),
    ] {
        let actual = publish(&chapters, &[], false, name, false);
        let expected = BookRenderer::new_with_profile(&chapters, profile).expect("parse-only book");
        assert_eq!(actual.bytes(), expected.render_pdf().expect("literal PDF"));
        assert_eq!(actual.source_length(), chapters[0].source.len());
    }
}

#[test]
fn source_dialect_protects_multiline_code_before_include_expansion() {
    let chapters = [file(
        "one.md",
        "# Source\n\n*Term\n: definition*\n\n\x60Term\n: definition\n{{#include absent.md}}\ntail\x60\n",
    )];
    let expected =
        BookRenderer::from_sources_with_profile(&chapters, &[], Some(Profile::CommonMarkGfm))
            .expect("CommonMark protects include-shaped inline code");
    assert_eq!(
        publish(&chapters, &[], true, Some("commonmark-gfm"), false).bytes(),
        expected.render_pdf().expect("profiled PDF")
    );
}

#[test]
fn empty_include_capture_preserves_the_older_configured_page_output() {
    let chapters = [
        file("one.md", "# One\n\n{{#include two.md}}\n"),
        file("two.md", "# Two\n\nA second chapter.\n"),
    ];
    let legacy = render_book_pdf_configured_page(
        chapters.iter().map(|file| file.path.clone()).collect(),
        chapters.iter().map(|file| file.source.clone()).collect(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
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
        vec![],
    )
    .expect("previous configured PDF entry point");
    let actual = publish(&chapters, &[], true, None, false);
    assert_eq!(actual.bytes(), legacy.bytes());
    assert_eq!(actual.diagnostics_json(), legacy.diagnostics_json());
    assert_eq!(actual.source_length(), legacy.source_length());
}
