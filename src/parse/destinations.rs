//! Exact source tokens for destinations recognized by the normal parser.
//!
//! Tracking is opt-in. The ordinary AST stays unchanged; borrowed source slices
//! and the parser's explicitly copied text retain byte coordinates only while
//! this collector is active. There is no second Markdown grammar or source-text
//! search, so equal-looking links in different includes keep distinct origins.

use super::*;
use std::collections::BTreeMap;

/// One exact destination token in the source passed to [`super::destination_spans`].
/// A token can supply an inline link, image or reference definition; its source
/// range alone does not establish which semantic nodes use that destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationSpan {
    /// The complete source token, including angle delimiters when present.
    pub range: Range<usize>,
    /// The parser-decoded destination, without a title or angle delimiters.
    pub dest: String,
}

impl DestinationSpan {
    /// Return the original UTF-8 byte range after the first decoded `#`, without
    /// an angle token's closing `>`. The separator's original spelling and every
    /// path/query byte stay outside this range, including `\#`, `&#35;` and
    /// `&num;`. Percent escapes are URI data and are not decoded here.
    ///
    /// Uses the normal parser's escape and character-reference decoder. Returns
    /// `None` for invalid/stale token bounds, a decoded value different from
    /// `dest`, or a destination without a fragment separator. This checks token
    /// spelling, not its semantic role or the caller's source revision identity.
    #[must_use]
    pub fn fragment_range(&self, source: &str) -> Option<Range<usize>> {
        let raw = source.get(self.range.clone())?;
        let (body, start, end) = if raw.starts_with('<') {
            (
                raw.strip_prefix('<')?.strip_suffix('>')?,
                self.range.start + 1,
                self.range.end - 1,
            )
        } else {
            (raw, self.range.start, self.range.end)
        };
        let chars: Vec<_> = body.chars().collect();
        let mut decoded = String::with_capacity(body.len());
        let mut index = 0;
        let mut byte_offset = 0;
        let mut fragment = None;
        while index < chars.len() {
            let previous = index;
            let hash = match chars[index] {
                '\\' if chars.get(index + 1).is_some_and(|&ch| is_ascii_punct(ch)) => {
                    let ch = chars[index + 1];
                    decoded.push(ch);
                    index += 2;
                    ch == '#'
                }
                '&' => {
                    if let Some((text, next)) = parse_character_reference(&chars, index) {
                        // No current multi-scalar entity contains '#'. Refuse a
                        // future mixed expansion rather than invent its suffix.
                        if text.contains('#') && text != "#" {
                            return None;
                        }
                        let hash = text == "#";
                        decoded.push_str(&text);
                        index = next;
                        hash
                    } else {
                        decoded.push('&');
                        index += 1;
                        false
                    }
                }
                ch => {
                    decoded.push(ch);
                    index += 1;
                    ch == '#'
                }
            };
            byte_offset += chars[previous..index]
                .iter()
                .map(|ch| ch.len_utf8())
                .sum::<usize>();
            if fragment.is_none() && hash {
                fragment = Some(start + byte_offset);
            }
        }
        (decoded == self.dest)
            .then_some(fragment)
            .flatten()
            .map(|start| start..end)
    }
}

pub(crate) fn destination_spans(source: &str) -> Vec<DestinationSpan> {
    if !source.contains('[') {
        return Vec::new();
    }
    tracked_destinations(source, false)
        .into_iter()
        .filter(|span| span.syntax == DestinationSyntax::Markdown)
        .map(|span| span.token)
        .collect()
}

/// Internal source edits need the token's syntax, while the public Markdown
/// destination API deliberately retains its existing recognition contract.
pub(crate) struct SourceDestination {
    pub token: DestinationSpan,
    pub syntax: DestinationSyntax,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DestinationSyntax {
    Markdown,
    HtmlAttribute,
    HtmlAttributeContent,
}

#[cfg(feature = "cli")]
pub(crate) fn resource_destination_spans(source: &str) -> Vec<SourceDestination> {
    if !source.contains(['[', '<']) {
        return Vec::new();
    }
    tracked_destinations(source, true)
}

fn tracked_destinations(source: &str, html: bool) -> Vec<SourceDestination> {
    let mut profiler = ParseProfiler::disabled();
    let mut tracker = DestinationTracker::new(source);
    tracker.html = html;
    profiler.destinations = Some(tracker);
    let _document = parse_document_inner(source, &mut profiler);
    let mut spans = profiler
        .destinations
        .map_or_else(Vec::new, |tracker| tracker.spans);
    spans.sort_by_key(|span| (span.token.range.start, span.token.range.end));
    spans.dedup_by(|right, left| right.token.range == left.token.range);
    spans
}

/// Source envelopes for fenced code owned by lists, quotes, or footnotes.
/// Transclusion uses these to keep container examples literal without guessing
/// a second container grammar. Reuse the block metadata walk: no inline AST,
/// HTML rendering, or ordinary destination tokens are produced in this mode.
/// Top-level include/fence recognition remains the expansion engine's contract.
pub(crate) fn container_fence_ranges(
    source: &str,
    skip_line: impl Fn(&str) -> bool,
) -> Vec<Range<usize>> {
    if !source.contains(['`', '~']) {
        return Vec::new();
    }
    let mut tracker = DestinationTracker::new(source);
    tracker.container_fences = Some(Vec::new());
    let body = document_body(source);
    // Named snippets remove anchor-marker lines before expansion. Omit those
    // same borrowed slices here, retaining their neighbors' original addresses.
    let mut originals = split_logical_lines(body);
    originals.retain(|line| !skip_line(line));
    let tab_storage: Option<Vec<String>> = body.contains('\t').then(|| {
        originals
            .iter()
            .map(|line| expand_leading_tabs(line).into_owned())
            .collect()
    });
    if let Some(storage) = &tab_storage {
        for (copied, original) in storage.iter().zip(&originals) {
            tracker.map_suffix(copied, original);
        }
    }
    let lines = if let Some(storage) = &tab_storage {
        storage.iter().map(String::as_str).collect()
    } else {
        originals
    };
    collect_link_reference_metadata_into_tracked(
        &lines,
        None,
        &mut ReferenceMap::new(),
        Some(&mut tracker),
    );
    let mut ranges = tracker.container_fences.unwrap_or_default();
    ranges.sort_unstable_by_key(|range| range.start);
    ranges
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
        let Some(original) = original else {
            return;
        };
        if let Some(last) = self.runs.last_mut() {
            // Byte checkpoints bound conversion from a char index while keeping
            // tracking much smaller than an Option<Range<usize>> per character.
            if last.chars.end == index
                && last.original.end == original.start
                && last.chars.len() < 128
            {
                last.chars.end += 1;
                last.original.end = original.end;
                return;
            }
        }
        self.runs.push(CharRun {
            chars: index..index + 1,
            original,
        });
    }

    fn source_range(&self, token: Range<usize>) -> Option<Range<usize>> {
        let mut index = self
            .runs
            .partition_point(|run| run.chars.end <= token.start);
        let first = self.runs.get(index)?;
        if first.chars.start > token.start {
            return None;
        }
        let start = first.original.start
            + self.widths[first.chars.start..token.start]
                .iter()
                .map(|width| usize::from(*width))
                .sum::<usize>();
        let mut cursor = token.start;
        let mut end = start;
        while cursor < token.end {
            let run = self.runs.get(index)?;
            if run.chars.start > cursor {
                return None;
            }
            let original = run.original.start
                + self.widths[run.chars.start..cursor]
                    .iter()
                    .map(|width| usize::from(*width))
                    .sum::<usize>();
            if original != end {
                return None;
            }
            let next = token.end.min(run.chars.end);
            end += self.widths[cursor..next]
                .iter()
                .map(|width| usize::from(*width))
                .sum::<usize>();
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
    // A parser scope may register the same allocation more than once. Keep
    // the displaced map so unwinding restores its caller's source identity.
    text_order: Vec<(usize, Option<TextMap>)>,
    chars: Vec<CharMap>,
    spans: Vec<SourceDestination>,
    html: bool,
    /// Present only for the metadata-only transclusion fence query.
    container_fences: Option<Vec<Range<usize>>>,
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
            html: false,
            container_fences: None,
        }
    }

    /// Map the first and last literal characters while the container's copied
    /// suffixes are still registered. The envelope includes original line
    /// prefixes/newlines; callers use it to classify whole source lines, never
    /// claim that stripped Markdown markers are rendered code characters.
    pub(super) fn container_fence(&mut self, lines: &[&str]) {
        if self.container_fences.is_none() {
            return;
        }
        let first = lines.iter().find_map(|line| {
            let text = line.trim();
            let width = text.chars().next()?.len_utf8();
            self.text_range(&text[..width])
        });
        let last = lines.iter().rev().find_map(|line| {
            let text = line.trim();
            let width = text.chars().next_back()?.len_utf8();
            self.text_range(&text[text.len() - width..])
        });
        if let (Some(first), Some(last), Some(ranges)) = (first, last, &mut self.container_fences) {
            ranges.push(first.start..last.end);
        }
    }

    pub(super) fn checkpoint(&self) -> usize {
        self.text_order.len()
    }

    pub(super) fn restore(&mut self, checkpoint: usize) {
        for (address, previous) in self.text_order.drain(checkpoint..).rev() {
            if let Some(map) = previous {
                self.text.insert(address, map);
            } else {
                self.text.remove(&address);
            }
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
        if !output.is_empty() {
            let address = output.as_ptr() as usize;
            // An entirely synthesized replacement must shadow an older map
            // too; otherwise reused storage can acquire an invented origin.
            let previous = self.text.insert(
                address,
                TextMap {
                    len: output.len(),
                    runs,
                },
            );
            self.text_order.push((address, previous));
        }
    }

    /// Container helpers only alter a prefix (markers, task boxes, or tabs).
    /// Preserve the exact shared suffix; synthesized prefix characters are not
    /// assigned invented source positions.
    pub(super) fn map_suffix(&mut self, output: &str, original: &str) {
        let mut common = output
            .bytes()
            .rev()
            .zip(original.bytes().rev())
            .take_while(|(left, right)| left == right)
            .count();
        while common > 0
            && (!output.is_char_boundary(output.len() - common)
                || !original.is_char_boundary(original.len() - common))
        {
            common -= 1;
        }
        if common > 0 {
            self.map_parts(
                output,
                &[(output.len() - common, &original[original.len() - common..])],
            );
        } else {
            self.map_parts(output, &[]);
        }
    }

    fn append_chars(&self, map: &mut CharMap, text: &str) {
        for (index, character) in text.char_indices() {
            let width = character.len_utf8();
            map.push(width, self.text_range(&text[index..index + width]));
        }
    }

    pub(super) fn push_chars_text(&mut self, chars: &[char], text: &str) {
        let mut map = CharMap {
            address: chars.as_ptr() as usize,
            widths: Vec::with_capacity(chars.len()),
            runs: Vec::new(),
        };
        self.append_chars(&mut map, text);
        self.chars.push(map);
    }

    pub(super) fn push_chars_lines(&mut self, chars: &[char], lines: &[&str]) {
        let mut map = CharMap {
            address: chars.as_ptr() as usize,
            widths: Vec::with_capacity(chars.len()),
            runs: Vec::new(),
        };
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                map.push(1, None);
            }
            let text = line.trim_start_matches([' ', '\t']);
            let text = if index + 1 == lines.len() {
                text.trim_end_matches([' ', '\t'])
            } else {
                text
            };
            self.append_chars(&mut map, text);
        }
        self.chars.push(map);
    }

    pub(super) fn pop_chars(&mut self) {
        self.chars.pop();
    }

    pub(super) fn record(&mut self, chars: &[char], token: Range<usize>, dest: &str) {
        self.record_token(chars, token, dest, DestinationSyntax::Markdown);
    }

    fn record_token(
        &mut self,
        chars: &[char],
        token: Range<usize>,
        dest: &str,
        syntax: DestinationSyntax,
    ) -> bool {
        if token.is_empty() || dest.is_empty() {
            return false;
        }
        let address = chars.as_ptr() as usize;
        let char_size = std::mem::size_of::<char>();
        let Some((map, offset)) = self.chars.iter().rev().find_map(|map| {
            let bytes = address.checked_sub(map.address)?;
            if bytes % char_size != 0 {
                return None;
            }
            let offset = bytes / char_size;
            (offset <= map.widths.len() && chars.len() <= map.widths.len() - offset)
                .then_some((map, offset))
        }) else {
            return false;
        };
        let Some(range) = map.source_range(offset + token.start..offset + token.end) else {
            return false;
        };
        self.spans.push(SourceDestination {
            token: DestinationSpan {
                range,
                dest: dest.to_string(),
            },
            syntax,
        });
        true
    }

    /// Invoked only after the Markdown parser accepts a real HTML node. Code,
    /// escaped tags, titles and failed speculative parses cannot supply URLs.
    pub(super) fn inline_html(&mut self, chars: &[char], start: usize, html: &str) {
        if !self.html {
            return;
        }
        for span in crate::safe_html::url_spans(html) {
            let begin = start + html[..span.range.start].chars().count();
            let end = begin + html[span.range.clone()].chars().count();
            if !self.record_token(
                chars,
                begin..end,
                &span.destination,
                DestinationSyntax::HtmlAttribute,
            ) && let Some(content) = span.quoted_content_range(html)
            {
                let begin = start + html[..content.start].chars().count();
                let end = begin + html[content].chars().count();
                self.record_token(
                    chars,
                    begin..end,
                    &span.destination,
                    DestinationSyntax::HtmlAttributeContent,
                );
            }
        }
    }

    pub(super) fn block_html(&mut self, html: &str, lines: &[&str]) {
        if !self.html {
            return;
        }
        let mut offset = 0;
        let parts: Vec<_> = lines
            .iter()
            .map(|line| {
                let part = (offset, *line);
                offset += line.len() + 1;
                part
            })
            .collect();
        let checkpoint = self.checkpoint();
        self.map_parts(html, &parts);
        for span in crate::safe_html::url_spans(html) {
            let mapped = self
                .text_range(&html[span.range.clone()])
                .map(|range| (range, DestinationSyntax::HtmlAttribute))
                .or_else(|| {
                    let content = span.quoted_content_range(html)?;
                    Some((
                        self.text_range(&html[content])?,
                        DestinationSyntax::HtmlAttributeContent,
                    ))
                });
            if let Some((range, syntax)) = mapped {
                self.spans.push(SourceDestination {
                    token: DestinationSpan {
                        range,
                        dest: span.destination,
                    },
                    syntax,
                });
            }
        }
        self.restore(checkpoint);
    }

    /// Invoked only after the real paragraph-aware collector accepts a def.
    pub(super) fn reference(&mut self, lines: &[&str], dest: &str) {
        if self.container_fences.is_some() {
            return;
        }
        let joined = lines.join("\n");
        let mut offset = 0;
        let parts: Vec<_> = lines
            .iter()
            .map(|line| {
                let part = (offset, *line);
                offset += line.len() + 1;
                part
            })
            .collect();
        let checkpoint = self.checkpoint();
        self.map_parts(&joined, &parts);
        let chars: Vec<_> = joined.chars().collect();
        self.push_chars_text(&chars, &joined);
        let token = (|| {
            let mut index = 0;
            skip_spaces(&chars, &mut index);
            let close = find_closing_bracket(&chars, index)?;
            if chars.get(close + 1) != Some(&':') {
                return None;
            }
            index = close + 2;
            skip_spaces_or_newline(&chars, &mut index);
            let start = index;
            // The parser's ASCII reference fast path deliberately accepts a
            // complete non-whitespace bare token, including unmatched parens.
            if lines
                .first()
                .and_then(|line| parse_simple_ascii_reference_definition(line))
                .is_some_and(|(_, reference)| reference.dest == dest)
            {
                if chars.get(index) == Some(&'<') {
                    index += 1;
                    while chars.get(index).is_some_and(|character| *character != '>') {
                        index += 1;
                    }
                    if chars.get(index) != Some(&'>') {
                        return None;
                    }
                    index += 1;
                } else {
                    while chars
                        .get(index)
                        .is_some_and(|character| !character.is_ascii_whitespace())
                    {
                        index += 1;
                    }
                }
            } else {
                if parse_link_destination(&chars, &mut index)?.as_str() != dest {
                    return None;
                }
            }
            Some(start..index)
        })();
        if let Some(token) = token {
            self.record(&chars, token, dest);
        }
        self.pop_chars();
        self.restore(checkpoint);
    }
}

impl ParseProfiler {
    pub(super) fn destination_checkpoint(&self) -> usize {
        self.destinations
            .as_ref()
            .map_or(0, DestinationTracker::checkpoint)
    }

    pub(super) fn destination_restore(&mut self, checkpoint: usize) {
        if let Some(tracker) = &mut self.destinations {
            tracker.restore(checkpoint);
        }
    }

    pub(super) fn destination_suffix(&mut self, output: &str, original: &str) {
        if let Some(tracker) = &mut self.destinations {
            tracker.map_suffix(output, original);
        }
    }

    pub(super) fn destination_parts(&mut self, output: &str, parts: &[(usize, &str)]) {
        if let Some(tracker) = &mut self.destinations {
            tracker.map_parts(output, parts);
        }
    }

    pub(super) fn destination_mark(&self) -> usize {
        self.destinations
            .as_ref()
            .map_or(0, |tracker| tracker.spans.len())
    }

    pub(super) fn destination_rollback(&mut self, mark: usize) {
        if let Some(tracker) = &mut self.destinations {
            tracker.spans.truncate(mark);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_spans_follow_real_html_nodes_and_preserve_markdown_only_api() {
        let source = "`<img src='code.svg'>`\n\n```html\n<a href='fenced.md'>x</a>\n```\n\n> 中 <img src='same.svg'> <a href='next.md?x=1&amp;y=2'>next</a>\n\n<div>\n<!-- <img src='comment.svg'> -->\n<img src=\"same.svg\" title=\"<a href='title.md'>\">\n<a href='block.md'>block</a>\n</div>\n\n[Markdown](real.md) ![<img src='alt.svg'>](image.svg)\n";
        let spans = tracked_destinations(source, true);
        let actual: Vec<_> = spans
            .iter()
            .map(|span| {
                (
                    &source[span.token.range.clone()],
                    span.token.dest.as_str(),
                    span.syntax != DestinationSyntax::Markdown,
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                ("'same.svg'", "same.svg", true),
                ("'next.md?x=1&amp;y=2'", "next.md?x=1&y=2", true),
                ("\"same.svg\"", "same.svg", true),
                ("'block.md'", "block.md", true),
                ("real.md", "real.md", false),
                ("image.svg", "image.svg", false),
            ]
        );
        let markdown: Vec<_> = destination_spans(source)
            .into_iter()
            .map(|span| span.dest)
            .collect();
        assert_eq!(markdown, ["real.md", "image.svg"]);
    }

    #[test]
    fn html_block_spans_keep_container_and_crlf_coordinates() {
        let source = "> <div>\r\n> <img src='one.svg'>\r\n> <a\r\n> href=\"next.md#target\">Next</a>\r\n> </div>\r\n\r\n- <img src='two.svg'>\r\n";
        let spans = tracked_destinations(source, true);
        let values: Vec<_> = spans
            .iter()
            .map(|span| &source[span.token.range.clone()])
            .collect();
        assert_eq!(values, ["'one.svg'", "\"next.md#target\"", "'two.svg'"]);
    }

    #[test]
    fn nested_registration_restores_each_displaced_utf8_origin() {
        let source = "同.png 同.png 同.png";
        let len = "同.png".len();
        let copied = source[..len].to_string();
        let mut tracker = DestinationTracker::new(source);
        let root = tracker.checkpoint();
        tracker.map_parts(&copied, &[(0, &source[..len])]);
        let outer = tracker.checkpoint();
        let second = len + 1;
        tracker.map_parts(&copied, &[(0, &source[second..second + len])]);
        let inner = tracker.checkpoint();
        let third = second * 2;
        tracker.map_parts(&copied, &[(0, &source[third..third + len])]);
        assert_eq!(tracker.text_range(&copied), Some(third..third + len));
        tracker.restore(inner);
        assert_eq!(tracker.text_range(&copied), Some(second..second + len));
        tracker.restore(outer);
        assert_eq!(tracker.text_range(&copied), Some(0..len));
        assert_eq!(tracker.text_range(&copied["同".len()..]), Some(3..len));
        tracker.restore(root);
        assert_eq!(tracker.text_range(&copied), None);
        assert!(tracker.text.is_empty());
        assert!(tracker.text_order.is_empty());
    }

    #[test]
    fn one_restore_unwinds_repeated_registration_in_reverse_order() {
        let source = "same same same";
        let copied = source[..4].to_string();
        let mut tracker = DestinationTracker::new(source);
        tracker.map_parts(&copied, &[(0, &source[..4])]);
        let checkpoint = tracker.checkpoint();
        tracker.map_parts(&copied, &[(0, &source[5..9])]);
        tracker.map_parts(&copied, &[(0, &source[10..14])]);
        tracker.restore(checkpoint);
        assert_eq!(tracker.text_range(&copied), Some(0..4));
        // Restoring the same checkpoint is idempotent.
        tracker.restore(checkpoint);
        assert_eq!(tracker.text_range(&copied), Some(0..4));
    }

    #[test]
    fn synthesized_replacements_hide_stale_origins_until_scope_exit() {
        let source = "asset.png";
        let copied = source.to_string();
        let mut tracker = DestinationTracker::new(source);
        tracker.map_parts(&copied, &[(0, source)]);
        let checkpoint = tracker.checkpoint();
        tracker.map_parts(&copied, &[]);
        assert_eq!(tracker.text_range(&copied), None);
        assert_eq!(tracker.text_range(&copied[6..]), None);
        tracker.restore(checkpoint);
        assert_eq!(tracker.text_range(&copied), Some(0..source.len()));

        tracker.map_suffix(&copied, "unmapped");
        assert_eq!(tracker.text_range(&copied), None);
        tracker.restore(checkpoint);
        assert_eq!(tracker.text_range(&copied), Some(0..source.len()));
    }

    fn tokens(source: &str) -> Vec<(&str, String)> {
        destination_spans(source)
            .into_iter()
            .map(|span| (&source[span.range], span.dest))
            .collect()
    }

    #[test]
    fn decoded_destinations_keep_exact_original_tokens_and_titles() {
        let source = "中 [a](<my file.svg?x=1&amp;y=2#part> \"Title\") ![b](a\\(b\\).png 'alt')";
        assert_eq!(
            tokens(source),
            [
                (
                    "<my file.svg?x=1&amp;y=2#part>",
                    "my file.svg?x=1&y=2#part".into()
                ),
                ("a\\(b\\).png", "a(b).png".into()),
            ]
        );
    }

    #[test]
    fn code_html_math_and_invalid_outer_links_do_not_create_tokens() {
        let source = "`[x](inline.png)` $[x](math.png)$\n\n```md\n[x](fence.png)\n```\n\n    [x](indent.png)\n\n<div>\n[x](html.png)\n</div>\n\n[a [b](inner.md)](outer.md)\n";
        assert_eq!(tokens(source), [("inner.md", "inner.md".into())]);
    }

    #[test]
    fn reference_definitions_use_the_definition_token_and_skip_literal_defs() {
        let source = "[use][id] ![image][img]\n\n[id]:\n  <guide/next.md#top>\n  \"Title\"\n\n> [img]: assets/a.png\n\n```\n[bad]: literal.png\n```\n";
        assert_eq!(
            tokens(source),
            [
                ("<guide/next.md#top>", "guide/next.md#top".into()),
                ("assets/a.png", "assets/a.png".into()),
            ]
        );
    }

    #[test]
    fn repeated_nested_container_text_keeps_distinct_source_offsets() {
        let source = "- [x] [same](pic.png)\n- [x] [same](pic.png)\n\n>\t[same](pic.png)\n\n| cell |\n| --- |\n| [same](pic.png) |\n\n[^note]: [same](pic.png)\n    ![same](other.png)\n\nTerm\n: [same](pic.png)\n";
        let spans = destination_spans(source);
        assert_eq!(spans.len(), 7);
        assert!(
            spans
                .windows(2)
                .all(|pair| pair[0].range.end <= pair[1].range.start)
        );
        assert_eq!(
            spans.iter().filter(|span| span.dest == "pic.png").count(),
            6
        );
        for span in spans {
            assert_eq!(&source[span.range], span.dest);
        }
    }

    #[test]
    fn image_descriptions_do_not_expose_hidden_image_or_link_destinations() {
        let source = "![alt [link](hidden.md) ![nested](hidden.png)](visible.png) [![yes](image.png)](link.md)";
        assert_eq!(
            tokens(source),
            [
                ("visible.png", "visible.png".into()),
                ("image.png", "image.png".into()),
                ("link.md", "link.md".into()),
            ]
        );
    }

    #[test]
    fn fragment_ranges_preserve_exact_path_query_and_encoded_separator_spelling()
    -> Result<(), &'static str> {
        let source = "中 [a](<path?x=1&amp;y=2&#35;%E7%8C%AB>) [b](path\\#old) [c](path&num;old) [d](f&#111;o.md#&#111;ld)";
        let spans = destination_spans(source);
        let mut suffixes = Vec::new();
        for span in &spans {
            let range = span.fragment_range(source).ok_or("fragment range")?;
            suffixes.push((
                &source[span.range.start..range.start],
                &source[range.clone()],
                &source[range.end..span.range.end],
            ));
        }
        assert_eq!(
            suffixes,
            [
                ("<path?x=1&amp;y=2&#35;", "%E7%8C%AB", ">"),
                ("path\\#", "old", ""),
                ("path&num;", "old", ""),
                ("f&#111;o.md#", "&#111;ld", ""),
            ]
        );
        Ok(())
    }

    #[test]
    fn fragment_ranges_use_reference_definition_tokens_and_keep_titles_outside()
    -> Result<(), &'static str> {
        let source = "[r]\r\n\r\n[r]:\r\n <guide.md?x=1&amp;y=2&num;part>\r\n \"Title\"\r\n";
        let spans = destination_spans(source);
        assert_eq!(spans.len(), 1);
        let range = spans[0].fragment_range(source).ok_or("fragment range")?;
        assert_eq!(&source[range.clone()], "part");
        assert_eq!(&source[range.end..], ">\r\n \"Title\"\r\n");
        Ok(())
    }

    #[test]
    fn fragment_ranges_reject_changed_or_invalid_tokens_and_do_not_decode_percent_hashes()
    -> Result<(), &'static str> {
        let source = "[a](<#old>) [b](path%23old) [c](#)";
        let spans = destination_spans(source);
        assert!(
            spans[0]
                .fragment_range("[a](<#new>) [b](path%23old) [c](#)")
                .is_none()
        );
        assert!(spans[1].fragment_range(source).is_none());
        assert!(
            spans[2]
                .fragment_range(source)
                .ok_or("empty fragment")?
                .is_empty()
        );
        for range in [Range { start: 2, end: 1 }, 0..usize::MAX, 1..2] {
            assert!(
                DestinationSpan {
                    range,
                    dest: "#old".into()
                }
                .fragment_range("猫")
                .is_none()
            );
        }
        Ok(())
    }
}
