//! File configuration exercised through public MCP dispatch and core rendering.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use franken_markdown::{DarkModePolicy, FontFamily, FontScale, HtmlOptions, PdfASettings,
    PdfOptions, SvgOptions, Theme, parse_markdown};
use franken_markdown::mcp::{JsonValue, base64_encode, handle_tool_call, parse_json, tools_list_result};

type Args = BTreeMap<String, JsonValue>;
const SOURCE: &str = "# A report\n\nA paragraph with **style**.\n\n## Evidence\n\n- First\n- Second\n";
static NEXT: AtomicU64 = AtomicU64::new(0);

fn args(json: &str) -> Args { parse_json(json).unwrap().as_object().unwrap().clone() }
fn set(map: &mut Args, key: &str, value: impl Into<String>) {
    map.insert(key.into(), JsonValue::String(value.into()));
}
fn call(name: &str, args: Args) -> Result<JsonValue, (i32, String, &'static str)> {
    handle_tool_call(Some(&JsonValue::Object(BTreeMap::from([
        ("name".into(), JsonValue::String(name.into())),
        ("arguments".into(), JsonValue::Object(args)),
    ]))), 1024 * 1024)
}
fn content(result: &JsonValue) -> &str {
    let JsonValue::Array(items) = result.get("content").unwrap() else { panic!("content array") };
    items[0].get("text").unwrap().as_str().unwrap()
}
fn scratch() -> PathBuf {
    for _ in 0..1000 {
        let path = std::env::temp_dir().join(format!("fmd-mcp-file-options-{}-{}",
            std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create scratch directory: {error}"),
        }
    }
    panic!("scratch names exhausted")
}
fn file_args(target: &str, json: &str) -> (PathBuf, Args) {
    let root = scratch();
    let input = root.join("report.md");
    std::fs::write(&input, SOURCE).unwrap();
    let mut arguments = args(json);
    set(&mut arguments, "path", input.to_string_lossy());
    set(&mut arguments, "to", target);
    (root, arguments)
}
fn theme() -> Theme {
    let mut theme = Theme::default().with_font_scale(FontScale::parse("125%").unwrap());
    theme.font = FontFamily::Serif;
    theme
}

#[test]
fn html_file_uses_metadata_navigation_css_and_typography() {
    let (_, arguments) = file_args("html", r#"{"font":"serif","fontScale":"125%","title":"  Draft  ","lang":"de","toc":true,"tocDepth":2,"customCss":"","darkMode":"disabled"}"#);
    let mut theme = theme();
    theme.dark_mode = DarkModePolicy::Disabled;
    let options = HtmlOptions { theme, title: Some("  Draft  ".into()), lang: Some("de".into()),
        toc: true, toc_depth: Some(2), custom_css: Some(String::new()), ..Default::default() };
    let expected = franken_markdown::render_html_document(&parse_markdown(SOURCE), &options).unwrap();
    assert_eq!(content(&call("fmd.render_file", arguments).unwrap()), expected);
}

#[test]
fn pdf_file_uses_archival_renderer_and_configured_metadata() {
    let (root, mut arguments) = file_args("pdf", r#"{"font":"serif","fontScale":"125%","title":"Archive","author":"Writer","lang":"fr","toc":true,"tocDepth":2,"pageNumbers":true,"metadataEpochSeconds":0,"pdfA":"2b","pdfAStrict":true}"#);
    let options = PdfOptions { theme: theme(), title: Some("Archive".into()), author: Some("Writer".into()),
        lang: Some("fr".into()), toc: true, toc_depth: Some(2), page_numbers: true,
        metadata_epoch_seconds: Some(0), ..Default::default() };
    let expected = franken_markdown::render_pdf_document_pdfa(&parse_markdown(SOURCE), &options,
        PdfASettings::a2b_strict()).unwrap();
    assert_eq!(content(&call("fmd.render_file", arguments.clone()).unwrap()), base64_encode(&expected));
    let destination = root.join("archive.pdf");
    set(&mut arguments, "out", destination.to_string_lossy());
    call("fmd.render_file", arguments).unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), expected);
}

#[test]
fn paired_export_accepts_union_and_configures_both_outputs() {
    let (_, arguments) = file_args("both", r#"{"title":"Pair","font":"serif","toc":true,"customCss":"body { color: navy; }","author":"Writer","pageNumbers":true,"microtype":"expansion"}"#);
    let result = call("fmd.render_file", arguments).unwrap();
    let parsed = parse_json(content(&result)).unwrap();
    let JsonValue::Array(outputs) = parsed.get("outputs").unwrap() else { panic!("outputs array") };
    let theme = Theme { font: FontFamily::Serif, ..Theme::default() };
    let html = HtmlOptions { title: Some("Pair".into()), theme: theme.clone(), toc: true,
        custom_css: Some("body { color: navy; }".into()), ..Default::default() };
    let pdf = PdfOptions { title: Some("Pair".into()), theme, toc: true, author: Some("Writer".into()),
        page_numbers: true, microtype: franken_markdown::layout::MicrotypeOptions {
            protrusion: false, ..franken_markdown::layout::MicrotypeOptions::CONSERVATIVE
        }, ..Default::default() };
    let doc = parse_markdown(SOURCE);
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].get("data").unwrap().as_str().unwrap(), franken_markdown::render_html_document(&doc, &html).unwrap());
    assert_eq!(outputs[1].get("data").unwrap().as_str().unwrap(), base64_encode(&franken_markdown::render_pdf_document(&doc, &pdf).unwrap()));
}

#[test]
fn interactive_file_export_uses_the_requested_html_renderer() {
    let (_, arguments) = file_args("html", r#"{"title":"Workspace","interactiveHtml":true,"lang":"en","font":"serif"}"#);
    let theme = Theme { font: FontFamily::Serif, ..Theme::default() };
    let opts = HtmlOptions { theme, title: Some("Workspace".into()), lang: Some("en".into()), ..Default::default() };
    let expected = franken_markdown::interactive::render_interactive_html(&parse_markdown(SOURCE), SOURCE, &opts);
    assert_eq!(content(&call("fmd.render_file", arguments).unwrap()), expected);
}

#[test]
fn epub_file_consumes_shared_publication_settings() {
    let (_, arguments) = file_args("epub", r#"{"title":"Publication","font":"serif","fontScale":"125%","lang":"es","toc":true,"tocDepth":1,"customCss":"p { line-height: 1.8; }"}"#);
    let options = HtmlOptions { theme: theme(), title: Some("Publication".into()), lang: Some("es".into()),
        toc: true, toc_depth: Some(1), custom_css: Some("p { line-height: 1.8; }".into()), ..Default::default() };
    let expected = franken_markdown::render_epub(&parse_markdown(SOURCE), &options).unwrap();
    assert_eq!(content(&call("fmd.render_file", arguments).unwrap()), base64_encode(&expected));
}

#[test]
fn svg_file_consumes_font_and_actual_poster_width() {
    let (_, arguments) = file_args("svg", r#"{"font":"serif","maxWidthPt":300}"#);
    let theme = Theme { font: FontFamily::Serif, ..Theme::default() };
    let expected = franken_markdown::render_svg(&parse_markdown(SOURCE), &SvgOptions { theme, max_width_pt: 300.0 });
    assert_eq!(content(&call("fmd.render_file", arguments).unwrap()).as_bytes(), expected);
}

#[test]
fn absent_title_keeps_filename_default_but_explicit_empty_is_preserved() {
    for title in [None, Some(""), Some("  ")] {
        let (_, mut arguments) = file_args("html", "{}");
        if let Some(title) = title { set(&mut arguments, "title", title); }
        let opts = HtmlOptions { title: Some(title.unwrap_or("report").into()), ..Default::default() };
        assert_eq!(content(&call("fmd.render_file", arguments).unwrap()),
            franken_markdown::render_html_document(&parse_markdown(SOURCE), &opts).unwrap());
    }
}

#[test]
fn option_refusals_happen_before_source_access() {
    let missing = scratch().join("not-created.md");
    for (json, reason) in [
        (r#"{"to":"pdf","customCss":"x"}"#, "unsupported_target_option"),
        (r#"{"to":"pdf","font":"missing-font"}"#, "invalid_font"),
        (r#"{"to":"both","pdfAStrict":true}"#, "invalid_pdf_a"),
        (r#"{"to":"svg","maxWidthPt":143}"#, "invalid_params"),
        (r#"{"to":"epub","interactiveHtml":false}"#, "unsupported_target_option"),
    ] {
        let mut arguments = args(json); set(&mut arguments, "path", missing.to_string_lossy());
        assert_eq!(call("fmd.render_file", arguments).unwrap_err().2, reason);
    }
}

#[test]
fn invalid_configuration_keeps_source_and_existing_pair_untouched() {
    let (root, mut arguments) = file_args("both", r#"{"pdfAStrict":true,"pdfA":"off","title":"Changed"}"#);
    let out = root.join("export.anything"); set(&mut arguments, "out", out.to_string_lossy());
    std::fs::write(out.with_extension("html"), "old html").unwrap();
    std::fs::write(out.with_extension("pdf"), "old pdf").unwrap();
    assert_eq!(call("fmd.render_file", arguments).unwrap_err().2, "invalid_pdf_a");
    assert_eq!(std::fs::read_to_string(out.with_extension("html")).unwrap(), "old html");
    assert_eq!(std::fs::read_to_string(out.with_extension("pdf")).unwrap(), "old pdf");
    assert_eq!(std::fs::read_to_string(root.join("report.md")).unwrap(), SOURCE);
}

#[test]
fn configured_output_still_cannot_replace_its_source() {
    let (root, mut arguments) = file_args("pdf", r#"{"title":"Changed","font":"serif"}"#);
    let input = root.join("report.md"); set(&mut arguments, "out", input.to_string_lossy());
    assert_eq!(call("fmd.render_file", arguments).unwrap_err().2, "output_overwrites_input");
    assert_eq!(std::fs::read_to_string(input).unwrap(), SOURCE);
}

#[test]
fn file_schema_exposes_configuration_without_accepting_inline_source() {
    let result = tools_list_result();
    let JsonValue::Array(tools) = result.get("tools").unwrap() else { panic!("tools array") };
    let file = tools.iter().find(|tool| tool.get("name").and_then(JsonValue::as_str) == Some("fmd.render_file")).unwrap();
    let props = file.get("inputSchema").unwrap().get("properties").unwrap();
    for field in ["font", "title", "author", "toc", "customCss", "microtype", "pdfA", "maxWidthPt"] {
        assert!(props.get(field).is_some(), "{field}");
    }
    assert!(props.get("markdown").is_none());
    assert_eq!(props.get("maxWidthPt").unwrap().get("minimum").unwrap().as_f64(), Some(144.0));
    assert!(props.get("author").unwrap().get("description").unwrap().as_str().unwrap().contains("pdf, both"));
}
