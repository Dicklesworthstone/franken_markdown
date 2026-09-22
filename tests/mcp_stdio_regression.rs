//! Behavioral regression tests for the same MCP connection loop used by the CLI.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::mcp::{
    DEFAULT_MAX_INPUT_BYTES, INVALID_PARAMS, INVALID_REQUEST, JsonValue, MAX_FRAME_BYTES,
    MAX_JSON_DEPTH, PARSE_ERROR, parse_json, parse_jsonrpc_request, read_frame, serve_stdio,
    write_frame,
};
use std::io::{self, BufReader, Cursor, Write};

fn session(input: &str) -> (usize, Vec<JsonValue>, String) {
    let mut output = Vec::new();
    let count = serve_stdio(
        &mut Cursor::new(input),
        &mut output,
        DEFAULT_MAX_INPUT_BYTES,
        MAX_FRAME_BYTES,
    )
    .expect("complete session");
    let raw = String::from_utf8(output).expect("UTF-8 output");
    let mut cursor = Cursor::new(raw.as_bytes());
    let mut responses = Vec::new();
    while let Some(frame) = read_frame(&mut cursor, MAX_FRAME_BYTES).expect("response frame") {
        responses.push(parse_json(&frame).expect("JSON response"));
    }
    (count, responses, raw)
}

fn error_code(response: &JsonValue) -> f64 {
    response
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(JsonValue::as_f64)
        .expect("error code")
}

#[test]
fn standard_stdio_handshake_notifications_discovery_and_ping() {
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\"}}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":\"ping-α\",\"method\":\"ping\"}\n",
    );
    let (count, responses, raw) = session(input);
    assert_eq!(count, 4);
    assert_eq!(responses.len(), 3);
    assert_eq!(raw.lines().count(), 3);
    assert!(!raw.contains("Content-Length:"));
    for line in raw.lines() {
        assert!(parse_json(line).is_ok(), "stdout must contain only JSON lines");
    }
    let result = responses[0].get("result").unwrap();
    assert_eq!(
        result.get("protocolVersion").and_then(JsonValue::as_str),
        Some("2024-11-05")
    );
    assert!(responses[1].get("result").unwrap().get("tools").is_some());
    assert_eq!(responses[2].get("id").and_then(JsonValue::as_str), Some("ping-α"));
}

#[test]
fn legacy_clients_receive_legacy_frames_for_the_entire_session() {
    let mut input = Vec::new();
    for message in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
    ] {
        write_frame(&mut input, message).unwrap();
    }
    let (count, responses, raw) = session(std::str::from_utf8(&input).unwrap());
    assert_eq!(count, 3);
    assert_eq!(responses.len(), 2);
    assert_eq!(raw.matches("Content-Length:").count(), 2);
    assert_eq!(responses[1].get("id").and_then(JsonValue::as_u64), Some(2));
}

#[test]
fn notifications_never_emit_responses() {
    let (count, responses, raw) = session(concat!(
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{}}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"unknown/notification\"}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"tools/list\"}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"params\":false}\n",
    ));
    assert_eq!(count, 5);
    assert!(responses.is_empty());
    assert!(raw.is_empty());
}

#[test]
fn malformed_json_does_not_poison_the_next_message() {
    let (_, responses, _) = session(concat!(
        "{invalid}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n",
    ));
    assert_eq!(responses.len(), 2);
    assert_eq!(error_code(&responses[0]), f64::from(PARSE_ERROR));
    assert_eq!(responses[1].get("id").and_then(JsonValue::as_u64), Some(7));
    assert!(responses[1].get("result").is_some());
}

#[test]
fn framing_does_not_change_mid_connection() {
    let (_, responses, raw) = session(concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n",
        "Content-Length: 0\n\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n",
    ));
    assert_eq!(responses.len(), 3);
    assert_eq!(error_code(&responses[1]), f64::from(PARSE_ERROR));
    assert!(!raw.contains("Content-Length:"));
    assert_eq!(responses[2].get("id").and_then(JsonValue::as_u64), Some(2));
}

#[test]
fn json_lines_limit_counts_utf8_bytes_and_allows_crlf() {
    let payload = r#"{"text":"α😀"}"#;
    for suffix in ["\n", "\r\n", ""] {
        let input = format!("{payload}{suffix}");
        assert_eq!(
            read_frame(&mut Cursor::new(&input), payload.len()).unwrap(),
            Some(payload.to_string())
        );
        assert!(read_frame(&mut Cursor::new(&input), payload.len() - 1).is_err());
    }
}

#[test]
fn oversized_frame_is_fatal_not_a_second_request() {
    let input = format!(
        "{{\"data\":\"{}\"}}\n{{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}}\n",
        "x".repeat(1024)
    );
    let mut output = Vec::new();
    let error = serve_stdio(&mut Cursor::new(input), &mut output, 32, 64).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    let raw = String::from_utf8(output).unwrap();
    assert_eq!(raw.lines().count(), 1);
    assert_eq!(error_code(&parse_json(raw.trim()).unwrap()), f64::from(PARSE_ERROR));
    assert!(!raw.contains("\"result\""));
}

#[test]
fn malformed_headers_and_bodies_fail_without_panics() {
    for input in [
        "Content-Length: 1\r\nContent-Length: 1\r\n\r\nx",
        "Content-Length: -1\r\n\r\n",
        "Content-Length: +1\r\n\r\nx",
        "Content-Length: nope\r\n\r\n",
        "Content-Length: 99999999999999999999999999\r\n\r\n",
        "Content-Type: application/json\r\n\r\n",
        "Content-Length: 65\r\n\r\n",
    ] {
        assert!(read_frame(&mut Cursor::new(input), 64).is_err(), "{input:?}");
    }
    for input in [
        "Content-Length: 3\r\n",
        "Content-Length: 3",
        "Content-Length: 3\r\n\r\nx",
    ] {
        assert_eq!(
            read_frame(&mut Cursor::new(input), 64).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
    let unicode = "xxxxxxxxxxxxx😀\n";
    assert_eq!(
        read_frame(&mut Cursor::new(unicode), 64).unwrap(),
        Some(unicode.trim_end().to_string())
    );
    assert!(read_frame(&mut Cursor::new(b"Content-Length: 1\r\n\r\n\xff"), 64).is_err());
}

#[test]
fn cumulative_header_budget_and_unterminated_lines_are_bounded() {
    let input = format!("Content-Length: 2\r\n{}\r\n{{}}", "X-Test: a\r\n".repeat(1024));
    assert!(read_frame(&mut Cursor::new(input), 64).is_err());
    let input = format!("Content-Length: {}", "1".repeat(16 * 1024));
    assert!(read_frame(&mut Cursor::new(input), MAX_FRAME_BYTES).is_err());
    assert!(read_frame(&mut Cursor::new("x".repeat(4096)), 64).is_err());
}

#[test]
fn single_byte_buffering_and_extra_headers_preserve_boundaries() {
    let input = b"content-type: application/json\r\ncontent-length: 2\r\n\r\n{}Content-Length: 2\r\n\r\n[]";
    let mut reader = BufReader::with_capacity(1, Cursor::new(input));
    assert_eq!(read_frame(&mut reader, 2).unwrap(), Some("{}".to_string()));
    assert_eq!(read_frame(&mut reader, 2).unwrap(), Some("[]".to_string()));
    assert!(read_frame(&mut reader, 2).unwrap().is_none());
}

#[test]
fn json_depth_limit_bounds_recursive_descent() {
    let at_limit = format!("{}0{}", "[".repeat(MAX_JSON_DEPTH), "]".repeat(MAX_JSON_DEPTH));
    assert!(parse_json(&at_limit).is_ok());
    for depth in [MAX_JSON_DEPTH + 1, 20_000] {
        let input = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        assert!(parse_json(&input).is_err());
    }
    let objects = format!(
        "{}0{}",
        "{\"a\":".repeat(MAX_JSON_DEPTH + 1),
        "}".repeat(MAX_JSON_DEPTH + 1)
    );
    assert!(parse_json(&objects).is_err());
}

#[test]
fn invalid_json_syntax_is_not_silently_repaired() {
    for input in [
        "01", "-01", "1.", "-.1", "1e", "1e+", "+1", "NaN", "1e9999",
        "[1,]", "{\"a\":1,}", "\u{a0}null", "null\u{b}", "\"raw\nnewline\"",
        "\"raw\tcontrol\"", "\"\u{0}\"", r#""\q""#, r#""\u+000""#,
    ] {
        assert!(parse_json(input).is_err(), "accepted invalid JSON: {input:?}");
    }
    for input in [
        "0", "-0", "1.25e+2", "-3E-2", " \t\r\nnull", r#""\n\t\u0000\uD83D\uDE00""#,
    ] {
        assert!(parse_json(input).is_ok(), "rejected valid JSON: {input:?}");
    }
}

#[test]
fn jsonrpc_version_id_and_parameter_shape_are_validated() {
    for input in [
        r#"{"id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":2,"id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":true,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":[],"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":9007199254740993,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#,
    ] {
        assert_eq!(parse_jsonrpc_request(input).unwrap_err().1, INVALID_REQUEST, "{input}");
    }
    assert_eq!(
        parse_jsonrpc_request(r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":false}"#)
            .unwrap_err().1,
        INVALID_PARAMS
    );
}

#[test]
fn unsigned_number_conversion_does_not_saturate_or_wrap() {
    for number in [f64::NAN, f64::INFINITY, -1.0, 1.5, u64::MAX as f64] {
        assert_eq!(JsonValue::Number(number).as_u64(), None);
    }
    assert_eq!(JsonValue::Number(0.0).as_u64(), Some(0));
    assert_eq!(JsonValue::Number(42.0).as_u64(), Some(42));
    let boundary = JsonValue::Number(i64::MAX as f64);
    assert_eq!(parse_json(&boundary.to_json_string()).unwrap(), boundary);
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed client"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn response_write_failure_propagates() {
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";
    let error = serve_stdio(&mut Cursor::new(input), &mut FailingWriter, 64, 128).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}
