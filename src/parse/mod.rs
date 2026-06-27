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

fn collect_link_references<'a>(lines: &[&'a str]) -> (Vec<&'a str>, ReferenceMap) {
    let mut refs = ReferenceMap::new();
    let mut kept = Vec::with_capacity(lines.len());
    for line in lines {
        if let Some((label, reference)) = parse_reference_definition(line) {
            refs.entry(label).or_insert(reference);
        } else {
            kept.push(*line);
        }
    }
    (kept, refs)
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
                || lines[i].trim_start().starts_with('>')
                || list_marker(lines[i]).is_some()
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

// ---- block detectors --------------------------------------------------------

fn atx_heading(line: &str) -> Option<(u8, &str)> {
    let t = line.trim_start();
    let hashes = t.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &t[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None; // `#text` is not a heading
    }
    // Strip an optional closing run of `#` and surrounding spaces.
    let content = rest.trim().trim_end_matches('#').trim_end();
    Some((hashes as u8, content))
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
    let t = line.trim_start();
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
    let t = line.trim();
    t.chars().all(|c| c == ch) && t.chars().count() >= len && !t.is_empty()
}

fn strip_blockquote(line: &str) -> String {
    let t = line.trim_start();
    let rest = t.strip_prefix('>').unwrap_or(t);
    rest.strip_prefix(' ').unwrap_or(rest).to_string()
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
        && t[first.len_utf8()..].starts_with(' ')
    {
        let rest = t[first.len_utf8() + 1..].to_string();
        return Some(Marker {
            indent,
            ordered: false,
            start: 1,
            content_indent: indent + 2,
            rest,
        });
    }
    // Ordered: digits then `.` or `)` then space.
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty() && digits.len() <= 9 {
        let after = &t[digits.len()..];
        if (after.starts_with(". ") || after.starts_with(") "))
            && let Ok(start) = digits.parse()
        {
            let rest = after[2..].to_string();
            return Some(Marker {
                indent,
                ordered: true,
                start,
                content_indent: indent + digits.len() + 2,
                rest,
            });
        }
    }
    None
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
            {
                break;
            }

            if leading_spaces(lines[i]) >= m.content_indent {
                item_lines.push(strip_n(lines[i], m.content_indent).to_string());
            } else {
                // CommonMark lazy continuation: an unindented, non-marker line
                // continues the current paragraph/list item.
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
    // Split on unescaped `|`.
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut prev_backslash = false;
    for c in t.chars() {
        if c == '|' && !prev_backslash {
            cells.push(cur.trim().to_string());
            cur = String::new();
        } else {
            if c == '\\' && !prev_backslash {
                prev_backslash = true;
                cur.push(c);
                continue;
            }
            cur.push(c);
        }
        prev_backslash = false;
    }
    cells.push(cur.trim().to_string());
    cells
}

fn parse_table(lines: &[&str], refs: &ReferenceMap) -> Option<(Table, usize)> {
    let header = split_table_row(lines[0]);
    let align_cells = split_table_row(lines[1]);
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
    let cols = header.len();
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
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '<' => {
                if let Some((url, next)) = parse_autolink(&bytes, i) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Link {
                        dest: url.clone(),
                        title: None,
                        content: vec![Inline::Text(url)],
                    });
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
                buf.push(c);
                i += 1;
            }
        }
    }
    flush(&mut buf, &mut out);
    out
}

fn run_len(chars: &[char], i: usize, ch: char) -> usize {
    chars[i..].iter().take_while(|&&c| c == ch).count()
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
            if run >= want && j > after && chars[j - 1] != ' ' {
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
    // Parse the destination + optional "title" up to the matching ')'.
    let mut k = j + 2;
    let mut dest = String::new();
    while k < chars.len() && chars[k] != ')' && chars[k] != ' ' && chars[k] != '"' {
        dest.push(chars[k]);
        k += 1;
    }
    let mut title = None;
    while k < chars.len() && chars[k] == ' ' {
        k += 1;
    }
    if chars.get(k) == Some(&'"') {
        let mut t = String::new();
        k += 1;
        while k < chars.len() && chars[k] != '"' {
            t.push(chars[k]);
            k += 1;
        }
        title = Some(t);
        k += 1;
        while k < chars.len() && chars[k] == ' ' {
            k += 1;
        }
    }
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

fn parse_autolink(chars: &[char], i: usize) -> Option<(String, usize)> {
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
        let dest = if url.contains('@') && !url.contains("://") {
            format!("mailto:{url}")
        } else {
            url
        };
        Some((dest, j + 1))
    } else {
        None
    }
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

fn strip_n(line: &str, n: usize) -> &str {
    let take = line.chars().take(n).take_while(|&c| c == ' ').count();
    &line[take..]
}

fn is_ascii_punct(c: char) -> bool {
    c.is_ascii_punctuation()
}
