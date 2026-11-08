//! Parser findings captured with the exact chapter text that produced the AST.
//!
//! These are deliberately separate from navigation validation. Expansion and
//! URL rebasing can change byte positions: every range belongs to the expanded
//! chapter, never to an invented location in an original include file.

use std::collections::BTreeSet;

use super::{BookInput, paths};
use crate::{DiagnosticSeverity, Document, ParseDiagnostic, RenderError, Result};

const MAX_CHAPTERS: usize = 4096;
const MAX_DIAGNOSTICS: usize = 4096;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_REPORT_BYTES: usize = 4 * 1024 * 1024;

/// Findings for one published chapter, in its expanded UTF-8 coordinate space.
/// Include-only files have no independent entry; findings from an included
/// fragment belong to every chapter whose expanded text contains that fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookChapterDiagnostics {
    pub path: String,
    /// Expanded/rebased chapter bytes immediately before parsing, including
    /// frontmatter. This is not the book's original source-capture byte count.
    pub source_bytes: usize,
    /// The parser's unchanged codes, severity, ranges and messages, in source order.
    pub diagnostics: Vec<ParseDiagnostic>,
}

/// Complete parser findings retained alongside one immutable book revision.
///
/// Construction admits the entire report before publication. Reports are never
/// truncated, and an over-budget update cannot replace a usable book. The
/// bounded serialized form is cached so repeated host checks neither parse
/// source again nor walk all diagnostics again. No source bodies are retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSourceDiagnostics {
    chapters: Vec<BookChapterDiagnostics>,
    diagnostic_count: usize,
    json: String,
}

impl BookSourceDiagnostics {
    pub(super) fn new(chapters: Vec<BookChapterDiagnostics>) -> Result<Self> {
        if chapters.is_empty() || chapters.len() > MAX_CHAPTERS {
            return Err(invalid("expected 1..4096 chapter reports"));
        }
        let mut source_bytes = 0usize;
        let mut diagnostic_count = 0usize;
        let mut paths_seen = BTreeSet::new();
        let mut json = Json(String::new());
        json.raw("{\"schema\":\"fmd-book-source-diagnostics-v1\",\"coordinateSpace\":\"expanded_chapter\",\"chapters\":[")?;
        for (index, chapter) in chapters.iter().enumerate() {
            if paths::source_path(&chapter.path).as_deref() != Some(chapter.path.as_str())
                || !paths_seen.insert(chapter.path.as_str())
            {
                return Err(invalid("invalid, noncanonical or duplicate chapter path"));
            }
            source_bytes = source_bytes
                .checked_add(chapter.source_bytes)
                .filter(|&bytes| bytes <= MAX_SOURCE_BYTES)
                .ok_or_else(|| invalid("expanded chapter text exceeds 64 MiB"))?;
            diagnostic_count = diagnostic_count
                .checked_add(chapter.diagnostics.len())
                .filter(|&count| count <= MAX_DIAGNOSTICS)
                .ok_or_else(|| invalid("more than 4096 parser diagnostics"))?;
            if index != 0 {
                json.raw(",")?;
            }
            json.raw("{\"path\":")?;
            json.string(&chapter.path)?;
            json.raw(",\"sourceBytes\":")?;
            json.raw(&chapter.source_bytes.to_string())?;
            json.raw(",\"diagnostics\":[")?;
            for (index, diagnostic) in chapter.diagnostics.iter().enumerate() {
                if diagnostic.span.start > diagnostic.span.end
                    || diagnostic.span.end > chapter.source_bytes
                    || diagnostic.code.is_empty()
                    || diagnostic.code.len() > 64
                    || !diagnostic
                        .code
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    || diagnostic.message.is_empty()
                    || diagnostic.message.len() > 2048
                {
                    return Err(invalid("invalid diagnostic range, code or message"));
                }
                if index != 0 {
                    json.raw(",")?;
                }
                json.raw("{\"code\":")?;
                json.string(diagnostic.code)?;
                json.raw(",\"severity\":")?;
                json.string(match diagnostic.severity {
                    DiagnosticSeverity::Warning => "warning",
                    DiagnosticSeverity::Error => "error",
                })?;
                json.raw(",\"start\":")?;
                json.raw(&diagnostic.span.start.to_string())?;
                json.raw(",\"end\":")?;
                json.raw(&diagnostic.span.end.to_string())?;
                json.raw(",\"message\":")?;
                json.string(&diagnostic.message)?;
                json.raw("}")?;
            }
            json.raw("]}")?;
        }
        json.raw("],\"summary\":{\"chapters\":")?;
        json.raw(&chapters.len().to_string())?;
        json.raw(",\"diagnostics\":")?;
        json.raw(&diagnostic_count.to_string())?;
        json.raw("}}")?;
        Ok(Self {
            chapters,
            diagnostic_count,
            json: json.0,
        })
    }

    /// Every chapter in current reading order, including chapters with no findings.
    #[must_use]
    pub fn chapters(&self) -> &[BookChapterDiagnostics] {
        &self.chapters
    }

    #[must_use]
    pub const fn diagnostic_count(&self) -> usize {
        self.diagnostic_count
    }

    /// True only when all captured chapters have no parser findings.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.diagnostic_count == 0
    }

    /// Complete, prevalidated JSON, at most 4 MiB. This is source inspection,
    /// not a navigation or output-conformance report.
    #[must_use]
    pub fn to_json(&self) -> String {
        self.json.clone()
    }
}

/// Transfer the parser-owned AST and findings without cloning its block tree.
pub(super) fn parse_chapter(
    input: &BookInput,
    path: &str,
    profile: Option<crate::Profile>,
) -> (Document, BookChapterDiagnostics) {
    let mut parsed = crate::parse_markdown_spanned_with_profile(&input.source, profile);
    let report = BookChapterDiagnostics {
        path: path.to_string(),
        source_bytes: parsed.source_len,
        diagnostics: std::mem::take(&mut parsed.diagnostics),
    };
    (parsed.into_document(), report)
}

/// Shared bounded admission for the diagnosed builder, before parser work.
pub(super) fn admit_inputs(inputs: &[BookInput]) -> Result<()> {
    if inputs.is_empty() || inputs.len() > MAX_CHAPTERS {
        return Err(invalid("expected 1..4096 chapters"));
    }
    let mut bytes = 0usize;
    for input in inputs {
        for size in [input.path.len(), input.source.len()] {
            bytes = bytes
                .checked_add(size)
                .filter(|&bytes| bytes <= MAX_SOURCE_BYTES)
                .ok_or_else(|| invalid("chapter text and paths exceed 64 MiB"))?;
        }
    }
    Ok(())
}

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("book_source_diagnostics: {message}"))
}

/// Keep escaping and byte admission in one writer so control-heavy strings
/// cannot exceed the report budget after an earlier unescaped-length check.
struct Json(String);

impl Json {
    fn raw(&mut self, value: &str) -> Result<()> {
        if value.len() > MAX_REPORT_BYTES.saturating_sub(self.0.len()) {
            return Err(invalid("serialized report exceeds 4 MiB"));
        }
        self.0.push_str(value);
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<()> {
        self.raw("\"")?;
        let mut start = 0;
        for (index, ch) in value.char_indices() {
            let escaped = match ch {
                '"' => "\\\"",
                '\\' => "\\\\",
                '\n' => "\\n",
                '\r' => "\\r",
                '\t' => "\\t",
                ch if ch < ' ' => {
                    self.raw(&value[start..index])?;
                    self.raw(&format!("\\u{:04x}", ch as u32))?;
                    start = index + ch.len_utf8();
                    continue;
                }
                _ => continue,
            };
            self.raw(&value[start..index])?;
            self.raw(escaped)?;
            start = index + ch.len_utf8();
        }
        self.raw(&value[start..])?;
        self.raw("\"")
    }
}
