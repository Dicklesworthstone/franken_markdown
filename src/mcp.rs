//! Model Context Protocol (MCP) stdio server (beads yjmy / jqhm / uxhr).
//!
//! Provides a zero-dependency, blocking stdio JSON-RPC 2.0 transport for MCP,
//! exposing `franken_markdown` render, verify, and capabilities as tools
//! callable directly by AI coding agents.
//!
//! Framing adheres to the MCP / LSP standard (`Content-Length: <n>\r\n\r\n`).
//! Frame size is capped at 72 MiB (64 MiB input payload + envelope headroom).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::Instant;

pub use crate::html::base64_encode;
use crate::theme::Theme;
use crate::{
    FontFamily, FontScale, HtmlFontFormat, HtmlOptions, PdfAMode, PdfASettings, PdfOptions,
    RenderError, parse_markdown, render_html_document, render_pdf_document,
    render_pdf_document_pdfa,
};

/// Default maximum frame bytes (72 MiB).
pub const MAX_FRAME_BYTES: usize = 72 * 1024 * 1024;
/// Default maximum input Markdown bytes (64 MiB).
pub const DEFAULT_MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

/// MCP protocol version supported.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC 2.0 Error Codes
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// Application-specific error codes matching CLI reasons
pub const ERROR_INPUT_TOO_LARGE: i32 = -32001;
pub const ERROR_INVALID_OPTIONS: i32 = -32002;
pub const ERROR_RENDER_FAILED: i32 = -32003;
pub const ERROR_INPUT_ERROR: i32 = -32004;

// ---------------------------------------------------------------------------
// Zero-Dependency JSON Model & Recursive-Descent Parser
// ---------------------------------------------------------------------------

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
            Self::Number(n) if *n >= 0.0 && n.fract() == 0.0 => Some(*n as u64),
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
                    if n.fract() == 0.0 && *n >= (i64::MIN as f64) && *n <= (i64::MAX as f64) {
                        out.push_str(&format!("{}", *n as i64));
                    } else {
                        out.push_str(&format!("{n}"));
                    }
                } else {
                    out.push('0');
                }
            }
            Self::String(s) => {
                write_json_str(s, out);
            }
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
    let val = parse_value(&chars, &mut idx)?;
    skip_whitespace(&chars, &mut idx);
    if idx < chars.len() {
        return Err(format!("unexpected trailing data at index {idx}"));
    }
    Ok(val)
}

fn skip_whitespace(chars: &[char], idx: &mut usize) {
    while *idx < chars.len() && chars[*idx].is_whitespace() {
        *idx += 1;
    }
}

fn parse_value(chars: &[char], idx: &mut usize) -> Result<JsonValue, String> {
    skip_whitespace(chars, idx);
    if *idx >= chars.len() {
        return Err("unexpected end of JSON input".to_string());
    }
    match chars[*idx] {
        'n' => parse_null(chars, idx),
        't' | 'f' => parse_bool(chars, idx),
        '"' => parse_string_val(chars, idx),
        '[' => parse_array(chars, idx),
        '{' => parse_object(chars, idx),
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
                                        let ch = char::from_u32(scalar).unwrap_or('\u{FFFD}');
                                        s.push(ch);
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
                    _ => s.push(esc),
                }
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
    if *idx < chars.len() && chars[*idx] == '-' {
        *idx += 1;
    }
    while *idx < chars.len() && chars[*idx].is_ascii_digit() {
        *idx += 1;
    }
    if *idx < chars.len() && chars[*idx] == '.' {
        *idx += 1;
        while *idx < chars.len() && chars[*idx].is_ascii_digit() {
            *idx += 1;
        }
    }
    if *idx < chars.len() && (chars[*idx] == 'e' || chars[*idx] == 'E') {
        *idx += 1;
        if *idx < chars.len() && (chars[*idx] == '+' || chars[*idx] == '-') {
            *idx += 1;
        }
        while *idx < chars.len() && chars[*idx].is_ascii_digit() {
            *idx += 1;
        }
    }
    let num_str: String = chars[start..*idx].iter().collect();
    let num = num_str
        .parse::<f64>()
        .map_err(|e| format!("invalid number '{num_str}': {e}"))?;
    Ok(JsonValue::Number(num))
}

fn parse_array(chars: &[char], idx: &mut usize) -> Result<JsonValue, String> {
    *idx += 1; // skip '['
    let mut arr = Vec::new();
    skip_whitespace(chars, idx);
    if *idx < chars.len() && chars[*idx] == ']' {
        *idx += 1;
        return Ok(JsonValue::Array(arr));
    }
    loop {
        let val = parse_value(chars, idx)?;
        arr.push(val);
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

fn parse_object(chars: &[char], idx: &mut usize) -> Result<JsonValue, String> {
    *idx += 1; // skip '{'
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
        *idx += 1; // skip ':'
        let val = parse_value(chars, idx)?;
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

// ---------------------------------------------------------------------------
// Framing Transport: read_frame / write_frame
// ---------------------------------------------------------------------------

/// Read one Content-Length framed JSON-RPC message from `reader`.
///
/// If input starts with `{` or `[`, raw newline-delimited JSON is accepted as a fallback.
pub fn read_frame<R: BufRead>(
    reader: &mut R,
    max_frame_bytes: usize,
) -> std::io::Result<Option<String>> {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Ok(None);
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let Some(len) = content_length {
                if len > max_frame_bytes {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("frame length {len} exceeds maximum {max_frame_bytes}"),
                    ));
                }
                let mut buf = vec![0u8; len];
                reader.read_exact(&mut buf)?;
                let text = String::from_utf8(buf).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid UTF-8: {e}"),
                    )
                })?;
                return Ok(Some(text));
            }
            continue;
        }

        // Direct JSON fallback (if no Content-Length header was sent)
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return Ok(Some(trimmed.to_string()));
        }

        if trimmed.len() >= 15 && trimmed[..15].eq_ignore_ascii_case("content-length:") {
            let len_str = trimmed[15..].trim();
            let len = len_str.parse::<usize>().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid Content-Length '{len_str}': {e}"),
                )
            })?;
            content_length = Some(len);
        }
    }
}

/// Write one Content-Length framed JSON-RPC message to `writer`.
pub fn write_frame<W: Write>(writer: &mut W, body: &str) -> std::io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    writer.flush()
}

// ---------------------------------------------------------------------------
// JSON-RPC Request / Response & MCP Dispatch
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct JsonRpcRequest {
    pub id: Option<JsonValue>,
    pub method: String,
    pub params: Option<JsonValue>,
}

pub fn parse_jsonrpc_request(
    payload: &str,
) -> Result<JsonRpcRequest, (Option<JsonValue>, i32, String, &'static str)> {
    let val = parse_json(payload).map_err(|e| {
        (
            None,
            PARSE_ERROR,
            format!("Parse error: {e}"),
            "parse_error",
        )
    })?;
    let obj = val.as_object().ok_or_else(|| {
        (
            None,
            INVALID_REQUEST,
            "Invalid Request: expected JSON object".to_string(),
            "invalid_request",
        )
    })?;

    let id = obj.get("id").cloned();
    let method = obj
        .get("method")
        .and_then(|m| m.as_str())
        .ok_or_else(|| {
            (
                id.clone(),
                INVALID_REQUEST,
                "Invalid Request: missing method string".to_string(),
                "invalid_request",
            )
        })?
        .to_string();

    let params = obj.get("params").cloned();
    Ok(JsonRpcRequest { id, method, params })
}

pub fn jsonrpc_success(id: &JsonValue, result: JsonValue) -> String {
    let mut map = BTreeMap::new();
    map.insert("jsonrpc".to_string(), JsonValue::String("2.0".to_string()));
    map.insert("id".to_string(), id.clone());
    map.insert("result".to_string(), result);
    JsonValue::Object(map).to_json_string()
}

pub fn jsonrpc_error(
    id: Option<&JsonValue>,
    code: i32,
    message: &str,
    reason: &'static str,
) -> String {
    let mut err_obj = BTreeMap::new();
    err_obj.insert("code".to_string(), JsonValue::Number(f64::from(code)));
    err_obj.insert(
        "message".to_string(),
        JsonValue::String(message.to_string()),
    );

    let mut data_obj = BTreeMap::new();
    data_obj.insert("reason".to_string(), JsonValue::String(reason.to_string()));
    err_obj.insert("data".to_string(), JsonValue::Object(data_obj));

    let mut map = BTreeMap::new();
    map.insert("jsonrpc".to_string(), JsonValue::String("2.0".to_string()));
    map.insert("id".to_string(), id.cloned().unwrap_or(JsonValue::Null));
    map.insert("error".to_string(), JsonValue::Object(err_obj));
    JsonValue::Object(map).to_json_string()
}

// ---------------------------------------------------------------------------
// Tool Declarations & Dispatch
// ---------------------------------------------------------------------------

pub fn tools_list_result() -> JsonValue {
    let mut tools = Vec::new();

    // 1. fmd.render_html
    {
        let mut t = BTreeMap::new();
        t.insert(
            "name".to_string(),
            JsonValue::String("fmd.render_html".to_string()),
        );
        t.insert(
            "description".to_string(),
            JsonValue::String(
                "Render Markdown text to beautiful, self-contained HTML with inlined fonts and styles."
                    .to_string(),
            ),
        );
        let mut schema = BTreeMap::new();
        schema.insert("type".to_string(), JsonValue::String("object".to_string()));
        let mut props = BTreeMap::new();
        props.insert(
            "markdown".to_string(),
            make_prop("string", "Markdown source text to render"),
        );
        props.insert(
            "font".to_string(),
            make_prop("string", "Body font family ('sans' or 'serif')"),
        );
        props.insert(
            "darkMode".to_string(),
            make_prop("string", "Dark mode policy ('auto' or 'disabled')"),
        );
        props.insert(
            "customCss".to_string(),
            make_prop("string", "Custom stylesheet CSS replacing default theme"),
        );
        props.insert(
            "title".to_string(),
            make_prop("string", "Document title metadata"),
        );
        props.insert(
            "lang".to_string(),
            make_prop("string", "Document language tag (e.g. 'en', 'de')"),
        );
        props.insert(
            "allowRawHtml".to_string(),
            make_prop("boolean", "Pass raw HTML through without escaping"),
        );
        props.insert(
            "toc".to_string(),
            make_prop("boolean", "Generate a table of contents"),
        );
        props.insert(
            "tocDepth".to_string(),
            make_prop("integer", "Maximum heading depth in table of contents"),
        );
        props.insert(
            "htmlFontFormat".to_string(),
            make_prop("string", "Font format ('woff1' or 'ttf')"),
        );
        props.insert(
            "interactiveHtml".to_string(),
            make_prop("boolean", "Generate self-hosting interactive HTML"),
        );
        props.insert(
            "fontScale".to_string(),
            make_prop(
                "string",
                "Typographic scale preset or multiplier ('sm', '125%')",
            ),
        );
        schema.insert("properties".to_string(), JsonValue::Object(props));
        schema.insert(
            "required".to_string(),
            JsonValue::Array(vec![JsonValue::String("markdown".to_string())]),
        );
        t.insert("inputSchema".to_string(), JsonValue::Object(schema));
        tools.push(JsonValue::Object(t));
    }

    // 2. fmd.render_pdf
    {
        let mut t = BTreeMap::new();
        t.insert(
            "name".to_string(),
            JsonValue::String("fmd.render_pdf".to_string()),
        );
        t.insert(
            "description".to_string(),
            JsonValue::String(
                "Render Markdown text to compact, deterministic PDF with embedded font subsets (returned as base64)."
                    .to_string(),
            ),
        );
        let mut schema = BTreeMap::new();
        schema.insert("type".to_string(), JsonValue::String("object".to_string()));
        let mut props = BTreeMap::new();
        props.insert(
            "markdown".to_string(),
            make_prop("string", "Markdown source text to render"),
        );
        props.insert(
            "font".to_string(),
            make_prop("string", "Body font family ('sans' or 'serif')"),
        );
        props.insert(
            "title".to_string(),
            make_prop("string", "Document title metadata"),
        );
        props.insert(
            "author".to_string(),
            make_prop("string", "Document author metadata"),
        );
        props.insert(
            "lang".to_string(),
            make_prop("string", "Document language tag for hyphenation"),
        );
        props.insert(
            "fontScale".to_string(),
            make_prop("string", "Typographic scale preset or multiplier"),
        );
        props.insert(
            "fitToPages".to_string(),
            make_prop("integer", "Adaptive page budgeting solver target pages"),
        );
        props.insert(
            "microtype".to_string(),
            make_prop(
                "string",
                "Opt-in microtypography ('off', 'protrusion', 'expansion', 'all')",
            ),
        );
        props.insert(
            "typographyHomogeneous".to_string(),
            make_prop(
                "boolean",
                "Gradual adjacent demerits in Knuth-Plass breaker",
            ),
        );
        props.insert(
            "codeLineNumbers".to_string(),
            make_prop("boolean", "Render line numbers in code blocks"),
        );
        props.insert(
            "pageNumbers".to_string(),
            make_prop("boolean", "Render running page numbers in bottom margin"),
        );
        props.insert(
            "toc".to_string(),
            make_prop("boolean", "Generate a table of contents"),
        );
        props.insert(
            "tocDepth".to_string(),
            make_prop("integer", "Maximum heading depth in table of contents"),
        );
        props.insert(
            "pdfA".to_string(),
            make_prop("string", "PDF/A profile ('2b' or 'off')"),
        );
        props.insert(
            "pdfAStrict".to_string(),
            make_prop("boolean", "Fail closed on non-conformable PDF/A constructs"),
        );
        props.insert(
            "metadataEpochSeconds".to_string(),
            make_prop("integer", "Deterministic UNIX epoch timestamp"),
        );
        schema.insert("properties".to_string(), JsonValue::Object(props));
        schema.insert(
            "required".to_string(),
            JsonValue::Array(vec![JsonValue::String("markdown".to_string())]),
        );
        t.insert("inputSchema".to_string(), JsonValue::Object(schema));
        tools.push(JsonValue::Object(t));
    }

    // 3. fmd.verify
    {
        let mut t = BTreeMap::new();
        t.insert(
            "name".to_string(),
            JsonValue::String("fmd.verify".to_string()),
        );
        t.insert(
            "description".to_string(),
            JsonValue::String(
                "Audit rendered document text layer, internal anchors, accessibility warnings, and horizontal margin overflow."
                    .to_string(),
            ),
        );
        let mut schema = BTreeMap::new();
        schema.insert("type".to_string(), JsonValue::String("object".to_string()));
        let mut props = BTreeMap::new();
        props.insert(
            "markdown".to_string(),
            make_prop("string", "Markdown source text to verify"),
        );
        props.insert(
            "a11y".to_string(),
            make_prop("boolean", "Restrict findings to accessibility audit"),
        );
        schema.insert("properties".to_string(), JsonValue::Object(props));
        schema.insert(
            "required".to_string(),
            JsonValue::Array(vec![JsonValue::String("markdown".to_string())]),
        );
        t.insert("inputSchema".to_string(), JsonValue::Object(schema));
        tools.push(JsonValue::Object(t));
    }

    // 4. fmd.capabilities
    {
        let mut t = BTreeMap::new();
        t.insert(
            "name".to_string(),
            JsonValue::String("fmd.capabilities".to_string()),
        );
        t.insert(
            "description".to_string(),
            JsonValue::String(
                "Discover the stable feature contract, theme models, and exit codes.".to_string(),
            ),
        );
        let mut schema = BTreeMap::new();
        schema.insert("type".to_string(), JsonValue::String("object".to_string()));
        schema.insert("properties".to_string(), JsonValue::Object(BTreeMap::new()));
        t.insert("inputSchema".to_string(), JsonValue::Object(schema));
        tools.push(JsonValue::Object(t));
    }

    // 5. fmd.render_file
    {
        let mut t = BTreeMap::new();
        t.insert(
            "name".to_string(),
            JsonValue::String("fmd.render_file".to_string()),
        );
        t.insert(
            "description".to_string(),
            JsonValue::String(
                "Render a local Markdown file using the host shell filesystem policy, writing to an output file or returning content."
                    .to_string(),
            ),
        );
        let mut schema = BTreeMap::new();
        schema.insert("type".to_string(), JsonValue::String("object".to_string()));
        let mut props = BTreeMap::new();
        props.insert(
            "path".to_string(),
            make_prop("string", "Local path to Markdown file"),
        );
        props.insert(
            "to".to_string(),
            make_prop(
                "string",
                "Output format ('html', 'pdf', 'both', 'epub', 'svg')",
            ),
        );
        props.insert(
            "out".to_string(),
            make_prop("string", "Optional destination output file path"),
        );
        schema.insert("properties".to_string(), JsonValue::Object(props));
        schema.insert(
            "required".to_string(),
            JsonValue::Array(vec![JsonValue::String("path".to_string())]),
        );
        t.insert("inputSchema".to_string(), JsonValue::Object(schema));
        tools.push(JsonValue::Object(t));
    }

    let mut res = BTreeMap::new();
    res.insert("tools".to_string(), JsonValue::Array(tools));
    JsonValue::Object(res)
}

fn make_prop(prop_type: &str, desc: &str) -> JsonValue {
    let mut map = BTreeMap::new();
    map.insert("type".to_string(), JsonValue::String(prop_type.to_string()));
    map.insert(
        "description".to_string(),
        JsonValue::String(desc.to_string()),
    );
    JsonValue::Object(map)
}

pub fn handle_tool_call(
    params: Option<&JsonValue>,
    max_input_bytes: u64,
) -> Result<JsonValue, (i32, String, &'static str)> {
    let params_obj = params.and_then(|p| p.as_object()).ok_or_else(|| {
        (
            INVALID_PARAMS,
            "Invalid params: expected object with 'name' and 'arguments'".to_string(),
            "invalid_params",
        )
    })?;

    let tool_name = params_obj
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| {
            (
                INVALID_PARAMS,
                "Invalid params: missing tool name string".to_string(),
                "invalid_params",
            )
        })?;

    let default_args = JsonValue::Object(BTreeMap::new());
    let args = params_obj.get("arguments").unwrap_or(&default_args);

    let start_time = Instant::now();
    let result = match tool_name {
        "fmd.render_html" => handle_render_html(args, max_input_bytes),
        "fmd.render_pdf" => handle_render_pdf(args, max_input_bytes),
        "fmd.verify" => handle_verify(args, max_input_bytes),
        "fmd.capabilities" => handle_capabilities(),
        "fmd.render_file" => handle_render_file(args, max_input_bytes),
        _ => Err((
            METHOD_NOT_FOUND,
            format!("Unknown tool: '{tool_name}'"),
            "method_not_found",
        )),
    };

    let elapsed = start_time.elapsed();
    eprintln!("fmd mcp tool '{}' completed in {:?}", tool_name, elapsed);
    result
}

fn handle_render_html(
    args: &JsonValue,
    max_input_bytes: u64,
) -> Result<JsonValue, (i32, String, &'static str)> {
    let md = args
        .get("markdown")
        .and_then(|m| m.as_str())
        .ok_or_else(|| {
            (
                ERROR_INVALID_OPTIONS,
                "Missing required argument: 'markdown'".to_string(),
                "missing_markdown",
            )
        })?;

    if md.len() as u64 > max_input_bytes {
        return Err((
            ERROR_INPUT_TOO_LARGE,
            format!(
                "Markdown input length {} exceeds limit {}",
                md.len(),
                max_input_bytes
            ),
            "input_too_large",
        ));
    }

    let mut opts = HtmlOptions::default();
    if let Some(f_str) = args.get("font").and_then(|f| f.as_str()) {
        opts.theme.font = FontFamily::parse(f_str).ok_or_else(|| {
            (
                ERROR_INVALID_OPTIONS,
                format!("Invalid font '{f_str}', expected 'sans' or 'serif'"),
                "invalid_font",
            )
        })?;
    }
    if let Some(css) = args.get("customCss").and_then(|c| c.as_str()) {
        opts.custom_css = Some(css.to_string());
    }
    if let Some(title) = args.get("title").and_then(|t| t.as_str()) {
        opts.title = Some(title.to_string());
    }
    if let Some(lang) = args.get("lang").and_then(|l| l.as_str()) {
        opts.lang = Some(lang.to_string());
    }
    if let Some(allow) = args.get("allowRawHtml").and_then(|a| a.as_bool()) {
        opts.allow_raw_html = allow;
    }
    if let Some(toc) = args.get("toc").and_then(|t| t.as_bool()) {
        opts.toc = toc;
    }
    if let Some(scale_str) = args.get("fontScale").and_then(|s| s.as_str()) {
        let scale = FontScale::parse(scale_str).ok_or_else(|| {
            (
                ERROR_INVALID_OPTIONS,
                format!("Invalid fontScale '{scale_str}'"),
                "invalid_font_scale",
            )
        })?;
        opts.theme = opts.theme.with_font_scale(scale);
    }
    if let Some(fmt_str) = args.get("htmlFontFormat").and_then(|f| f.as_str()) {
        opts.html_font_format = match fmt_str.to_ascii_lowercase().as_str() {
            "woff1" | "woff" => HtmlFontFormat::Woff1,
            "ttf" => HtmlFontFormat::Ttf,
            _ => {
                return Err((
                    ERROR_INVALID_OPTIONS,
                    format!("Invalid htmlFontFormat '{fmt_str}', expected 'woff1' or 'ttf'"),
                    "invalid_html_font_format",
                ));
            }
        };
    }

    let is_interactive = args
        .get("interactiveHtml")
        .and_then(|i| i.as_bool())
        .unwrap_or(false);
    let doc = parse_markdown(md);
    let html = if is_interactive {
        crate::interactive::render_interactive_html(&doc, md, &opts)
    } else {
        render_html_document(&doc, &opts).map_err(|e| {
            (
                ERROR_RENDER_FAILED,
                format!("HTML render failed: {e}"),
                "render_failed",
            )
        })?
    };

    Ok(wrap_text_content(&html))
}

fn handle_render_pdf(
    args: &JsonValue,
    max_input_bytes: u64,
) -> Result<JsonValue, (i32, String, &'static str)> {
    let md = args
        .get("markdown")
        .and_then(|m| m.as_str())
        .ok_or_else(|| {
            (
                ERROR_INVALID_OPTIONS,
                "Missing required argument: 'markdown'".to_string(),
                "missing_markdown",
            )
        })?;

    if md.len() as u64 > max_input_bytes {
        return Err((
            ERROR_INPUT_TOO_LARGE,
            format!(
                "Markdown input length {} exceeds limit {}",
                md.len(),
                max_input_bytes
            ),
            "input_too_large",
        ));
    }

    let mut opts = PdfOptions::default();
    if let Some(f_str) = args.get("font").and_then(|f| f.as_str()) {
        opts.theme.font = FontFamily::parse(f_str).ok_or_else(|| {
            (
                ERROR_INVALID_OPTIONS,
                format!("Invalid font '{f_str}', expected 'sans' or 'serif'"),
                "invalid_font",
            )
        })?;
    }
    if let Some(title) = args.get("title").and_then(|t| t.as_str()) {
        opts.title = Some(title.to_string());
    }
    if let Some(author) = args.get("author").and_then(|a| a.as_str()) {
        opts.author = Some(author.to_string());
    }
    if let Some(lang) = args.get("lang").and_then(|l| l.as_str()) {
        opts.lang = Some(lang.to_string());
    }
    if let Some(fit) = args.get("fitToPages").and_then(|p| p.as_u64()) {
        opts.fit_to_pages = Some(fit as usize);
    }
    if let Some(homo) = args.get("typographyHomogeneous").and_then(|h| h.as_bool()) {
        opts.gradual_demerits = homo;
    }
    if let Some(line_num) = args.get("codeLineNumbers").and_then(|l| l.as_bool()) {
        opts.code_line_numbers = line_num;
    }
    if let Some(page_num) = args.get("pageNumbers").and_then(|p| p.as_bool()) {
        opts.page_numbers = page_num;
    }
    if let Some(toc) = args.get("toc").and_then(|t| t.as_bool()) {
        opts.toc = toc;
    }
    if let Some(depth) = args.get("tocDepth").and_then(|d| d.as_u64()) {
        opts.toc_depth = Some(depth as u8);
    }
    if let Some(epoch) = args.get("metadataEpochSeconds").and_then(|e| e.as_u64()) {
        opts.metadata_epoch_seconds = Some(epoch);
    }
    if let Some(scale_str) = args.get("fontScale").and_then(|s| s.as_str()) {
        let scale = FontScale::parse(scale_str).ok_or_else(|| {
            (
                ERROR_INVALID_OPTIONS,
                format!("Invalid fontScale '{scale_str}'"),
                "invalid_font_scale",
            )
        })?;
        opts.theme = opts.theme.with_font_scale(scale);
    }

    let pdf_a_mode = args.get("pdfA").and_then(|a| a.as_str());
    let pdf_a_strict = args
        .get("pdfAStrict")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    let doc = parse_markdown(md);
    let pdf_bytes = if let Some(mode_str) = pdf_a_mode {
        let mode = PdfAMode::parse(mode_str).ok_or_else(|| {
            (
                ERROR_INVALID_OPTIONS,
                format!("Invalid pdfA '{mode_str}'"),
                "invalid_pdf_a",
            )
        })?;
        let settings = PdfASettings {
            mode,
            strict: pdf_a_strict,
        };
        render_pdf_document_pdfa(&doc, &opts, settings).map_err(|e| {
            (
                ERROR_RENDER_FAILED,
                format!("PDF/A render failed: {e}"),
                "render_failed",
            )
        })?
    } else {
        render_pdf_document(&doc, &opts).map_err(|e| {
            (
                ERROR_RENDER_FAILED,
                format!("PDF render failed: {e}"),
                "render_failed",
            )
        })?
    };

    let b64 = base64_encode(&pdf_bytes);
    Ok(wrap_text_content(&b64))
}

fn handle_verify(
    args: &JsonValue,
    max_input_bytes: u64,
) -> Result<JsonValue, (i32, String, &'static str)> {
    let md = args
        .get("markdown")
        .and_then(|m| m.as_str())
        .ok_or_else(|| {
            (
                ERROR_INVALID_OPTIONS,
                "Missing required argument: 'markdown'".to_string(),
                "missing_markdown",
            )
        })?;

    if md.len() as u64 > max_input_bytes {
        return Err((
            ERROR_INPUT_TOO_LARGE,
            format!(
                "Markdown input length {} exceeds limit {}",
                md.len(),
                max_input_bytes
            ),
            "input_too_large",
        ));
    }

    let a11y_only = args.get("a11y").and_then(|a| a.as_bool()).unwrap_or(false);
    let doc = parse_markdown(md);
    let opts = PdfOptions::default();
    let report = crate::verify::verify_pdf(&doc, &opts).ok_or_else(|| {
        (
            ERROR_RENDER_FAILED,
            "Verification failed: cannot load fonts".to_string(),
            "verify_failed",
        )
    })?;

    let json_text = if a11y_only {
        let filtered = crate::verify::filter_a11y(report);
        crate::verify::to_json(&filtered)
    } else {
        crate::verify::to_json(&report)
    };

    Ok(wrap_text_content(&json_text))
}

fn handle_capabilities() -> Result<JsonValue, (i32, String, &'static str)> {
    let cap_json = format!(
        "{{\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"outputs\":[\"html\",\"pdf\",\"both\",\"epub\",\"svg\"],\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"features\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"mcp\":\"available_stdio_jsonrpc\"}}}}",
        env!("CARGO_PKG_VERSION"),
        Theme::default().to_config_json()
    );
    Ok(wrap_text_content(&cap_json))
}

fn handle_render_file(
    args: &JsonValue,
    max_input_bytes: u64,
) -> Result<JsonValue, (i32, String, &'static str)> {
    let path_str = args.get("path").and_then(|p| p.as_str()).ok_or_else(|| {
        (
            ERROR_INVALID_OPTIONS,
            "Missing required argument: 'path'".to_string(),
            "missing_path",
        )
    })?;

    let path = Path::new(path_str);
    if !path.exists() {
        return Err((
            ERROR_INPUT_ERROR,
            format!("File not found: '{}'", path.display()),
            "file_not_found",
        ));
    }

    let metadata = std::fs::metadata(path).map_err(|e| {
        (
            ERROR_INPUT_ERROR,
            format!("Cannot stat '{}': {e}", path.display()),
            "io_error",
        )
    })?;

    if metadata.len() > max_input_bytes {
        return Err((
            ERROR_INPUT_TOO_LARGE,
            format!(
                "File size {} exceeds limit {}",
                metadata.len(),
                max_input_bytes
            ),
            "input_too_large",
        ));
    }

    let src = std::fs::read_to_string(path).map_err(|e| {
        (
            ERROR_INPUT_ERROR,
            format!("Cannot read '{}': {e}", path.display()),
            "io_error",
        )
    })?;

    let to = args
        .get("to")
        .and_then(|t| t.as_str())
        .unwrap_or("html")
        .to_ascii_lowercase();

    let out_path = args.get("out").and_then(|o| o.as_str());

    match to.as_str() {
        "html" => {
            let opts = HtmlOptions {
                title: Some(
                    path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ),
                ..Default::default()
            };
            let doc = parse_markdown(&src);
            let html = render_html_document(&doc, &opts).map_err(|e| {
                (
                    ERROR_RENDER_FAILED,
                    format!("HTML render failed: {e}"),
                    "render_failed",
                )
            })?;
            if let Some(dest) = out_path {
                std::fs::write(dest, &html).map_err(|e| {
                    (
                        ERROR_INPUT_ERROR,
                        format!("Cannot write '{}': {e}", dest),
                        "write_error",
                    )
                })?;
                Ok(wrap_text_content(&format!(
                    "Rendered HTML written to {dest}"
                )))
            } else {
                Ok(wrap_text_content(&html))
            }
        }
        "pdf" => {
            let opts = PdfOptions {
                title: Some(
                    path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ),
                ..Default::default()
            };
            let doc = parse_markdown(&src);
            let pdf_bytes = render_pdf_document(&doc, &opts).map_err(|e| {
                (
                    ERROR_RENDER_FAILED,
                    format!("PDF render failed: {e}"),
                    "render_failed",
                )
            })?;
            if let Some(dest) = out_path {
                std::fs::write(dest, &pdf_bytes).map_err(|e| {
                    (
                        ERROR_INPUT_ERROR,
                        format!("Cannot write '{}': {e}", dest),
                        "write_error",
                    )
                })?;
                Ok(wrap_text_content(&format!(
                    "Rendered PDF written to {dest}"
                )))
            } else {
                Ok(wrap_text_content(&base64_encode(&pdf_bytes)))
            }
        }
        _ => Err((
            ERROR_INVALID_OPTIONS,
            format!("Unsupported output target: '{to}'"),
            "unsupported_target",
        )),
    }
}

fn wrap_text_content(text: &str) -> JsonValue {
    let mut content_item = BTreeMap::new();
    content_item.insert("type".to_string(), JsonValue::String("text".to_string()));
    content_item.insert("text".to_string(), JsonValue::String(text.to_string()));

    let mut res = BTreeMap::new();
    res.insert(
        "content".to_string(),
        JsonValue::Array(vec![JsonValue::Object(content_item)]),
    );
    res.insert("isError".to_string(), JsonValue::Bool(false));
    JsonValue::Object(res)
}

// ---------------------------------------------------------------------------
// Main MCP Stdio Dispatcher Loop
// ---------------------------------------------------------------------------

pub fn run_stdio_server(max_input_bytes: u64) -> Result<(), RenderError> {
    eprintln!(
        "fmd mcp stdio server started (protocol {})",
        MCP_PROTOCOL_VERSION
    );
    let mut reader = BufReader::new(std::io::stdin());
    let mut stdout = std::io::stdout();
    let mut request_count = 0usize;

    loop {
        let frame = match read_frame(&mut reader, MAX_FRAME_BYTES) {
            Ok(Some(f)) => f,
            Ok(None) => break, // EOF reached cleanly
            Err(e) => {
                eprintln!("fmd mcp frame read error: {e}");
                let err_resp = jsonrpc_error(
                    None,
                    PARSE_ERROR,
                    &format!("Frame read error: {e}"),
                    "frame_error",
                );
                let _ = write_frame(&mut stdout, &err_resp);
                continue;
            }
        };

        request_count += 1;
        let response_str = match parse_jsonrpc_request(&frame) {
            Ok(req) => {
                match req.method.as_str() {
                    "initialize" => {
                        let mut res = BTreeMap::new();
                        res.insert(
                            "protocolVersion".to_string(),
                            JsonValue::String(MCP_PROTOCOL_VERSION.to_string()),
                        );
                        let mut cap = BTreeMap::new();
                        cap.insert("tools".to_string(), JsonValue::Object(BTreeMap::new()));
                        res.insert("capabilities".to_string(), JsonValue::Object(cap));
                        let mut server_info = BTreeMap::new();
                        server_info
                            .insert("name".to_string(), JsonValue::String("fmd".to_string()));
                        server_info.insert(
                            "version".to_string(),
                            JsonValue::String(env!("CARGO_PKG_VERSION").to_string()),
                        );
                        res.insert("serverInfo".to_string(), JsonValue::Object(server_info));

                        let id = req.id.as_ref().unwrap_or(&JsonValue::Null);
                        Some(jsonrpc_success(id, JsonValue::Object(res)))
                    }
                    "notifications/initialized" | "initialized" => {
                        // Handshake notification - no response required
                        None
                    }
                    "ping" => {
                        let id = req.id.as_ref().unwrap_or(&JsonValue::Null);
                        Some(jsonrpc_success(id, JsonValue::Object(BTreeMap::new())))
                    }
                    "tools/list" => {
                        let id = req.id.as_ref().unwrap_or(&JsonValue::Null);
                        Some(jsonrpc_success(id, tools_list_result()))
                    }
                    "tools/call" => {
                        let id = req.id.as_ref().unwrap_or(&JsonValue::Null);
                        match handle_tool_call(req.params.as_ref(), max_input_bytes) {
                            Ok(tool_res) => Some(jsonrpc_success(id, tool_res)),
                            Err((code, msg, reason)) => {
                                Some(jsonrpc_error(Some(id), code, &msg, reason))
                            }
                        }
                    }
                    _ => {
                        let id = req.id.as_ref();
                        Some(jsonrpc_error(
                            id,
                            METHOD_NOT_FOUND,
                            &format!("Method not found: '{}'", req.method),
                            "method_not_found",
                        ))
                    }
                }
            }
            Err((id, code, msg, reason)) => Some(jsonrpc_error(id.as_ref(), code, &msg, reason)),
        };

        if let Some(resp) = response_str {
            if let Err(e) = write_frame(&mut stdout, &resp) {
                eprintln!("fmd mcp frame write error: {e}");
                break;
            }
        }
    }

    eprintln!("fmd mcp stdio server closed cleanly (handled {request_count} requests)");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn json_parser_handles_primitives_and_nested_structures() {
        let raw = r#"{
            "num": 42,
            "float": 3.1415,
            "neg": -10,
            "str": "hello\nworld",
            "bool_t": true,
            "bool_f": false,
            "null_val": null,
            "arr": [1, "two", true, null],
            "nested": { "key": "value" }
        }"#;

        let val = parse_json(raw).expect("parse json");
        assert_eq!(val.get("num").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(
            val.get("str").and_then(|v| v.as_str()),
            Some("hello\nworld")
        );
        assert_eq!(val.get("bool_t").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(val.get("null_val"), Some(&JsonValue::Null));

        let reserialized = val.to_json_string();
        let roundtrip = parse_json(&reserialized).expect("roundtrip parse");
        assert_eq!(val, roundtrip);
    }

    #[test]
    fn read_write_framed_messages_roundtrip() {
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let mut buf = Vec::new();
        write_frame(&mut buf, msg).expect("write frame");

        let mut cursor = std::io::Cursor::new(buf);
        let read = read_frame(&mut cursor, MAX_FRAME_BYTES)
            .expect("read frame")
            .expect("some message");
        assert_eq!(read, msg);
    }

    #[test]
    fn tools_list_returns_valid_schema_declarations() {
        let list = tools_list_result();
        let tools = list
            .get("tools")
            .and_then(|t| match t {
                JsonValue::Array(a) => Some(a),
                _ => None,
            })
            .expect("tools array");

        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"fmd.render_html"));
        assert!(names.contains(&"fmd.render_pdf"));
        assert!(names.contains(&"fmd.verify"));
        assert!(names.contains(&"fmd.capabilities"));
        assert!(names.contains(&"fmd.render_file"));
    }

    #[test]
    fn tool_call_render_html_produces_valid_html() {
        let mut args_map = BTreeMap::new();
        args_map.insert(
            "markdown".to_string(),
            JsonValue::String("# Hello MCP\n\nTesting HTML".to_string()),
        );
        args_map.insert("font".to_string(), JsonValue::String("serif".to_string()));

        let mut params = BTreeMap::new();
        params.insert(
            "name".to_string(),
            JsonValue::String("fmd.render_html".to_string()),
        );
        params.insert("arguments".to_string(), JsonValue::Object(args_map));

        let res = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
            .expect("tool call");
        let content = res.get("content").expect("content");
        let text = match content {
            JsonValue::Array(arr) => arr[0].get("text").and_then(|t| t.as_str()).expect("text"),
            _ => panic!("expected array"),
        };
        assert!(text.contains("<!DOCTYPE html>"));
        assert!(text.contains("Hello MCP"));
        assert!(text.contains("Testing HTML"));
    }

    #[test]
    fn tool_call_render_pdf_produces_valid_base64() {
        let mut args_map = BTreeMap::new();
        args_map.insert(
            "markdown".to_string(),
            JsonValue::String("# Hello PDF".to_string()),
        );

        let mut params = BTreeMap::new();
        params.insert(
            "name".to_string(),
            JsonValue::String("fmd.render_pdf".to_string()),
        );
        params.insert("arguments".to_string(), JsonValue::Object(args_map));

        let res = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
            .expect("tool call");
        let content = res.get("content").expect("content");
        let b64 = match content {
            JsonValue::Array(arr) => arr[0].get("text").and_then(|t| t.as_str()).expect("text"),
            _ => panic!("expected array"),
        };
        assert!(!b64.is_empty());
        // PDF magic %PDF- base64 starts with JVBER
        assert!(b64.starts_with("JVBER"));
    }

    #[test]
    fn tool_call_verify_produces_json_report() {
        let mut args_map = BTreeMap::new();
        args_map.insert(
            "markdown".to_string(),
            JsonValue::String("# Title\n\nSome text".to_string()),
        );

        let mut params = BTreeMap::new();
        params.insert(
            "name".to_string(),
            JsonValue::String("fmd.verify".to_string()),
        );
        params.insert("arguments".to_string(), JsonValue::Object(args_map));

        let res = handle_tool_call(Some(&JsonValue::Object(params)), DEFAULT_MAX_INPUT_BYTES)
            .expect("verify call");
        let content = res.get("content").expect("content");
        let text = match content {
            JsonValue::Array(arr) => arr[0].get("text").and_then(|t| t.as_str()).expect("text"),
            _ => panic!("expected array"),
        };
        assert!(text.contains("\"schema_version\""));
        assert!(text.contains("\"verdict\""));
    }

    #[test]
    fn json_parser_handles_utf16_surrogate_pairs() {
        // \uD83D\uDE00 is 😀 (U+1F600)
        let json = r#"{"emoji":"\uD83D\uDE00","clef":"\uD834\uDD1E"}"#;
        let val = parse_json(json).expect("parse surrogates");
        assert_eq!(val.get("emoji").and_then(|v| v.as_str()), Some("😀"));
        assert_eq!(val.get("clef").and_then(|v| v.as_str()), Some("𝄞"));
    }

    #[test]
    fn read_frame_accepts_lowercase_content_length() {
        let msg = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}";
        let raw = format!("content-length: {}\r\n\r\n{msg}", msg.len());
        let mut cursor = std::io::Cursor::new(raw.as_bytes());
        let read = read_frame(&mut cursor, MAX_FRAME_BYTES)
            .expect("read frame")
            .expect("some message");
        assert_eq!(read, msg);
    }
}
