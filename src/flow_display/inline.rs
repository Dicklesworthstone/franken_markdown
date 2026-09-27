//! Inline formatting retained from the shared AST, in reading-text coordinates.
//! No Markdown reparsing and no source-position guessing occurs here.

use super::{
    AssetRequestId, BlockMeta, DisplayBlock, FlowDisplayError, PreparedBlock,
    ResumableFlowDisplay, UnresolvedAsset, emit_text,
};
use super::limits::Projection;
use crate::ast::Inline;
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

/// Inline font/decorations, composable with the enclosing block's font role.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlowInlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strikethrough: bool,
}

/// One styled reading-text range. Ranges are ordered, non-overlapping UTF-8
/// boundaries into the corresponding block text (or table cell), NOT Markdown
/// source offsets. Adjacent equal styles/targets are coalesced. Link targets are
/// retained even when activation is blocked, so hosts can expose diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowInlineRun {
    pub range: Range<usize>,
    pub style: FlowInlineStyle,
    pub link: Option<Arc<str>>,
}

impl FlowInlineRun {
    /// Conservative activation policy; authorization/navigation belongs to the
    /// host. Reading text never acquires an executable or arbitrary URI scheme.
    #[must_use]
    pub fn active_link_target(&self) -> Option<&str> {
        active_link_target(self.link.as_deref()?)
    }
}

/// Allow fragment/relative references and HTTP(S)/mailto/tel only. Controls are
/// rejected before trimming, including controls at either edge of a target.
///
/// The AST already decodes Markdown escapes/entities, but callers may also
/// construct targets directly or pass them to hosts that decode again. Inspect
/// a bounded, decoded probe to prevent such decoding from enabling a blocked
/// scheme. Never return the probe: accepted destinations retain their original
/// encoding, and rejected destinations remain available in inline metadata.
/// This function never fetches or opens anything.
#[must_use]
pub fn active_link_target(target: &str) -> Option<&str> {
    if target.chars().any(char::is_control) {
        return None;
    }
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    let mut probe = Cow::Borrowed(target);
    // Fail closed on excessive nesting instead of permitting an unexamined
    // destination or spending unbounded time unwrapping attacker-controlled text.
    for _ in 0..8 {
        let Cow::Owned(decoded) = decode_activation_probe(&probe) else {
            return allowed_activation_probe(&probe).then_some(target);
        };
        probe = Cow::Owned(decoded);
    }
    None
}

fn allowed_activation_probe(target: &str) -> bool {
    if target.chars().any(char::is_control) || target.contains('\\') {
        return false;
    }
    let target = target.trim();
    if target.is_empty() {
        return false;
    }
    let prefix_end = target.find(['/', '?', '#']).unwrap_or(target.len());
    if let Some(colon) = target[..prefix_end].find(':') {
        let scheme = &target[..colon];
        return ["https", "http", "mailto", "tel"]
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed));
    }
    true
}

/// Decode only ASCII percent escapes and relevant HTML references for policy
/// inspection. Non-ASCII percent-encoded UTF-8 is preserved, not interpreted as
/// individual Latin-1 characters. Ordinary links take the allocation-free path.
fn decode_activation_probe(target: &str) -> Cow<'_, str> {
    if !target.contains(['%', '&']) {
        return Cow::Borrowed(target);
    }
    let mut output = String::new();
    let mut copied = 0;
    let mut offset = 0;
    while offset < target.len() {
        let tail = &target[offset..];
        let decoded = if tail.starts_with('%') {
            tail.as_bytes().get(1..3).and_then(|digits| {
                let high = char::from(digits[0]).to_digit(16)?;
                let low = char::from(digits[1]).to_digit(16)?;
                let value = high * 16 + low;
                (value <= 0x7f).then(|| (char::from_u32(value).unwrap_or('\u{fffd}'), 3))
            })
        } else if tail.starts_with('&') {
            decode_activation_reference(tail)
        } else {
            None
        };
        if let Some((ch, consumed)) = decoded {
            output.push_str(&target[copied..offset]);
            output.push(ch);
            offset += consumed;
            copied = offset;
        } else if let Some(ch) = tail.chars().next() {
            offset += ch.len_utf8();
        } else {
            break;
        }
    }
    if copied == 0 {
        Cow::Borrowed(target)
    } else {
        output.push_str(&target[copied..]);
        Cow::Owned(output)
    }
}

fn decode_activation_reference(tail: &str) -> Option<(char, usize)> {
    for (reference, ch) in [
        ("&colon;", ':'),
        ("&Tab;", '\t'),
        ("&NewLine;", '\n'),
        ("&amp;", '&'),
        ("&AMP;", '&'),
    ] {
        if tail.starts_with(reference) {
            return Some((ch, reference.len()));
        }
    }
    let digits = tail.strip_prefix("&#")?;
    let (digits, radix, prefix) = match digits.strip_prefix('x').or_else(|| digits.strip_prefix('X')) {
        Some(digits) => (digits, 16, 3),
        None => (digits, 10, 2),
    };
    let count = digits.bytes().take_while(|byte| match radix {
        16 => byte.is_ascii_hexdigit(),
        _ => byte.is_ascii_digit(),
    }).count();
    if count == 0 {
        return None;
    }
    let value = u32::from_str_radix(&digits[..count], radix).ok()?;
    let ch = char::from_u32(value)?;
    let consumed = prefix + count + usize::from(digits.as_bytes().get(count) == Some(&b';'));
    Some((ch, consumed))
}

impl ResumableFlowDisplay {
    /// Borrow formatting for an emitted text block. Code blocks have an empty
    /// slice: their block role supplies monospace styling. None means no block.
    #[must_use]
    pub fn inline_runs_for_block(&self, index: usize) -> Option<&[FlowInlineRun]> {
        self.metadata.get(index).map(|meta| meta.inline_runs.as_slice())
    }

    /// Borrow formatting in one emitted table cell's reading-text coordinates.
    #[must_use]
    pub fn inline_runs_for_cell(&self, block: usize, column: usize) -> Option<&[FlowInlineRun]> {
        self.metadata.get(block)?.cell_runs.get(column).map(Vec::as_slice)
    }

    /// Retained link destination enclosing an image, including blocked targets.
    #[must_use]
    pub fn image_link_for_block(&self, index: usize) -> Option<&str> {
        self.metadata.get(index)?.image_link.as_deref()
    }
}

struct Context<'a> {
    inline: &'a Inline,
    style: FlowInlineStyle,
    link: Option<Arc<str>>,
}

enum Leaf<'a> {
    Text(Cow<'a, str>),
    Image { alt: &'a str, dest: &'a str },
}

struct Walker<'a> {
    stack: Vec<Context<'a>>,
}

impl<'a> Walker<'a> {
    fn new(inlines: &'a [Inline]) -> Self {
        Self { stack: inlines.iter().rev().map(|inline| Context {
            inline, style: FlowInlineStyle::default(), link: None,
        }).collect() }
    }
}

impl<'a> Iterator for Walker<'a> {
    type Item = (Leaf<'a>, FlowInlineStyle, Option<Arc<str>>);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(Context { inline, mut style, mut link }) = self.stack.pop() {
            let children = match inline {
                Inline::Emphasis(children) => { style.italic = true; Some(children) }
                Inline::Strong(children) => { style.bold = true; Some(children) }
                Inline::Strikethrough(children) => { style.strikethrough = true; Some(children) }
                Inline::Link { content, dest, .. } => {
                    link = Some(Arc::from(dest.as_str()));
                    Some(content)
                }
                _ => None,
            };
            if let Some(children) = children {
                self.stack.extend(children.iter().rev().map(|inline| Context {
                    inline, style, link: link.clone(),
                }));
                continue;
            }
            let leaf = match inline {
                Inline::Text(text) | Inline::Math(text) | Inline::DisplayMath(text)
                | Inline::Html(text) => Leaf::Text(Cow::Borrowed(text)),
                Inline::Code(text) => { style.code = true; Leaf::Text(Cow::Borrowed(text)) }
                Inline::SoftBreak => Leaf::Text(Cow::Borrowed(" ")),
                Inline::HardBreak => Leaf::Text(Cow::Borrowed("\n")),
                Inline::FootnoteRef { id } => Leaf::Text(Cow::Owned(format!("[^{id}]"))),
                Inline::Image { alt, dest, .. } => Leaf::Image { alt, dest },
                // Container variants were consumed above.
                Inline::Emphasis(_) | Inline::Strong(_) | Inline::Strikethrough(_)
                | Inline::Link { .. } => continue,
            };
            return Some((leaf, style, link));
        }
        None
    }
}

#[derive(Default)]
struct Builder {
    text: String,
    runs: Vec<FlowInlineRun>,
    bytes: usize,
}

impl Builder {
    fn append(&mut self, text: &str, style: FlowInlineStyle, link: Option<Arc<str>>, limit: usize)
        -> Result<(), FlowDisplayError>
    {
        if text.is_empty() { return Ok(()); }
        let merge = self.runs.last().is_some_and(|run| run.style == style && run.link == link);
        // Conservative: count a target per retained run even when Arc shares
        // its allocation. This also bounds repeated target comparisons/hashing.
        let link_bytes = if merge { 0 } else { link.as_ref().map_or(0, |s| s.len()) };
        let next = self.bytes.checked_add(text.len()).and_then(|n| n.checked_add(link_bytes))
            .filter(|n| *n <= limit)
            .ok_or_else(|| FlowDisplayError::BudgetExceeded("inline output bytes".to_owned()))?;
        let start = self.text.len();
        self.text.push_str(text);
        if merge {
            if let Some(last) = self.runs.last_mut() { last.range.end = self.text.len(); }
        } else {
            self.runs.push(FlowInlineRun { range: start..self.text.len(), style, link });
        }
        self.bytes = next;
        Ok(())
    }

    fn emit(&mut self, meta: &mut BlockMeta, output: &mut Projection<'_>) -> Result<(), FlowDisplayError> {
        meta.inline_runs = std::mem::take(&mut self.runs);
        self.bytes = 0;
        emit_text(std::mem::take(&mut self.text), meta, output)
    }
}

/// Headings/table cells retain their existing reading-text representation of
/// images (alt text); paragraphs still emit image requests as separate blocks.
pub(super) fn collect(inlines: &[Inline], limit: usize) -> Result<(String, Vec<FlowInlineRun>), FlowDisplayError> {
    let mut builder = Builder::default();
    for (leaf, style, link) in Walker::new(inlines) {
        match leaf {
            Leaf::Text(text) => builder.append(&text, style, link, limit)?,
            Leaf::Image { alt, .. } => builder.append(alt, style, link, limit)?,
        }
    }
    Ok((builder.text, builder.runs))
}

pub(super) fn emit_inlines(inlines: &[Inline], meta: &mut BlockMeta, output: &mut Projection<'_>, next_id: &mut u64)
    -> Result<(), FlowDisplayError>
{
    let mut builder = Builder::default();
    for (leaf, style, link) in Walker::new(inlines) {
        match leaf {
            Leaf::Text(text) => builder.append(&text, style, link, output.remaining_bytes())?,
            Leaf::Image { alt, dest } => {
                builder.emit(meta, output)?;
                let asset = UnresolvedAsset {
                    id: AssetRequestId(*next_id), kind: "image", reference: dest.to_owned(),
                    source_offset: meta.span.start, generation: 0,
                    estimated_width: 320, estimated_height: 240, alt_text: alt.to_owned(),
                };
                *next_id += 1;
                let mut image_meta = meta.clone();
                image_meta.image_link = link;
                output.push_back(PreparedBlock { block: DisplayBlock::UnresolvedAsset(asset), meta: image_meta })?;
            }
        }
    }
    builder.emit(meta, output)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::flow_display::FlowDisplayLimits;

    #[test]
    fn nesting_links_unicode_and_code_retain_exact_reading_ranges() {
        let source = "A **bold *é* end** [link `code`][r] ~~gone~~.\n\n[r]: https://example.com\n";
        let mut engine = ResumableFlowDisplay::new(source, 1);
        engine.process_all().unwrap();
        let DisplayBlock::Paragraph { text } = &engine.blocks()[0] else { panic!("paragraph"); };
        assert_eq!(text, "A bold é end link code gone.");
        let runs = engine.inline_runs_for_block(0).unwrap();
        let mut end = 0;
        for run in runs {
            assert_eq!(run.range.start, end);
            assert!(text.get(run.range.clone()).is_some());
            end = run.range.end;
        }
        assert_eq!(end, text.len());
        assert!(runs.iter().any(|r| &text[r.range.clone()] == "é" && r.style.bold && r.style.italic));
        assert!(runs.iter().any(|r| &text[r.range.clone()] == "code" && r.style.code
            && r.active_link_target() == Some("https://example.com")));
        assert!(runs.iter().any(|r| &text[r.range.clone()] == "gone" && r.style.strikethrough));
    }

    #[test]
    fn style_is_retained_in_headings_tables_and_across_image_splits() {
        let source = "# **Title**\n\n| *Head* |\n| --- |\n| `cell` |\n\n**before ![alt](x.png) after**\n";
        let mut engine = ResumableFlowDisplay::new(source, 1);
        engine.process_all().unwrap();
        assert!(engine.inline_runs_for_block(0).unwrap()[0].style.bold);
        assert!(engine.inline_runs_for_cell(1, 0).unwrap()[0].style.italic);
        assert!(engine.inline_runs_for_cell(2, 0).unwrap()[0].style.code);
        for (index, block) in engine.blocks().iter().enumerate().skip(3) {
            if let DisplayBlock::Paragraph { text } = block {
                let runs = engine.inline_runs_for_block(index).unwrap();
                assert_eq!(runs[0].range, 0..text.len());
                assert!(runs[0].style.bold);
            }
        }
        assert_eq!(engine.unresolved_assets().len(), 1);
    }

    #[test]
    fn image_links_and_blocked_destinations_are_retained_without_activation() {
        let mut engine = ResumableFlowDisplay::new("[![a](a.png)](https://example.com) [bad](javascript:alert)", 8);
        engine.process_all().unwrap();
        assert_eq!(engine.image_link_for_block(0), Some("https://example.com"));
        let runs = engine.inline_runs_for_block(1).unwrap();
        let bad = runs.iter().find(|run| run.link.is_some()).unwrap();
        assert_eq!(bad.link.as_deref(), Some("javascript:alert"));
        assert_eq!(bad.active_link_target(), None);
        for target in ["javascript:x", "data:text/html,x", "file:///etc/passwd", "https:\\evil", "java\tscript:x", "", "vbscript:x"] {
            assert!(active_link_target(target).is_none(), "{target:?}");
        }
        for target in ["#section", "../guide.md#part", "https://example.com", "HTTP://example.com", "mailto:a@example.com", "/path/a:b"] {
            assert_eq!(active_link_target(target), Some(target));
        }
    }

    #[test]
    fn activation_rejects_edge_controls_and_encoded_scheme_bypasses() {
        for target in [
            "\thttps://example.com", "https://example.com\n", "\u{0085}#section",
            "%6a%61vascript%3Aalert(1)", "java%09script:alert(1)",
            "javascript&colon;alert(1)", "javascript&#58;alert(1)",
            "javascript&#x3a;alert(1)", "javascript&#58alert(1)",
            "&#106;avascript:alert(1)", "java&Tab;script:alert(1)",
            "java&NewLine;script:alert(1)", "%256aavascript%253aalert(1)",
            "javascript&amp;colon;alert(1)", "data%3atext/html,payload",
            "https:%5c%5cevil.example", "https://example.com/%0d%0aheader",
        ] {
            assert_eq!(active_link_target(target), None, "{target:?}");
        }
    }

    #[test]
    fn activation_preserves_safe_encoding_and_relative_url_semantics() {
        for target in [
            "https://example.com/a%20b?q=a%26b#part", "mailto:a@example.com",
            "tel:+1-555-0100", "TEL:+1-555-0100", "../résumé%20final.md",
            "/path/a:b", "./a:b", "?next=javascript:literal", "#javascript:literal",
            "https://example.com/%F0%9F%98%80", "//example.com/path",
            "https://example.com/?x=1&amp;y=2", "#日本語",
        ] {
            assert_eq!(active_link_target(target), Some(target), "{target:?}");
        }
        assert_eq!(active_link_target("  https://example.com  "), Some("https://example.com"));
        assert!(matches!(decode_activation_probe("https://example.com"), Cow::Borrowed(_)));
        assert!(matches!(decode_activation_probe("résumé%F0%9F%98%80"), Cow::Borrowed(_)));
    }

    #[test]
    fn activation_decoding_is_bounded_and_never_rewrites_metadata() {
        let mut target = "javascript:alert(1)".to_owned();
        for _ in 0..16 {
            target = target.replace('%', "%25").replace(':', "%3a");
        }
        assert_eq!(active_link_target(&target), None);
        let run = FlowInlineRun {
            range: 0..5,
            style: FlowInlineStyle::default(),
            link: Some(Arc::from(target.as_str())),
        };
        assert_eq!(run.active_link_target(), None);
        assert_eq!(run.link.as_deref(), Some(target.as_str()));
    }

    #[test]
    fn link_retention_is_budgeted_and_failure_is_atomic() {
        let limits = FlowDisplayLimits { max_output_bytes: 8, ..FlowDisplayLimits::default() };
        let mut engine = ResumableFlowDisplay::try_with_limits("[x](https://example.com)", 1, 1, limits).unwrap();
        assert!(matches!(engine.step(), Err(FlowDisplayError::BudgetExceeded(_))));
        assert!(engine.blocks().is_empty());
        assert!(engine.metadata.is_empty());
    }

    #[test]
    fn emission_batch_sizes_do_not_change_inline_metadata() {
        let source = "# *Title*\n\n**one ![a](a.png) two** [x](#title)\n\n| Head |\n| --- |\n| ~~cell~~ |";
        let mut whole = ResumableFlowDisplay::new(source, usize::MAX);
        whole.process_all().unwrap();
        for batch in [1, 2, 3, 8] {
            let mut stepped = ResumableFlowDisplay::new(source, batch);
            stepped.process_all().unwrap();
            assert_eq!(stepped.blocks(), whole.blocks());
            for index in 0..whole.blocks().len() {
                assert_eq!(stepped.inline_runs_for_block(index), whole.inline_runs_for_block(index));
                assert_eq!(stepped.inline_runs_for_cell(index, 0), whole.inline_runs_for_cell(index, 0));
                assert_eq!(stepped.image_link_for_block(index), whole.image_link_for_block(index));
            }
        }
    }
}
