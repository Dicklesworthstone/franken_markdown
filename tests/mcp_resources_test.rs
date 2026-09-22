//! Exercise the public MCP dispatcher with the real renderer, not mock outputs.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use franken_markdown::{FontAssetSlot, FontAssets, HtmlOptions, PdfImageAsset, PdfOptions};
use franken_markdown::mcp::{JsonValue, base64_encode, handle_tool_call, parse_json, tools_list_result};

const SOURCE: &str = "# Results\n\n![plot](plot.svg)\n\nMeasured text.";
const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><path d="M0 0L20 10" stroke="red"/></svg>"#;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn args() -> BTreeMap<String, JsonValue> {
    let encoded = base64_encode(SVG);
    let font = base64_encode(franken_markdown::text::bundled::CM_REGULAR);
    parse_json(&format!(r#"{{"images":[{{"destination":"plot.svg","base64":"{encoded}"}}],"fonts":[{{"slot":"body-regular","base64":"{font}","weight":400}}]}}"#))
        .unwrap().as_object().unwrap().clone()
}

fn call(name: &str, args: BTreeMap<String, JsonValue>) -> Result<JsonValue, (i32, String, &'static str)> {
    handle_tool_call(Some(&JsonValue::Object(BTreeMap::from([
        ("name".to_owned(), JsonValue::String(name.to_owned())),
        ("arguments".to_owned(), JsonValue::Object(args)),
    ]))), 1024 * 1024)
}

fn content(value: &JsonValue) -> &str {
    let JsonValue::Array(items) = value.get("content").unwrap() else { panic!("content is not an array") };
    items[0].get("text").unwrap().as_str().unwrap()
}

fn fonts() -> FontAssets {
    FontAssets::default().with_slot(FontAssetSlot::BodyRegular,
        franken_markdown::text::bundled::CM_REGULAR.to_vec()).unwrap()
        .with_slot_weight(FontAssetSlot::BodyRegular, 400).unwrap()
}

fn scratch() -> PathBuf {
    for _ in 0..1000 {
        let path = std::env::temp_dir().join(format!("fmd-mcp-resources-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create test scratch directory: {error}"),
        }
    }
    panic!("test scratch names exhausted")
}

#[test]
fn memory_html_and_pdf_match_direct_resource_aware_rendering() {
    let doc = franken_markdown::parse_markdown(SOURCE);
    let mut args = args();
    args.insert("markdown".to_owned(), JsonValue::String(SOURCE.to_owned()));
    let html = call("fmd.render_html", args.clone()).unwrap();
    let opts = HtmlOptions { font_assets: fonts(), image_assets: vec![PdfImageAsset::new("plot.svg", SVG.to_vec())], ..Default::default() };
    assert_eq!(content(&html), franken_markdown::render_html_document(&doc, &opts).unwrap());
    assert!(content(&html).contains("data:image/svg+xml;base64,"));
    let pdf = call("fmd.render_pdf", args).unwrap();
    let opts = PdfOptions { font_assets: fonts(), image_assets: vec![PdfImageAsset::new("plot.svg", SVG.to_vec())], ..Default::default() };
    assert_eq!(content(&pdf), base64_encode(&franken_markdown::render_pdf_document(&doc, &opts).unwrap()));
}

#[test]
fn every_file_export_uses_the_supplied_image_and_font_bytes() {
    let root = scratch();
    let input = root.join("results.md");
    std::fs::write(&input, SOURCE).unwrap();
    let doc = franken_markdown::parse_markdown(SOURCE);
    let html_opts = HtmlOptions { title: Some("results".into()), font_assets: fonts(), image_assets: vec![PdfImageAsset::new("plot.svg", SVG.to_vec())], ..Default::default() };
    let pdf_opts = PdfOptions { title: Some("results".into()), font_assets: fonts(), image_assets: html_opts.image_assets.clone(), ..Default::default() };
    let expected = [
        ("html", franken_markdown::render_html_document(&doc, &html_opts).unwrap().into_bytes()),
        ("pdf", franken_markdown::render_pdf_document(&doc, &pdf_opts).unwrap()),
        ("epub", franken_markdown::render_epub(&doc, &html_opts).unwrap()),
        ("svg", franken_markdown::svg::render_svg_with_resources(&doc, &Default::default(), &fonts(), &html_opts.image_assets).unwrap().0),
    ];
    for (format, bytes) in expected {
        let output = root.join(format!("output.{format}"));
        let mut args = args();
        args.insert("path".to_owned(), JsonValue::String(input.to_string_lossy().into_owned()));
        args.insert("to".to_owned(), JsonValue::String(format.to_owned()));
        args.insert("out".to_owned(), JsonValue::String(output.to_string_lossy().into_owned()));
        let result = call("fmd.render_file", args).unwrap();
        assert_eq!(result.get("isError"), Some(&JsonValue::Bool(false)));
        assert_eq!(std::fs::read(&output).unwrap(), bytes, "{format}");
        assert_eq!(std::fs::read_to_string(&input).unwrap(), SOURCE);
    }
}

#[test]
fn paired_output_applies_resources_to_both_artifacts() {
    let root = scratch();
    let input = root.join("results.md");
    std::fs::write(&input, SOURCE).unwrap();
    let mut args = args();
    args.insert("path".to_owned(), JsonValue::String(input.to_string_lossy().into_owned()));
    args.insert("to".to_owned(), JsonValue::String("both".to_owned()));
    let result = call("fmd.render_file", args).unwrap();
    let outputs = parse_json(content(&result)).unwrap();
    let JsonValue::Array(outputs) = outputs.get("outputs").unwrap() else { panic!("outputs array") };
    assert_eq!(outputs.len(), 2);
    assert!(outputs[0].get("data").unwrap().as_str().unwrap().contains("data:image/svg+xml;base64,"));
    let doc = franken_markdown::parse_markdown(SOURCE);
    let options = PdfOptions { title: Some("results".into()), font_assets: fonts(), image_assets: vec![PdfImageAsset::new("plot.svg", SVG.to_vec())], ..Default::default() };
    assert_eq!(outputs[1].get("data").unwrap().as_str().unwrap(), base64_encode(&franken_markdown::render_pdf_document(&doc, &options).unwrap()));
}

#[test]
fn invalid_resources_and_aliases_leave_source_and_outputs_untouched() {
    let root = scratch();
    let input = root.join("input.md");
    let output = root.join("output.svg");
    std::fs::write(&input, SOURCE).unwrap();
    std::fs::write(&output, "previous artifact").unwrap();
    let mut args = args();
    args.insert("path".to_owned(), JsonValue::String(input.to_string_lossy().into_owned()));
    args.insert("to".to_owned(), JsonValue::String("svg".into()));
    args.insert("out".to_owned(), JsonValue::String(input.to_string_lossy().into_owned()));
    assert_eq!(call("fmd.render_file", args.clone()).unwrap_err().2, "output_overwrites_input");
    args.insert("out".to_owned(), JsonValue::String(output.to_string_lossy().into_owned()));
    args.insert("images".to_owned(), parse_json(r#"[{"destination":"plot.svg","base64":"AB=="}]"#).unwrap());
    assert_eq!(call("fmd.render_file", args).unwrap_err().2, "invalid_resources");
    assert_eq!(std::fs::read_to_string(&input).unwrap(), SOURCE);
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "previous artifact");
}

#[test]
fn svg_recoverable_diagnostics_are_returned_with_saved_and_inline_output() {
    let root = scratch();
    let input = root.join("input.md");
    std::fs::write(&input, "![lost](missing.png)").unwrap();
    let mut args = BTreeMap::from([
        ("path".to_owned(), JsonValue::String(input.to_string_lossy().into_owned())),
        ("to".to_owned(), JsonValue::String("svg".into())),
    ]);
    for save in [false, true] {
        if save { args.insert("out".to_owned(), JsonValue::String(root.join("result.svg").to_string_lossy().into_owned())); }
        let result = call("fmd.render_file", args.clone()).unwrap();
        let JsonValue::Array(warnings) = result.get("warnings").unwrap() else { panic!("warnings array") };
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].get("code").unwrap().as_str(), Some("svg_image_missing"));
        assert_eq!(result.get("isError"), Some(&JsonValue::Bool(false)));
    }
}

#[test]
fn discovery_exposes_nested_resource_fields_on_render_and_verify_tools() {
    let list = tools_list_result();
    let JsonValue::Array(tools) = list.get("tools").unwrap() else { panic!("tools array") };
    for tool in tools {
        let name = tool.get("name").unwrap().as_str().unwrap();
        let properties = tool.get("inputSchema").unwrap().get("properties").unwrap();
        if name.starts_with("fmd.render_") || name == "fmd.verify" {
            for field in ["images", "fonts"] {
                let schema = properties.get(field).unwrap();
                assert_eq!(schema.get("type").unwrap().as_str(), Some("array"));
                assert_eq!(schema.get("items").unwrap().get("additionalProperties"), Some(&JsonValue::Bool(false)));
            }
        } else {
            assert!(properties.get("images").is_none());
            assert!(properties.get("fonts").is_none());
        }
    }
}
