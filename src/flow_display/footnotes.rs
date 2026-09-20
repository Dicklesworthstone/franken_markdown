//! AST-backed note resolution for the live reader. Numbering follows the PDF
//! lane: first reference in body order, then references from queued notes, then
//! unreferenced definitions in source order. Cycles never expand note bodies.
//!
//! This plan borrows the admitted spanned AST. Only inline slices containing a
//! resolvable citation are rewritten; ordinary text, code, and image labels are
//! not reparsed. A note remains a sequence of semantic blocks, not flat text.

use std::borrow::Cow;
use std::collections::{BTreeMap, btree_map::Entry};

use crate::ast::{Block, Inline};
use crate::span::{SourceSpan, SpannedDocument};

#[derive(Clone, Copy)]
pub(super) struct Note<'a> {
    pub blocks: &'a [Block],
    pub span: SourceSpan,
}

pub(super) struct Footnotes<'a> {
    definitions: Vec<Note<'a>>,
    indices: BTreeMap<&'a str, usize>,
    numbering: Numbering,
}

impl<'a> Footnotes<'a> {
    /// Called only after the flow engine's source/AST/depth admission audit.
    pub fn new(document: &'a SpannedDocument) -> Self {
        let mut definitions = Vec::new();
        let mut indices = BTreeMap::new();
        let mut stack: Vec<_> = document.blocks().iter().rev()
            .map(|block| (&block.node, block.span)).collect();
        while let Some((block, span)) = stack.pop() {
            match block {
                Block::FootnoteDefinition { id, blocks } => {
                    // Match the shared PDF policy, including not descending
                    // into a discarded duplicate's nested definitions.
                    if let Entry::Vacant(entry) = indices.entry(id.as_str()) {
                        entry.insert(definitions.len());
                        definitions.push(Note { blocks, span });
                        stack.extend(blocks.iter().rev().map(|child| (child, span)));
                    }
                }
                Block::BlockQuote(blocks) => {
                    stack.extend(blocks.iter().rev().map(|child| (child, span)));
                }
                Block::List(list) => {
                    for item in list.items.iter().rev() {
                        stack.extend(item.blocks.iter().rev().map(|child| (child, span)));
                    }
                }
                _ => {}
            }
        }
        let mut numbering = Numbering {
            numbers: vec![0; definitions.len()],
            order: Vec::with_capacity(definitions.len()),
        };
        if !definitions.is_empty() {
            for block in document.blocks() {
                references(&block.node, &mut |id| numbering.reference(&indices, id));
            }
            let mut visited = 0;
            numbering.follow(&definitions, &indices, &mut visited);
            for index in 0..definitions.len() {
                numbering.assign(index);
                numbering.follow(&definitions, &indices, &mut visited);
            }
        }
        Self { definitions, indices, numbering }
    }

    pub fn order(&self) -> &[usize] { &self.numbering.order }

    pub fn note(&self, index: usize) -> Note<'a> { self.definitions[index] }

    pub fn number(&self, index: usize) -> usize { self.numbering.numbers[index] }

    /// Colons cannot be produced by the Markdown heading slugger. Keep note
    /// destinations outside the user-heading namespace without changing slugs.
    pub fn anchor(&self, index: usize) -> String {
        format!("fmd:note:{}", self.number(index))
    }

    pub fn inlines<'s>(&self, inlines: &'s [Inline]) -> Cow<'s, [Inline]> {
        if self.indices.is_empty() || !has_resolved_reference(inlines, &self.indices) {
            return Cow::Borrowed(inlines);
        }
        Cow::Owned(inlines.iter().map(|inline| self.rewrite(inline)).collect())
    }

    fn rewrite(&self, inline: &Inline) -> Inline {
        match inline {
            Inline::FootnoteRef { id } => match self.indices.get(id.as_str()) {
                Some(&index) => Inline::Link {
                    dest: format!("#{}", self.anchor(index)),
                    title: None,
                    content: vec![Inline::Text(format!("[{}]", self.number(index)))],
                },
                None => inline.clone(),
            },
            Inline::Emphasis(children) => Inline::Emphasis(self.inlines(children).into_owned()),
            Inline::Strong(children) => Inline::Strong(self.inlines(children).into_owned()),
            Inline::Strikethrough(children) => Inline::Strikethrough(self.inlines(children).into_owned()),
            Inline::Link { content, dest, title } => Inline::Link {
                content: self.inlines(content).into_owned(), dest: dest.clone(), title: title.clone(),
            },
            // Literal code/math/HTML and image alt text are not footnote syntax.
            _ => inline.clone(),
        }
    }
}

struct Numbering {
    numbers: Vec<usize>,
    order: Vec<usize>,
}

impl Numbering {
    fn assign(&mut self, index: usize) {
        if self.numbers[index] == 0 {
            self.numbers[index] = self.order.len() + 1;
            self.order.push(index);
        }
    }

    fn reference(&mut self, indices: &BTreeMap<&str, usize>, id: &str) {
        if let Some(&index) = indices.get(id) { self.assign(index); }
    }

    fn follow(&mut self, notes: &[Note<'_>], indices: &BTreeMap<&str, usize>, visited: &mut usize) {
        while *visited < self.order.len() {
            let index = self.order[*visited];
            *visited += 1;
            for block in notes[index].blocks {
                references(block, &mut |id| self.reference(indices, id));
            }
        }
    }
}

fn has_resolved_reference(inlines: &[Inline], indices: &BTreeMap<&str, usize>) -> bool {
    let mut stack: Vec<_> = inlines.iter().rev().collect();
    while let Some(inline) = stack.pop() {
        match inline {
            Inline::FootnoteRef { id } if indices.contains_key(id.as_str()) => return true,
            Inline::Emphasis(children) | Inline::Strong(children) | Inline::Strikethrough(children)
            | Inline::Link { content: children, .. } => stack.extend(children.iter().rev()),
            _ => {}
        }
    }
    false
}

enum Visit<'a> { Block(&'a Block), Inline(&'a Inline) }

/// Visit actual AST citations in reading order. A definition is deliberately
/// excluded; its outgoing references are visited when the note queue reaches it.
fn references(block: &Block, visit: &mut impl FnMut(&str)) {
    let mut stack = vec![Visit::Block(block)];
    while let Some(node) = stack.pop() {
        match node {
            Visit::Block(block) => match block {
                Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                    stack.extend(inlines.iter().rev().map(Visit::Inline));
                }
                Block::BlockQuote(blocks) => stack.extend(blocks.iter().rev().map(Visit::Block)),
                Block::List(list) => {
                    for item in list.items.iter().rev() {
                        stack.extend(item.blocks.iter().rev().map(Visit::Block));
                    }
                }
                Block::Table(table) => {
                    for row in std::iter::once(&table.head).chain(table.rows.iter()).rev() {
                        for cell in row.iter().rev() {
                            stack.extend(cell.iter().rev().map(Visit::Inline));
                        }
                    }
                }
                Block::DefinitionList(items) => {
                    for item in items.iter().rev() {
                        for inlines in item.terms.iter().chain(item.definitions.iter()).rev() {
                            stack.extend(inlines.iter().rev().map(Visit::Inline));
                        }
                    }
                }
                _ => {}
            },
            Visit::Inline(inline) => match inline {
                Inline::FootnoteRef { id } => visit(id),
                Inline::Emphasis(children) | Inline::Strong(children) | Inline::Strikethrough(children)
                | Inline::Link { content: children, .. } => {
                    stack.extend(children.iter().rev().map(Visit::Inline));
                }
                _ => {}
            },
        }
    }
}

impl super::ResumableFlowDisplay {
    /// Engine-assigned navigation identity for an emitted heading or note
    /// section. Unlike `DisplayBlock::slug`, this includes collision suffixes
    /// and generated note namespaces. None means not a heading or not emitted.
    /// Consumers must not reconstruct these IDs from text or source positions.
    #[must_use]
    pub fn heading_id_for_block(&self, index: usize) -> Option<&str> {
        self.metadata.get(index)?.heading_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::flow_display::{DisplayBlock, FlowDisplayLimits, ResumableFlowDisplay};

    fn render(source: &str, batch: usize) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, batch);
        engine.process_all().unwrap();
        engine
    }

    fn paragraph(text: &str) -> DisplayBlock { DisplayBlock::Paragraph { text: text.to_owned() } }
    fn label(number: usize) -> DisplayBlock {
        DisplayBlock::Heading { level: 6, text: format!("[{number}]") }
    }

    #[test]
    fn first_use_numbering_moves_complete_definitions_after_the_body() {
        let source = "[^a]: Alpha\n\nBody [^b], **[^a]**, again [^b], unknown [^missing].\n\n[^b]: Beta\n";
        let engine = render(source, 1);
        assert_eq!(engine.blocks(), &[
            paragraph("Body [1], [2], again [1], unknown [^missing]."),
            label(1), paragraph("Beta"), label(2), paragraph("Alpha"),
        ]);
        let DisplayBlock::Paragraph { text } = &engine.blocks()[0] else { panic!("body"); };
        let runs = engine.inline_runs_for_block(0).unwrap();
        let citations: Vec<_> = runs.iter().filter(|run| run.link.is_some()).collect();
        assert_eq!(citations.len(), 3);
        assert_eq!(citations[0].active_link_target(), Some("#fmd:note:1"));
        assert_eq!(citations[1].active_link_target(), Some("#fmd:note:2"));
        assert!(citations[1].style.bold);
        assert_eq!(&text[citations[1].range.clone()], "[2]");
        assert!(engine.source_span_for_block(1).unwrap().slice(source).unwrap().contains("[^b]:"));
        assert!(engine.source_span_for_block(3).unwrap().slice(source).unwrap().contains("[^a]:"));
    }

    #[test]
    fn note_to_note_cycles_repeated_references_and_unreferenced_notes_terminate() {
        let source = "Body [^b] [^b].\n\n[^a]: A [^b]\n\n[^b]: B [^a] [^b]\n\n[^unused]: Unreferenced\n";
        let engine = render(source, 1);
        assert_eq!(engine.blocks(), &[
            paragraph("Body [1] [1]."), label(1), paragraph("B [2] [1]"),
            label(2), paragraph("A [1]"), label(3), paragraph("Unreferenced"),
        ]);
    }

    #[test]
    fn ordinary_documents_and_literal_code_do_not_acquire_note_links() {
        let source = "Literal `[^a]` and unknown [^missing].\n\n[^a]: Alpha\n";
        let engine = render(source, 1);
        assert_eq!(engine.blocks()[0], paragraph("Literal [^a] and unknown [^missing]."));
        assert!(engine.inline_runs_for_block(0).unwrap().iter().all(|run| run.link.is_none()));
        let document = crate::parse_markdown_spanned("Plain **text** and `code`.");
        let notes = Footnotes::new(&document);
        let Block::Paragraph(inlines) = &document.blocks()[0].node else { panic!("paragraph"); };
        assert!(matches!(notes.inlines(inlines), Cow::Borrowed(_)));
    }

    #[test]
    fn citations_in_table_cells_and_nested_containers_keep_reading_order() {
        let source = "| H [^b] |\n| --- |\n| Cell [^a] |\n\n> Quote [^c]\n\n- List [^b]\n\n[^a]: A\n\n[^b]: B\n\n[^c]: C\n";
        let engine = render(source, 2);
        assert_eq!(engine.blocks()[0], DisplayBlock::TableHeader { cells: vec!["H [1]".into()] });
        assert_eq!(engine.blocks()[1], DisplayBlock::TableRow { cells: vec!["Cell [2]".into()] });
        assert!(engine.inline_runs_for_cell(0, 0).unwrap().iter()
            .any(|run| run.active_link_target() == Some("#fmd:note:1")));
        assert!(engine.inline_runs_for_cell(1, 0).unwrap().iter()
            .any(|run| run.active_link_target() == Some("#fmd:note:2")));
        assert!(engine.blocks().iter().any(|block| matches!(block,
            DisplayBlock::Quote { text } if text == "Quote [3]")));
        let labels: Vec<_> = engine.blocks().iter().filter_map(|block| match block {
            DisplayBlock::Heading { level: 6, text } => Some(text.as_str()), _ => None,
        }).collect();
        assert_eq!(labels, vec!["[1]", "[2]", "[3]"]);
    }

    #[test]
    fn note_bodies_preserve_code_tables_lists_formatting_and_authorized_asset_requests() {
        let source = "Use [^rich].\n\n[^rich]: **Bold** é中🙂\n\n    ```rust\n    let x = 1;\n    ```\n\n    | H |\n    | --- |\n    | V |\n\n    - item\n\n    ![caption](note.png)\n";
        let engine = render(source, 1);
        assert!(engine.blocks().iter().any(|block| matches!(block,
            DisplayBlock::CodeBlock { language, source } if language.as_deref() == Some("rust") && source.contains("let x = 1;"))));
        assert!(engine.blocks().iter().any(|block| matches!(block, DisplayBlock::TableHeader { .. })));
        assert!(engine.blocks().iter().any(|block| matches!(block, DisplayBlock::ListItem { .. })));
        assert_eq!(engine.unresolved_assets().len(), 1);
        assert_eq!(engine.unresolved_assets()[0].url, "note.png");
        assert_eq!(engine.unresolved_assets()[0].alt_text, "caption");
        let body = engine.blocks().iter().position(|block| matches!(block,
            DisplayBlock::Paragraph { text } if text == "Bold é中🙂")).unwrap();
        assert!(engine.inline_runs_for_block(body).unwrap().iter().any(|run| run.style.bold));
        for index in 0..engine.blocks().len() {
            assert!(engine.source_span_for_block(index).unwrap().slice(source).is_some());
        }
    }

    #[test]
    fn note_targets_are_unique_and_stepping_is_identical_to_whole_document_output() {
        let source = "# fmd:note:1\n\nText [^b] [^a].\n\n[^a]: First definition\n\n[^b]: Second definition\n";
        let expected = render(source, usize::MAX);
        let expected_display = expected.to_display_list();
        let anchors: Vec<_> = expected_display.anchors().map(|anchor| anchor.anchor_id.as_str()).collect();
        assert!(anchors.contains(&"fmd:note:1"));
        assert!(anchors.contains(&"fmd:note:2"));
        let mut unique = anchors.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), anchors.len());
        for batch in [1, 2, 3, 7] {
            let mut engine = ResumableFlowDisplay::new(source, batch);
            while let Some(step) = engine.step().unwrap() { assert!(step.blocks.len() <= batch); }
            assert_eq!(engine.blocks(), expected.blocks());
            assert_eq!(engine.to_display_list(), expected_display);
            for index in 0..engine.blocks().len() {
                assert_eq!(engine.inline_runs_for_block(index), expected.inline_runs_for_block(index));
                assert_eq!(engine.source_span_for_block(index), expected.source_span_for_block(index));
            }
        }
    }

    #[test]
    fn note_projection_budget_failure_publishes_no_partial_document() {
        let limits = FlowDisplayLimits { max_output_blocks: 2, ..FlowDisplayLimits::default() };
        let mut engine = ResumableFlowDisplay::try_with_limits("Use [^x].\n\n[^x]: Note\n", 1, 1, limits).unwrap();
        let error = engine.step().unwrap_err();
        assert!(matches!(error, super::super::FlowDisplayError::BudgetExceeded(_)));
        assert!(engine.blocks().is_empty());
        assert!(engine.unresolved_assets().is_empty());
        assert_eq!(engine.step().unwrap_err(), error);
    }
}
