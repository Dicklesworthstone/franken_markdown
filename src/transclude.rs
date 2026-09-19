//! Transclusion (bead qpqv): `{{#include relative/path.md}}` splices another
//! Markdown file's content into the source at that line, recursively.
//!
//! The pure render core never touches the filesystem. A host resolver owns
//! path sandboxing and bounded file reads. Expansion uses one output buffer,
//! checks its budget before every append, and bounds depth and resolver calls.
//! Top-level fenced and indented code remains literal: documentation examples
//! cannot trigger include resolution merely by containing a directive.
//!
//! Select one line (`file.md:3`), an inclusive range (`file.md:3:8`), an
//! open-ended range (`file.md:3:` or `file.md::8`), or a named snippet
//! (`file.md:example`). Named snippets use comment-only `ANCHOR: example`
//! and `ANCHOR_END: example` lines. Quote a path to disambiguate literal
//! colons: `{{#include "file:with:colons.md":2:4}}`.
//! Selectors are removed before calling the resolver, so hosts continue to
//! receive only paths and canonical parent origins.

use crate::{RenderError, Result};
use std::collections::BTreeMap;
use std::ops::Range;

const INCLUDE_PREFIX: &str = "{{#include";
const MAX_DEPTH: usize = 16;
const MAX_RESOLUTIONS: usize = 4096;
const MAX_MAPPING_SPANS: usize = 262_144;

/// Default expanded-document byte budget (64 MiB).
pub const DEFAULT_MAX_EXPANDED_BYTES: usize = 64 * 1024 * 1024;

/// True when the source may contain an include directive.
#[inline(always)]
#[must_use]
pub fn has_includes(src: &str) -> bool {
    src.contains(INCLUDE_PREFIX)
}

/// Content and canonical origin key, missing file, or a host policy refusal.
/// The resolver must bound its own reads; returned strings already occupy
/// memory before the expansion engine can check their emitted size.
pub type ResolveResult = std::result::Result<Option<(String, String)>, String>;

/// Expand include directives with a 64 MiB output budget.
///
/// `resolver(path, origin)` returns content and a canonical key. Nested paths
/// resolve against that key; the root origin is `<input>`. Repeated inclusion
/// outside the active chain is allowed. Canonical keys plus selected byte
/// ranges identify cycles, so different snippets of the same file can include
/// each other without a false cycle. Only selected content is expanded.
/// Named snippets remove their boundary lines and nested anchor marker lines;
/// line selections preserve original UTF-8 bytes and line endings.
///
/// # Errors
/// Reports missing/malformed includes, cycles, depth over 16, more than 4096
/// resolver calls, output over 64 MiB, invalid selectors, missing/ambiguous
/// anchors, out-of-bounds line ranges, and host policy refusals. No partial
/// expanded document is returned. Use [`expand_includes_with_limit`] to supply
/// a different output budget.
pub fn expand_includes(
    src: &str,
    resolver: &dyn Fn(&str, &str) -> ResolveResult,
) -> Result<String> {
    expand_includes_with_limit(src, resolver, DEFAULT_MAX_EXPANDED_BYTES)
}

/// Expand into a single shared output buffer, with an explicit byte budget.
///
/// The limit applies to the final UTF-8 bytes, including inserted newlines.
/// It is checked before copying, not after materializing an oversized result.
/// Depth and resolution-count limits apply even when included content is empty.
/// No filesystem, network, clock, or process access occurs here.
///
/// # Errors
/// Same errors as [`expand_includes`], using `max_output_bytes` for output size.
pub fn expand_includes_with_limit(
    src: &str,
    resolver: &dyn Fn(&str, &str) -> ResolveResult,
    max_output_bytes: usize,
) -> Result<String> {
    let mut expansion = Expansion {
        resolver,
        output: String::with_capacity(src.len().min(max_output_bytes)),
        stack: Vec::new(),
        resolutions: 0,
        max_output_bytes,
        mapping: None,
    };
    expansion.expand(src, 0, "<input>", false, 0)?;
    Ok(expansion.output)
}

/// A contiguous segment of expanded output and its original source bytes.
/// Segments tile the output in order; removed directives and anchor marker
/// lines have no segment. `source_id` indexes [`ExpandedDocument::sources`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionSpan {
    pub expanded: Range<usize>,
    pub source_id: usize,
    pub original: Range<usize>,
    /// True only for separators inserted by expansion. Their original range
    /// is empty and points to the end of the selected source fragment.
    pub generated: bool,
}

/// Expanded Markdown plus an exact byte map to canonical host source keys.
/// Source keys are interned in deterministic first-visit order. No source file
/// contents are retained or reread; hosts retain their own immutable snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedDocument {
    pub text: String,
    pub sources: Vec<String>,
    pub spans: Vec<ExpansionSpan>,
}

/// An original position corresponding to one expanded UTF-8 character boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpansionLocation<'a> {
    pub origin: &'a str,
    pub byte_offset: usize,
    pub generated: bool,
}

impl ExpandedDocument {
    /// Resolve an expanded byte offset in O(log(number of spans)). Returns
    /// `None` for EOF, out-of-bounds positions, and UTF-8 continuation bytes.
    /// Generated separators resolve to the selected fragment's original end,
    /// with `generated = true`, rather than claiming an invented source byte.
    #[must_use]
    pub fn source_at(&self, offset: usize) -> Option<ExpansionLocation<'_>> {
        if offset >= self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }
        let index = self.spans.partition_point(|span| span.expanded.end <= offset);
        let span = self.spans.get(index)?;
        if !span.expanded.contains(&offset) {
            return None;
        }
        let byte_offset = if span.generated {
            span.original.start
        } else {
            let original = span.original.start.checked_add(offset - span.expanded.start)?;
            if original >= span.original.end {
                return None;
            }
            original
        };
        Some(ExpansionLocation {
            origin: self.sources.get(span.source_id)?,
            byte_offset,
            generated: span.generated,
        })
    }
}

/// Expand Markdown and retain original-file byte positions for diagnostics,
/// selections, and preview navigation, using the default 64 MiB output limit.
/// `root_origin` is the canonical key of `src` and is passed to the resolver
/// for root-level includes. It is metadata, not filesystem authorization.
///
/// # Errors
/// All errors from [`expand_includes`]. Also rejects source maps exceeding
/// 262,144 non-coalescible spans. No partial text or mapping is returned.
pub fn expand_includes_mapped(
    src: &str,
    root_origin: &str,
    resolver: &dyn Fn(&str, &str) -> ResolveResult,
) -> Result<ExpandedDocument> {
    expand_includes_mapped_with_limit(src, root_origin, resolver, DEFAULT_MAX_EXPANDED_BYTES)
}

/// As [`expand_includes_mapped`], with an explicit expanded-byte budget.
/// Existing string-only APIs use the same engine without allocating a map.
///
/// # Errors
/// Same errors as [`expand_includes_mapped`], using `max_output_bytes`.
pub fn expand_includes_mapped_with_limit(
    src: &str,
    root_origin: &str,
    resolver: &dyn Fn(&str, &str) -> ResolveResult,
    max_output_bytes: usize,
) -> Result<ExpandedDocument> {
    let mut expansion = Expansion {
        resolver,
        output: String::with_capacity(src.len().min(max_output_bytes)),
        stack: Vec::new(),
        resolutions: 0,
        max_output_bytes,
        mapping: Some(ExpansionMapping::default()),
    };
    expansion.expand(src, 0, root_origin, false, 0)?;
    let mapping = expansion.mapping.unwrap_or_default();
    Ok(ExpandedDocument {
        text: expansion.output,
        sources: mapping.sources,
        spans: mapping.spans,
    })
}

#[derive(Default)]
struct ExpansionMapping {
    sources: Vec<String>,
    ids: BTreeMap<String, usize>,
    spans: Vec<ExpansionSpan>,
}

impl ExpansionMapping {
    fn source_id(&mut self, origin: &str) -> usize {
        if let Some(&id) = self.ids.get(origin) {
            return id;
        }
        let id = self.sources.len();
        self.sources.push(origin.to_string());
        self.ids.insert(origin.to_string(), id);
        id
    }

    fn record(
        &mut self,
        expanded: Range<usize>,
        source_id: usize,
        original: Range<usize>,
        generated: bool,
    ) -> Result<()> {
        if expanded.is_empty() {
            return Ok(());
        }
        if let Some(last) = self.spans.last_mut() {
            let adjacent = if generated {
                last.original == original
            } else {
                last.original.end == original.start
            };
            if last.source_id == source_id && last.generated == generated
                && last.expanded.end == expanded.start && adjacent
            {
                last.expanded.end = expanded.end;
                if !generated {
                    last.original.end = original.end;
                }
                return Ok(());
            }
        }
        if self.spans.len() == MAX_MAPPING_SPANS {
            return Err(RenderError::InvalidInput(format!(
                "include_map_budget: more than {MAX_MAPPING_SPANS} source mapping spans"
            )));
        }
        self.spans.push(ExpansionSpan { expanded, source_id, original, generated });
        Ok(())
    }
}

struct Expansion<'a> {
    resolver: &'a dyn Fn(&str, &str) -> ResolveResult,
    output: String,
    stack: Vec<ActiveInclude>,
    resolutions: usize,
    max_output_bytes: usize,
    mapping: Option<ExpansionMapping>,
}

struct ActiveInclude {
    origin: String,
    range: Range<usize>,
    label: String,
}

impl Expansion<'_> {
    fn source_id(&mut self, origin: &str) -> usize {
        self.mapping.as_mut().map(|mapping| mapping.source_id(origin)).unwrap_or(0)
    }

    fn append(
        &mut self,
        text: &str,
        source_id: usize,
        original_start: usize,
        generated: bool,
    ) -> Result<()> {
        if text.len() > self.max_output_bytes.saturating_sub(self.output.len()) {
            return Err(RenderError::InvalidInput(format!(
                "include_size: expanded document exceeds {} bytes",
                self.max_output_bytes
            )));
        }
        if let Some(mapping) = &mut self.mapping {
            let original_end = if generated { original_start } else { original_start + text.len() };
            mapping.record(
                self.output.len()..self.output.len() + text.len(), source_id,
                original_start..original_end, generated,
            )?;
        }
        self.output.push_str(text);
        Ok(())
    }

    fn expand(
        &mut self,
        src: &str,
        depth: usize,
        origin: &str,
        strip_anchors: bool,
        source_offset: usize,
    ) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(RenderError::InvalidInput(format!(
                "include_depth: include nesting exceeds {MAX_DEPTH} levels (origin {origin})"
            )));
        }
        let source_id = self.source_id(origin);
        if !strip_anchors && !has_includes(src) {
            return self.append(src, source_id, source_offset, false);
        }
        let mut line_offset = source_offset;
        let mut fence: Option<(u8, usize)> = None;
        for line in src.split_inclusive('\n') {
            let original_start = line_offset;
            line_offset += line.len();
            if strip_anchors && anchor_marker(line).is_some() {
                continue;
            }
            let candidate = fence_candidate(line);
            if let Some((marker, length)) = fence {
                if candidate.is_some_and(|(next, count, tail)| {
                    next == marker && count >= length
                        && tail.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
                }) {
                    fence = None;
                }
                self.append(line, source_id, original_start, false)?;
                continue;
            }
            if let Some((marker, length, tail)) = candidate {
                if marker == b'~' || !tail.contains('`') {
                    fence = Some((marker, length));
                    self.append(line, source_id, original_start, false)?;
                    continue;
                }
            }
            if indented_code(line) {
                self.append(line, source_id, original_start, false)?;
                continue;
            }
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix(INCLUDE_PREFIX) else {
                self.append(line, source_id, original_start, false)?;
                continue;
            };
            // A longer directive name is ordinary text, not an include.
            if rest.chars().next().is_some_and(|ch| !ch.is_whitespace() && ch != '}') {
                self.append(line, source_id, original_start, false)?;
                continue;
            }
            let target = rest.strip_suffix("}}").map(str::trim).filter(|p| !p.is_empty())
                .ok_or_else(|| RenderError::InvalidInput(format!(
                    "include_missing: malformed or empty include directive in {origin}: {trimmed}"
                )))?;
            let (path, selection) = parse_target(target)?;
            if self.resolutions == MAX_RESOLUTIONS {
                return Err(RenderError::InvalidInput(format!(
                    "include_budget: more than {MAX_RESOLUTIONS} include resolutions"
                )));
            }
            self.resolutions += 1;
            let (content, resolved) = match (self.resolver)(path, origin) {
                Ok(Some(pair)) => pair,
                Ok(None) => {
                    let mut chain: Vec<_> = self.stack.iter().map(|entry| entry.label.clone()).collect();
                    chain.push(target.to_string());
                    return Err(RenderError::InvalidInput(format!(
                        "include_missing: cannot read {path} (chain: {})", chain.join(" -> ")
                    )));
                }
                Err(reason) => return Err(RenderError::InvalidInput(reason)),
            };
            let range = selection.range(&content, &resolved)?;
            let label = selection.label(&resolved);
            if self.stack.iter().any(|entry| entry.origin == resolved && entry.range == range) {
                let mut chain: Vec<_> = self.stack.iter().map(|entry| entry.label.clone()).collect();
                chain.push(label);
                return Err(RenderError::InvalidInput(format!(
                    "include_cycle: {} forms an include cycle", chain.join(" -> ")
                )));
            }
            let start = self.output.len();
            self.stack.push(ActiveInclude {
                origin: resolved.clone(),
                range: range.clone(),
                label,
            });
            let result = self.expand(
                &content[range.clone()], depth + 1, &resolved,
                matches!(selection, Selection::Anchor(_)), range.start,
            );
            self.stack.pop();
            result?;
            if !self.output[start..].ends_with('\n') {
                let included_source_id = self.source_id(&resolved);
                self.append("\n", included_source_id, range.end, true)?;
            }
        }
        Ok(())
    }
}


#[derive(Debug, PartialEq, Eq)]
enum Selection {
    Whole,
    Lines { start: usize, end: Option<usize> },
    Anchor(String),
}

fn selector_error(target: &str) -> RenderError {
    RenderError::InvalidInput(format!("include_selector: invalid include target {target:?}"))
}

/// Separate host-owned paths from engine-owned selectors. Quoting is literal:
/// backslashes are path bytes, never escape sequences or sandbox bypasses.
fn parse_target(target: &str) -> Result<(&str, Selection)> {
    let (path, suffix) = if let Some(quote @ ('\'' | '"')) = target.chars().next() {
        let close = target[1..].find(quote).ok_or_else(|| selector_error(target))? + 1;
        (&target[1..close], target[close + 1..].trim())
    } else {
        // A Windows drive prefix belongs to the host path, not a selector.
        let separator = target.char_indices().find(|&(index, ch)| {
            ch == ':' && !(index == 1 && target.as_bytes()[0].is_ascii_alphabetic()
                && target.as_bytes().get(2).is_some_and(|&byte| matches!(byte, b'/' | b'\\')))
        }).map(|(index, _)| index);
        match separator {
            Some(index) => (&target[..index], &target[index..]),
            None => (target, ""),
        }
    };
    if path.is_empty() {
        return Err(selector_error(target));
    }
    if suffix.is_empty() {
        return Ok((path, Selection::Whole));
    }
    let selector = suffix.strip_prefix(':').ok_or_else(|| selector_error(target))?;
    let selection = if let Some((first, last)) = selector.split_once(':') {
        let start = if first.is_empty() { 1 } else { line_number(first, target)? };
        let end = if last.is_empty() { None } else { Some(line_number(last, target)?) };
        if end.is_some_and(|end| end < start) {
            return Err(selector_error(target));
        }
        Selection::Lines { start, end }
    } else if !selector.is_empty() && selector.bytes().all(|byte| byte.is_ascii_digit()) {
        let line = line_number(selector, target)?;
        Selection::Lines { start: line, end: Some(line) }
    } else if valid_anchor_name(selector) {
        Selection::Anchor(selector.to_string())
    } else {
        return Err(selector_error(target));
    };
    Ok((path, selection))
}

fn line_number(text: &str, target: &str) -> Result<usize> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(selector_error(target));
    }
    text.parse::<usize>().ok().filter(|&line| line > 0)
        .ok_or_else(|| selector_error(target))
}

fn valid_anchor_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

impl Selection {
    fn label(&self, origin: &str) -> String {
        match self {
            Self::Whole => origin.to_string(),
            Self::Lines { start, end: Some(end) } => format!("{origin}:{start}:{end}"),
            Self::Lines { start, end: None } => format!("{origin}:{start}:"),
            Self::Anchor(name) => format!("{origin}:{name}"),
        }
    }

    /// Select by byte offsets, without allocating a second copy of a file.
    fn range(&self, content: &str, origin: &str) -> Result<Range<usize>> {
        match self {
            Self::Whole => Ok(0..content.len()),
            Self::Lines { start, end } => {
                let mut offset = 0;
                let mut first = None;
                let mut count = 0;
                for line in content.split_inclusive('\n') {
                    count += 1;
                    if count == *start {
                        first = Some(offset);
                    }
                    offset += line.len();
                    if *end == Some(count) {
                        break;
                    }
                }
                match first {
                    Some(first) if end.is_none_or(|end| end <= count) => Ok(first..offset),
                    _ => Err(RenderError::InvalidInput(format!(
                        "include_range: {} is outside {count} available lines", self.label(origin)
                    ))),
                }
            }
            Self::Anchor(name) => {
                let mut offset = 0;
                let mut first = None;
                let mut last = None;
                for line in content.split_inclusive('\n') {
                    if let Some((closing, marker)) = anchor_marker(line) {
                        if marker == name.as_str() {
                            if closing {
                                if first.is_none() || last.is_some() {
                                    return Err(anchor_error(origin, name, "unmatched or duplicate end"));
                                }
                                last = Some(offset);
                            } else {
                                if first.is_some() {
                                    return Err(anchor_error(origin, name, "duplicate start"));
                                }
                                first = Some(offset + line.len());
                            }
                        }
                    }
                    offset += line.len();
                }
                match (first, last) {
                    (Some(first), Some(last)) => Ok(first..last),
                    (Some(_), None) => Err(anchor_error(origin, name, "missing end")),
                    _ => Err(anchor_error(origin, name, "not found")),
                }
            }
        }
    }
}

fn anchor_error(origin: &str, name: &str, reason: &str) -> RenderError {
    RenderError::InvalidInput(format!("include_anchor: {origin}:{name}: {reason}"))
}

/// Recognize only complete comment marker lines, never prose or strings
/// containing the word ANCHOR. Paired comment delimiters must close on the line.
fn anchor_marker(line: &str) -> Option<(bool, &str)> {
    let line = line.trim();
    let body = if let Some(body) = line.strip_prefix("<!--") {
        body.strip_suffix("-->")?
    } else if let Some(body) = line.strip_prefix("/*") {
        body.strip_suffix("*/")?
    } else {
        ["//", "#", "--", ";", "%", "*"].iter()
            .find_map(|&prefix| line.strip_prefix(prefix))?
    }.trim();
    let (closing, name) = if let Some(name) = body.strip_prefix("ANCHOR_END:") {
        (true, name.trim())
    } else {
        (false, body.strip_prefix("ANCHOR:")?.trim())
    };
    valid_anchor_name(name).then_some((closing, name))
}

/// Only up to three leading ASCII spaces are allowed before a fence.
fn fence_candidate(line: &str) -> Option<(u8, usize, &str)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let indent = line.bytes().take_while(|&byte| byte == b' ').count();
    if indent > 3 { return None; }
    let text = &line[indent..];
    let marker = *text.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') { return None; }
    let count = text.bytes().take_while(|&byte| byte == marker).count();
    (count >= 3).then(|| (marker, count, &text[count..]))
}

fn indented_code(line: &str) -> bool {
    let mut column = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => column += 1,
            b'\t' => column += 4 - column % 4,
            _ => break,
        }
        if column >= 4 { return true; }
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::BTreeMap;

    fn resolver(files: &[(&str, &str)]) -> impl Fn(&str, &str) -> ResolveResult {
        let map: BTreeMap<String, String> = files.iter()
            .map(|(key, value)| (key.to_string(), value.to_string())).collect();
        move |path, _origin| Ok(map.get(path).map(|text| (text.clone(), path.to_string())))
    }

    #[test]
    fn splices_content_in_place() {
        let resolve = resolver(&[("part.md", "shared **bold** text\n")]);
        assert_eq!(
            expand_includes("# A\n\n{{#include part.md}}\n\nend\n", &resolve).unwrap(),
            "# A\n\nshared **bold** text\n\nend\n"
        );
    }

    #[test]
    fn no_directive_is_identity() {
        let source = "# A\n\nno includes here\n";
        assert_eq!(expand_includes(source, &resolver(&[])).unwrap(), source);
    }

    #[test]
    fn cycle_reports_chain() {
        let resolve = resolver(&[
            ("a.md", "{{#include b.md}}\n"),
            ("b.md", "{{#include a.md}}\n"),
        ]);
        let error = expand_includes("{{#include a.md}}\n", &resolve).unwrap_err().to_string();
        assert!(error.contains("include_cycle"));
        assert!(error.contains("a.md -> b.md -> a.md"));
    }

    #[test]
    fn missing_names_chain() {
        let error = expand_includes("{{#include gone.md}}\n", &resolver(&[])).unwrap_err();
        assert!(error.to_string().contains("include_missing"));
        assert!(error.to_string().contains("gone.md"));
    }

    #[test]
    fn policy_refusal_surfaces_verbatim() {
        let resolve = |_: &str, _: &str| -> ResolveResult {
            Err("include_escape: path leaves the document root".to_string())
        };
        let error = expand_includes("{{#include ../secret.md}}\n", &resolve).unwrap_err();
        assert!(error.to_string().contains("include_escape"));
    }

    #[test]
    fn nested_include_expands_transitively() {
        let resolve = resolver(&[
            ("a.md", "A\n{{#include b.md}}\n"),
            ("b.md", "B\n{{#include c.md}}\n"),
            ("c.md", "C\n"),
        ]);
        assert_eq!(expand_includes("{{#include a.md}}\n", &resolve).unwrap(), "A\nB\nC\n");
    }

    #[test]
    fn depth_cap_errors() {
        let map: BTreeMap<_, _> = (0..20).map(|index| (
            format!("d{index}.md"), format!("{{{{#include d{}.md}}}}\n", index + 1)
        )).collect();
        let resolve = |path: &str, _: &str| {
            Ok(map.get(path).map(|content| (content.clone(), path.to_string())))
        };
        let error = expand_includes("{{#include d0.md}}\n", &resolve).unwrap_err();
        assert!(error.to_string().contains("include_depth"));
    }

    #[test]
    fn malformed_directive_errors() {
        for source in ["{{#include oops.md }\n", "{{#include }}\n"] {
            assert!(expand_includes(source, &resolver(&[])).unwrap_err()
                .to_string().contains("include_missing"));
        }
    }

    #[test]
    fn budget_counts_utf8_and_inserted_newlines_exactly() {
        let resolve = resolver(&[("part", "中")]);
        assert_eq!(expand_includes_with_limit("{{#include part}}", &resolve, 4).unwrap(), "中\n");
        assert!(expand_includes_with_limit("{{#include part}}", &resolve, 3).unwrap_err()
            .to_string().contains("include_size"));
        assert_eq!(expand_includes_with_limit("", &resolve, 0).unwrap(), "");
        assert!(expand_includes_with_limit("a", &resolve, 0).is_err());
    }

    #[test]
    fn repeated_includes_share_one_output_budget() {
        let resolve = resolver(&[("part", "12345\n")]);
        let source = "{{#include part}}\n{{#include part}}\n";
        assert_eq!(expand_includes_with_limit(source, &resolve, 12).unwrap(), "12345\n12345\n");
        assert!(expand_includes_with_limit(source, &resolve, 11).is_err());
    }

    #[test]
    fn exponential_expansion_stops_before_visiting_the_full_tree() {
        let calls = Cell::new(0usize);
        let resolve = |path: &str, _: &str| -> ResolveResult {
            calls.set(calls.get() + 1);
            let index: usize = path.parse().unwrap();
            let content = if index == 12 { "leaf\n".to_string() } else {
                format!("{{{{#include {}}}}}\n{{{{#include {}}}}}\n", index + 1, index + 1)
            };
            Ok(Some((content, path.to_string())))
        };
        let error = expand_includes_with_limit("{{#include 0}}", &resolve, 64).unwrap_err();
        assert!(error.to_string().contains("include_size"));
        assert!(calls.get() < 100, "visited {} nodes", calls.get());
    }

    #[test]
    fn resolver_call_budget_applies_to_empty_includes() {
        let calls = Cell::new(0usize);
        let resolve = |path: &str, _: &str| -> ResolveResult {
            calls.set(calls.get() + 1);
            Ok(Some((String::new(), path.to_string())))
        };
        let source = "{{#include empty}}\n".repeat(MAX_RESOLUTIONS + 1);
        let error = expand_includes_with_limit(&source, &resolve, source.len()).unwrap_err();
        assert!(error.to_string().contains("include_budget"));
        assert_eq!(calls.get(), MAX_RESOLUTIONS);
    }

    #[test]
    fn canonical_keys_allow_same_spelling_in_different_directories() {
        let resolve = |path: &str, origin: &str| -> ResolveResult {
            match (path, origin) {
                ("part", "<input>") => Ok(Some(("{{#include part}}".into(), "first/part".into()))),
                ("part", "first/part") => Ok(Some(("nested".into(), "second/part".into()))),
                _ => Ok(None),
            }
        };
        assert_eq!(expand_includes("{{#include part}}", &resolve).unwrap(), "nested\n");
    }


    #[test]
    fn literal_code_examples_never_invoke_the_resolver() {
        let calls = Cell::new(0usize);
        let resolve = |_: &str, _: &str| -> ResolveResult {
            calls.set(calls.get() + 1);
            Err("must not resolve literal code".into())
        };
        for source in [
            "```markdown\n{{#include missing.md}}\n```\n",
            "~~~ example\n{{#include missing.md}}\n~~~\n",
            "    {{#include missing.md}}\n",
            "\t{{#include missing.md}}\n",
            "  \t{{#include missing.md}}\n",
            "````markdown\n```\n{{#include missing.md}}\n````\n",
            "```\n~~~\n{{#include missing.md}}\n```\n",
            "```\n{{#include missing.md}}\n",
        ] {
            assert_eq!(expand_includes(source, &resolve).unwrap(), source);
        }
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn real_directives_resume_after_a_matching_fence() {
        let resolve = resolver(&[("real.md", "included")]);
        let source = "   ```md\r\n{{#include missing.md}}\r\n   `````\t\r\n{{#include real.md}}\n";
        let expected = "   ```md\r\n{{#include missing.md}}\r\n   `````\t\r\nincluded\n";
        assert_eq!(expand_includes(source, &resolve).unwrap(), expected);
    }

    #[test]
    fn malformed_closing_fences_do_not_reenable_includes() {
        let resolve = resolver(&[]);
        let source = "```\n``` not a close\n{{#include missing.md}}\n    ```\n{{#include missing.md}}\n```\n";
        assert_eq!(expand_includes(source, &resolve).unwrap(), source);
    }

    #[test]
    fn line_selectors_preserve_bytes_and_only_resolve_the_file_path() {
        let resolve = resolver(&[("part.md", "one\r\n中\r\nlast")]);
        for (selector, expected) in [
            ("1", "one\r\n"), ("2", "中\r\n"), ("3", "last\n"),
            ("1:2", "one\r\n中\r\n"), (":2", "one\r\n中\r\n"),
            ("2:", "中\r\nlast\n"), (":", "one\r\n中\r\nlast\n"),
        ] {
            let source = format!("{{{{#include part.md:{selector}}}}}");
            assert_eq!(expand_includes(&source, &resolve).unwrap(), expected, "{selector}");
        }
    }

    #[test]
    fn invalid_selectors_fail_before_host_resolution() {
        let calls = Cell::new(0usize);
        let resolve = |_: &str, _: &str| -> ResolveResult {
            calls.set(calls.get() + 1);
            Ok(None)
        };
        for target in ["part:0", "part:3:2", "part:1:2:3", "part:x:2", "part:",
            "part:99999999999999999999999999999999", "\"\":1", "\"part\" junk", "\"unclosed"] {
            let source = format!("{{{{#include {target}}}}}");
            assert!(expand_includes(&source, &resolve).unwrap_err().to_string()
                .contains("include_selector"), "{target}");
        }
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn out_of_bounds_is_not_silently_truncated_or_a_phantom_final_line() {
        let resolve = resolver(&[("part", "one\ntwo\n"), ("empty", "")]);
        for target in ["part:3", "part:2:3", "part:3:", "empty:1"] {
            let source = format!("{{{{#include {target}}}}}");
            assert!(expand_includes(&source, &resolve).unwrap_err().to_string()
                .contains("include_range"), "{target}");
        }
    }

    #[test]
    fn named_snippets_strip_nested_markers_and_ignore_unselected_includes() {
        let resolve = resolver(&[("part", concat!(
            "{{#include must-not-resolve}}\n",
            "<!-- ANCHOR: example -->\r\n",
            "α\r\n// ANCHOR: nested\r\nβ\r\n// ANCHOR_END: nested\r\n",
            "<!-- ANCHOR_END: example -->\r\n",
            "{{#include must-not-resolve-either}}\n"
        ))]);
        assert_eq!(expand_includes("{{#include part:example}}", &resolve).unwrap(), "α\r\nβ\r\n");
        assert_eq!(expand_includes("{{#include part:nested}}", &resolve).unwrap(), "β\r\n");
    }

    #[test]
    fn missing_duplicate_and_unbalanced_anchors_report_errors() {
        for content in [
            "no markers\n", "// ANCHOR: x\nunfinished\n", "// ANCHOR_END: x\n",
            "// ANCHOR: x\n// ANCHOR: x\n// ANCHOR_END: x\n",
            "// ANCHOR: x\na\n// ANCHOR_END: x\n// ANCHOR: x\nb\n// ANCHOR_END: x\n",
        ] {
            let resolve = resolver(&[("part", content)]);
            let error = expand_includes("{{#include part:x}}", &resolve).unwrap_err().to_string();
            assert!(error.contains("include_anchor") && error.contains("part:x"), "{error}");
        }
        assert_eq!(anchor_marker("a string says ANCHOR: x"), None);
        assert_eq!(anchor_marker("<!-- ANCHOR: x"), None);
        assert_eq!(anchor_marker("/* ANCHOR: x */"), Some((false, "x")));
        assert_eq!(anchor_marker("# ANCHOR_END: x"), Some((true, "x")));
    }

    #[test]
    fn separate_snippets_in_one_file_are_not_false_cycles() {
        let resolve = |path: &str, origin: &str| -> ResolveResult {
            assert_eq!(path, "part");
            assert!(matches!(origin, "<input>" | "canonical/part"));
            Ok(Some((concat!(
                "// ANCHOR: a\nA\n{{#include part:b}}\n// ANCHOR_END: a\n",
                "// ANCHOR: b\nB\n// ANCHOR_END: b\n"
            ).into(), "canonical/part".into())))
        };
        assert_eq!(expand_includes("{{#include part:a}}", &resolve).unwrap(), "A\nB\n");
    }

    #[test]
    fn canonical_file_and_selected_bytes_detect_alias_cycles() {
        let resolve = |_: &str, _: &str| -> ResolveResult {
            Ok(Some(("{{#include alias:1:1}}\n".into(), "same-file".into())))
        };
        let error = expand_includes("{{#include original:1}}", &resolve).unwrap_err().to_string();
        assert!(error.contains("include_cycle"), "{error}");
    }

    #[test]
    fn quoted_colon_paths_and_windows_drive_prefixes_remain_host_paths() {
        assert_eq!(parse_target(r#""dir/a:b.md":2:4"#).unwrap(),
            ("dir/a:b.md", Selection::Lines { start: 2, end: Some(4) }));
        assert_eq!(parse_target(r#"'a:b.md'"#).unwrap(), ("a:b.md", Selection::Whole));
        assert_eq!(parse_target(r"C:\docs\part.md:2").unwrap(),
            (r"C:\docs\part.md", Selection::Lines { start: 2, end: Some(2) }));
        let resolve = resolver(&[("a:b.md", "ok")]);
        assert_eq!(expand_includes(r#"{{#include "a:b.md"}}"#, &resolve).unwrap(), "ok\n");
    }

    #[test]
    fn selecting_small_content_keeps_the_output_budget_and_host_policy() {
        let content = format!("{}\n中\n{}\n", "large".repeat(1000), "large".repeat(1000));
        let resolve = resolver(&[("part", &content)]);
        assert_eq!(expand_includes_with_limit("{{#include part:2}}", &resolve, 4).unwrap(), "中\n");
        assert!(expand_includes_with_limit("{{#include part:2}}", &resolve, 3).is_err());
        let refuse = |path: &str, origin: &str| -> ResolveResult {
            assert_eq!(path, "../secret.md");
            assert_eq!(origin, "<input>");
            Err("include_escape: refused".into())
        };
        assert!(expand_includes("{{#include ../secret.md:1}}", &refuse).unwrap_err()
            .to_string().contains("include_escape: refused"));
    }

    #[test]
    fn unrelated_directive_names_are_literal() {
        let source = "{{#included part}}\n{{#includefoo}}\n";
        assert_eq!(expand_includes(source, &resolver(&[])).unwrap(), source);
    }
    fn assert_exact_map(document: &ExpandedDocument, originals: &[(&str, &str)]) {
        let originals: BTreeMap<_, _> = originals.iter().copied().collect();
        let mut covered = 0;
        for span in &document.spans {
            assert_eq!(span.expanded.start, covered, "mapping must tile the output");
            assert!(span.expanded.start < span.expanded.end);
            let original = originals[document.sources[span.source_id].as_str()];
            assert!(original.is_char_boundary(span.original.start));
            assert!(original.is_char_boundary(span.original.end));
            if span.generated {
                assert!(span.original.is_empty());
                assert!(document.text[span.expanded.clone()].bytes().all(|byte| byte == b'\n'));
            } else {
                assert_eq!(&document.text[span.expanded.clone()], &original[span.original.clone()]);
            }
            covered = span.expanded.end;
        }
        assert_eq!(covered, document.text.len());
        for (offset, _) in document.text.char_indices() {
            let location = document.source_at(offset).expect("all characters have an origin");
            assert!(originals[location.origin].is_char_boundary(location.byte_offset));
        }
        assert!(document.source_at(document.text.len()).is_none());
    }

    #[test]
    fn mapped_plain_source_coalesces_and_rejects_non_character_positions() {
        let source = "α\r\nplain\n";
        let document = expand_includes_mapped(source, "root.md", &resolver(&[])).unwrap();
        assert_eq!(document.text, source);
        assert_eq!(document.sources, ["root.md"]);
        assert_eq!(document.spans.len(), 1);
        assert_eq!(document.spans[0].original, 0..source.len());
        assert_eq!(document.source_at(0), Some(ExpansionLocation {
            origin: "root.md", byte_offset: 0, generated: false,
        }));
        assert!(document.source_at(1).is_none(), "inside a UTF-8 scalar");
        assert!(document.source_at(usize::MAX).is_none());
        assert_exact_map(&document, &[("root.md", source)]);
    }

    #[test]
    fn nested_expansion_maps_original_files_offsets_and_generated_separators() {
        let root = "root\r\n{{#include a}}\r\nend\n";
        let a = "α\n{{#include b:2}}\nω";
        let b = "skip\n中";
        let resolve = resolver(&[("a", a), ("b", b)]);
        let document = expand_includes_mapped(root, "root.md", &resolve).unwrap();
        assert_eq!(document.text, "root\r\nα\n中\nω\nend\n");
        assert_eq!(document.sources, ["root.md", "a", "b"]);
        for (text, origin, byte_offset) in [
            ("α", "a", 0), ("中", "b", b.find('中').unwrap()),
            ("ω", "a", a.find('ω').unwrap()), ("end", "root.md", root.find("end").unwrap()),
        ] {
            assert_eq!(document.source_at(document.text.find(text).unwrap()),
                Some(ExpansionLocation { origin, byte_offset, generated: false }));
        }
        let separator = document.text.find('中').unwrap() + '中'.len_utf8();
        assert_eq!(document.source_at(separator), Some(ExpansionLocation {
            origin: "b", byte_offset: b.len(), generated: true,
        }));
        assert_exact_map(&document, &[("root.md", root), ("a", a), ("b", b)]);
    }

    #[test]
    fn mapped_selectors_keep_full_file_offsets_across_stripped_markers() {
        let part = concat!(
            "outside\n<!-- ANCHOR: x -->\nα\n// ANCHOR: nested\n",
            "β\n// ANCHOR_END: nested\n<!-- ANCHOR_END: x -->\n"
        );
        let root = "{{#include part:x}}";
        let document = expand_includes_mapped(root, "root", &resolver(&[("part", part)])).unwrap();
        assert_eq!(document.text, "α\nβ\n");
        assert_eq!(document.spans.len(), 2, "removed marker bytes must not be coalesced");
        for ch in ['α', 'β'] {
            assert_eq!(document.source_at(document.text.find(ch).unwrap()),
                Some(ExpansionLocation { origin: "part", byte_offset: part.find(ch).unwrap(), generated: false }));
        }
        assert_exact_map(&document, &[("root", root), ("part", part)]);
        let root = "{{#include part:3:3}}";
        let line = expand_includes_mapped(root, "root", &resolver(&[("part", part)])).unwrap();
        assert_eq!(line.text, "α\n");
        assert_eq!(line.spans[0].original.start, part.find('α').unwrap());
        assert_exact_map(&line, &[("root", root), ("part", part)]);
    }

    #[test]
    fn repeated_includes_intern_origins_without_merging_distinct_source_occurrences() {
        let root = "{{#include part}}\n{{#include part}}\n";
        let resolve = resolver(&[("part", "data\n")]);
        let document = expand_includes_mapped(root, "root", &resolve).unwrap();
        assert_eq!(document.sources, ["root", "part"]);
        assert_eq!(document.spans.len(), 2);
        assert_eq!(document.spans[0].original, document.spans[1].original);
        assert_exact_map(&document, &[("root", root), ("part", "data\n")]);
        assert_eq!(document, expand_includes_mapped(root, "root", &resolve).unwrap());
        assert_eq!(document.text, expand_includes(root, &resolve).unwrap());
    }

    #[test]
    fn empty_sources_and_empty_includes_have_truthful_maps() {
        let resolve = resolver(&[("empty", "")]);
        let empty = expand_includes_mapped_with_limit("", "root", &resolve, 0).unwrap();
        assert_eq!(empty.sources, ["root"]);
        assert!(empty.text.is_empty() && empty.spans.is_empty());
        assert!(empty.source_at(0).is_none());
        let root = "{{#include empty}}";
        let document = expand_includes_mapped(root, "root", &resolve).unwrap();
        assert_eq!(document.text, "\n");
        assert_eq!(document.source_at(0), Some(ExpansionLocation {
            origin: "empty", byte_offset: 0, generated: true,
        }));
        assert_exact_map(&document, &[("root", root), ("empty", "")]);
        assert!(expand_includes_mapped_with_limit(root, "root", &resolve, 0).is_err());
    }

    #[test]
    fn mapped_literal_code_keeps_one_root_span_without_resolver_calls() {
        let source = "```md\n{{#include missing:x}}\n```\n    {{#include missing:2}}\n";
        let calls = Cell::new(0usize);
        let resolve = |_: &str, _: &str| -> ResolveResult {
            calls.set(calls.get() + 1);
            Ok(None)
        };
        let document = expand_includes_mapped(source, "root", &resolve).unwrap();
        assert_eq!(calls.get(), 0);
        assert_eq!(document.text, source);
        assert_eq!(document.spans.len(), 1);
        assert_exact_map(&document, &[("root", source)]);
    }

    #[test]
    fn mapping_preserves_custom_root_context_and_legacy_budget_behavior() {
        let source = "{{#include child:2}}";
        let resolve = |path: &str, origin: &str| -> ResolveResult {
            assert_eq!(path, "child");
            assert_eq!(origin, "docs/root.md");
            Ok(Some(("skip\n中".into(), "docs/child.md".into())))
        };
        let document = expand_includes_mapped(source, "docs/root.md", &resolve).unwrap();
        assert_eq!(document.sources, ["docs/root.md", "docs/child.md"]);
        let resolve = resolver(&[("child", "skip\n中")]);
        for budget in 0..=5 {
            let plain = expand_includes_with_limit(source, &resolve, budget).map_err(|e| e.to_string());
            let mapped = expand_includes_mapped_with_limit(source, "<input>", &resolve, budget)
                .map(|document| document.text).map_err(|e| e.to_string());
            assert_eq!(plain, mapped, "byte budget {budget}");
        }
    }

    #[test]
    fn mapping_budget_limits_metadata_but_allows_adjacent_coalescing() {
        let span = ExpansionSpan { expanded: 0..1, source_id: 0, original: 0..1, generated: false };
        let mut mapping = ExpansionMapping {
            spans: vec![span; MAX_MAPPING_SPANS], ..ExpansionMapping::default()
        };
        mapping.record(1..2, 0, 1..2, false).unwrap();
        assert_eq!(mapping.spans.len(), MAX_MAPPING_SPANS);
        let error = mapping.record(2..3, 1, 0..1, false).unwrap_err().to_string();
        assert!(error.contains("include_map_budget"));
        assert_eq!(mapping.spans.last().unwrap().expanded, 0..2);
    }
}
