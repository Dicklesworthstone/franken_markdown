//! End-to-end MCP tool behavior against the actual renderer and filesystem.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, INVALID_PARAMS, JsonValue, base64_encode, handle_tool_call, parse_json,
};
use franken_markdown::{DarkModePolicy, FontFamily, HtmlFontFormat, HtmlOptions, PdfOptions};

type ToolError = (i32, String, &'static str);

fn call(name: &str, args: JsonValue, limit: u64) -> Result<JsonValue, ToolError> {
    handle_tool_call(Some(&JsonValue::Object(BTreeMap::from([
        ("name".to_string(), JsonValue::String(name.to_string())),
        ("arguments".to_string(), args),
    ]))), limit)
}

fn text(result: &JsonValue) -> &str {
    assert_eq!(result.get("isError"), Some(&JsonValue::Bool(false)));
    let JsonValue::Array(content) = result.get("content").unwrap() else { panic!("content array") };
    content[0].get("text").and_then(JsonValue::as_str).unwrap()
}

fn file_args(path: &Path, target: &str, out: Option<&Path>) -> JsonValue {
    let mut args = BTreeMap::from([
        ("path".to_string(), JsonValue::String(path.to_str().unwrap().to_string())),
        ("to".to_string(), JsonValue::String(target.to_string())),
    ]);
    if let Some(out) = out {
        args.insert("out".to_string(), JsonValue::String(out.to_str().unwrap().to_string()));
    }
    JsonValue::Object(args)
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-mcp-tools-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf { self.0.join(name) }
    fn source(&self) -> PathBuf {
        let source = self.path("input.md");
        fs::write(&source, "# Hello tools\n\nSome **text**.\n").unwrap();
        source
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Only remove this test's immediate entries, never recursively follow
        // a caller path or a symlink. A nonempty unexpected directory is kept.
        if let Ok(entries) = fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    let _ = fs::remove_dir(entry.path());
                } else {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        let _ = fs::remove_dir(&self.0);
    }
}

#[test]
fn every_advertised_file_target_returns_an_artifact() {
    let scratch = Scratch::new();
    let source = scratch.source();
    for target in ["html", "pdf", "epub", "svg", "both"] {
        let result = call("fmd.render_file", file_args(&source, target, None), DEFAULT_MAX_INPUT_BYTES).unwrap();
        let output = text(&result);
        match target {
            "html" => assert!(output.contains("<!DOCTYPE html>")),
            "pdf" => assert!(output.starts_with("JVBER")),
            "epub" => assert!(output.starts_with("UEsDB")),
            "svg" => assert!(output.contains("<svg")),
            "both" => {
                let envelope = parse_json(output).unwrap();
                let JsonValue::Array(outputs) = envelope.get("outputs").unwrap() else { panic!("outputs") };
                assert_eq!(outputs.len(), 2);
                assert_eq!(outputs[0].get("format").and_then(JsonValue::as_str), Some("html"));
                assert_eq!(outputs[0].get("encoding").and_then(JsonValue::as_str), Some("utf-8"));
                assert!(outputs[0].get("data").and_then(JsonValue::as_str).unwrap().contains("Hello tools"));
                assert_eq!(outputs[1].get("mimeType").and_then(JsonValue::as_str), Some("application/pdf"));
                assert_eq!(outputs[1].get("encoding").and_then(JsonValue::as_str), Some("base64"));
                assert!(outputs[1].get("data").and_then(JsonValue::as_str).unwrap().starts_with("JVBER"));
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn paired_and_binary_file_outputs_write_real_bytes() {
    let scratch = Scratch::new();
    let source = scratch.source();
    let out = scratch.path("output.any");
    call("fmd.render_file", file_args(&source, "both", Some(&out)), DEFAULT_MAX_INPUT_BYTES).unwrap();
    assert!(fs::read_to_string(scratch.path("output.html")).unwrap().contains("Hello tools"));
    assert!(fs::read(scratch.path("output.pdf")).unwrap().starts_with(b"%PDF-"));
    for target in ["epub", "svg"] {
        let out = scratch.path(&format!("output.{target}"));
        call("fmd.render_file", file_args(&source, target, Some(&out)), DEFAULT_MAX_INPUT_BYTES).unwrap();
        let bytes = fs::read(out).unwrap();
        if target == "epub" { assert!(bytes.starts_with(b"PK\x03\x04")); }
        else { assert!(String::from_utf8(bytes).unwrap().contains("<svg")); }
    }
}

#[test]
fn output_cannot_replace_the_source_or_a_derived_pair_destination() {
    let scratch = Scratch::new();
    let source = scratch.source();
    let before = fs::read(&source).unwrap();
    let error = call("fmd.render_file", file_args(&source, "html", Some(&source)), DEFAULT_MAX_INPUT_BYTES).unwrap_err();
    assert_eq!(error.2, "output_overwrites_input");
    assert_eq!(fs::read(&source).unwrap(), before);
    let html_source = scratch.path("source.html");
    fs::write(&html_source, &before).unwrap();
    let error = call("fmd.render_file", file_args(&html_source, "both", Some(&scratch.path("source.any"))), DEFAULT_MAX_INPUT_BYTES).unwrap_err();
    assert_eq!(error.2, "output_overwrites_input");
    assert_eq!(fs::read(&html_source).unwrap(), before);
    assert!(!scratch.path("source.pdf").exists());
}

#[cfg(unix)]
#[test]
fn symlink_and_hard_link_source_aliases_are_rejected() {
    let scratch = Scratch::new();
    let source = scratch.source();
    let before = fs::read(&source).unwrap();
    let symbolic = scratch.path("symbolic.html");
    let hard = scratch.path("hard.html");
    std::os::unix::fs::symlink(&source, &symbolic).unwrap();
    fs::hard_link(&source, &hard).unwrap();
    for out in [symbolic, hard] {
        let error = call("fmd.render_file", file_args(&source, "html", Some(&out)), DEFAULT_MAX_INPUT_BYTES).unwrap_err();
        assert_eq!(error.2, "output_overwrites_input");
        assert_eq!(fs::read(out).unwrap(), before);
        assert_eq!(fs::read(&source).unwrap(), before);
    }
}

#[test]
fn invalid_second_destination_preserves_existing_first_output() {
    let scratch = Scratch::new();
    let source = scratch.source();
    let html = scratch.path("output.html");
    fs::write(&html, "keep this output").unwrap();
    fs::create_dir(scratch.path("output.pdf")).unwrap();
    let error = call("fmd.render_file", file_args(&source, "both", Some(&scratch.path("output"))), DEFAULT_MAX_INPUT_BYTES).unwrap_err();
    assert_eq!(error.2, "write_error");
    assert_eq!(fs::read_to_string(html).unwrap(), "keep this output");
}

#[test]
fn limits_utf8_and_nonregular_inputs_fail_before_output() {
    let scratch = Scratch::new();
    let source = scratch.source();
    let out = scratch.path("output.html");
    let args = file_args(&source, "html", Some(&out));
    assert_eq!(call("fmd.render_file", args.clone(), 2).unwrap_err().2, "input_too_large");
    assert!(!out.exists());
    fs::write(&source, [0xff]).unwrap();
    assert_eq!(call("fmd.render_file", args, 2).unwrap_err().2, "invalid_utf8");
    assert!(!out.exists());
    assert_eq!(call("fmd.render_file", file_args(&scratch.0, "html", None), 64).unwrap_err().2, "not_regular_file");
}

#[test]
fn invalid_format_is_rejected_before_attempting_a_file_read() {
    let scratch = Scratch::new();
    let error = call("fmd.render_file", file_args(&scratch.path("missing"), "unknown", None), 64).unwrap_err();
    assert_eq!(error.2, "unsupported_target");
}

#[test]
fn invalid_argument_types_and_unknown_options_are_not_silently_ignored() {
    for (tool, arguments) in [
        ("fmd.capabilities", "[]"),
        ("fmd.render_html", r#"{"markdown":"x","toc":"true"}"#),
        ("fmd.render_html", r#"{"markdown":"x","darkMode":false}"#),
        ("fmd.render_html", r#"{"markdown":"x","typoOption":true}"#),
        ("fmd.render_pdf", r#"{"markdown":"x","microtype":true}"#),
        ("fmd.render_file", r#"{"path":"missing.md","out":false}"#),
    ] {
        assert_eq!(call(tool, parse_json(arguments).unwrap(), 64).unwrap_err().0, INVALID_PARAMS);
    }
}

#[test]
fn numeric_options_are_checked_before_narrowing() {
    for field in ["tocDepth", "fitToPages", "metadataEpochSeconds"] {
        for number in ["-1", "1.5", "18446744073709551616", "9007199254740993"] {
            let args = parse_json(&format!("{{\"markdown\":\"x\",\"{field}\":{number}}}")).unwrap();
            assert_eq!(call("fmd.render_pdf", args, 64).unwrap_err().0, INVALID_PARAMS);
        }
    }
    for number in [0, 7, 256, 257] {
        let args = parse_json(&format!("{{\"markdown\":\"x\",\"tocDepth\":{number}}}")).unwrap();
        assert_eq!(call("fmd.render_html", args, 64).unwrap_err().0, INVALID_PARAMS);
    }
    let args = parse_json(r#"{"markdown":"x","fitToPages":0}"#).unwrap();
    assert_eq!(call("fmd.render_pdf", args, 64).unwrap_err().0, INVALID_PARAMS);
}

#[test]
fn html_tool_output_matches_the_requested_core_options() {
    let source = "# Title\n\n## Detail\n\nText.";
    let mut args = parse_json(r#"{"markdown":"","darkMode":"disabled","toc":true,"tocDepth":1,"htmlFontFormat":"woff2","font":"serif"}"#).unwrap();
    let JsonValue::Object(map) = &mut args else { unreachable!() };
    map.insert("markdown".to_string(), JsonValue::String(source.to_string()));
    let mut options = HtmlOptions { toc: true, toc_depth: Some(1), html_font_format: HtmlFontFormat::Woff2, ..Default::default() };
    options.theme.dark_mode = DarkModePolicy::Disabled;
    options.theme.font = FontFamily::Serif;
    let expected = franken_markdown::render_html_document(&franken_markdown::parse_markdown(source), &options).unwrap();
    let actual = call("fmd.render_html", args, DEFAULT_MAX_INPUT_BYTES).unwrap();
    assert_eq!(text(&actual), expected);
}

#[test]
fn pdf_tool_output_matches_requested_microtype_and_metadata() {
    let source = "# Title\n\nSome words with punctuation, some more words.\n";
    let mut args = parse_json(r#"{"markdown":"","microtype":"expansion","metadataEpochSeconds":0,"pageNumbers":true,"tocDepth":2}"#).unwrap();
    let JsonValue::Object(map) = &mut args else { unreachable!() };
    map.insert("markdown".to_string(), JsonValue::String(source.to_string()));
    let options = PdfOptions {
        microtype: franken_markdown::layout::MicrotypeOptions { protrusion: false, ..franken_markdown::layout::MicrotypeOptions::CONSERVATIVE },
        metadata_epoch_seconds: Some(0), page_numbers: true, toc_depth: Some(2), ..Default::default()
    };
    let expected = franken_markdown::render_pdf_document(&franken_markdown::parse_markdown(source), &options).unwrap();
    let actual = call("fmd.render_pdf", args, DEFAULT_MAX_INPUT_BYTES).unwrap();
    assert_eq!(text(&actual), base64_encode(&expected));
}
