//! Structural correspondence for source probes, across every Markdown container.
//! Only Link destinations are counted/restored. Images, code and raw HTML never
//! become links because they contain matching text. Callers validate full AST
//! equality after restoration; this walker does not grant source authority.

use franken_markdown::{Block, Inline};

pub(super) fn supported(block: &Block) -> bool {
    matches!(
        block,
        Block::Paragraph(_)
            | Block::Heading { .. }
            | Block::BlockQuote(_)
            | Block::List(_)
            | Block::Table(_)
            | Block::DefinitionList(_)
            | Block::FootnoteDefinition { .. }
    )
}

pub(super) fn count(blocks: &[Block], probe: &str) -> usize {
    blocks.iter().map(|block| count_block(block, probe)).sum()
}

pub(super) fn count_block(block: &Block, probe: &str) -> usize {
    match block {
        Block::Paragraph(items) | Block::Heading { inlines: items, .. } => {
            count_inlines(items, probe)
        }
        Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => {
            count(blocks, probe)
        }
        Block::List(list) => list.items.iter().map(|item| count(&item.blocks, probe)).sum(),
        Block::Table(table) => table
            .head
            .iter()
            .chain(table.rows.iter().flatten())
            .map(|cell| count_inlines(cell, probe))
            .sum(),
        Block::DefinitionList(items) => items
            .iter()
            .flat_map(|item| item.terms.iter().chain(&item.definitions))
            .map(|items| count_inlines(items, probe))
            .sum(),
        _ => 0,
    }
}

fn count_inlines(items: &[Inline], probe: &str) -> usize {
    items
        .iter()
        .map(|item| match item {
            Inline::Link { dest, content, .. } => {
                usize::from(dest.as_str() == probe) + count_inlines(content, probe)
            }
            Inline::Emphasis(items) | Inline::Strong(items) | Inline::Strikethrough(items) => {
                count_inlines(items, probe)
            }
            _ => 0,
        })
        .sum()
}

pub(super) fn restore(
    block: &mut Block,
    original: &Block,
    probe: &str,
    destination: &mut Option<String>,
) {
    match (block, original) {
        (Block::Paragraph(items), Block::Paragraph(old))
        | (Block::Heading { inlines: items, .. }, Block::Heading { inlines: old, .. }) => {
            restore_inlines(items, old, probe, destination);
        }
        (Block::BlockQuote(blocks), Block::BlockQuote(old))
        | (Block::FootnoteDefinition { blocks, .. }, Block::FootnoteDefinition { blocks: old, .. }) => {
            restore_blocks(blocks, old, probe, destination);
        }
        (Block::List(list), Block::List(old)) if list.items.len() == old.items.len() => {
            for (item, old) in list.items.iter_mut().zip(&old.items) {
                restore_blocks(&mut item.blocks, &old.blocks, probe, destination);
            }
        }
        (Block::Table(table), Block::Table(old)) if table.rows.len() == old.rows.len() => {
            restore_cells(&mut table.head, &old.head, probe, destination);
            for (row, old) in table.rows.iter_mut().zip(&old.rows) {
                restore_cells(row, old, probe, destination);
            }
        }
        (Block::DefinitionList(items), Block::DefinitionList(old)) if items.len() == old.len() => {
            for (item, old) in items.iter_mut().zip(old) {
                restore_cells(&mut item.terms, &old.terms, probe, destination);
                restore_cells(&mut item.definitions, &old.definitions, probe, destination);
            }
        }
        _ => {}
    }
}

fn restore_blocks(
    blocks: &mut [Block],
    original: &[Block],
    probe: &str,
    destination: &mut Option<String>,
) {
    if blocks.len() == original.len() {
        for (block, old) in blocks.iter_mut().zip(original) {
            restore(block, old, probe, destination);
        }
    }
}

fn restore_cells(
    cells: &mut [Vec<Inline>],
    original: &[Vec<Inline>],
    probe: &str,
    destination: &mut Option<String>,
) {
    if cells.len() == original.len() {
        for (cell, old) in cells.iter_mut().zip(original) {
            restore_inlines(cell, old, probe, destination);
        }
    }
}

fn restore_inlines(
    items: &mut [Inline],
    original: &[Inline],
    probe: &str,
    destination: &mut Option<String>,
) {
    if items.len() != original.len() {
        return;
    }
    for (item, original) in items.iter_mut().zip(original) {
        match (item, original) {
            (
                Inline::Link { dest, content, .. },
                Inline::Link { dest: old, content: old_content, .. },
            ) => {
                if dest.as_str() == probe {
                    *destination = Some(old.clone());
                    dest.clone_from(old);
                }
                restore_inlines(content, old_content, probe, destination);
            }
            (Inline::Emphasis(items), Inline::Emphasis(old))
            | (Inline::Strong(items), Inline::Strong(old))
            | (Inline::Strikethrough(items), Inline::Strikethrough(old)) => {
                restore_inlines(items, old, probe, destination);
            }
            _ => {}
        }
    }
}
