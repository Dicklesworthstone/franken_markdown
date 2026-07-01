//! Clean-room Markdown parser: line-based block parsing + a single-pass inline
//! parser. This is a focused CommonMark + GFM subset covering the constructs
//! that matter for documents (headings, paragraphs, fenced code, blockquotes,
//! lists + task lists, pipe tables, thematic breaks; emphasis/strong/strike,
//! code spans, links, images, autolinks, hard/soft breaks).
//!
//! It is deliberately not (yet) a full CommonMark implementation — full
//! reference conformance (remaining nested-list edge cases and HTML blocks) is
//! tracked in beads. The design priority is correct, fast handling of the
//! common 95% with zero dependencies and no `unwrap`/`panic`.

use std::collections::BTreeMap;

use crate::ast::{Align, Block, Document, Inline, List, ListItem, Table};
use crate::span::{ParseDiagnostic, SourceSpan, Spanned, SpannedDocument};

mod entities;

#[cfg(not(target_arch = "wasm32"))]
type ParseStageStart = std::time::Instant;
#[cfg(target_arch = "wasm32")]
type ParseStageStart = ();

#[derive(Debug, Clone)]
struct LinkReference {
    dest: String,
    title: Option<String>,
}

type ReferenceMap = BTreeMap<String, LinkReference>;

/// Parsed document plus parser stage attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseProfile {
    /// Parsed renderer AST.
    pub document: Document,
    /// Stable parser stage ledger in observation order.
    pub stages: Vec<ParseStageSummary>,
}

/// Spanned parsed document plus parser stage attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedParseProfile {
    /// Parsed document with source spans and diagnostics.
    pub document: SpannedDocument,
    /// Stable parser stage ledger in observation order.
    pub stages: Vec<ParseStageSummary>,
}

/// One measured parser stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseStageSummary {
    /// Stable stage identifier used by perf artifacts and Beads closeouts.
    pub stage: &'static str,
    /// Stage-specific work count: lines, blocks, inline input bytes, rows, etc.
    pub count: usize,
    /// Elapsed nanoseconds for this invocation. Zero on wasm32 until a browser
    /// clock provider exists.
    pub elapsed_ns: u128,
    /// Stage-specific input byte count when meaningful.
    pub bytes: usize,
    /// Approximate number of parser-owned objects/strings/vectors produced.
    pub allocations: usize,
    /// Short stable explanation for artifact readers.
    pub notes: &'static str,
}

struct ParseProfiler {
    enabled: bool,
    stages: Vec<ParseStageSummary>,
    /// Current block-nesting recursion depth, used to bound deeply-nested
    /// blockquote/list input so pathological untrusted documents cannot overflow
    /// the stack (a DoS). Threaded for free since the profiler is already `&mut`
    /// through every block-parsing call.
    block_depth: usize,
}

impl ParseProfiler {
    fn disabled() -> Self {
        Self {
            enabled: false,
            stages: Vec::new(),
            block_depth: 0,
        }
    }

    fn enabled() -> Self {
        Self {
            enabled: true,
            stages: Vec::new(),
            block_depth: 0,
        }
    }

    fn checkpoint(&self) -> Option<ParseStageStart> {
        if self.enabled {
            parse_stage_now()
        } else {
            None
        }
    }

    fn record_since(
        &mut self,
        stage: &'static str,
        count: usize,
        bytes: usize,
        allocations: usize,
        notes: &'static str,
        started: Option<ParseStageStart>,
    ) {
        if !self.enabled {
            return;
        }
        self.stages.push(ParseStageSummary {
            stage,
            count,
            elapsed_ns: parse_stage_elapsed_ns(started),
            bytes,
            allocations,
            notes,
        });
    }

    fn measure<T, F, C>(&mut self, stage: &'static str, notes: &'static str, f: F, counts: C) -> T
    where
        F: FnOnce() -> T,
        C: FnOnce(&T) -> (usize, usize, usize),
    {
        let started = self.checkpoint();
        let result = f();
        let (count, bytes, allocations) = if self.enabled {
            counts(&result)
        } else {
            (0, 0, 0)
        };
        self.record_since(stage, count, bytes, allocations, notes, started);
        result
    }

    fn finish(self) -> Vec<ParseStageSummary> {
        self.stages
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_stage_now() -> Option<ParseStageStart> {
    Some(std::time::Instant::now())
}

#[cfg(target_arch = "wasm32")]
fn parse_stage_now() -> Option<ParseStageStart> {
    Some(())
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_stage_elapsed_ns(started: Option<ParseStageStart>) -> u128 {
    started.map_or(0, |start| start.elapsed().as_nanos())
}

#[cfg(target_arch = "wasm32")]
fn parse_stage_elapsed_ns(_started: Option<ParseStageStart>) -> u128 {
    0
}

/// Parse a full Markdown document.
#[must_use]
pub fn parse_document(src: &str) -> Document {
    let mut profiler = ParseProfiler::disabled();
    parse_document_inner(src, &mut profiler)
}

/// Parse a full Markdown document and collect parser stage attribution.
#[must_use]
pub fn parse_document_profiled(src: &str) -> ParseProfile {
    let mut profiler = ParseProfiler::enabled();
    let document = parse_document_inner(src, &mut profiler);
    ParseProfile {
        document,
        stages: profiler.finish(),
    }
}

fn parse_document_inner(src: &str, profiler: &mut ParseProfiler) -> Document {
    // Normalize: strip a UTF-8 BOM; `lines()` handles both `\n` and `\r\n`.
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let lines = profiler.measure(
        "line_split",
        "strip UTF-8 BOM if present and split source into logical lines",
        || src.lines().collect::<Vec<&str>>(),
        |lines| (lines.len(), src.len(), 1),
    );
    let reference_started = profiler.checkpoint();
    let (lines, refs) = collect_link_references(&lines);
    profiler.record_since(
        "reference_collection",
        refs.len(),
        src.len(),
        refs.len() + lines.len(),
        "collect link reference definitions and remove consumed definition lines",
        reference_started,
    );
    let block_started = profiler.checkpoint();
    let blocks = parse_blocks_with_refs_profiled(&lines, &refs, profiler);
    let block_count = blocks.len();
    profiler.record_since(
        "block_parse_total",
        block_count,
        src.len(),
        block_count,
        "line classification, block assembly, and recursive block parsing",
        block_started,
    );
    Document { blocks }
}

/// Parse a document and attach top-level source spans plus recoverable parser
/// diagnostics. This is intentionally additive: the normal renderer AST remains
/// span-free.
#[must_use]
pub fn parse_document_spanned(src: &str) -> SpannedDocument {
    parse_document_spanned_inner(src, &mut ParseProfiler::disabled())
}

/// Parse a spanned document and collect parser stage attribution.
#[must_use]
pub fn parse_document_spanned_profiled(src: &str) -> SpannedParseProfile {
    let mut profiler = ParseProfiler::enabled();
    let document = parse_document_spanned_inner(src, &mut profiler);
    SpannedParseProfile {
        document,
        stages: profiler.finish(),
    }
}

fn parse_document_spanned_inner(src: &str, profiler: &mut ParseProfiler) -> SpannedDocument {
    let document = parse_document_inner(src, profiler);
    let spans = profiler.measure(
        "span_collection",
        "collect top-level source spans for editor/WASM diagnostics",
        || collect_top_level_spans(src),
        |spans| (spans.len(), src.len(), spans.len()),
    );
    let fallback = SourceSpan::new(0, src.len());
    let blocks = document
        .blocks
        .into_iter()
        .enumerate()
        .map(|(idx, block)| Spanned::new(block, spans.get(idx).copied().unwrap_or(fallback)))
        .collect();

    SpannedDocument {
        blocks,
        diagnostics: profiler.measure(
            "diagnostics_collection",
            "collect recoverable parser diagnostics such as malformed references and fences",
            || collect_parse_diagnostics(src),
            |diagnostics| (diagnostics.len(), src.len(), diagnostics.len()),
        ),
        source_len: src.len(),
    }
}

#[derive(Debug, Clone, Copy)]
struct SourceLine<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn source_lines(src: &str) -> Vec<SourceLine<'_>> {
    let mut lines = Vec::new();
    let (src, mut start) = src
        .strip_prefix('\u{feff}')
        .map_or((src, 0usize), |stripped| (stripped, '\u{feff}'.len_utf8()));

    for raw in src.split_inclusive('\n') {
        let raw_start = start;
        start += raw.len();

        let without_lf = raw.strip_suffix('\n').unwrap_or(raw);
        let text = without_lf.strip_suffix('\r').unwrap_or(without_lf);
        lines.push(SourceLine {
            text,
            start: raw_start,
            end: raw_start + text.len(),
        });
    }
    lines
}

fn collect_top_level_spans(src: &str) -> Vec<SourceSpan> {
    let raw_lines = source_lines(src);
    let consumed_reference_lines = {
        let line_texts: Vec<&str> = raw_lines.iter().map(|line| line.text).collect();
        collect_link_reference_metadata(&line_texts).0
    };
    let lines: Vec<SourceLine<'_>> = raw_lines
        .into_iter()
        .enumerate()
        .filter_map(|(idx, line)| (!consumed_reference_lines[idx]).then_some(line))
        .collect();
    let refs = ReferenceMap::new();
    let mut spans = Vec::new();
    let mut i = 0usize;

    'blocks: while i < lines.len() {
        let line = lines[i].text;
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        if is_thematic_break(line) || atx_heading(line).is_some() {
            spans.push(span_for_lines(&lines, i, i + 1));
            i += 1;
            continue;
        }

        if let Some((fence_ch, fence_len, _info)) = open_fence(line) {
            let mut end = i + 1;
            while end < lines.len() {
                if is_close_fence(lines[end].text, fence_ch, fence_len) {
                    end += 1;
                    break;
                }
                end += 1;
            }
            spans.push(span_for_lines(&lines, i, end));
            i = end;
            continue;
        }

        if indented_code_start(line) {
            let rest: Vec<&str> = lines[i..].iter().map(|line| line.text).collect();
            let (_code, used) = parse_indented_code(&rest);
            spans.push(span_for_lines(&lines, i, i + used));
            i += used;
            continue;
        }

        if line.trim_start().starts_with('>') {
            let start = i;
            while i < lines.len() && lines[i].text.trim_start().starts_with('>') {
                i += 1;
            }
            spans.push(span_for_lines(&lines, start, i));
            continue;
        }

        if let Some(end_cond) = html_block_kind(line) {
            let start = i;
            i = html_block_end(&lines, i, end_cond, |l| l.text);
            spans.push(span_for_lines(&lines, start, i));
            continue;
        }

        if i + 1 < lines.len() && line.contains('|') && is_table_delimiter(lines[i + 1].text) {
            let rest: Vec<&str> = lines[i..].iter().map(|line| line.text).collect();
            if let Some((_table, used)) = parse_table(&rest, &refs) {
                spans.push(span_for_lines(&lines, i, i + used));
                i += used;
                continue;
            }
        }

        if list_marker(line).is_some() {
            let rest: Vec<&str> = lines[i..].iter().map(|line| line.text).collect();
            let (_list, used) = parse_list(&rest, &refs);
            spans.push(span_for_lines(&lines, i, i + used));
            i += used;
            continue;
        }

        let start = i;
        while i < lines.len() && !lines[i].text.trim().is_empty() {
            if i > start && setext_underline(lines[i].text).is_some() {
                spans.push(span_for_lines(&lines, start, i + 1));
                i += 1;
                continue 'blocks;
            }
            if is_thematic_break(lines[i].text)
                || atx_heading(lines[i].text).is_some()
                || open_fence(lines[i].text).is_some()
                || indented_code_start(lines[i].text)
                || lines[i].text.trim_start().starts_with('>')
                || html_block_start(lines[i].text)
                || list_marker_interrupts_paragraph(lines[i].text)
            {
                break;
            }
            i += 1;
        }
        spans.push(span_for_lines(&lines, start, i));
    }

    spans
}

fn span_for_lines(lines: &[SourceLine<'_>], start: usize, end: usize) -> SourceSpan {
    let Some(first) = lines.get(start) else {
        return SourceSpan::default();
    };
    let Some(last) = end.checked_sub(1).and_then(|idx| lines.get(idx)) else {
        return SourceSpan::new(first.start, first.end);
    };
    SourceSpan::new(first.start, last.end)
}

fn collect_parse_diagnostics(src: &str) -> Vec<ParseDiagnostic> {
    let lines = source_lines(src);
    let mut diagnostics = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];
        if looks_like_reference_definition(line.text)
            && parse_reference_definition(line.text).is_none()
        {
            diagnostics.push(ParseDiagnostic::warning(
                SourceSpan::new(line.start, line.end),
                "malformed link reference definition rendered as text",
            ));
        }

        if let Some((fence_ch, fence_len, _info)) = open_fence(line.text) {
            let mut end = i + 1;
            let mut closed = false;
            while end < lines.len() {
                if is_close_fence(lines[end].text, fence_ch, fence_len) {
                    closed = true;
                    break;
                }
                end += 1;
            }
            if !closed {
                diagnostics.push(ParseDiagnostic::warning(
                    SourceSpan::new(line.start, src.len()),
                    "unclosed fenced code block reaches end of document",
                ));
                break;
            }
            i = end;
        }

        i += 1;
    }

    diagnostics
}

fn looks_like_reference_definition(line: &str) -> bool {
    if leading_spaces(line) > 3 {
        return false;
    }
    let t = line.trim_start();
    t.starts_with('[') && t.contains("]:")
}

fn collect_link_references<'a>(lines: &[&'a str]) -> (Vec<&'a str>, ReferenceMap) {
    let (consumed, refs) = collect_link_reference_metadata(lines);
    let mut kept = Vec::with_capacity(lines.len());
    for (idx, line) in lines.iter().enumerate() {
        if !consumed[idx] {
            kept.push(*line);
        }
    }
    (kept, refs)
}

fn collect_link_reference_metadata(lines: &[&str]) -> (Vec<bool>, ReferenceMap) {
    let mut refs = ReferenceMap::new();
    let mut consumed = vec![false; lines.len()];
    let mut i = 0usize;

    while i < lines.len() {
        // Fenced code block contents are literal text, never reference
        // definitions. Skip over the whole fence (matching the block parser's
        // own fence handling) so a `[label]: dest`-looking code line is not
        // extracted and silently deleted from the rendered code block.
        if let Some((fence_ch, fence_len, _info)) = open_fence(lines[i]) {
            i += 1;
            while i < lines.len() && !is_close_fence(lines[i], fence_ch, fence_len) {
                i += 1;
            }
            i += 1; // step past the closing fence (or past end if unclosed)
            continue;
        }
        let Some((label, mut reference)) = parse_reference_definition(lines[i]) else {
            i += 1;
            continue;
        };

        let mut used = 1usize;
        if reference.title.is_none()
            && let Some(title_line) = lines.get(i + 1)
            && let Some(title) = parse_reference_title_line(title_line)
        {
            reference.title = Some(title);
            used = 2;
        }

        refs.entry(label).or_insert(reference);
        for consumed_line in consumed.iter_mut().skip(i).take(used) {
            *consumed_line = true;
        }
        i += used;
    }

    (consumed, refs)
}

/// Defensive recursion bound for block nesting (blockquotes, lists). Real
/// documents nest only a handful of levels; this cap is far beyond any legitimate
/// use yet low enough that even a debug build (with large stack frames) cannot
/// overflow on adversarial input such as `">".repeat(100_000)`.
const MAX_BLOCK_NESTING_DEPTH: usize = 128;

/// Recurse into nested block content (a blockquote body or a list item body) with
/// a depth guard. Past [`MAX_BLOCK_NESTING_DEPTH`] levels, stop nesting and emit
/// the remaining content as one flat paragraph instead of recursing — this keeps
/// the text while making a stack-overflow DoS on deeply-nested untrusted input
/// impossible. Inline parsing is iterative (bounded), so the fallback is safe.
fn parse_blocks_bounded(
    lines: &[&str],
    refs: &ReferenceMap,
    profiler: &mut ParseProfiler,
) -> Vec<Block> {
    if profiler.block_depth >= MAX_BLOCK_NESTING_DEPTH {
        let text = lines.join("\n");
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        return vec![Block::Paragraph(parse_inlines_with_refs_profiled(
            trimmed, refs, profiler,
        ))];
    }
    profiler.block_depth += 1;
    let blocks = parse_blocks_with_refs_profiled(lines, refs, profiler);
    profiler.block_depth -= 1;
    blocks
}

fn parse_blocks_with_refs_profiled(
    lines: &[&str],
    refs: &ReferenceMap,
    profiler: &mut ParseProfiler,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = 0;
    'blocks: while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if is_thematic_break(line) {
            blocks.push(Block::ThematicBreak);
            i += 1;
            continue;
        }
        if let Some((level, text)) = atx_heading(line) {
            let started = profiler.checkpoint();
            let inlines = parse_inlines_with_refs_profiled(text, refs, profiler);
            profiler.record_since(
                "heading_block",
                1,
                line.len(),
                1 + inlines.len(),
                "parse one ATX heading block and its inline content",
                started,
            );
            blocks.push(Block::Heading { level, inlines });
            i += 1;
            continue;
        }
        if let Some((fence_ch, fence_len, info)) = open_fence(line) {
            let started = profiler.checkpoint();
            let lang = {
                let t = info.trim();
                t.split_whitespace()
                    .next()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            let mut code = String::new();
            i += 1;
            while i < lines.len() {
                if is_close_fence(lines[i], fence_ch, fence_len) {
                    i += 1;
                    break;
                }
                code.push_str(lines[i]);
                code.push('\n');
                i += 1;
            }
            profiler.record_since(
                "fenced_code_block",
                1,
                code.len(),
                usize::from(lang.is_some()) + 1,
                "parse one fenced code block body and language info",
                started,
            );
            blocks.push(Block::CodeBlock { lang, code });
            continue;
        }
        if indented_code_start(line) {
            let started = profiler.checkpoint();
            let (code, used) = parse_indented_code(&lines[i..]);
            profiler.record_since(
                "indented_code_block",
                used,
                code.len(),
                1,
                "parse one indented code block",
                started,
            );
            blocks.push(Block::CodeBlock { lang: None, code });
            i += used;
            continue;
        }
        if line.trim_start().starts_with('>') {
            let started = profiler.checkpoint();
            let mut inner = Vec::new();
            while i < lines.len() {
                if lines[i].trim_start().starts_with('>') {
                    inner.push(strip_blockquote(lines[i]));
                    i += 1;
                } else if blockquote_lazy_continuation(inner.last().map(String::as_str), lines[i]) {
                    // CommonMark lazy continuation: a non-blank, non-`>` line that
                    // does not start a new block continues the blockquote's open
                    // paragraph instead of ending the quote.
                    inner.push(lines[i].trim_start().to_string());
                    i += 1;
                } else {
                    break;
                }
            }
            let inner_refs: Vec<&str> = inner.iter().map(String::as_str).collect();
            let inner_blocks = parse_blocks_bounded(&inner_refs, refs, profiler);
            profiler.record_since(
                "blockquote_block",
                inner.len(),
                inner.iter().map(String::len).sum(),
                inner.len() + inner_blocks.len(),
                "parse one blockquote and recursively parse its inner blocks",
                started,
            );
            blocks.push(Block::BlockQuote(inner_blocks));
            continue;
        }
        if let Some(end_cond) = html_block_kind(line) {
            let started = profiler.checkpoint();
            let start = i;
            i = html_block_end(lines, i, end_cond, |l| *l);
            let html = lines[start..i].join("\n");
            profiler.record_since(
                "html_block",
                i - start,
                html.len(),
                1,
                "parse one raw HTML block",
                started,
            );
            blocks.push(Block::HtmlBlock(html));
            continue;
        }
        if i + 1 < lines.len() && line.contains('|') && is_table_delimiter(lines[i + 1]) {
            let started = profiler.checkpoint();
            if let Some((table, used)) = parse_table_profiled(&lines[i..], refs, profiler) {
                profiler.record_since(
                    "table_block",
                    used,
                    lines[i..i + used].iter().map(|line| line.len()).sum(),
                    1 + table.head.len() + table.rows.iter().map(Vec::len).sum::<usize>(),
                    "parse one pipe table block including aligned cells",
                    started,
                );
                blocks.push(Block::Table(table));
                i += used;
                continue;
            }
        }
        if list_marker(line).is_some() {
            let started = profiler.checkpoint();
            let (list, used) = parse_list_profiled(&lines[i..], refs, profiler);
            profiler.record_since(
                "list_block",
                used,
                lines[i..i + used].iter().map(|line| line.len()).sum(),
                1 + list.items.len(),
                "parse one ordered/unordered/task list block",
                started,
            );
            blocks.push(Block::List(list));
            i += used;
            continue;
        }
        // Paragraph: collect until a blank line or the start of another block.
        let start = i;
        while i < lines.len() && !lines[i].trim().is_empty() {
            if i > start
                && let Some(level) = setext_underline(lines[i])
            {
                let started = profiler.checkpoint();
                let (inlines, text_len, join_allocations) =
                    parse_lines_as_inlines(&lines[start..i], refs, profiler);
                profiler.record_since(
                    "setext_heading_block",
                    i - start + 1,
                    text_len,
                    join_allocations + 1 + inlines.len(),
                    "parse one setext heading and its inline content",
                    started,
                );
                blocks.push(Block::Heading { level, inlines });
                i += 1;
                continue 'blocks;
            }
            if is_thematic_break(lines[i])
                || atx_heading(lines[i]).is_some()
                || open_fence(lines[i]).is_some()
                || indented_code_start(lines[i])
                || lines[i].trim_start().starts_with('>')
                || html_block_start(lines[i])
                || list_marker_interrupts_paragraph(lines[i])
            {
                break;
            }
            i += 1;
        }
        let started = profiler.checkpoint();
        let (inlines, text_len, join_allocations) =
            parse_lines_as_inlines(&lines[start..i], refs, profiler);
        profiler.record_since(
            "paragraph_block",
            i - start,
            text_len,
            join_allocations + 1 + inlines.len(),
            "parse one paragraph block and its inline content",
            started,
        );
        blocks.push(Block::Paragraph(inlines));
    }
    blocks
}

fn parse_lines_as_inlines(
    lines: &[&str],
    refs: &ReferenceMap,
    profiler: &mut ParseProfiler,
) -> (Vec<Inline>, usize, usize) {
    match lines {
        [] => (Vec::new(), 0, 0),
        [line] => (
            parse_inlines_with_refs_profiled(line, refs, profiler),
            line.len(),
            0,
        ),
        _ => {
            let text = lines.join("\n");
            let len = text.len();
            (
                parse_inlines_with_refs_profiled(&text, refs, profiler),
                len,
                1,
            )
        }
    }
}

fn parse_reference_definition(line: &str) -> Option<(String, LinkReference)> {
    if leading_spaces(line) > 3 {
        return None;
    }
    let t = line.trim_start();
    let chars: Vec<char> = t.chars().collect();
    if chars.first() != Some(&'[') {
        return None;
    }
    let close = find_closing_bracket(&chars, 0)?;
    if chars.get(close + 1) != Some(&':') {
        return None;
    }
    let raw_label: String = chars[1..close].iter().collect();
    let label = normalize_reference_label(&raw_label)?;
    let mut i = close + 2;
    skip_spaces(&chars, &mut i);
    if i >= chars.len() {
        return None;
    }

    let dest = if chars[i] == '<' {
        i += 1;
        let start = i;
        while i < chars.len() && chars[i] != '>' {
            i += 1;
        }
        if i >= chars.len() {
            return None;
        }
        let dest: String = chars[start..i].iter().collect();
        i += 1;
        dest
    } else {
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        chars[start..i].iter().collect()
    };
    if dest.is_empty() {
        return None;
    }

    skip_spaces(&chars, &mut i);
    let title = if i >= chars.len() {
        None
    } else {
        let close_ch = match chars[i] {
            '"' => '"',
            '\'' => '\'',
            '(' => ')',
            _ => return None,
        };
        i += 1;
        let start = i;
        while i < chars.len() && chars[i] != close_ch {
            i += 1;
        }
        if i >= chars.len() {
            return None;
        }
        let title: String = chars[start..i].iter().collect();
        i += 1;
        skip_spaces(&chars, &mut i);
        if i != chars.len() {
            return None;
        }
        Some(title)
    };

    Some((label, LinkReference { dest, title }))
}

fn parse_reference_title_line(line: &str) -> Option<String> {
    if leading_spaces(line) > 3 {
        return None;
    }
    let t = line.trim_start();
    if t.is_empty() {
        return None;
    }
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0usize;
    let title = parse_link_title(&chars, &mut i)?;
    skip_spaces(&chars, &mut i);
    (i == chars.len()).then_some(title)
}

// ---- block detectors --------------------------------------------------------

fn atx_heading(line: &str) -> Option<(u8, &str)> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let t = &line[indent..];
    let hashes = t.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &t[hashes..];
    if !rest.is_empty() && !starts_space_or_tab(rest) {
        return None; // `#text` is not a heading
    }
    let content = atx_heading_content(rest);
    Some((hashes as u8, content))
}

fn atx_heading_content(rest: &str) -> &str {
    let raw = trim_end_space_tab(rest);
    let bytes = raw.as_bytes();
    let mut hash_start = bytes.len();
    while hash_start > 0 && bytes[hash_start - 1] == b'#' {
        hash_start -= 1;
    }

    if hash_start < bytes.len() && hash_start > 0 && is_space_or_tab_byte(bytes[hash_start - 1]) {
        return trim_space_tab(&raw[..hash_start]);
    }

    trim_space_tab(raw)
}

fn setext_underline(line: &str) -> Option<u8> {
    if leading_spaces(line) > 3 {
        return None;
    }
    let t = line.trim();
    let first = t.chars().next()?;
    let level = match first {
        '=' => 1,
        '-' => 2,
        _ => return None,
    };
    let marker_count = t.chars().filter(|&c| c == first).count();
    if marker_count > 0 && t.chars().all(|c| c == first || c == ' ') {
        Some(level)
    } else {
        None
    }
}

fn is_thematic_break(line: &str) -> bool {
    if leading_spaces(line) > 3 {
        return false;
    }
    let t = line.trim();
    if t.len() < 3 {
        return false;
    }
    for ch in ['-', '*', '_'] {
        if t.chars().all(|c| c == ch || c == ' ') && t.chars().filter(|&c| c == ch).count() >= 3 {
            return true;
        }
    }
    false
}

fn open_fence(line: &str) -> Option<(char, usize, &str)> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let t = &line[indent..];
    for ch in ['`', '~'] {
        let n = t.chars().take_while(|&c| c == ch).count();
        if n >= 3 {
            let info = &t[n..];
            // A ``` info string must not itself contain a backtick.
            if ch == '`' && info.contains('`') {
                return None;
            }
            return Some((ch, n, info));
        }
    }
    None
}

fn is_close_fence(line: &str, ch: char, len: usize) -> bool {
    let indent = leading_spaces(line);
    if indent > 3 {
        return false;
    }
    let t = &line[indent..];
    let marker_len = t.chars().take_while(|&c| c == ch).count();
    marker_len >= len && t[marker_len..].chars().all(is_space_or_tab)
}

fn indented_code_start(line: &str) -> bool {
    leading_spaces(line) >= 4
}

fn parse_indented_code(lines: &[&str]) -> (String, usize) {
    let mut code = String::new();
    let mut i = 0usize;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            let mut next = i + 1;
            while next < lines.len() && lines[next].trim().is_empty() {
                next += 1;
            }
            if next >= lines.len() || !indented_code_start(lines[next]) {
                break;
            }
            code.push('\n');
            i += 1;
            continue;
        }
        if !indented_code_start(lines[i]) {
            break;
        }
        code.push_str(strip_n(lines[i], 4));
        code.push('\n');
        i += 1;
    }
    (code, i)
}

fn strip_blockquote(line: &str) -> String {
    let t = line.trim_start();
    let rest = t.strip_prefix('>').unwrap_or(t);
    rest.strip_prefix(' ').unwrap_or(rest).to_string()
}

/// True when `line` lazily continues an open paragraph inside a block quote.
///
/// CommonMark lets a block quote's paragraph be continued by a following line
/// that omits the `>` marker ("laziness"), provided the previous quoted line was
/// open paragraph text and the continuation line would not itself start a new
/// block. `prev` is the previously collected (already `>`-stripped) quote line.
fn blockquote_lazy_continuation(prev: Option<&str>, line: &str) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    // Only an OPEN paragraph can be lazily continued: the previous quoted line
    // must be plain paragraph text, not a blank or another block's opener.
    let prev_is_open_paragraph = prev.is_some_and(|p| {
        !p.trim().is_empty()
            && !is_thematic_break(p)
            && atx_heading(p).is_none()
            && open_fence(p).is_none()
            && list_marker(p).is_none()
            && !html_block_start(p)
            && !p.trim_start().starts_with('>')
    });
    if !prev_is_open_paragraph {
        return false;
    }
    // The continuation line itself must be paragraph text, not a block starter
    // (a heading, fence, thematic break, list that interrupts, or HTML block all
    // end the quote rather than continue it).
    !is_thematic_break(line)
        && atx_heading(line).is_none()
        && open_fence(line).is_none()
        && !html_block_start(line)
        && !list_marker_interrupts_paragraph(line)
}

/// The end condition for a started HTML block (CommonMark block types 1-7).
#[derive(Clone, Copy)]
enum HtmlBlockEnd {
    /// Types 1-5: continue until (and including) the first line that contains
    /// one of these end markers — even across blank lines.
    Marker(&'static [&'static str]),
    /// Types 6-7: continue until the next blank line.
    Blank,
}

/// Classify an HTML block start and return its end condition, or `None` when the
/// line does not begin an HTML block. Types 1-5 end at a literal closing token
/// (`-->`, `</pre>`, ...) that may sit on a later line; types 6-7 end at a blank
/// line. The previous implementation blank-terminated *every* type, which split
/// `<pre>`/comment blocks at the first blank line and emitted unterminated tags.
fn html_block_kind(line: &str) -> Option<HtmlBlockEnd> {
    let t = line.trim_start();
    // Type 2: comment.
    if t.starts_with("<!--") {
        return Some(HtmlBlockEnd::Marker(&["-->"]));
    }
    // Type 5: CDATA section.
    if t.starts_with("<![CDATA[") {
        return Some(HtmlBlockEnd::Marker(&["]]>"]));
    }
    // Type 3: processing instruction.
    if t.starts_with("<?") {
        return Some(HtmlBlockEnd::Marker(&["?>"]));
    }
    // Type 4: declaration `<!` + ASCII letter (e.g. `<!DOCTYPE html>`).
    if t.starts_with("<!") {
        return if t.as_bytes().get(2).is_some_and(u8::is_ascii_alphabetic) {
            Some(HtmlBlockEnd::Marker(&[">"]))
        } else {
            // Preserve the historical bare-`<!` start (blank-terminated).
            Some(HtmlBlockEnd::Blank)
        };
    }
    // Type 1: raw-text elements (`<script>`, `<pre>`, `<style>`, `<textarea>`).
    if let Some(markers) = html_raw_text_end_markers(t) {
        return Some(HtmlBlockEnd::Marker(markers));
    }
    // Types 6-7: recognized block-level tags terminate at the next blank line.
    let name = html_tag_name(t)?;
    is_html_block_tag(name).then_some(HtmlBlockEnd::Blank)
}

fn html_block_start(line: &str) -> bool {
    html_block_kind(line).is_some()
}

/// End markers for CommonMark type-1 raw-text HTML blocks. A start matches
/// `<name` (case-insensitive) followed by whitespace, `>`, `/`, or end of line.
fn html_raw_text_end_markers(t: &str) -> Option<&'static [&'static str]> {
    const RAW: [(&str, &[&str]); 4] = [
        ("script", &["</script>"]),
        ("pre", &["</pre>"]),
        ("style", &["</style>"]),
        ("textarea", &["</textarea>"]),
    ];
    let lower = t.to_ascii_lowercase();
    for (name, markers) in RAW {
        if let Some(after) = lower.strip_prefix('<').and_then(|s| s.strip_prefix(name)) {
            match after.chars().next() {
                None => return Some(markers),
                Some(c) if c.is_whitespace() || c == '>' || c == '/' => return Some(markers),
                _ => {}
            }
        }
    }
    None
}

/// Given an HTML block that starts at `start`, return the exclusive end line
/// index per its `end` condition. `text` extracts the raw text of a line.
fn html_block_end<T>(
    lines: &[T],
    start: usize,
    end: HtmlBlockEnd,
    text: impl Fn(&T) -> &str,
) -> usize {
    match end {
        HtmlBlockEnd::Marker(markers) => {
            let mut k = start;
            while k < lines.len() {
                let lower = text(&lines[k]).to_ascii_lowercase();
                let hit = markers.iter().any(|m| lower.contains(m));
                k += 1;
                if hit {
                    break;
                }
            }
            k
        }
        HtmlBlockEnd::Blank => {
            let mut k = start + 1;
            while k < lines.len() && !text(&lines[k]).trim().is_empty() {
                k += 1;
            }
            k
        }
    }
}

fn html_tag_name(t: &str) -> Option<&str> {
    let rest = t.strip_prefix("</").or_else(|| t.strip_prefix('<'))?;
    let mut end = 0usize;
    for (idx, ch) in rest.char_indices() {
        if idx == 0 && !ch.is_ascii_alphabetic() {
            return None;
        }
        if ch.is_ascii_alphanumeric() || ch == '-' {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 { None } else { Some(&rest[..end]) }
}

fn is_html_block_tag(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "address"
            | "article"
            | "aside"
            | "base"
            | "basefont"
            | "blockquote"
            | "body"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hr"
            | "html"
            | "iframe"
            | "legend"
            | "li"
            | "link"
            | "main"
            | "menu"
            | "menuitem"
            | "nav"
            | "noframes"
            | "ol"
            | "optgroup"
            | "option"
            | "p"
            | "param"
            | "pre"
            | "script"
            | "section"
            | "style"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
    )
}

// ---- lists ------------------------------------------------------------------

struct Marker<'a> {
    indent: usize,
    ordered: bool,
    start: u64,
    content_indent: usize,
    rest: &'a str,
}

fn list_marker(line: &str) -> Option<Marker<'_>> {
    // `indent` is a column count (a leading tab counts as up to 4 columns) used
    // for the content-indent math below. To find the marker we must slice off the
    // *actual* leading whitespace by pattern — using the column count as a byte
    // index panics on any tab-indented line (a tab is 1 byte but >= 1 column).
    let indent = leading_spaces(line);
    let t = line.trim_start_matches(is_space_or_tab);
    if let Some(first) = t.chars().next()
        && (first == '-' || first == '*' || first == '+')
    {
        let after_marker = &t[first.len_utf8()..];
        let (rest, padding) = marker_padding(after_marker)?;
        return Some(Marker {
            indent,
            ordered: false,
            start: 1,
            content_indent: indent + first.len_utf8() + padding,
            rest,
        });
    }
    // Ordered: digits then `.` or `)` then space.
    let digit_len = t.bytes().take_while(u8::is_ascii_digit).count();
    if digit_len > 0 && digit_len <= 9 {
        let digits = &t[..digit_len];
        let after = &t[digit_len..];
        if (after.starts_with('.') || after.starts_with(')'))
            && let Ok(start) = digits.parse()
            && let Some((rest, padding)) = marker_padding(&after[1..])
        {
            return Some(Marker {
                indent,
                ordered: true,
                start,
                content_indent: indent + digit_len + 1 + padding,
                rest,
            });
        }
    }
    None
}

fn marker_padding(after_marker: &str) -> Option<(&str, usize)> {
    if after_marker.is_empty() {
        return Some(("", 1));
    }
    let first = after_marker.chars().next()?;
    if first == ' ' || first == '\t' {
        let width = first.len_utf8();
        Some((&after_marker[width..], 1))
    } else {
        None
    }
}

fn list_marker_interrupts_paragraph(line: &str) -> bool {
    list_marker(line).is_some_and(|m| !m.ordered || m.start == 1)
}

fn parse_list(lines: &[&str], refs: &ReferenceMap) -> (List, usize) {
    let mut profiler = ParseProfiler::disabled();
    parse_list_profiled(lines, refs, &mut profiler)
}

fn parse_list_profiled(
    lines: &[&str],
    refs: &ReferenceMap,
    profiler: &mut ParseProfiler,
) -> (List, usize) {
    let Some(first) = list_marker(lines[0]) else {
        return (
            List {
                ordered: false,
                start: 1,
                tight: true,
                items: Vec::new(),
            },
            1,
        );
    };
    let ordered = first.ordered;
    let start = first.start;
    let mut items: Vec<ListItem> = Vec::new();
    let mut tight = true;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < lines.len() && list_marker(lines[j]).is_some_and(|m| m.ordered == ordered) {
                tight = false;
                i = j;
                continue;
            }
            break;
        }
        let Some(m) = list_marker(lines[i]).filter(|m| m.ordered == ordered) else {
            break;
        };
        let mut item_lines = vec![m.rest.to_string()];
        i += 1;

        while i < lines.len() {
            if lines[i].trim().is_empty() {
                let mut j = i + 1;
                while j < lines.len() && lines[j].trim().is_empty() {
                    j += 1;
                }
                if j < lines.len()
                    && list_marker(lines[j])
                        .is_some_and(|next| next.ordered == ordered && next.indent == m.indent)
                {
                    tight = false;
                    i = j;
                    break;
                }
                // A blank line followed by a new DIRECT block of THIS item (a
                // second paragraph at the item's content column) makes the list
                // loose (CommonMark: an item holding two blank-separated blocks).
                // Require the post-blank line to sit at EXACTLY the content column
                // and to not be a list marker: deeper-indented content belongs to
                // a nested sub-list (whose own blank loosens it via recursion), a
                // marker continues a sub-list, and a dedent is a trailing blank —
                // none of those loosen THIS list.
                if j < lines.len()
                    && leading_spaces(lines[j]) == m.content_indent
                    && list_marker(strip_n(lines[j], m.content_indent)).is_none()
                {
                    tight = false;
                }
                item_lines.push(String::new());
                i += 1;
                continue;
            }

            if let Some(next) = list_marker(lines[i])
                && next.indent <= m.indent
                && (next.ordered == ordered || !next.ordered || next.start == 1)
            {
                break;
            }

            if leading_spaces(lines[i]) >= m.content_indent {
                let stripped = strip_n(lines[i], m.content_indent).to_string();
                // A non-1-start ordered marker cannot interrupt a paragraph, so
                // after prose it would be lazily absorbed; a blank line forces it
                // into its own sub-list. But when the previous content line is
                // itself a list item, this marker is just the natural 2nd/3rd/...
                // item of an ordered sub-list (start 2, 3, ...) and must stay in
                // one tight list — do not split it.
                let prev_is_list_item = item_lines
                    .last()
                    .is_some_and(|prev| list_marker(prev).is_some());
                if !prev_is_list_item
                    && list_marker(&stripped)
                        .is_some_and(|marker| marker.ordered && marker.start != 1)
                    && item_lines
                        .last()
                        .is_some_and(|prev| !prev.trim().is_empty())
                {
                    item_lines.push(String::new());
                }
                item_lines.push(stripped);
            } else if item_lines.last().is_some_and(|prev| prev.trim().is_empty()) {
                // A blank line separates this unindented line from the item, so
                // there is no open paragraph to lazily continue: it begins a new
                // top-level block and ends the list. (CommonMark lazy continuation
                // only extends an *open* paragraph — never after a blank line.)
                break;
            } else {
                // CommonMark lazy continuation: an unindented, non-marker line
                // continues the current OPEN paragraph/list item.
                item_lines.push(lines[i].trim_start().to_string());
            }
            i += 1;
        }

        let (task, first_body) = split_task_marker(&item_lines[0]);
        let mut normalized = Vec::with_capacity(item_lines.len());
        normalized.push(first_body.to_string());
        normalized.extend(item_lines.into_iter().skip(1));
        let item_refs: Vec<&str> = normalized.iter().map(String::as_str).collect();
        items.push(ListItem {
            task,
            blocks: parse_blocks_bounded(&item_refs, refs, profiler),
        });
    }
    (
        List {
            ordered,
            start,
            tight,
            items,
        },
        i,
    )
}

fn split_task_marker(text: &str) -> (Option<bool>, &str) {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed
        .strip_prefix("[ ] ")
        .or_else(|| trimmed.strip_prefix("[ ]"))
    {
        return (Some(false), rest);
    }
    for open in ["[x] ", "[X] ", "[x]", "[X]"] {
        if let Some(rest) = trimmed.strip_prefix(open) {
            return (Some(true), rest);
        }
    }
    (None, text)
}

// ---- tables -----------------------------------------------------------------

fn is_table_delimiter(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('-') {
        return false;
    }
    t.split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .all(|cell| {
            let core = cell.trim_start_matches(':').trim_end_matches(':');
            !core.is_empty() && core.chars().all(|c| c == '-')
        })
        && t.chars().any(|c| c == '|' || c == '-')
}

fn split_table_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    // Split on unescaped `|` outside inline code spans.
    let chars: Vec<char> = t.chars().collect();
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut code_ticks = 0usize;
    let mut prev_backslash = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '`' && !prev_backslash {
            let ticks = run_len(&chars, i, '`');
            for _ in 0..ticks {
                cur.push('`');
            }
            if code_ticks == 0 {
                code_ticks = ticks;
            } else if code_ticks == ticks {
                code_ticks = 0;
            }
            prev_backslash = false;
            i += ticks;
            continue;
        }
        if c == '|' && !prev_backslash && code_ticks == 0 {
            cells.push(cur.trim().to_string());
            cur = String::new();
        } else {
            if c == '\\' && !prev_backslash {
                prev_backslash = true;
                cur.push(c);
                i += 1;
                continue;
            }
            cur.push(c);
        }
        prev_backslash = false;
        i += 1;
    }
    cells.push(cur.trim().to_string());
    cells
}

fn parse_table(lines: &[&str], refs: &ReferenceMap) -> Option<(Table, usize)> {
    let mut profiler = ParseProfiler::disabled();
    parse_table_profiled(lines, refs, &mut profiler)
}

fn parse_table_profiled(
    lines: &[&str],
    refs: &ReferenceMap,
    profiler: &mut ParseProfiler,
) -> Option<(Table, usize)> {
    let header = split_table_row(lines[0]);
    let align_cells = split_table_row(lines[1]);
    let cols = header.len();
    if cols == 0 || align_cells.len() != cols {
        return None;
    }
    let align: Vec<Align> = align_cells
        .iter()
        .map(|c| {
            let left = c.starts_with(':');
            let right = c.ends_with(':');
            match (left, right) {
                (true, true) => Align::Center,
                (true, false) => Align::Left,
                (false, true) => Align::Right,
                (false, false) => Align::None,
            }
        })
        .collect();
    let head: Vec<Vec<Inline>> = header
        .iter()
        .map(|c| parse_inlines_with_refs_profiled(c, refs, profiler))
        .collect();
    let mut rows = Vec::new();
    let mut i = 2;
    while i < lines.len() && !lines[i].trim().is_empty() && lines[i].contains('|') {
        let mut cells: Vec<Vec<Inline>> = split_table_row(lines[i])
            .iter()
            .map(|c| parse_inlines_with_refs_profiled(c, refs, profiler))
            .collect();
        cells.resize_with(cols, Vec::new);
        cells.truncate(cols);
        rows.push(cells);
        i += 1;
    }
    Some((Table { align, head, rows }, i))
}

// ---- inline parser ----------------------------------------------------------

/// Parse a run of text (which may contain `\n`) into inline elements.
#[must_use]
pub fn parse_inlines(text: &str) -> Vec<Inline> {
    parse_inlines_with_refs(text, &ReferenceMap::new())
}

fn parse_inlines_with_refs(text: &str, refs: &ReferenceMap) -> Vec<Inline> {
    let mut profiler = ParseProfiler::disabled();
    parse_inlines_with_refs_profiled(text, refs, &mut profiler)
}

fn parse_inlines_with_refs_profiled(
    text: &str,
    refs: &ReferenceMap,
    profiler: &mut ParseProfiler,
) -> Vec<Inline> {
    let started = profiler.checkpoint();
    // Inline parsing is two phases. Phase 1 (this loop) tokenizes the text into a
    // flat list of `InlineEl` nodes: finalized inlines (code, links, images,
    // autolinks, raw HTML, breaks) interleaved with raw `*`/`_` emphasis
    // delimiter runs. Phase 2 (`resolve_emphasis`) runs the CommonMark
    // "process emphasis" delimiter-stack algorithm over that list to pair openers
    // with closers and build the correct nested `Emphasis`/`Strong` tree.
    let mut els: Vec<InlineEl> = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut buf = String::new();
    let mut i = 0;
    let flush = |buf: &mut String, els: &mut Vec<InlineEl>| {
        if !buf.is_empty() {
            els.push(InlineEl::Text(std::mem::take(buf)));
        }
    };
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            '\\' if i + 1 < bytes.len() && is_ascii_punct(bytes[i + 1]) => {
                buf.push(bytes[i + 1]);
                i += 2;
            }
            '\n' => {
                // Hard break: two+ trailing spaces or a trailing backslash before \n.
                let hard = buf.ends_with("  ") || buf.ends_with('\\');
                while buf.ends_with(' ') {
                    buf.pop();
                }
                if buf.ends_with('\\') {
                    buf.pop();
                }
                flush(&mut buf, &mut els);
                els.push(InlineEl::Node(if hard {
                    Inline::HardBreak
                } else {
                    Inline::SoftBreak
                }));
                i += 1;
            }
            '`' => {
                let n = run_len(&bytes, i, '`');
                if let Some(end) = find_code_close(&bytes, i + n, '`', n) {
                    flush(&mut buf, &mut els);
                    let inner: String = bytes[i + n..end].iter().collect();
                    els.push(InlineEl::Node(Inline::Code(normalize_code_span(&inner))));
                    i = end + n;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '!' if i + 1 < bytes.len() && bytes[i + 1] == '[' => {
                if let Some((alt, dest, title, next)) =
                    parse_link_like(&bytes, i + 1, refs, profiler)
                {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Image {
                        dest,
                        title,
                        alt: inlines_to_plain(&alt),
                    }));
                    i = next;
                } else if let Some((alt, dest, title, next)) =
                    parse_reference_link_like(&bytes, i + 1, refs, profiler)
                {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Image {
                        dest,
                        title,
                        alt: inlines_to_plain(&alt),
                    }));
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '[' => {
                if let Some((content, dest, title, next)) =
                    parse_link_like(&bytes, i, refs, profiler)
                {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Link {
                        dest,
                        title,
                        content,
                    }));
                    i = next;
                } else if let Some((content, dest, title, next)) =
                    parse_reference_link_like(&bytes, i, refs, profiler)
                {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Link {
                        dest,
                        title,
                        content,
                    }));
                    i = next;
                } else if let Some((html, next)) = parse_inline_html(&bytes, i) {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Html(html)));
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '<' => {
                if let Some((label, dest, next)) = parse_autolink(&bytes, i) {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Link {
                        dest,
                        title: None,
                        content: vec![Inline::Text(label)],
                    }));
                    i = next;
                } else if let Some((html, next)) = parse_inline_html(&bytes, i) {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Html(html)));
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '&' => {
                if let Some((decoded, next)) = parse_character_reference(&bytes, i) {
                    buf.push_str(&decoded);
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '~' if run_len(&bytes, i, '~') >= 2 => {
                if let Some((inner, next)) = parse_delim(&bytes, i, '~', 2) {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Strikethrough(
                        parse_inlines_with_refs_profiled(&inner, refs, profiler),
                    )));
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '*' | '_' => {
                // Push a `*`/`_` delimiter run onto the node list with its
                // CommonMark left/right-flanking can_open/can_close flags. The
                // actual pairing into emphasis/strong is deferred to
                // `resolve_emphasis` (the delimiter-stack pass) so that nested and
                // overlapping runs resolve correctly.
                let n = run_len(&bytes, i, c);
                let before = i.checked_sub(1).map(|idx| bytes[idx]);
                let after = bytes.get(i + n).copied();
                let (can_open, can_close) = emphasis_flanking(before, after, c);
                if can_open || can_close {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Delim {
                        ch: c,
                        count: n,
                        orig: n,
                        can_open,
                        can_close,
                    });
                } else {
                    // An inert run (e.g. an intraword `_`) is literal text.
                    for _ in 0..n {
                        buf.push(c);
                    }
                }
                i += n;
            }
            _ => {
                if let Some((label, dest, next)) = parse_bare_url_autolink(&bytes, i)
                    .or_else(|| parse_bare_email_autolink(&bytes, i))
                {
                    flush(&mut buf, &mut els);
                    els.push(InlineEl::Node(Inline::Link {
                        dest,
                        title: None,
                        content: vec![Inline::Text(label)],
                    }));
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
        }
    }
    flush(&mut buf, &mut els);
    let out = resolve_emphasis(els);
    profiler.record_since(
        "inline_parse",
        bytes.len(),
        text.len(),
        1 + bytes.len() + out.len(),
        "parse inline delimiters, links, references, autolinks, code spans, and text",
        started,
    );
    out
}

/// One element of the flat inline token list built before emphasis resolution.
enum InlineEl {
    /// A run of plain text (entities already decoded, escapes already applied).
    Text(String),
    /// A finalized inline that emphasis processing treats as opaque (code,
    /// links, images, autolinks, raw HTML, breaks, and emphasis/strong/strike
    /// subtrees produced by `resolve_emphasis` itself).
    Node(Inline),
    /// An unresolved run of `*` or `_` emphasis delimiters.
    Delim {
        ch: char,
        /// Remaining (not yet consumed) delimiter characters.
        count: usize,
        /// Original run length, used for the CommonMark "rule of three".
        orig: usize,
        can_open: bool,
        can_close: bool,
    },
}

/// CommonMark left/right-flanking classification for a `*`/`_` delimiter run.
///
/// Returns `(can_open, can_close)`. `before`/`after` are the characters
/// immediately adjacent to the run (the start and end of the text count as
/// whitespace). For `_`, the additional intraword rule applies so that
/// `foo_bar` stays literal while `_foo_` opens/closes emphasis.
fn emphasis_flanking(before: Option<char>, after: Option<char>, ch: char) -> (bool, bool) {
    let before_ws = before.is_none_or(char::is_whitespace);
    let before_punct = before.is_some_and(|c| c.is_ascii_punctuation());
    let after_ws = after.is_none_or(char::is_whitespace);
    let after_punct = after.is_some_and(|c| c.is_ascii_punctuation());

    let left_flanking = !after_ws && (!after_punct || before_ws || before_punct);
    let right_flanking = !before_ws && (!before_punct || after_ws || after_punct);

    if ch == '_' {
        let can_open = left_flanking && (!right_flanking || before_punct);
        let can_close = right_flanking && (!left_flanking || after_punct);
        (can_open, can_close)
    } else {
        (left_flanking, right_flanking)
    }
}

/// Append text to `out`, merging with a trailing `Text` node when possible so the
/// resolved inline sequence keeps adjacent literal runs coalesced.
fn push_inline_text(out: &mut Vec<Inline>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Inline::Text(t)) = out.last_mut() {
        t.push_str(s);
    } else {
        out.push(Inline::Text(s.to_string()));
    }
}

/// Emit one `InlineEl` into a finalized inline vector. Unmatched delimiter runs
/// degrade to their literal characters.
fn emit_inline_el(el: &InlineEl, out: &mut Vec<Inline>) {
    match el {
        InlineEl::Text(s) => push_inline_text(out, s),
        InlineEl::Node(inl) => out.push(inl.clone()),
        InlineEl::Delim { ch, count, .. } => {
            let mut s = String::with_capacity(*count);
            for _ in 0..*count {
                s.push(*ch);
            }
            push_inline_text(out, &s);
        }
    }
}

/// Move an element's content into `out` without cloning.
///
/// Used when consuming the nodes enclosed by a matched delimiter pair: those
/// nodes are marked dead and spliced out of the list, so they are never read
/// again and their (possibly deeply nested) subtree can be moved rather than
/// cloned. Cloning here made repeated wrapping — e.g. `***…***x***…***`, which
/// nests one `Strong`/`Emphasis` per delimiter pair — quadratic, because each
/// pair re-cloned the whole growing subtree.
fn emit_inline_el_owned(el: InlineEl, out: &mut Vec<Inline>) {
    match el {
        InlineEl::Text(s) => push_inline_text(out, &s),
        InlineEl::Node(inl) => out.push(inl),
        InlineEl::Delim { ch, count, .. } => {
            let mut s = String::with_capacity(count);
            for _ in 0..count {
                s.push(ch);
            }
            push_inline_text(out, &s);
        }
    }
}

/// Resolve a flat token list into a nested inline tree using the CommonMark
/// "process emphasis" delimiter-stack algorithm, then linearize what remains.
fn resolve_emphasis(els: Vec<InlineEl>) -> Vec<Inline> {
    let n = els.len();
    let mut els = els;
    // Intrusive doubly linked list over `els`, with tombstones (`alive`) instead
    // of physical removal so indices stay stable as nodes are spliced in/out.
    let mut prev: Vec<Option<usize>> = (0..n).map(|i| i.checked_sub(1)).collect();
    let mut next: Vec<Option<usize>> = (0..n).map(|i| (i + 1 < n).then_some(i + 1)).collect();
    let mut alive: Vec<bool> = vec![true; n];
    // Nesting depth of the subtree each node represents (1 for a leaf token).
    // Used to bound how deep emphasis/strong wrapping may go.
    let mut node_depth: Vec<usize> = vec![1; n];
    let mut head: Option<usize> = (n > 0).then_some(0);

    process_emphasis(
        &mut els,
        &mut prev,
        &mut next,
        &mut alive,
        &mut node_depth,
        &mut head,
    );

    let mut out = Vec::new();
    let mut idx = head;
    while let Some(k) = idx {
        if alive[k] {
            emit_inline_el(&els[k], &mut out);
        }
        idx = next[k];
    }
    out
}

/// Splice element `x` out of the intrusive linked list and mark it dead.
fn unlink_el(
    x: usize,
    prev: &mut [Option<usize>],
    next: &mut [Option<usize>],
    alive: &mut [bool],
    head: &mut Option<usize>,
) {
    let p = prev[x];
    let nx = next[x];
    match p {
        Some(pp) => next[pp] = nx,
        None => *head = nx,
    }
    if let Some(nn) = nx {
        prev[nn] = p;
    }
    alive[x] = false;
}

/// Defensive bound on emphasis/strong nesting depth. Real prose never nests
/// inline emphasis more than a handful deep; a pathological run like
/// `***…***x***…***` would otherwise build a tree thousands of levels deep,
/// which overflows the stack when it is later rendered or dropped (recursive
/// descent over `Inline`). Past this cap we stop wrapping and leave the surplus
/// delimiters as literal text — mirroring [`MAX_BLOCK_NESTING_DEPTH`].
const MAX_INLINE_NESTING_DEPTH: usize = 1000;

/// The CommonMark "process emphasis" pass: walk closers left to right, match each
/// to the nearest compatible opener honoring the rule of three, and wrap the
/// enclosed nodes in `Strong` (2 delimiters) or `Emphasis` (1 delimiter).
///
/// Matching a closer means walking back over openers, and for pathological
/// both-open-and-close runs (e.g. alternating `*_*_…`) that back-walk is
/// quadratic even with `openers_bottom`. We cap the *total* back-walk work at a
/// linear multiple of the token count; once spent we stop pairing and leave the
/// remaining delimiters as literal text. Legitimate prose never approaches the
/// budget (its openers are always nearby), so output is unaffected; only crafted
/// worst-case input degrades, and it degrades deterministically.
fn process_emphasis(
    els: &mut Vec<InlineEl>,
    prev: &mut Vec<Option<usize>>,
    next: &mut Vec<Option<usize>>,
    alive: &mut Vec<bool>,
    node_depth: &mut Vec<usize>,
    head: &mut Option<usize>,
) {
    // Per (char, slot) lower bound below which no opener can be found; `slot`
    // folds the closer's can_open flag and run length mod 3, mirroring the
    // reference implementation's `openers_bottom`.
    let mut openers_bottom: BTreeMap<(char, usize), Option<usize>> = BTreeMap::new();
    // Linear back-walk budget (see fn doc). 64x the token count is far above any
    // real document yet turns the adversarial quadratic case into linear time.
    let step_budget = els.len().saturating_mul(64).max(4096);
    let mut steps: usize = 0;
    let mut ci = *head;

    while let Some(c) = ci {
        if !alive[c] {
            ci = next[c];
            continue;
        }
        let (cch, closer_can_open, corig) = match &els[c] {
            InlineEl::Delim {
                ch,
                can_close,
                can_open,
                orig,
                ..
            } if *can_close => (*ch, *can_open, *orig),
            _ => {
                ci = next[c];
                continue;
            }
        };
        let slot = (if closer_can_open { 3 } else { 0 }) + (corig % 3);
        let bound = openers_bottom.get(&(cch, slot)).copied().flatten();

        // Walk back to the nearest opener of the same char that is not rejected
        // by the rule of three.
        let mut opener_idx = prev[c];
        let mut found: Option<usize> = None;
        while let Some(o) = opener_idx {
            steps += 1;
            if steps > step_budget {
                // Back-walk budget exhausted: stop pairing. Any still-unmatched
                // delimiters stay alive and are emitted as literal text by the
                // caller's final linearization. Bounds worst-case CPU on crafted
                // both-open-and-close runs without affecting real prose.
                return;
            }
            if Some(o) == bound {
                break;
            }
            if alive[o]
                && let InlineEl::Delim {
                    ch,
                    can_open,
                    can_close,
                    orig,
                    ..
                } = &els[o]
                && *ch == cch
                && *can_open
            {
                // Rule of three: if either delimiter can both open and close, the
                // summed run lengths must not be a multiple of 3 unless both are.
                let odd_match = (closer_can_open || *can_close)
                    && (*orig + corig) % 3 == 0
                    && !(*orig % 3 == 0 && corig % 3 == 0);
                if !odd_match {
                    found = Some(o);
                    break;
                }
            }
            opener_idx = prev[o];
        }

        let Some(o) = found else {
            // No opener: remember the lower bound. The delimiter itself is
            // still literal source text, so leave it alive for final emission.
            openers_bottom.insert((cch, slot), prev[c]);
            ci = next[c];
            continue;
        };

        let ocount = match &els[o] {
            InlineEl::Delim { count, .. } => *count,
            _ => 0,
        };
        let ccount = match &els[c] {
            InlineEl::Delim { count, .. } => *count,
            _ => 0,
        };
        // Pair delimiters into strong (2) or emphasis (1). A single multi-run gets
        // strong as the OUTER wrapper (`***x***` -> <strong><em>x</em></strong>)
        // by consuming the lone emphasis delimiter first — but ONLY when an odd
        // delimiter is actually left over (both runs odd). Even runs pair entirely
        // into strong, so `****x****` is <strong><strong>x</strong></strong>
        // (bold), never nested emphasis (which would wrongly render as italic).
        let use_delims = if ocount >= 3 && ccount >= 3 && ocount % 2 == 1 && ccount % 2 == 1 {
            1
        } else if ocount >= 2 && ccount >= 2 {
            2
        } else {
            1
        };

        // Bound nesting depth before building anything: the deepest node strictly
        // between opener and closer determines the wrapper's depth. Past the cap
        // we refuse to wrap and leave this closer as literal text (advance past
        // it) so the resulting tree cannot overflow the stack at render/drop.
        // The `next` chain only threads live nodes, so every visited index is
        // alive; the `filter` folds the `!= c` terminator into the loop head.
        let mut max_child_depth = 0usize;
        let mut m = next[o];
        while let Some(mi) = m.filter(|&mi| mi != c) {
            max_child_depth = max_child_depth.max(node_depth[mi]);
            m = next[mi];
        }
        if max_child_depth >= MAX_INLINE_NESTING_DEPTH {
            ci = next[c];
            continue;
        }

        // Collect and consume the nodes strictly between opener and closer.
        let mut content: Vec<Inline> = Vec::new();
        let mut m = next[o];
        while let Some(mi) = m {
            if mi == c {
                break;
            }
            let nxt = next[mi];
            if alive[mi] {
                alive[mi] = false;
                // Move the node out rather than clone it: it is now dead and
                // spliced out below, so it is never read again. This keeps
                // repeated wrapping (deeply nested `Strong`/`Emphasis`) linear
                // instead of re-cloning the growing subtree each pair.
                let taken = std::mem::replace(&mut els[mi], InlineEl::Text(String::new()));
                emit_inline_el_owned(taken, &mut content);
            }
            m = nxt;
        }
        let node = if use_delims == 2 {
            Inline::Strong(content)
        } else {
            Inline::Emphasis(content)
        };

        if let InlineEl::Delim { count, .. } = &mut els[o] {
            *count -= use_delims;
        }
        if let InlineEl::Delim { count, .. } = &mut els[c] {
            *count -= use_delims;
        }

        // Splice the new node between the (possibly shortened) opener and closer.
        let ni = els.len();
        els.push(InlineEl::Node(node));
        prev.push(Some(o));
        next.push(Some(c));
        alive.push(true);
        node_depth.push(max_child_depth + 1);
        next[o] = Some(ni);
        prev[c] = Some(ni);

        if matches!(&els[o], InlineEl::Delim { count, .. } if *count == 0) {
            unlink_el(o, prev, next, alive, head);
        }
        if matches!(&els[c], InlineEl::Delim { count, .. } if *count == 0) {
            let after = next[c];
            unlink_el(c, prev, next, alive, head);
            ci = after;
        } else {
            // Closer still has delimiters: keep matching it against more openers.
            ci = Some(c);
        }
    }
}

fn run_len(chars: &[char], i: usize, ch: char) -> usize {
    chars[i..].iter().take_while(|&&c| c == ch).count()
}

fn is_intraword_underscore_run(chars: &[char], i: usize, run: usize) -> bool {
    if chars.get(i) != Some(&'_') {
        return false;
    }
    let before = i.checked_sub(1).and_then(|idx| chars.get(idx));
    let after = chars.get(i + run);
    before.is_some_and(|ch| ch.is_alphanumeric()) && after.is_some_and(|ch| ch.is_alphanumeric())
}

fn find_code_close(chars: &[char], from: usize, ch: char, n: usize) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == ch && run_len(chars, i, ch) == n {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn normalize_code_span(s: &str) -> String {
    // CommonMark: collapse internal line endings to spaces; strip one leading and
    // trailing space if the span is not all spaces.
    let s = s.replace('\n', " ");
    if s.len() >= 2 && s.starts_with(' ') && s.ends_with(' ') && s.trim() != "" {
        s[1..s.len() - 1].to_string()
    } else {
        s
    }
}

/// Parse a balanced delimiter run `<ch>{want} ... <ch>{want}` returning the inner
/// text and the index just past the close.
fn parse_delim(chars: &[char], i: usize, ch: char, want: usize) -> Option<(String, usize)> {
    let open_run = run_len(chars, i, ch);
    if open_run < want {
        return None;
    }
    // No space immediately after the opener (left-flanking-ish heuristic).
    let after = i + want;
    if after >= chars.len() || chars[after] == ' ' || chars[after] == '\n' {
        return None;
    }
    let mut j = after;
    while j < chars.len() {
        if chars[j] == ch {
            let run = run_len(chars, j, ch);
            if run >= want
                && j > after
                && chars[j - 1] != ' '
                && !is_intraword_underscore_run(chars, j, run)
            {
                let inner: String = chars[after..j].iter().collect();
                return Some((inner, j + want));
            }
            j += run;
        } else {
            j += 1;
        }
    }
    None
}

/// Parse `[content](dest "title")` starting at the `[`.
fn parse_link_like(
    chars: &[char],
    i: usize,
    refs: &ReferenceMap,
    profiler: &mut ParseProfiler,
) -> Option<(Vec<Inline>, String, Option<String>, usize)> {
    if chars.get(i) != Some(&'[') {
        return None;
    }
    let j = find_closing_bracket(chars, i)?;
    if chars.get(j) != Some(&']') || chars.get(j + 1) != Some(&'(') {
        return None;
    }
    let text: String = chars[i + 1..j].iter().collect();
    let mut k = j + 2;

    skip_spaces(chars, &mut k);
    let dest = parse_link_destination(chars, &mut k)?;
    skip_spaces(chars, &mut k);

    let title = if chars.get(k) == Some(&')') {
        None
    } else {
        let title = parse_link_title(chars, &mut k)?;
        skip_spaces(chars, &mut k);
        Some(title)
    };

    if chars.get(k) != Some(&')') {
        return None;
    }
    Some((
        parse_inlines_with_refs_profiled(&text, refs, profiler),
        dest.trim().to_string(),
        title,
        k + 1,
    ))
}

fn parse_link_destination(chars: &[char], i: &mut usize) -> Option<String> {
    if chars.get(*i) == Some(&'<') {
        parse_angle_link_destination(chars, i)
    } else {
        parse_bare_link_destination(chars, i)
    }
}

fn parse_angle_link_destination(chars: &[char], i: &mut usize) -> Option<String> {
    if chars.get(*i) != Some(&'<') {
        return None;
    }
    *i += 1;
    let mut dest = String::new();
    while let Some(&ch) = chars.get(*i) {
        match ch {
            '>' => {
                *i += 1;
                return Some(dest);
            }
            '\n' | '<' => return None,
            '\\' if chars.get(*i + 1).is_some_and(|&next| is_ascii_punct(next)) => {
                dest.push(chars[*i + 1]);
                *i += 2;
            }
            '&' => {
                if let Some((decoded, next)) = parse_character_reference(chars, *i) {
                    dest.push_str(&decoded);
                    *i = next;
                } else {
                    dest.push(ch);
                    *i += 1;
                }
            }
            _ => {
                dest.push(ch);
                *i += 1;
            }
        }
    }
    None
}

fn parse_bare_link_destination(chars: &[char], i: &mut usize) -> Option<String> {
    let mut dest = String::new();
    let mut paren_depth = 0usize;

    while let Some(&ch) = chars.get(*i) {
        match ch {
            ')' if paren_depth == 0 => break,
            ')' => {
                paren_depth -= 1;
                dest.push(ch);
                *i += 1;
            }
            '(' => {
                paren_depth += 1;
                dest.push(ch);
                *i += 1;
            }
            '<' | '\n' => return None,
            ch if ch.is_whitespace() => break,
            '\\' if chars.get(*i + 1).is_some_and(|&next| is_ascii_punct(next)) => {
                dest.push(chars[*i + 1]);
                *i += 2;
            }
            '&' => {
                if let Some((decoded, next)) = parse_character_reference(chars, *i) {
                    dest.push_str(&decoded);
                    *i = next;
                } else {
                    dest.push(ch);
                    *i += 1;
                }
            }
            _ => {
                dest.push(ch);
                *i += 1;
            }
        }
    }

    if paren_depth == 0 { Some(dest) } else { None }
}

fn parse_link_title(chars: &[char], i: &mut usize) -> Option<String> {
    let (open, close) = match chars.get(*i).copied()? {
        '"' => ('"', '"'),
        '\'' => ('\'', '\''),
        '(' => ('(', ')'),
        _ => return None,
    };
    if chars.get(*i) != Some(&open) {
        return None;
    }
    *i += 1;

    let mut title = String::new();
    while let Some(&ch) = chars.get(*i) {
        match ch {
            c if c == close => {
                *i += 1;
                return Some(title);
            }
            '\n' => return None,
            '\\' if chars.get(*i + 1).is_some_and(|&next| is_ascii_punct(next)) => {
                title.push(chars[*i + 1]);
                *i += 2;
            }
            _ => {
                title.push(ch);
                *i += 1;
            }
        }
    }
    None
}

fn parse_reference_link_like(
    chars: &[char],
    i: usize,
    refs: &ReferenceMap,
    profiler: &mut ParseProfiler,
) -> Option<(Vec<Inline>, String, Option<String>, usize)> {
    if chars.get(i) != Some(&'[') {
        return None;
    }
    let close = find_closing_bracket(chars, i)?;
    let text: String = chars[i + 1..close].iter().collect();

    let (label, next) = if chars.get(close + 1) == Some(&'[') {
        let label_start = close + 2;
        let label_close = find_closing_bracket(chars, close + 1)?;
        let raw_label: String = chars[label_start..label_close].iter().collect();
        let label = if raw_label.is_empty() {
            normalize_reference_label(&text)?
        } else {
            normalize_reference_label(&raw_label)?
        };
        (label, label_close + 1)
    } else {
        (normalize_reference_label(&text)?, close + 1)
    };

    let reference = refs.get(&label)?;
    Some((
        parse_inlines_with_refs_profiled(&text, refs, profiler),
        reference.dest.clone(),
        reference.title.clone(),
        next,
    ))
}

fn parse_autolink(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    if chars.get(i) != Some(&'<') {
        return None;
    }
    // Autolink content runs to the closing `>` and may not contain whitespace,
    // `<`, or ASCII control characters (CommonMark).
    let mut j = i + 1;
    let mut content = String::new();
    while j < chars.len() {
        let ch = chars[j];
        if ch == '>' {
            break;
        }
        if ch == '<' || ch.is_whitespace() || ch.is_control() {
            return None;
        }
        content.push(ch);
        j += 1;
    }
    if chars.get(j) != Some(&'>') || content.is_empty() {
        return None;
    }
    if is_uri_autolink(&content) {
        // URI autolinks keep the destination verbatim (including `tel:`, `urn:`,
        // `mailto:`, and other opaque schemes that lack `://`).
        Some((content.clone(), content, j + 1))
    } else if is_email_autolink(&content) {
        let dest = format!("mailto:{content}");
        Some((content, dest, j + 1))
    } else {
        None
    }
}

/// A CommonMark absolute-URI autolink scheme: an ASCII letter followed by 1..=31
/// of `[A-Za-z0-9+.-]`, then a `:`. The body after the colon is validated by the
/// caller (no whitespace / `<` / control characters).
fn is_uri_autolink(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.first().is_none_or(|b| !b.is_ascii_alphabetic()) {
        return false;
    }
    let mut k = 1;
    while k < bytes.len() {
        let b = bytes[k];
        if b == b':' {
            // Scheme is `bytes[..k]`; total scheme length must be 2..=32.
            return (2..=32).contains(&k);
        }
        if b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-') {
            k += 1;
        } else {
            return false;
        }
    }
    false
}

/// A CommonMark email autolink (the HTML5 email-address production).
fn is_email_autolink(s: &str) -> bool {
    let Some(at) = s.find('@') else {
        return false;
    };
    let (local, domain) = (&s[..at], &s[at + 1..]);
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    let local_ok = local.bytes().all(|b| {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'.' | b'!'
                    | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'/'
                    | b'='
                    | b'?'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'{'
                    | b'|'
                    | b'}'
                    | b'~'
                    | b'-'
            )
    });
    if !local_ok {
        return false;
    }
    // Domain: dot-separated labels of `[A-Za-z0-9-]`, each 1..=63 chars and not
    // starting or ending with `-`.
    domain.split('.').all(|label| {
        let b = label.as_bytes();
        !b.is_empty()
            && b.len() <= 63
            && b[0] != b'-'
            && b[b.len() - 1] != b'-'
            && b.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'-')
    })
}

fn parse_character_reference(chars: &[char], i: usize) -> Option<(String, usize)> {
    if chars.get(i) != Some(&'&') {
        return None;
    }
    let semi = chars[i + 1..]
        .iter()
        .position(|&ch| ch == ';')
        .map(|offset| i + 1 + offset)?;
    if semi == i + 1 {
        return None;
    }
    let body = chars[i + 1..semi].iter().collect::<String>();
    let decoded: String =
        if let Some(numeric) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
            decode_numeric_reference(numeric, 16)?.to_string()
        } else if let Some(numeric) = body.strip_prefix('#') {
            decode_numeric_reference(numeric, 10)?.to_string()
        } else {
            // Full HTML5 named character reference set (semicolon-terminated, as
            // CommonMark requires). A few entities resolve to two code points.
            entities::lookup(&body)?.to_string()
        };
    Some((decoded, semi + 1))
}

fn parse_bare_url_autolink(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    if !bare_url_left_boundary(chars, i) {
        return None;
    }

    let is_www = starts_with_chars(chars, i, "www.");
    if !(starts_with_chars(chars, i, "http://")
        || starts_with_chars(chars, i, "https://")
        || is_www)
    {
        return None;
    }

    let mut end = i;
    while end < chars.len() && !chars[end].is_whitespace() && chars[end] != '<' && chars[end] != '>'
    {
        end += 1;
    }
    while end > i && bare_url_trailing_punctuation(chars[end - 1]) {
        end -= 1;
    }
    end = trim_unmatched_trailing_parens(chars, i, end);
    if end == i || (is_www && end <= i + 4) {
        return None;
    }

    let label = chars[i..end].iter().collect::<String>();
    let dest = if is_www {
        format!("http://{label}")
    } else {
        label.clone()
    };
    Some((label, dest, end))
}

/// Parse a bare (scheme-less) email address into a `mailto:` autolink, GFM-style.
///
/// Returns `(label, dest, end)` where `label` is the matched address and `dest`
/// is `mailto:<label>`. Conservative: the local part is alphanumeric plus
/// `. - _ +`, the domain is dot-separated alphanumeric/`-`/`_` labels with at
/// least one dot, trailing sentence dots are trimmed, and the domain may not end
/// in `-`/`_` — so ordinary `@`-containing prose is not falsely linked.
fn parse_bare_email_autolink(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    if !bare_url_left_boundary(chars, i) {
        return None;
    }
    let mut j = i;
    while chars
        .get(j)
        .is_some_and(|&c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
    {
        j += 1;
    }
    // Need a non-empty local part immediately followed by `@`.
    if j == i || chars.get(j) != Some(&'@') {
        return None;
    }
    j += 1;
    let domain_start = j;
    while chars
        .get(j)
        .is_some_and(|&c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        j += 1;
    }
    // Trim trailing dots (sentence punctuation), then require a real dotted domain
    // that does not end in `-`/`_`.
    let mut end = j;
    while end > domain_start && chars[end - 1] == '.' {
        end -= 1;
    }
    if end <= domain_start || matches!(chars[end - 1], '-' | '_') {
        return None;
    }
    let domain: String = chars[domain_start..end].iter().collect();
    if !domain.contains('.') {
        return None;
    }
    let label: String = chars[i..end].iter().collect();
    let dest = format!("mailto:{label}");
    Some((label, dest, end))
}

fn starts_with_chars(chars: &[char], i: usize, needle: &str) -> bool {
    for (offset, expected) in needle.chars().enumerate() {
        if chars.get(i + offset) != Some(&expected) {
            return false;
        }
    }
    true
}

fn bare_url_left_boundary(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    chars
        .get(i - 1)
        .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '(' | '[' | '{' | '"' | '\''))
}

const fn bare_url_trailing_punctuation(ch: char) -> bool {
    matches!(ch, '.' | ',' | ';' | ':' | '!' | '?')
}

fn trim_unmatched_trailing_parens(chars: &[char], start: usize, mut end: usize) -> usize {
    while end > start && chars[end - 1] == ')' && has_unmatched_closing_paren(chars, start, end) {
        end -= 1;
    }
    end
}

fn has_unmatched_closing_paren(chars: &[char], start: usize, end: usize) -> bool {
    let mut opens = 0usize;
    let mut closes = 0usize;
    for ch in &chars[start..end] {
        match ch {
            '(' => opens += 1,
            ')' => closes += 1,
            _ => {}
        }
    }
    closes > opens
}

fn decode_numeric_reference(value: &str, radix: u32) -> Option<char> {
    if value.is_empty() {
        return None;
    }
    // Only an all-digits run (in the given radix) is a numeric reference at all;
    // anything else stays literal (`&#xyz;` etc.).
    if !value.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    // CommonMark: the NUL code point (U+0000), out-of-range values (> U+10FFFF,
    // including digit runs that overflow `u32`), and surrogate code points
    // (U+D800..=U+DFFF) all decode to the replacement character U+FFFD rather
    // than emitting a raw/invalid byte. `char::from_u32` already rejects
    // surrogates and out-of-range scalars; we additionally fold U+0000 so a
    // literal NUL never reaches the output.
    let code = u32::from_str_radix(value, radix).unwrap_or(u32::MAX);
    Some(
        char::from_u32(code)
            .filter(|&c| c != '\0')
            .unwrap_or('\u{FFFD}'),
    )
}

fn parse_inline_html(chars: &[char], i: usize) -> Option<(String, usize)> {
    if chars.get(i) != Some(&'<') {
        return None;
    }
    if chars.get(i + 1) == Some(&'!')
        && chars.get(i + 2) == Some(&'-')
        && chars.get(i + 3) == Some(&'-')
    {
        let mut j = i + 4;
        while j + 2 < chars.len() {
            if chars[j] == '-' && chars[j + 1] == '-' && chars[j + 2] == '>' {
                let html: String = chars[i..=j + 2].iter().collect();
                return Some((html, j + 3));
            }
            j += 1;
        }
        return None;
    }

    let first = chars.get(i + 1).copied()?;
    let tag_like = first.is_ascii_alphabetic()
        || first == '!'
        || first == '?'
        || (first == '/' && chars.get(i + 2).is_some_and(|ch| ch.is_ascii_alphabetic()));
    if !tag_like {
        return None;
    }

    let mut j = i + 1;
    while j < chars.len() && chars[j] != '>' && chars[j] != '\n' {
        j += 1;
    }
    if chars.get(j) != Some(&'>') {
        return None;
    }
    let html: String = chars[i..=j].iter().collect();
    Some((html, j + 1))
}

fn find_closing_bracket(chars: &[char], open: usize) -> Option<usize> {
    if chars.get(open) != Some(&'[') {
        return None;
    }
    let mut depth = 1;
    let mut i = open + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn normalize_reference_label(label: &str) -> Option<String> {
    let mut out = String::new();
    let mut pending_space = false;
    for ch in label.trim().chars() {
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
        pending_space = false;
    }
    if out.is_empty() { None } else { Some(out) }
}

fn skip_spaces(chars: &[char], i: &mut usize) {
    while *i < chars.len() && (chars[*i] == ' ' || chars[*i] == '\t') {
        *i += 1;
    }
}

fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => s.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                s.push_str(&inlines_to_plain(c));
            }
            Inline::Link { content, .. } => s.push_str(&inlines_to_plain(content)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => s.push(' '),
            Inline::Html(h) => s.push_str(h),
        }
    }
    s
}

// ---- small helpers ----------------------------------------------------------

/// Width, in columns, of a line's leading indentation. Tabs advance to the next
/// 4-column tab stop (CommonMark), so a single leading tab counts as the four
/// columns that make a line indented code. Only the leading run is measured;
/// tabs elsewhere on the line are left for the content to keep verbatim.
fn leading_spaces(line: &str) -> usize {
    let mut col = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => col += 1,
            '\t' => col += 4 - col % 4,
            _ => break,
        }
    }
    col
}

fn trim_space_tab(s: &str) -> &str {
    trim_start_space_tab(trim_end_space_tab(s))
}

fn trim_start_space_tab(s: &str) -> &str {
    s.trim_start_matches(is_space_or_tab)
}

fn trim_end_space_tab(s: &str) -> &str {
    s.trim_end_matches(is_space_or_tab)
}

fn starts_space_or_tab(s: &str) -> bool {
    s.as_bytes()
        .first()
        .is_some_and(|&byte| is_space_or_tab_byte(byte))
}

fn is_space_or_tab(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

fn is_space_or_tab_byte(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// Remove up to `n` columns of leading indentation, expanding tabs to 4-column
/// tab stops. A tab that would straddle the `n`-column boundary is left intact
/// (we never split a tab into spaces, so the result stays a borrowed slice).
fn strip_n(line: &str, n: usize) -> &str {
    let mut col = 0usize;
    for (idx, ch) in line.char_indices() {
        if col >= n {
            return &line[idx..];
        }
        match ch {
            ' ' => col += 1,
            '\t' => {
                let next = col + (4 - col % 4);
                if next > n {
                    return &line[idx..];
                }
                col = next;
            }
            _ => return &line[idx..],
        }
    }
    ""
}

fn is_ascii_punct(c: char) -> bool {
    c.is_ascii_punctuation()
}

#[cfg(test)]
mod emphasis_dos_tests {
    use super::{MAX_INLINE_NESTING_DEPTH, parse_inlines};
    use crate::ast::Inline;

    /// Deepest chain of nested emphasis/strong/strike in an inline list. (These
    /// tests only build `*`/`_`/`~~` nesting, so other recursive variants need
    /// not be walked; `fold` avoids an empty-input branch.)
    fn inline_depth(inlines: &[Inline]) -> usize {
        inlines
            .iter()
            .map(|i| match i {
                Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                    1 + inline_depth(c)
                }
                _ => 1,
            })
            .fold(0, usize::max)
    }

    #[test]
    fn deep_emphasis_run_is_depth_capped_not_stack_overflowing() {
        // ~1500 nesting levels would result without the cap. The cap flattens the
        // surplus to literal text (bounded depth), and the move-based wrapping
        // keeps it linear rather than re-cloning the growing subtree per pair.
        let stars = "*".repeat(3000);
        let out = parse_inlines(&format!("{stars}x{stars}"));
        assert!(!out.is_empty());
        assert!(
            inline_depth(&out) <= MAX_INLINE_NESTING_DEPTH + 1,
            "emphasis nesting {} exceeded cap {MAX_INLINE_NESTING_DEPTH}",
            inline_depth(&out)
        );
    }

    #[test]
    fn alternating_delimiter_runs_hit_backwalk_budget_and_stay_bounded() {
        // Alternating both-open-and-close runs make every closer walk back over the
        // opposite delimiter; the linear back-walk budget stops pairing before this
        // goes quadratic. The test completing is the proof it stays bounded.
        let open = "*_".repeat(20_000);
        let close = "_*".repeat(20_000);
        let out = parse_inlines(&format!("{open}x{close}"));
        assert!(!out.is_empty());
        assert!(inline_depth(&out) <= MAX_INLINE_NESTING_DEPTH + 1);
    }

    #[test]
    fn normal_emphasis_is_unaffected() {
        // The cap/budget never trip on ordinary input: exact shapes still hold.
        assert_eq!(
            parse_inlines("*a* **b** ***c***"),
            vec![
                Inline::Emphasis(vec![Inline::Text("a".into())]),
                Inline::Text(" ".into()),
                Inline::Strong(vec![Inline::Text("b".into())]),
                Inline::Text(" ".into()),
                Inline::Strong(vec![Inline::Emphasis(vec![Inline::Text("c".into())])]),
            ]
        );
    }

    #[test]
    fn strikethrough_and_emphasis_nest_and_measure() {
        // Exercises the `~~` strikethrough recursion path and the Strikethrough
        // arm of the depth measure, and confirms mixed nesting is preserved.
        let out = parse_inlines("~~a *b* c~~");
        assert_eq!(
            out,
            vec![Inline::Strikethrough(vec![
                Inline::Text("a ".into()),
                Inline::Emphasis(vec![Inline::Text("b".into())]),
                Inline::Text(" c".into()),
            ])]
        );
        assert_eq!(inline_depth(&out), 3);
    }
}
