//! Semantic list labels retain source identity through wrapping and pagination.
//!
//! Label provenance comes from layout-created markers, never from their text.
//! A label belongs directly to its source /LI, before the sibling /LBody that
//! owns paragraphs, nested lists, quotes, code, tables and figures.

use super::*;

pub(super) fn has_label(line: &Line) -> bool {
    !line.list_path.is_empty()
        && line
            .segs
            .iter()
            .any(|seg| seg.list_label && !seg.text.is_empty())
}

pub(super) fn path(line: &Line) -> Option<SmallPath> {
    if !has_label(line) {
        return None;
    }
    let item = line.list_path.last()?.item;
    let prefix = container_prefix(line);
    let end = prefix
        .iter()
        .rposition(|elem| elem.key == SKey::ListItem(item))?;
    let mut path = SmallPath::from_slice(&prefix[..=end]);
    path.push(SElem {
        key: SKey::ListLabel(item),
        tag: "Lbl",
    });
    Some(path)
}

/// An explicitly empty list item still has a body in the logical structure.
/// No invisible glyph or empty marked-content occurrence is needed to retain
/// it. Nonempty bodies are already shared across all of the item's fragments.
pub(super) fn complete_empty_bodies(nodes: &mut Vec<SNode>) {
    let existing = nodes.len();
    for parent in 0..existing {
        if nodes[parent].tag != "LI"
            || nodes[parent]
                .kids
                .iter()
                .any(|kid| matches!(kid, SKid::Node(index) if nodes[*index].tag == "LBody"))
        {
            continue;
        }
        let index = nodes.len();
        nodes.push(SNode {
            tag: "LBody",
            parent,
            kids: Vec::new(),
            alt: None,
            bbox: None,
            scope_column: false,
            table_column: None,
            list_numbering: None,
            page: None,
        });
        nodes[parent].kids.push(SKid::Node(index));
    }
}
