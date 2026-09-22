//! Model Context Protocol (MCP) stdio server (beads yjmy / jqhm / uxhr).
//!
//! Zero-dependency, blocking JSON-RPC for the native agent-facing surface.
//! Standard clients use newline-delimited JSON; legacy Content-Length clients
//! receive matching replies. Input frames are capped at 72 MiB.

use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Write};

use crate::RenderError;
pub use crate::html::base64_encode;

mod file_render;
mod tools;
mod transport;

pub use tools::{handle_tool_call, tools_list_result};
pub use transport::{read_frame, write_frame};
use transport::{Framing, read_message, write_message};

/// Default maximum frame bytes (72 MiB).
pub const MAX_FRAME_BYTES: usize = 72 * 1024 * 1024;
/// Maximum nested JSON containers accepted from an untrusted peer.
pub const MAX_JSON_DEPTH: usize = 128;
/// Default maximum input Markdown bytes (64 MiB).
pub const DEFAULT_MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
/// Supported MCP protocol version.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC 2.0 error codes.
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// Application-specific error codes matching CLI reasons.
pub const ERROR_INPUT_TOO_LARGE: i32 = -32001;
pub const ERROR_INVALID_OPTIONS: i32 = -32002;
pub const ERROR_RENDER_FAILED: i32 = -32003;
pub const ERROR_INPUT_ERROR: i32 = -32004;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    #[inline(always)]
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(n)
                if n.is_finite() && *n >= 0.0 && *n < (u64::MAX as f64) && n.fract() == 0.0 =>
            {
                Some(*n as u64)
            }
            _ => None,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            Self::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Number(n) => {
                if n.is_finite() {
                    if n.fract() == 0.0 && *n >= (i64::MIN as f64) && *n < (i64::MAX as f64) {
                        out.push_str(&format!("{}", *n as i64));
                    } else {
                        out.push_str(&format!("{n}"));
                    }
                } else {
                    out.push('0');
                }
            }
            Self::String(s) => write_json_str(s, out),
            Self::Array(arr) => {
                out.push('[');
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write_json(out);
                }
                out.push(']');
            }
            Self::Object(map) => {
                out.push('{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_json_str(k, out);
                    out.push(':');
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

pub fn parse_json(input: &str) -> Result<JsonValue, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut idx = 0;
    skip_whitespace(&chars, &mut idx);
    let val = parse_value(&chars, &mut idx, 0)?;
    skip_whitespace(&chars, &mut idx);
    if idx < chars.len() {
        return Err(format!("unexpected trailing data at index {idx}"));
    }
    Ok(val)
}

fn skip_whitespace(chars: &[char], idx: &mut usize) {
    while *idx < chars.len() && matches!(chars[*idx], ' ' | '\t' | '\r' | '\n') {
        *idx += 1;
    }
}

fn parse_value(chars: &[char], idx: &mut usize, depth: usize) -> Result<JsonValue, String> {
    skip_whitespace(chars, idx);
    if *idx >= chars.len() {
        return Err("unexpected end of JSON input".to_string());
    }
    if depth >= MAX_JSON_DEPTH && matches!(chars[*idx], '[' | '{') {
        return Err(format!("JSON nesting exceeds maximum {MAX_JSON_DEPTH}"));
    }
    match chars[*idx] {
        'n' => parse_null(chars, idx),
        't' | 'f' => parse_bool(chars, idx),
        '"' => parse_string_val(chars, idx),
        '[' => parse_array(chars, idx, depth + 1),
        '{' => parse_object(chars, idx, depth + 1),
        '-' | '0'..='9' => parse_number(chars, idx),
        c => Err(format!("unexpected character '{c}' at index {idx}")),
    }
}

fn parse_null(chars: &[char], idx: &mut usize) -> Result<JsonValue, String> {
    if *idx + 4 <= chars.len() && chars[*idx..*idx + 4] == ['n', 'u', 'l', 'l'] {
        *idx += 4;
        Ok(JsonValue::Null)
    } else {
        Err(format!("invalid null literal at index {idx}"))
    }
}

fn parse_bool(chars: &[char], idx: &mut usize) -> Result<JsonValue, String> {
    if *idx + 4 <= chars.len() && chars[*idx..*idx + 4] == ['t', 'r', 'u', 'e'] {
        *idx += 4;
        Ok(JsonValue::Bool(true))
    } else if *idx + 5 <= chars.len() && chars[*idx..*idx + 5] == ['f', 'a', 'l', 's', 'e'] {
        *idx += 5;
        Ok(JsonValue::Bool(false))
    } else {
        Err(format!("invalid boolean literal at index {idx}"))
    }
}

fn parse_string_raw(chars: &[char], idx: &mut usize) -> Result<String, String> {
    if *idx >= chars.len() || chars[*idx] != '"' {
        return Err(format!("expected '\"' at index {idx}"));
    }
    *idx += 1;
    let mut s = String::new();
    while *idx < chars.len() {
        let c = chars[*idx];
        *idx += 1;
        match c {
            '"' => return Ok(s),
            '\\' => {
                if *idx >= chars.len() {
                    return Err("unexpected EOF inside string escape".to_string());
                }
                let esc = chars[*idx];
                *idx += 1;
                match esc {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'b' => s.push('\u{08}'),
                    'f' => s.push('\u{0c}'),
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    'u' => {
                        if *idx + 4 > chars.len() {
                            return Err("truncated unicode escape".to_string());
                        }
                        if !chars[*idx..*idx + 4].iter().all(char::is_ascii_hexdigit) {
                            return Err("invalid hex in unicode escape".to_string());
                        }
                        let hex: String = chars[*idx..*idx + 4].iter().collect();
                        *idx += 4;
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|e| format!("invalid hex in unicode escape: {e}"))?;
                        if (0xD800..=0xDBFF).contains(&code) {
                            if *idx + 6 <= chars.len()
                                && chars[*idx] == '\\'
                                && chars[*idx + 1] == 'u'
                            {
                                let low_hex: String = chars[*idx + 2..*idx + 6].iter().collect();
                                if let Ok(low_code) = u32::from_str_radix(&low_hex, 16) {
                                    if (0xDC00..=0xDFFF).contains(&low_code) {
                                        *idx += 6;
                                        let scalar = 0x10000
                                            + (((code - 0xD800) << 10) | (low_code - 0xDC00));
                                        s.push(char::from_u32(scalar).unwrap_or('\u{FFFD}'));
                                        continue;
                                    }
                                }
                            }
                            s.push('\u{FFFD}');
                        } else if (0xDC00..=0xDFFF).contains(&code) {
                            s.push('\u{FFFD}');
                        } else {
                            let ch = char::from_u32(code)
                                .ok_or_else(|| format!("invalid unicode code point: {code}"))?;
                            s.push(ch);
                        }
                    }
                    _ => return Err("invalid JSON string escape".to_string()),
                }
            }
            c if c <= '\u{1f}' => {
                return Err("unescaped control character in JSON string".to_string());
            }
            _ => s.push(c),
        }
    }
    Err("unterminated string literal".to_string())
}

fn parse_string_val(chars: &[char], idx: &mut usize) -> Result<JsonValue, String> {
    parse_string_raw(chars, idx).map(JsonValue::String)
}

fn parse_number(chars: &[char], idx: &mut usize) -> Result<JsonValue, String> {
    let start = *idx;
    if chars.get(*idx) == Some(&'-') {
        *idx += 1;
    }
    match chars.get(*idx) {
        Some('0') => {
            *idx += 1;
            if chars.get(*idx).is_some_and(char::is_ascii_digit) {
                return Err("leading zero in JSON number".to_string());
            }
        }
        Some('1'..='9') => {
            while chars.get(*idx).is_some_and(char::is_ascii_digit) {
                *idx += 1;
            }
        }
        _ => return Err("missing integer part in JSON number".to_string()),
    }
    if chars.get(*idx) == Some(&'.') {
        *idx += 1;
        let digits = *idx;
        while chars.get(*idx).is_some_and(char::is_ascii_digit) {
            *idx += 1;
        }
        if *idx == digits {
            return Err("missing fractional digits in JSON number".to_string());
        }
    }
    if matches!(chars.get(*idx), Some('e' | 'E')) {
        *idx += 1;
        if matches!(chars.get(*idx), Some('+' | '-')) {
            *idx += 1;
        }
        let digits = *idx;
        while chars.get(*idx).is_some_and(char::is_ascii_digit) {
            *idx += 1;
        }
        if *idx == digits {
            return Err("missing exponent digits in JSON number".to_string());
        }
    }
    let num_str: String = chars[start..*idx].iter().collect();
    let num = num_str.parse::<f64>().map_err(|_| "invalid JSON number".to_string())?;
    if !num.is_finite() {
        return Err("JSON number is outside the supported finite range".to_string());
    }
    Ok(JsonValue::Number(num))
}

fn parse_array(chars: &[char], idx: &mut usize, depth: usize) -> Result<JsonValue, String> {
    *idx += 1;
    let mut arr = Vec::new();
    skip_whitespace(chars, idx);
    if *idx < chars.len() && chars[*idx] == ']' {
        *idx += 1;
        return Ok(JsonValue::Array(arr));
    }
    loop {
        arr.push(parse_value(chars, idx, depth)?);
        skip_whitespace(chars, idx);
        if *idx >= chars.len() {
            return Err("unterminated array".to_string());
        }
        match chars[*idx] {
            ',' => {
                *idx += 1;
                skip_whitespace(chars, idx);
            }
            ']' => {
                *idx += 1;
                return Ok(JsonValue::Array(arr));
            }
            c => return Err(format!("expected ',' or ']' in array, found '{c}'")),
        }
    }
}

fn parse_object(chars: &[char], idx: &mut usize, depth: usize) -> Result<JsonValue, String> {
    *idx += 1;
    let mut map = BTreeMap::new();
    skip_whitespace(chars, idx);
    if *idx < chars.len() && chars[*idx] == '}' {
        *idx += 1;
        return Ok(JsonValue::Object(map));
    }
    loop {
        skip_whitespace(chars, idx);
        let key = parse_string_raw(chars, idx)?;
        skip_whitespace(chars, idx);
        if *idx >= chars.len() || chars[*idx] != ':' {
            return Err("expected ':' after object key".to_string());
        }
        *idx += 1;
        let val = parse_value(chars, idx, depth)?;
        map.insert(key, val);
        skip_whitespace(chars, idx);
        if *idx >= chars.len() {
            return Err("unterminated object".to_string());
        }
        match chars[*idx] {
            ',' => {
                *idx += 1;
                skip_whitespace(chars, idx);
            }
            '}' => {
                *idx += 1;
                return Ok(JsonValue::Object(map));
            }
            c => return Err(format!("expected ',' or '}}' in object, found '{c}'")),
        }
    }
}

#[derive(Debug)]
pub struct JsonRpcRequest {
    pub id: Option<JsonValue>,
    pub method: String,
    pub params: Option<JsonValue>,
}

pub fn parse_jsonrpc_request(
    payload: &str,
) -> Result<JsonRpcRequest, (Option<JsonValue>, i32, String, &'static str)> {
    let val = parse_json(payload).map_err(|error| (
        None, PARSE_ERROR, format!("Parse error: {error}"), "parse_error",
    ))?;
    let obj = val.as_object().ok_or_else(|| (
        None, INVALID_REQUEST, "Invalid Request: expected JSON object".to_string(), "invalid_request",
    ))?;
    let id = obj.get("id").cloned();
    let valid_id = match &id {
        None | Some(JsonValue::Null | JsonValue::String(_)) => true,
        // f64 cannot echo arbitrary large integer IDs faithfully.
        Some(JsonValue::Number(number)) => {
            number.is_finite() && number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0
        }
        _ => false,
    };
    if !valid_id {
        return Err((None, INVALID_REQUEST, "Invalid Request: id must be a string, null, or safe integer".to_string(), "invalid_request"));
    }
    if obj.get("jsonrpc").and_then(JsonValue::as_str) != Some("2.0") {
        return Err((id, INVALID_REQUEST, "Invalid Request: jsonrpc must be '2.0'".to_string(), "invalid_request"));
    }
    let method = obj.get("method").and_then(JsonValue::as_str).ok_or_else(|| (
        id.clone(), INVALID_REQUEST, "Invalid Request: missing method string".to_string(), "invalid_request",
    ))?.to_string();
    let params = obj.get("params").cloned();
    if params.as_ref().is_some_and(|value| !matches!(value, JsonValue::Object(_) | JsonValue::Array(_))) {
        return Err((id, INVALID_PARAMS, "Invalid params: expected object or array".to_string(), "invalid_params"));
    }
    Ok(JsonRpcRequest { id, method, params })
}

pub fn jsonrpc_success(id: &JsonValue, result: JsonValue) -> String {
    JsonValue::Object(BTreeMap::from([
        ("jsonrpc".to_string(), JsonValue::String("2.0".to_string())),
        ("id".to_string(), id.clone()),
        ("result".to_string(), result),
    ])).to_json_string()
}

pub fn jsonrpc_error(id: Option<&JsonValue>, code: i32, message: &str, reason: &'static str) -> String {
    let data = JsonValue::Object(BTreeMap::from([
        ("reason".to_string(), JsonValue::String(reason.to_string())),
    ]));
    let error = JsonValue::Object(BTreeMap::from([
        ("code".to_string(), JsonValue::Number(f64::from(code))),
        ("message".to_string(), JsonValue::String(message.to_string())),
        ("data".to_string(), data),
    ]));
    JsonValue::Object(BTreeMap::from([
        ("jsonrpc".to_string(), JsonValue::String("2.0".to_string())),
        ("id".to_string(), id.cloned().unwrap_or(JsonValue::Null)),
        ("error".to_string(), error),
    ])).to_json_string()
}

pub fn run_stdio_server(max_input_bytes: u64) -> Result<(), RenderError> {
    eprintln!("fmd mcp stdio server started (protocol {})", MCP_PROTOCOL_VERSION);
    let mut reader = BufReader::new(std::io::stdin());
    let stdout = std::io::stdout();
    match serve_stdio(&mut reader, &mut stdout.lock(), max_input_bytes, MAX_FRAME_BYTES) {
        Ok(count) => {
            eprintln!("fmd mcp stdio server closed cleanly (handled {count} requests)");
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Run the CLI's real MCP connection loop over caller-provided streams.
/// Framing failures terminate the connection. Well-framed JSON/request errors
/// are isolated to one message and do not poison subsequent requests.
pub fn serve_stdio<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    max_input_bytes: u64,
    max_frame_bytes: usize,
) -> io::Result<usize> {
    let mut framing = None;
    let mut request_count = 0usize;
    loop {
        let frame = match read_message(reader, &mut framing, max_frame_bytes) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(request_count),
            Err(error) => {
                let response = jsonrpc_error(None, PARSE_ERROR, &format!("Frame read error: {error}"), "frame_error");
                write_message(writer, framing.unwrap_or(Framing::JsonLines), &response)?;
                return Err(error);
            }
        };
        request_count += 1;
        if let Some(response) = dispatch_message(&frame, max_input_bytes) {
            write_message(writer, framing.unwrap_or(Framing::JsonLines), &response)?;
        }
    }
}

fn dispatch_message(frame: &str, max_input_bytes: u64) -> Option<String> {
    let request = match parse_jsonrpc_request(frame) {
        Ok(request) => request,
        Err((None, INVALID_PARAMS, _, _)) => return None,
        Err((id, code, message, reason)) => return Some(jsonrpc_error(id.as_ref(), code, &message, reason)),
    };
    // Notifications, including unknown methods, never receive replies. Tool
    // invocations require an id and are not executed as notifications.
    let id = request.id.as_ref()?;
    let result = match request.method.as_str() {
        "initialize" => {
            let capabilities = JsonValue::Object(BTreeMap::from([
                ("tools".to_string(), JsonValue::Object(BTreeMap::new())),
            ]));
            let info = JsonValue::Object(BTreeMap::from([
                ("name".to_string(), JsonValue::String("fmd".to_string())),
                ("version".to_string(), JsonValue::String(env!("CARGO_PKG_VERSION").to_string())),
            ]));
            Ok(JsonValue::Object(BTreeMap::from([
                ("protocolVersion".to_string(), JsonValue::String(MCP_PROTOCOL_VERSION.to_string())),
                ("capabilities".to_string(), capabilities),
                ("serverInfo".to_string(), info),
            ])))
        }
        "ping" => Ok(JsonValue::Object(BTreeMap::new())),
        "tools/list" => Ok(tools_list_result()),
        "tools/call" => handle_tool_call(request.params.as_ref(), max_input_bytes),
        _ => Err((METHOD_NOT_FOUND, format!("Method not found: '{}'", request.method), "method_not_found")),
    };
    Some(match result {
        Ok(result) => jsonrpc_success(id, result),
        Err((code, message, reason)) => jsonrpc_error(Some(id), code, &message, reason),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn json_parser_handles_primitives_and_nested_structures() {
        let raw = r#"{
            "num": 42, "float": 3.1415, "neg": -10, "str": "hello\nworld",
            "bool_t": true, "bool_f": false, "null_val": null,
            "arr": [1, "two", true, null], "nested": { "key": "value" }
        }"#;
        let val = parse_json(raw).expect("parse json");
        assert_eq!(val.get("num").and_then(JsonValue::as_u64), Some(42));
        assert_eq!(val.get("str").and_then(JsonValue::as_str), Some("hello\nworld"));
        assert_eq!(val.get("bool_t").and_then(JsonValue::as_bool), Some(true));
        assert_eq!(val.get("null_val"), Some(&JsonValue::Null));
        assert_eq!(val, parse_json(&val.to_json_string()).expect("roundtrip parse"));
    }

    #[test]
    fn read_write_framed_messages_roundtrip() {
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let mut buf = Vec::new();
        write_frame(&mut buf, msg).expect("write frame");
        assert_eq!(read_frame(&mut std::io::Cursor::new(buf), MAX_FRAME_BYTES).unwrap().unwrap(), msg);
    }

    #[test]
    fn tools_list_returns_valid_schema_declarations() {
        let list = tools_list_result();
        let JsonValue::Array(tools) = list.get("tools").expect("tools") else { panic!("tools array") };
        assert_eq!(tools.len(), 5);
        let names: Vec<_> = tools.iter().filter_map(|tool| tool.get("name").and_then(JsonValue::as_str)).collect();
        for name in ["fmd.render_html", "fmd.render_pdf", "fmd.verify", "fmd.capabilities", "fmd.render_file"] {
            assert!(names.contains(&name));
        }
    }

    fn call(tool: &str, markdown: &str) -> String {
        let params = JsonValue::Object(BTreeMap::from([
            ("name".to_string(), JsonValue::String(tool.to_string())),
            ("arguments".to_string(), JsonValue::Object(BTreeMap::from([
                ("markdown".to_string(), JsonValue::String(markdown.to_string())),
            ]))),
        ]));
        let result = handle_tool_call(Some(&params), DEFAULT_MAX_INPUT_BYTES).unwrap();
        let JsonValue::Array(items) = result.get("content").unwrap() else { panic!("content array") };
        items[0].get("text").and_then(JsonValue::as_str).unwrap().to_string()
    }

    #[test]
    fn tool_call_render_html_produces_valid_html() {
        let html = call("fmd.render_html", "# Hello MCP\n\nTesting HTML");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Hello MCP"));
        assert!(html.contains("Testing HTML"));
    }

    #[test]
    fn tool_call_render_pdf_produces_valid_base64() {
        assert!(call("fmd.render_pdf", "# Hello PDF").starts_with("JVBER"));
    }

    #[test]
    fn tool_call_verify_produces_json_report() {
        let report = call("fmd.verify", "# Title\n\nSome text");
        assert!(report.contains("\"schema_version\""));
        assert!(report.contains("\"verdict\""));
    }

    #[test]
    fn json_parser_handles_utf16_surrogate_pairs() {
        let val = parse_json(r#"{"emoji":"\uD83D\uDE00","clef":"\uD834\uDD1E"}"#).unwrap();
        assert_eq!(val.get("emoji").and_then(JsonValue::as_str), Some("😀"));
        assert_eq!(val.get("clef").and_then(JsonValue::as_str), Some("𝄞"));
    }

    #[test]
    fn read_frame_accepts_lowercase_content_length() {
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let raw = format!("content-length: {}\r\n\r\n{msg}", msg.len());
        assert_eq!(read_frame(&mut std::io::Cursor::new(raw), MAX_FRAME_BYTES).unwrap().unwrap(), msg);
    }
}
