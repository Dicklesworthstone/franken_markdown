//! Root ownership for the canonical paged-note transform.

use super::{Notes, number_notes};
use crate::{Block, Document, RenderError, Result};

/// For every output root, return the original root containing its source.
/// Prepared note wrappers and their generated labels inherit their definition,
/// even when another chapter supplies their first citation. The same inventory
/// and numbering used by preparation decide duplicate and nested definitions.
pub(crate) fn paged_root_origins(doc: &Document) -> Result<Vec<usize>> {
    let mut notes = Notes::default();
    notes.collect(&doc.blocks);
    let numbering = number_notes(doc, &notes);
    let mut definitions = vec![None; notes.definitions.len()];
    fn visit(blocks: &[Block], root: usize, notes: &Notes<'_>, definitions: &mut [Option<usize>]) {
        for block in blocks {
            match block {
                Block::FootnoteDefinition { id, blocks } => {
                    if let Some(&index) = notes.indices.get(id.as_str())
                        && definitions[index].is_none()
                    {
                        definitions[index] = Some(root);
                        visit(blocks, root, notes, definitions);
                    }
                }
                Block::BlockQuote(blocks) => visit(blocks, root, notes, definitions),
                Block::List(list) => {
                    for item in &list.items {
                        visit(&item.blocks, root, notes, definitions);
                    }
                }
                _ => {}
            }
        }
    }
    let mut origins = Vec::with_capacity(doc.blocks.len().saturating_add(definitions.len()));
    for (index, block) in doc.blocks.iter().enumerate() {
        visit(std::slice::from_ref(block), index, &notes, &mut definitions);
        if !matches!(block, Block::FootnoteDefinition { .. }) {
            origins.push(index);
        }
    }
    for definition in numbering.order {
        origins.push(definitions[definition].ok_or_else(|| {
            RenderError::InvalidInput(
                "book_pdf_language: note definition has no source owner".to_string(),
            )
        })?);
    }
    Ok(origins)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::Inline;

    fn note(id: &str, blocks: Vec<Block>) -> Block {
        Block::FootnoteDefinition {
            id: id.into(),
            blocks,
        }
    }

    fn paragraph(text: &str) -> Block {
        Block::Paragraph(vec![Inline::Text(text.into())])
    }

    #[test]
    fn reordered_nested_and_duplicate_notes_keep_their_original_root() {
        let doc = Document {
            blocks: vec![
                Block::Paragraph(vec![Inline::FootnoteRef { id: "later".into() }]),
                Block::BlockQuote(vec![note(
                    "early",
                    vec![paragraph("Early"), note("inner", vec![paragraph("Inner")])],
                )]),
                note("later", vec![paragraph("Later")]),
                note(
                    "later",
                    vec![note("hidden", vec![paragraph("Discarded duplicate")])],
                ),
                note("unused", vec![paragraph("Unused")]),
            ],
        };
        let prepared = crate::footnotes::for_pdf_paged(&doc);
        let origins = paged_root_origins(&doc).unwrap();
        assert_eq!(origins, [0, 1, 2, 1, 1, 4]);
        assert_eq!(origins.len(), prepared.blocks.len());
    }
}
