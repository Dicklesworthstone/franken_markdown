//! Real MCP dispatch and transport for source-only document analysis.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;

use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, ERROR_INPUT_TOO_LARGE, ERROR_INVALID_OPTIONS, INVALID_PARAMS,
    JsonValue as Json, MAX_FRAME_BYTES, handle_tool_call, parse_json, read_frame, serve_stdio,
    tools_list_result, write_frame,
};
use franken_markdown::{Theme, parse_markdown};

type Args = BTreeMap<String, Json>;
type Failure = (i32, String, &'static str);

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
        _ => panic!("expected array: {value:?}"),
    }
}

fn stats_args(source: &str) -> Args {
    BTreeMap::from([("markdown".to_owned(), text(source))])
}

fn diff_args(old: &str, new: &str) -> Args {
    BTreeMap::from([
        ("oldMarkdown".to_owned(), text(old)),
        ("newMarkdown".to_owned(), text(new)),
    ])
}

fn params(name: &str, args: Args) -> Json {
    object([("name", text(name)), ("arguments", Json::Object(args))])
}

fn call_limit(name: &str, args: Args, limit: u64) -> Result<Json, Failure> {
    handle_tool_call(Some(&params(name, args)), limit)
}

fn call(name: &str, args: Args) -> Json {
    call_limit(name, args, DEFAULT_MAX_INPUT_BYTES).unwrap()
}

fn content(result: &Json) -> &str {
    assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
    let content = array(result.get("content").unwrap());
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].get("type"), Some(&text("text")));
    content[0].get("text").and_then(Json::as_str).unwrap()
}

fn report(result: &Json) -> Json {
    parse_json(content(result)).unwrap()
}

fn changes(report: &Json) -> &[Json] {
    assert_eq!(report.get("schema"), Some(&text("fmd-diff-changes-v1")));
    array(report.get("changes").unwrap())
}

#[test]
fn discovery_advertises_exact_source_fields_profiles_and_output_formats() {
    let listing = tools_list_result();
    let tools = array(listing.get("tools").unwrap());
    for (name, fields, required) in [
        (
            "fmd.stats",
            &["markdown", "profile"][..],
            &["markdown"][..],
        ),
        (
            "fmd.diff",
            &["oldMarkdown", "newMarkdown", "profile", "format"][..],
            &["oldMarkdown", "newMarkdown"][..],
        ),
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Json::as_str) == Some(name))
            .unwrap();
        let schema = tool.get("inputSchema").unwrap();
        assert_eq!(schema.get("additionalProperties"), Some(&Json::Bool(false)));
        assert_eq!(
            array(schema.get("required").unwrap()),
            required.iter().map(|field| text(field)).collect::<Vec<_>>()
        );
        let properties = schema.get("properties").unwrap().as_object().unwrap();
        assert_eq!(
            properties.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            fields.iter().copied().collect::<BTreeSet<_>>()
        );
        for property in properties.values() {
            assert_eq!(property.get("type"), Some(&text("string")));
        }
        assert_eq!(
            array(properties["profile"].get("enum").unwrap()),
            [text("commonmark-gfm"), text("gfm-plus")]
        );
        if name == "fmd.diff" {
            assert_eq!(
                array(properties["format"].get("enum").unwrap()),
                [text("json"), text("html")]
            );
        }
    }
}

#[test]
fn stats_returns_native_intelligence_and_original_utf8_counts() {
    let source = "# Overview β\r\n\r\nA [broken link](#absent).\r\n\r\n- [x] Done\r\n";
    let result = call("fmd.stats", stats_args(source));
    assert_eq!(result, call("fmd.stats", stats_args(source)));
    let expected =
        franken_markdown::doc_stats::compute_doc_stats(source, &parse_markdown(source));
    assert_eq!(content(&result), expected.to_json());
    let report = report(&result);
    assert_eq!(report.get("schema"), Some(&text("fmd-document-stats-v1")));
    assert_eq!(report.get("bytes").and_then(Json::as_u64), Some(source.len() as u64));
    let structure = report.get("structure").unwrap();
    assert_eq!(structure.get("headings_total").and_then(Json::as_u64), Some(1));
    assert_eq!(structure.get("task_items_completed").and_then(Json::as_u64), Some(1));
    assert_eq!(array(report.get("outline").unwrap())[0].get("text"), Some(&text("Overview β")));
    assert!(array(report.get("findings").unwrap()).iter().any(|finding| {
        finding.get("code").and_then(Json::as_str) == Some("broken_internal_anchor")
    }));
}

#[test]
fn stats_selects_grammar_before_cross_line_links_and_preserves_legacy_comments() {
    let source = "[Cross-line β\r\n: link](#missing)\r\n";
    for (profile, paragraphs, links) in [
        (None, 0, 0),
        (Some("commonmark-gfm"), 1, 1),
        (Some("gfm-plus"), 0, 0),
    ] {
        let mut args = stats_args(source);
        if let Some(profile) = profile {
            args.insert("profile".to_owned(), text(profile));
        }
        let report = report(&call("fmd.stats", args));
        let structure = report.get("structure").unwrap();
        assert_eq!(structure.get("paragraphs").and_then(Json::as_u64), Some(paragraphs));
        assert_eq!(structure.get("links_total").and_then(Json::as_u64), Some(links));
        assert_eq!(report.get("bytes").and_then(Json::as_u64), Some(source.len() as u64));
    }
    let source = "<!-- pagebreak -->\n";
    let legacy = call("fmd.stats", stats_args(source));
    assert_eq!(
        content(&legacy),
        franken_markdown::doc_stats::compute_doc_stats(source, &parse_markdown(source)).to_json()
    );
    for profile in ["commonmark-gfm", "gfm-plus"] {
        let mut args = stats_args(source);
        args.insert("profile".to_owned(), text(profile));
        let result = call("fmd.stats", args);
        if profile == "commonmark-gfm" {
            assert_eq!(result, legacy);
        } else {
            assert_eq!(report(&result).get("characters").and_then(Json::as_u64), Some(0));
            assert!(report(&legacy).get("characters").and_then(Json::as_u64).unwrap() > 0);
        }
    }
}

#[test]
fn diff_returns_typed_changes_and_original_block_indices_instead_of_only_counts() {
    for (old, new, kind) in [
        ("# Old\n", "## New\n", "heading_changed"),
        ("[same](first)\n", "[same](second)\n", "link_target_changed"),
        ("- one\n- two\n", "- one\n- three\n", "list_changed"),
        ("| A |\n|---|\n| old |\n", "| A |\n|---|\n| new |\n", "table_changed"),
        ("```rust\nold\n```\n", "```python\nnew\n```\n", "code_block_changed"),
    ] {
        let result = call("fmd.diff", diff_args(old, new));
        assert_eq!(result, call("fmd.diff", diff_args(old, new)));
        let report = report(&result);
        assert_eq!(report.get("identical"), Some(&Json::Bool(false)));
        let changes = changes(&report);
        assert_eq!(changes.len(), 1, "{report:?}");
        let change = &changes[0];
        assert_eq!(change.get("kind"), Some(&text(kind)));
        assert_eq!(change.get("old_index").and_then(Json::as_u64), Some(0));
        assert_eq!(change.get("new_index").and_then(Json::as_u64), Some(0));
        assert!(!change.get("summary").and_then(Json::as_str).unwrap().is_empty());
        match kind {
            "heading_changed" => assert_eq!(change.get("level").and_then(Json::as_u64), Some(2)),
            "link_target_changed" => {
                assert_eq!(change.get("old"), Some(&text("first")));
                assert_eq!(change.get("new"), Some(&text("second")));
            }
            "code_block_changed" => assert_eq!(change.get("lang"), Some(&text("python"))),
            _ => {}
        }
    }
    let report = report(&call("fmd.diff", diff_args("# Keep\n", "# Keep\n\n## Added\n")));
    let changes = changes(&report);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].get("kind"), Some(&text("heading_added")));
    assert_eq!(changes[0].get("old_index"), Some(&Json::Null));
    assert_eq!(changes[0].get("new_index").and_then(Json::as_u64), Some(1));
}

#[test]
fn diff_retains_semantic_whitespace_and_move_behavior() {
    let old = "Alpha words.\n\nBeta words.\n\nGamma words.\n";
    let new = "Beta words.\n\nAlpha words.\n\nGamma words.\n";
    let result = call("fmd.diff", diff_args(old, new));
    assert_eq!(
        content(&result),
        franken_markdown::diff::report_json(&franken_markdown::diff::diff_documents(
            &parse_markdown(old),
            &parse_markdown(new),
        ))
    );
    assert!(changes(&report(&result)).iter().any(|change| {
        change.get("kind").and_then(Json::as_str) == Some("block_moved")
    }));
    let mut args = diff_args("One  word.\n", "One word.\n");
    args.insert("format".to_owned(), text("json"));
    let report = report(&call("fmd.diff", args));
    assert_eq!(report.get("identical"), Some(&Json::Bool(true)));
    assert!(changes(&report).is_empty());
}

#[test]
fn diff_uses_one_grammar_for_both_revisions_and_page_break_transforms() {
    let old = "*Term\n: old value*\n";
    let new = "*Term\n: new value*\n";
    for (profile, kind) in [("commonmark-gfm", "paragraph_edited"), ("gfm-plus", "list_changed")] {
        let mut args = diff_args(old, new);
        args.insert("profile".to_owned(), text(profile));
        let report = report(&call("fmd.diff", args.clone()));
        assert_eq!(changes(&report).len(), 1);
        assert_eq!(changes(&report)[0].get("kind"), Some(&text(kind)));
        args.insert("format".to_owned(), text("html"));
        let result = call("fmd.diff", args);
        let html = content(&result);
        assert_eq!(html.contains("<dl>"), profile == "gfm-plus");
        if profile == "commonmark-gfm" {
            assert!(html.contains("<em>Term\n: old value</em>"));
            assert!(html.contains("<em>Term\n: new value</em>"));
        }
    }
    let old = "<!-- pagebreak -->\n";
    let new = "  <!-- pagebreak -->\t\n";
    let legacy = call("fmd.diff", diff_args(old, new));
    assert_eq!(report(&legacy).get("identical"), Some(&Json::Bool(false)));
    for profile in ["commonmark-gfm", "gfm-plus"] {
        let mut args = diff_args(old, new);
        args.insert("profile".to_owned(), text(profile));
        let result = call("fmd.diff", args);
        if profile == "commonmark-gfm" {
            assert_eq!(result, legacy);
        } else {
            let report = report(&result);
            assert_eq!(report.get("identical"), Some(&Json::Bool(true)));
            assert!(changes(&report).is_empty());
        }
    }
}

#[test]
fn html_review_keeps_safe_full_revisions_anchors_and_footnotes() {
    let old = "# Review\n\n[Local](#review) and note.[^a]\n\n[^a]: Old note.\n\n<details><summary>More</summary>Safe markup.</details>\n\n<script>hostile()</script>\n";
    let new = old.replace("Old note.", "New note.");
    let mut args = diff_args(old, &new);
    args.insert("format".to_owned(), text("html"));
    let result = call("fmd.diff", args);
    let html = content(&result);
    assert_eq!(
        html,
        franken_markdown::diff::compute_diff(
            &parse_markdown(old),
            &parse_markdown(&new),
            "Before",
            "Current",
        ).to_html(&Theme::default())
    );
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("Old note.") && html.contains("New note."));
    assert!(html.contains("<details>") && html.contains("<summary>More</summary>"));
    assert!(!html.contains("<script>hostile()</script>"));
    assert!(html.contains("&lt;script&gt;hostile()&lt;/script&gt;"));
    for version in ["old", "new"] {
        for id in ["review", "fn-a"] {
            assert!(html.contains(&format!("id=\"diff-{version}-{id}\"")));
            assert!(html.contains(&format!("href=\"#diff-{version}-{id}\"")));
        }
    }
}

#[test]
fn admission_rejects_unknown_fields_types_profiles_and_formats() {
    for (name, base) in [("fmd.stats", stats_args("x")), ("fmd.diff", diff_args("x", "y"))] {
        for field in ["path", "out", "images", "fonts", "expandIncludes", "allowRawHtml"] {
            let mut args = base.clone();
            args.insert(field.to_owned(), Json::Bool(true));
            let error = call_limit(name, args, 0).unwrap_err();
            assert_eq!((error.0, error.2), (INVALID_PARAMS, "invalid_params"));
        }
        for invalid in ["unknown", "gfm", "GFM-Plus", " commonmark-gfm"] {
            let mut args = base.clone();
            args.insert("profile".to_owned(), text(invalid));
            let error = call_limit(name, args, 0).unwrap_err();
            assert_eq!((error.0, error.2), (ERROR_INVALID_OPTIONS, "invalid_profile"));
        }
        let mut args = base;
        args.insert("profile".to_owned(), Json::Null);
        let error = call_limit(name, args, 0).unwrap_err();
        assert_eq!((error.0, error.2), (INVALID_PARAMS, "invalid_params"));
    }
    for (field, reason) in [("oldMarkdown", "missing_old_markdown"), ("newMarkdown", "missing_new_markdown")] {
        let mut args = diff_args("x", "y");
        args.remove(field);
        let error = call_limit("fmd.diff", args, 0).unwrap_err();
        assert_eq!((error.0, error.2), (ERROR_INVALID_OPTIONS, reason));
        let mut args = diff_args("x", "y");
        args.insert(field.to_owned(), Json::Number(0.0));
        let error = call_limit("fmd.diff", args, 0).unwrap_err();
        assert_eq!((error.0, error.2), (INVALID_PARAMS, "invalid_params"));
    }
    for (format, code, reason) in [
        (text("JSON"), ERROR_INVALID_OPTIONS, "invalid_diff_format"),
        (text("pdf"), ERROR_INVALID_OPTIONS, "invalid_diff_format"),
        (Json::Bool(false), INVALID_PARAMS, "invalid_params"),
    ] {
        let mut args = diff_args("x", "y");
        args.insert("format".to_owned(), format);
        let error = call_limit("fmd.diff", args, 0).unwrap_err();
        assert_eq!((error.0, error.2), (code, reason));
    }
}

#[test]
fn source_budgets_count_combined_utf8_bytes_and_admit_empty_documents() {
    assert!(call_limit("fmd.stats", stats_args("β"), 2).is_ok());
    let error = call_limit("fmd.stats", stats_args("β"), 1).unwrap_err();
    assert_eq!((error.0, error.2), (ERROR_INPUT_TOO_LARGE, "input_too_large"));
    assert!(call_limit("fmd.stats", stats_args(""), 0).is_ok());
    for format in ["json", "html"] {
        let mut args = diff_args("β", "😀");
        args.insert("format".to_owned(), text(format));
        // Each revision fits five bytes; together they require six.
        let error = call_limit("fmd.diff", args.clone(), 5).unwrap_err();
        assert_eq!((error.0, error.2), (ERROR_INPUT_TOO_LARGE, "input_too_large"));
        assert!(call_limit("fmd.diff", args, 6).is_ok());
        for (old, new) in [("β😀", ""), ("", "β😀")] {
            let mut args = diff_args(old, new);
            args.insert("format".to_owned(), text(format));
            assert_eq!(call_limit("fmd.diff", args, 5).unwrap_err().0, ERROR_INPUT_TOO_LARGE);
        }
    }
    let result = call_limit("fmd.diff", diff_args("", ""), 0).unwrap();
    let report = report(&result);
    assert_eq!(report.get("identical"), Some(&Json::Bool(true)));
    assert!(changes(&report).is_empty());
}

fn tool_request(id: u32, name: &str, args: Args) -> Json {
    object([
        ("jsonrpc", text("2.0")),
        ("id", Json::Number(f64::from(id))),
        ("method", text("tools/call")),
        ("params", params(name, args)),
    ])
}

#[test]
fn actual_json_lines_and_framed_sessions_recover_after_analysis_errors() {
    let mut invalid_profile = stats_args("text");
    invalid_profile.insert("profile".to_owned(), text("unknown"));
    let mut html = diff_args("a", "b");
    html.insert("format".to_owned(), text("html"));
    let messages = [
        object([("jsonrpc", text("2.0")), ("id", Json::Number(1.0)), ("method", text("initialize"))]),
        tool_request(2, "fmd.stats", invalid_profile),
        tool_request(3, "fmd.diff", diff_args("β", "😀")),
        tool_request(4, "fmd.stats", stats_args("Hi.")),
        tool_request(5, "fmd.diff", diff_args("a", "b")),
        tool_request(6, "fmd.diff", html),
        object([("jsonrpc", text("2.0")), ("method", text("tools/call")), ("params", params("fmd.diff", diff_args("a", "b")))]),
    ];
    for framed in [false, true] {
        let mut input = Vec::new();
        for message in &messages {
            let message = message.to_json_string();
            if framed {
                write_frame(&mut input, &message).unwrap();
            } else {
                input.extend_from_slice(message.as_bytes());
                input.push(b'\n');
            }
        }
        let mut output = Vec::new();
        let count = serve_stdio(&mut Cursor::new(input), &mut output, 5, MAX_FRAME_BYTES).unwrap();
        assert_eq!(count, messages.len());
        assert_eq!(std::str::from_utf8(&output).unwrap().starts_with("Content-Length:"), framed);
        let mut cursor = Cursor::new(output);
        let mut replies = Vec::new();
        while let Some(frame) = read_frame(&mut cursor, MAX_FRAME_BYTES).unwrap() {
            replies.push(parse_json(&frame).unwrap());
        }
        assert_eq!(replies.len(), 6, "notifications must not produce a reply");
        for (index, reply) in replies.iter().enumerate() {
            assert_eq!(reply.get("id").and_then(Json::as_u64), Some(index as u64 + 1));
        }
        for (reply, code, reason) in [
            (&replies[1], ERROR_INVALID_OPTIONS, "invalid_profile"),
            (&replies[2], ERROR_INPUT_TOO_LARGE, "input_too_large"),
        ] {
            let error = reply.get("error").unwrap();
            assert_eq!(error.get("code").and_then(Json::as_f64), Some(f64::from(code)));
            assert_eq!(error.get("data").unwrap().get("reason"), Some(&text(reason)));
        }
        assert_eq!(report(replies[3].get("result").unwrap()).get("words").and_then(Json::as_u64), Some(1));
        assert_eq!(changes(&report(replies[4].get("result").unwrap())).len(), 1);
        assert!(content(replies[5].get("result").unwrap()).starts_with("<!DOCTYPE html>"));
    }
}
