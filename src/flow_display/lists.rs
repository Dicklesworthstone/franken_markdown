//! Authoritative list ancestry retained during the shared AST projection.
//! Identities are snapshot-local, never stable editor IDs or source positions.

use super::ResumableFlowDisplay;

/// One enclosing list item, from outermost to innermost. Every emitted block
/// retains its ancestry, including continuation prose, code, tables and images.
/// A new item starts with a ListItem display block, possibly with empty text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowListItem {
    /// Unique within one projected document. Fence by source revision as well.
    pub list_id: u64,
    pub ordered: bool,
    /// The AST list's starting ordinal; unordered lists do not display it.
    pub start: u64,
    /// Zero-based ordinal in the AST list, not a Markdown source number.
    pub item_index: usize,
    /// Read-only task state of this item, including its continuation blocks.
    pub task: Option<bool>,
}

impl ResumableFlowDisplay {
    /// Exact parser-derived enclosing items. Some(empty) means outside a list;
    /// None means no such emitted block. Never infer hierarchy from bounds or
    /// shared top-level source spans. The returned path belongs to this snapshot.
    #[must_use]
    pub fn list_path_for_block(&self, index: usize) -> Option<&[FlowListItem]> {
        self.metadata.get(index).map(|meta| meta.list_path.as_ref())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::flow_display::{DisplayBlock, FlowDisplayLimits, FlowDisplayError};

    fn project(source: &str, batch: usize) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, batch);
        engine.process_all().unwrap();
        engine
    }

    #[test]
    fn nested_mixed_lists_preserve_start_items_and_continuations() {
        let source = "3. first\n\n   continuation\n\n   - [x] child\n\n     ```\n     code\n     ```\n\n   after child\n\n4. last\n\noutside\n";
        let engine = project(source, 1);
        let paths: Vec<_> = (0..engine.blocks().len()).map(|i| engine.list_path_for_block(i).unwrap()).collect();
        let first = paths[0][0];
        assert!(first.ordered);
        assert_eq!(first.start, 3);
        assert_eq!(first.item_index, 0);
        assert_eq!(paths[1], &[first]);
        let child = paths.iter().find(|path| path.len() == 2).unwrap();
        assert_eq!(child[0], first);
        assert!(!child[1].ordered);
        assert_eq!(child[1].task, Some(true));
        let code = engine.blocks().iter().position(|b| matches!(b, DisplayBlock::CodeBlock { .. })).unwrap();
        assert_eq!(paths[code], *child);
        assert!(paths.iter().any(|path| path.len() == 1 && path[0].list_id == first.list_id && path[0].item_index == 1));
        assert!(paths.last().unwrap().is_empty());
        assert!(engine.list_path_for_block(usize::MAX).is_none());
    }

    #[test]
    fn separate_nested_lists_do_not_merge_despite_identical_source_envelopes() {
        let engine = project("- outer\n  - one\n  + two\n- next\n", 1);
        let children: Vec<_> = (0..engine.blocks().len())
            .filter_map(|i| engine.list_path_for_block(i).filter(|p| p.len() == 2)).collect();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0][0], children[1][0]);
        assert_ne!(children[0][1].list_id, children[1][1].list_id);
        assert_eq!(children[0][1].item_index, 0);
        assert_eq!(children[1][1].item_index, 0);
    }

    #[test]
    fn code_first_items_tables_images_and_empty_items_keep_their_owner() {
        let engine = project("-\n\n- ```\n  code\n  ```\n\n- ![pic](x.png) tail\n\n  | A |\n  | --- |\n  | B |\n", 2);
        let mut previous = None;
        for (i, block) in engine.blocks().iter().enumerate() {
            let path = engine.list_path_for_block(i).unwrap();
            assert_eq!(path.len(), 1);
            let item = path[0];
            if previous != Some(item.item_index) {
                assert!(matches!(block, DisplayBlock::ListItem { .. }));
                previous = Some(item.item_index);
            }
        }
        assert!(engine.blocks().iter().any(|b| matches!(b, DisplayBlock::CodeBlock { .. })));
        assert!(engine.blocks().iter().any(|b| matches!(b, DisplayBlock::UnresolvedAsset(_))));
        assert!(engine.blocks().iter().any(|b| matches!(b, DisplayBlock::TableRow { .. })));
    }

    #[test]
    fn ancestry_is_deterministic_across_steps_and_survives_asset_reflow() {
        let source = "7. first\n   - ![pic](x.png)\n8. second\n";
        let whole = project(source, usize::MAX);
        for batch in [1, 2, 3, 8] {
            let mut stepped = project(source, batch);
            for i in 0..whole.blocks().len() {
                assert_eq!(whole.list_path_for_block(i), stepped.list_path_for_block(i));
            }
            let before: Vec<_> = (0..stepped.blocks().len()).map(|i| stepped.list_path_for_block(i).unwrap().to_vec()).collect();
            let id = stepped.unresolved_assets()[0].id;
            stepped.provide_asset(crate::flow_display::AssetResult {
                request_id: id, generation: 1, width: 42, height: 42, bytes: None,
            }).unwrap();
            stepped.set_generation(2);
            for (i, path) in before.iter().enumerate() { assert_eq!(stepped.list_path_for_block(i).unwrap(), path); }
        }
    }

    #[test]
    fn structural_retention_has_an_independent_preallocation_budget() {
        let limits = FlowDisplayLimits { max_output_bytes: 1, ..FlowDisplayLimits::default() };
        let mut engine = ResumableFlowDisplay::try_with_limits("- x", 1, 1, limits).unwrap();
        assert!(matches!(engine.process_all(), Err(FlowDisplayError::BudgetExceeded(_))));
        assert!(engine.blocks().is_empty());
        assert!(engine.unresolved_assets().is_empty());
    }
}
