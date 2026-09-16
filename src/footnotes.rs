//! Lossless PDF footnote preparation over the shared document AST.
//!
//! The PDF layout engine consumes ordinary blocks and inline text. Resolve
//! references once, move complete note bodies to the end, and leave their
//! internal block structure intact. Note-reference cycles are graph edges,
//! not recursive expansion: every definition is emitted at most once.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::ast::{Block, Document, Inline};

/// Preserve the historical `[n]` PDF reference style while retaining every
/// supported block in each note. Number referenced notes by first use in the
/// body, then traverse note-to-note references in that order. Definitions
/// without references follow in source order, preserving the legacy policy
/// that an explicitly supplied definition is not silently discarded.
///
/// Duplicate definitions use the first definition. Undefined references stay
/// visible as `[^id]`, never the misleading `[0]`. A document with no footnote
/// syntax is borrowed, avoiding a whole-AST clone on the ordinary PDF path.
pub(crate) fn for_pdf(doc: &Document) -> Cow<'_, Document> {
    let mut notes = Notes::default();
    notes.collect(&doc.blocks);
    if notes.definitions.is_empty() && !notes.has_reference {
        return Cow::Borrowed(doc);
    }

    let mut numbering = Numbering {
        numbers: vec![0; notes.definitions.len()],
        order: Vec::with_capacity(notes.definitions.len()),
    };
    references_in_blocks(&doc.blocks, &mut |id| numbering.reference(&notes, id));
    let mut visited = 0;
    numbering.follow_references(&notes, &mut visited);
    for index in 0..notes.definitions.len() {
        numbering.assign(index);
        numbering.follow_references(&notes, &mut visited);
    }

    let mut blocks = rewrite_blocks(&doc.blocks, &notes, &numbering);
    if !notes.definitions.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            inlines: vec![Inline::Text("Notes".to_string())],
        });
        for &index in &numbering.order {
            let mut body = rewrite_blocks(notes.definitions[index], &notes, &numbering);
            let label = format!("[{}]", numbering.numbers[index]);
            match body.first_mut() {
                Some(Block::Paragraph(inlines)) => {
                    inlines.insert(0, Inline::Text(format!("{label} ")));
                }
                _ => {
                    // A code fence, table, list, image, equation, or heading
                    // is not a paragraph to flatten. Give it a separate note
                    // label and preserve the original blocks underneath it.
                    body.insert(0, Block::Paragraph(vec![Inline::Text(label)]));
                }
            }
            blocks.extend(body);
        }
    }
    Cow::Owned(Document { blocks })
}

#[derive(Default)]
struct Notes<'a> {
    indices: BTreeMap<&'a str, usize>,
    definitions: Vec<&'a [Block]>,
    has_reference: bool,
}

impl<'a> Notes<'a> {
    fn collect(&mut self, blocks: &'a [Block]) {
        for block in blocks {
            match block {
                Block::FootnoteDefinition { id, blocks } => {
                    if !self.indices.contains_key(id.as_str()) {
                        let index = self.definitions.len();
                        self.indices.insert(id, index);
                        self.definitions.push(blocks);
                        self.collect(blocks);
                    }
                }
                Block::BlockQuote(inner) => self.collect(inner),
                Block::List(list) => {
                    for item in &list.items {
                        self.collect(&item.blocks);
                    }
                }
                _ => references_in_block(block, &mut |_| self.has_reference = true),
            }
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

    fn reference(&mut self, notes: &Notes<'_>, id: &str) {
        if let Some(&index) = notes.indices.get(id) {
            self.assign(index);
        }
    }

    fn follow_references(&mut self, notes: &Notes<'_>, visited: &mut usize) {
        while *visited < self.order.len() {
            let index = self.order[*visited];
            *visited += 1;
            references_in_blocks(notes.definitions[index], &mut |id| self.reference(notes, id));
        }
    }
}

/// Definitions do not appear in normal flow. Their references are traversed
/// separately when their turn arrives in the note queue.
fn references_in_blocks(blocks: &[Block], visit: &mut impl FnMut(&str)) {
    for block in blocks {
        references_in_block(block, visit);
    }
}

fn references_in_block(block: &Block, visit: &mut impl FnMut(&str)) {
    match block {
        Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
            references_in_inlines(inlines, visit);
        }
        Block::BlockQuote(inner) => references_in_blocks(inner, visit),
        Block::List(list) => {
            for item in &list.items {
                references_in_blocks(&item.blocks, visit);
            }
        }
        Block::Table(table) => {
            for cell in &table.head {
                references_in_inlines(cell, visit);
            }
            for row in &table.rows {
                for cell in row {
                    references_in_inlines(cell, visit);
                }
            }
        }
        Block::DefinitionList(items) => {
            for item in items {
                for inlines in item.terms.iter().chain(&item.definitions) {
                    references_in_inlines(inlines, visit);
                }
            }
        }
        Block::FootnoteDefinition { .. }
        | Block::CodeBlock { .. }
        | Block::ThematicBreak
        | Block::HtmlBlock(_)
        | Block::MathBlock(_)
        | Block::PageBreak => {}
    }
}

fn references_in_inlines(inlines: &[Inline], visit: &mut impl FnMut(&str)) {
    for inline in inlines {
        match inline {
            Inline::FootnoteRef { id } => visit(id),
            Inline::Emphasis(content)
            | Inline::Strong(content)
            | Inline::Strikethrough(content)
            | Inline::Link { content, .. } => references_in_inlines(content, visit),
            _ => {}
        }
    }
}

fn rewrite_blocks(blocks: &[Block], notes: &Notes<'_>, numbering: &Numbering) -> Vec<Block> {
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        let rewritten = match block {
            Block::FootnoteDefinition { .. } => continue,
            Block::Paragraph(inlines) => {
                Block::Paragraph(rewrite_inlines(inlines, notes, numbering))
            }
            Block::Heading { level, inlines } => Block::Heading {
                level: *level,
                inlines: rewrite_inlines(inlines, notes, numbering),
            },
            Block::BlockQuote(inner) => Block::BlockQuote(rewrite_blocks(inner, notes, numbering)),
            Block::List(list) => {
                let mut list = list.clone();
                for item in &mut list.items {
                    item.blocks = rewrite_blocks(&item.blocks, notes, numbering);
                }
                Block::List(list)
            }
            Block::Table(table) => {
                let mut table = table.clone();
                for cell in &mut table.head {
                    *cell = rewrite_inlines(cell, notes, numbering);
                }
                for row in &mut table.rows {
                    for cell in row {
                        *cell = rewrite_inlines(cell, notes, numbering);
                    }
                }
                Block::Table(table)
            }
            Block::DefinitionList(items) => {
                let mut items = items.clone();
                for item in &mut items {
                    for inlines in item.terms.iter_mut().chain(&mut item.definitions) {
                        *inlines = rewrite_inlines(inlines, notes, numbering);
                    }
                }
                Block::DefinitionList(items)
            }
            other => other.clone(),
        };
        out.push(rewritten);
    }
    out
}

fn rewrite_inlines(inlines: &[Inline], notes: &Notes<'_>, numbering: &Numbering) -> Vec<Inline> {
    inlines.iter().map(|inline| match inline {
        Inline::FootnoteRef { id } => Inline::Text(match notes.indices.get(id.as_str()) {
            Some(&index) => format!("[{}]", numbering.numbers[index]),
            None => format!("[^{id}]"),
        }),
        Inline::Emphasis(content) => Inline::Emphasis(rewrite_inlines(content, notes, numbering)),
        Inline::Strong(content) => Inline::Strong(rewrite_inlines(content, notes, numbering)),
        Inline::Strikethrough(content) => {
            Inline::Strikethrough(rewrite_inlines(content, notes, numbering))
        }
        Inline::Link { dest, title, content } => Inline::Link {
            dest: dest.clone(),
            title: title.clone(),
            content: rewrite_inlines(content, notes, numbering),
        },
        other => other.clone(),
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Align, DefinitionItem, List, ListItem, Table};

    fn text(value: &str) -> Inline { Inline::Text(value.to_string()) }
    fn reference(id: &str) -> Inline { Inline::FootnoteRef { id: id.to_string() } }
    fn paragraph(value: &str) -> Block { Block::Paragraph(vec![text(value)]) }
    fn definition(id: &str, blocks: Vec<Block>) -> Block {
        Block::FootnoteDefinition { id: id.to_string(), blocks }
    }
    fn document(blocks: Vec<Block>) -> Document { Document { blocks } }

    #[test]
    fn note_free_document_is_borrowed_including_literal_footnote_text() {
        let doc = document(vec![paragraph("literal [^id]"), Block::CodeBlock {
            lang: None, code: "[^id]: not a parsed definition".into(),
        }]);
        assert!(matches!(for_pdf(&doc), Cow::Borrowed(_)));
    }

    #[test]
    fn single_paragraph_note_retains_historical_shape() {
        let doc = document(vec![
            Block::Paragraph(vec![text("Body"), reference("a")]),
            definition("a", vec![paragraph("Citation")]),
        ]);
        assert_eq!(for_pdf(&doc).as_ref(), &document(vec![
            Block::Paragraph(vec![text("Body"), text("[1]")]),
            Block::Heading { level: 2, inlines: vec![text("Notes")] },
            Block::Paragraph(vec![text("[1] "), text("Citation")]),
        ]));
    }

    #[test]
    fn numbering_follows_references_not_definition_order() {
        let doc = document(vec![
            definition("a", vec![paragraph("A")]),
            definition("b", vec![paragraph("B")]),
            Block::Paragraph(vec![reference("b"), reference("a"), reference("b")]),
        ]);
        let transformed = for_pdf(&doc);
        assert_eq!(transformed.blocks[0], Block::Paragraph(vec![text("[1]"), text("[2]"), text("[1]")]));
        assert_eq!(transformed.blocks[2], Block::Paragraph(vec![text("[1] "), text("B")]));
        assert_eq!(transformed.blocks[3], Block::Paragraph(vec![text("[2] "), text("A")]));
    }

    #[test]
    fn every_rich_note_block_survives_in_order_and_sources_are_unchanged() {
        let rich = vec![
            Block::CodeBlock { lang: Some("rust".into()), code: "let evidence = 42;".into() },
            Block::BlockQuote(vec![paragraph("Quoted evidence")]),
            Block::List(List { ordered: true, start: 3, tight: false, items: vec![ListItem {
                task: Some(true), blocks: vec![paragraph("List evidence")],
            }] }),
            Block::Table(Table { align: vec![Align::Right], head: vec![vec![text("Measure")]],
                rows: vec![vec![vec![text("42")]]] }),
            Block::MathBlock("x^2".into()),
            Block::DefinitionList(vec![DefinitionItem {
                terms: vec![vec![text("Term")]], definitions: vec![vec![text("Meaning")]],
            }]),
            Block::Paragraph(vec![Inline::Image { dest: "plot.svg".into(), title: None, alt: "Plot".into() }]),
            Block::HtmlBlock("<aside>Raw evidence</aside>".into()),
            Block::Heading { level: 3, inlines: vec![text("Appendix")] },
            Block::ThematicBreak,
            Block::PageBreak,
            paragraph("Last evidence"),
        ];
        let doc = document(vec![Block::Paragraph(vec![reference("rich")]), definition("rich", rich.clone())]);
        let original = doc.clone();
        let result = for_pdf(&doc);
        assert_eq!(result.blocks[2], paragraph("[1]"));
        assert_eq!(&result.blocks[3..], rich.as_slice());
        assert_eq!(doc, original);
    }

    #[test]
    fn references_inside_tables_and_definition_lists_are_resolved() {
        let doc = document(vec![
            Block::Table(Table { align: vec![Align::Left], head: vec![vec![reference("b")]],
                rows: vec![vec![vec![Inline::Strong(vec![reference("a")])]]] }),
            Block::DefinitionList(vec![DefinitionItem {
                terms: vec![vec![reference("a")]],
                definitions: vec![vec![Inline::Link { dest: "https://example.com".into(),
                    title: Some("Link".into()), content: vec![reference("b")] }]],
            }),
            definition("a", vec![paragraph("A")]), definition("b", vec![paragraph("B")]),
        ]);
        let result = for_pdf(&doc);
        assert_eq!(result.blocks[0], Block::Table(Table { align: vec![Align::Left],
            head: vec![vec![text("[1]")]], rows: vec![vec![vec![Inline::Strong(vec![text("[2]")])]]] }));
        assert_eq!(result.blocks[1], Block::DefinitionList(vec![DefinitionItem {
            terms: vec![vec![text("[2]")]],
            definitions: vec![vec![Inline::Link { dest: "https://example.com".into(),
                title: Some("Link".into()), content: vec![text("[1]")] }]],
        }]));
    }

    #[test]
    fn nested_reference_cycles_emit_each_note_once_without_recursive_expansion() {
        let doc = document(vec![
            Block::Paragraph(vec![reference("a")]),
            definition("a", vec![Block::Paragraph(vec![text("A"), reference("b")])]),
            definition("b", vec![Block::Paragraph(vec![text("B"), reference("a")])]),
        ]);
        let result = for_pdf(&doc);
        assert_eq!(result.blocks.len(), 4);
        assert_eq!(result.blocks[2], Block::Paragraph(vec![text("[1] "), text("A"), text("[2]")]));
        assert_eq!(result.blocks[3], Block::Paragraph(vec![text("[2] "), text("B"), text("[1]")]));
    }

    #[test]
    fn first_definition_wins_and_unreferenced_notes_are_not_lost() {
        let doc = document(vec![
            definition("unused", vec![paragraph("Unreferenced evidence")]),
            Block::Paragraph(vec![reference("a")]),
            definition("a", vec![paragraph("First definition")]),
            definition("a", vec![paragraph("Duplicate definition")]),
        ]);
        let result = for_pdf(&doc);
        assert_eq!(result.blocks.len(), 4);
        assert_eq!(result.blocks[2], Block::Paragraph(vec![text("[1] "), text("First definition")]));
        assert_eq!(result.blocks[3], Block::Paragraph(vec![text("[2] "), text("Unreferenced evidence")]));
    }

    #[test]
    fn undefined_references_remain_literal_instead_of_zero() {
        let doc = document(vec![Block::Paragraph(vec![reference("missing")])]);
        assert_eq!(for_pdf(&doc).as_ref(), &document(vec![paragraph("[^missing]")]));
    }

    #[test]
    fn nested_definitions_are_moved_once_and_continuation_paragraphs_keep_their_text() {
        let doc = document(vec![
            Block::Paragraph(vec![reference("outer")]),
            definition("outer", vec![paragraph("First"), paragraph("Continuation"),
                definition("inner", vec![paragraph("Nested definition")]),
                Block::Paragraph(vec![reference("inner")])]),
        ]);
        let result = for_pdf(&doc);
        assert_eq!(result.blocks.len(), 6);
        assert_eq!(result.blocks[3], paragraph("Continuation"));
        assert_eq!(result.blocks[4], Block::Paragraph(vec![text("[2]")]));
        assert_eq!(result.blocks[5], Block::Paragraph(vec![text("[2] "), text("Nested definition")]));
        assert!(matches!(for_pdf(result.as_ref()), Cow::Borrowed(_)), "preparation is idempotent");
    }
}
