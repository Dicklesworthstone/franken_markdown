//! Bounded MCP JSON-lines transport, with explicit legacy LSP compatibility.
//!
//! A connection keeps the framing selected by its first message. Framing errors
//! are fatal: an oversized or truncated frame cannot be safely resynchronized.

use std::io::{self, BufRead, Write};

const MAX_HEADER_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Framing {
    JsonLines,
    ContentLength,
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Read at most `limit` bytes, leaving the next message in the input buffer.
/// Unlike `read_line`, allocation is bounded even without a newline.
fn read_line_bounded<R: BufRead>(reader: &mut R, limit: usize) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    loop {
        let available = match reader.fill_buf() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if available.is_empty() {
            return Ok(line);
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if count > limit.saturating_sub(line.len()) {
            return Err(invalid_data("MCP line exceeds its byte limit"));
        }
        line.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            return Ok(line);
        }
    }
}

fn decode_utf8(bytes: Vec<u8>) -> io::Result<String> {
    String::from_utf8(bytes).map_err(|_| invalid_data("MCP frame is not valid UTF-8"))
}

fn is_legacy_header(line: &str) -> bool {
    line.split_once(':').is_some_and(|(name, _)| {
        name.trim().eq_ignore_ascii_case("content-length")
            || name.trim().eq_ignore_ascii_case("content-type")
    })
}

fn read_content_length<R: BufRead>(
    reader: &mut R,
    mut line: Vec<u8>,
    max_frame_bytes: usize,
) -> io::Result<String> {
    let mut header_bytes = 0usize;
    let mut content_length = None;
    loop {
        if line.is_empty() || !line.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated MCP frame headers",
            ));
        }
        header_bytes = header_bytes.saturating_add(line.len());
        if header_bytes > MAX_HEADER_BYTES {
            return Err(invalid_data("MCP headers exceed their byte limit"));
        }
        let header = decode_utf8(line)?;
        let header = header.trim();
        if header.is_empty() {
            break;
        }
        let (name, value) = header
            .split_once(':')
            .ok_or_else(|| invalid_data("malformed MCP frame header"))?;
        if !name.is_ascii() || name.trim().is_empty() {
            return Err(invalid_data("invalid MCP frame header name"));
        }
        if name.trim().eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(invalid_data("duplicate Content-Length header"));
            }
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid_data("invalid Content-Length header"));
            }
            let length = value
                .parse::<usize>()
                .map_err(|_| invalid_data("Content-Length is out of range"))?;
            if length > max_frame_bytes {
                return Err(invalid_data(format!(
                    "frame length {length} exceeds maximum {max_frame_bytes}"
                )));
            }
            content_length = Some(length);
        }
        line = read_line_bounded(reader, MAX_HEADER_BYTES - header_bytes)?;
    }
    let length = content_length.ok_or_else(|| invalid_data("missing Content-Length header"))?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    decode_utf8(body)
}

pub(super) fn read_message<R: BufRead>(
    reader: &mut R,
    framing: &mut Option<Framing>,
    max_frame_bytes: usize,
) -> io::Result<Option<String>> {
    let mut preamble_bytes = 0usize;
    loop {
        // Conventional legacy headers start with C; JSON cannot. Cap these
        // before allocation, then enforce a cumulative cap for all headers.
        let first_byte = match reader.fill_buf() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?.first().copied(),
        };
        let header_candidate = *framing == Some(Framing::ContentLength)
            || (framing.is_none() && matches!(first_byte, Some(b'C' | b'c')));
        let limit = if header_candidate {
            MAX_HEADER_BYTES
        } else {
            max_frame_bytes.saturating_add(2)
        };
        let mut line = read_line_bounded(reader, limit)?;
        if line.is_empty() {
            return Ok(None);
        }
        if line
            .iter()
            .all(|&byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            preamble_bytes = preamble_bytes.saturating_add(line.len());
            if preamble_bytes > MAX_HEADER_BYTES {
                return Err(invalid_data("MCP blank preamble exceeds its byte limit"));
            }
            continue;
        }
        if *framing == Some(Framing::ContentLength) {
            return read_content_length(reader, line, max_frame_bytes).map(Some);
        }
        // Never slice UTF-8 at an arbitrary byte offset while inspecting headers.
        let first = std::str::from_utf8(&line)
            .map_err(|_| invalid_data("MCP frame is not valid UTF-8"))?;
        if framing.is_none() && is_legacy_header(first.trim()) {
            *framing = Some(Framing::ContentLength);
            return read_content_length(reader, line, max_frame_bytes).map(Some);
        }
        *framing = Some(Framing::JsonLines);
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        if line.len() > max_frame_bytes {
            return Err(invalid_data(format!(
                "frame length {} exceeds maximum {max_frame_bytes}",
                line.len()
            )));
        }
        return decode_utf8(line).map(Some);
    }
}

/// Read one JSON-lines or legacy Content-Length message. The limit counts UTF-8
/// payload bytes, excluding the line delimiter or legacy headers.
///
/// Stop reading this stream after a framing error. The server retains framing
/// across messages; this standalone helper detects it independently per call.
pub fn read_frame<R: BufRead>(reader: &mut R, max_frame_bytes: usize) -> io::Result<Option<String>> {
    read_message(reader, &mut None, max_frame_bytes)
}

/// Write a legacy Content-Length frame. Retained for existing library callers;
/// the MCP stdio server replies with JSON lines unless its client selects legacy.
pub fn write_frame<W: Write>(writer: &mut W, body: &str) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    writer.flush()
}

pub(super) fn write_message<W: Write>(
    writer: &mut W,
    framing: Framing,
    body: &str,
) -> io::Result<()> {
    match framing {
        Framing::ContentLength => write_frame(writer, body),
        Framing::JsonLines => {
            if body.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "MCP JSON-lines output contains an embedded line break",
                ));
            }
            writer.write_all(body.as_bytes())?;
            writer.write_all(b"\n")?;
            writer.flush()
        }
    }
}
