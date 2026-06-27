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

#[derive(Debug, Clone)]
struct LinkReference {
    dest: String,
    title: Option<String>,
}

type ReferenceMap = BTreeMap<String, LinkReference>;

/// Parse a full Markdown document.
#[must_use]
pub fn parse_document(src: &str) -> Document {
    // Normalize: strip a UTF-8 BOM; `lines()` handles both `\n` and `\r\n`.
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let lines: Vec<&str> = src.lines().collect();
    let (lines, refs) = collect_link_references(&lines);
    Document {
        blocks: parse_blocks_with_refs(&lines, &refs),
    }
}

/// Parse a document and attach top-level source spans plus recoverable parser
/// diagnostics. This is intentionally additive: the normal renderer AST remains
/// span-free.
#[must_use]
pub fn parse_document_spanned(src: &str) -> SpannedDocument {
    let document = parse_document(src);
    let spans = collect_top_level_spans(src);
    let fallback = SourceSpan::new(0, src.len());
    let blocks = document
        .blocks
        .into_iter()
        .enumerate()
        .map(|(idx, block)| Spanned::new(block, spans.get(idx).copied().unwrap_or(fallback)))
        .collect();

    SpannedDocument {
        blocks,
        diagnostics: collect_parse_diagnostics(src),
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

        if html_block_start(line) {
            let start = i;
            i += 1;
            while i < lines.len() && !lines[i].text.trim().is_empty() {
                i += 1;
            }
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

fn parse_blocks_with_refs(lines: &[&str], refs: &ReferenceMap) -> Vec<Block> {
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
            blocks.push(Block::Heading {
                level,
                inlines: parse_inlines_with_refs(text, refs),
            });
            i += 1;
            continue;
        }
        if let Some((fence_ch, fence_len, info)) = open_fence(line) {
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
            blocks.push(Block::CodeBlock { lang, code });
            continue;
        }
        if indented_code_start(line) {
            let (code, used) = parse_indented_code(&lines[i..]);
            blocks.push(Block::CodeBlock { lang: None, code });
            i += used;
            continue;
        }
        if line.trim_start().starts_with('>') {
            let mut inner = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                inner.push(strip_blockquote(lines[i]));
                i += 1;
            }
            let inner_refs: Vec<&str> = inner.iter().map(String::as_str).collect();
            blocks.push(Block::BlockQuote(parse_blocks_with_refs(&inner_refs, refs)));
            continue;
        }
        if html_block_start(line) {
            let start = i;
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() {
                i += 1;
            }
            blocks.push(Block::HtmlBlock(lines[start..i].join("\n")));
            continue;
        }
        if i + 1 < lines.len() && line.contains('|') && is_table_delimiter(lines[i + 1]) {
            if let Some((table, used)) = parse_table(&lines[i..], refs) {
                blocks.push(Block::Table(table));
                i += used;
                continue;
            }
        }
        if list_marker(line).is_some() {
            let (list, used) = parse_list(&lines[i..], refs);
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
                let text = lines[start..i].join("\n");
                blocks.push(Block::Heading {
                    level,
                    inlines: parse_inlines_with_refs(&text, refs),
                });
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
        let text = lines[start..i].join("\n");
        blocks.push(Block::Paragraph(parse_inlines_with_refs(&text, refs)));
    }
    blocks
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

fn html_block_start(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("<!--") || t.starts_with("<!") || t.starts_with("<?") {
        return true;
    }
    let Some(name) = html_tag_name(t) else {
        return false;
    };
    is_html_block_tag(name)
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

struct Marker {
    indent: usize,
    ordered: bool,
    start: u64,
    content_indent: usize,
    rest: String,
}

fn list_marker(line: &str) -> Option<Marker> {
    let indent = leading_spaces(line);
    let t = &line[indent..];
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
            rest: rest.to_string(),
        });
    }
    // Ordered: digits then `.` or `)` then space.
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty() && digits.len() <= 9 {
        let after = &t[digits.len()..];
        if (after.starts_with('.') || after.starts_with(')'))
            && let Ok(start) = digits.parse()
            && let Some((rest, padding)) = marker_padding(&after[1..])
        {
            return Some(Marker {
                indent,
                ordered: true,
                start,
                content_indent: indent + digits.len() + 1 + padding,
                rest: rest.to_string(),
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
        let mut item_lines = vec![m.rest.clone()];
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
                if list_marker(&stripped).is_some_and(|marker| marker.ordered && marker.start != 1)
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
            blocks: parse_blocks_with_refs(&item_refs, refs),
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
        .map(|c| parse_inlines_with_refs(c, refs))
        .collect();
    let mut rows = Vec::new();
    let mut i = 2;
    while i < lines.len() && !lines[i].trim().is_empty() && lines[i].contains('|') {
        let mut cells: Vec<Vec<Inline>> = split_table_row(lines[i])
            .iter()
            .map(|c| parse_inlines_with_refs(c, refs))
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
    let mut out = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut buf = String::new();
    let mut i = 0;
    let flush = |buf: &mut String, out: &mut Vec<Inline>| {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf)));
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
                flush(&mut buf, &mut out);
                out.push(if hard {
                    Inline::HardBreak
                } else {
                    Inline::SoftBreak
                });
                i += 1;
            }
            '`' => {
                let n = run_len(&bytes, i, '`');
                if let Some(end) = find_code_close(&bytes, i + n, '`', n) {
                    flush(&mut buf, &mut out);
                    let inner: String = bytes[i + n..end].iter().collect();
                    out.push(Inline::Code(normalize_code_span(&inner)));
                    i = end + n;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '!' if i + 1 < bytes.len() && bytes[i + 1] == '[' => {
                if let Some((alt, dest, title, next)) = parse_link_like(&bytes, i + 1, refs) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Image {
                        dest,
                        title,
                        alt: inlines_to_plain(&alt),
                    });
                    i = next;
                } else if let Some((alt, dest, title, next)) =
                    parse_reference_link_like(&bytes, i + 1, refs)
                {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Image {
                        dest,
                        title,
                        alt: inlines_to_plain(&alt),
                    });
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '[' => {
                if let Some((content, dest, title, next)) = parse_link_like(&bytes, i, refs) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Link {
                        dest,
                        title,
                        content,
                    });
                    i = next;
                } else if let Some((content, dest, title, next)) =
                    parse_reference_link_like(&bytes, i, refs)
                {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Link {
                        dest,
                        title,
                        content,
                    });
                    i = next;
                } else if let Some((html, next)) = parse_inline_html(&bytes, i) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Html(html));
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '<' => {
                if let Some((label, dest, next)) = parse_autolink(&bytes, i) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Link {
                        dest,
                        title: None,
                        content: vec![Inline::Text(label)],
                    });
                    i = next;
                } else if let Some((html, next)) = parse_inline_html(&bytes, i) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Html(html));
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '&' => {
                if let Some((ch, next)) = parse_character_reference(&bytes, i) {
                    buf.push(ch);
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '~' if run_len(&bytes, i, '~') >= 2 => {
                if let Some((inner, next)) = parse_delim(&bytes, i, '~', 2) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Strikethrough(parse_inlines_with_refs(&inner, refs)));
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '*' | '_' => {
                let n = run_len(&bytes, i, c);
                if is_intraword_underscore_run(&bytes, i, n) {
                    for _ in 0..n {
                        buf.push(c);
                    }
                    i += n;
                    continue;
                }
                // Triple delimiter run: `***x***` / `___x___` is emphasis *and*
                // strong (bold-italic). Try it before the 2/1 split so the run is
                // not greedily consumed as `**` + a stray `*`.
                if n >= 3
                    && let Some((inner, next)) = parse_delim(&bytes, i, c, 3)
                {
                    flush(&mut buf, &mut out);
                    let parsed = parse_inlines_with_refs(&inner, refs);
                    out.push(Inline::Strong(vec![Inline::Emphasis(parsed)]));
                    i = next;
                    continue;
                }
                let want = if n >= 2 { 2 } else { 1 };
                if let Some((inner, next)) = parse_delim(&bytes, i, c, want) {
                    flush(&mut buf, &mut out);
                    let parsed = parse_inlines_with_refs(&inner, refs);
                    out.push(if want == 2 {
                        Inline::Strong(parsed)
                    } else {
                        Inline::Emphasis(parsed)
                    });
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            _ => {
                if let Some((label, dest, next)) = parse_bare_url_autolink(&bytes, i) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Link {
                        dest,
                        title: None,
                        content: vec![Inline::Text(label)],
                    });
                    i = next;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
        }
    }
    flush(&mut buf, &mut out);
    out
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
        parse_inlines_with_refs(&text, refs),
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
        parse_inlines_with_refs(&text, refs),
        reference.dest.clone(),
        reference.title.clone(),
        next,
    ))
}

fn parse_autolink(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    if chars.get(i) != Some(&'<') {
        return None;
    }
    let mut j = i + 1;
    let mut url = String::new();
    while j < chars.len() && chars[j] != '>' && chars[j] != ' ' && chars[j] != '\n' {
        url.push(chars[j]);
        j += 1;
    }
    if chars.get(j) == Some(&'>') && (url.contains("://") || url.contains('@')) {
        let label = url;
        let dest = if label.contains('@') && !label.contains("://") {
            format!("mailto:{label}")
        } else {
            label.clone()
        };
        Some((label, dest, j + 1))
    } else {
        None
    }
}

fn parse_character_reference(chars: &[char], i: usize) -> Option<(char, usize)> {
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
    let decoded = if let Some(numeric) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X"))
    {
        decode_numeric_reference(numeric, 16)
    } else if let Some(numeric) = body.strip_prefix('#') {
        decode_numeric_reference(numeric, 10)
    } else {
        decode_named_reference(&body)
    }?;
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
    let code = u32::from_str_radix(value, radix).ok()?;
    char::from_u32(code)
}

const fn decode_named_reference(name: &str) -> Option<char> {
    match name.as_bytes() {
        b"amp" => Some('&'),
        b"lt" => Some('<'),
        b"gt" => Some('>'),
        b"quot" => Some('"'),
        b"apos" => Some('\''),
        b"nbsp" => Some('\u{00a0}'),
        b"copy" => Some('\u{00a9}'),
        b"reg" => Some('\u{00ae}'),
        b"trade" => Some('\u{2122}'),
        b"ndash" => Some('\u{2013}'),
        b"mdash" => Some('\u{2014}'),
        _ => None,
    }
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

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|&c| c == ' ').count()
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

fn strip_n(line: &str, n: usize) -> &str {
    let take = line.chars().take(n).take_while(|&c| c == ' ').count();
    &line[take..]
}

fn is_ascii_punct(c: char) -> bool {
    c.is_ascii_punctuation()
}
