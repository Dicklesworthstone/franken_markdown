//! Shared marker recognition and document-scoped TOC placement decisions.

use crate::ast::{Block, Inline};

/// A marker is a whole paragraph, never an inline substring or code span.
/// The optional depth is the greatest source heading level to include.
pub(crate) fn marker_depth(inlines: &[Inline]) -> Option<Option<u8>> {
    let [Inline::Text(text)] = inlines else {
        return None;
    };
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix("[[")
        .and_then(|s| s.strip_suffix("]]"))
        .or_else(|| trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')))?
        .trim_start_matches('_')
        .trim_end_matches('_')
        .to_ascii_lowercase();
    let inner = inner.strip_prefix("toc")?;
    if inner.is_empty() {
        return Some(None);
    }
    let depth = inner.strip_prefix(':')?.trim().parse::<u8>().ok()?;
    (depth >= 1).then_some(Some(depth))
}

#[derive(Default)]
pub(crate) struct Markers {
    /// An explicit marker in the document body owns automatic TOC placement,
    /// including inside a quote, alert, or list item.
    pub body: bool,
    /// Notes are rendered separately and may be unreferenced or omitted. Their
    /// markers need the body outline, but cannot suppress its automatic TOC.
    pub notes: bool,
}

impl Markers {
    pub fn any(&self) -> bool {
        self.body || self.notes
    }
}

pub(crate) fn markers(blocks: &[Block]) -> Markers {
    let mut found = Markers::default();
    let mut pending = vec![(blocks, false)];
    while let Some((blocks, in_note)) = pending.pop() {
        for block in blocks {
            match block {
                Block::Paragraph(inlines) if marker_depth(inlines).is_some() => {
                    if in_note {
                        found.notes = true;
                    } else {
                        found.body = true;
                    }
                }
                Block::BlockQuote(inner) => pending.push((inner, in_note)),
                Block::List(list) => pending.extend(
                    list.items
                        .iter()
                        .map(|item| (item.blocks.as_slice(), in_note)),
                ),
                Block::FootnoteDefinition { blocks: inner, .. } => {
                    pending.push((inner, true));
                }
                _ => {}
            }
        }
    }
    found
}
