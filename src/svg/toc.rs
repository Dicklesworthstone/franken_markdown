//! Body outlines over immutable prepared headings, with native SVG hit regions.

use super::{Block, Ink, Inline, Piece, Poster, RStyle, SvgOptions, links::Navigation};
use franken_markdown::PreparedEndnotes;

const MAX_HEADINGS: usize = 4096;
const MAX_TITLE_BYTES: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_EXPANDED_ROWS: usize = 16_384;
const MAX_EXPANDED_TEXT: usize = 4 * 1024 * 1024;

/// Identity is used only for transient lookup. Its address never enters output
/// bytes, ordering or a source-visible fragment namespace. Even empty headings
/// have distinct identities because this names the block, not its inline slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct HeadingTarget(usize);

impl HeadingTarget {
    pub(super) fn new(block: &Block) -> Self {
        Self(std::ptr::from_ref(block) as usize)
    }
}

struct Entry {
    target: HeadingTarget,
    level: u8,
    title: String,
}

pub(super) struct Contents {
    entries: Vec<Entry>,
    automatic: bool,
    depth: Option<u8>,
    active: bool,
    limited: bool,
    warned: bool,
    expansion_limited: bool,
    remaining_rows: usize,
    remaining_text: usize,
}

impl Contents {
    pub(super) fn new(options: &SvgOptions) -> Self {
        Self {
            entries: Vec::new(),
            automatic: options.toc,
            depth: options.toc_depth,
            active: false,
            limited: false,
            warned: false,
            expansion_limited: false,
            remaining_rows: MAX_EXPANDED_ROWS,
            remaining_text: MAX_EXPANDED_TEXT,
        }
    }

    pub(super) fn prepare(&mut self, prepared: &PreparedEndnotes<'_>, navigation: &Navigation) {
        let body_marker = crate::toc::markers(prepared.body_blocks()).body;
        let any_marker = crate::toc::markers(&prepared.document().blocks).any();
        self.active = self.automatic || any_marker;
        self.automatic &= !body_marker;
        if !self.active {
            return;
        }
        let mut bytes = 0;
        let mut pending = vec![prepared.body_blocks().iter()];
        while let Some(iter) = pending.last_mut() {
            let Some(block) = iter.next() else {
                pending.pop();
                continue;
            };
            match block {
                Block::Heading { level, inlines } => {
                    let title = title(inlines, navigation);
                    let Some(title) = title.filter(|title| {
                        self.entries.len() < MAX_HEADINGS && title.len() <= MAX_TEXT_BYTES - bytes
                    }) else {
                        // Never publish a plausible-looking but incomplete outline.
                        self.entries.clear();
                        self.active = false;
                        self.automatic = false;
                        self.limited = true;
                        return;
                    };
                    bytes += title.len();
                    self.entries.push(Entry {
                        target: HeadingTarget::new(block),
                        level: (*level).clamp(1, 6),
                        title,
                    });
                }
                Block::BlockQuote(inner) => pending.push(inner.iter()),
                Block::List(list) => {
                    // Push in reverse so the ordinary depth-first paint order
                    // is retained without flattening or cloning source blocks.
                    for item in list.items.iter().rev() {
                        pending.push(item.blocks.iter());
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn active(&self) -> bool {
        self.active
    }

    #[cfg(test)]
    pub(super) fn limited(&self) -> bool {
        self.limited
    }

    fn take_warning(&mut self) -> bool {
        let warn = self.limited && !self.warned;
        self.warned |= warn;
        warn
    }

    /// Admit a complete expansion before painting its first row. Repeated
    /// markers cannot multiply a bounded source inventory into unbounded output.
    /// Keep heading identities active after a refusal: earlier TOCs can still
    /// have forward links to headings that have not been painted yet.
    fn reserve(&mut self, depth: Option<u8>) -> bool {
        if self.expansion_limited {
            return false;
        }
        let depth = depth.or(self.depth);
        let mut rows = 0;
        let mut bytes = 0;
        for entry in self
            .entries
            .iter()
            .filter(|entry| depth.is_none_or(|max| entry.level <= max))
        {
            if rows == self.remaining_rows || entry.title.len() > self.remaining_text - bytes {
                self.expansion_limited = true;
                self.limited = true;
                return false;
            }
            rows += 1;
            bytes += entry.title.len();
        }
        self.remaining_rows -= rows;
        self.remaining_text -= bytes;
        true
    }

    pub(super) fn take_automatic(&mut self, target: HeadingTarget) -> bool {
        if self.automatic && self.entries.first().is_some_and(|entry| entry.target == target) {
            self.automatic = false;
            true
        } else {
            false
        }
    }
}

/// Plain, complete row text: formatting and source links keep their content,
/// images supply alt text, and notes do not duplicate their generated citation
/// links inside the outline. Bounds apply before retaining a title or emitting
/// any rows, so a failed inventory cannot leave half a table of contents.
fn title(inlines: &[Inline], navigation: &Navigation) -> Option<String> {
    let mut out = String::new();
    let mut pending = vec![inlines.iter()];
    let mut count = 0usize;
    while let Some(iter) = pending.last_mut() {
        let Some(inline) = iter.next() else {
            pending.pop();
            continue;
        };
        count += 1;
        if count > 8192 {
            return None;
        }
        if navigation.inline_note(inline).is_some() {
            continue;
        }
        let text = match inline {
            Inline::Text(text)
            | Inline::Code(text)
            | Inline::Math(text)
            | Inline::DisplayMath(text)
            | Inline::Html(text) => text.as_str(),
            Inline::Image { alt, .. } => alt,
            Inline::SoftBreak | Inline::HardBreak => " ",
            Inline::FootnoteRef { .. } => continue,
            Inline::Emphasis(inner)
            | Inline::Strong(inner)
            | Inline::Strikethrough(inner)
            | Inline::Link { content: inner, .. } => {
                if pending.len() >= 128 {
                    return None;
                }
                pending.push(inner.iter());
                continue;
            }
        };
        if text.len() > MAX_TITLE_BYTES - out.len() {
            return None;
        }
        out.push_str(text);
    }
    let trimmed = out.trim();
    Some(if trimmed.is_empty() {
        "Section".to_owned()
    } else {
        trimmed.to_owned()
    })
}

impl Poster {
    pub(super) fn warn_toc_limit(&mut self) {
        if self.contents.take_warning() {
            self.warnings.push(super::SvgWarning {
                code: "svg_toc_limit",
                message: "Table of contents inventory or repeated expansion budget exceeded; affected explicit markers remain visible and source headings are retained.".to_owned(),
            });
        }
    }

    pub(super) fn table_of_contents(&mut self, depth: Option<u8>, l: f64, r: f64, quote: bool) -> bool {
        if !self.contents.reserve(depth) {
            self.warn_toc_limit();
            return false;
        }
        let depth = depth.or(self.contents.depth);
        let size = f64::from(self.scale.body);
        let mut drawn = false;
        for index in 0..self.contents.entries.len() {
            let entry = &self.contents.entries[index];
            if depth.is_some_and(|max| entry.level > max) {
                continue;
            }
            let indent = self.inset(l, r, f64::from(entry.level - 1) * 14.0 * self.unit_scale());
            let link = self
                .navigation
                .borrow_mut()
                .intern_heading(entry.target, &entry.title);
            let piece = Piece::Text(
                entry.title.clone(),
                RStyle {
                    ink: Ink::Accent,
                    link: Some(link),
                    ..RStyle::BODY
                },
            );
            if !drawn {
                self.y += size * 0.35;
                drawn = true;
            }
            self.text_lines(&[piece], size, l + indent, r, quote, size * 0.15);
        }
        if drawn {
            self.y += size * 0.55;
        }
        true
    }
}

#[cfg(test)]
#[path = "toc_tests.rs"]
mod tests;
