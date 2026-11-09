//! Immutable source provenance for PDF note structure.
//!
//! The public AST remains ordinary text and links. Only the preparation pass
//! knows which nodes came from actual references and inserted labels. Store
//! their preorder ordinals, so borrowing or owning a prepared document cannot
//! leave stale addresses behind. Addresses are bound only for a final render.

use super::{Notes, Numbering, paragraph_accepts_note_label};
use crate::{Block, Document, Inline};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Mark {
    Reference {
        number: u32,
        occurrence: u32,
        /// References inside existing links retain that enclosing action.
        linked: bool,
    },
    Label(u32),
}

#[derive(Default)]
pub(crate) struct Semantics {
    inlines: Vec<(usize, Mark)>,
    bodies: Vec<(usize, u32)>,
}

impl Semantics {
    pub(super) fn collect(
        source: &Document,
        prepared: &Document,
        notes: &Notes<'_>,
        numbering: &Numbering,
        backlinks: bool,
    ) -> Self {
        let mut pairs = Pairs::default();
        let body_len = source
            .blocks
            .iter()
            .filter(|block| !matches!(block, Block::FootnoteDefinition { .. }))
            .count();
        pairs.blocks(
            &source.blocks,
            &prepared.blocks[..body_len],
            notes,
            numbering,
            0,
        );
        let mut bodies = Vec::with_capacity(numbering.order.len());
        for (offset, &index) in numbering.order.iter().enumerate() {
            let Ok(number) = u32::try_from(numbering.numbers[index]) else {
                continue;
            };
            if number == 0 || number == u32::MAX {
                continue;
            }
            let root = body_len + offset;
            let Some(Block::FootnoteDefinition { blocks, .. }) = prepared.blocks.get(root) else {
                continue;
            };
            bodies.push((root, number));
            if let Some(Block::Paragraph(items)) = blocks.first()
                && let Some(label) = items.first()
            {
                pairs.targets.insert(identity(label), Mark::Label(number));
            }
            let original = notes.definitions[index];
            let joined = original
                .iter()
                .find(|block| !matches!(block, Block::FootnoteDefinition { .. }))
                .is_some_and(|block| {
                    matches!(block, Block::Paragraph(items) if paragraph_accepts_note_label(items))
                });
            let body_start = usize::from(!joined);
            let prefix = if joined {
                // Linked labels insert a separate separating space. This is
                // decided from the generated node, never its destination text.
                if backlinks
                    && matches!(blocks.first(), Some(Block::Paragraph(items)) if matches!(items.first(), Some(Inline::Link { .. })))
                {
                    2
                } else {
                    1
                }
            } else {
                0
            };
            pairs.blocks(original, &blocks[body_start..], notes, numbering, prefix);
        }
        let mut inlines = Vec::with_capacity(pairs.targets.len());
        let mut ordinal = 0;
        super::endnote_navigation::visit_inlines(&prepared.blocks, &mut |inline| {
            if let Some(&mark) = pairs.targets.get(&identity(inline)) {
                inlines.push((ordinal, mark));
            }
            ordinal += 1;
        });
        Self { inlines, bodies }
    }

    pub(crate) fn bind_inlines(&self, document: &Document) -> BTreeMap<usize, Mark> {
        let mut pending = self.inlines.iter().peekable();
        let mut result = BTreeMap::new();
        let mut ordinal = 0;
        super::endnote_navigation::visit_inlines(&document.blocks, &mut |inline| {
            if pending.peek().is_some_and(|(index, _)| *index == ordinal)
                && let Some(&(_, mark)) = pending.next()
            {
                result.insert(identity(inline), mark);
            }
            ordinal += 1;
        });
        result
    }

    pub(crate) fn bind_bodies(&self, document: &Document) -> BTreeMap<usize, u32> {
        self.bodies
            .iter()
            .filter_map(|&(root, number)| match document.blocks.get(root) {
                Some(Block::FootnoteDefinition { blocks, .. }) => {
                    Some((blocks.as_ptr() as usize, number))
                }
                _ => None,
            })
            .collect()
    }

    pub(crate) fn linked_references(&self) -> std::collections::BTreeSet<u32> {
        self.inlines
            .iter()
            .filter_map(|&(_, mark)| match mark {
                Mark::Reference {
                    number,
                    linked: true,
                    ..
                } => Some(number),
                _ => None,
            })
            .collect()
    }
}

fn identity(inline: &Inline) -> usize {
    std::ptr::from_ref(inline) as usize
}

#[derive(Default)]
struct Pairs {
    targets: BTreeMap<usize, Mark>,
    next_reference: u32,
}

impl Pairs {
    fn blocks(
        &mut self,
        source: &[Block],
        prepared: &[Block],
        notes: &Notes<'_>,
        numbering: &Numbering,
        first_inline_prefix: usize,
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
                ) => self.inlines(
                    source,
                    &prepared[if index == 0 { first_inline_prefix } else { 0 }..],
                    notes,
                    numbering,
                ),
                (Block::BlockQuote(source), Block::BlockQuote(prepared)) => {
                    self.blocks(source, prepared, notes, numbering, 0);
                }
                (Block::List(source), Block::List(prepared)) => {
                    for (source, prepared) in source.items.iter().zip(&prepared.items) {
                        self.blocks(&source.blocks, &prepared.blocks, notes, numbering, 0);
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
                    let Some(&index) = notes.indices.get(id.as_str()) else {
                        continue;
                    };
                    let Ok(number) = u32::try_from(numbering.numbers[index]) else {
                        continue;
                    };
                    let Some(occurrence) = self.next_reference.checked_add(1) else {
                        continue;
                    };
                    if number == 0 || number == u32::MAX {
                        continue;
                    }
                    self.next_reference = occurrence;
                    self.targets.insert(
                        identity(prepared),
                        Mark::Reference {
                            number,
                            occurrence,
                            linked: matches!(prepared, Inline::Link { .. }),
                        },
                    );
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
                ) => self.inlines(source, prepared, notes, numbering),
                _ => {}
            }
        }
    }
}
