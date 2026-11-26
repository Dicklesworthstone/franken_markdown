//! Source previews for parser-proven link and footnote definitions.
//!
//! The existing definition providers own origin-token recognition, destination
//! resolution and ambiguity. This module only reads the returned location in
//! an already opened, synchronized buffer. No URI is opened or authored Markdown
//! emitted as active hover markup; target coordinates never become an origin range.

use std::collections::BTreeMap;

use franken_markdown::{Block, SourceSpan};

use super::{Buffer, Json, LineIndex, MAX_DOCUMENT_BYTES, Parser, Position, object, string};

type Failure = (i32, &'static str);
const MAX_PREVIEW_BYTES: usize = 8192;
const MAX_PREVIEW_LINES: usize = 48;
const MAX_HOVER_BYTES: usize = 3 * MAX_PREVIEW_BYTES;
const TRUNCATION: &str = "\n\n[Preview truncated]";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Format {
    #[default]
    Legacy,
    Plaintext,
    Markdown,
    Disabled,
}

impl Format {
    pub(super) fn negotiate(capabilities: Option<&Json>) -> Self {
        match capabilities
            .and_then(|value| value.get("hover"))
            .and_then(|value| value.get("contentFormat"))
        {
            None => Self::Legacy,
            Some(Json::Array(values)) => values
                .iter()
                .find_map(|value| match value.as_str() {
                    Some("plaintext") => Some(Self::Plaintext),
                    Some("markdown") => Some(Self::Markdown),
                    _ => None,
                })
                .unwrap_or(Self::Disabled),
            Some(_) => Self::Disabled,
        }
    }

    pub(super) fn supported(self) -> bool {
        self != Self::Disabled
    }

    fn contents(self, value: String) -> Result<Json, Failure> {
        match self {
            Self::Legacy => Ok(object([
                ("language", string("markdown")),
                ("value", string(&value)),
            ])),
            Self::Plaintext => Ok(object([
                ("kind", string("plaintext")),
                ("value", string(&value)),
            ])),
            Self::Markdown => {
                // A closing fence must contain at least as many backticks as
                // its opener. Never let authored code or HTML end our wrapper.
                let mut longest = 0usize;
                let mut current = 0usize;
                for ch in value.chars() {
                    current = if ch == '`' { current + 1 } else { 0 };
                    longest = longest.max(current);
                }
                let fence = "`".repeat((longest + 1).max(3));
                let encoded = format!("{fence}\n{value}\n{fence}");
                if encoded.len() > MAX_HOVER_BYTES {
                    return Err((-32803, "hover markup exceeds the response budget"));
                }
                Ok(object([
                    ("kind", string("markdown")),
                    ("value", string(&encoded)),
                ]))
            }
            Self::Disabled => Ok(Json::Null),
        }
    }
}

/// Keep source spelling and line endings, stopping at a scalar boundary.
/// The marker is included in the byte ceiling; the line ceiling counts source
/// lines, treating CRLF as one line break and honoring lone CR as the editor does.
fn preview(source: &str) -> String {
    let source = source.trim_matches(['\r', '\n']);
    let mut end = 0;
    let mut lines = 1;
    let mut previous_cr = false;
    for (offset, ch) in source.char_indices() {
        let next = offset + ch.len_utf8();
        if next > MAX_PREVIEW_BYTES - TRUNCATION.len() {
            break;
        }
        let newline = ch == '\r' || (ch == '\n' && !previous_cr);
        if newline {
            if lines == MAX_PREVIEW_LINES {
                break;
            }
            lines += 1;
        }
        previous_cr = ch == '\r';
        end = next;
    }
    let mut result = source[..end].trim_end().to_string();
    if end < source.len() {
        result.push_str(TRUNCATION);
    }
    result
}

/// Enlarge only real Markdown heading blocks into their source sections.
/// Nested headings and notes keep the truthful enclosing primary block span;
/// they never acquire guessed inline or independently nested source locations.
fn source_preview(
    parser: Parser,
    source: &str,
    selected: SourceSpan,
    format: Format,
) -> Result<Json, Failure> {
    let document = parser.spanned(source);
    let landing = selected.start == 0 && selected.end == 0;
    let owner = if landing {
        (!document.blocks.is_empty()).then_some(0)
    } else {
        document.blocks.iter().position(|block| {
            block.span.start <= selected.start
                && selected.start < block.span.end
                && selected.end <= block.span.end
        })
    };
    let Some(owner) = owner else {
        return Ok(Json::Null);
    };
    let block = &document.blocks[owner];
    let level = match &block.node {
        Block::Heading { level, .. } => Some(*level),
        _ if landing => Some(6),
        _ => None,
    };
    let end = match level {
        Some(level) => document
            .blocks
            .iter()
            .skip(owner + 1)
            .find(|next| matches!(&next.node, Block::Heading { level: next, .. } if *next <= level))
            .map_or(source.len(), |next| next.span.start),
        None => block.span.end,
    };
    let text = source
        .get(block.span.start..end)
        .ok_or((-32803, "hover source span could not be validated"))?;
    let value = preview(text);
    if value.is_empty() {
        return Ok(Json::Null);
    }
    // Every negotiated representation keeps source links, HTML and command
    // destinations as inert text, including legacy and Markdown-only clients.
    Ok(object([("contents", format.contents(value)?)]))
}

pub(super) fn request(
    parser: Parser,
    documents: &BTreeMap<String, Buffer>,
    definition: &Json,
    format: Format,
) -> Result<Json, Failure> {
    if !format.supported() || matches!(definition, Json::Null) {
        return Ok(Json::Null);
    }
    let uri = definition
        .get("uri")
        .and_then(Json::as_str)
        .ok_or((-32803, "hover definition has no document URI"))?;
    let buffer = documents
        .get(uri)
        .ok_or((-32803, "hover target is not an opened document"))?;
    if !buffer.synchronized {
        return Err((-32801, "hover target requires full-text resynchronization"));
    }
    if buffer.text.len() > MAX_DOCUMENT_BYTES {
        return Err((-32803, "target document exceeds the hover budget"));
    }
    let range = definition
        .get("range")
        .ok_or((-32803, "hover definition has no source range"))?;
    let index = LineIndex::new(&buffer.text);
    let offset = |side| {
        let value = range
            .get(side)
            .ok_or((-32803, "hover definition range is incomplete"))?;
        let position = Position::parse(value).map_err(|reason| (-32803, reason))?;
        index
            .offset(&buffer.text, position)
            .map_err(|reason| (-32803, reason))
    };
    let start = offset("start")?;
    let end = offset("end")?;
    if end < start {
        return Err((-32803, "hover definition range is reversed"));
    }
    source_preview(parser, &buffer.text, SourceSpan::new(start, end), format)
}

#[cfg(test)]
#[path = "hover_tests.rs"]
mod tests;
