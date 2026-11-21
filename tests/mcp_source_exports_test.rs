//! Stateless MCP publications must reach the existing checked native renderers.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::mcp::{
    ERROR_INPUT_TOO_LARGE, ERROR_INVALID_OPTIONS, ERROR_RENDER_FAILED, INVALID_PARAMS,
    JsonValue as Json, base64_encode, handle_tool_call, tools_list_result,
};
use franken_markdown::{
    DarkModePolicy, FontAssetSlot, FontAssets, FontEmbedding, FontFamily, FontScale, HtmlOptions,
    PdfImageAsset, Profile, SvgOptions, Theme, apply_profile, parse_markdown_with_profile,
};

type Args = BTreeMap<String, Json>;
type Failure = (i32, String, &'static str);
const TOOLS: [&str; 2] = ["fmd.render_svg", "fmd.render_epub"];
const SOURCE: &str = "# Publication\n\nA **bold** statement and [local link](#publication).\n";
const PROFILE_SOURCE: &str = "*Term\n: definition*\n\n<!-- pagebreak -->\n\nTail.\n";
const IMAGE_SOURCE: &str = "# Results\n\n## Detail\n\n![plot](plot.svg)\n\nAB **AB**.\n";
const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><path d="M0 0L20 10" stroke="red"/></svg>"#;
const VARIABLE: &[u8] = include_bytes!("../fmd-font/fonts/test-variable/FmdMappedMetricsVF.ttf");

fn text(value: &str) -> Json {
    Json::String(value.to_owned())
}

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(items) => items,
        _ => panic!("expected array"),
    }
}

fn args(source: &str) -> Args {
    BTreeMap::from([("markdown".to_owned(), text(source))])
}

fn call_with_limit(name: &str, args: Args, limit: u64) -> Result<Json, Failure> {
    handle_tool_call(
        Some(&object([
            ("name", text(name)),
            ("arguments", Json::Object(args)),
        ])),
        limit,
    )
}

fn call(name: &str, args: Args) -> Result<Json, Failure> {
    call_with_limit(name, args, 1024 * 1024)
}

fn content(value: &Json) -> &str {
    assert_eq!(value.get("isError"), Some(&Json::Bool(false)));
    let items = array(value.get("content").unwrap());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get("type"), Some(&text("text")));
    items[0].get("text").and_then(Json::as_str).unwrap()
}

fn font_request(bytes: &[u8], weight: u16) -> Json {
    Json::Array(vec![object([
        ("slot", text("body-regular")),
        ("base64", text(&base64_encode(bytes))),
        ("weight", Json::Number(f64::from(weight))),
    ])])
}

fn font_assets(bytes: &[u8], weight: u16) -> FontAssets {
    FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, bytes.to_vec())
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyRegular, weight)
        .unwrap()
}

fn resource_args() -> Args {
    let mut request = args(IMAGE_SOURCE);
    request.insert(
        "images".to_owned(),
        Json::Array(vec![object([
            ("destination", text("plot.svg")),
            ("base64", text(&base64_encode(SVG))),
        ])]),
    );
    request.insert(
        "fonts".to_owned(),
        font_request(franken_markdown::text::bundled::CM_REGULAR, 400),
    );
    request
}

fn styled_theme() -> Theme {
    let mut theme = Theme::default().with_font_scale(FontScale::parse("125%").unwrap());
    theme.font = FontFamily::Serif;
    theme
}

#[test]
fn source_discovery_exposes_only_consumed_fields_and_shared_resource_contracts() {
    const SVG_FIELDS: &[&str] = &[
        "markdown",
        "profile",
        "font",
        "fontScale",
        "maxWidthPt",
        "toc",
        "tocDepth",
        "images",
        "fonts",
    ];
    const EPUB_FIELDS: &[&str] = &[
        "markdown",
        "profile",
        "font",
        "fontScale",
        "darkMode",
        "customCss",
        "title",
        "author",
        "lang",
        "toc",
        "tocDepth",
        "fontEmbedding",
        "images",
        "fonts",
    ];
    let listing = tools_list_result();
    let tools = array(listing.get("tools").unwrap());
    for (name, expected) in [
        ("fmd.render_svg", SVG_FIELDS),
        ("fmd.render_epub", EPUB_FIELDS),
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Json::as_str) == Some(name))
            .unwrap();
        let schema = tool.get("inputSchema").unwrap();
        assert_eq!(schema.get("additionalProperties"), Some(&Json::Bool(false)));
        assert_eq!(array(schema.get("required").unwrap()), [text("markdown")]);
        let properties = schema.get("properties").unwrap().as_object().unwrap();
        assert_eq!(
            properties
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            expected.iter().copied().collect::<BTreeSet<_>>(),
            "{name}"
        );
        assert_eq!(
            array(properties["profile"].get("enum").unwrap()),
            [text("commonmark-gfm"), text("gfm-plus")]
        );
        for resource in ["images", "fonts"] {
            assert_eq!(properties[resource].get("type"), Some(&text("array")));
            assert_eq!(
                properties[resource]
                    .get("items")
                    .unwrap()
                    .get("additionalProperties"),
                Some(&Json::Bool(false))
            );
        }
        if name == "fmd.render_svg" {
            assert_eq!(
                properties["maxWidthPt"]
                    .get("minimum")
                    .and_then(Json::as_f64),
                Some(144.0)
            );
            assert_eq!(
                properties["maxWidthPt"]
                    .get("maximum")
                    .and_then(Json::as_f64),
                Some(14400.0)
            );
        }
    }
}

#[test]
fn default_source_publications_match_native_bytes_and_are_repeatable() {
    let document = franken_markdown::parse_markdown(SOURCE);
    let svg = franken_markdown::svg::render_svg_with_resources(
        &document,
        &SvgOptions::default(),
        &FontAssets::default(),
        &[],
    )
    .unwrap();
    assert!(svg.2.is_empty());
    let epub = franken_markdown::render_epub(&document, &HtmlOptions::default()).unwrap();
    assert!(epub.starts_with(b"PK\x03\x04"));
    for (name, expected) in [
        ("fmd.render_svg", String::from_utf8(svg.0).unwrap()),
        ("fmd.render_epub", base64_encode(&epub)),
    ] {
        let first = call(name, args(SOURCE)).unwrap();
        assert_eq!(content(&first), expected);
        assert!(first.get("warnings").is_none());
        assert_eq!(first, call(name, args(SOURCE)).unwrap());
    }
}

#[test]
fn svg_source_options_and_explicit_resources_reach_native_measurement() {
    let mut request = resource_args();
    for (key, value) in [
        ("font", text("serif")),
        ("fontScale", text("125%")),
        ("maxWidthPt", Json::Number(431.25)),
        ("toc", Json::Bool(true)),
        ("tocDepth", Json::Number(2.0)),
    ] {
        request.insert(key.to_owned(), value);
    }
    let options = SvgOptions {
        theme: styled_theme(),
        max_width_pt: 431.25,
        toc: true,
        toc_depth: Some(2),
    };
    let expected = franken_markdown::svg::render_svg_with_resources(
        &franken_markdown::parse_markdown(IMAGE_SOURCE),
        &options,
        &font_assets(franken_markdown::text::bundled::CM_REGULAR, 400),
        &[PdfImageAsset::new("plot.svg", SVG.to_vec())],
    )
    .unwrap();
    assert!(expected.2.is_empty());
    let actual = call("fmd.render_svg", request).unwrap();
    assert_eq!(content(&actual).as_bytes(), expected.0);
    assert!(actual.get("warnings").is_none());
    assert_ne!(
        content(&actual),
        content(&call("fmd.render_svg", args(IMAGE_SOURCE)).unwrap())
    );
}

#[test]
fn epub_source_options_and_full_fonts_match_the_native_archive() {
    let mut request = resource_args();
    for (key, value) in [
        ("font", text("serif")),
        ("fontScale", text("125%")),
        ("darkMode", text("disabled")),
        ("title", text("  Explicit title  ")),
        ("author", text("Ada & Équipe")),
        ("lang", text("de")),
        ("customCss", text(".fmd { color: #123456; }")),
        ("fontEmbedding", text("full")),
        ("toc", Json::Bool(true)),
        ("tocDepth", Json::Number(2.0)),
    ] {
        request.insert(key.to_owned(), value);
    }
    let mut theme = styled_theme();
    theme.dark_mode = DarkModePolicy::Disabled;
    let options = HtmlOptions {
        theme,
        title: Some("  Explicit title  ".into()),
        author: Some("Ada & Équipe".into()),
        lang: Some("de".into()),
        custom_css: Some(".fmd { color: #123456; }".into()),
        font_embedding: FontEmbedding::Full,
        toc: true,
        toc_depth: Some(2),
        font_assets: font_assets(franken_markdown::text::bundled::CM_REGULAR, 400),
        image_assets: vec![PdfImageAsset::new("plot.svg", SVG.to_vec())],
        ..HtmlOptions::default()
    };
    let expected =
        franken_markdown::render_epub(&franken_markdown::parse_markdown(IMAGE_SOURCE), &options)
            .unwrap();
    let actual = call("fmd.render_epub", request.clone()).unwrap();
    assert_eq!(content(&actual), base64_encode(&expected));
    assert_eq!(actual, call("fmd.render_epub", request.clone()).unwrap());
    request.insert("fontEmbedding".into(), text("subset"));
    assert_ne!(
        content(&actual),
        content(&call("fmd.render_epub", request).unwrap())
    );
}

#[test]
fn source_dialects_select_grammar_before_parsing_and_output_transforms() {
    let mut outputs = [Vec::new(), Vec::new()];
    for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
        let mut request = args(PROFILE_SOURCE);
        if let Some(profile) = profile {
            request.insert("profile".into(), text(profile.as_str()));
        }
        let document = parse_markdown_with_profile(PROFILE_SOURCE, profile);
        let profiled = apply_profile(&document, profile);
        let svg = franken_markdown::svg::render_svg_with_resources(
            &profiled,
            &SvgOptions::default(),
            &FontAssets::default(),
            &[],
        )
        .unwrap()
        .0;
        let epub = franken_markdown::render_epub(
            &document,
            &HtmlOptions {
                profile,
                ..HtmlOptions::default()
            },
        )
        .unwrap();
        for (index, (name, expected)) in [
            ("fmd.render_svg", String::from_utf8(svg).unwrap()),
            ("fmd.render_epub", base64_encode(&epub)),
        ]
        .into_iter()
        .enumerate()
        {
            let actual = call(name, request.clone()).unwrap();
            assert_eq!(content(&actual), expected, "{name}: {profile:?}");
            outputs[index].push(content(&actual).to_owned());
        }
    }
    for output in outputs {
        assert_eq!(
            output[0], output[2],
            "omission retains the extended grammar"
        );
        assert_ne!(
            output[1], output[2],
            "CommonMark paragraph ownership changes output"
        );
    }
}

#[test]
fn svg_source_keeps_recoverable_diagnostics_beside_the_native_output() {
    let source = "![lost](https://example.invalid/missing.png)\n";
    let expected = franken_markdown::svg::render_svg_with_resources(
        &franken_markdown::parse_markdown(source),
        &SvgOptions::default(),
        &FontAssets::default(),
        &[],
    )
    .unwrap();
    assert_eq!(expected.2.len(), 1);
    assert_eq!(expected.2[0].code, "svg_image_missing");
    let actual = call("fmd.render_svg", args(source)).unwrap();
    assert_eq!(content(&actual).as_bytes(), expected.0);
    let warnings = array(actual.get("warnings").unwrap());
    assert_eq!(warnings.len(), expected.2.len());
    for (actual, expected) in warnings.iter().zip(&expected.2) {
        assert_eq!(
            actual,
            &object([
                ("code", text(expected.code)),
                ("message", text(&expected.message)),
            ])
        );
    }
}

#[test]
fn source_tools_reject_file_authority_and_options_without_a_consumer() {
    let common = [
        ("path", text("/absent/document.md")),
        ("out", text("/absent/publication")),
        ("to", text("svg")),
        ("expandIncludes", Json::Bool(false)),
        ("includeSources", Json::Array(vec![])),
        ("chapters", Json::Array(vec![])),
    ];
    for name in TOOLS {
        let mut rejected = common.to_vec();
        rejected.extend([
            ("allowRawHtml", Json::Bool(false)),
            ("interactiveHtml", Json::Bool(false)),
            ("htmlFontFormat", text("ttf")),
            ("page", object([])),
            ("pdfA", text("off")),
        ]);
        if name == "fmd.render_svg" {
            rejected.extend([
                ("title", text("Title")),
                ("author", text("Writer")),
                ("lang", text("en")),
                ("customCss", text("")),
                ("darkMode", text("auto")),
                ("fontEmbedding", text("subset")),
            ]);
        } else {
            rejected.push(("maxWidthPt", Json::Number(612.0)));
        }
        for (field, value) in rejected {
            let mut request = args("text");
            request.insert(field.to_owned(), value);
            request.insert("images".into(), Json::Array(vec![Json::Null]));
            let failure = call(name, request).unwrap_err();
            assert_eq!(failure.0, INVALID_PARAMS, "{name}: {field}");
            assert_eq!(failure.2, "invalid_params", "{name}: {field}");
            assert!(failure.1.contains(field), "{failure:?}");
        }
    }
}

#[test]
fn malformed_options_fail_before_resource_decoding() {
    for name in TOOLS {
        for (field, value, reason) in [
            ("profile", text("commonmark"), "invalid_profile"),
            ("font", text("missing-family"), "invalid_font"),
            ("fontScale", text("large-ish"), "invalid_font_scale"),
        ] {
            let mut request = args("text");
            request.insert(field.to_owned(), value);
            request.insert("images".into(), Json::Array(vec![Json::Null]));
            let failure = call(name, request).unwrap_err();
            assert_eq!(failure.0, ERROR_INVALID_OPTIONS, "{name}: {field}");
            assert_eq!(failure.2, reason, "{name}: {field}");
        }
        for (field, value) in [
            ("markdown", Json::Null),
            ("profile", Json::Bool(false)),
            ("toc", text("true")),
            ("tocDepth", Json::Number(0.0)),
            ("tocDepth", Json::Number(7.0)),
            ("images", Json::Null),
            ("fonts", object([])),
        ] {
            let mut request = args("text");
            request.insert(field.to_owned(), value);
            assert_eq!(
                call(name, request).unwrap_err().0,
                INVALID_PARAMS,
                "{name}: {field}"
            );
        }
    }
    for width in [0.0, 143.99, 14400.01, f64::INFINITY, f64::NAN] {
        let mut request = args("text");
        request.insert("maxWidthPt".into(), Json::Number(width));
        assert_eq!(
            call("fmd.render_svg", request).unwrap_err().0,
            INVALID_PARAMS
        );
    }
    for (field, value, reason) in [
        ("fontEmbedding", text("unbounded"), "invalid_font_embedding"),
        ("darkMode", text("dark"), "invalid_dark_mode"),
    ] {
        let mut request = args("text");
        request.insert(field.into(), value);
        request.insert("fonts".into(), Json::Array(vec![Json::Null]));
        assert_eq!(call("fmd.render_epub", request).unwrap_err().2, reason);
    }
}

#[test]
fn source_presence_and_byte_limits_are_enforced_before_resources() {
    for name in TOOLS {
        assert_eq!(call(name, Args::new()).unwrap_err().2, "missing_markdown");
        let mut request = args("é");
        request.insert("images".into(), Json::Array(vec![Json::Null]));
        let failure = call_with_limit(name, request, 1).unwrap_err();
        assert_eq!(failure.0, ERROR_INPUT_TOO_LARGE);
        assert_eq!(failure.2, "input_too_large");
        assert!(call_with_limit(name, args("é"), 2).is_ok());
        assert!(call_with_limit(name, args(""), 0).is_ok());
    }
}

#[test]
fn explicit_resources_retain_shared_nested_validation_and_count_limits() {
    for name in TOOLS {
        for invalid in [
            Json::Array(vec![Json::Null]),
            Json::Array(vec![object([
                ("destination", text("plot.svg")),
                ("base64", text("AB==")),
            ])]),
            Json::Array(vec![object([
                ("destination", text("plot.svg")),
                ("base64", text("AA==")),
                ("path", text("/absent/image.svg")),
            ])]),
        ] {
            let mut request = args("text");
            request.insert("images".into(), invalid);
            assert_eq!(call(name, request).unwrap_err().2, "invalid_resources");
        }
        for (field, count) in [("images", 4097), ("fonts", 6)] {
            let mut request = args("text");
            request.insert(field.into(), Json::Array(vec![Json::Null; count]));
            let failure = call(name, request).unwrap_err();
            assert_eq!(failure.0, ERROR_INPUT_TOO_LARGE);
            assert_eq!(failure.2, "resources_too_large");
        }
        let mut request = args("text");
        request.insert("fonts".into(), font_request(&[0], 400));
        assert_eq!(call(name, request).unwrap_err().2, "invalid_resources");
    }
}

fn malformed_variable_font() -> Vec<u8> {
    let mut bytes = VARIABLE.to_vec();
    let count = usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap()));
    let table = (0..count)
        .find_map(|index| {
            let record = 12 + 16 * index;
            (&bytes[record..record + 4] == b"HVAR").then(|| {
                u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize
            })
        })
        .unwrap();
    bytes[table..table + 2].copy_from_slice(&2u16.to_be_bytes());
    let font = franken_markdown::text::Font::parse(bytes.clone()).unwrap();
    assert!(font.instance_bounds(*b"wght").is_some());
    assert!(font.instance(650.0).is_none());
    bytes
}

#[test]
fn requested_variable_font_instances_succeed_or_fail_with_the_checked_renderer() {
    let document = franken_markdown::parse_markdown("AB **AB**");
    for broken in [false, true] {
        let bytes = if broken {
            malformed_variable_font()
        } else {
            VARIABLE.to_vec()
        };
        let fonts = font_assets(&bytes, 650);
        let svg = franken_markdown::svg::render_svg_with_resources(
            &document,
            &SvgOptions::default(),
            &fonts,
            &[],
        )
        .map(|(bytes, _, _)| String::from_utf8(bytes).unwrap());
        let epub = franken_markdown::render_epub(
            &document,
            &HtmlOptions {
                font_assets: fonts,
                ..HtmlOptions::default()
            },
        )
        .map(|bytes| base64_encode(&bytes));
        for (name, expected) in [("fmd.render_svg", svg), ("fmd.render_epub", epub)] {
            let mut request = args("AB **AB**");
            request.insert("fonts".into(), font_request(&bytes, 650));
            let actual = call(name, request);
            if broken {
                let expected = expected.unwrap_err();
                let actual = actual.unwrap_err();
                assert_eq!(actual.0, ERROR_RENDER_FAILED);
                assert_eq!(actual.2, "render_failed");
                assert_eq!(actual.1, format!("Render failed: {expected}"));
                assert!(actual.1.contains("body-regular"));
            } else {
                assert_eq!(content(&actual.unwrap()), expected.unwrap());
            }
        }
    }
}
