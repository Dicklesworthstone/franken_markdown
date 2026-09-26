use super::*;
use std::io::{BufReader, Cursor};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn json(source: &str) -> Result<Json, String> { parse_json(source) }
fn initialized() -> Result<Server, String> {
    let mut server = Server::default();
    let replies = server.handle(json(r#"{"jsonrpc":"2.0","id":"init","method":"initialize","params":{}}"#)?);
    assert_eq!(replies[0].get("id").and_then(Json::as_str), Some("init"));
    assert!(replies[0].get("result").is_some());
    Ok(server)
}
fn buffer(text: &str) -> Buffer {
    Buffer { text: text.to_string(), version: 1, synchronized: true }
}
fn edits(source: &str) -> Result<Vec<Json>, String> {
    match json(source)? {
        Json::Array(values) => Ok(values),
        _ => Err("test edits must be an array".to_string()),
    }
}
fn values(value: &Json) -> &[Json] {
    match value { Json::Array(values) => values, _ => &[] }
}

#[test]
fn utf16_coordinates_cover_astral_crlf_lone_cr_and_empty_final_lines() -> TestResult {
    let text = "a😀b\r\n猫\r\nx\r";
    let index = LineIndex::new(text);
    assert_eq!(index.offset(text, Position { line: 0, character: 3 })?, 5);
    assert!(index.offset(text, Position { line: 0, character: 2 }).is_err());
    assert_eq!(index.offset(text, Position { line: 0, character: 99 })?, 6);
    assert_eq!(index.position(text, 6), Position { line: 0, character: 4 });
    assert_eq!(index.position(text, 7), Position { line: 0, character: 4 });
    assert_eq!(index.position(text, 8), Position { line: 1, character: 0 });
    assert_eq!(index.offset(text, Position { line: 3, character: 0 })?, text.len());
    assert!(index.offset(text, Position { line: 4, character: 0 }).is_err());
    Ok(())
}

#[test]
fn every_unicode_boundary_round_trips() -> TestResult {
    for text in ["", "a", "e\u{301}😀猫", "😀\n猫\n", "a\rb"] {
        let index = LineIndex::new(text);
        for offset in (0..=text.len()).filter(|&n| text.is_char_boundary(n)) {
            assert_eq!(index.offset(text, index.position(text, offset))?, offset);
        }
    }
    Ok(())
}

#[test]
fn incremental_changes_are_sequential_and_count_utf16_not_bytes() -> TestResult {
    let mut doc = buffer("A😀B");
    doc.change(2, &edits(r#"[
        {"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"rangeLength":2,"text":"猫"},
        {"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":3}},"text":"!"}
    ]"#)?, MAX_DOCUMENT_BYTES)?;
    assert_eq!(doc.text, "A猫!");
    assert_eq!(doc.version, 2);
    Ok(())
}

#[test]
fn rejected_batch_is_atomic_and_requires_full_text_resynchronization() -> TestResult {
    let mut doc = buffer("original");
    let changes = edits(r#"[
        {"text":"tentative"},
        {"range":{"start":{"line":9,"character":0},"end":{"line":9,"character":0}},"text":"bad"}
    ]"#)?;
    assert!(doc.change(2, &changes, MAX_DOCUMENT_BYTES).is_err());
    assert_eq!(doc.text, "original");
    assert!(!doc.synchronized);
    let incremental = edits(r#"[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"text":"x"}]"#)?;
    assert!(doc.change(3, &incremental, MAX_DOCUMENT_BYTES).is_err());
    doc.change(4, &edits(r#"[{"text":"recovered"}]"#)?, MAX_DOCUMENT_BYTES)?;
    assert!(doc.synchronized);
    assert_eq!(doc.text, "recovered");
    Ok(())
}

#[test]
fn stale_versions_cannot_replace_current_text_or_poison_it() -> TestResult {
    let mut doc = buffer("current");
    assert!(doc.change(1, &edits(r#"[{"text":"stale"}]"#)?, MAX_DOCUMENT_BYTES).is_err());
    assert!(doc.synchronized);
    assert_eq!(doc.text, "current");
    Ok(())
}

#[test]
fn edit_budgets_surrogates_lengths_and_reversed_ranges_fail_closed() -> TestResult {
    for change in [
        r#"[{"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":3}},"text":""}]"#,
        r#"[{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"rangeLength":4,"text":"x"}]"#,
        r#"[{"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":1}},"text":""}]"#,
        r#"[{"range":null,"text":"x"}]"#,
        r#"[]"#,
    ] {
        let mut doc = buffer("a😀b");
        assert!(doc.change(2, &edits(change)?, MAX_DOCUMENT_BYTES).is_err());
        assert_eq!(doc.text, "a😀b");
    }
    let mut doc = buffer("abc");
    assert!(doc.change(2, &edits(r#"[{"text":"toolong"}]"#)?, 3).is_err());
    let mut doc = buffer("abc");
    assert!(doc.change(2, &edits(r#"[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"text":""}]"#)?, 2).is_err());
    let mut doc = buffer("abc");
    assert!(doc.change(2, &vec![object([("text", string("x"))]); 129], MAX_DOCUMENT_BYTES).is_err());
    Ok(())
}

#[test]
fn lifecycle_preserves_ids_and_rejects_requests_before_initialize() -> TestResult {
    let mut server = Server::default();
    let replies = server.handle(json(r#"{"jsonrpc":"2.0","id":7,"method":"textDocument/hover"}"#)?);
    assert_eq!(replies[0].get("error").and_then(|e| e.get("code")).and_then(integer), Some(-32002));
    assert!(server.handle(json(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#)?).is_empty());
    server = initialized()?;
    let replies = server.handle(json(r#"{"jsonrpc":"2.0","id":"unicode-😀","method":"unknown"}"#)?);
    assert_eq!(replies[0].get("id").and_then(Json::as_str), Some("unicode-😀"));
    assert!(server.handle(json(r#"{"jsonrpc":"2.0","method":"unknown"}"#)?).is_empty());
    let replies = server.handle(json(r#"{"jsonrpc":"2.0","id":1.5,"method":"shutdown"}"#)?);
    assert!(replies[0].get("error").is_some());
    server.handle(json(r#"{"jsonrpc":"2.0","id":9,"method":"shutdown"}"#)?);
    assert!(server.handle(json(r#"{"jsonrpc":"2.0","method":"exit"}"#)?).is_empty());
    assert_eq!(server.exit, Some(true));
    Ok(())
}

#[test]
fn diagnostics_are_real_parser_findings_and_clear_after_fix_and_close() -> TestResult {
    let mut server = initialized()?;
    let source = "😀\n\n```rust\nlet x = 1;\n";
    let parsed = parse_markdown_spanned(source);
    assert!(!parsed.diagnostics.is_empty());
    let report = server.open(&object([("textDocument", object([
        ("uri", string("untitled:notes")), ("version", number(1)), ("text", string(source)),
    ]))]))?;
    let findings = values(report[0].get("params").and_then(|p| p.get("diagnostics")).ok_or("missing diagnostics")?);
    assert_eq!(findings.len(), parsed.diagnostics.len());
    assert_eq!(findings[0].get("message").and_then(Json::as_str), Some(parsed.diagnostics[0].message.as_str()));
    let report = server.change(&json(r#"{"textDocument":{"uri":"untitled:notes","version":2},"contentChanges":[{"text":"# Fixed\n"}]}"#)?)?;
    assert!(values(report[0].get("params").and_then(|p| p.get("diagnostics")).ok_or("missing diagnostics")?).is_empty());
    let report = server.close(&json(r#"{"textDocument":{"uri":"untitled:notes"}}"#)?)?;
    assert!(values(report[0].get("params").and_then(|p| p.get("diagnostics")).ok_or("missing diagnostics")?).is_empty());
    assert!(server.documents.is_empty());
    Ok(())
}

#[test]
fn framed_stream_handles_fragmented_unicode_parse_errors_and_clean_exit() -> TestResult {
    let mut input = Vec::new();
    for message in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        "{",
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"untitled:😀","version":1,"text":"# hello"}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ] { write_frame(&mut input, message)?; }
    let mut output = Vec::new();
    assert!(run(&mut BufReader::with_capacity(1, Cursor::new(input)), &mut output)?);
    let mut reader = Cursor::new(output);
    let mut replies = Vec::new();
    while let Some(body) = read_frame(&mut reader, MAX_FRAME_BYTES)? { replies.push(json(&body)?); }
    assert_eq!(replies.len(), 4);
    assert_eq!(replies[1].get("error").and_then(|e| e.get("code")).and_then(integer), Some(-32700));
    assert_eq!(replies[2].get("params").and_then(|p| p.get("uri")).and_then(Json::as_str), Some("untitled:😀"));
    assert_eq!(replies[3].get("result"), Some(&Json::Null));
    Ok(())
}

#[test]
fn truncated_or_oversized_frames_and_abrupt_exits_do_not_succeed() -> TestResult {
    for input in [
        "Content-Length: 8388609\r\n\r\n",
        "Content-Length: 10\r\n\r\n{}",
        "Content-Length: 1\r\nContent-Length: 1\r\n\r\n{",
    ] {
        assert!(run(&mut Cursor::new(input), &mut Vec::new()).is_err());
    }
    assert!(!run(&mut Cursor::new(Vec::<u8>::new()), &mut Vec::new())?);
    let mut input = Vec::new();
    write_frame(&mut input, r#"{"jsonrpc":"2.0","method":"exit"}"#)?;
    assert!(!run(&mut Cursor::new(input), &mut Vec::new())?);
    Ok(())
}
