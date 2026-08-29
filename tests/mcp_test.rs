//! Integration tests for the Model Context Protocol (MCP) server (beads yjmy / jqhm / uxhr).

#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::io::Cursor;

use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, ERROR_INVALID_OPTIONS, JsonValue, METHOD_NOT_FOUND, handle_tool_call,
    parse_json, parse_jsonrpc_request, read_frame, write_frame,
};

#[test]
fn mcp_handshake_and_ping() {
    let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-client","version":"1.0"}}}"#;
    let req = parse_jsonrpc_request(init_req).expect("parse init req");
    assert_eq!(req.method, "initialize");
    assert_eq!(req.id.and_then(|i| i.as_u64()), Some(1));

    let init_notify = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let notif = parse_jsonrpc_request(init_notify).expect("parse notif");
    assert_eq!(notif.method, "notifications/initialized");
    assert!(notif.id.is_none());

    let ping_req = r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#;
    let ping = parse_jsonrpc_request(ping_req).expect("parse ping");
    assert_eq!(ping.method, "ping");
    assert_eq!(ping.id.and_then(|i| i.as_u64()), Some(2));
}

#[test]
fn mcp_tools_list_schema_contract() {
    let list_req = r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#;
    let req = parse_jsonrpc_request(list_req).expect("parse tools/list");
    assert_eq!(req.method, "tools/list");

    let tools_res = franken_markdown::mcp::tools_list_result();
    let tools = tools_res
        .get("tools")
        .and_then(|t| match t {
            JsonValue::Array(a) => Some(a),
            _ => None,
        })
        .expect("tools array");

    assert_eq!(tools.len(), 5);
    for tool in tools {
        assert!(tool.get("name").is_some());
        assert!(tool.get("description").is_some());
        assert!(tool.get("inputSchema").is_some());
    }
}

#[test]
fn mcp_tool_render_html_options() {
    let mut args = BTreeMap::new();
    args.insert(
        "markdown".to_string(),
        JsonValue::String("# Header\n\nParagraph with **bold** text.".to_string()),
    );
    args.insert("font".to_string(), JsonValue::String("serif".to_string()));
    args.insert("toc".to_string(), JsonValue::Bool(true));
    args.insert(
        "title".to_string(),
        JsonValue::String("Test Document".to_string()),
    );

    let mut params = BTreeMap::new();
    params.insert(
        "name".to_string(),
        JsonValue::String("fmd.render_html".to_string()),
    );
    params.insert("arguments".to_string(), JsonValue::Object(args));

    let res = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
        .expect("render_html");
    let content = res.get("content").expect("content");
    let html = match content {
        JsonValue::Array(arr) => arr[0].get("text").and_then(|t| t.as_str()).expect("text"),
        _ => panic!("expected array"),
    };

    assert!(html.contains("<title>Test Document</title>"));
    assert!(html.contains("Header"));
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains("class=\"fmd-toc\"") || html.contains("nav"));
}

#[test]
fn mcp_tool_render_pdf_parity() {
    let markdown = "# Testing PDF Generation\n\nHigh quality typesetting via MCP.";
    let mut args = BTreeMap::new();
    args.insert(
        "markdown".to_string(),
        JsonValue::String(markdown.to_string()),
    );
    args.insert("pageNumbers".to_string(), JsonValue::Bool(true));

    let mut params = BTreeMap::new();
    params.insert(
        "name".to_string(),
        JsonValue::String("fmd.render_pdf".to_string()),
    );
    params.insert("arguments".to_string(), JsonValue::Object(args));

    let res = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
        .expect("render_pdf");
    let content = res.get("content").expect("content");
    let b64 = match content {
        JsonValue::Array(arr) => arr[0].get("text").and_then(|t| t.as_str()).expect("text"),
        _ => panic!("expected array"),
    };

    assert!(!b64.is_empty());
    // Directly verify PDF output from renderer
    let mut opts = franken_markdown::PdfOptions::default();
    opts.page_numbers = true;
    let expected_pdf =
        franken_markdown::render_pdf(markdown, &opts).expect("direct render");
    let expected_b64 = franken_markdown::mcp::base64_encode(&expected_pdf);
    assert_eq!(
        b64, expected_b64,
        "MCP PDF render must match direct library render bytes"
    );
}

#[test]
fn mcp_tool_verify() {
    let mut args = BTreeMap::new();
    args.insert(
        "markdown".to_string(),
        JsonValue::String("# Heading 1\n### Heading 3 (skipped h2)\n".to_string()),
    );
    args.insert("a11y".to_string(), JsonValue::Bool(true));

    let mut params = BTreeMap::new();
    params.insert(
        "name".to_string(),
        JsonValue::String("fmd.verify".to_string()),
    );
    params.insert("arguments".to_string(), JsonValue::Object(args));

    let res = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
        .expect("verify");
    let content = res.get("content").expect("content");
    let text = match content {
        JsonValue::Array(arr) => arr[0].get("text").and_then(|t| t.as_str()).expect("text"),
        _ => panic!("expected array"),
    };
    let json_report = parse_json(text).expect("valid JSON verify output");
    assert!(json_report.get("findings").is_some());
}

#[test]
fn mcp_tool_capabilities() {
    let mut params = BTreeMap::new();
    params.insert(
        "name".to_string(),
        JsonValue::String("fmd.capabilities".to_string()),
    );
    params.insert("arguments".to_string(), JsonValue::Object(BTreeMap::new()));

    let res = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
        .expect("capabilities");
    let content = res.get("content").expect("content");
    let text = match content {
        JsonValue::Array(arr) => arr[0].get("text").and_then(|t| t.as_str()).expect("text"),
        _ => panic!("expected array"),
    };
    let json_cap = parse_json(text).expect("valid JSON capabilities");
    assert_eq!(json_cap.get("tool").and_then(|t| t.as_str()), Some("fmd"));
    assert_eq!(
        json_cap.get("contract_version").and_then(|c| c.as_str()),
        Some("0.1.0")
    );
}

#[test]
fn mcp_error_taxonomy_on_invalid_options() {
    let mut args = BTreeMap::new();
    args.insert(
        "markdown".to_string(),
        JsonValue::String("# Test".to_string()),
    );
    args.insert(
        "font".to_string(),
        JsonValue::String("comic-sans".to_string()),
    );

    let mut params = BTreeMap::new();
    params.insert(
        "name".to_string(),
        JsonValue::String("fmd.render_html".to_string()),
    );
    params.insert("arguments".to_string(), JsonValue::Object(args));

    let err = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
        .expect_err("should fail with invalid font");
    assert_eq!(err.0, ERROR_INVALID_OPTIONS);
    assert_eq!(err.2, "invalid_font");
}

#[test]
fn mcp_error_on_unknown_tool() {
    let mut params = BTreeMap::new();
    params.insert(
        "name".to_string(),
        JsonValue::String("fmd.nonexistent_tool".to_string()),
    );
    params.insert("arguments".to_string(), JsonValue::Object(BTreeMap::new()));

    let err = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
        .expect_err("unknown tool");
    assert_eq!(err.0, METHOD_NOT_FOUND);
    assert_eq!(err.2, "method_not_found");
}

#[test]
fn mcp_sequential_calls_session_stream() {
    let mut stream_buf = Vec::new();
    for i in 1..=10 {
        let mut args = BTreeMap::new();
        args.insert(
            "markdown".to_string(),
            JsonValue::String(format!("# Document {i}")),
        );
        let mut params = BTreeMap::new();
        params.insert(
            "name".to_string(),
            JsonValue::String("fmd.render_html".to_string()),
        );
        params.insert("arguments".to_string(), JsonValue::Object(args));
        let mut req = BTreeMap::new();
        req.insert("jsonrpc".to_string(), JsonValue::String("2.0".to_string()));
        req.insert("id".to_string(), JsonValue::Number(i as f64));
        req.insert(
            "method".to_string(),
            JsonValue::String("tools/call".to_string()),
        );
        req.insert("params".to_string(), JsonValue::Object(params));
        let msg = JsonValue::Object(req).to_json_string();
        write_frame(&mut stream_buf, &msg).expect("write frame");
    }

    let mut cursor = Cursor::new(stream_buf);
    for i in 1..=10 {
        let frame = read_frame(&mut cursor, franken_markdown::mcp::MAX_FRAME_BYTES)
            .expect("read frame")
            .expect("some frame");
        let req = parse_jsonrpc_request(&frame).expect("parse jsonrpc");
        assert_eq!(req.id.and_then(|id| id.as_u64()), Some(i));
        let tool_res = handle_tool_call(req.params.as_ref(), DEFAULT_MAX_INPUT_BYTES)
            .expect("handle tool call");
        let content = tool_res.get("content").expect("content");
        let html = match content {
            JsonValue::Array(arr) => arr[0].get("text").and_then(|t| t.as_str()).expect("text"),
            _ => panic!("expected array"),
        };
        assert!(html.contains(&format!("Document {i}")));
    }
}
