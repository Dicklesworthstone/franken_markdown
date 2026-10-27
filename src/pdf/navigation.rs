//! PDF bookmark topology from the actual source heading levels.
//!
//! Items retain document order and stable object-number ordinals. A heading's
//! parent is the nearest preceding heading of a lower level; skipped levels
//! do not create fictitious bookmarks. All items start open, so each /Count is
//! the number of all descendants and the root count is the item count.

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct OutlineNode {
    pub(super) parent: Option<usize>,
    pub(super) previous: Option<usize>,
    pub(super) next: Option<usize>,
    pub(super) first: Option<usize>,
    pub(super) last: Option<usize>,
    pub(super) descendants: usize,
}

#[derive(Debug, Default)]
pub(super) struct OutlineTree {
    pub(super) nodes: Vec<OutlineNode>,
    /// First and last root-level siblings, not the first and last descendants.
    pub(super) first: Option<usize>,
    pub(super) last: Option<usize>,
}

impl OutlineTree {
    pub(super) fn new(levels: impl IntoIterator<Item = u8>) -> Self {
        let mut tree = Self::default();
        let mut ancestors: Vec<(u8, usize)> = Vec::with_capacity(6);
        for level in levels {
            let level = level.clamp(1, 6);
            while ancestors.last().is_some_and(|&(open, _)| open >= level) {
                ancestors.pop();
            }
            let index = tree.nodes.len();
            let parent = ancestors.last().map(|&(_, index)| index);
            let previous = parent.map_or(tree.last, |parent| tree.nodes[parent].last);
            tree.nodes.push(OutlineNode {
                parent,
                previous,
                ..OutlineNode::default()
            });
            if let Some(previous) = previous {
                tree.nodes[previous].next = Some(index);
            }
            if let Some(parent) = parent {
                tree.nodes[parent].first.get_or_insert(index);
                tree.nodes[parent].last = Some(index);
            } else {
                tree.first.get_or_insert(index);
                tree.last = Some(index);
            }
            // At most six source heading levels can be open. Counting here
            // avoids an unbounded recursive walk or a quadratic subtree scan.
            for &(_, ancestor) in &ancestors {
                tree.nodes[ancestor].descendants += 1;
            }
            ancestors.push((level, index));
        }
        tree
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_flat_outlines_retain_their_sibling_shape() {
        let empty = OutlineTree::new([]);
        assert!(empty.nodes.is_empty());
        assert_eq!((empty.first, empty.last), (None, None));
        let flat = OutlineTree::new([2, 2, 2]);
        assert_eq!((flat.first, flat.last), (Some(0), Some(2)));
        assert_eq!(
            flat.nodes[0],
            OutlineNode {
                next: Some(1),
                ..OutlineNode::default()
            }
        );
        assert_eq!(
            flat.nodes[1],
            OutlineNode {
                previous: Some(0),
                next: Some(2),
                ..OutlineNode::default()
            }
        );
        assert_eq!(
            flat.nodes[2],
            OutlineNode {
                previous: Some(1),
                ..OutlineNode::default()
            }
        );
    }

    #[test]
    fn nested_items_link_siblings_and_count_all_open_descendants() {
        let tree = OutlineTree::new([1, 2, 3, 2, 1, 2]);
        assert_eq!((tree.first, tree.last), (Some(0), Some(4)));
        assert_eq!(
            tree.nodes[0],
            OutlineNode {
                next: Some(4),
                first: Some(1),
                last: Some(3),
                descendants: 3,
                ..OutlineNode::default()
            }
        );
        assert_eq!(
            tree.nodes[1],
            OutlineNode {
                parent: Some(0),
                next: Some(3),
                first: Some(2),
                last: Some(2),
                descendants: 1,
                ..OutlineNode::default()
            }
        );
        assert_eq!(
            tree.nodes[2],
            OutlineNode {
                parent: Some(1),
                ..OutlineNode::default()
            }
        );
        assert_eq!(
            tree.nodes[3],
            OutlineNode {
                parent: Some(0),
                previous: Some(1),
                ..OutlineNode::default()
            }
        );
        assert_eq!(
            tree.nodes[4],
            OutlineNode {
                previous: Some(0),
                first: Some(5),
                last: Some(5),
                descendants: 1,
                ..OutlineNode::default()
            }
        );
        assert_eq!(
            tree.nodes[5],
            OutlineNode {
                parent: Some(4),
                ..OutlineNode::default()
            }
        );
    }

    #[test]
    fn skipped_levels_find_the_nearest_lower_heading_without_placeholder_nodes() {
        let tree = OutlineTree::new([3, 6, 4, 5, 2, 5, 1, 6]);
        assert_eq!(tree.nodes.len(), 8);
        assert_eq!(
            tree.nodes
                .iter()
                .map(|node| node.parent)
                .collect::<Vec<_>>(),
            [
                None,
                Some(0),
                Some(0),
                Some(2),
                None,
                Some(4),
                None,
                Some(6)
            ]
        );
        assert_eq!((tree.first, tree.last), (Some(0), Some(6)));
        assert_eq!(tree.nodes[0].next, Some(4));
        assert_eq!(tree.nodes[4].next, Some(6));
        assert_eq!(tree.nodes[0].descendants, 3);
    }

    #[test]
    fn exhaustive_small_trees_preserve_source_order_and_reciprocal_links() {
        for encoding in 0..3_usize.pow(6) {
            let mut value = encoding;
            let levels: Vec<_> = (0..6)
                .map(|_| {
                    let level = 1 + (value % 3) as u8;
                    value /= 3;
                    level
                })
                .collect();
            let tree = OutlineTree::new(levels.iter().copied());
            let mut stack = vec![tree.first.unwrap()];
            let mut visited = Vec::new();
            while let Some(index) = stack.pop() {
                assert!(!visited.contains(&index), "cycle in {levels:?}");
                visited.push(index);
                let node = &tree.nodes[index];
                if let Some(next) = node.next {
                    assert_eq!(tree.nodes[next].previous, Some(index));
                    assert_eq!(tree.nodes[next].parent, node.parent);
                    stack.push(next);
                }
                if let Some(first) = node.first {
                    assert_eq!(tree.nodes[first].parent, Some(index));
                    assert_eq!(tree.nodes[first].previous, None);
                    assert_eq!(tree.nodes[node.last.unwrap()].next, None);
                    stack.push(first);
                }
                let expected_parent = (0..index)
                    .rev()
                    .find(|&prior| levels[prior] < levels[index]);
                assert_eq!(node.parent, expected_parent, "{levels:?}");
                let subtree_end = (index + 1..levels.len())
                    .find(|&later| levels[later] <= levels[index])
                    .unwrap_or(levels.len());
                assert_eq!(node.descendants, subtree_end - index - 1, "{levels:?}");
            }
            assert_eq!(visited, (0..levels.len()).collect::<Vec<_>>());
        }
    }

    #[test]
    fn alert_labels_keep_their_layout_without_consuming_heading_destinations() {
        use crate::pdf::{Faces, FlowKind, PageGeom, heading_metadata, layout};

        let doc = crate::parse_markdown(
            "> [!NOTE]\n> Alert body.\n\n# Note\n\n## Child\n\n> [!WARNING]\n> Caution.\n\n# Warning\n\n# Note\n",
        );
        let options = crate::PdfOptions::default();
        let faces = Faces::load(&options).unwrap();
        let page = PageGeom::from_theme(&options.theme);
        let lines = layout(&doc.blocks, &options, &faces, page);
        let metadata = heading_metadata(&lines);
        assert_eq!(metadata.len(), 4);
        assert_eq!(
            metadata
                .values()
                .map(|heading| (heading.id.as_str(), heading.level))
                .collect::<Vec<_>>(),
            [("note", 1), ("child", 2), ("warning", 1), ("note-2", 1)]
        );
        let labels: Vec<_> = lines
            .iter()
            .filter(|line| line.flow.kind == FlowKind::Heading && line.heading_source.is_none())
            .collect();
        assert_eq!(labels.len(), 2);
        assert!(
            labels
                .iter()
                .all(|label| !metadata.contains_key(&label.flow.group))
        );
        assert_eq!(
            labels[0]
                .segs
                .iter()
                .map(|seg| seg.text.as_str())
                .collect::<String>(),
            "Note"
        );
        assert_eq!(
            labels[1]
                .segs
                .iter()
                .map(|seg| seg.text.as_str())
                .collect::<String>(),
            "Warning"
        );
    }
}
