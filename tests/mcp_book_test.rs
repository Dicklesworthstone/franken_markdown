//! Real book renderers through the public MCP dispatcher; no renderer doubles.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer, render_book_pdf, render_book_site};
use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, JsonValue as Json, base64_encode, handle_tool_call, parse_json,
    tools_list_result,
};
use franken_markdown::{HtmlOptions, PdfOptions};

type Failure = (i32, String, &'static str);
fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(fields.into_iter().map(|(key, value)| (key.into(), value)).collect())
}
fn text(value: &str) -> Json { Json::String(value.into()) }
fn array(value: &Json) -> &[Json] {
    match value { Json::Array(items) => items, _ => panic!("expected an array") }
}
fn source(path: &str, markdown: &str) -> Json {
    object([("path", text(path)), ("markdown", text(markdown))])
}
fn input(path: &str, markdown: &str) -> BookInput {
    BookInput { path: path.into(), source: markdown.into() }
}
fn args(to: &str) -> Json {
    object([
        ("chapters", Json::Array(vec![
            source("guide/intro.md", "# Introduction\n\n[Continue](../next.md#next)\n\n{{#include ../shared/body.md}}\n"),
            source("next.md", "# Next\n\n[Return](guide/intro.md#introduction)\n"),
        ])),
        ("includeSources", Json::Array(vec![source("shared/body.md", "Included **body**.\n")])),
        ("to", text(to)),
    ])
}
fn core() -> BookRenderer {
    BookRenderer::from_sources(&[
        input("guide/intro.md", "# Introduction\n\n[Continue](../next.md#next)\n\n{{#include ../shared/body.md}}\n"),
        input("next.md", "# Next\n\n[Return](guide/intro.md#introduction)\n"),
    ], &[input("shared/body.md", "Included **body**.\n")]).unwrap()
}
fn set(args: &mut Json, key: &str, value: Json) {
    match args { Json::Object(fields) => { fields.insert(key.into(), value); }, _ => panic!("object") }
}
fn invoke(args: Json, limit: u64) -> Result<Json, Failure> {
    handle_tool_call(Some(&object([("name", text("fmd.render_book")), ("arguments", args)])), limit)
}
fn receipt(args: Json) -> Json {
    let result = invoke(args, DEFAULT_MAX_INPUT_BYTES).unwrap();
    assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
    parse_json(array(result.get("content").unwrap())[0].get("text").unwrap().as_str().unwrap()).unwrap()
}
fn outputs(receipt: &Json) -> &[Json] { array(receipt.get("outputs").unwrap()) }
fn payload(output: &Json) -> &str { output.get("data").unwrap().as_str().unwrap() }

#[test]
fn all_formats_match_the_existing_book_renderers_and_preserve_order() {
    let renderer = core();
    let expected = [
        ("pdf", "application/pdf", render_book_pdf(renderer.book(), &PdfOptions::default()).unwrap()),
        ("epub", "application/epub+zip", franken_markdown::epub::render_book_epub(renderer.book(), &HtmlOptions::default()).unwrap()),
        ("site", "application/zip", render_book_site(renderer.book(), &HtmlOptions::default()).unwrap()),
    ];
    assert!(expected[0].2.starts_with(b"%PDF-"));
    assert!(expected[1].2.starts_with(b"PK\x03\x04"));
    assert!(expected[2].2.starts_with(b"PK\x03\x04"));
    let result = receipt(args("all"));
    assert_eq!(result.get("schema").and_then(Json::as_str), Some("fmd-mcp-book-publication-v1"));
    assert_eq!(result.get("chapters").and_then(Json::as_u64), Some(2));
    assert_eq!(result.get("sourceBytes").and_then(Json::as_u64), Some(renderer.source_length() as u64));
    assert_eq!(outputs(&result).len(), 3);
    let total: usize = expected.iter().map(|(_, _, bytes)| bytes.len()).sum();
    assert_eq!(result.get("artifactBytes").and_then(Json::as_u64), Some(total as u64));
    for (row, (format, mime, bytes)) in outputs(&result).iter().zip(expected) {
        assert_eq!(row.get("format").and_then(Json::as_str), Some(format));
        assert_eq!(row.get("mimeType").and_then(Json::as_str), Some(mime));
        assert_eq!(row.get("encoding").and_then(Json::as_str), Some("base64"));
        assert_eq!(row.get("bytes").and_then(Json::as_u64), Some(bytes.len() as u64));
        assert_eq!(payload(row), base64_encode(&bytes));
    }
}

#[test]
fn every_single_format_and_the_default_pdf_route_are_deterministic() {
    for format in ["pdf", "epub", "site"] {
        let one = receipt(args(format));
        assert_eq!(one, receipt(args(format)));
        assert_eq!(outputs(&one).len(), 1);
        assert_eq!(outputs(&one)[0].get("format").and_then(Json::as_str), Some(format));
    }
    let mut request = args("pdf");
    if let Json::Object(fields) = &mut request { fields.remove("to"); }
    assert_eq!(receipt(request), receipt(args("pdf")));
}

#[test]
fn pdf_geometry_typography_metadata_and_running_text_reach_book_rendering() {
    let mut request = args("pdf");
    set(&mut request, "title", text("Manual"));
    set(&mut request, "author", text("Author"));
    set(&mut request, "baseFontSize", Json::Number(13.0));
    set(&mut request, "pageNumbers", Json::Bool(true));
    set(&mut request, "metadataEpochSeconds", Json::Number(0.0));
    set(&mut request, "page", parse_json(r#"{"size":{"widthPt":400,"heightPt":600},"margins":40}"#).unwrap());
    let mut options = PdfOptions {
        title: Some("Manual".into()), author: Some("Author".into()), base_font_size: Some(13.0),
        page_numbers: true, metadata_epoch_seconds: Some(0), ..Default::default()
    };
    options.theme.page = franken_markdown::PageStyle {
        size: franken_markdown::PageSize { name: "custom", width_pt: 400.0, height_pt: 600.0 },
        margins: franken_markdown::PageMargins { top_pt: 40.0, right_pt: 40.0, bottom_pt: 40.0, left_pt: 40.0 },
    };
    let expected = render_book_pdf(core().book(), &options).unwrap();
    assert_ne!(expected, render_book_pdf(core().book(), &PdfOptions::default()).unwrap());
    assert_eq!(payload(&outputs(&receipt(request))[0]), base64_encode(&expected));
}

#[test]
fn explicit_empty_css_is_preserved_for_epub_and_site() {
    for format in ["epub", "site"] {
        let mut request = args(format);
        set(&mut request, "customCss", text(""));
        let options = HtmlOptions { custom_css: Some(String::new()), ..Default::default() };
        let renderer = core();
        let expected = if format == "site" { render_book_site(renderer.book(), &options) }
            else { franken_markdown::epub::render_book_epub(renderer.book(), &options) }.unwrap();
        assert_eq!(payload(&outputs(&receipt(request))[0]), base64_encode(&expected));
    }
}

#[test]
fn source_objects_reading_order_and_include_only_roles_are_not_guessed() {
    let mut request = object([("chapters", Json::Array(vec![source("z.md", "# Last lexical"), source("a.md", "# First lexical")])), ("to", text("epub"))]);
    let expected = BookRenderer::new(&[input("z.md", "# Last lexical"), input("a.md", "# First lexical")]).unwrap().render_epub().unwrap();
    assert_eq!(payload(&outputs(&receipt(request.clone()))[0]), base64_encode(&expected));
    set(&mut request, "includeSources", Json::Array(vec![source("extra.md", "# Not a chapter")]));
    assert_eq!(receipt(request).get("chapters").and_then(Json::as_u64), Some(2));
}

#[test]
fn unknown_fields_wrong_types_and_unconsumed_options_are_rejected() {
    for (to, name, value, reason) in [
        ("pdf", "customCss", text(""), "unsupported_target_option"),
        ("epub", "pageNumbers", Json::Bool(false), "unsupported_target_option"),
        ("epub", "allowRawHtml", Json::Bool(false), "unsupported_target_option"),
        ("epub", "htmlFontFormat", text("woff2"), "unsupported_target_option"),
        ("site", "metadataEpochSeconds", Json::Number(0.0), "unsupported_target_option"),
        ("site", "pdfA", text("2b"), "unsupported_target_option"),
        ("all", "interactiveHtml", Json::Bool(true), "invalid_params"),
        ("all", "out", text("/not-written/book.pdf"), "invalid_params"),
        ("all", "baseFontSize", text("12"), "invalid_params"),
    ] {
        let mut request = args(to); set(&mut request, name, value);
        assert_eq!(invoke(request, DEFAULT_MAX_INPUT_BYTES).unwrap_err().2, reason, "{to}/{name}");
    }
    assert_eq!(invoke(args("both"), DEFAULT_MAX_INPUT_BYTES).unwrap_err().2, "unsupported_target");
}

#[test]
fn bad_source_sets_never_fall_back_to_filesystem_reads_or_partial_books() {
    for chapters in [
        Json::Array(vec![]),
        Json::Array(vec![object([("path", text("a.md"))])]),
        Json::Array(vec![object([("path", text("a.md")), ("markdown", text("# A")), ("url", text("https://example.invalid/a.md"))])]),
        Json::Array(vec![source("../outside.md", "# No")]),
        Json::Array(vec![source("a.md", "# One"), source("./a.md", "# Two")]),
        Json::Array(vec![source("a/b.md", "# One"), source("a__b.md", "# Two")]),
        Json::Array(vec![source("a.md", "{{#include missing.md}}")]),
        Json::Array(vec![source("a.md", "{{#include b.md}}"), source("b.md", "{{#include a.md}}")]),
    ] {
        let request = object([("chapters", chapters)]);
        assert_eq!(invoke(request, DEFAULT_MAX_INPUT_BYTES).unwrap_err().2, "invalid_book_sources");
    }
}

#[test]
fn input_limit_counts_utf8_paths_and_include_sources_before_parsing() {
    let request = object([
        ("chapters", Json::Array(vec![source("é.md", "# A")])),
        ("includeSources", Json::Array(vec![source("extra.md", "xx")])),
    ]);
    let limit = ("é.md".len() + "# A".len() + "extra.md".len() + "xx".len()) as u64;
    assert_eq!(invoke(request.clone(), limit - 1).unwrap_err().2, "input_too_large");
    assert!(invoke(request, limit).is_ok());
}

#[test]
fn output_limit_covers_the_whole_multi_format_result() {
    let mut request = args("all");
    set(&mut request, "maxOutputBytes", Json::Number(1.0));
    assert_eq!(invoke(request, DEFAULT_MAX_INPUT_BYTES).unwrap_err().2, "output_too_large");
    for value in [0.0, -1.0, 1.5, 33_554_433.0] {
        let mut request = args("pdf"); set(&mut request, "maxOutputBytes", Json::Number(value));
        assert_eq!(invoke(request, DEFAULT_MAX_INPUT_BYTES).unwrap_err().2, "invalid_params");
    }
}

#[test]
fn resource_admission_and_target_option_union_are_preserved() {
    let mut request = args("all");
    set(&mut request, "images", parse_json(r#"[{"destination":"image.png","base64":"?"}]"#).unwrap());
    assert_eq!(invoke(request, DEFAULT_MAX_INPUT_BYTES).unwrap_err().2, "invalid_resources");
    let mut request = args("all");
    set(&mut request, "customCss", text(".fmd { color: navy; }"));
    set(&mut request, "pageNumbers", Json::Bool(true));
    assert_eq!(outputs(&receipt(request)).len(), 3);
}

#[test]
fn discovery_has_precise_source_schema_and_explicit_pdfa_options() {
    let result = tools_list_result();
    let tools = array(result.get("tools").unwrap());
    let tool = tools.iter().find(|tool| tool.get("name").and_then(Json::as_str) == Some("fmd.render_book")).unwrap();
    let schema = tool.get("inputSchema").unwrap();
    assert_eq!(schema.get("additionalProperties"), Some(&Json::Bool(false)));
    assert_eq!(array(schema.get("required").unwrap()), [text("chapters")]);
    let properties = schema.get("properties").unwrap();
    let chapters = properties.get("chapters").unwrap();
    assert_eq!(chapters.get("minItems").and_then(Json::as_u64), Some(1));
    assert_eq!(chapters.get("maxItems").and_then(Json::as_u64), Some(4096));
    assert_eq!(chapters.get("items").unwrap().get("additionalProperties"), Some(&Json::Bool(false)));
    assert_eq!(array(chapters.get("items").unwrap().get("required").unwrap()), [text("path"), text("markdown")]);
    assert_eq!(properties.get("maxOutputBytes").unwrap().get("maximum").and_then(Json::as_u64), Some(33_554_432));
    assert_eq!(properties.get("pdfA").unwrap().get("type").and_then(Json::as_str), Some("string"));
    assert_eq!(properties.get("pdfAStrict").unwrap().get("type").and_then(Json::as_str), Some("boolean"));
    assert!(properties.get("out").is_none());
}

#[test]
fn chapter_relative_image_bytes_survive_all_three_outputs() {
    const PNG: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 8, 88, 240, 1, 0, 3, 36, 1, 225, 240, 249, 204, 143, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
    let markdown = "# Figure\n\n![Pixel](../assets/pixel.png)\n";
    let renderer = BookRenderer::new(&[input("guide/image.md", markdown)]).unwrap();
    let images = vec![franken_markdown::PdfImageAsset::new("assets/pixel.png", PNG.to_vec())];
    let html = HtmlOptions { image_assets: images.clone(), ..Default::default() };
    let pdf = PdfOptions { image_assets: images, ..Default::default() };
    let expected = [
        render_book_pdf(renderer.book(), &pdf).unwrap(),
        franken_markdown::epub::render_book_epub(renderer.book(), &html).unwrap(),
        render_book_site(renderer.book(), &html).unwrap(),
    ];
    let request = object([
        ("chapters", Json::Array(vec![source("guide/image.md", markdown)])),
        ("to", text("all")),
        ("images", Json::Array(vec![object([
            ("destination", text("assets/pixel.png")),
            ("base64", Json::String(base64_encode(PNG))),
        ])])),
    ]);
    let result = receipt(request);
    for (row, bytes) in outputs(&result).iter().zip(expected) {
        assert_eq!(payload(row), base64_encode(&bytes));
    }
}

#[test]
fn all_output_budget_is_aggregate_not_a_per_file_ceiling() {
    let original = receipt(args("all"));
    let sum = original.get("artifactBytes").and_then(Json::as_u64).unwrap();
    let largest = outputs(&original).iter()
        .map(|output| output.get("bytes").and_then(Json::as_u64).unwrap()).max().unwrap();
    assert!(largest < sum);
    let mut request = args("all");
    set(&mut request, "maxOutputBytes", Json::Number(largest as f64));
    assert_eq!(invoke(request, DEFAULT_MAX_INPUT_BYTES).unwrap_err().2, "output_too_large");
    let mut exact = args("all");
    set(&mut exact, "maxOutputBytes", Json::Number(sum as f64));
    assert_eq!(receipt(exact), original);
}
