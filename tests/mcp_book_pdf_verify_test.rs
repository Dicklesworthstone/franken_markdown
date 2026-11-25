//! Actual assembled-book PDF verification through the agent-facing MCP route.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, JsonValue as Json, MAX_FRAME_BYTES, base64_encode,
    handle_tool_call, parse_json, read_frame, serve_stdio, write_frame,
};
use franken_markdown::{
    FontAssetSlot, FontAssets, PageMargins, PageSize, PageStyle, PdfASettings, PdfImageAsset,
    PdfOptions, PdfRunningBand, PdfRunningContent, Profile,
};

type Failure = (i32, String, &'static str);
const ONE: &str = "---\nlang: de\n---\n# Shared\n\n{{#include ../parts/body.md}}\n";
const TWO: &str = "---\nlang: en\n---\n# Shared\n\n[Return](guide/one.md#shared). Another note[^n].\n\n[^n]: Second chapter note.";
const INCLUDED: &str = "[Next](../two.md#shared). First note[^n].\n\n![Plot](plot.svg)\n\n[^n]: First chapter note.";
const SVG: &[u8] = br##"<svg width="40" height="20" xmlns="http://www.w3.org/2000/svg"><rect width="40" height="20" fill="#246"/></svg>"##;

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}
fn text(value: &str) -> Json {
    Json::String(value.to_string())
}
fn source(path: &str, markdown: &str) -> Json {
    object([("path", text(path)), ("markdown", text(markdown))])
}
fn input(path: &str, markdown: &str) -> BookInput {
    BookInput {
        path: path.to_string(),
        source: markdown.to_string(),
    }
}
fn set(args: &mut Json, key: &str, value: Json) {
    let Json::Object(fields) = args else {
        panic!("expected object")
    };
    fields.insert(key.to_string(), value);
}
fn remove(args: &mut Json, key: &str) {
    let Json::Object(fields) = args else {
        panic!("expected object")
    };
    fields.remove(key);
}
fn array(value: &Json) -> &[Json] {
    let Json::Array(items) = value else {
        panic!("expected array")
    };
    items
}
fn args(markdown: &str) -> Json {
    object([
        ("target", text("pdf")),
        ("chapters", Json::Array(vec![source("one.md", markdown)])),
    ])
}
fn call(name: &str, args: Json, limit: u64) -> Result<Json, Failure> {
    handle_tool_call(
        Some(&object([("name", text(name)), ("arguments", args)])),
        limit,
    )
}
fn verify(args: Json) -> Result<Json, Failure> {
    call("fmd.verify_book", args, DEFAULT_MAX_INPUT_BYTES)
}
fn content(result: &Json) -> &str {
    assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
    array(result.get("content").unwrap())[0]
        .get("text")
        .and_then(Json::as_str)
        .unwrap()
}
fn report(args: Json) -> Json {
    parse_json(content(&verify(args).unwrap())).unwrap()
}
fn core() -> BookRenderer {
    BookRenderer::from_sources_with_profile(
        &[input("guide/one.md", ONE), input("two.md", TWO)],
        &[input("parts/body.md", INCLUDED)],
        Some(Profile::CommonMarkGfm),
    )
    .unwrap()
}

#[test]
fn pdf_target_uses_the_exact_book_options_sources_and_resources() {
    let options = PdfOptions {
        profile: Some(Profile::CommonMarkGfm),
        title: Some("Configured edition".into()),
        lang: Some("fr".into()),
        toc: true,
        toc_depth: Some(3),
        page_numbers: true,
        footnote_backlinks: true,
        running: PdfRunningContent {
            header: PdfRunningBand {
                left: Some("{chapter}".into()),
                right: Some("{page}/{pages}".into()),
                ..Default::default()
            },
            ..Default::default()
        },
        font_assets: FontAssets::default()
            .with_slot(
                FontAssetSlot::BodyRegular,
                franken_markdown::text::bundled::CM_REGULAR.to_vec(),
            )
            .unwrap(),
        image_assets: vec![PdfImageAsset::new("parts/plot.svg", SVG.to_vec())],
        theme: franken_markdown::Theme {
            page: PageStyle {
                size: PageSize {
                    name: "custom",
                    width_pt: 360.0,
                    height_pt: 504.0,
                },
                margins: PageMargins {
                    top_pt: 36.0,
                    right_pt: 36.0,
                    bottom_pt: 36.0,
                    left_pt: 36.0,
                },
                baseline_grid: None,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let request = object([
        ("target", text("pdf")),
        ("profile", text("commonmark-gfm")),
        (
            "chapters",
            Json::Array(vec![source("guide/one.md", ONE), source("two.md", TWO)]),
        ),
        (
            "includeSources",
            Json::Array(vec![source("parts/body.md", INCLUDED)]),
        ),
        ("title", text("Configured edition")),
        ("lang", text("fr")),
        ("toc", Json::Bool(true)),
        ("tocDepth", Json::Number(3.0)),
        ("pageNumbers", Json::Bool(true)),
        ("footnoteBacklinks", Json::Bool(true)),
        (
            "running",
            parse_json(r#"{"header":{"left":"{chapter}","right":"{page}/{pages}"}}"#).unwrap(),
        ),
        (
            "page",
            parse_json(r#"{"size":{"widthPt":360,"heightPt":504},"margins":36}"#).unwrap(),
        ),
        (
            "images",
            Json::Array(vec![object([
                ("destination", text("parts/plot.svg")),
                ("base64", text(&base64_encode(SVG))),
            ])]),
        ),
        (
            "fonts",
            Json::Array(vec![object([
                ("slot", text("body-regular")),
                (
                    "base64",
                    text(&base64_encode(franken_markdown::text::bundled::CM_REGULAR)),
                ),
            ])]),
        ),
    ]);
    let expected = core()
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap()
        .to_json()
        .unwrap();
    let actual = verify(request.clone()).unwrap();
    assert_eq!(content(&actual), expected);
    assert_eq!(actual, verify(request).unwrap());
    let parsed = parse_json(&expected).unwrap();
    assert_eq!(
        parsed.get("schema").and_then(Json::as_str),
        Some("fmd-book-pdf-verification-v1")
    );
    assert!(parsed.get("layout").is_some());
    assert!(parsed.get("destinations").is_some());
    assert!(!expected.contains("missing_image"));
    assert!(!expected.contains("fmd-book-pdf-nav/"));
}

#[test]
fn omitted_and_explicit_navigation_keep_the_original_report_bytes() {
    let markdown = "# Top\n\n[missing](#absent)";
    let renderer = BookRenderer::new(&[input("one.md", markdown)]).unwrap();
    let expected = renderer.validate_links_with_sources().unwrap().to_json();
    let mut request = args(markdown);
    remove(&mut request, "target");
    let old = verify(request.clone()).unwrap();
    assert_eq!(content(&old), expected);
    set(&mut request, "target", text("navigation"));
    assert_eq!(verify(request).unwrap(), old);
}

#[test]
fn invalid_targets_and_publication_only_fields_are_refused() {
    for (name, value, reason) in [
        ("target", text("epub"), "unsupported_target"),
        ("target", Json::Bool(true), "invalid_params"),
        ("maxReportBytes", Json::Number(0.0), "invalid_params"),
        (
            "maxReportBytes",
            Json::Number(16_777_217.0),
            "invalid_params",
        ),
        ("maxReportBytes", Json::Number(1.5), "invalid_params"),
        ("out", text("/not-written/book.pdf"), "invalid_params"),
        ("to", text("pdf"), "invalid_params"),
        ("strictLinks", Json::Bool(true), "invalid_params"),
        ("customCss", text("body {}"), "invalid_params"),
    ] {
        let mut request = args("# Top");
        set(&mut request, name, value);
        assert_eq!(verify(request).unwrap_err().2, reason, "{name}");
    }
    let mut request = args("# Top");
    set(&mut request, "target", text("navigation"));
    set(&mut request, "images", Json::Array(Vec::new()));
    assert_eq!(verify(request).unwrap_err().2, "invalid_params");
}

#[test]
fn pdf_options_and_source_limits_precede_resource_decoding() {
    let mut request = args("# Top");
    set(
        &mut request,
        "images",
        parse_json(r#"[{"destination":"x","base64":"?"}]"#).unwrap(),
    );
    assert_eq!(verify(request.clone()).unwrap_err().2, "invalid_resources");
    set(&mut request, "baselineGridPt", Json::Number(12.0));
    set(&mut request, "fitToPages", Json::Number(1.0));
    assert_eq!(
        verify(request).unwrap_err().2,
        "baseline_grid_incompatible"
    );

    let mut request = args("# Top");
    set(
        &mut request,
        "fonts",
        parse_json(r#"[{"slot":"body-regular","base64":"?"}]"#).unwrap(),
    );
    assert_eq!(
        call("fmd.verify_book", request, 1).unwrap_err().2,
        "input_too_large"
    );
}

#[test]
fn pdf_verification_keeps_strict_pagination_failure_typed() {
    let mut request = args("A  \nB  \nC");
    set(
        &mut request,
        "page",
        parse_json(r#"{"margins":{"topPt":360,"bottomPt":360}}"#).unwrap(),
    );
    set(&mut request, "baseFontSize", Json::Number(24.0));
    set(&mut request, "strictPagination", Json::Bool(true));
    let failure = verify(request.clone()).unwrap_err();
    assert_eq!(failure.2, "pagination_infeasible");

    remove(&mut request, "target");
    set(&mut request, "to", text("pdf"));
    let publication = call("fmd.render_book", request, DEFAULT_MAX_INPUT_BYTES).unwrap_err();
    assert_eq!(failure.0, publication.0);
    assert_eq!(failure.2, publication.2);
}

#[test]
fn report_limit_counts_all_content_items_and_never_returns_a_prefix() {
    for target in ["navigation", "pdf"] {
        let mut request = args("# Top\n\n[broken]:\n\n~~~rust\nunclosed");
        set(&mut request, "target", text(target));
        let complete = verify(request.clone()).unwrap();
        let items = array(complete.get("content").unwrap());
        assert_eq!(items.len(), 2);
        let bytes: usize = items
            .iter()
            .map(|item| item.get("text").and_then(Json::as_str).unwrap().len())
            .sum();
        assert!(items[1]
            .get("text")
            .and_then(Json::as_str)
            .unwrap()
            .contains("unclosed_fence"));

        set(&mut request, "maxReportBytes", Json::Number(bytes as f64));
        assert_eq!(verify(request.clone()).unwrap(), complete);
        set(
            &mut request,
            "maxReportBytes",
            Json::Number((bytes - 1) as f64),
        );
        assert_eq!(verify(request).unwrap_err().2, "report_too_large");
    }
}

#[test]
fn missing_links_keep_original_include_provenance_in_the_pdf_report() {
    let request = object([
        ("target", text("pdf")),
        (
            "chapters",
            Json::Array(vec![source(
                "guide/one.md",
                "# Top\n\n{{#include ../parts/links.md}}",
            )]),
        ),
        (
            "includeSources",
            Json::Array(vec![source(
                "parts/links.md",
                "[first](#absent-one) [second](#absent-two)",
            )]),
        ),
    ]);
    let actual = report(request);
    let navigation = actual.get("navigation").unwrap().to_json_string();
    for expected in ["absent-one", "absent-two", "parts/links.md", "destination"] {
        assert!(navigation.contains(expected), "{expected}: {navigation}");
    }
    assert!(!actual.to_json_string().contains("fmd-book-pdf-nav/"));
}

#[test]
fn archival_options_reuse_the_real_book_writer_without_claiming_certification() {
    for (mode, settings) in [
        ("off", PdfASettings::OFF),
        ("2b", PdfASettings::a2b()),
    ] {
        let markdown = "# Archive\n\nBody[^n].\n\n[^n]: Complete note.";
        let mut request = args(markdown);
        set(&mut request, "pdfA", text(mode));
        let expected = BookRenderer::new(&[input("one.md", markdown)])
            .unwrap()
            .verify_pdf_with_options(&PdfOptions::default(), settings)
            .unwrap()
            .to_json()
            .unwrap();
        assert_eq!(content(&verify(request).unwrap()), expected);
    }
    let mut request = args("# Top");
    set(&mut request, "pdfAStrict", Json::Bool(true));
    assert_eq!(verify(request).unwrap_err().2, "invalid_pdf_a");
}

#[test]
fn framed_session_recovers_from_refusal_and_keeps_reports_request_local() {
    let markdown = "# Top\n\n[up](#top)";
    let mut rejected = args(markdown);
    set(&mut rejected, "maxReportBytes", Json::Number(1.0));
    let requests = [rejected, args(markdown), args("# Renamed\n\n[up](#top)")];
    let mut input = Vec::new();
    for (index, arguments) in requests.into_iter().enumerate() {
        let request = object([
            ("jsonrpc", text("2.0")),
            ("id", Json::Number((index + 1) as f64)),
            ("method", text("tools/call")),
            (
                "params",
                object([("name", text("fmd.verify_book")), ("arguments", arguments)]),
            ),
        ]);
        write_frame(&mut input, &request.to_json_string()).unwrap();
    }
    let mut output = Vec::new();
    assert_eq!(
        serve_stdio(
            &mut Cursor::new(input),
            &mut output,
            DEFAULT_MAX_INPUT_BYTES,
            MAX_FRAME_BYTES,
        )
        .unwrap(),
        3
    );
    let mut reader = Cursor::new(output);
    let mut replies = Vec::new();
    while let Some(frame) = read_frame(&mut reader, MAX_FRAME_BYTES).unwrap() {
        replies.push(parse_json(&frame).unwrap());
    }
    assert_eq!(replies.len(), 3);
    assert_eq!(
        replies[0]
            .get("error")
            .unwrap()
            .get("data")
            .unwrap()
            .get("reason")
            .and_then(Json::as_str),
        Some("report_too_large")
    );
    let clean = parse_json(content(replies[1].get("result").unwrap())).unwrap();
    let broken = parse_json(content(replies[2].get("result").unwrap())).unwrap();
    assert_eq!(
        clean
            .get("navigation")
            .unwrap()
            .get("summary")
            .unwrap()
            .get("findings")
            .and_then(Json::as_u64),
        Some(0)
    );
    assert_eq!(
        broken
            .get("navigation")
            .unwrap()
            .get("summary")
            .unwrap()
            .get("findings")
            .and_then(Json::as_u64),
        Some(1)
    );
}
