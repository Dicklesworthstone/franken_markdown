//! Agent publication and verification consume the shared appearance presets.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::{BookInput, BookRenderer, render_book_site};
use franken_markdown::mcp::{
    ERROR_INVALID_OPTIONS, INVALID_PARAMS, JsonValue as Json, base64_encode, handle_tool_call,
    parse_json, tools_list_result,
};
use franken_markdown::{
    FontFamily, FontScale, HtmlOptions, PdfASettings, PdfOptions, SvgOptions, SystemAppearance,
    Theme, parse_markdown,
};

const SOURCE: &str = "# Appearance\n\nReadable **body** and [local navigation](#appearance).\n\n```rust\nlet answer = 42;\n```\n";
const TOOLS: [&str; 7] = [
    "fmd.render_html",
    "fmd.render_pdf",
    "fmd.render_svg",
    "fmd.render_epub",
    "fmd.render_file",
    "fmd.render_book",
    "fmd.verify",
];
type Args = BTreeMap<String, Json>;
type Failure = (i32, String, &'static str);

fn text(value: &str) -> Json {
    Json::String(value.into())
}

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(items) => items,
        _ => panic!("expected array"),
    }
}

fn call(name: &str, args: Args) -> Result<Json, Failure> {
    handle_tool_call(
        Some(&object([
            ("name", text(name)),
            ("arguments", Json::Object(args)),
        ])),
        1024 * 1024,
    )
}

fn content(result: &Json) -> &str {
    assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
    array(result.get("content").unwrap())[0]
        .get("text")
        .and_then(Json::as_str)
        .unwrap()
}

fn source_args() -> Args {
    BTreeMap::from([("markdown".into(), text(SOURCE))])
}

fn book_args() -> Args {
    BTreeMap::from([(
        "chapters".into(),
        Json::Array(vec![object([
            ("path", text("chapter.md")),
            ("markdown", text(SOURCE)),
        ])]),
    )])
}

fn palettes() -> [(&'static str, Theme); 6] {
    [
        ("auto", Theme::default()),
        (
            "light",
            Theme::default().with_appearance(SystemAppearance::Light),
        ),
        (
            "dark",
            Theme::default().with_appearance(SystemAppearance::Dark),
        ),
        ("charcoal", Theme::charcoal()),
        ("high-contrast-light", Theme::high_contrast_light()),
        ("high-contrast-dark", Theme::high_contrast_dark()),
    ]
}

#[test]
fn discovery_exposes_canonical_appearances_on_every_publication_and_pdf_audit() {
    let listing = tools_list_result();
    for name in TOOLS {
        let tool = array(listing.get("tools").unwrap())
            .iter()
            .find(|tool| tool.get("name").and_then(Json::as_str) == Some(name))
            .unwrap();
        let schema = tool.get("inputSchema").unwrap();
        let appearance = schema.get("properties").unwrap().get("appearance").unwrap();
        assert_eq!(appearance.get("type"), Some(&text("string")));
        assert_eq!(
            array(appearance.get("enum").unwrap()),
            palettes().map(|(name, _)| text(name))
        );
        assert!(!array(schema.get("required").unwrap()).contains(&text("appearance")));
        assert_eq!(schema.get("additionalProperties"), Some(&Json::Bool(false)));
    }
    // Link-only book checks have no rendered appearance to configure.
    let mut request = book_args();
    request.insert("appearance".into(), text("dark"));
    assert_eq!(
        call("fmd.verify_book", request).unwrap_err().0,
        INVALID_PARAMS
    );
}

#[test]
fn invalid_appearances_fail_before_source_io_or_resource_decoding() {
    for name in TOOLS {
        let request = if name == "fmd.render_file" {
            BTreeMap::from([("path".into(), text("/absent/appearance/source.md"))])
        } else if name == "fmd.render_book" {
            BTreeMap::from([("chapters".into(), Json::Array(vec![Json::Null]))])
        } else {
            source_args()
        };
        for invalid in [
            "",
            "Dark",
            " dark",
            "dark ",
            "high_contrast_dark",
            "unknown",
        ] {
            let mut invalid_request = request.clone();
            invalid_request.insert("appearance".into(), text(invalid));
            invalid_request.insert("images".into(), Json::Array(vec![Json::Null]));
            let failure = call(name, invalid_request).unwrap_err();
            assert_eq!(failure.0, ERROR_INVALID_OPTIONS, "{name}: {invalid:?}");
            assert_eq!(failure.2, "invalid_appearance", "{name}: {invalid:?}");
        }
        for invalid in [Json::Null, Json::Bool(false), Json::Number(1.0)] {
            let mut invalid_request = request.clone();
            invalid_request.insert("appearance".into(), invalid);
            assert_eq!(
                call(name, invalid_request).unwrap_err().0,
                INVALID_PARAMS,
                "{name}"
            );
        }
    }
}

#[test]
fn source_publications_match_presets_after_typography_overrides() {
    let document = parse_markdown(SOURCE);
    let scale = FontScale::parse("125%").unwrap();
    for (name, theme) in palettes() {
        let theme = theme.with_font(FontFamily::Serif).with_font_scale(scale);
        let html_options = HtmlOptions {
            theme: theme.clone(),
            ..Default::default()
        };
        let pdf_options = PdfOptions {
            theme: theme.clone(),
            ..Default::default()
        };
        let mut request = source_args();
        request.insert("appearance".into(), text(name));
        request.insert("font".into(), text("serif"));
        request.insert("fontScale".into(), text("125%"));
        for (tool, expected) in [
            (
                "fmd.render_html",
                franken_markdown::render_html_document(&document, &html_options).unwrap(),
            ),
            (
                "fmd.render_pdf",
                base64_encode(
                    &franken_markdown::render_pdf_document(&document, &pdf_options).unwrap(),
                ),
            ),
            (
                "fmd.render_epub",
                base64_encode(&franken_markdown::render_epub(&document, &html_options).unwrap()),
            ),
            (
                "fmd.render_svg",
                String::from_utf8(franken_markdown::render_svg(
                    &document,
                    &SvgOptions {
                        theme: theme.clone(),
                        ..Default::default()
                    },
                ))
                .unwrap(),
            ),
        ] {
            assert_eq!(
                content(&call(tool, request.clone()).unwrap()),
                expected,
                "{tool}: {name}"
            );
            if name == "auto" {
                let mut legacy = request.clone();
                legacy.remove("appearance");
                assert_eq!(
                    content(&call(tool, legacy).unwrap()),
                    expected,
                    "{tool}: omitted appearance"
                );
            }
        }
    }
}

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-mcp-appearance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn paired_file_exports_share_the_palette_and_invalid_presets_preserve_outputs() {
    let workspace = Workspace::new();
    let source = workspace.0.join("document.md");
    let output = workspace.0.join("publication.html");
    std::fs::write(&source, SOURCE).unwrap();
    let mut request = BTreeMap::from([
        ("path".into(), text(&source.to_string_lossy())),
        ("out".into(), text(&output.to_string_lossy())),
        ("to".into(), text("both")),
        ("appearance".into(), text("high-contrast-dark")),
    ]);
    call("fmd.render_file", request.clone()).unwrap();
    // File exports keep the documented filename-stem title when none is given
    // (docs/MCP_FILE_OPTIONS.md), unlike in-memory renders of the same source.
    let title = Some("document".to_string());
    let expected_html = franken_markdown::render_html(
        SOURCE,
        &HtmlOptions {
            theme: Theme::high_contrast_dark(),
            title: title.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    let expected_pdf = franken_markdown::render_pdf(
        SOURCE,
        &PdfOptions {
            theme: Theme::high_contrast_dark(),
            title: title.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&output).unwrap(), expected_html);
    assert_eq!(
        std::fs::read(output.with_extension("pdf")).unwrap(),
        expected_pdf
    );
    request.insert("appearance".into(), text("high_contrast_dark"));
    assert_eq!(
        call("fmd.render_file", request).unwrap_err().2,
        "invalid_appearance"
    );
    assert_eq!(std::fs::read_to_string(&output).unwrap(), expected_html);
    assert_eq!(
        std::fs::read(output.with_extension("pdf")).unwrap(),
        expected_pdf
    );
    assert_eq!(std::fs::read_to_string(&source).unwrap(), SOURCE);

    let mut svg = BTreeMap::from([
        ("path".into(), text(&source.to_string_lossy())),
        ("to".into(), text("svg")),
        ("appearance".into(), text("high-contrast-dark")),
    ]);
    let expected_svg = String::from_utf8(franken_markdown::render_svg(
        &parse_markdown(SOURCE),
        &SvgOptions {
            theme: Theme::high_contrast_dark(),
            ..Default::default()
        },
    ))
    .unwrap();
    assert_eq!(
        content(&call("fmd.render_file", svg.clone()).unwrap()),
        expected_svg
    );
    svg.insert("to".into(), text("epub"));
    let epub = franken_markdown::render_epub(
        &parse_markdown(SOURCE),
        &HtmlOptions {
            theme: Theme::high_contrast_dark(),
            title,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        content(&call("fmd.render_file", svg).unwrap()),
        base64_encode(&epub)
    );
}

#[test]
fn book_editions_share_named_high_contrast_palettes_across_all_artifacts() {
    let renderer = BookRenderer::new(&[BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }])
    .unwrap();
    for (name, theme) in [
        ("high-contrast-light", Theme::high_contrast_light()),
        ("high-contrast-dark", Theme::high_contrast_dark()),
    ] {
        let mut request = book_args();
        request.insert("to".into(), text("all"));
        request.insert("appearance".into(), text(name));
        let result = call("fmd.render_book", request).unwrap();
        let receipt = parse_json(content(&result)).unwrap();
        let html_options = HtmlOptions {
            theme: theme.clone(),
            ..Default::default()
        };
        let pdf_options = PdfOptions {
            theme,
            ..Default::default()
        };
        for (output, (format, bytes)) in array(receipt.get("outputs").unwrap()).iter().zip([
            (
                "pdf",
                renderer
                    .render_pdf_with_options(&pdf_options, PdfASettings::OFF)
                    .unwrap(),
            ),
            (
                "epub",
                franken_markdown::epub::render_book_epub(renderer.book(), &html_options).unwrap(),
            ),
            (
                "site",
                render_book_site(renderer.book(), &html_options).unwrap(),
            ),
        ]) {
            assert_eq!(output.get("format").and_then(Json::as_str), Some(format));
            assert_eq!(
                output.get("data").and_then(Json::as_str),
                Some(base64_encode(&bytes).as_str()),
                "{name}: {format}"
            );
        }
    }
}

#[test]
fn verification_matches_the_explicit_palette_used_for_publication() {
    let mut request = source_args();
    request.insert("appearance".into(), text("high-contrast-dark"));
    let expected = franken_markdown::verify::verify_pdf_result(
        &parse_markdown(SOURCE),
        &PdfOptions {
            theme: Theme::high_contrast_dark(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        content(&call("fmd.verify", request).unwrap()),
        franken_markdown::verify::to_json(&expected)
    );
}
