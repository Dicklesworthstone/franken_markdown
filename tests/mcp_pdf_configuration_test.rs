//! PDF configuration must reach both rendering and source-layout verification.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::mcp::{
    JsonValue, base64_encode, handle_tool_call, parse_json, tools_list_result,
};
use franken_markdown::{
    FontAssetSlot, FontAssets, HtmlOptions, PageMargins, PageSize, PageStyle, PdfImageAsset,
    PdfOptions, Theme, parse_markdown,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

type Args = BTreeMap<String, JsonValue>;
const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="blue"/></svg>"#;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn args(json: &str, source: &str) -> Args {
    let mut args = parse_json(json).unwrap().as_object().unwrap().clone();
    args.insert("markdown".into(), JsonValue::String(source.into()));
    args
}
fn call(name: &str, args: Args) -> Result<JsonValue, (i32, String, &'static str)> {
    handle_tool_call(
        Some(&JsonValue::Object(BTreeMap::from([
            ("name".into(), JsonValue::String(name.into())),
            ("arguments".into(), JsonValue::Object(args)),
        ]))),
        1024 * 1024,
    )
}
fn content(result: &JsonValue) -> &str {
    let JsonValue::Array(items) = result.get("content").unwrap() else {
        panic!("content array")
    };
    items[0].get("text").unwrap().as_str().unwrap()
}
fn scratch() -> PathBuf {
    for _ in 0..1000 {
        let path = std::env::temp_dir().join(format!(
            "fmd-mcp-pdf-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create scratch directory: {error}"),
        }
    }
    panic!("scratch names exhausted")
}
fn custom_page() -> PageStyle {
    PageStyle {
        size: PageSize {
            name: "custom",
            width_pt: 300.0,
            height_pt: 300.0,
        },
        margins: PageMargins {
            top_pt: 20.0,
            right_pt: 20.0,
            bottom_pt: 20.0,
            left_pt: 20.0,
        },
    }
}

#[test]
fn custom_paper_and_numeric_typography_match_direct_pdf_bytes() {
    let source = "# Custom paper\n\nA paragraph.\n\n| A | B |\n|---|---|\n| One | Two |";
    let args = args(
        r#"{"page":{"size":{"widthPt":300,"heightPt":300},"margins":20},"baseFontSize":18,"headingScale":1.4,"tableFontSize":9,"pageNumbers":true}"#,
        source,
    );
    let opts = PdfOptions {
        theme: Theme {
            page: custom_page(),
            ..Default::default()
        },
        base_font_size: Some(18.0),
        heading_scale: Some(1.4),
        table_font_size: Some(9.0),
        page_numbers: true,
        ..Default::default()
    };
    let expected = franken_markdown::render_pdf_document(&parse_markdown(source), &opts).unwrap();
    assert_eq!(
        content(&call("fmd.render_pdf", args).unwrap()),
        base64_encode(&expected)
    );
    assert_ne!(
        expected,
        franken_markdown::render_pdf_document(&parse_markdown(source), &PdfOptions::default())
            .unwrap()
    );
}

#[test]
fn verification_uses_the_configured_pagination_not_default_paper() {
    let source =
        "A representative paragraph with enough words to wrap repeatedly on a small page.\n\n"
            .repeat(40);
    let args = args(
        r#"{"page":{"size":{"widthPt":300,"heightPt":300},"margins":20},"baseFontSize":18}"#,
        &source,
    );
    let options = PdfOptions {
        theme: Theme {
            page: custom_page(),
            ..Default::default()
        },
        base_font_size: Some(18.0),
        ..Default::default()
    };
    let doc = parse_markdown(&source);
    let expected = franken_markdown::verify::verify_pdf(&doc, &options).unwrap();
    let defaults = franken_markdown::verify::verify_pdf(&doc, &PdfOptions::default()).unwrap();
    assert!(expected.page_count > defaults.page_count);
    assert_eq!(
        content(&call("fmd.verify", args).unwrap()),
        franken_markdown::verify::to_json(&expected)
    );
}

#[test]
fn verification_keeps_supplied_images_and_font_faces() {
    let source = "# Assets\n\n![plot](plot.svg)\n\nProportional text to measure.";
    let image = base64_encode(SVG);
    let font = base64_encode(franken_markdown::text::bundled::CM_REGULAR);
    let arguments = args(
        &format!(
            r#"{{"images":[{{"destination":"plot.svg","base64":"{image}"}}],"fonts":[{{"slot":"body-regular","base64":"{font}"}}]}}"#
        ),
        source,
    );
    let fonts = FontAssets::default()
        .with_slot(
            FontAssetSlot::BodyRegular,
            franken_markdown::text::bundled::CM_REGULAR.to_vec(),
        )
        .unwrap();
    let options = PdfOptions {
        font_assets: fonts,
        image_assets: vec![PdfImageAsset::new("plot.svg", SVG.to_vec())],
        ..Default::default()
    };
    let doc = parse_markdown(source);
    let expected = franken_markdown::verify::verify_pdf(&doc, &options).unwrap();
    let defaults = franken_markdown::verify::verify_pdf(&doc, &PdfOptions::default()).unwrap();
    assert_ne!(expected.digest, defaults.digest);
    assert_eq!(
        content(&call("fmd.verify", arguments).unwrap()),
        franken_markdown::verify::to_json(&expected)
    );
}

#[test]
fn accessibility_filter_runs_after_configured_layout_and_recomputes_digest() {
    let source = "# Heading\n\n### Skipped level\n\n[click here](https://example.com)";
    let arguments = args(
        r#"{"a11y":true,"page":{"size":{"widthPt":300,"heightPt":300},"margins":20},"baseFontSize":16}"#,
        source,
    );
    let options = PdfOptions {
        theme: Theme {
            page: custom_page(),
            ..Default::default()
        },
        base_font_size: Some(16.0),
        ..Default::default()
    };
    let expected = franken_markdown::verify::filter_a11y(
        franken_markdown::verify::verify_pdf(&parse_markdown(source), &options).unwrap(),
    );
    assert!(!expected.findings.is_empty());
    assert_eq!(
        content(&call("fmd.verify", arguments).unwrap()),
        franken_markdown::verify::to_json(&expected)
    );
}

#[test]
fn pdf_defaults_remain_byte_identical_with_no_new_options() {
    let source = "# Defaults\n\nOrdinary text.";
    let doc = parse_markdown(source);
    let expected = franken_markdown::render_pdf_document(&doc, &PdfOptions::default()).unwrap();
    for config in [
        "{}",
        r#"{"page":{}}"#,
        r#"{"page":{"size":"letter","margins":72}}"#,
    ] {
        assert_eq!(
            content(&call("fmd.render_pdf", args(config, source)).unwrap()),
            base64_encode(&expected)
        );
    }
    let expected = franken_markdown::verify::to_json(
        &franken_markdown::verify::verify_pdf(&doc, &PdfOptions::default()).unwrap(),
    );
    assert_eq!(
        content(&call("fmd.verify", args("{}", source)).unwrap()),
        expected
    );
}

#[test]
fn paired_file_page_settings_affect_pdf_without_changing_html() {
    let source = "# Two targets\n\nDocument text.";
    let root = scratch();
    let input = root.join("source.md");
    std::fs::write(&input, source).unwrap();
    let mut arguments = args(
        r#"{"title":"Pair","to":"both","page":{"size":{"widthPt":300,"heightPt":300},"margins":20},"baseFontSize":18}"#,
        source,
    );
    arguments.remove("markdown");
    arguments.insert(
        "path".into(),
        JsonValue::String(input.to_string_lossy().into_owned()),
    );
    let result = call("fmd.render_file", arguments).unwrap();
    let result = parse_json(content(&result)).unwrap();
    let JsonValue::Array(outputs) = result.get("outputs").unwrap() else {
        panic!("outputs array")
    };
    let doc = parse_markdown(source);
    let html = HtmlOptions {
        title: Some("Pair".into()),
        ..Default::default()
    };
    let pdf = PdfOptions {
        title: Some("Pair".into()),
        theme: Theme {
            page: custom_page(),
            ..Default::default()
        },
        base_font_size: Some(18.0),
        ..Default::default()
    };
    assert_eq!(
        outputs[0].get("data").unwrap().as_str().unwrap(),
        franken_markdown::render_html_document(&doc, &html).unwrap()
    );
    assert_eq!(
        outputs[1].get("data").unwrap().as_str().unwrap(),
        base64_encode(&franken_markdown::render_pdf_document(&doc, &pdf).unwrap())
    );
}

#[test]
fn invalid_geometry_or_resource_payload_never_becomes_a_clean_report() {
    for (config, reason) in [
        (r#"{"page":{"margins":400}}"#, "invalid_page"),
        (r#"{"page":{"margins":{"left":20}}}"#, "invalid_page"),
        (r#"{"page":null}"#, "invalid_params"),
        (
            r#"{"images":[{"destination":"plot.svg","base64":"AB=="}]}"#,
            "invalid_resources",
        ),
        (
            r#"{"fonts":[{"slot":"body-regular","base64":"eA=="}]}"#,
            "invalid_resources",
        ),
        (r#"{"pdfA":"2b"}"#, "invalid_params"),
        (r#"{"pdfAStrict":false}"#, "invalid_params"),
    ] {
        assert_eq!(
            call("fmd.verify", args(config, "# Report")).unwrap_err().2,
            reason,
            "{config}"
        );
    }
}

#[test]
fn numeric_typography_contract_rejects_silent_clamping_and_wrong_types() {
    for config in [
        r#"{"baseFontSize":5.9}"#,
        r#"{"baseFontSize":24.1}"#,
        r#"{"baseFontSize":"18"}"#,
        r#"{"headingScale":1}"#,
        r#"{"headingScale":2.1}"#,
        r#"{"tableFontSize":4}"#,
    ] {
        for tool in ["fmd.render_pdf", "fmd.verify"] {
            assert_eq!(
                call(tool, args(config, "# Test")).unwrap_err().2,
                "invalid_params"
            );
        }
    }
}

#[test]
fn geometry_failure_precedes_file_io_and_preserves_existing_outputs() {
    let root = scratch();
    let missing = root.join("missing.md");
    let output = root.join("old.pdf");
    std::fs::write(&output, "previous artifact").unwrap();
    let mut arguments = args(r#"{"to":"pdf","page":{"size":"letter","margins":400}}"#, "");
    arguments.remove("markdown");
    arguments.insert(
        "path".into(),
        JsonValue::String(missing.to_string_lossy().into_owned()),
    );
    arguments.insert(
        "out".into(),
        JsonValue::String(output.to_string_lossy().into_owned()),
    );
    assert_eq!(
        call("fmd.render_file", arguments).unwrap_err().2,
        "invalid_page"
    );
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "previous artifact"
    );
    assert!(!missing.exists());
}

#[test]
fn named_paper_reaches_pdf_files_books_and_verification_in_both_orientations() {
    let source = format!(
        "# Physical paper\n\n{}",
        "A paragraph whose measured wrapping and page breaks depend on the chosen paper.\n\n"
            .repeat(16),
    );
    let root = scratch();
    let input = root.join("chapter.md");
    std::fs::write(&input, &source).unwrap();
    let doc = parse_markdown(&source);
    let book = BookRenderer::from_sources(
        &[BookInput {
            path: "chapter.md".into(),
            source: source.clone(),
        }],
        &[],
    )
    .unwrap();
    // Physical dimensions are independent of the implementation's paper table.
    for (name, width, height) in [
        ("letter", 612.0, 792.0),
        ("a4", 210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4),
        ("a5", 148.0 * 72.0 / 25.4, 210.0 * 72.0 / 25.4),
        ("legal", 612.0, 1008.0),
        ("tabloid", 792.0, 1224.0),
    ] {
        for orientation in ["portrait", "landscape"] {
            let (width, height) = if orientation == "landscape" {
                (height, width)
            } else {
                (width, height)
            };
            let config = format!(
                r#"{{"title":"Paper","page":{{"size":"{name}","orientation":"{orientation}","margins":{{"topPt":18,"rightPt":24,"bottomPt":30,"leftPt":36}}}},"baseFontSize":14,"pageNumbers":true}}"#,
            );
            let options = PdfOptions {
                title: Some("Paper".into()),
                theme: Theme {
                    page: PageStyle {
                        size: PageSize {
                            name: "custom",
                            width_pt: width as f32,
                            height_pt: height as f32,
                        },
                        margins: PageMargins {
                            top_pt: 18.0,
                            right_pt: 24.0,
                            bottom_pt: 30.0,
                            left_pt: 36.0,
                        },
                    },
                    ..Default::default()
                },
                base_font_size: Some(14.0),
                page_numbers: true,
                ..Default::default()
            };
            let expected = franken_markdown::render_pdf_document(&doc, &options).unwrap();
            assert_eq!(
                content(&call("fmd.render_pdf", args(&config, &source)).unwrap()),
                base64_encode(&expected),
                "inline {name} {orientation}",
            );
            let mut file_args = args(&config, &source);
            file_args.remove("markdown");
            file_args.insert("to".into(), JsonValue::String("pdf".into()));
            file_args.insert(
                "path".into(),
                JsonValue::String(input.to_string_lossy().into_owned()),
            );
            assert_eq!(
                content(&call("fmd.render_file", file_args).unwrap()),
                base64_encode(&expected),
                "file {name} {orientation}",
            );
            assert_eq!(
                content(&call("fmd.verify", args(&config, &source)).unwrap()),
                franken_markdown::verify::to_json(
                    &franken_markdown::verify::verify_pdf(&doc, &options).unwrap(),
                ),
                "verify {name} {orientation}",
            );
            let mut book_args = args(&config, &source);
            book_args.remove("markdown");
            book_args.insert(
                "chapters".into(),
                JsonValue::Array(vec![JsonValue::Object(BTreeMap::from([
                    ("path".into(), JsonValue::String("chapter.md".into())),
                    ("markdown".into(), JsonValue::String(source.clone())),
                ]))]),
            );
            let result = call("fmd.render_book", book_args).unwrap();
            let receipt = parse_json(content(&result)).unwrap();
            let JsonValue::Array(outputs) = receipt.get("outputs").unwrap() else {
                panic!("book outputs array")
            };
            assert_eq!(
                outputs[0].get("data").unwrap().as_str().unwrap(),
                base64_encode(
                    &book
                        .render_pdf_with_options(&options, franken_markdown::PdfASettings::OFF)
                        .unwrap(),
                ),
                "book {name} {orientation}",
            );
        }
    }
}

#[test]
fn book_geometry_is_admitted_before_source_expansion_and_has_no_non_pdf_consumer() {
    let mut arguments = args(r#"{"page":{"size":"a5","margins":400}}"#, "");
    arguments.remove("markdown");
    arguments.insert(
        "chapters".into(),
        JsonValue::Array(vec![JsonValue::Object(BTreeMap::from([
            ("path".into(), JsonValue::String("chapter.md".into())),
            (
                "markdown".into(),
                JsonValue::String("{{#include absent.md}}".into()),
            ),
        ]))]),
    );
    for target in ["pdf", "all"] {
        arguments.insert("to".into(), JsonValue::String(target.into()));
        assert_eq!(
            call("fmd.render_book", arguments.clone()).unwrap_err().2,
            "invalid_page",
        );
    }
    for target in ["epub", "site"] {
        arguments.insert("to".into(), JsonValue::String(target.into()));
        assert_eq!(
            call("fmd.render_book", arguments.clone()).unwrap_err().2,
            "unsupported_target_option",
        );
    }
}

#[test]
fn discovery_exposes_page_shape_and_auditable_resources_without_pdfa_claims() {
    let list = tools_list_result();
    let JsonValue::Array(tools) = list.get("tools").unwrap() else {
        panic!("tools array")
    };
    for tool in tools {
        let name = tool.get("name").unwrap().as_str().unwrap();
        let props = tool.get("inputSchema").unwrap().get("properties").unwrap();
        if matches!(
            name,
            "fmd.render_pdf" | "fmd.render_file" | "fmd.render_book" | "fmd.verify"
        ) {
            let page = props.get("page").unwrap();
            assert_eq!(page.get("type").unwrap().as_str(), Some("object"));
            assert_eq!(
                page.get("additionalProperties"),
                Some(&JsonValue::Bool(false))
            );
            let JsonValue::Array(sizes) = page
                .get("properties")
                .unwrap()
                .get("size")
                .unwrap()
                .get("oneOf")
                .unwrap()
            else {
                panic!("paper size choices")
            };
            assert_eq!(
                sizes[0].get("enum"),
                Some(&JsonValue::Array(
                    ["letter", "a4", "a5", "legal", "tabloid"]
                        .map(|name| JsonValue::String(name.into()))
                        .to_vec(),
                )),
            );
            assert!(props.get("images").is_some());
            assert!(props.get("fonts").is_some());
            assert_eq!(
                props
                    .get("baseFontSize")
                    .unwrap()
                    .get("minimum")
                    .unwrap()
                    .as_f64(),
                Some(6.0)
            );
        } else {
            assert!(props.get("page").is_none());
        }
        if name == "fmd.verify" {
            assert!(props.get("pdfA").is_none());
            assert!(props.get("pdfAStrict").is_none());
            assert!(props.get("a11y").is_some());
        }
    }
}
