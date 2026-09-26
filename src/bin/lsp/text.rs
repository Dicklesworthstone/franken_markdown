//! UTF-16 editor coordinates and transactional, bounded buffer edits.

use franken_markdown::mcp::JsonValue;

pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_CHANGES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

impl Position {
    pub fn parse(value: &JsonValue) -> Result<Self, &'static str> {
        let coordinate = |key| {
            value
                .get(key)
                .and_then(JsonValue::as_u64)
                .filter(|&n| n <= i32::MAX as u64)
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("position coordinates must be nonnegative LSP integers")
        };
        Ok(Self {
            line: coordinate("line")?,
            character: coordinate("character")?,
        })
    }
}

/// Line ends exclude terminators. CRLF is one newline, and both lone CR and
/// trailing empty lines have the same meaning as in an editor text document.
pub struct LineIndex {
    starts: Vec<usize>,
    ends: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        let mut ends = Vec::new();
        let bytes = text.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            if matches!(bytes[offset], b'\r' | b'\n') {
                ends.push(offset);
                if bytes[offset] == b'\r' && bytes.get(offset + 1) == Some(&b'\n') {
                    offset += 1;
                }
                starts.push(offset + 1);
            }
            offset += 1;
        }
        ends.push(text.len());
        Self { starts, ends }
    }

    pub fn offset(&self, text: &str, position: Position) -> Result<usize, &'static str> {
        let (&start, &end) = self
            .starts
            .get(position.line)
            .zip(self.ends.get(position.line))
            .ok_or("edit line is outside the document")?;
        let mut units = 0;
        for (offset, ch) in text[start..end].char_indices() {
            if units == position.character {
                return Ok(start + offset);
            }
            units += ch.len_utf16();
            if units > position.character {
                return Err("edit splits a UTF-16 surrogate pair");
            }
        }
        // LSP defines characters past the line length as its end.
        Ok(end)
    }

    pub fn position(&self, text: &str, offset: usize) -> Position {
        let offset = offset.min(text.len());
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        let mut end = offset.min(self.ends[line]);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Position {
            line,
            character: text[self.starts[line]..end].encode_utf16().count(),
        }
    }
}

pub fn integer(value: &JsonValue) -> Option<i32> {
    value.as_f64().filter(|n| {
        n.is_finite() && n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64
    }).map(|n| n as i32)
}

pub struct Buffer {
    pub text: String,
    pub version: i32,
    pub synchronized: bool,
}

impl Buffer {
    /// Each range addresses the result of the preceding edit in this batch.
    /// No live text is changed until EVERY edit has passed validation. On a
    /// rejected version, only a full replacement can restore synchronization.
    pub fn change(
        &mut self,
        version: i32,
        changes: &[JsonValue],
        byte_limit: usize,
    ) -> Result<(), &'static str> {
        if version <= self.version {
            return Err("stale document version ignored");
        }
        let result = self.prepare_changes(changes, byte_limit.min(MAX_DOCUMENT_BYTES));
        self.version = version;
        match result {
            Ok(text) => {
                self.text = text;
                self.synchronized = true;
                Ok(())
            }
            Err(error) => {
                self.synchronized = false;
                Err(error)
            }
        }
    }

    fn prepare_changes(
        &self,
        changes: &[JsonValue],
        limit: usize,
    ) -> Result<String, &'static str> {
        if changes.is_empty() || changes.len() > MAX_CHANGES {
            return Err("contentChanges must contain between 1 and 128 edits");
        }
        if !self.synchronized && changes[0].get("range").is_some() {
            return Err("buffer requires a full-text replacement after a rejected edit");
        }
        let mut text = self.text.clone();
        for change in changes {
            let replacement = change.get("text").and_then(JsonValue::as_str)
                .ok_or("each edit requires text")?;
            let (start, end) = if let Some(range) = change.get("range") {
                let start = Position::parse(range.get("start").ok_or("missing range start")?)?;
                let end = Position::parse(range.get("end").ok_or("missing range end")?)?;
                if (start.line, start.character) > (end.line, end.character) {
                    return Err("edit range is reversed");
                }
                let index = LineIndex::new(&text);
                (index.offset(&text, start)?, index.offset(&text, end)?)
            } else {
                (0, text.len())
            };
            if let Some(length) = change.get("rangeLength") {
                if change.get("range").is_none() {
                    return Err("rangeLength requires a range");
                }
                let length = length.as_u64().ok_or("invalid rangeLength")?;
                if length != text[start..end].encode_utf16().count() as u64 {
                    return Err("rangeLength does not match the replaced UTF-16 text");
                }
            }
            let retained = text.len() - (end - start);
            if retained > limit || replacement.len() > limit - retained {
                return Err("document or session byte budget exceeded");
            }
            text.replace_range(start..end, replacement);
        }
        Ok(text)
    }
}
