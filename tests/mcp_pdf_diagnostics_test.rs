//! PDF fidelity diagnostics must travel with the exact MCP publication.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, JsonValue as Json, MAX_FRAME_BYTES, base64_encode, handle_tool_call,
    parse_json, serve_stdio,
};
use franken_markdown::{PdfASettings, PdfOptions, parse_markdown};

type Args = BTreeMap<String, Json>;
static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE: &str = "# Report\n\n![Required chart](missing.svg)\n\nBody text.\n";

fn args(config: &str) -> Args {
    parse_json(config).unwrap().as_object().unwrap().clone()
}

fn text(value: impl Into<String>) -> Json {
    Json::String(value.into())
}

fn params(name: &str, arguments: Args) -> Json {
    Json::Object(BTreeMap::from([
        ("name".into(), text(name)),
        ("arguments".into(), Json::Object(arguments)),
    ]))
}

fn call(name: &str, arguments: Args) -> Json {
    handle_tool_call(Some(&params(name, arguments)), DEFAULT_MAX_INPUT_BYTES).unwrap()
}

fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(values) => values,
        _ => panic!("expected array"),
    }
}

fn content(response: &Json) -> &str {
    assert_eq!(response.get("isError"), Some(&Json::Bool(false)));
    array(response.get("content").unwrap())[0]
        .get("text")
        .and_then(Json::as_str)
        .unwrap()
}

fn warning<'a>(response: &'a Json, code: &str) -> &'a Json {
    array(response.get("warnings").expect("PDF diagnostics were lost"))
        .iter()
        .find(|warning| warning.get("code").and_then(Json::as_str) == Some(code))
        .unwrap_or_else(|| panic!("missing diagnostic {code}: {response:?}"))
}

fn source_args(source: &str, config: &str) -> Args {
    let mut arguments = args(config);
    arguments.insert("markdown".into(), text(source));
    arguments
}

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "fmd-mcp-pdf-diagnostics-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}

#[test]
fn source_pdf_reports_missing_content_without_changing_pdf_or_pdfa_bytes() {
    for (config, settings) in [
        ("{}", PdfASettings::OFF),
        (
            r#"{"pdfA":"2b","pdfAStrict":true}"#,
            PdfASettings::a2b_strict(),
        ),
    ] {
        let response = call("fmd.render_pdf", source_args(SOURCE, config));
        let expected = franken_markdown::render_pdf_document_pdfa(
            &parse_markdown(SOURCE),
            &PdfOptions::default(),
            settings,
        )
        .unwrap();
        assert_eq!(content(&response), base64_encode(&expected));
        let warning = warning(&response, "unresolved_image");
        assert!(
            warning
                .get("message")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("missing.svg")
        );
    }
}

#[test]
fn source_pdf_keeps_invalid_image_and_missing_glyph_diagnostics() {
    let source = "![Broken chart](broken.png)\n\nUncovered character: \u{10ffff}.";
    let config = format!(
        r#"{{"images":[{{"destination":"broken.png","base64":"{}"}}]}}"#,
        base64_encode(b"not an image")
    );
    let response = call("fmd.render_pdf", source_args(source, &config));
    warning(&response, "unsupported_image");
    warning(&response, "missing_glyphs");
    assert!(content(&response).starts_with("JVBERi0"));
}

#[test]
fn file_pdf_and_paired_exports_report_warnings_inline_and_after_staged_save() {
    let root = scratch();
    let input = root.join("report.md");
    std::fs::write(&input, SOURCE).unwrap();
    let options = PdfOptions {
        title: Some("report".into()),
        ..Default::default()
    };
    let expected = franken_markdown::render_pdf(SOURCE, &options).unwrap();
    for target in ["pdf", "both"] {
        for save in [false, true] {
            let output = root.join(format!("{target}.pdf"));
            let mut arguments = args("{}");
            arguments.insert("path".into(), text(input.to_string_lossy()));
            arguments.insert("to".into(), text(target));
            if save {
                arguments.insert("out".into(), text(output.to_string_lossy()));
            }
            let response = call("fmd.render_file", arguments);
            warning(&response, "unresolved_image");
            if save {
                assert_eq!(std::fs::read(&output).unwrap(), expected);
                assert!(content(&response).contains("written to"));
                if target == "both" {
                    assert!(
                        std::fs::read_to_string(output.with_extension("html"))
                            .unwrap()
                            .contains("Required chart")
                    );
                }
            } else if target == "pdf" {
                assert_eq!(content(&response), base64_encode(&expected));
            } else {
                let receipt = parse_json(content(&response)).unwrap();
                let pdf = array(receipt.get("outputs").unwrap())
                    .iter()
                    .find(|item| item.get("format").and_then(Json::as_str) == Some("pdf"))
                    .unwrap();
                assert_eq!(
                    pdf.get("data").unwrap().as_str().unwrap(),
                    base64_encode(&expected)
                );
            }
            assert_eq!(std::fs::read_to_string(&input).unwrap(), SOURCE);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn nonstrict_pagination_fallback_is_visible_and_strict_failure_remains_an_error() {
    let config = r#"{"optimalPagination":true,"baseFontSize":24,"page":{"margins":{"topPt":360,"bottomPt":360}}}"#;
    let source = "A  \nB  \nC";
    let response = call("fmd.render_pdf", source_args(source, config));
    let message = warning(&response, "pagination_infeasible")
        .get("message")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(message.contains("exact PDF pagination could not be honored"));
    let mut arguments = source_args(source, config);
    arguments.insert("strictPagination".into(), Json::Bool(true));
    let error = handle_tool_call(
        Some(&params("fmd.render_pdf", arguments)),
        DEFAULT_MAX_INPUT_BYTES,
    )
    .unwrap_err();
    assert_eq!(error.2, "pagination_infeasible");
}

#[test]
fn book_pdf_warnings_remain_attached_to_pdf_inside_single_and_multi_format_receipts() {
    let inputs = [BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }];
    let renderer = BookRenderer::new(&inputs).unwrap();
    for target in ["pdf", "all"] {
        for archival in [false, true] {
            let mut arguments = args("{}");
            arguments.insert("to".into(), text(target));
            arguments.insert(
                "chapters".into(),
                Json::Array(vec![Json::Object(BTreeMap::from([
                    ("path".into(), text("chapter.md")),
                    ("markdown".into(), text(SOURCE)),
                ]))]),
            );
            if archival {
                arguments.insert("pdfA".into(), text("2b"));
                arguments.insert("pdfAStrict".into(), Json::Bool(true));
            }
            let response = call("fmd.render_book", arguments);
            let receipt = parse_json(content(&response)).unwrap();
            let outputs = array(receipt.get("outputs").unwrap());
            let pdf = outputs
                .iter()
                .find(|output| output.get("format").and_then(Json::as_str) == Some("pdf"))
                .unwrap();
            warning(pdf, "unresolved_image");
            let expected = renderer
                .render_pdf_with_options(
                    &PdfOptions::default(),
                    if archival {
                        PdfASettings::a2b_strict()
                    } else {
                        PdfASettings::OFF
                    },
                )
                .unwrap();
            assert_eq!(
                pdf.get("data").unwrap().as_str().unwrap(),
                base64_encode(&expected)
            );
            for output in outputs {
                if output.get("format").and_then(Json::as_str) != Some("pdf") {
                    assert!(
                        output.get("warnings").is_none(),
                        "PDF warnings were attributed to another format"
                    );
                }
            }
            warning(&response, "unresolved_image");
        }
    }
}

#[test]
fn clean_pdf_keeps_the_existing_response_shape_and_stdio_preserves_warnings() {
    let clean = call(
        "fmd.render_pdf",
        source_args("# Clean\n\nPlain text.", "{}"),
    );
    assert!(clean.get("warnings").is_none());
    assert_eq!(clean.as_object().unwrap().len(), 2);
    let request = Json::Object(BTreeMap::from([
        ("jsonrpc".into(), text("2.0")),
        ("id".into(), Json::Number(1.0)),
        ("method".into(), text("tools/call")),
        (
            "params".into(),
            params("fmd.render_pdf", source_args(SOURCE, "{}")),
        ),
    ]));
    let mut output = Vec::new();
    let input = format!("{}\n", request.to_json_string());
    assert_eq!(
        serve_stdio(
            &mut Cursor::new(input),
            &mut output,
            DEFAULT_MAX_INPUT_BYTES,
            MAX_FRAME_BYTES
        )
        .unwrap(),
        1
    );
    let response = parse_json(std::str::from_utf8(&output).unwrap().trim()).unwrap();
    let result = response.get("result").unwrap();
    warning(result, "unresolved_image");
    assert_eq!(
        content(result),
        content(&call("fmd.render_pdf", source_args(SOURCE, "{}")))
    );
}
