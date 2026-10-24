//! End-to-end MCP dispatch into the real chapter-aware archival writer.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;

use franken_markdown::book::{BookInput, BookRenderer, render_book_site};
use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, JsonValue as Json, base64_encode, handle_tool_call, parse_json,
    read_frame, serve_stdio, write_frame,
};
use franken_markdown::{
    HtmlOptions, PageMargins, PageSize, PageStyle, PdfASettings, PdfImageAsset, PdfOptions,
};

type Failure = (i32, String, &'static str);
const ONE: &str = "# One\n\n[Next](two.md#two)\n\n{{#include shared.md}}";
const TWO: &str = "# Two\n\n[Back](one.md#one)";
const SHARED: &str = "Shared **content**.";

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn text(value: &str) -> Json {
    Json::String(value.into())
}
fn source(path: &str, markdown: &str) -> Json {
    object([("path", text(path)), ("markdown", text(markdown))])
}
fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}
fn args(to: &str) -> Json {
    object([
        (
            "chapters",
            Json::Array(vec![source("one.md", ONE), source("two.md", TWO)]),
        ),
        (
            "includeSources",
            Json::Array(vec![source("shared.md", SHARED)]),
        ),
        ("to", text(to)),
    ])
}
fn core() -> BookRenderer {
    BookRenderer::from_sources(
        &[input("one.md", ONE), input("two.md", TWO)],
        &[input("shared.md", SHARED)],
    )
    .unwrap()
}
fn set(args: &mut Json, key: &str, value: Json) {
    let Json::Object(fields) = args else {
        panic!("expected object")
    };
    fields.insert(key.into(), value);
}
fn params(args: Json) -> Json {
    object([("name", text("fmd.render_book")), ("arguments", args)])
}
fn invoke(args: Json) -> Result<Json, Failure> {
    handle_tool_call(Some(&params(args)), DEFAULT_MAX_INPUT_BYTES)
}
fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(items) => items,
        _ => panic!("expected array"),
    }
}
fn receipt(result: &Json) -> Json {
    assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
    let content = array(result.get("content").unwrap());
    assert_eq!(content.len(), 1);
    parse_json(content[0].get("text").and_then(Json::as_str).unwrap()).unwrap()
}
fn outputs(receipt: &Json) -> &[Json] {
    array(receipt.get("outputs").unwrap())
}
fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|part| part == needle)
}

#[test]
fn single_pdfa_request_reaches_the_core_with_geometry_and_explicit_metadata() {
    let mut request = args("pdf");
    for (key, value) in [
        ("pdfA", text("2b")),
        ("pdfAStrict", Json::Bool(true)),
        ("title", text("Archive & proof")),
        ("author", text("Author")),
        ("metadataEpochSeconds", Json::Number(1_700_000_000.0)),
        ("toc", Json::Bool(true)),
        ("pageNumbers", Json::Bool(true)),
        (
            "page",
            parse_json(r#"{"size":{"widthPt":360,"heightPt":504},"margins":36}"#).unwrap(),
        ),
    ] {
        set(&mut request, key, value);
    }
    let mut options = PdfOptions {
        title: Some("Archive & proof".into()),
        author: Some("Author".into()),
        metadata_epoch_seconds: Some(1_700_000_000),
        toc: true,
        page_numbers: true,
        ..Default::default()
    };
    options.theme.page = PageStyle {
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
    };
    let expected = core()
        .render_pdf_with_options(&options, PdfASettings::a2b_strict())
        .unwrap();
    assert!(contains(&expected, b"<pdfaid:part>2</pdfaid:part>"));
    assert!(!contains(&expected, b"fmd-book-pdf-nav/"));
    let result = receipt(&invoke(request.clone()).unwrap());
    assert_eq!(outputs(&result).len(), 1);
    let output = &outputs(&result)[0];
    assert_eq!(
        output.get("data").and_then(Json::as_str),
        Some(base64_encode(&expected).as_str())
    );
    assert_eq!(output.get("pdfA").and_then(Json::as_str), Some("2b"));
    assert_eq!(output.get("pdfAStrict"), Some(&Json::Bool(true)));
    assert_eq!(
        output.get("mimeType").and_then(Json::as_str),
        Some("application/pdf")
    );
    assert_eq!(
        output.get("bytes").and_then(Json::as_u64),
        Some(expected.len() as u64)
    );
    assert_eq!(result, receipt(&invoke(request).unwrap()));
}

#[test]
fn archival_all_changes_only_the_pdf_output_and_its_receipt() {
    let ordinary = receipt(&invoke(args("all")).unwrap());
    let mut request = args("all");
    set(&mut request, "pdfA", text("2b"));
    let archival = receipt(&invoke(request).unwrap());
    assert_eq!(outputs(&archival).len(), 3);
    assert_eq!(outputs(&ordinary)[1..], outputs(&archival)[1..]);
    assert_ne!(
        outputs(&ordinary)[0].get("data"),
        outputs(&archival)[0].get("data")
    );
    assert_eq!(
        outputs(&archival)[0].get("pdfAStrict"),
        Some(&Json::Bool(false))
    );
    for output in &outputs(&archival)[1..] {
        assert!(output.get("pdfA").is_none());
        assert!(output.get("pdfAStrict").is_none());
    }
    assert_eq!(archival.get("sourceBytes"), ordinary.get("sourceBytes"));
    let total: u64 = outputs(&archival)
        .iter()
        .map(|output| output.get("bytes").and_then(Json::as_u64).unwrap())
        .sum();
    assert_eq!(
        archival.get("artifactBytes").and_then(Json::as_u64),
        Some(total)
    );
}

#[test]
fn omitted_and_explicit_off_profiles_preserve_existing_receipt_bytes() {
    for format in ["pdf", "all"] {
        let ordinary = invoke(args(format)).unwrap();
        let mut request = args(format);
        set(&mut request, "pdfA", text("off"));
        set(&mut request, "pdfAStrict", Json::Bool(false));
        assert_eq!(invoke(request).unwrap(), ordinary);
        assert!(outputs(&receipt(&ordinary))[0].get("pdfA").is_none());
    }
}

#[test]
fn invalid_archival_options_fail_before_source_or_asset_processing() {
    for (mode, strict, reason) in [
        (None, Json::Bool(true), "invalid_pdf_a"),
        (Some(text("off")), Json::Bool(true), "invalid_pdf_a"),
        (
            Some(text("unsupported")),
            Json::Bool(false),
            "invalid_pdf_a",
        ),
        (Some(Json::Bool(true)), Json::Bool(false), "invalid_params"),
        (Some(text("2b")), text("true"), "invalid_params"),
    ] {
        let mut request = args("all");
        if let Some(mode) = mode {
            set(&mut request, "pdfA", mode);
        }
        set(&mut request, "pdfAStrict", strict);
        set(&mut request, "chapters", Json::Array(vec![]));
        set(
            &mut request,
            "images",
            parse_json(r#"[{"destination":"x.png","base64":"?"}]"#).unwrap(),
        );
        assert_eq!(invoke(request).unwrap_err().2, reason);
    }
}

#[test]
fn non_pdf_targets_and_source_only_verification_do_not_accept_archival_flags() {
    for target in ["epub", "site"] {
        for (key, value) in [("pdfA", text("off")), ("pdfAStrict", Json::Bool(false))] {
            let mut request = args(target);
            set(&mut request, key, value);
            assert_eq!(invoke(request).unwrap_err().2, "unsupported_target_option");
        }
    }
    let verification = object([
        ("name", text("fmd.verify_book")),
        (
            "arguments",
            object([
                ("chapters", Json::Array(vec![source("one.md", "# One")])),
                ("pdfA", text("2b")),
            ]),
        ),
    ]);
    assert_eq!(
        handle_tool_call(Some(&verification), DEFAULT_MAX_INPUT_BYTES)
            .unwrap_err()
            .2,
        "invalid_params"
    );
}

#[test]
fn strict_archival_failure_returns_no_partial_multi_format_payload() {
    for target in ["pdf", "all"] {
        let mut request = args(target);
        set(&mut request, "pdfA", text("2b"));
        set(&mut request, "pdfAStrict", Json::Bool(true));
        set(
            &mut request,
            "chapters",
            Json::Array(vec![
                source(
                    "one.md",
                    "# One\n\n[Next](two.md#two)\n\n[^unused]: [Forbidden](file:///private.txt)",
                ),
                source("two.md", TWO),
            ]),
        );
        let error = invoke(request).unwrap_err();
        assert_eq!(error.2, "render_failed");
        assert!(error.1.contains("pdf_a_file_uri"));
    }
}

#[test]
fn framed_session_combines_link_and_archival_strictness_then_recovers() {
    let mut input_bytes = Vec::new();
    for (index, markdown) in [
        "# One\n\n[Next](two.md#missing) [Forbidden](file:///private.txt)",
        "# One\n\n[Next](two.md#two) [Forbidden](file:///private.txt)",
        "# One\n\n[Next](two.md#two)",
    ]
    .into_iter()
    .enumerate()
    {
        let mut request = args("all");
        set(
            &mut request,
            "chapters",
            Json::Array(vec![source("one.md", markdown), source("two.md", TWO)]),
        );
        set(&mut request, "pdfA", text("2b"));
        set(&mut request, "pdfAStrict", Json::Bool(true));
        set(&mut request, "strictLinks", Json::Bool(true));
        let message = object([
            ("jsonrpc", text("2.0")),
            ("id", Json::Number((index + 1) as f64)),
            ("method", text("tools/call")),
            ("params", params(request)),
        ])
        .to_json_string();
        write_frame(&mut input_bytes, &message).unwrap();
    }
    let mut output = Vec::new();
    assert_eq!(
        serve_stdio(
            &mut Cursor::new(input_bytes),
            &mut output,
            DEFAULT_MAX_INPUT_BYTES,
            8 * 1024 * 1024
        )
        .unwrap(),
        3
    );
    let mut cursor = Cursor::new(output);
    for (index, reason) in [Some("book_links_failed"), Some("render_failed"), None]
        .into_iter()
        .enumerate()
    {
        let reply =
            parse_json(&read_frame(&mut cursor, 8 * 1024 * 1024).unwrap().unwrap()).unwrap();
        assert_eq!(
            reply.get("id").and_then(Json::as_u64),
            Some((index + 1) as u64)
        );
        if let Some(reason) = reason {
            assert!(reply.get("result").is_none());
            let error = reply.get("error").unwrap();
            assert_eq!(
                error
                    .get("data")
                    .unwrap()
                    .get("reason")
                    .and_then(Json::as_str),
                Some(reason)
            );
            if index == 1 {
                assert!(
                    error
                        .get("message")
                        .and_then(Json::as_str)
                        .unwrap()
                        .contains("pdf_a_file_uri")
                );
            }
        } else {
            assert!(reply.get("error").is_none());
            let publication = receipt(reply.get("result").unwrap());
            assert_eq!(outputs(&publication).len(), 3);
            assert_eq!(
                outputs(&publication)[0].get("pdfA").and_then(Json::as_str),
                Some("2b")
            );
        }
    }
    assert!(read_frame(&mut cursor, 8 * 1024 * 1024).unwrap().is_none());
}

#[test]
fn output_admission_charges_archival_bytes_and_remains_aggregate() {
    let mut request = args("all");
    set(&mut request, "pdfA", text("2b"));
    let original = invoke(request.clone()).unwrap();
    let total = receipt(&original)
        .get("artifactBytes")
        .and_then(Json::as_u64)
        .unwrap();
    set(&mut request, "maxOutputBytes", Json::Number(total as f64));
    assert_eq!(invoke(request.clone()).unwrap(), original);
    set(
        &mut request,
        "maxOutputBytes",
        Json::Number((total - 1) as f64),
    );
    assert_eq!(invoke(request).unwrap_err().2, "output_too_large");
}

#[test]
fn explicit_images_are_shared_with_epub_and_site_after_archival_pdf() {
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0,
        3, 1, 1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let markdown = "# Figure\n\n![Pixel](../assets/pixel.png)\n\n[Next](../end.md#end)";
    let renderer =
        BookRenderer::new(&[input("guide/figure.md", markdown), input("end.md", "# End")]).unwrap();
    let images = vec![PdfImageAsset::new("assets/pixel.png", PNG)];
    let html = HtmlOptions {
        image_assets: images.clone(),
        ..Default::default()
    };
    let pdf = PdfOptions {
        image_assets: images,
        ..Default::default()
    };
    let expected = [
        renderer
            .render_pdf_with_options(&pdf, PdfASettings::a2b_strict())
            .unwrap(),
        franken_markdown::epub::render_book_epub(renderer.book(), &html).unwrap(),
        render_book_site(renderer.book(), &html).unwrap(),
    ];
    assert!(contains(&expected[0], b"/Subtype /Image"));
    let request = object([
        (
            "chapters",
            Json::Array(vec![
                source("guide/figure.md", markdown),
                source("end.md", "# End"),
            ]),
        ),
        ("to", text("all")),
        ("pdfA", text("2b")),
        ("pdfAStrict", Json::Bool(true)),
        (
            "images",
            Json::Array(vec![object([
                ("destination", text("assets/pixel.png")),
                ("base64", text(&base64_encode(PNG))),
            ])]),
        ),
    ]);
    let publication = receipt(&invoke(request).unwrap());
    for (output, bytes) in outputs(&publication).iter().zip(expected) {
        assert_eq!(
            output.get("data").and_then(Json::as_str),
            Some(base64_encode(&bytes).as_str())
        );
    }
}
