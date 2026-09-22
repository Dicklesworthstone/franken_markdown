//! Configured SVG typography reaches the core before measurement, for inline
//! and saved file responses. No generated-binding stubs are involved here.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use franken_markdown::{FontFamily, FontScale, SvgOptions, Theme, parse_markdown, render_svg};
use franken_markdown::mcp::{JsonValue, handle_tool_call, tools_list_result};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn scratch() -> PathBuf {
    for _ in 0..1000 {
        let root = std::env::temp_dir().join(format!("fmd-svg-geometry-{}-{}",
            std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        match std::fs::create_dir(&root) {
            Ok(()) => return root,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create scratch directory: {error}"),
        }
    }
    panic!("scratch directory names exhausted")
}
fn call(args: BTreeMap<String, JsonValue>) -> Result<JsonValue, (i32, String, &'static str)> {
    handle_tool_call(Some(&JsonValue::Object(BTreeMap::from([
        ("name".into(), JsonValue::String("fmd.render_file".into())),
        ("arguments".into(), JsonValue::Object(args)),
    ]))), 1024 * 1024)
}

#[test]
fn svg_file_scale_matches_core_for_returned_and_saved_artifacts() {
    let root = scratch();
    let input = root.join("report.md");
    let output = root.join("poster.svg");
    let source = "# Evidence\n\nA report with $x^2$.[^a]\n\n1. First\n2. Second\n\n| Name | Value |\n|---|---|\n| A | 42 |\n\n[^a]: Complete note.\n";
    std::fs::write(&input, source).unwrap();
    let mut arguments = BTreeMap::from([
        ("path".into(), JsonValue::String(input.to_string_lossy().into_owned())),
        ("to".into(), JsonValue::String("svg".into())),
        ("font".into(), JsonValue::String("serif".into())),
        ("fontScale".into(), JsonValue::String("150%".into())),
        ("maxWidthPt".into(), JsonValue::Number(400.0)),
    ]);
    let theme = Theme::default().with_font(FontFamily::Serif)
        .with_font_scale(FontScale::parse("150%").unwrap());
    let doc = parse_markdown(source);
    let expected = render_svg(&doc, &SvgOptions { theme, max_width_pt: 400.0 });
    let result = call(arguments.clone()).unwrap();
    let JsonValue::Array(items) = result.get("content").unwrap() else { panic!("content array") };
    assert_eq!(items[0].get("text").unwrap().as_str().unwrap().as_bytes(), expected);
    assert_ne!(expected, render_svg(&doc, &SvgOptions { theme: Theme::serif(), max_width_pt: 400.0 }));
    assert!(String::from_utf8(expected.clone()).unwrap().contains("scale(0.36)"));
    arguments.insert("out".into(), JsonValue::String(output.to_string_lossy().into_owned()));
    call(arguments).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), expected);
    assert_eq!(std::fs::read_to_string(input).unwrap(), source);
}

#[test]
fn malformed_svg_scale_is_rejected_before_missing_source_access() {
    let missing = scratch().join("not-created.md");
    let result = call(BTreeMap::from([
        ("path".into(), JsonValue::String(missing.to_string_lossy().into_owned())),
        ("to".into(), JsonValue::String("svg".into())),
        ("fontScale".into(), JsonValue::String("not-a-scale".into())),
    ]));
    assert_eq!(result.unwrap_err().2, "invalid_font_scale");
    assert!(!missing.exists());
}

#[test]
fn discovery_identifies_svg_as_a_real_font_scale_consumer() {
    let result = tools_list_result();
    let JsonValue::Array(tools) = result.get("tools").unwrap() else { panic!("tools array") };
    let file = tools.iter().find(|tool|
        tool.get("name").and_then(JsonValue::as_str) == Some("fmd.render_file")).unwrap();
    let scale = file.get("inputSchema").unwrap().get("properties").unwrap().get("fontScale").unwrap();
    assert!(scale.get("description").and_then(JsonValue::as_str).unwrap().contains("svg"));
}
