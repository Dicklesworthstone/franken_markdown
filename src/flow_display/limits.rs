//! Resource admission for the AST-backed flow engine. Limits are checked before
//! source retention, before AST projection, before each queued output, and before
//! accepting a host payload. A rejected preparation publishes no partial output.

use super::{
    AssetResult, BlockMeta, DisplayBlock, FlowDisplayError, PreparedBlock,
    ResumableFlowDisplay, SourceLine,
};
use crate::ast::{Block, Inline};
use crate::span::SpannedDocument;
use std::collections::VecDeque;

/// Explicit limits for untrusted Markdown and host-supplied image payloads.
///
/// Source limits run BEFORE copying or parsing. AST limits run after the shared
/// parser has built its bounded-source AST, but before display projection. These
/// are resource admission limits, not a claim that parsing is incremental or has
/// a wall-clock deadline. Output byte limits measure retained string contents;
/// node/count limits separately bound structural overhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowDisplayLimits {
    pub max_source_bytes: usize,
    pub max_source_lines: usize,
    pub max_ast_nodes: usize,
    pub max_nesting_depth: usize,
    pub max_output_blocks: usize,
    pub max_output_bytes: usize,
    pub max_asset_requests: usize,
    pub max_asset_bytes: usize,
    pub max_retained_asset_bytes: usize,
    pub max_image_dimension: u32,
}

impl Default for FlowDisplayLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 32 * 1024 * 1024,
            max_source_lines: 200_000,
            max_ast_nodes: 500_000,
            max_nesting_depth: 128,
            max_output_blocks: 200_000,
            max_output_bytes: 64 * 1024 * 1024,
            max_asset_requests: 8192,
            max_asset_bytes: 16 * 1024 * 1024,
            max_retained_asset_bytes: 64 * 1024 * 1024,
            max_image_dimension: 32_768,
        }
    }
}

fn exceeded(name: &str, actual: usize, maximum: usize) -> FlowDisplayError {
    FlowDisplayError::BudgetExceeded(format!("{name}: {actual} exceeds limit {maximum}"))
}

fn charge(total: &mut usize, amount: usize, maximum: usize, name: &str) -> Result<(), FlowDisplayError> {
    let next = total.checked_add(amount)
        .ok_or_else(|| FlowDisplayError::BudgetExceeded(format!("{name}: counter overflow")))?;
    if next > maximum {
        return Err(exceeded(name, next, maximum));
    }
    *total = next;
    Ok(())
}

impl FlowDisplayLimits {
    fn check_source(&self, source: &str) -> Result<(), FlowDisplayError> {
        if source.len() > self.max_source_bytes {
            return Err(exceeded("source bytes", source.len(), self.max_source_bytes));
        }
        let count = source.split_inclusive('\n')
            .take(self.max_source_lines.saturating_add(1)).count();
        if count > self.max_source_lines {
            return Err(exceeded("source lines", count, self.max_source_lines));
        }
        Ok(())
    }

    pub(super) fn check_asset(&self, result: &AssetResult, retained: usize) -> Result<usize, FlowDisplayError> {
        if result.width == 0 || result.height == 0
            || result.width > self.max_image_dimension || result.height > self.max_image_dimension
        {
            return Err(FlowDisplayError::InvalidAssetDimensions {
                width: result.width, height: result.height,
            });
        }
        let bytes = result.bytes.as_ref().map_or(0, Vec::len);
        if bytes > self.max_asset_bytes {
            return Err(exceeded("asset bytes", bytes, self.max_asset_bytes));
        }
        let mut next = retained;
        charge(&mut next, bytes, self.max_retained_asset_bytes, "retained asset bytes")?;
        Ok(next)
    }
}

impl ResumableFlowDisplay {
    /// Admit a document before copying it. Whole-document parsing is deferred to
    /// the first step, where AST/output limits can still return an error.
    pub fn try_with_limits(
        source: &str,
        batch_size: usize,
        generation: u64,
        limits: FlowDisplayLimits,
    ) -> Result<Self, FlowDisplayError> {
        limits.check_source(source)?;
        let mut engine = Self::empty(batch_size, generation, limits);
        engine.source = source.to_owned();
        let mut offset = 0;
        for segment in source.split_inclusive('\n') {
            let end_offset = offset + segment.len();
            engine.lines.push_back(SourceLine { start_offset: offset, end_offset });
            offset = end_offset;
        }
        Ok(engine)
    }

    #[must_use]
    pub const fn limits(&self) -> FlowDisplayLimits {
        self.limits
    }

    /// Bytes retained from accepted host payloads in the current generation.
    #[must_use]
    pub const fn retained_asset_bytes(&self) -> usize {
        self.retained_asset_bytes
    }
}

enum Node<'a> {
    Block(&'a Block, usize),
    Inline(&'a Inline, usize),
}

pub(super) fn audit_document(
    document: &SpannedDocument,
    source: &str,
    limits: &FlowDisplayLimits,
) -> Result<(), FlowDisplayError> {
    let mut count = 0;
    let mut stack = Vec::new();
    // Charge nodes before pushing them, so the audit's own worklist is bounded.
    for block in document.blocks().iter().rev() {
        if block.span.slice(source).is_none() {
            return Err(FlowDisplayError::InvalidSourceSpan(block.span));
        }
        charge(&mut count, 1, limits.max_ast_nodes, "AST nodes")?;
        stack.push(Node::Block(&block.node, 0));
    }
    while let Some(node) = stack.pop() {
        let depth = match &node { Node::Block(_, d) | Node::Inline(_, d) => *d };
        if depth > limits.max_nesting_depth {
            return Err(exceeded("AST nesting depth", depth, limits.max_nesting_depth));
        }
        let child_depth = depth.saturating_add(1);
        match node {
            Node::Block(block, _) => match block {
                Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                    push_inlines(inlines, child_depth, &mut stack, &mut count, limits)?;
                }
                Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => {
                    push_blocks(blocks, child_depth, &mut stack, &mut count, limits)?;
                }
                Block::List(list) => {
                    for item in list.items.iter().rev() {
                        charge(&mut count, 1, limits.max_ast_nodes, "AST nodes")?;
                        push_blocks(&item.blocks, child_depth, &mut stack, &mut count, limits)?;
                    }
                }
                Block::Table(table) => {
                    for row in std::iter::once(&table.head).chain(table.rows.iter()) {
                        charge(&mut count, 1, limits.max_ast_nodes, "AST nodes")?;
                        for cell in row {
                            charge(&mut count, 1, limits.max_ast_nodes, "AST nodes")?;
                            push_inlines(cell, child_depth, &mut stack, &mut count, limits)?;
                        }
                    }
                }
                Block::DefinitionList(items) => {
                    for item in items {
                        charge(&mut count, 1, limits.max_ast_nodes, "AST nodes")?;
                        for inlines in item.terms.iter().chain(item.definitions.iter()) {
                            charge(&mut count, 1, limits.max_ast_nodes, "AST nodes")?;
                            push_inlines(inlines, child_depth, &mut stack, &mut count, limits)?;
                        }
                    }
                }
                Block::CodeBlock { .. } | Block::HtmlBlock(_) | Block::MathBlock(_)
                | Block::ThematicBreak | Block::PageBreak => {}
            },
            Node::Inline(inline, _) => match inline {
                Inline::Emphasis(children) | Inline::Strong(children) | Inline::Strikethrough(children)
                | Inline::Link { content: children, .. } => {
                    push_inlines(children, child_depth, &mut stack, &mut count, limits)?;
                }
                Inline::Text(_) | Inline::Code(_) | Inline::Html(_) | Inline::Math(_)
                | Inline::DisplayMath(_) | Inline::Image { .. } | Inline::FootnoteRef { .. }
                | Inline::SoftBreak | Inline::HardBreak => {}
            },
        }
    }
    Ok(())
}

fn push_blocks<'a>(
    blocks: &'a [Block], depth: usize, stack: &mut Vec<Node<'a>>, count: &mut usize, limits: &FlowDisplayLimits,
) -> Result<(), FlowDisplayError> {
    charge(count, blocks.len(), limits.max_ast_nodes, "AST nodes")?;
    stack.extend(blocks.iter().rev().map(|block| Node::Block(block, depth)));
    Ok(())
}

fn push_inlines<'a>(
    inlines: &'a [Inline], depth: usize, stack: &mut Vec<Node<'a>>, count: &mut usize, limits: &FlowDisplayLimits,
) -> Result<(), FlowDisplayError> {
    charge(count, inlines.len(), limits.max_ast_nodes, "AST nodes")?;
    stack.extend(inlines.iter().rev().map(|inline| Node::Inline(inline, depth)));
    Ok(())
}

/// Queue admission is transactional. No item is retained until all applicable
/// counts and string-byte limits have passed. One candidate may already own its
/// strings; the input byte limit bounds that candidate's source material.
pub(super) struct Projection<'a> {
    items: VecDeque<PreparedBlock>,
    bytes: usize,
    assets: usize,
    limits: &'a FlowDisplayLimits,
}

impl<'a> Projection<'a> {
    pub(super) fn new(limits: &'a FlowDisplayLimits) -> Self {
        Self { items: VecDeque::new(), bytes: 0, assets: 0, limits }
    }

    pub(super) fn push_back(&mut self, item: PreparedBlock) -> Result<(), FlowDisplayError> {
        let mut count = self.items.len();
        charge(&mut count, 1, self.limits.max_output_blocks, "output blocks")?;
        let mut bytes = self.bytes;
        let mut assets = self.assets;
        let mut add = |amount| charge(&mut bytes, amount, self.limits.max_output_bytes, "output bytes");
        match &item.block {
            DisplayBlock::Heading { text, .. } | DisplayBlock::Paragraph { text }
            | DisplayBlock::ListItem { text, .. } | DisplayBlock::Quote { text } => add(text.len())?,
            DisplayBlock::CodeBlock { language, source } => {
                add(source.len())?;
                add(language.as_ref().map_or(0, String::len))?;
            }
            DisplayBlock::TableHeader { cells } | DisplayBlock::TableRow { cells } => {
                for cell in cells { add(cell.len())?; }
            }
            DisplayBlock::UnresolvedAsset(asset) => {
                charge(&mut assets, 1, self.limits.max_asset_requests, "asset requests")?;
                add(asset.reference.len())?;
                add(asset.alt_text.len())?;
            }
            DisplayBlock::Rule => {}
        }
        let BlockMeta { heading_id, marker, .. } = &item.meta;
        add(heading_id.as_ref().map_or(0, String::len))?;
        add(marker.as_ref().map_or(0, String::len))?;
        self.bytes = bytes;
        self.assets = assets;
        self.items.push_back(item);
        Ok(())
    }

    pub(super) fn into_items(self) -> VecDeque<PreparedBlock> { self.items }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::flow_display::AssetRequestId;

    #[test]
    fn source_admission_checks_utf8_bytes_and_physical_lines_before_retention() {
        let tiny = FlowDisplayLimits { max_source_bytes: 1, ..FlowDisplayLimits::default() };
        assert!(ResumableFlowDisplay::try_with_limits("é", 1, 1, tiny).is_err());
        assert!(ResumableFlowDisplay::try_with_limits("x", 1, 1, tiny).is_ok());
        let one_line = FlowDisplayLimits { max_source_lines: 1, ..FlowDisplayLimits::default() };
        assert!(ResumableFlowDisplay::try_with_limits("one\n", 1, 1, one_line).is_ok());
        assert!(ResumableFlowDisplay::try_with_limits("one\n\n", 1, 1, one_line).is_err());
        let empty = FlowDisplayLimits { max_source_bytes: 0, max_source_lines: 0, ..FlowDisplayLimits::default() };
        let mut engine = ResumableFlowDisplay::try_with_limits("", 1, 1, empty).unwrap();
        assert!(engine.step().unwrap().is_none());
        assert!(engine.is_finished());
        assert!(ResumableFlowDisplay::try_with_limits("\n", 1, 1, empty).is_err());
    }

    #[test]
    fn default_constructor_keeps_no_rejected_source_and_repeats_only_the_error() {
        let source = "\n".repeat(FlowDisplayLimits::default().max_source_lines + 1);
        let mut engine = ResumableFlowDisplay::new(&source, 1);
        assert!(engine.source().is_empty());
        assert!(engine.lines.is_empty());
        let error = engine.step().unwrap_err();
        assert_eq!(engine.step().unwrap_err(), error);
        assert!(engine.prepared.is_none());
        assert!(engine.blocks().is_empty());
        assert_eq!(engine.source_frontier, 0);
    }

    #[test]
    fn ast_audit_counts_inline_nodes_and_checks_depth_and_source_boundaries() {
        let doc = crate::parse_markdown_spanned("**hello**");
        let limits = FlowDisplayLimits { max_ast_nodes: 2, ..FlowDisplayLimits::default() };
        assert!(audit_document(&doc, "**hello**", &limits).is_err());
        let limits = FlowDisplayLimits { max_nesting_depth: 1, ..FlowDisplayLimits::default() };
        assert!(audit_document(&doc, "**hello**", &limits).is_err());
        let limits = FlowDisplayLimits { max_ast_nodes: 3, max_nesting_depth: 2, ..FlowDisplayLimits::default() };
        assert!(audit_document(&doc, "**hello**", &limits).is_ok());
        assert!(matches!(audit_document(&doc, "", &limits), Err(FlowDisplayError::InvalidSourceSpan(_))));
    }

    #[test]
    fn failed_preparation_publishes_no_blocks_assets_or_progress() {
        for (source, limits) in [
            ("one\n\ntwo", FlowDisplayLimits { max_output_blocks: 1, ..FlowDisplayLimits::default() }),
            ("word", FlowDisplayLimits { max_output_bytes: 3, ..FlowDisplayLimits::default() }),
            ("![a](a.png) ![b](b.png)", FlowDisplayLimits { max_asset_requests: 1, ..FlowDisplayLimits::default() }),
            ("**bold**", FlowDisplayLimits { max_ast_nodes: 2, ..FlowDisplayLimits::default() }),
        ] {
            let mut engine = ResumableFlowDisplay::try_with_limits(source, 1, 1, limits).unwrap();
            let error = engine.step().unwrap_err();
            assert!(matches!(error, FlowDisplayError::BudgetExceeded(_)));
            assert_eq!(engine.step().unwrap_err(), error);
            assert!(engine.blocks().is_empty());
            assert!(engine.unresolved_assets().is_empty());
            assert!(engine.metadata.is_empty());
            assert_eq!(engine.current_line, 0);
            assert_eq!(engine.source_frontier, 0);
        }
        let limits = FlowDisplayLimits { max_output_blocks: 1, max_output_bytes: 4, ..FlowDisplayLimits::default() };
        let mut engine = ResumableFlowDisplay::try_with_limits("word", 1, 1, limits).unwrap();
        assert_eq!(engine.process_all().unwrap().len(), 1);
    }

    fn payload(id: AssetRequestId, bytes: usize) -> AssetResult {
        AssetResult { request_id: id, generation: 1, width: 10, height: 20, bytes: Some(vec![7; bytes]) }
    }

    #[test]
    fn rejected_asset_results_leave_requests_and_retained_bytes_unchanged() {
        let limits = FlowDisplayLimits {
            max_asset_bytes: 3, max_retained_asset_bytes: 4, max_image_dimension: 100,
            ..FlowDisplayLimits::default()
        };
        let mut engine = ResumableFlowDisplay::try_with_limits("![a](a.png) ![b](b.png)", 8, 1, limits).unwrap();
        engine.process_all().unwrap();
        let first = engine.unresolved_assets()[0].id;
        let second = engine.unresolved_assets()[1].id;
        for (width, height) in [(0, 20), (10, 0), (101, 20), (10, 101)] {
            assert!(matches!(engine.provide_asset(AssetResult { width, height, ..payload(first, 1) }),
                Err(FlowDisplayError::InvalidAssetDimensions { .. })));
            assert_eq!(engine.unresolved_assets().len(), 2);
            assert_eq!(engine.retained_asset_bytes(), 0);
        }
        assert!(engine.provide_asset(payload(first, 4)).is_err());
        assert_eq!(engine.unresolved_assets().len(), 2);
        engine.provide_asset(payload(first, 3)).unwrap();
        assert_eq!(engine.retained_asset_bytes(), 3);
        assert!(engine.provide_asset(payload(second, 2)).is_err());
        assert_eq!(engine.unresolved_assets()[0].id, second);
        assert_eq!(engine.resolved_assets().len(), 1);
        assert_eq!(engine.retained_asset_bytes(), 3);
        engine.provide_asset(payload(second, 1)).unwrap();
        assert_eq!(engine.retained_asset_bytes(), 4);
        assert!(engine.unresolved_assets().is_empty());
        assert!(matches!(engine.provide_asset(payload(first, 1)), Err(FlowDisplayError::UnknownAssetRequest(_))));
        assert_eq!(engine.retained_asset_bytes(), 4);
        engine.set_generation(2);
        assert_eq!(engine.retained_asset_bytes(), 0);
        assert_eq!(engine.unresolved_assets().len(), 2);
        engine.provide_asset(AssetResult { generation: 2, ..payload(first, 3) }).unwrap();
        assert_eq!(engine.retained_asset_bytes(), 3);
    }

    #[test]
    fn dimension_only_asset_results_do_not_consume_payload_budget() {
        let limits = FlowDisplayLimits { max_asset_bytes: 0, max_retained_asset_bytes: 0, ..FlowDisplayLimits::default() };
        let mut engine = ResumableFlowDisplay::try_with_limits("![a](a.png)", 1, 1, limits).unwrap();
        engine.process_all().unwrap();
        let id = engine.unresolved_assets()[0].id;
        engine.provide_asset(AssetResult { bytes: None, ..payload(id, 0) }).unwrap();
        assert!(engine.is_asset_resolved(id));
        assert_eq!(engine.retained_asset_bytes(), 0);
    }
}
