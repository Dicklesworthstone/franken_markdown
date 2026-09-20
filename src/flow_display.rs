//! Synchronous-resumable, AST-backed flow display engine (FCB-074.A).
//!
//! The shared Markdown parser runs once, on the first step. It resolves the
//! entire document (including forward references); subsequent steps advance a
//! source-line frontier and emit complete display blocks. Parsing itself is not
//! incremental. Both source-line consumption and block emission are bounded by
//! `batch_size`, so a large container cannot emit all of its children in one step.
//!
//! The engine performs no ambient I/O. Asset requests are data for a host to
//! authorize and resolve. Source spans are truthful enclosing top-level AST spans,
//! not invented exact inline locations. Default resource limits are enforced;
//! `try_with_limits` lets hosts choose stricter ingress and asset budgets.

#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use std::fmt;

use crate::ast::{Block, Inline, ListItem};
use crate::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayImage, DisplayItem, DisplayList,
    DisplayRect, DisplaySemanticAnchor, DisplayTextRun, DisplayVectorPath, VectorShapeType,
};
use crate::html::slug_inlines;
use crate::span::SourceSpan;

mod inline;
mod limits;
mod lists;
pub use lists::FlowListItem;
pub use inline::{FlowInlineRun, FlowInlineStyle, active_link_target};
use inline::emit_inlines;
mod reflow;
pub use limits::FlowDisplayLimits;
pub use reflow::{FlowLayoutError, FlowLayoutOptions, FlowTextRole};
use limits::Projection;

/// Strongly-typed identifier for an external asset request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetRequestId(pub u64);

impl fmt::Display for AssetRequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "asset-req-{}", self.0)
    }
}

/// An unresolved image and its conservative layout estimate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedAsset {
    pub id: AssetRequestId,
    pub kind: &'static str,
    pub reference: String,
    /// Start of the enclosing source block, not an exact inline syntax offset.
    pub source_offset: usize,
    pub generation: u64,
    pub estimated_width: u32,
    pub estimated_height: u32,
    pub alt_text: String,
}

/// A host-authorized asset request; the renderer never fetches the URL itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRequest {
    pub id: AssetRequestId,
    pub kind: &'static str,
    pub url: String,
    /// Start of the enclosing source block. See `source_span_for_block`.
    pub source_offset: usize,
    pub generation: u64,
    pub estimated_width: u32,
    pub estimated_height: u32,
    pub alt_text: String,
}

impl From<&UnresolvedAsset> for AssetRequest {
    fn from(asset: &UnresolvedAsset) -> Self {
        Self {
            id: asset.id,
            kind: asset.kind,
            url: asset.reference.clone(),
            source_offset: asset.source_offset,
            generation: asset.generation,
            estimated_width: asset.estimated_width,
            estimated_height: asset.estimated_height,
            alt_text: asset.alt_text.clone(),
        }
    }
}

/// Dimensions and optional payload supplied by the host for one generation.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetResult {
    pub request_id: AssetRequestId,
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub bytes: Option<Vec<u8>>,
}

/// An immutable semantic output block. Inline Markdown delimiters are already
/// interpreted by the shared parser; code and math content remain literal text.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayBlock {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    CodeBlock { language: Option<String>, source: String },
    ListItem { ordered: bool, text: String },
    Quote { text: String },
    Rule,
    TableHeader { cells: Vec<String> },
    TableRow { cells: Vec<String> },
    UnresolvedAsset(UnresolvedAsset),
}

impl DisplayBlock {
    /// Base slug using the HTML renderer's rules. A document's emitted anchors
    /// additionally disambiguate collisions, including literal suffixed headings.
    #[must_use]
    pub fn slug(&self) -> Option<String> {
        match self {
            Self::Heading { text, .. } => {
                let slug = slug_inlines(&[Inline::Text(text.clone())]);
                Some(if slug.is_empty() { "section".to_owned() } else { slug })
            }
            _ => None,
        }
    }
}

/// One bounded emission step. An empty `blocks` vector with `has_more = true`
/// means progress was made toward the end of an unfinished AST block.
#[derive(Clone, Debug, PartialEq)]
pub struct StepResult {
    pub blocks: Vec<DisplayBlock>,
    pub unresolved_assets: Vec<AssetRequest>,
    pub has_more: bool,
    pub total_blocks: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowDisplayError {
    BudgetExceeded(String),
    AlreadyFinished,
    CorruptCheckpoint,
    StaleAssetGeneration { expected: u64, actual: u64 },
    UnknownAssetRequest(AssetRequestId),
    InvalidSourceSpan(SourceSpan),
    InvalidAssetDimensions { width: u32, height: u32 },
}

impl fmt::Display for FlowDisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BudgetExceeded(msg) => write!(f, "flow display budget exceeded: {msg}"),
            Self::AlreadyFinished => write!(f, "flow display engine already finished"),
            Self::CorruptCheckpoint => write!(f, "corrupt flow display checkpoint"),
            Self::StaleAssetGeneration { expected, actual } => {
                write!(f, "stale asset generation: expected {expected}, actual {actual}")
            }
            Self::UnknownAssetRequest(id) => write!(f, "unknown asset request ID: {id}"),
            Self::InvalidSourceSpan(span) => write!(f, "invalid AST source span [{}, {})", span.start, span.end),
            Self::InvalidAssetDimensions { width, height } => write!(f, "invalid or over-limit asset dimensions: {width}x{height}"),
        }
    }
}

impl std::error::Error for FlowDisplayError {}

#[derive(Clone, Debug)]
struct SourceLine {
    start_offset: usize,
    end_offset: usize,
}

#[derive(Clone, Debug, Default)]
struct BlockMeta {
    span: SourceSpan,
    heading_id: Option<String>,
    list_depth: u16,
    // One shared immutable allocation per AST item, not a path copy per output.
    list_path: std::sync::Arc<[FlowListItem]>,
    quote_depth: u16,
    marker: Option<String>,
    ordered: bool,
    task: Option<bool>,
    inline_runs: Vec<FlowInlineRun>,
    cell_runs: Vec<Vec<FlowInlineRun>>,
    image_link: Option<std::sync::Arc<str>>,
}

#[derive(Debug)]
struct PreparedBlock {
    block: DisplayBlock,
    meta: BlockMeta,
}

/// Resumable display emission over the same AST used by HTML and PDF.
///
/// Initialization parses the whole source once. `step` then consumes at most
/// `batch_size` source lines and emits at most `batch_size` completed display
/// blocks. A nested container may require additional steps after source EOF.
/// Resume changes neither block identity nor asset IDs nor heading anchors.
#[derive(Debug)]
pub struct ResumableFlowDisplay {
    source: String,
    lines: VecDeque<SourceLine>,
    current_line: usize,
    source_frontier: usize,
    prepared: Option<VecDeque<PreparedBlock>>,
    blocks: Vec<DisplayBlock>,
    metadata: Vec<BlockMeta>,
    pending_assets: Vec<AssetRequest>,
    resolved_assets: Vec<AssetResult>,
    finished: bool,
    batch_size: usize,
    generation: u64,
    limits: FlowDisplayLimits,
    initial_error: Option<FlowDisplayError>,
    retained_asset_bytes: usize,
}

impl ResumableFlowDisplay {
    #[must_use]
    pub fn new(source: &str, batch_size: usize) -> Self {
        Self::with_generation(source, batch_size, 1)
    }

    /// Use default admission limits. An oversized document is not retained;
    /// `step` returns its admission error. Use `try_with_limits` to receive
    /// ingress errors immediately, before constructing an engine.
    #[must_use]
    pub fn with_generation(source: &str, batch_size: usize, generation: u64) -> Self {
        let limits = FlowDisplayLimits::default();
        match Self::try_with_limits(source, batch_size, generation, limits) {
            Ok(engine) => engine,
            Err(error) => {
                let mut engine = Self::empty(batch_size, generation, limits);
                engine.initial_error = Some(error);
                engine
            }
        }
    }

    fn empty(batch_size: usize, generation: u64, limits: FlowDisplayLimits) -> Self {
        Self {
            source: String::new(),
            lines: VecDeque::new(),
            current_line: 0,
            source_frontier: 0,
            prepared: None,
            blocks: Vec::new(),
            metadata: Vec::new(),
            pending_assets: Vec::new(),
            resolved_assets: Vec::new(),
            finished: false,
            batch_size: batch_size.max(1),
            generation,
            limits,
            initial_error: None,
            retained_asset_bytes: 0,
        }
    }

    /// Retained source, or an empty string when default ingress was rejected.
    #[must_use]
    pub fn source(&self) -> &str { &self.source }

    #[must_use]
    pub const fn generation(&self) -> u64 { self.generation }

    #[must_use]
    pub const fn batch_size(&self) -> usize { self.batch_size }

    #[must_use]
    pub fn unresolved_assets(&self) -> &[AssetRequest] { &self.pending_assets }

    #[must_use]
    pub fn resolved_assets(&self) -> &[AssetResult] { &self.resolved_assets }

    #[must_use]
    pub fn blocks(&self) -> &[DisplayBlock] { &self.blocks }

    #[must_use]
    pub const fn is_finished(&self) -> bool { self.finished }

    /// Truthful enclosing top-level AST range. Nested children can share a
    /// range; this must not be presented as an exact inline selection mapping.
    #[must_use]
    pub fn source_span_for_block(&self, index: usize) -> Option<SourceSpan> {
        self.metadata.get(index).map(|meta| meta.span)
    }

    /// Reflow the same document in a new generation. Invalidate previous asset
    /// payloads and reissue all produced requests with stable request IDs.
    pub fn set_generation(&mut self, generation: u64) {
        if generation == self.generation { return; }
        self.generation = generation;
        self.resolved_assets.clear();
        self.retained_asset_bytes = 0;
        self.pending_assets.clear();
        for block in &mut self.blocks {
            if let DisplayBlock::UnresolvedAsset(asset) = block {
                asset.generation = generation;
                self.pending_assets.push(AssetRequest::from(&*asset));
            }
        }
    }

    #[must_use]
    pub fn is_asset_resolved(&self, id: AssetRequestId) -> bool {
        self.resolved_assets.iter().any(|result| {
            result.request_id == id && result.generation == self.generation
        })
    }

    pub fn provide_asset(&mut self, result: AssetResult) -> Result<(), FlowDisplayError> {
        if result.generation != self.generation {
            return Err(FlowDisplayError::StaleAssetGeneration {
                expected: self.generation, actual: result.generation,
            });
        }
        let index = self.pending_assets.iter()
            .position(|request| request.id == result.request_id)
            .ok_or(FlowDisplayError::UnknownAssetRequest(result.request_id))?;
        let retained = self.limits.check_asset(&result, self.retained_asset_bytes)?;
        self.pending_assets.remove(index);
        self.resolved_assets.push(result);
        self.retained_asset_bytes = retained;
        Ok(())
    }

    /// Advance the source frontier and emit completed blocks. The first call
    /// runs the shared whole-document parser; no per-line Markdown parser exists.
    pub fn step(&mut self) -> Result<Option<StepResult>, FlowDisplayError> {
        if let Some(error) = &self.initial_error { return Err(error.clone()); }
        if self.finished { return Ok(None); }
        if self.prepared.is_none() {
            match prepare_document(&self.source, &self.limits) {
                Ok(prepared) => self.prepared = Some(prepared),
                Err(error) => {
                    // Publish nothing and do not repeat an expensive failed
                    // preparation on the next step.
                    self.initial_error = Some(error.clone());
                    return Err(error);
                }
            }
        }
        for _ in 0..self.batch_size {
            let Some(line) = self.lines.pop_front() else { break; };
            debug_assert_eq!(line.start_offset, self.source_frontier);
            self.source_frontier = line.end_offset;
            self.current_line += 1;
        }
        let mut blocks = Vec::new();
        let mut requests = Vec::new();
        if let Some(prepared) = self.prepared.as_mut() {
            while blocks.len() < self.batch_size {
                let ready = prepared.front().is_some_and(|item| {
                    item.meta.span.end <= self.source_frontier
                });
                if !ready { break; }
                let Some(mut item) = prepared.pop_front() else { break; };
                if let DisplayBlock::UnresolvedAsset(asset) = &mut item.block {
                    // Preparation may precede a generation change. Never issue
                    // an old-generation request when its block finally appears.
                    asset.generation = self.generation;
                    let request = AssetRequest::from(&*asset);
                    self.pending_assets.push(request.clone());
                    requests.push(request);
                }
                self.metadata.push(item.meta);
                self.blocks.push(item.block.clone());
                blocks.push(item.block);
            }
            self.finished = self.lines.is_empty() && prepared.is_empty();
        }
        if blocks.is_empty() && self.finished { return Ok(None); }
        Ok(Some(StepResult {
            blocks,
            unresolved_assets: requests,
            has_more: !self.finished,
            total_blocks: self.blocks.len(),
        }))
    }

    /// Finish processing and return only the blocks emitted by this call.
    pub fn process_all(&mut self) -> Result<Vec<DisplayBlock>, FlowDisplayError> {
        let mut blocks = Vec::new();
        while let Some(step) = self.step()? { blocks.extend(step.blocks); }
        Ok(blocks)
    }

    /// Produce an immutable, renderer-neutral display list at the default width.
    /// Text uses logical estimates here, not PDF-quality glyph shaping.
    #[must_use]
    pub fn to_display_list(&self) -> DisplayList {
        let mut list = DisplayList::new();
        let mut y = 0.0;
        let resolved: HashMap<_, _> = self.resolved_assets.iter()
            .filter(|result| result.generation == self.generation)
            .map(|result| (result.request_id, result)).collect();
        for (block, meta) in self.blocks.iter().zip(&self.metadata) {
            let x = f32::from(meta.list_depth) * 20.0 + f32::from(meta.quote_depth) * 16.0;
            let width = (800.0 - x).max(1.0);
            let span = meta.span;
            match block {
                DisplayBlock::Heading { level, text } => {
                    let size = match level { 1 => 28.0, 2 => 22.0, 3 => 18.0, _ => 16.0 };
                    let bounds = DisplayRect::new(x, y, width, size * 1.5);
                    push_text(&mut list, text, bounds, size, "heading", span);
                    if let Some(id) = &meta.heading_id {
                        list.push_item(DisplayItem::Anchor(DisplaySemanticAnchor {
                            bounds, anchor_id: id.clone(), is_heading: true, level: *level,
                            source_span: span,
                        }));
                    }
                    push_reading(&mut list, AccessibleReadingRole::Heading { level: *level }, text, bounds, span);
                    y += bounds.height + 10.0;
                }
                DisplayBlock::Paragraph { text } | DisplayBlock::ListItem { text, .. }
                | DisplayBlock::Quote { text } => {
                    let bounds = DisplayRect::new(x, y, width, text.lines().count().max(1) as f32 * 20.0);
                    if meta.quote_depth > 0 {
                        push_vector(&mut list, DisplayRect::new(x - 12.0, y, 4.0, bounds.height),
                            VectorShapeType::CalloutAccentBar, "accent", span);
                    }
                    if let Some(marker) = &meta.marker {
                        let marker_bounds = DisplayRect::new((x - 20.0).max(0.0), y, 18.0, 20.0);
                        if let Some(checked) = meta.task {
                            push_vector(&mut list, marker_bounds, VectorShapeType::CheckboxOutline, "border", span);
                            if checked {
                                push_vector(&mut list, marker_bounds, VectorShapeType::CheckboxCheck, "accent", span);
                            }
                        } else {
                            push_text(&mut list, marker, marker_bounds, 14.0, "text", span);
                        }
                    }
                    push_text(&mut list, text, bounds, 14.0, "text", span);
                    let role = match block {
                        DisplayBlock::ListItem { .. } => AccessibleReadingRole::ListItem,
                        DisplayBlock::Quote { .. } => AccessibleReadingRole::BlockQuote,
                        _ => AccessibleReadingRole::Paragraph,
                    };
                    let reading_text = match meta.task {
                        Some(true) => format!("[x] {text}"),
                        Some(false) => format!("[ ] {text}"),
                        None => text.clone(),
                    };
                    push_reading(&mut list, role, &reading_text, bounds, span);
                    y += bounds.height + if meta.list_depth > 0 { 4.0 } else { 8.0 };
                }
                DisplayBlock::CodeBlock { source, .. } => {
                    let bounds = DisplayRect::new(x, y, width, source.lines().count().max(1) as f32 * 20.0);
                    push_text(&mut list, source, bounds, 13.0, "code", span);
                    push_reading(&mut list, AccessibleReadingRole::CodeBlock, source, bounds, span);
                    y += bounds.height + 12.0;
                }
                DisplayBlock::Rule => {
                    let bounds = DisplayRect::new(x, y, width, 2.0);
                    push_vector(&mut list, bounds, VectorShapeType::HorizontalRule, "border", span);
                    push_reading(&mut list, AccessibleReadingRole::ThematicBreak, "", bounds, span);
                    y += 16.0;
                }
                DisplayBlock::TableHeader { cells } | DisplayBlock::TableRow { cells } => {
                    let bounds = DisplayRect::new(x, y, width, 20.0);
                    push_vector(&mut list, bounds, VectorShapeType::TableBorder, "table-border", span);
                    let header = matches!(block, DisplayBlock::TableHeader { .. });
                    let cell_width = width / cells.len().max(1) as f32;
                    let mut children = Vec::new();
                    for (index, cell) in cells.iter().enumerate() {
                        let cell_bounds = DisplayRect::new(x + index as f32 * cell_width, y, cell_width, 20.0);
                        push_text(&mut list, cell, cell_bounds, 14.0, if header { "heading" } else { "text" }, span);
                        children.push(AccessibleReadingNode {
                            role: if header { AccessibleReadingRole::TableHeaderCell } else { AccessibleReadingRole::TableCell },
                            text: cell.clone(), source_span: span, bounds: cell_bounds, children: Vec::new(),
                        });
                    }
                    list.push_reading_node(AccessibleReadingNode {
                        role: if header { AccessibleReadingRole::TableHeaderRow } else { AccessibleReadingRole::TableRow },
                        text: cells.join(" | "), source_span: span, bounds, children,
                    });
                    y += 24.0;
                }
                DisplayBlock::UnresolvedAsset(asset) => {
                    let result = resolved.get(&asset.id);
                    let bounds = DisplayRect::new(x, y,
                        result.map_or(asset.estimated_width, |r| r.width) as f32,
                        result.map_or(asset.estimated_height, |r| r.height) as f32);
                    list.push_item(DisplayItem::Image(DisplayImage {
                        bounds, request_id: asset.id.0, destination: asset.reference.clone(),
                        alt_text: asset.alt_text.clone(), is_resolved: result.is_some(), source_span: span,
                    }));
                    push_reading(&mut list, AccessibleReadingRole::Image, &asset.alt_text, bounds, span);
                    y += bounds.height + 10.0;
                }
            }
        }
        list
    }
}

fn push_text(list: &mut DisplayList, text: &str, bounds: DisplayRect, size: f32, role: &str, span: SourceSpan) {
    list.push_item(DisplayItem::Text(DisplayTextRun {
        bounds, text: text.to_owned(), font_run: None, color_role: role.to_owned(),
        source_span: span, font_size: size,
    }));
}

fn push_vector(list: &mut DisplayList, bounds: DisplayRect, shape: VectorShapeType, role: &str, span: SourceSpan) {
    list.push_item(DisplayItem::Vector(DisplayVectorPath {
        bounds, shape, stroke_width: 2.0, color_role: role.to_owned(), source_span: span,
    }));
}

fn push_reading(list: &mut DisplayList, role: AccessibleReadingRole, text: &str, bounds: DisplayRect, span: SourceSpan) {
    list.push_reading_node(AccessibleReadingNode {
        role, text: text.to_owned(), source_span: span, bounds, children: Vec::new(),
    });
}

// The worklist traverses containers and paragraph inlines iteratively.
// Shared heading/table text helpers are protected by the AST depth audit. All syntax decisions, including reference resolution and table cell
// boundaries, belong to parse_markdown_spanned, not this projection.
enum Work<'a> {
    Block(&'a Block, BlockMeta),
    Item(&'a ListItem, FlowListItem, BlockMeta),
}

fn prepare_document(source: &str, limits: &FlowDisplayLimits) -> Result<VecDeque<PreparedBlock>, FlowDisplayError> {
    let document = crate::parse_markdown_spanned(source);
    limits::audit_document(&document, source, limits)?;
    let mut work = Vec::new();
    for block in document.blocks().iter().rev() {
        work.push(Work::Block(&block.node, BlockMeta { span: block.span, ..BlockMeta::default() }));
    }
    let mut output = Projection::new(limits);
    let mut slugs = HashMap::<String, usize>::new();
    let mut next_request_id = 1;
    let mut next_list_id = 1_u64;
    let mut list_bytes = 0_usize;
    while let Some(item) = work.pop() {
        match item {
            Work::Item(item, context, mut meta) => {
                // Independently charge retained ancestry before allocating it.
                // Children/image splits share this Arc; transient parent paths
                // are conservatively charged too. Text/output budgets still apply.
                list_bytes = meta.list_path.len().checked_add(1)
                    .and_then(|n| n.checked_mul(std::mem::size_of::<FlowListItem>()))
                    .and_then(|n| list_bytes.checked_add(n))
                    .filter(|n| *n <= limits.max_output_bytes)
                    .ok_or_else(|| FlowDisplayError::BudgetExceeded("list ancestry bytes".to_owned()))?;
                let mut path = meta.list_path.to_vec();
                path.push(context);
                meta.list_path = path.into();
                meta.list_depth = meta.list_depth.saturating_add(1);
                meta.ordered = context.ordered;
                let number = context.start.saturating_add(context.item_index as u64);
                meta.marker = Some(if context.ordered { format!("{number}.") } else { "•".to_owned() });
                meta.task = item.task;
                let first_is_paragraph = matches!(item.blocks.first(), Some(Block::Paragraph(_)));
                if !first_is_paragraph {
                    emit_text(String::new(), &mut meta, &mut output)?;
                }
                for (index, block) in item.blocks.iter().enumerate().rev() {
                    let mut child = meta.clone();
                    if index != 0 { child.marker = None; child.task = None; }
                    work.push(Work::Block(block, child));
                }
            }
            Work::Block(block, mut meta) => match block {
                Block::Heading { level, inlines } => {
                    let mut base = slug_inlines(inlines);
                    if base.is_empty() { base.push_str("section"); }
                    let mut suffix = slugs.get(&base).copied().unwrap_or(1);
                    let id = loop {
                        let candidate = if suffix == 1 { base.clone() } else { format!("{base}-{suffix}") };
                        suffix += 1;
                        if !slugs.contains_key(&candidate) { break candidate; }
                    };
                    slugs.insert(id.clone(), 1);
                    slugs.insert(base, suffix);
                    meta.heading_id = Some(id);
                    let (text, runs) = inline::collect(inlines, output.remaining_bytes())?;
                    meta.inline_runs = runs;
                    output.push_back(PreparedBlock {
                        block: DisplayBlock::Heading { level: *level, text }, meta,
                    })?;
                }
                Block::Paragraph(inlines) => {
                    emit_inlines(inlines, &mut meta, &mut output, &mut next_request_id)?;
                }
                Block::CodeBlock { lang, code } => output.push_back(PreparedBlock {
                    block: DisplayBlock::CodeBlock { language: lang.clone(), source: code.clone() }, meta,
                })?,
                Block::BlockQuote(children) => {
                    meta.quote_depth = meta.quote_depth.saturating_add(1);
                    for child in children.iter().rev() { work.push(Work::Block(child, meta.clone())); }
                }
                Block::List(list) => {
                    let list_id = next_list_id;
                    next_list_id = next_list_id.checked_add(1)
                        .ok_or_else(|| FlowDisplayError::BudgetExceeded("list identities".to_owned()))?;
                    for (index, item) in list.items.iter().enumerate().rev() {
                        work.push(Work::Item(item, FlowListItem {
                            list_id, ordered: list.ordered, start: list.start,
                            item_index: index, task: item.task,
                        }, meta.clone()));
                    }
                }
                Block::Table(table) => {
                    for (index, row) in std::iter::once(&table.head).chain(table.rows.iter()).enumerate() {
                        let mut cells = Vec::new();
                        let mut row_meta = meta.clone();
                        let mut remaining = output.remaining_bytes();
                        for cell in row {
                            let (text, runs) = inline::collect(cell, remaining)?;
                            remaining = remaining.saturating_sub(text.len());
                            for run in &runs {
                                remaining = remaining.saturating_sub(run.link.as_ref().map_or(0, |link| link.len()));
                            }
                            cells.push(text);
                            row_meta.cell_runs.push(runs);
                        }
                        let block = if index == 0 { DisplayBlock::TableHeader { cells } }
                            else { DisplayBlock::TableRow { cells } };
                        output.push_back(PreparedBlock { block, meta: row_meta })?;
                    }
                }
                Block::ThematicBreak | Block::PageBreak => output.push_back(PreparedBlock { block: DisplayBlock::Rule, meta })?,
                Block::HtmlBlock(text) | Block::MathBlock(text) => {
                    emit_text(text.clone(), &mut meta, &mut output)?;
                }
                Block::FootnoteDefinition { id, blocks } => {
                    emit_text(format!("[^{id}]:"), &mut meta, &mut output)?;
                    for child in blocks.iter().rev() { work.push(Work::Block(child, meta.clone())); }
                }
                Block::DefinitionList(items) => {
                    for item in items {
                        for term in &item.terms { emit_inlines(term, &mut meta, &mut output, &mut next_request_id)?; }
                        for definition in &item.definitions {
                            let mut child = meta.clone();
                            child.list_depth = child.list_depth.saturating_add(1);
                            emit_inlines(definition, &mut child, &mut output, &mut next_request_id)?;
                        }
                    }
                }
            },
        }
    }
    Ok(output.into_items())
}

fn emit_text(text: String, meta: &mut BlockMeta, output: &mut Projection<'_>) -> Result<(), FlowDisplayError> {
    if text.trim().is_empty() && meta.marker.is_none() {
        meta.inline_runs.clear();
        return Ok(());
    }
    let block = if meta.marker.is_some() {
        DisplayBlock::ListItem { ordered: meta.ordered, text }
    } else if meta.quote_depth > 0 {
        DisplayBlock::Quote { text }
    } else {
        DisplayBlock::Paragraph { text }
    };
    output.push_back(PreparedBlock { block, meta: meta.clone() })?;
    meta.marker = None;
    meta.task = None;
    meta.inline_runs.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn fences_preserve_literal_content_across_every_step_boundary() {
        let cases = [
            ("```rust\nfn main() {}\n```", Some("rust"), "fn main() {}\n"),
            ("~~~text\n# literal\n![literal](private.png)\n~~~", Some("text"), "# literal\n![literal](private.png)\n"),
            ("````text\n```\n~~~\n`````", Some("text"), "```\n~~~\n"),
            ("  ```text\n\talpha\n b\n  ```", Some("text"), "  alpha\nb\n"),
            ("```text\nunterminated", Some("text"), "unterminated\n"),
            ("~~~\n\n\n~~~", None, "\n\n"),
            ("```\n  ``` trailing\nend\n```", None, "  ``` trailing\nend\n"),
            ("```", None, ""),
            ("~~~lang extra words\nbody\n~~~~", Some("lang"), "body\n"),
        ];
        for (source, language, code) in cases {
            for batch in [1, 2, 3, 4, 7, usize::MAX] {
                let mut engine = ResumableFlowDisplay::new(source, batch);
                assert_eq!(engine.process_all().unwrap(), vec![DisplayBlock::CodeBlock {
                    language: language.map(str::to_owned), source: code.to_owned(),
                }], "source={source:?}, batch={batch}");
                assert!(engine.unresolved_assets().is_empty());
                assert!(engine.is_finished());
                assert!(engine.step().unwrap().is_none());
            }
        }
    }

    #[test]
    fn fence_buffering_yields_without_exceeding_line_budget() {
        let mut engine = ResumableFlowDisplay::new("```text\nfirst\nsecond\n```", 1);
        for expected_line in 1..=3 {
            let step = engine.step().unwrap().unwrap();
            assert!(step.blocks.is_empty());
            assert!(step.has_more);
            assert_eq!(engine.current_line, expected_line);
        }
        let last = engine.step().unwrap().unwrap();
        assert_eq!(last.blocks.len(), 1);
        assert!(!last.has_more);
        assert!(engine.is_finished());
    }

    #[test]
    fn invalid_fences_and_single_pipe_follow_shared_parser_semantics() {
        for source in ["|", "```info`", "~~"] {
            let mut engine = ResumableFlowDisplay::new(source, 1);
            assert_eq!(engine.process_all().unwrap(), vec![DisplayBlock::Paragraph {
                text: source.to_owned(),
            }]);
        }
        // Four columns of indentation are CODE, not ordinary prose. The old
        // secondary parser's Paragraph expectation was contrary to the core.
        for source in ["    ```rust", "\t~~~"] {
            let mut engine = ResumableFlowDisplay::new(source, 1);
            assert!(matches!(&engine.process_all().unwrap()[0], DisplayBlock::CodeBlock { language: None, .. }));
        }
    }

    #[test]
    fn source_line_offsets_count_empty_lines_unicode_and_crlf() {
        let source = "é\r\n\r\né\r\n![image](x.png)\r\n";
        let mut engine = ResumableFlowDisplay::new(source, 1);
        let starts: Vec<_> = engine.lines.iter().map(|line| line.start_offset).collect();
        assert_eq!(starts, vec![0, 4, 6, 10]);
        engine.process_all().unwrap();
        let index = engine.blocks().iter().position(|block| matches!(block, DisplayBlock::UnresolvedAsset(_))).unwrap();
        let span = engine.source_span_for_block(index).unwrap();
        assert!(span.contains(10));
        assert_eq!(engine.unresolved_assets()[0].source_offset, span.start);
        assert!(span.slice(source).unwrap().contains("![image](x.png)"));
    }

    #[test]
    fn resolved_image_dimensions_reflow_following_content() {
        let mut engine = ResumableFlowDisplay::new("![image](x.png)\nafter", 8);
        engine.process_all().unwrap();
        let id = engine.unresolved_assets()[0].id;
        let before = engine.to_display_list();
        assert_eq!(before.reading_order()[1].bounds.y, 250.0);
        engine.provide_asset(AssetResult {
            request_id: id, generation: 1, width: 640, height: 480, bytes: Some(vec![1]),
        }).unwrap();
        let after = engine.to_display_list();
        let DisplayItem::Image(image) = &after.items()[0] else { panic!("expected image"); };
        assert!(image.is_resolved);
        assert_eq!(image.bounds, DisplayRect::new(0.0, 0.0, 640.0, 480.0));
        assert_eq!(after.reading_order()[1].bounds.y, 490.0);
        assert_eq!(before.reading_order()[1].bounds.y, 250.0);
    }

    #[test]
    fn generation_change_invalidates_and_reissues_resolved_assets() {
        let mut engine = ResumableFlowDisplay::new("![one](a.png)\n![two](b.png)", 8);
        engine.process_all().unwrap();
        let id = engine.unresolved_assets()[0].id;
        let result = AssetResult {
            request_id: id, generation: 1, width: 640, height: 480, bytes: Some(vec![1]),
        };
        engine.provide_asset(result.clone()).unwrap();
        engine.set_generation(1);
        assert!(engine.is_asset_resolved(id));
        assert_eq!(engine.unresolved_assets().len(), 1);
        engine.set_generation(2);
        assert!(!engine.is_asset_resolved(id));
        assert!(engine.resolved_assets().is_empty());
        assert_eq!(engine.unresolved_assets().len(), 2);
        assert!(engine.unresolved_assets().iter().all(|req| req.generation == 2));
        assert_eq!(engine.provide_asset(result.clone()), Err(FlowDisplayError::StaleAssetGeneration {
            expected: 2, actual: 1,
        }));
        assert_eq!(engine.unresolved_assets().len(), 2);
        engine.provide_asset(AssetResult { generation: 2, ..result }).unwrap();
        assert!(engine.is_asset_resolved(id));
        assert_eq!(engine.unresolved_assets().len(), 1);
    }

    #[test]
    fn paragraphs_setext_and_forward_references_use_the_shared_ast() {
        let source = "Title\n=====\n\nA **bold** [reference][r]\ncontinues.\n\n[r]: https://example.com\n";
        for batch in [1, 2, 8, usize::MAX] {
            let mut engine = ResumableFlowDisplay::new(source, batch);
            assert_eq!(engine.process_all().unwrap(), vec![
                DisplayBlock::Heading { level: 1, text: "Title".to_owned() },
                DisplayBlock::Paragraph { text: "A bold reference continues.".to_owned() },
            ]);
        }
    }

    #[test]
    fn tables_have_real_headers_and_are_not_inferred_from_lone_pipe_lines() {
        let source = "| Name | Value |\n| --- | --- |\n| **row** | `code` |\n";
        let mut engine = ResumableFlowDisplay::new(source, 1);
        assert_eq!(engine.process_all().unwrap(), vec![
            DisplayBlock::TableHeader { cells: vec!["Name".to_owned(), "Value".to_owned()] },
            DisplayBlock::TableRow { cells: vec!["row".to_owned(), "code".to_owned()] },
        ]);
        let list = engine.to_display_list();
        assert_eq!(list.reading_order()[0].role, AccessibleReadingRole::TableHeaderRow);
        assert_eq!(list.reading_order()[0].children[0].role, AccessibleReadingRole::TableHeaderCell);
        assert_eq!(list.reading_order()[1].children[1].text, "code");
        let mut prose = ResumableFlowDisplay::new("| not | a table |", 1);
        assert!(matches!(&prose.process_all().unwrap()[0], DisplayBlock::Paragraph { .. }));
    }

    #[test]
    fn nested_lists_keep_start_numbers_tasks_and_code_semantics() {
        let source = "3. first\n   - [x] child\n\n   ```text\n   # literal\n   ```\n4. last\n";
        let mut engine = ResumableFlowDisplay::new(source, 1);
        engine.process_all().unwrap();
        let list = engine.to_display_list();
        assert!(list.items().iter().any(|item| matches!(item, DisplayItem::Text(run) if run.text == "3.")));
        assert!(list.items().iter().any(|item| matches!(item, DisplayItem::Text(run) if run.text == "4.")));
        assert!(list.items().iter().any(|item| matches!(item, DisplayItem::Vector(path) if path.shape == VectorShapeType::CheckboxCheck)));
        assert!(engine.blocks().iter().any(|block| matches!(block, DisplayBlock::CodeBlock { source, .. } if source == "# literal\n")));
        assert!(list.reading_order().iter().any(|node| node.text.contains("child") && node.bounds.x >= 40.0));
    }

    #[test]
    fn inline_and_reference_images_are_requests_but_code_is_not() {
        let source = "before ![caption][picture] after\n\n`![literal](never.png)`\n\n[picture]: actual.png\n";
        let mut engine = ResumableFlowDisplay::new(source, 1);
        engine.process_all().unwrap();
        assert_eq!(engine.unresolved_assets().len(), 1);
        assert_eq!(engine.unresolved_assets()[0].url, "actual.png");
        assert_eq!(engine.unresolved_assets()[0].alt_text, "caption");
        assert!(engine.blocks().iter().any(|block| matches!(block, DisplayBlock::Paragraph { text } if text.contains("![literal](never.png)"))));
    }

    #[test]
    fn complete_container_emission_is_bounded_independently_of_source_lines() {
        let source = (0..100).map(|n| format!("- item {n}\n")).collect::<String>();
        let mut engine = ResumableFlowDisplay::new(&source, 1);
        let mut count = 0;
        while let Some(step) = engine.step().unwrap() {
            assert!(step.blocks.len() <= 1);
            count += 1;
            assert!(count <= 200, "source plus output work bounds the number of steps");
        }
        assert_eq!(engine.blocks().len(), 100);
        assert!(engine.is_finished());
    }

    #[test]
    fn heading_collisions_and_all_display_spans_are_deterministic() {
        let source = "## Repeat\n\n## Repeat-2\n\n## Repeat\n\n> quote\n\n![pic](x.png)\n";
        let mut whole = ResumableFlowDisplay::new(source, usize::MAX);
        whole.process_all().unwrap();
        let expected = whole.to_display_list();
        let ids: Vec<_> = expected.anchors().map(|anchor| anchor.anchor_id.as_str()).collect();
        assert_eq!(ids, vec!["repeat", "repeat-2", "repeat-3"]);
        for item in expected.items() {
            assert!(!item.source_span().is_empty());
            assert!(item.source_span().slice(source).is_some());
        }
        for batch in [1, 2, 3, 5] {
            let mut stepped = ResumableFlowDisplay::new(source, batch);
            stepped.process_all().unwrap();
            assert_eq!(stepped.to_display_list(), expected);
            assert_eq!(stepped.blocks(), whole.blocks());
        }
    }
}
