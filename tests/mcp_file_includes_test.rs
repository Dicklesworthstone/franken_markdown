//! Native include admission and publication through the public MCP dispatcher.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::mcp::{
    JsonValue, base64_encode, handle_tool_call, parse_json, tools_list_result,
};
use franken_markdown::{HtmlOptions, PdfOptions, SvgOptions, parse_markdown};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type Args = BTreeMap<String, JsonValue>;
static NEXT: AtomicU64 = AtomicU64::new(0);
const LIMIT: u64 = 1024 * 1024;

fn scratch() -> PathBuf {
    for _ in 0..1000 {
        let path = std::env::temp_dir().join(format!(
            "fmd-mcp-includes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create scratch directory: {error}"),
        }
    }
    panic!("scratch names exhausted")
}

fn set(args: &mut Args, name: &str, value: impl Into<String>) {
    args.insert(name.into(), JsonValue::String(value.into()));
}

fn arguments(input: &Path, target: &str) -> Args {
    BTreeMap::from([
        (
            "path".into(),
            JsonValue::String(input.to_string_lossy().into_owned()),
        ),
        ("to".into(), JsonValue::String(target.into())),
        ("expandIncludes".into(), JsonValue::Bool(true)),
    ])
}

fn call(args: Args, limit: u64) -> Result<JsonValue, (i32, String, &'static str)> {
    handle_tool_call(
        Some(&JsonValue::Object(BTreeMap::from([
            ("name".into(), JsonValue::String("fmd.render_file".into())),
            ("arguments".into(), JsonValue::Object(args)),
        ]))),
        limit,
    )
}

fn content(result: &JsonValue) -> &str {
    let JsonValue::Array(items) = result.get("content").unwrap() else {
        panic!("content array")
    };
    items[0].get("text").unwrap().as_str().unwrap()
}

#[test]
fn includes_are_explicit_and_omission_preserves_literal_output() {
    let root = scratch();
    let source = "# Root\n\n{{#include missing.md}}\n";
    let input = root.join("report.md");
    fs::write(&input, source).unwrap();
    let expected = franken_markdown::render_html_document(
        &parse_markdown(source),
        &HtmlOptions {
            title: Some("report".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let mut args = arguments(&input, "html");
    args.remove("expandIncludes");
    assert_eq!(content(&call(args.clone(), LIMIT).unwrap()), expected);
    args.insert("expandIncludes".into(), JsonValue::Bool(false));
    assert_eq!(content(&call(args.clone(), LIMIT).unwrap()), expected);
    args.insert("expandIncludes".into(), JsonValue::Bool(true));
    assert_eq!(call(args, LIMIT).unwrap_err().2, "include_error");
}

#[test]
fn all_file_targets_publish_the_same_expanded_document() {
    let root = scratch();
    let input = root.join("report.md");
    fs::create_dir(root.join("parts")).unwrap();
    fs::write(&input, "# Root\n\n{{#include parts/body.md}}\n").unwrap();
    fs::write(
        root.join("parts/body.md"),
        "## Included\n\nSome **body**.\n",
    )
    .unwrap();
    let doc = parse_markdown("# Root\n\n## Included\n\nSome **body**.\n");
    let html_options = HtmlOptions {
        title: Some("report".into()),
        ..Default::default()
    };
    let pdf_options = PdfOptions {
        title: Some("report".into()),
        ..Default::default()
    };
    let html = franken_markdown::render_html_document(&doc, &html_options).unwrap();
    let pdf = franken_markdown::render_pdf_document(&doc, &pdf_options).unwrap();
    let epub = franken_markdown::render_epub(&doc, &html_options).unwrap();
    let svg = franken_markdown::svg::render_svg_with_resources(
        &doc,
        &SvgOptions::default(),
        &pdf_options.font_assets,
        &pdf_options.image_assets,
    )
    .unwrap()
    .0;
    for (target, expected) in [
        ("html", html.clone()),
        ("pdf", base64_encode(&pdf)),
        ("epub", base64_encode(&epub)),
        ("svg", String::from_utf8(svg).unwrap()),
    ] {
        assert_eq!(
            content(&call(arguments(&input, target), LIMIT).unwrap()),
            expected,
            "{target}"
        );
    }
    let both = call(arguments(&input, "both"), LIMIT).unwrap();
    let receipt = parse_json(content(&both)).unwrap();
    let JsonValue::Array(outputs) = receipt.get("outputs").unwrap() else {
        panic!("outputs")
    };
    assert_eq!(
        outputs[0].get("data").unwrap().as_str(),
        Some(html.as_str())
    );
    assert_eq!(
        outputs[1].get("data").unwrap().as_str(),
        Some(base64_encode(&pdf).as_str())
    );
}

#[test]
fn nested_selectors_rebase_links_and_match_explicit_image_bindings() {
    let root = scratch();
    let input = root.join("report.md");
    fs::create_dir_all(root.join("parts/child")).unwrap();
    fs::write(&input, "# Root\n\n{{#include parts/body.md}}\n").unwrap();
    fs::write(root.join("parts/body.md"),
        "{{#include child/nested.md:chosen}}\n\n{{#include lines.md:2:2}}\n\n[Read](notes.md#read)\n\n![Plot](plot.svg)\n").unwrap();
    fs::write(root.join("parts/child/nested.md"),
        "Discarded\n<!-- ANCHOR: chosen -->\nNested **selection**.\n<!-- ANCHOR_END: chosen -->\nDiscarded\n").unwrap();
    fs::write(
        root.join("parts/lines.md"),
        "Discarded\nSelected line.\nDiscarded\n",
    )
    .unwrap();
    // A file on disk is not an asset grant, even when includes are authorized.
    let image = br##"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="8"><path fill="#28a" d="M0 0h12v8H0z"/></svg>"##;
    fs::write(root.join("parts/plot.svg"), image).unwrap();
    let mut args = arguments(&input, "html");
    let without_asset = call(args.clone(), LIMIT).unwrap();
    let html = content(&without_asset);
    assert!(html.contains("Nested <strong>selection</strong>."));
    assert!(html.contains("Selected line."));
    assert!(!html.contains("Discarded"));
    assert!(html.contains("href=\"parts/notes.md#read\""));
    assert!(html.contains("src=\"parts/plot.svg\""));
    assert!(!html.contains("data:image/svg+xml"));
    args.insert(
        "images".into(),
        JsonValue::Array(vec![JsonValue::Object(BTreeMap::from([
            (
                "destination".into(),
                JsonValue::String("parts/plot.svg".into()),
            ),
            ("base64".into(), JsonValue::String(base64_encode(image))),
        ]))]),
    );
    assert!(content(&call(args, LIMIT).unwrap()).contains("data:image/svg+xml"));
}

#[test]
fn expansion_uses_the_selected_grammar_and_keeps_code_literal() {
    let root = scratch();
    let input = root.join("report.md");
    let source = "# Root\n\n```md\n{{#include absent.md}}\n```\n\n    {{#include absent.md}}\n\n{{#include body.md}}\n";
    fs::write(&input, source).unwrap();
    fs::write(root.join("body.md"), "Term\n: Definition\n").unwrap();
    let mut args = arguments(&input, "html");
    set(&mut args, "profile", "commonmark-gfm");
    let commonmark = call(args.clone(), LIMIT).unwrap();
    assert!(!content(&commonmark).contains("<dl>"));
    assert!(content(&commonmark).contains("absent.md"));
    set(&mut args, "profile", "gfm-plus");
    assert!(content(&call(args, LIMIT).unwrap()).contains("<dl>"));
}

#[test]
fn selected_grammar_controls_include_discovery_inside_nested_multiline_code() {
    let root = scratch();
    let input = root.join("report.md");
    fs::write(&input, "{{#include body.md}}\n").unwrap();
    fs::write(
        root.join("body.md"),
        "`Example\n: code\n{{#include missing.txt}}\ntail`\n",
    )
    .unwrap();
    let mut args = arguments(&input, "html");
    set(&mut args, "profile", "commonmark-gfm");
    let output = call(args.clone(), LIMIT).unwrap();
    assert!(content(&output).contains("{{#include missing.txt}}"));
    assert!(content(&output).contains("<code>Example : code"));
    set(&mut args, "profile", "gfm-plus");
    assert_eq!(call(args, LIMIT).unwrap_err().2, "include_error");
}

#[test]
fn interactive_output_carries_expanded_source_for_future_edits() {
    let root = scratch();
    let input = root.join("report.md");
    fs::write(&input, "{{#include body.md}}\n").unwrap();
    fs::write(root.join("body.md"), "# Included source\n").unwrap();
    let mut args = arguments(&input, "html");
    args.insert("interactiveHtml".into(), JsonValue::Bool(true));
    let result = call(args, LIMIT).unwrap();
    let html = content(&result);
    assert!(html.contains("Included source"));
    assert!(!html.contains("{{#include body.md}}"));
}

#[test]
fn includes_cannot_replace_any_source_in_a_paired_publication() {
    let root = scratch();
    let input = root.join("report.md");
    let fragment = root.join("publication.html");
    let sibling = root.join("publication.pdf");
    fs::write(&input, "{{#include publication.html}}\n").unwrap();
    fs::write(&fragment, "# Included fragment\n").unwrap();
    fs::write(&sibling, "existing PDF").unwrap();
    let mut args = arguments(&input, "both");
    set(&mut args, "out", sibling.to_string_lossy());
    assert_eq!(call(args, LIMIT).unwrap_err().2, "output_overwrites_input");
    assert_eq!(
        fs::read_to_string(fragment).unwrap(),
        "# Included fragment\n"
    );
    assert_eq!(fs::read_to_string(sibling).unwrap(), "existing PDF");
    assert_eq!(
        fs::read_to_string(input).unwrap(),
        "{{#include publication.html}}\n"
    );
}

#[cfg(unix)]
#[test]
fn include_aliases_are_protected_and_escaping_aliases_are_refused() {
    use std::os::unix::fs::symlink;
    let root = scratch();
    let input = root.join("report.md");
    let fragment = root.join("body.md");
    fs::write(&input, "{{#include body.md}}\n").unwrap();
    fs::write(&fragment, "# Included fragment\n").unwrap();
    let hard = root.join("hard.html");
    let symbolic = root.join("symbolic.html");
    fs::hard_link(&fragment, &hard).unwrap();
    symlink(&fragment, &symbolic).unwrap();
    for output in [&hard, &symbolic] {
        let mut args = arguments(&input, "html");
        set(&mut args, "out", output.to_string_lossy());
        assert_eq!(call(args, LIMIT).unwrap_err().2, "output_overwrites_input");
        assert_eq!(fs::read_to_string(output).unwrap(), "# Included fragment\n");
    }
    let outside = scratch().join("secret.md");
    fs::write(&outside, "Outside root").unwrap();
    symlink(&outside, root.join("escape.md")).unwrap();
    fs::write(&input, "{{#include escape.md}}\n").unwrap();
    let error = call(arguments(&input, "html"), LIMIT).unwrap_err();
    assert_eq!(error.2, "include_error");
    assert!(error.1.contains("include_escape"));
}

#[cfg(unix)]
#[test]
fn selected_source_symlink_keeps_its_requested_include_root() {
    use std::os::unix::fs::symlink;
    let root = scratch();
    let original = scratch();
    fs::write(original.join("report.md"), "{{#include body.md}}\n").unwrap();
    fs::write(original.join("body.md"), "Wrong directory").unwrap();
    fs::write(root.join("body.md"), "Chosen directory").unwrap();
    symlink(original.join("report.md"), root.join("report.md")).unwrap();
    let output = call(arguments(&root.join("report.md"), "html"), LIMIT).unwrap();
    assert!(content(&output).contains("Chosen directory"));
    assert!(!content(&output).contains("Wrong directory"));
}

#[test]
fn expansion_failures_preserve_all_existing_outputs() {
    let root = scratch();
    let input = root.join("report.md");
    fs::create_dir(root.join("directory")).unwrap();
    fs::write(root.join("invalid.md"), [0xff]).unwrap();
    fs::write(root.join("cycle.md"), "{{#include cycle.md}}\n").unwrap();
    let html = root.join("output.html");
    let pdf = root.join("output.pdf");
    fs::write(&html, "old HTML").unwrap();
    fs::write(&pdf, "old PDF").unwrap();
    for target in ["missing.md", "directory", "invalid.md", "cycle.md"] {
        fs::write(&input, format!("{{{{#include {target}}}}}\n")).unwrap();
        let mut args = arguments(&input, "both");
        set(&mut args, "out", pdf.to_string_lossy());
        assert_eq!(
            call(args, LIMIT).unwrap_err().2,
            "include_error",
            "{target}"
        );
        assert_eq!(fs::read_to_string(&html).unwrap(), "old HTML");
        assert_eq!(fs::read_to_string(&pdf).unwrap(), "old PDF");
    }
}

#[test]
fn both_individual_include_reads_and_total_expansion_are_bounded() {
    let root = scratch();
    let input = root.join("report.md");
    fs::write(&input, "{{#include body.md}}\n").unwrap();
    fs::write(root.join("body.md"), "x".repeat(65)).unwrap();
    let error = call(arguments(&input, "html"), 64).unwrap_err();
    assert_eq!(error.2, "include_error");
    assert!(error.1.contains("include_oversize"));
    fs::write(&input, "{{#include body.md}}\n{{#include body.md}}\n").unwrap();
    fs::write(root.join("body.md"), "x".repeat(40)).unwrap();
    let error = call(arguments(&input, "html"), 64).unwrap_err();
    assert_eq!(error.2, "include_error");
    assert!(error.1.contains("include_size"));
}

#[test]
fn include_discovery_and_admission_are_file_only_and_strictly_typed() {
    let result = tools_list_result();
    let JsonValue::Array(tools) = result.get("tools").unwrap() else {
        panic!("tools")
    };
    for tool in tools {
        let schema = tool.get("inputSchema").unwrap().get("properties").unwrap();
        if tool.get("name").and_then(JsonValue::as_str) == Some("fmd.render_file") {
            assert_eq!(
                schema
                    .get("expandIncludes")
                    .unwrap()
                    .get("type")
                    .and_then(JsonValue::as_str),
                Some("boolean")
            );
        } else {
            assert!(schema.get("expandIncludes").is_none());
        }
    }
    let mut args = arguments(Path::new("missing.md"), "html");
    set(&mut args, "expandIncludes", "true");
    let error = call(args, LIMIT).unwrap_err();
    assert_eq!(error.0, -32602);
    assert_eq!(error.2, "invalid_params");
    assert!(error.1.contains("expandIncludes"));
}
