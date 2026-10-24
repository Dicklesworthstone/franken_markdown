//! Shared book navigation and strict publishing through the real MCP boundary.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, JsonValue as Json, MAX_FRAME_BYTES, handle_tool_call, parse_json,
    read_frame, serve_stdio, tools_list_result, write_frame,
};

type Failure = (i32, String, &'static str);
fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}
fn text(value: &str) -> Json {
    Json::String(value.to_string())
}
fn source(path: &str, markdown: &str) -> Json {
    object([("path", text(path)), ("markdown", text(markdown))])
}
fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(items) => items,
        _ => panic!("expected array"),
    }
}
fn set(args: &mut Json, key: &str, value: Json) {
    if let Json::Object(fields) = args {
        fields.insert(key.to_string(), value);
    } else {
        panic!("expected object");
    }
}
fn args(markdown: &str) -> Json {
    object([("chapters", Json::Array(vec![source("a.md", markdown)]))])
}
fn call(name: &str, args: Json, limit: u64) -> Result<Json, Failure> {
    handle_tool_call(
        Some(&object([("name", text(name)), ("arguments", args)])),
        limit,
    )
}
fn content(result: &Json) -> &str {
    assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
    array(result.get("content").unwrap())[0]
        .get("text")
        .and_then(Json::as_str)
        .unwrap()
}
fn verify(args: Json) -> Json {
    parse_json(content(
        &call("fmd.verify_book", args, DEFAULT_MAX_INPUT_BYTES).unwrap(),
    ))
    .unwrap()
}
fn count(report: &Json, name: &str) -> u64 {
    report
        .get("summary")
        .unwrap()
        .get(name)
        .and_then(Json::as_u64)
        .unwrap()
}
fn codes(report: &Json) -> Vec<&str> {
    array(report.get("chapters").unwrap())
        .iter()
        .flat_map(|chapter| array(chapter.get("findings").unwrap()))
        .map(|finding| finding.get("code").and_then(Json::as_str).unwrap())
        .collect()
}
fn strict(mut args: Json, to: &str) -> Json {
    set(&mut args, "strictLinks", Json::Bool(true));
    set(&mut args, "to", text(to));
    args
}

#[test]
fn verification_returns_the_exact_shared_report_after_include_expansion() {
    let inputs = [
        BookInput {
            path: "guide/intro.md".into(),
            source: "# Intro\n\n{{#include ../part.md}}\n".into(),
        },
        BookInput {
            path: "next.md".into(),
            source: "# Next\n\n[home](guide/intro.md#intro)".into(),
        },
    ];
    let includes = [BookInput {
        path: "part.md".into(),
        source: "[forward](../next.md#next) [bad](../next.md#absent)".into(),
    }];
    let renderer = BookRenderer::from_sources(&inputs, &includes).unwrap();
    let expected = renderer.validate_links().unwrap().to_json().unwrap();
    let request = object([
        (
            "chapters",
            Json::Array(inputs.iter().map(|s| source(&s.path, &s.source)).collect()),
        ),
        (
            "includeSources",
            Json::Array(
                includes
                    .iter()
                    .map(|s| source(&s.path, &s.source))
                    .collect(),
            ),
        ),
    ]);
    let result = call("fmd.verify_book", request, DEFAULT_MAX_INPUT_BYTES).unwrap();
    assert_eq!(content(&result), expected);
    let report = parse_json(content(&result)).unwrap();
    assert_eq!(count(&report, "chapters"), 2);
    assert_eq!(count(&report, "findings"), 1);
    assert_eq!(codes(&report), ["missing_anchor"]);
    assert_eq!(
        report.get("scope").and_then(Json::as_str),
        Some("expanded-html-navigation")
    );
}

#[test]
fn a_broken_book_is_successful_report_data_not_a_false_clean_report() {
    let request = args("# Top\n\n[x](missing.md#x) [y](#absent) [bad](#%zz)");
    let report = verify(request);
    assert_eq!(count(&report, "findings"), 3);
    assert_eq!(
        codes(&report),
        ["missing_chapter", "missing_anchor", "invalid_fragment"]
    );
    assert!(report.get("outputs").is_none());
}

#[test]
fn external_urls_and_nonchapter_downloads_remain_explicitly_unchecked() {
    let request = args(
        "# Top\n\n[x](https://example.invalid/#absent) [y](//example.invalid/a.md) [download](data.csv) ![image](missing.png)",
    );
    let report = verify(request.clone());
    assert_eq!(count(&report, "external"), 2);
    assert_eq!(count(&report, "unchecked"), 1);
    assert_eq!(count(&report, "findings"), 0);
    assert!(
        call(
            "fmd.render_book",
            strict(request, "epub"),
            DEFAULT_MAX_INPUT_BYTES
        )
        .is_ok()
    );
}

#[test]
fn strict_links_stop_every_output_format_before_any_artifact_is_returned() {
    for to in ["pdf", "epub", "site", "all"] {
        let error = call(
            "fmd.render_book",
            strict(args("# Top\n\n[x](#missing)"), to),
            DEFAULT_MAX_INPUT_BYTES,
        )
        .unwrap_err();
        assert_eq!(error.2, "book_links_failed", "{to}");
        assert!(error.1.contains("missing_anchor"));
        assert!(error.1.contains("fmd.verify_book"));
    }
}

#[test]
fn strict_navigation_admission_precedes_resource_decoding_and_rendering() {
    let mut request = strict(args("# Top\n\n[x](#missing)"), "all");
    set(
        &mut request,
        "images",
        parse_json(r#"[{"destination":"x.png","base64":"?"}]"#).unwrap(),
    );
    set(&mut request, "maxOutputBytes", Json::Number(1.0));
    assert_eq!(
        call("fmd.render_book", request, DEFAULT_MAX_INPUT_BYTES)
            .unwrap_err()
            .2,
        "book_links_failed"
    );
}

#[test]
fn valid_strict_requests_preserve_artifact_bytes_and_receipts() {
    let input = args("# Top\n\n[ok](#top)");
    for to in ["pdf", "epub", "site", "all"] {
        let mut ordinary = input.clone();
        set(&mut ordinary, "to", text(to));
        let expected = call("fmd.render_book", ordinary, DEFAULT_MAX_INPUT_BYTES).unwrap();
        let actual = call(
            "fmd.render_book",
            strict(input.clone(), to),
            DEFAULT_MAX_INPUT_BYTES,
        )
        .unwrap();
        assert_eq!(actual, expected, "{to}");
    }
}

#[test]
fn strict_false_retains_non_strict_publishing_behavior() {
    let mut ordinary = args("# Top\n\n[x](#missing)");
    set(&mut ordinary, "to", text("epub"));
    let expected = call("fmd.render_book", ordinary.clone(), DEFAULT_MAX_INPUT_BYTES).unwrap();
    set(&mut ordinary, "strictLinks", Json::Bool(false));
    assert_eq!(
        call("fmd.render_book", ordinary, DEFAULT_MAX_INPUT_BYTES).unwrap(),
        expected
    );
}

#[test]
fn clean_unicode_nested_and_reference_style_links_use_publication_ids() {
    let report = verify(args(
        "# Café\r\n\r\n> ## Nested\r\n> [accent][c]\r\n\r\n- [nested](#nested)\r\n\r\n[c]: #caf%C3%A9\r\n",
    ));
    assert_eq!(count(&report, "findings"), 0);
    assert_eq!(count(&report, "checked"), 2);
}

#[test]
fn published_footnote_collisions_fail_but_unpublished_examples_do_not() {
    let report = verify(args(
        "# fn-note\n\n[^note] [ambiguous](#fn-note)\n\n[^note]: Visible.\n\n[^unused]: [hidden](#missing)\n",
    ));
    assert_eq!(codes(&report), ["ambiguous_anchor"]);
    let report = verify(args(
        "# Top\n\n`[not a link](#missing)`\n\n```md\n[example](#missing)\n```\n\n[^unused]: [hidden](#missing)",
    ));
    assert_eq!(count(&report, "findings"), 0);
}

#[test]
fn verification_accepts_only_explicit_source_fields() {
    for (key, value) in [
        ("images", Json::Array(Vec::new())),
        ("fonts", Json::Array(Vec::new())),
        ("out", text("/not-written/book.pdf")),
        ("to", text("pdf")),
        ("pageNumbers", Json::Bool(false)),
        ("strictLinks", Json::Bool(false)),
    ] {
        let mut request = args("# Top");
        set(&mut request, key, value);
        assert_eq!(
            call("fmd.verify_book", request, DEFAULT_MAX_INPUT_BYTES)
                .unwrap_err()
                .2,
            "invalid_params",
            "{key}"
        );
    }
    assert_eq!(
        call("fmd.verify_book", object([]), DEFAULT_MAX_INPUT_BYTES)
            .unwrap_err()
            .2,
        "missing_chapters"
    );
    let mut request = args("# Top");
    set(&mut request, "strictLinks", text("true"));
    assert_eq!(
        call("fmd.render_book", request, DEFAULT_MAX_INPUT_BYTES)
            .unwrap_err()
            .2,
        "invalid_params"
    );
}

#[test]
fn verifier_reuses_source_limits_and_fails_on_missing_includes() {
    let request = args("# 😀");
    let limit = ("a.md".len() + "# 😀".len()) as u64;
    assert_eq!(
        call("fmd.verify_book", request.clone(), limit - 1)
            .unwrap_err()
            .2,
        "input_too_large"
    );
    assert!(call("fmd.verify_book", request, limit).is_ok());
    assert_eq!(
        call(
            "fmd.verify_book",
            args("{{#include absent.md}}"),
            DEFAULT_MAX_INPUT_BYTES
        )
        .unwrap_err()
        .2,
        "invalid_book_sources"
    );
}

#[test]
fn excessive_findings_never_become_a_partial_success_or_strict_pass() {
    let request = args(&"[bad](#missing) ".repeat(4097));
    assert_eq!(
        call("fmd.verify_book", request.clone(), DEFAULT_MAX_INPUT_BYTES)
            .unwrap_err()
            .2,
        "book_link_analysis_failed"
    );
    assert_eq!(
        call(
            "fmd.render_book",
            strict(request, "epub"),
            DEFAULT_MAX_INPUT_BYTES
        )
        .unwrap_err()
        .2,
        "book_link_analysis_failed"
    );
}

#[test]
fn repeated_requests_cannot_reuse_a_previous_books_successful_check() {
    assert_eq!(count(&verify(args("# Top\n\n[x](#top)")), "findings"), 0);
    assert_eq!(
        count(&verify(args("# Renamed\n\n[x](#top)")), "findings"),
        1
    );
    assert_eq!(count(&verify(args("# Top\n\n[x](#top)")), "findings"), 0);
}

#[test]
fn discovery_keeps_verification_narrow_and_advertises_strict_publication() {
    let list = tools_list_result();
    let tools = array(list.get("tools").unwrap());
    let lookup = |name| {
        tools
            .iter()
            .find(|tool| tool.get("name").and_then(Json::as_str) == Some(name))
            .unwrap()
    };
    let schema = lookup("fmd.verify_book").get("inputSchema").unwrap();
    let fields = schema.get("properties").and_then(Json::as_object).unwrap();
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        ["chapters", "includeSources"]
    );
    assert_eq!(schema.get("additionalProperties"), Some(&Json::Bool(false)));
    assert_eq!(array(schema.get("required").unwrap()), [text("chapters")]);
    assert_eq!(
        fields["chapters"]
            .get("items")
            .unwrap()
            .get("additionalProperties"),
        Some(&Json::Bool(false))
    );
    let strict = lookup("fmd.render_book")
        .get("inputSchema")
        .unwrap()
        .get("properties")
        .unwrap()
        .get("strictLinks")
        .unwrap();
    assert_eq!(strict.get("type").and_then(Json::as_str), Some("boolean"));
}

fn framed(messages: &[Json]) -> Vec<Json> {
    let mut input = Vec::new();
    for message in messages {
        write_frame(&mut input, &message.to_json_string()).unwrap();
    }
    let mut output = Vec::new();
    assert_eq!(
        serve_stdio(
            &mut Cursor::new(input),
            &mut output,
            DEFAULT_MAX_INPUT_BYTES,
            MAX_FRAME_BYTES
        )
        .unwrap(),
        messages.len()
    );
    let mut reader = Cursor::new(output);
    let mut replies = Vec::new();
    while let Some(frame) = read_frame(&mut reader, MAX_FRAME_BYTES).unwrap() {
        replies.push(parse_json(&frame).unwrap());
    }
    replies
}
fn request(id: u32, name: &str, args: Json) -> Json {
    object([
        ("jsonrpc", text("2.0")),
        ("id", Json::Number(f64::from(id))),
        ("method", text("tools/call")),
        (
            "params",
            object([("name", text(name)), ("arguments", args)]),
        ),
    ])
}

#[test]
fn framed_connection_reports_strict_failure_then_accepts_repaired_publication() {
    let replies = framed(&[
        request(
            1,
            "fmd.render_book",
            strict(args("# Top\n\n[x](#missing)"), "epub"),
        ),
        request(2, "fmd.verify_book", args("# Top\n\n[x](#missing)")),
        request(
            3,
            "fmd.render_book",
            strict(args("# Top\n\n[x](#top)"), "epub"),
        ),
    ]);
    assert_eq!(replies.len(), 3);
    assert_eq!(
        replies[0]
            .get("error")
            .unwrap()
            .get("data")
            .unwrap()
            .get("reason")
            .and_then(Json::as_str),
        Some("book_links_failed")
    );
    assert!(replies[0].get("result").is_none());
    let report = parse_json(content(replies[1].get("result").unwrap())).unwrap();
    assert_eq!(count(&report, "findings"), 1);
    let published = parse_json(content(replies[2].get("result").unwrap())).unwrap();
    assert_eq!(array(published.get("outputs").unwrap()).len(), 1);
    for (index, reply) in replies.iter().enumerate() {
        assert_eq!(
            reply.get("id").and_then(Json::as_u64),
            Some(index as u64 + 1)
        );
    }
}

#[test]
fn notifications_do_not_execute_or_reply_to_book_tool_calls() {
    let mut notification = request(1, "fmd.verify_book", args("{{#include absent.md}}"));
    if let Json::Object(fields) = &mut notification {
        fields.remove("id");
    }
    let replies = framed(&[
        notification,
        object([
            ("jsonrpc", text("2.0")),
            ("id", Json::Number(2.0)),
            ("method", text("ping")),
        ]),
    ]);
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].get("id").and_then(Json::as_u64), Some(2));
    assert!(replies[0].get("error").is_none());
}
