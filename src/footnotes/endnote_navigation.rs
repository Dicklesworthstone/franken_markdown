//! Exact, transient navigation provenance for the existing endnote transform.
//!
//! Source and prepared trees are walked together: only a source FootnoteRef or
//! the label inserted by assembly creates a navigation identity. Visible text
//! and author-controlled link destinations are never interpreted as markers.

use super::{Notes, Numbering, assemble_notes, number_notes, paragraph_accepts_note_label};
use crate::{Block, Document, Inline};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

const MAX_NOTES: usize = 4096;
const MAX_INLINE_TARGETS: usize = 16_384;
const MAX_ALIAS_BYTES: usize = 4 * 1024 * 1024;
const MAX_ALIAS_LEN: usize = 4096;

/// A destination in a prepared endnote document. Numbers are one-based and
/// assigned by the same first-reference traversal as [`Document::with_endnotes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EndnoteTarget {
    /// The beginning of the complete numbered note body.
    Note(usize),
    /// The first citation that the host actually paints for this note.
    Reference(usize),
}

/// Lossless endnotes with exact navigation provenance for renderer hosts.
///
/// The document has the same blocks and text as [`Document::with_endnotes`].
/// Navigation is an immutable sidecar, so the shared AST gains no synthetic
/// identifiers or special link destinations. A note-free document is borrowed.
///
/// Inline identities refer to allocations owned or borrowed by this wrapper.
/// The wrapper does not expose mutation, cloning or ownership extraction; its
/// identities remain valid after moving the wrapper and for its whole lifetime.
/// Addresses are used only for lookup and never for numbering or output order.
pub struct PreparedEndnotes<'a> {
    document: Cow<'a, Document>,
    inline_targets: BTreeMap<usize, EndnoteTarget>,
    note_starts: BTreeMap<usize, usize>,
    aliases: BTreeMap<String, EndnoteTarget>,
    limited: bool,
}

impl<'a> PreparedEndnotes<'a> {
    /// Prepare complete rich note bodies and their navigation metadata.
    /// Undefined references remain literal text; duplicate definitions retain
    /// the existing first-definition-wins policy. Reference cycles are followed
    /// once, and unreferenced definitions remain in the final Notes section.
    #[must_use]
    pub fn new(source: &'a Document) -> Self {
        let source = crate::safe_html::lower(source);
        let mut notes = Notes::default();
        notes.collect(&source.blocks);
        if notes.definitions.is_empty() && !notes.has_reference {
            return Self {
                document: source,
                inline_targets: BTreeMap::new(),
                note_starts: BTreeMap::new(),
                aliases: BTreeMap::new(),
                limited: false,
            };
        }
        let numbering = number_notes(&source, &notes);
        let document = assemble_notes(&source, &notes, &numbering, false);
        let mut metadata = Metadata::default();
        let body_len = rendered_block_count(&source.blocks);
        metadata.blocks(
            &source.blocks,
            &document.blocks[..body_len],
            &notes,
            &numbering,
            false,
        );

        let mut starts = BTreeMap::new();
        let mut labels = Vec::new();
        let mut cursor = body_len + usize::from(!notes.definitions.is_empty());
        for &index in &numbering.order {
            let number = numbering.numbers[index];
            let original = notes.definitions[index];
            let count = rendered_block_count(original);
            let joined_label = original
                .iter()
                .find(|block| !matches!(block, Block::FootnoteDefinition { .. }))
                .is_some_and(|block| {
                    matches!(block, Block::Paragraph(items) if paragraph_accepts_note_label(items))
                });
            let body_start = cursor + usize::from(!joined_label);
            if number <= MAX_NOTES {
                starts.insert(cursor, number);
                if let Some(Block::Paragraph(items)) = document.blocks.get(cursor)
                    && let Some(label) = items.first()
                {
                    labels.push((identity(label), number));
                }
            } else {
                metadata.limited = true;
            }
            metadata.blocks(
                original,
                &document.blocks[body_start..body_start + count],
                &notes,
                &numbering,
                joined_label,
            );
            cursor = body_start + count;
        }
        for (label, number) in labels {
            if metadata.cited.contains(&number) {
                metadata.insert(label, EndnoteTarget::Reference(number));
            }
        }

        let mut aliases = BTreeMap::new();
        let mut alias_bytes = 0usize;
        for (&id, &index) in &notes.indices {
            let number = numbering.numbers[index];
            if number > MAX_NOTES {
                continue;
            }
            let note_alias = if id.len() <= MAX_ALIAS_LEN - 3 {
                Some(format!("fn-{id}"))
            } else {
                metadata.limited = true;
                None
            };
            for (name, target) in [
                (note_alias, EndnoteTarget::Note(number)),
                (
                    Some(format!("fnref-{number}")),
                    EndnoteTarget::Reference(number),
                ),
            ] {
                let Some(name) = name else {
                    continue;
                };
                if matches!(target, EndnoteTarget::Reference(_))
                    && !metadata.cited.contains(&number)
                {
                    continue;
                }
                if name.len() > MAX_ALIAS_LEN || name.len() > MAX_ALIAS_BYTES - alias_bytes {
                    metadata.limited = true;
                    continue;
                }
                alias_bytes += name.len();
                aliases.insert(name, target);
            }
        }
        Self {
            document: Cow::Owned(document),
            inline_targets: metadata.targets,
            note_starts: starts,
            aliases,
            limited: metadata.limited,
        }
    }

    /// The unchanged endnote document. Keep this wrapper alive while consuming
    /// inline references obtained from [`Self::for_each_inline_target`].
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The numbered note beginning at a root block index, if any. Its first
    /// block contains the existing superscript label, even for leading figures,
    /// code, tables, equations, nested lists or empty definitions.
    #[must_use]
    pub fn note_at_block(&self, index: usize) -> Option<usize> {
        self.note_starts.get(&index).copied()
    }

    /// Visit exact generated citation and return-label nodes in document order.
    /// A host can associate these immutable inline references with its own
    /// render-local identities. Repeated measurement must not record positions;
    /// the first painted citation determines a Reference destination.
    pub fn for_each_inline_target(&self, mut visit: impl FnMut(&Inline, EndnoteTarget)) {
        if self.inline_targets.is_empty() {
            return;
        }
        visit_inlines(&self.document.blocks, &mut |inline| {
            if let Some(&target) = self.inline_targets.get(&identity(inline)) {
                visit(inline, target);
            }
        });
    }

    /// Literal source fragment names (`fn-ID`, `fnref-N`) and typed targets.
    /// Names are not percent-decoded: hosts decode incoming fragments once and
    /// must refuse aliases colliding with another emitted source destination.
    pub fn aliases(&self) -> impl Iterator<Item = (&str, EndnoteTarget)> {
        self.aliases
            .iter()
            .map(|(name, &target)| (name.as_str(), target))
    }

    /// Navigation admission reached a bound. All document content is retained;
    /// hosts should report that affected navigation may remain inactive.
    #[must_use]
    pub fn navigation_limited(&self) -> bool {
        self.limited
    }
}

fn identity(inline: &Inline) -> usize {
    std::ptr::from_ref(inline) as usize
}

fn rendered_block_count(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .filter(|block| !matches!(block, Block::FootnoteDefinition { .. }))
        .count()
}

#[derive(Default)]
struct Metadata {
    targets: BTreeMap<usize, EndnoteTarget>,
    cited: BTreeSet<usize>,
    limited: bool,
}

impl Metadata {
    fn insert(&mut self, inline: usize, target: EndnoteTarget) {
        if self.targets.len() >= MAX_INLINE_TARGETS {
            self.limited = true;
        } else {
            self.targets.insert(inline, target);
        }
    }

    fn blocks(
        &mut self,
        source: &[Block],
        prepared: &[Block],
        notes: &Notes<'_>,
        numbering: &Numbering,
        skip_first_label: bool,
    ) {
        let source = source
            .iter()
            .filter(|block| !matches!(block, Block::FootnoteDefinition { .. }));
        for (index, (source, prepared)) in source.zip(prepared).enumerate() {
            match (source, prepared) {
                (Block::Paragraph(source), Block::Paragraph(prepared))
                | (
                    Block::Heading {
                        inlines: source, ..
                    },
                    Block::Heading {
                        inlines: prepared, ..
                    },
                ) => {
                    let start = usize::from(skip_first_label && index == 0);
                    self.inlines(source, &prepared[start..], notes, numbering);
                }
                (Block::BlockQuote(source), Block::BlockQuote(prepared)) => {
                    self.blocks(source, prepared, notes, numbering, false);
                }
                (Block::List(source), Block::List(prepared)) => {
                    for (source, prepared) in source.items.iter().zip(&prepared.items) {
                        self.blocks(
                            &source.blocks,
                            &prepared.blocks,
                            notes,
                            numbering,
                            false,
                        );
                    }
                }
                (Block::Table(source), Block::Table(prepared)) => {
                    for (source, prepared) in source
                        .head
                        .iter()
                        .chain(source.rows.iter().flatten())
                        .zip(prepared.head.iter().chain(prepared.rows.iter().flatten()))
                    {
                        self.inlines(source, prepared, notes, numbering);
                    }
                }
                (Block::DefinitionList(source), Block::DefinitionList(prepared)) => {
                    for (source, prepared) in source.iter().zip(prepared) {
                        for (source, prepared) in source
                            .terms
                            .iter()
                            .chain(&source.definitions)
                            .zip(prepared.terms.iter().chain(&prepared.definitions))
                        {
                            self.inlines(source, prepared, notes, numbering);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn inlines(
        &mut self,
        source: &[Inline],
        prepared: &[Inline],
        notes: &Notes<'_>,
        numbering: &Numbering,
    ) {
        for (source, prepared) in source.iter().zip(prepared) {
            match (source, prepared) {
                (Inline::FootnoteRef { id }, _) => {
                    if let Some(&index) = notes.indices.get(id.as_str()) {
                        let number = numbering.numbers[index];
                        if number <= MAX_NOTES {
                            self.cited.insert(number);
                            self.insert(identity(prepared), EndnoteTarget::Note(number));
                        } else {
                            self.limited = true;
                        }
                    }
                }
                (Inline::Emphasis(source), Inline::Emphasis(prepared))
                | (Inline::Strong(source), Inline::Strong(prepared))
                | (Inline::Strikethrough(source), Inline::Strikethrough(prepared))
                | (
                    Inline::Link {
                        content: source, ..
                    },
                    Inline::Link {
                        content: prepared, ..
                    },
                ) => {
                    self.inlines(source, prepared, notes, numbering);
                }
                _ => {}
            }
        }
    }
}

fn visit_inlines(blocks: &[Block], visit: &mut impl FnMut(&Inline)) {
    fn inlines(items: &[Inline], visit: &mut impl FnMut(&Inline)) {
        for inline in items {
            visit(inline);
            match inline {
                Inline::Emphasis(inner)
                | Inline::Strong(inner)
                | Inline::Strikethrough(inner)
                | Inline::Link { content: inner, .. } => inlines(inner, visit),
                _ => {}
            }
        }
    }
    for block in blocks {
        match block {
            Block::Paragraph(items)
            | Block::Heading {
                inlines: items, ..
            } => inlines(items, visit),
            Block::BlockQuote(inner)
            | Block::FootnoteDefinition { blocks: inner, .. } => visit_inlines(inner, visit),
            Block::List(list) => {
                for item in &list.items {
                    visit_inlines(&item.blocks, visit);
                }
            }
            Block::Table(table) => {
                for cell in table.head.iter().chain(table.rows.iter().flatten()) {
                    inlines(cell, visit);
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for content in item.terms.iter().chain(&item.definitions) {
                        inlines(content, visit);
                    }
                }
            }
            _ => {}
        }
    }
}
