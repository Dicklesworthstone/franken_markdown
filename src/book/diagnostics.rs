//! Parser findings captured with the exact chapter text that produced the AST.
//!
//! These are deliberately separate from navigation validation. Expansion and
//! URL rebasing can change byte positions. Expanded ranges stay unchanged;
//! a separate bounded sidecar records proven original-source attribution.

use std::collections::BTreeSet;

use super::source_locations::{BookSourceLocation, SourceMap};
use super::{BookInput, paths};
use crate::{DiagnosticSeverity, Document, ParseDiagnostic, RenderError, Result};

const MAX_CHAPTERS: usize = 4096;
const MAX_DIAGNOSTICS: usize = 4096;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_REPORT_BYTES: usize = 4 * 1024 * 1024;
const MAX_PROVENANCE_BYTES: usize = 2 * 1024 * 1024;

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
    source_locations: Vec<Vec<BookSourceLocation>>,
    diagnostic_count: usize,
    json: String,
}

impl BookSourceDiagnostics {
    pub(super) fn new(chapters: Vec<BookChapterDiagnostics>) -> Result<Self> {
        Self::build(chapters, None)
    }

    /// Recompose attribution without parsing. Used even when an include edit
    /// moves source ranges but leaves the expanded chapter bytes unchanged.
    pub(crate) fn with_source_maps(self, maps: &[SourceMap]) -> Result<Self> {
        Self::build(self.chapters, Some(maps))
    }

    fn build(chapters: Vec<BookChapterDiagnostics>, maps: Option<&[SourceMap]>) -> Result<Self> {
        if chapters.is_empty() || chapters.len() > MAX_CHAPTERS {
            return Err(invalid("expected 1..4096 chapter reports"));
        }
        if maps.is_some_and(|maps| maps.len() != chapters.len()) {
            return Err(invalid("source provenance does not match chapter count"));
        }
        let mut source_bytes = 0usize;
        let mut diagnostic_count = 0usize;
        let mut source_locations = Vec::with_capacity(chapters.len());
        let mut location_bytes = 0usize;
        let mut location_ranges = 0usize;
        let mut paths_seen = BTreeSet::new();
        let total_diagnostics = chapters.iter().try_fold(0usize, |total, chapter| {
            total
                .checked_add(chapter.diagnostics.len())
                .filter(|&total| total <= MAX_DIAGNOSTICS)
                .ok_or_else(|| invalid("more than 4096 parser diagnostics"))
        })?;
        let mut remaining_locations = total_diagnostics;
        let mut json = Json::new();
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
            let identity;
            let map = if let Some(maps) = maps {
                &maps[index]
            } else {
                identity = SourceMap::identity(&chapter.path, chapter.source_bytes);
                &identity
            };
            if map.len() != chapter.source_bytes {
                return Err(invalid("source provenance does not match chapter bytes"));
            }
            let mut locations = Vec::with_capacity(chapter.diagnostics.len());
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
                let mut location = map.locate(diagnostic.span.start..diagnostic.span.end);
                if let BookSourceLocation::Mapped(ranges) = &location {
                    let bytes = ranges.iter().map(|range| range.path.len()).sum::<usize>();
                    if bytes > (256 * 1024usize).saturating_sub(location_bytes)
                        || ranges.len() > (16_384usize).saturating_sub(location_ranges)
                    {
                        location = BookSourceLocation::Unavailable("report_provenance_limit");
                    } else {
                        location_bytes += bytes;
                        location_ranges += ranges.len();
                    }
                }
                remaining_locations -= 1;
                let mut extra = Json::new();
                write_location(&mut extra, &location)?;
                // Every remaining finding must be able to state that its
                // optional provenance is unavailable. This allowance is wholly
                // separate from the unchanged 4 MiB base-report admission.
                let reserved = remaining_locations * 128;
                if extra.0.len() + 10 > MAX_PROVENANCE_BYTES.saturating_sub(json.2 + reserved) {
                    location = BookSourceLocation::Unavailable("report_provenance_limit");
                    extra = Json::new();
                    write_location(&mut extra, &location)?;
                }
                json.provenance(",\"source\":")?;
                json.provenance(&extra.0)?;
                locations.push(location);
                json.raw("}")?;
            }
            source_locations.push(locations);
            json.raw("]}")?;
        }
        json.raw("],\"summary\":{\"chapters\":")?;
        json.raw(&chapters.len().to_string())?;
        json.raw(",\"diagnostics\":")?;
        json.raw(&diagnostic_count.to_string())?;
        json.raw("}}")?;
        Ok(Self {
            chapters,
            source_locations,
            diagnostic_count,
            json: json.0,
        })
    }

    /// Every chapter in current reading order, including chapters with no findings.
    #[must_use]
    pub fn chapters(&self) -> &[BookChapterDiagnostics] {
        &self.chapters
    }

    /// Original source attribution for the indexed finding. Existing chapter
    /// and diagnostic structs retain their public construction/access shape.
    /// A mapped warning can name multiple original files or source fragments.
    #[must_use]
    pub fn source_location(
        &self,
        chapter: usize,
        diagnostic: usize,
    ) -> Option<&BookSourceLocation> {
        self.source_locations.get(chapter)?.get(diagnostic)
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

    /// Complete, prevalidated JSON: at most 4 MiB of original report data plus
    /// a separately bounded 2 MiB source-attribution sidecar. Source-location
    /// exhaustion cannot reject an otherwise admitted parser report.
    /// This is source inspection,
    /// not a navigation or output-conformance report.
    #[must_use]
    pub fn to_json(&self) -> String {
        self.json.clone()
    }
}

fn write_location(json: &mut Json, location: &BookSourceLocation) -> Result<()> {
    match location {
        BookSourceLocation::Unavailable(reason) => {
            json.raw("{\"status\":\"unavailable\",\"reason\":")?;
            json.string(reason)?;
            json.raw("}")
        }
        BookSourceLocation::Mapped(ranges) => {
            json.raw("{\"status\":\"mapped\",\"ranges\":[")?;
            for (index, range) in ranges.iter().enumerate() {
                if index != 0 {
                    json.raw(",")?;
                }
                json.raw("{\"path\":")?;
                json.string(&range.path)?;
                json.raw(",\"start\":")?;
                json.raw(&range.start.to_string())?;
                json.raw(",\"end\":")?;
                json.raw(&range.end.to_string())?;
                json.raw(",\"kind\":")?;
                json.string(range.kind.name())?;
                json.raw("}")?;
            }
            json.raw("]}")
        }
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
struct Json(String, usize, usize);

impl Json {
    fn new() -> Self {
        Self(String::new(), 0, 0)
    }

    fn raw(&mut self, value: &str) -> Result<()> {
        if value.len() > MAX_REPORT_BYTES.saturating_sub(self.1) {
            return Err(invalid("serialized report exceeds 4 MiB"));
        }
        self.0.push_str(value);
        self.1 += value.len();
        Ok(())
    }

    fn provenance(&mut self, value: &str) -> Result<()> {
        if value.len() > MAX_PROVENANCE_BYTES.saturating_sub(self.2) {
            return Err(invalid(
                "source attribution exceeds its separate 2 MiB allowance",
            ));
        }
        self.0.push_str(value);
        self.2 += value.len();
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod source_location_budget_tests {
    use super::*;

    #[test]
    fn optional_provenance_does_not_reject_a_near_limit_base_report() {
        let diagnostic = ParseDiagnostic::warning_with_code(
            crate::span::SourceSpan { start: 0, end: 1 },
            "example",
            "warning",
        );
        let chapter = BookChapterDiagnostics {
            path: "a.md".into(),
            source_bytes: 1,
            diagnostics: vec![diagnostic],
        };
        let tiny = BookSourceDiagnostics::new(vec![chapter.clone()]).unwrap();
        let mut extra = Json::new();
        write_location(&mut extra, tiny.source_location(0, 0).unwrap()).unwrap();
        let base_bytes = tiny.to_json().len() - 10 - extra.0.len();
        let mut chapter = chapter;
        chapter.path = format!("{}a.md", "x".repeat(MAX_REPORT_BYTES - base_bytes - 8));
        let report = BookSourceDiagnostics::new(vec![chapter.clone()]).unwrap();
        assert!(report.to_json().len() > MAX_REPORT_BYTES);
        assert!(report.to_json().len() <= MAX_REPORT_BYTES + MAX_PROVENANCE_BYTES);
        assert_eq!(
            report.source_location(0, 0),
            Some(&BookSourceLocation::Unavailable("provenance_limit"))
        );
        assert_eq!(report.chapters()[0].diagnostics, chapter.diagnostics);

        chapter.path.insert_str(0, "sixteen_more_byte");
        assert!(
            BookSourceDiagnostics::new(vec![chapter])
                .unwrap_err()
                .to_string()
                .contains("serialized report exceeds 4 MiB")
        );
    }
}
