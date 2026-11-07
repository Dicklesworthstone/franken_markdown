//! Real-renderer coverage for shared MCP pagination policy and baseline grids.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::{BookInput, BookRenderer, render_book_site};
use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, ERROR_INVALID_OPTIONS, ERROR_RENDER_FAILED, INVALID_PARAMS, JsonValue,
    MAX_FRAME_BYTES, base64_encode, handle_tool_call, parse_json, serve_stdio, tools_list_result,
};
use franken_markdown::{
    BaselineGrid, HtmlOptions, PageMargins, PageSize, PdfASettings, PdfOptions, parse_markdown,
};

type Args = BTreeMap<String, JsonValue>;
type Failure = (i32, String, &'static str);

const SOURCE: &str = "# A measured grid\n\nBody text follows an explicit baseline rhythm.\n\nA second paragraph includes **strong text** and `code`.\n";
const GRID: &str =
    r#"{"strictPagination":true,"optimalPagination":false,"baselineGridPt":13.1254}"#;
const INFEASIBLE: &str = "A  \nB  \nC";
const TIGHT: &str = r#"{"strictPagination":true,"baseFontSize":24,"page":{"margins":{"topPt":360,"bottomPt":360}}}"#;
const TALL: &str =
    r#"{"strictPagination":true,"page":{"size":{"widthPt":612,"heightPt":10200},"margins":36}}"#;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn args(config: &str, source: &str) -> Args {
    let mut fields = parse_json(config).unwrap().as_object().unwrap().clone();
    fields.insert("markdown".into(), JsonValue::String(source.into()));
    fields
}

fn params(name: &str, arguments: Args) -> JsonValue {
    JsonValue::Object(BTreeMap::from([
        ("name".into(), JsonValue::String(name.into())),
        ("arguments".into(), JsonValue::Object(arguments)),
    ]))
}

fn call(name: &str, arguments: Args) -> Result<JsonValue, Failure> {
    handle_tool_call(Some(&params(name, arguments)), DEFAULT_MAX_INPUT_BYTES)
}

fn array(value: &JsonValue) -> &[JsonValue] {
    let JsonValue::Array(items) = value else {
        panic!("expected array")
    };
    items
}

fn content(result: &JsonValue) -> &str {
    assert_eq!(result.get("isError"), Some(&JsonValue::Bool(false)));
    array(result.get("content").unwrap())[0]
        .get("text")
        .and_then(JsonValue::as_str)
        .unwrap()
}

fn receipt(result: &JsonValue) -> JsonValue {
    parse_json(content(result)).unwrap()
}

fn output<'a>(receipt: &'a JsonValue, format: &str) -> &'a JsonValue {
    array(receipt.get("outputs").unwrap())
        .iter()
        .find(|item| item.get("format").and_then(JsonValue::as_str) == Some(format))
        .expect("requested output")
}

fn payload(output: &JsonValue) -> &str {
    output.get("data").and_then(JsonValue::as_str).unwrap()
}

fn file_args(config: &str, path: &Path, target: &str) -> Args {
    let mut fields = args(config, "");
    fields.remove("markdown");
    fields.insert(
        "path".into(),
        JsonValue::String(path.to_string_lossy().into_owned()),
    );
    fields.insert("to".into(), JsonValue::String(target.into()));
    fields
}

fn chapter(path: &str, source: &str) -> JsonValue {
    JsonValue::Object(BTreeMap::from([
        ("path".into(), JsonValue::String(path.into())),
        ("markdown".into(), JsonValue::String(source.into())),
    ]))
}

fn book_args(config: &str, source: &str, target: &str) -> Args {
    let mut fields = args(config, "");
    fields.remove("markdown");
    fields.insert(
        "chapters".into(),
        JsonValue::Array(vec![chapter("one.md", source)]),
    );
    fields.insert("to".into(), JsonValue::String(target.into()));
    fields
}

fn scratch() -> PathBuf {
    for _ in 0..1000 {
        let path = std::env::temp_dir().join(format!(
            "fmd-mcp-pagination-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create scratch directory: {error}"),
        }
    }
    panic!("scratch names exhausted")
}

fn grid_options() -> PdfOptions {
    let mut options = PdfOptions {
        optimal_pagination: true,
        strict_pagination: true,
        ..Default::default()
    };
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(13.1254).unwrap());
    options
}

fn tight_options() -> PdfOptions {
    let mut options = PdfOptions {
        optimal_pagination: true,
        strict_pagination: true,
        base_font_size: Some(24.0),
        ..Default::default()
    };
    options.theme.page.margins.top_pt = 360.0;
    options.theme.page.margins.bottom_pt = 360.0;
    options
}

fn tall_options() -> PdfOptions {
    let mut options = PdfOptions {
        optimal_pagination: true,
        strict_pagination: true,
        ..Default::default()
    };
    options.theme.page.size = PageSize {
        name: "custom",
        width_pt: 612.0,
        height_pt: 10200.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options
}

fn assert_refused(result: Result<JsonValue, Failure>, reason: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.0, ERROR_RENDER_FAILED, "{error:?}");
    assert_eq!(error.2, reason, "{error:?}");
    assert!(
        error.1.contains("strict PDF pagination failed"),
        "{error:?}"
    );
}

#[test]
fn strict_grid_matches_core_pdf_bytes_and_quantizes_once() {
    let doc = parse_markdown(SOURCE);
    let expected = franken_markdown::render_pdf_document(&doc, &grid_options()).unwrap();
    assert_eq!(
        content(&call("fmd.render_pdf", args(GRID, SOURCE)).unwrap()),
        base64_encode(&expected)
    );
    assert_ne!(
        expected,
        franken_markdown::render_pdf_document(&doc, &PdfOptions::default()).unwrap()
    );
    let mut quantized = args(GRID, SOURCE);
    quantized.insert("baselineGridPt".into(), JsonValue::Number(13.125));
    assert_eq!(
        content(&call("fmd.render_pdf", quantized).unwrap()),
        base64_encode(&expected)
    );

    let mut archival = args(GRID, SOURCE);
    archival.insert("pdfA".into(), JsonValue::String("2b".into()));
    archival.insert("pdfAStrict".into(), JsonValue::Bool(true));
    let expected = franken_markdown::render_pdf_document_pdfa(
        &doc,
        &grid_options(),
        PdfASettings::a2b_strict(),
    )
    .unwrap();
    assert_eq!(
        content(&call("fmd.render_pdf", archival).unwrap()),
        base64_encode(&expected)
    );
}

#[test]
fn omitted_and_disabled_controls_preserve_defaults_and_zero_allows_fitting() {
    let doc = parse_markdown(SOURCE);
    let expected = franken_markdown::render_pdf_document(&doc, &PdfOptions::default()).unwrap();
    for config in ["{}", r#"{"strictPagination":false,"baselineGridPt":0}"#] {
        assert_eq!(
            content(&call("fmd.render_pdf", args(config, SOURCE)).unwrap()),
            base64_encode(&expected)
        );
    }
    let options = PdfOptions {
        fit_to_pages: Some(1),
        ..Default::default()
    };
    assert_eq!(
        content(
            &call(
                "fmd.render_pdf",
                args(r#"{"baselineGridPt":0,"fitToPages":1}"#, SOURCE),
            )
            .unwrap()
        ),
        base64_encode(&franken_markdown::render_pdf_document(&doc, &options).unwrap())
    );
}

#[test]
fn file_and_paired_exports_share_the_grid_without_changing_html() {
    let workspace = scratch();
    let source = workspace.join("source.md");
    std::fs::write(&source, SOURCE).unwrap();
    let mut arguments = file_args(GRID, &source, "both");
    arguments.insert("title".into(), JsonValue::String("Grid report".into()));
    let mut options = grid_options();
    options.title = Some("Grid report".into());
    let doc = parse_markdown(SOURCE);
    let pdf = franken_markdown::render_pdf_document(&doc, &options).unwrap();
    let html = franken_markdown::render_html_document(
        &doc,
        &HtmlOptions {
            title: Some("Grid report".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let paired = receipt(&call("fmd.render_file", arguments.clone()).unwrap());
    assert_eq!(payload(output(&paired, "pdf")), base64_encode(&pdf));
    assert_eq!(payload(output(&paired, "html")), html);

    let mut single = arguments.clone();
    single.insert("to".into(), JsonValue::String("pdf".into()));
    assert_eq!(
        content(&call("fmd.render_file", single).unwrap()),
        base64_encode(&pdf)
    );
    arguments.insert(
        "out".into(),
        JsonValue::String(
            workspace
                .join("publication.pdf")
                .to_string_lossy()
                .into_owned(),
        ),
    );
    call("fmd.render_file", arguments).unwrap();
    assert_eq!(
        std::fs::read(workspace.join("publication.pdf")).unwrap(),
        pdf
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("publication.html")).unwrap(),
        html
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
}

#[test]
fn combined_book_applies_grid_to_pdf_and_preserves_other_artifacts() {
    let one = "# One\n\n[Next](two.md#two)\n\nFirst chapter body.";
    let two = "# Two\n\n[Back](one.md#one)\n\nSecond chapter body.";
    let mut arguments = book_args(GRID, one, "all");
    arguments.insert(
        "chapters".into(),
        JsonValue::Array(vec![chapter("one.md", one), chapter("two.md", two)]),
    );
    arguments.insert("strictLinks".into(), JsonValue::Bool(true));
    let renderer = BookRenderer::from_sources(
        &[
            BookInput {
                path: "one.md".into(),
                source: one.into(),
            },
            BookInput {
                path: "two.md".into(),
                source: two.into(),
            },
        ],
        &[],
    )
    .unwrap();
    let expected = [
        (
            "pdf",
            renderer
                .render_pdf_with_options(&grid_options(), PdfASettings::OFF)
                .unwrap(),
        ),
        (
            "epub",
            franken_markdown::epub::render_book_epub(renderer.book(), &HtmlOptions::default())
                .unwrap(),
        ),
        (
            "site",
            render_book_site(renderer.book(), &HtmlOptions::default()).unwrap(),
        ),
    ];
    let result = receipt(&call("fmd.render_book", arguments).unwrap());
    let outputs = array(result.get("outputs").unwrap());
    assert_eq!(outputs.len(), 3);
    for (item, (format, bytes)) in outputs.iter().zip(expected) {
        assert_eq!(item.get("format").and_then(JsonValue::as_str), Some(format));
        assert_eq!(payload(item), base64_encode(&bytes));
    }
}

#[test]
fn strict_refusals_keep_typed_reasons_across_direct_file_and_book_routes() {
    for (source, config, reason, options) in [
        (INFEASIBLE, TIGHT, "pagination_infeasible", tight_options()),
        ("A", TALL, "pagination_budget_exceeded", tall_options()),
    ] {
        let core_error =
            franken_markdown::render_pdf_document(&parse_markdown(source), &options).unwrap_err();
        assert_eq!(core_error.code(), reason);
        assert_refused(call("fmd.render_pdf", args(config, source)), reason);
        let workspace = scratch();
        let input = workspace.join("source.md");
        std::fs::write(&input, source).unwrap();
        assert_refused(
            call("fmd.render_file", file_args(config, &input, "pdf")),
            reason,
        );

        let html_path = workspace.join("publication.html");
        let pdf_path = workspace.join("publication.pdf");
        std::fs::write(&html_path, b"prior HTML").unwrap();
        std::fs::write(&pdf_path, b"prior PDF").unwrap();
        let mut paired = file_args(config, &input, "both");
        paired.insert(
            "out".into(),
            JsonValue::String(pdf_path.to_string_lossy().into_owned()),
        );
        assert_refused(call("fmd.render_file", paired), reason);
        assert_eq!(std::fs::read(html_path).unwrap(), b"prior HTML");
        assert_eq!(std::fs::read(pdf_path).unwrap(), b"prior PDF");
        assert_eq!(std::fs::read_to_string(input).unwrap(), source);
        assert_refused(
            call("fmd.render_book", book_args(config, source, "all")),
            reason,
        );

        let mut best_effort = args(config, source);
        best_effort.insert("strictPagination".into(), JsonValue::Bool(false));
        best_effort.insert("optimalPagination".into(), JsonValue::Bool(true));
        assert!(content(&call("fmd.render_pdf", best_effort).unwrap()).starts_with("JVBERi0"));
    }
}

#[test]
fn archival_direct_and_book_routes_preserve_strict_budget_refusals() {
    let error = franken_markdown::render_pdf_document_pdfa(
        &parse_markdown("A"),
        &tall_options(),
        PdfASettings::a2b_strict(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "pagination_budget_exceeded");
    for (tool, mut arguments) in [
        ("fmd.render_pdf", args(TALL, "A")),
        ("fmd.render_book", book_args(TALL, "A", "pdf")),
        ("fmd.render_book", book_args(TALL, "A", "all")),
    ] {
        arguments.insert("pdfA".into(), JsonValue::String("2b".into()));
        arguments.insert("pdfAStrict".into(), JsonValue::Bool(true));
        assert_refused(call(tool, arguments), "pagination_budget_exceeded");
    }
}

#[test]
fn malformed_controls_fail_before_source_io_or_resource_decoding() {
    let missing = scratch().join("not-created.md");
    let bad_assets = parse_json(r#"[{"destination":"bad.png","base64":"%%%"}]"#).unwrap();
    for (config, code, reason) in [
        (
            r#"{"strictPagination":"true"}"#,
            INVALID_PARAMS,
            "invalid_params",
        ),
        (
            r#"{"strictPagination":null}"#,
            INVALID_PARAMS,
            "invalid_params",
        ),
        (
            r#"{"baselineGridPt":"13"}"#,
            INVALID_PARAMS,
            "invalid_params",
        ),
        (
            r#"{"baselineGridPt":null}"#,
            INVALID_PARAMS,
            "invalid_params",
        ),
        (r#"{"baselineGridPt":-1}"#, INVALID_PARAMS, "invalid_params"),
        (
            r#"{"baselineGridPt":144.001}"#,
            INVALID_PARAMS,
            "invalid_params",
        ),
        (
            r#"{"baselineGridPt":0.5}"#,
            ERROR_INVALID_OPTIONS,
            "invalid_baseline_grid",
        ),
        (
            r#"{"baselineGridPt":13,"fitToPages":1}"#,
            ERROR_INVALID_OPTIONS,
            "baseline_grid_incompatible",
        ),
    ] {
        let mut book = book_args(config, SOURCE, "all");
        book.insert("chapters".into(), JsonValue::Array(Vec::new()));
        for (tool, mut arguments) in [
            ("fmd.render_pdf", args(config, SOURCE)),
            ("fmd.verify", args(config, SOURCE)),
            ("fmd.render_file", file_args(config, &missing, "both")),
            ("fmd.render_book", book),
        ] {
            arguments.insert("images".into(), bad_assets.clone());
            let error = call(tool, arguments).unwrap_err();
            assert_eq!(error.0, code, "{tool}: {error:?}");
            assert_eq!(error.2, reason, "{tool}: {error:?}");
        }
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (tool, mut arguments) in [
            ("fmd.render_pdf", args("{}", SOURCE)),
            ("fmd.verify", args("{}", SOURCE)),
            ("fmd.render_file", file_args("{}", &missing, "pdf")),
            ("fmd.render_book", book_args("{}", SOURCE, "pdf")),
        ] {
            arguments.insert("baselineGridPt".into(), JsonValue::Number(value));
            let error = call(tool, arguments).unwrap_err();
            assert_eq!(error.0, INVALID_PARAMS, "{tool}: {error:?}");
            assert_eq!(error.2, "invalid_params", "{tool}: {error:?}");
        }
    }
}

#[test]
fn non_pdf_targets_reject_even_explicitly_disabled_controls() {
    let missing = scratch().join("not-created.md");
    for (field, value) in [
        ("strictPagination", JsonValue::Bool(false)),
        ("baselineGridPt", JsonValue::Number(0.0)),
    ] {
        let mut html = args("{}", SOURCE);
        html.insert(field.into(), value.clone());
        assert_eq!(
            call("fmd.render_html", html).unwrap_err().2,
            "invalid_params"
        );
        for target in ["html", "epub", "svg"] {
            let mut request = file_args("{}", &missing, target);
            request.insert(field.into(), value.clone());
            let error = call("fmd.render_file", request).unwrap_err();
            assert_eq!(error.0, ERROR_INVALID_OPTIONS);
            assert_eq!(error.2, "unsupported_target_option");
        }
        for target in ["epub", "site"] {
            let mut request = book_args("{}", SOURCE, target);
            request.insert(field.into(), value.clone());
            let error = call("fmd.render_book", request).unwrap_err();
            assert_eq!(error.0, ERROR_INVALID_OPTIONS);
            assert_eq!(error.2, "unsupported_target_option");
        }
    }
}

#[test]
fn verification_honors_grid_and_strict_admission_before_reporting() {
    let expected =
        franken_markdown::verify::verify_pdf_result(&parse_markdown(SOURCE), &grid_options())
            .unwrap();
    assert_eq!(
        content(&call("fmd.verify", args(GRID, SOURCE)).unwrap()),
        franken_markdown::verify::to_json(&expected)
    );
    let mut a11y = args(GRID, SOURCE);
    a11y.insert("a11y".into(), JsonValue::Bool(true));
    assert_eq!(
        content(&call("fmd.verify", a11y).unwrap()),
        franken_markdown::verify::to_json(&franken_markdown::verify::filter_a11y(expected))
    );
    for (source, config, reason, mut options) in [
        (INFEASIBLE, TIGHT, "pagination_infeasible", tight_options()),
        ("A", TALL, "pagination_budget_exceeded", tall_options()),
    ] {
        assert_refused(call("fmd.verify", args(config, source)), reason);
        let mut arguments = args(config, source);
        arguments.insert("strictPagination".into(), JsonValue::Bool(false));
        arguments.insert("optimalPagination".into(), JsonValue::Bool(true));
        options.strict_pagination = false;
        let report =
            franken_markdown::verify::verify_pdf_result(&parse_markdown(source), &options).unwrap();
        assert_eq!(
            content(&call("fmd.verify", arguments).unwrap()),
            franken_markdown::verify::to_json(&report)
        );
    }
}

#[test]
fn discovery_advertises_the_executable_grid_range_and_strict_policy() {
    let listed = tools_list_result();
    for tool in array(listed.get("tools").unwrap()) {
        let name = tool.get("name").and_then(JsonValue::as_str).unwrap();
        let properties = tool.get("inputSchema").unwrap().get("properties").unwrap();
        let accepts_pdf = matches!(
            name,
            "fmd.render_pdf" | "fmd.render_file" | "fmd.render_book" | "fmd.verify"
        );
        if accepts_pdf {
            let strict = properties.get("strictPagination").unwrap();
            assert_eq!(
                strict.get("type").and_then(JsonValue::as_str),
                Some("boolean")
            );
            let grid = properties.get("baselineGridPt").unwrap();
            assert_eq!(grid.get("type").and_then(JsonValue::as_str), Some("number"));
            assert_eq!(grid.get("minimum").and_then(JsonValue::as_f64), Some(0.0));
            assert_eq!(grid.get("maximum").and_then(JsonValue::as_f64), Some(144.0));
            let alternatives = array(grid.get("anyOf").unwrap());
            assert_eq!(alternatives.len(), 2);
            assert_eq!(
                alternatives[0].get("const").and_then(JsonValue::as_f64),
                Some(0.0)
            );
            assert_eq!(
                alternatives[1].get("minimum").and_then(JsonValue::as_f64),
                Some(1.0)
            );
        } else {
            assert!(properties.get("strictPagination").is_none(), "{name}");
            assert!(properties.get("baselineGridPt").is_none(), "{name}");
        }
        if name == "fmd.verify" {
            assert!(properties.get("pdfA").is_none());
            assert!(properties.get("pdfAStrict").is_none());
        }
    }
}

#[test]
fn stdio_preserves_typed_refusals_and_remains_usable_after_errors() {
    let requests = [
        ("fmd.render_pdf", args(TALL, "A")),
        ("fmd.verify", args(TIGHT, INFEASIBLE)),
        ("fmd.render_html", args("{}", "# Still alive")),
        (
            "fmd.render_pdf",
            args(
                r#"{"page":{"margins":{"topPt":0}},"running":{"header":{"left":"Header"}}}"#,
                SOURCE,
            ),
        ),
    ];
    let mut input = String::new();
    for (index, (name, arguments)) in requests.into_iter().enumerate() {
        let request = JsonValue::Object(BTreeMap::from([
            ("jsonrpc".into(), JsonValue::String("2.0".into())),
            ("id".into(), JsonValue::Number((index + 1) as f64)),
            ("method".into(), JsonValue::String("tools/call".into())),
            ("params".into(), params(name, arguments)),
        ]));
        input.push_str(&request.to_json_string());
        input.push('\n');
    }
    let mut output = Vec::new();
    assert_eq!(
        serve_stdio(
            &mut Cursor::new(input.as_bytes()),
            &mut output,
            DEFAULT_MAX_INPUT_BYTES,
            MAX_FRAME_BYTES,
        )
        .unwrap(),
        4
    );
    let output = String::from_utf8(output).unwrap();
    let responses: Vec<_> = output
        .lines()
        .map(|line| parse_json(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 4);
    for (index, response) in responses.iter().enumerate() {
        assert_eq!(
            response.get("id").and_then(JsonValue::as_u64),
            Some((index + 1) as u64)
        );
    }
    for (index, reason) in [
        (0, "pagination_budget_exceeded"),
        (1, "pagination_infeasible"),
        (3, "render_failed"),
    ] {
        let error = responses[index].get("error").unwrap();
        assert_eq!(
            error.get("code").and_then(JsonValue::as_f64),
            Some(f64::from(ERROR_RENDER_FAILED))
        );
        assert_eq!(
            error
                .get("data")
                .unwrap()
                .get("reason")
                .and_then(JsonValue::as_str),
            Some(reason)
        );
    }
    assert!(content(responses[2].get("result").unwrap()).contains("Still alive"));
}
