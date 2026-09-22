//! Exact source tokens for destinations recognized by the normal parser.
//!
//! Tracking is opt-in. The ordinary AST stays unchanged; borrowed source slices
//! and the parser's explicitly copied text retain byte coordinates only while
//! this collector is active. There is no second Markdown grammar or source-text
//! search, so equal-looking links in different includes keep distinct origins.

use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DestinationSpan {
    /// The complete source token, including angle delimiters when present.
    pub range: Range<usize>,
    /// The parser-decoded destination, without a title or angle delimiters.
    pub dest: String,
}

pub(crate) fn destination_spans(source: &str) -> Vec<DestinationSpan> {
    if !source.contains('[') {
        return Vec::new();
    }
    let mut profiler = ParseProfiler::disabled();
    profiler.destinations = Some(DestinationTracker::new(source));
    let _document = parse_document_inner(source, &mut profiler);
    let mut spans = profiler.destinations.map_or_else(Vec::new, |tracker| tracker.spans);
    spans.sort_by_key(|span| (span.range.start, span.range.end));
    spans.dedup_by(|right, left| right.range == left.range);
    spans
}

#[derive(Clone)]
struct TextRun {
    copied: Range<usize>,
    original: Range<usize>,
}

struct TextMap {
    len: usize,
    runs: Vec<TextRun>,
}

struct CharRun {
    chars: Range<usize>,
    original: Range<usize>,
}

struct CharMap {
    address: usize,
    widths: Vec<u8>,
    runs: Vec<CharRun>,
}

impl CharMap {
    fn push(&mut self, width: usize, original: Option<Range<usize>>) {
        let index = self.widths.len();
        self.widths.push(width as u8);
        let Some(original) = original else { return; };
        if let Some(last) = self.runs.last_mut() {
            // Byte checkpoints bound conversion from a char index while keeping
            // tracking much smaller than an Option<Range<usize>> per character.
            if last.chars.end == index && last.original.end == original.start
                && last.chars.len() < 128
            {
                last.chars.end += 1;
                last.original.end = original.end;
                return;
            }
        }
        self.runs.push(CharRun { chars: index..index + 1, original });
    }

    fn source_range(&self, token: Range<usize>) -> Option<Range<usize>> {
        let mut index = self.runs.partition_point(|run| run.chars.end <= token.start);
        let first = self.runs.get(index)?;
        if first.chars.start > token.start { return None; }
        let start = first.original.start + self.widths[first.chars.start..token.start]
            .iter().map(|width| usize::from(*width)).sum::<usize>();
        let mut cursor = token.start;
        let mut end = start;
        while cursor < token.end {
            let run = self.runs.get(index)?;
            if run.chars.start > cursor { return None; }
            let original = run.original.start + self.widths[run.chars.start..cursor]
                .iter().map(|width| usize::from(*width)).sum::<usize>();
            if original != end { return None; }
            let next = token.end.min(run.chars.end);
            end += self.widths[cursor..next].iter().map(|width| usize::from(*width)).sum::<usize>();
            cursor = next;
            index += 1;
        }
        Some(start..end)
    }
}

pub(super) struct DestinationTracker {
    source_address: usize,
    source_len: usize,
    text: BTreeMap<usize, TextMap>,
    text_order: Vec<usize>,
    chars: Vec<CharMap>,
    spans: Vec<DestinationSpan>,
}

impl DestinationTracker {
    fn new(source: &str) -> Self {
        Self {
            source_address: source.as_ptr() as usize,
            source_len: source.len(),
            text: BTreeMap::new(),
            text_order: Vec::new(),
            chars: Vec::new(),
            spans: Vec::new(),
        }
    }

    pub(super) fn checkpoint(&self) -> usize {
        self.text_order.len()
    }

    pub(super) fn restore(&mut self, checkpoint: usize) {
        for address in self.text_order.drain(checkpoint..) {
            self.text.remove(&address);
        }
    }

    fn text_range(&self, text: &str) -> Option<Range<usize>> {
        let address = text.as_ptr() as usize;
        if let Some(offset) = address.checked_sub(self.source_address) {
            if text.len() <= self.source_len.saturating_sub(offset) && offset <= self.source_len {
                return Some(offset..offset + text.len());
            }
        }
        let (&base, map) = self.text.range(..=address).next_back()?;
        let offset = address.checked_sub(base)?;
        if offset > map.len || text.len() > map.len - offset {
            return None;
        }
        let end = offset + text.len();
        let index = map.runs.partition_point(|run| run.copied.end <= offset);
        let run = map.runs.get(index)?;
        if run.copied.start > offset || run.copied.end < end {
            return None;
        }
        let start = run.original.start + offset - run.copied.start;
        Some(start..start + text.len())
    }

    fn push_run(runs: &mut Vec<TextRun>, copied: Range<usize>, original: Range<usize>) {
        if copied.is_empty() {
            return;
        }
        if let Some(last) = runs.last_mut() {
            if last.copied.end == copied.start && last.original.end == original.start {
                last.copied.end = copied.end;
                last.original.end = original.end;
                return;
            }
        }
        runs.push(TextRun { copied, original });
    }

    /// Register exact copied pieces. Inserted separators have no source bytes.
    pub(super) fn map_parts(&mut self, output: &str, parts: &[(usize, &str)]) {
        let mut runs = Vec::new();
        for &(offset, part) in parts {
            for (index, character) in part.char_indices() {
                let end = index + character.len_utf8();
                if let Some(original) = self.text_range(&part[index..end]) {
                    Self::push_run(&mut runs, offset + index..offset + end, original);
                }
            }
        }
        if !runs.is_empty() {
            let address = output.as_ptr() as usize;
            self.text.insert(address, TextMap { len: output.len(), runs });
            self.text_order.push(address);
        }
    }

    /// Container helpers only alter a prefix (markers, task boxes, or tabs).
    /// Preserve the exact shared suffix; synthesized prefix characters are not
    /// assigned invented source positions.
    pub(super) fn map_suffix(&mut self, output: &str, original: &str) {
        let mut common = output.bytes().rev().zip(original.bytes().rev())
            .take_while(|(left, right)| left == right).count();
        while common > 0 && (!output.is_char_boundary(output.len() - common)
            || !original.is_char_boundary(original.len() - common))
        {
            common -= 1;
        }
        if common > 0 {
            self.map_parts(output, &[(output.len() - common, &original[original.len() - common..])]);
        }
    }

    fn append_chars(&self, map: &mut CharMap, text: &str) {
        for (index, character) in text.char_indices() {
            let width = character.len_utf8();
            map.push(width, self.text_range(&text[index..index + width]));
        }
    }

    pub(super) fn push_chars_text(&mut self, chars: &[char], text: &str) {
        let mut map = CharMap { address: chars.as_ptr() as usize,
            widths: Vec::with_capacity(chars.len()), runs: Vec::new() };
        self.append_chars(&mut map, text);
        self.chars.push(map);
    }

    pub(super) fn push_chars_lines(&mut self, chars: &[char], lines: &[&str]) {
        let mut map = CharMap { address: chars.as_ptr() as usize,
            widths: Vec::with_capacity(chars.len()), runs: Vec::new() };
        for (index, line) in lines.iter().enumerate() {
            if index > 0 { map.push(1, None); }
            let text = line.trim_start_matches([' ', '\t']);
            let text = if index + 1 == lines.len() {
                text.trim_end_matches([' ', '\t'])
            } else { text };
            self.append_chars(&mut map, text);
        }
        self.chars.push(map);
    }

    pub(super) fn pop_chars(&mut self) {
        self.chars.pop();
    }

    pub(super) fn record(&mut self, chars: &[char], token: Range<usize>, dest: &str) {
        if token.is_empty() || dest.is_empty() {
            return;
        }
        let address = chars.as_ptr() as usize;
        let char_size = std::mem::size_of::<char>();
        let Some((map, offset)) = self.chars.iter().rev().find_map(|map| {
            let bytes = address.checked_sub(map.address)?;
            if bytes % char_size != 0 { return None; }
            let offset = bytes / char_size;
            (offset <= map.widths.len()
                && chars.len() <= map.widths.len() - offset).then_some((map, offset))
        }) else { return; };
        let Some(range) = map.source_range(offset + token.start..offset + token.end) else { return; };
        self.spans.push(DestinationSpan { range, dest: dest.to_string() });
    }

    /// Invoked only after the real paragraph-aware collector accepts a def.
    pub(super) fn reference(&mut self, lines: &[&str], dest: &str) {
        let joined = lines.join("\n");
        let mut offset = 0;
        let parts: Vec<_> = lines.iter().map(|line| {
            let part = (offset, *line);
            offset += line.len() + 1;
            part
        }).collect();
        let checkpoint = self.checkpoint();
        self.map_parts(&joined, &parts);
        let chars: Vec<_> = joined.chars().collect();
        self.push_chars_text(&chars, &joined);
        let token = (|| {
            let mut index = 0;
            skip_spaces(&chars, &mut index);
            let close = find_closing_bracket(&chars, index)?;
            if chars.get(close + 1) != Some(&':') { return None; }
            index = close + 2;
            skip_spaces_or_newline(&chars, &mut index);
            let start = index;
            // The parser's ASCII reference fast path deliberately accepts a
            // complete non-whitespace bare token, including unmatched parens.
            if lines.first().and_then(|line| parse_simple_ascii_reference_definition(line))
                .is_some_and(|(_, reference)| reference.dest == dest)
            {
                if chars.get(index) == Some(&'<') {
                    index += 1;
                    while chars.get(index).is_some_and(|character| *character != '>') { index += 1; }
                    if chars.get(index) != Some(&'>') { return None; }
                    index += 1;
                } else {
                    while chars.get(index).is_some_and(|character| !character.is_ascii_whitespace()) { index += 1; }
                }
            } else {
                if parse_link_destination(&chars, &mut index)?.as_str() != dest { return None; }
            }
            Some(start..index)
        })();
        if let Some(token) = token { self.record(&chars, token, dest); }
        self.pop_chars();
        self.restore(checkpoint);
    }
}

impl ParseProfiler {
    pub(super) fn destination_checkpoint(&self) -> usize {
        self.destinations.as_ref().map_or(0, DestinationTracker::checkpoint)
    }

    pub(super) fn destination_restore(&mut self, checkpoint: usize) {
        if let Some(tracker) = &mut self.destinations { tracker.restore(checkpoint); }
    }

    pub(super) fn destination_suffix(&mut self, output: &str, original: &str) {
        if let Some(tracker) = &mut self.destinations { tracker.map_suffix(output, original); }
    }

    pub(super) fn destination_parts(&mut self, output: &str, parts: &[(usize, &str)]) {
        if let Some(tracker) = &mut self.destinations { tracker.map_parts(output, parts); }
    }

    pub(super) fn destination_mark(&self) -> usize {
        self.destinations.as_ref().map_or(0, |tracker| tracker.spans.len())
    }

    pub(super) fn destination_rollback(&mut self, mark: usize) {
        if let Some(tracker) = &mut self.destinations { tracker.spans.truncate(mark); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(source: &str) -> Vec<(&str, String)> {
        destination_spans(source).into_iter()
            .map(|span| (&source[span.range], span.dest)).collect()
    }

    #[test]
    fn decoded_destinations_keep_exact_original_tokens_and_titles() {
        let source = "中 [a](<my file.svg?x=1&amp;y=2#part> \"Title\") ![b](a\\(b\\).png 'alt')";
        assert_eq!(tokens(source), [
            ("<my file.svg?x=1&amp;y=2#part>", "my file.svg?x=1&y=2#part".into()),
            ("a\\(b\\).png", "a(b).png".into()),
        ]);
    }

    #[test]
    fn code_html_math_and_invalid_outer_links_do_not_create_tokens() {
        let source = "`[x](inline.png)` $[x](math.png)$\n\n```md\n[x](fence.png)\n```\n\n    [x](indent.png)\n\n<div>\n[x](html.png)\n</div>\n\n[a [b](inner.md)](outer.md)\n";
        assert_eq!(tokens(source), [("inner.md", "inner.md".into())]);
    }

    #[test]
    fn reference_definitions_use_the_definition_token_and_skip_literal_defs() {
        let source = "[use][id] ![image][img]\n\n[id]:\n  <guide/next.md#top>\n  \"Title\"\n\n> [img]: assets/a.png\n\n```\n[bad]: literal.png\n```\n";
        assert_eq!(tokens(source), [
            ("<guide/next.md#top>", "guide/next.md#top".into()),
            ("assets/a.png", "assets/a.png".into()),
        ]);
    }

    #[test]
    fn repeated_nested_container_text_keeps_distinct_source_offsets() {
        let source = "- [x] [same](pic.png)\n- [x] [same](pic.png)\n\n>\t[same](pic.png)\n\n| cell |\n| --- |\n| [same](pic.png) |\n\n[^note]: [same](pic.png)\n    ![same](other.png)\n\nTerm\n: [same](pic.png)\n";
        let spans = destination_spans(source);
        assert_eq!(spans.len(), 7);
        assert!(spans.windows(2).all(|pair| pair[0].range.end <= pair[1].range.start));
        assert_eq!(spans.iter().filter(|span| span.dest == "pic.png").count(), 6);
        for span in spans { assert_eq!(&source[span.range], span.dest); }
    }

    #[test]
    fn image_descriptions_do_not_expose_hidden_image_or_link_destinations() {
        let source = "![alt [link](hidden.md) ![nested](hidden.png)](visible.png) [![yes](image.png)](link.md)";
        assert_eq!(tokens(source), [
            ("visible.png", "visible.png".into()),
            ("image.png", "image.png".into()),
            ("link.md", "link.md".into()),
        ]);
    }
}
