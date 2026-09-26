//! Minimal native LSP, reusing the first-party JSON codec and Markdown parser.
//! URIs are opaque buffer keys: no network, filesystem reads, or transclusion.

mod diagnostics;
mod links;
mod navigation;
mod text;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};

use franken_markdown::mcp::{JsonValue as Json, parse_json, read_frame, write_frame};
#[cfg(test)]
use franken_markdown::parse_markdown_spanned;
use text::{Buffer, LineIndex, MAX_DOCUMENT_BYTES, Position, integer};

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_SESSION_BYTES: usize = 16 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 64;

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(fields.into_iter().map(|(key, value)| (key.to_string(), value)).collect())
}
fn string(value: &str) -> Json { Json::String(value.to_string()) }
fn number(value: usize) -> Json { Json::Number(value as f64) }
fn position(value: Position) -> Json {
    object([("line", number(value.line)), ("character", number(value.character))])
}
fn response(id: Json, result: Json) -> Json {
    object([("jsonrpc", string("2.0")), ("id", id), ("result", result)])
}
fn error(id: Json, code: i32, message: &str) -> Json {
    object([
        ("jsonrpc", string("2.0")), ("id", id),
        ("error", object([("code", Json::Number(f64::from(code))), ("message", string(message))])),
    ])
}
fn notification(method: &str, params: Json) -> Json {
    object([("jsonrpc", string("2.0")), ("method", string(method)), ("params", params)])
}
fn log(message: &str) -> Json {
    notification("window/logMessage", object([("type", number(2)), ("message", string(message))]))
}
fn text_field<'a>(value: &'a Json, key: &str) -> Result<&'a str, &'static str> {
    value.get(key).and_then(Json::as_str).ok_or("missing or invalid string parameter")
}
fn version(value: &Json) -> Result<i32, &'static str> {
    value.get("version").and_then(integer).ok_or("document version must be an LSP integer")
}

#[derive(Default, PartialEq, Eq)]
enum Phase { #[default] New, Running, Shutdown }

#[derive(Default)]
struct Server {
    phase: Phase,
    documents: BTreeMap<String, Buffer>,
    exit: Option<bool>,
    hierarchical_symbols: bool,
    folding_limit: Option<usize>,
}

impl Server {
    fn handle(&mut self, message: Json) -> Vec<Json> {
        let id = message.get("id").cloned();
        let valid_id = id.as_ref().is_none_or(|id| matches!(id, Json::String(_)) || integer(id).is_some());
        let Some(method) = message.get("method").and_then(Json::as_str) else {
            // Client responses to server messages need no response of their own.
            if message.get("jsonrpc").and_then(Json::as_str) == Some("2.0")
                && id.is_some() && valid_id
                && (message.get("result").is_some() || message.get("error").is_some())
            { return Vec::new(); }
            return vec![error(Json::Null, -32600, "Invalid Request")];
        };
        if message.get("jsonrpc").and_then(Json::as_str) != Some("2.0") || !valid_id {
            return vec![error(Json::Null, -32600, "Invalid Request")];
        }
        if method == "exit" && id.is_none() {
            self.exit = Some(self.phase == Phase::Shutdown);
            return Vec::new();
        }
        if method == "initialize" {
            let Some(id) = id else { return Vec::new(); };
            if self.phase != Phase::New {
                return vec![error(id, -32600, "Server is already initialized")];
            }
            if !message.get("params").is_some_and(|p| p.as_object().is_some()) {
                return vec![error(id, -32602, "initialize requires object params")];
            }
            let capabilities = message.get("params").and_then(|p| p.get("capabilities"))
                .and_then(|p| p.get("textDocument"));
            self.hierarchical_symbols = capabilities.and_then(|c| c.get("documentSymbol"))
                .and_then(|c| c.get("hierarchicalDocumentSymbolSupport"))
                .and_then(Json::as_bool).unwrap_or(false);
            self.folding_limit = capabilities.and_then(|c| c.get("foldingRange"))
                .and_then(|c| c.get("rangeLimit")).and_then(Json::as_u64)
                .map(|n| n.min(navigation::MAX_NAVIGATION_ITEMS as u64) as usize);
            self.phase = Phase::Running;
            return vec![response(id, object([
                ("capabilities", object([
                    ("positionEncoding", string("utf-16")),
                    ("documentSymbolProvider", Json::Bool(true)),
                    ("foldingRangeProvider", Json::Bool(true)),
                    ("selectionRangeProvider", Json::Bool(true)),
                    ("definitionProvider", Json::Bool(true)),
                    ("completionProvider", object([
                        ("triggerCharacters", Json::Array(vec![string("#")])),
                        ("resolveProvider", Json::Bool(false)),
                    ])),
                    ("textDocumentSync", object([
                        ("openClose", Json::Bool(true)), ("change", number(2)),
                        ("save", object([("includeText", Json::Bool(false))])),
                    ])),
                ])),
                ("serverInfo", object([("name", string("fmd-lsp")), ("version", string(env!("CARGO_PKG_VERSION")))])),
            ]))];
        }
        if self.phase != Phase::Running {
            return id.map(|id| error(id, if self.phase == Phase::New { -32002 } else { -32600 },
                "Server is not running")).into_iter().collect();
        }
        if method == "shutdown" {
            if let Some(id) = id {
                self.phase = Phase::Shutdown;
                self.documents.clear();
                return vec![response(id, Json::Null)];
            }
            return Vec::new();
        }
        let empty = object([]);
        let params = message.get("params").unwrap_or(&empty);
        if let Some(id) = id {
            if matches!(method, "textDocument/documentSymbol" | "textDocument/foldingRange" | "textDocument/selectionRange" | "textDocument/completion" | "textDocument/definition") {
                return vec![match self.navigate(method, params) {
                    Ok(result) => response(id, result),
                    Err((code, reason)) => error(id, code, reason),
                }];
            }
            return vec![error(id, -32601, "Method not found")];
        }
        let result = match method {
            "textDocument/didOpen" => self.open(params),
            "textDocument/didChange" => self.change(params),
            "textDocument/didClose" => self.close(params),
            // Save does not change the authoritative buffer; every accepted
            // open/change already publishes diagnostics. Unknown notifications
            // (including cancellation in this synchronous server) are ignored.
            _ => return Vec::new(),
        };
        result.unwrap_or_else(|reason| vec![log(reason)])
    }

    fn navigate(&self, method: &str, params: &Json) -> Result<Json, (i32, &'static str)> {
        let document = params.get("textDocument").ok_or((-32602, "missing textDocument"))?;
        let uri = text_field(document, "uri").map_err(|reason| (-32602, reason))?;
        let buffer = self.documents.get(uri).ok_or((-32602, "document is not open"))?;
        if !buffer.synchronized {
            return Err((-32801, "document requires full-text resynchronization"));
        }
        if matches!(method, "textDocument/completion" | "textDocument/definition") {
            return links::request(method, params, uri, &buffer.text);
        }
        navigation::request(method, params, uri, &buffer.text, self.hierarchical_symbols,
            self.folding_limit.unwrap_or(navigation::MAX_NAVIGATION_ITEMS))
            .map_err(|reason| (-32602, reason))
    }

    fn open(&mut self, params: &Json) -> Result<Vec<Json>, &'static str> {
        let document = params.get("textDocument").ok_or("missing textDocument")?;
        let uri = text_field(document, "uri")?;
        let text = text_field(document, "text")?;
        let version = version(document)?;
        if uri.is_empty() || uri.len() > 8192 { return Err("invalid document URI"); }
        if self.documents.contains_key(uri) { return Err("document is already open"); }
        let used: usize = self.documents.values().map(|doc| doc.text.len()).sum();
        if self.documents.len() >= MAX_DOCUMENTS || text.len() > MAX_DOCUMENT_BYTES
            || text.len() > MAX_SESSION_BYTES.saturating_sub(used)
        { return Err("open document or session budget exceeded"); }
        let buffer = Buffer { text: text.to_string(), version, synchronized: true };
        let report = diagnostics::publish(uri, &buffer);
        self.documents.insert(uri.to_string(), buffer);
        Ok(vec![report])
    }

    fn change(&mut self, params: &Json) -> Result<Vec<Json>, &'static str> {
        let document = params.get("textDocument").ok_or("missing textDocument")?;
        let uri = text_field(document, "uri")?;
        let version = version(document)?;
        let other_bytes: usize = self.documents.iter()
            .filter(|(key, _)| key.as_str() != uri).map(|(_, doc)| doc.text.len()).sum();
        let buffer = self.documents.get_mut(uri).ok_or("change for unopened document")?;
        // An absent/non-array update is malformed, unlike a valid empty
        // array (a version-only update). Route it through transactional reject.
        let malformed = [Json::Null];
        let changes = match params.get("contentChanges") {
            Some(Json::Array(changes)) => changes.as_slice(),
            _ => &malformed,
        };
        match buffer.change(version, changes, MAX_SESSION_BYTES.saturating_sub(other_bytes)) {
            Ok(()) => Ok(vec![diagnostics::publish(uri, buffer)]),
            Err(reason) if !buffer.synchronized => Ok(vec![log(reason), clear_diagnostics(uri)]),
            Err(reason) => Err(reason),
        }
    }

    fn close(&mut self, params: &Json) -> Result<Vec<Json>, &'static str> {
        let document = params.get("textDocument").ok_or("missing textDocument")?;
        let uri = text_field(document, "uri")?;
        self.documents.remove(uri);
        Ok(vec![clear_diagnostics(uri)])
    }
}

fn clear_diagnostics(uri: &str) -> Json {
    notification("textDocument/publishDiagnostics", object([
        ("uri", string(uri)), ("diagnostics", Json::Array(Vec::new())),
    ]))
}


/// Framing failures are fatal; malformed JSON bodies receive a parse error and
/// the next bounded frame remains readable. The existing transport accepts
/// Content-Length input (and JSON lines); all output uses LSP Content-Length.
/// Returns true only for the normal shutdown-request / exit-notification pair.
pub fn run<R: BufRead, W: Write>(reader: &mut R, writer: &mut W) -> io::Result<bool> {
    let mut server = Server::default();
    while let Some(body) = read_frame(reader, MAX_FRAME_BYTES)? {
        let replies = match parse_json(&body) {
            Ok(message) => server.handle(message),
            Err(_) => vec![error(Json::Null, -32700, "Parse error")],
        };
        for reply in replies { write_frame(writer, &reply.to_json_string())?; }
        if let Some(success) = server.exit { return Ok(success); }
    }
    Ok(false)
}
