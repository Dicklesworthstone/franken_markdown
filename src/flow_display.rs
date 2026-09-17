//! Synchronous-resumable flow display engine (FCB-074.A).
//!
//! Provides budget-bounded stepping over Markdown documents with typed
//! unresolved assets, generational asset invalidation, and immutable output primitives.
//! Budget exhaustion followed by resume produces the exact same output as whole-input
//! processing — verified by the oracle tests.
//!
//! The engine emits renderer-neutral display lists ([`crate::display::DisplayList`])
//! without GPU, AppKit, or ambient I/O dependencies.

#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use std::fmt;

use crate::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayImage, DisplayItem, DisplayList,
    DisplayRect, DisplaySemanticAnchor, DisplayTextRun, DisplayVectorPath, VectorShapeType,
};
use crate::span::SourceSpan;

/// Strongly-typed identifier for an unresolved external asset request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetRequestId(pub u64);

impl fmt::Display for AssetRequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "asset-req-{}", self.0)
    }
}

/// Typed reference to an external asset that hasn't been loaded yet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedAsset {
    /// Unique request identifier.
    pub id: AssetRequestId,
    /// The asset kind (image, link, include, font, etc.).
    pub kind: &'static str,
    /// The asset path or URI as it appears in the source.
    pub reference: String,
    /// Byte offset in the source document where the reference appears.
    pub source_offset: usize,
    /// Document/layout generation when this asset was requested.
    pub generation: u64,
    /// Estimated width in layout units/pixels.
    pub estimated_width: u32,
    /// Estimated height in layout units/pixels.
    pub estimated_height: u32,
    /// Alt-text description for fallback and accessibility.
    pub alt_text: String,
}

/// A request for an external asset issued by the flow engine to the host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRequest {
    /// Unique request identifier.
    pub id: AssetRequestId,
    /// The asset kind ("image", "font", etc.).
    pub kind: &'static str,
    /// Requested URL or relative destination path.
    pub url: String,
    /// Byte offset in the source document.
    pub source_offset: usize,
    /// Layout generation ID.
    pub generation: u64,
    /// Estimated width.
    pub estimated_width: u32,
    /// Estimated height.
    pub estimated_height: u32,
    /// Alt text.
    pub alt_text: String,
}

/// The result of resolving an asset, supplied by the host.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetResult {
    /// Target request identifier.
    pub request_id: AssetRequestId,
    /// Layout generation ID of the document when this result is supplied.
    pub generation: u64,
    /// Resolved width in pixels.
    pub width: u32,
    /// Resolved height in pixels.
    pub height: u32,
    /// Raw asset payload bytes if loaded.
    pub bytes: Option<Vec<u8>>,
}

/// An immutable output block produced by the flow display engine.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayBlock {
    /// A heading with its level and text.
    Heading { level: u8, text: String },
    /// A paragraph of text.
    Paragraph { text: String },
    /// A fenced code block with language and classified source.
    CodeBlock { language: Option<String>, source: String },
    /// A list item (ordered or unordered).
    ListItem { ordered: bool, text: String },
    /// A blockquote.
    Quote { text: String },
    /// A horizontal rule.
    Rule,
    /// A table header row.
    TableHeader { cells: Vec<String> },
    /// A table body row.
    TableRow { cells: Vec<String> },
    /// An asset reference that couldn't be resolved yet.
    UnresolvedAsset(UnresolvedAsset),
}

impl DisplayBlock {
    /// Return the slug identifier if this block is a heading.
    #[must_use]
    pub fn slug(&self) -> Option<String> {
        match self {
            Self::Heading { text, .. } => {
                let slug: String = text
                    .to_lowercase()
                    .replace(' ', "-")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-')
                    .collect();
                Some(slug)
            }
            _ => None,
        }
    }
}

/// The result of one bounded processing step.
#[derive(Clone, Debug, PartialEq)]
pub struct StepResult {
    /// Blocks produced by this step. May be empty while a fenced block is buffered.
    pub blocks: Vec<DisplayBlock>,
    /// Unresolved asset requests produced by this step.
    pub unresolved_assets: Vec<AssetRequest>,
    /// Whether more lines/blocks remain to be processed.
    pub has_more: bool,
    /// Total blocks produced so far across all steps.
    pub total_blocks: usize,
}

/// Errors from the flow display engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowDisplayError {
    /// A budget was exceeded during processing.
    BudgetExceeded(String),
    /// The engine was already finished.
    AlreadyFinished,
    /// A checkpoint could not be deserialized.
    CorruptCheckpoint,
    /// An asset result was provided for a stale generation.
    StaleAssetGeneration { expected: u64, actual: u64 },
    /// An asset result was provided for an unknown request ID.
    UnknownAssetRequest(AssetRequestId),
}

impl fmt::Display for FlowDisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BudgetExceeded(msg) => write!(f, "flow display budget exceeded: {msg}"),
            Self::AlreadyFinished => write!(f, "flow display engine already finished"),
            Self::CorruptCheckpoint => write!(f, "corrupt flow display checkpoint"),
            Self::StaleAssetGeneration { expected, actual } => write!(
                f,
                "stale asset generation: expected {expected}, actual {actual}"
            ),
            Self::UnknownAssetRequest(id) => write!(f, "unknown asset request ID: {id}"),
        }
    }
}

impl std::error::Error for FlowDisplayError {}

#[derive(Clone, Debug)]
struct SourceLine {
    text: String,
    start_offset: usize,
}

// State survives a step boundary: code is emitted once, only on its closing
// fence or EOF. Its body must never enter the ordinary Markdown classifier or
// create host asset requests. See CommonMark 0.31.2, section 4.5.
#[derive(Debug)]
struct FencedCode {
    marker: u8,
    fence_len: usize,
    indent: usize,
    language: Option<String>,
    source: String,
}

impl FencedCode {
    fn candidate(line: &str) -> Option<(u8, usize, usize, &str)> {
        let indent = line.bytes().take_while(|&b| b == b' ').count();
        if indent > 3 {
            return None;
        }
        let rest = &line[indent..];
        let marker = *rest.as_bytes().first()?;
        if !matches!(marker, b'`' | b'~') {
            return None;
        }
        let fence_len = rest.bytes().take_while(|&b| b == marker).count();
        if fence_len < 3 {
            return None;
        }
        Some((marker, fence_len, indent, &rest[fence_len..]))
    }

    fn open(line: &str) -> Option<Self> {
        let (marker, fence_len, indent, info) = Self::candidate(line)?;
        if marker == b'`' && info.contains('`') {
            return None;
        }
        Some(Self {
            marker,
            fence_len,
            indent,
            language: info.split_whitespace().next().map(str::to_owned),
            source: String::new(),
        })
    }

    fn is_closing(&self, line: &str) -> bool {
        Self::candidate(line).is_some_and(|(marker, fence_len, _, rest)| {
            marker == self.marker
                && fence_len >= self.fence_len
                && rest.trim_matches([' ', '\t']).is_empty()
        })
    }

    fn append_line(&mut self, line: &str) {
        let mut cursor = 0;
        let mut removed = 0;
        while removed < self.indent {
            match line.as_bytes().get(cursor) {
                Some(b' ') => {
                    cursor += 1;
                    removed += 1;
                }
                Some(b'\t') => {
                    // The opening indentation is at most three columns, so
                    // this leading tab reaches column four. Retain the part
                    // beyond the indentation instead of dropping the whole tab.
                    cursor += 1;
                    for _ in self.indent..4 {
                        self.source.push(' ');
                    }
                    break;
                }
                _ => break,
            }
        }
        self.source.push_str(&line[cursor..]);
        self.source.push('\n');
    }

    fn into_block(self) -> DisplayBlock {
        DisplayBlock::CodeBlock {
            language: self.language,
            source: self.source,
        }
    }
}

/// A synchronous-resumable flow display engine.
///
/// Processes a Markdown source document in bounded steps. Each step
/// processes a fixed number of source lines and produces display blocks.
/// When the budget is exhausted, the engine yields partial progress; the
/// caller can resume from that point and the accumulated output is
/// byte-for-byte and element-for-element identical to whole-input processing.
#[derive(Debug)]
pub struct ResumableFlowDisplay {
    source: String,
    lines: VecDeque<SourceLine>,
    current_line: usize,
    blocks: Vec<DisplayBlock>,
    pending_assets: Vec<AssetRequest>,
    resolved_assets: Vec<AssetResult>,
    open_fence: Option<FencedCode>,
    finished: bool,
    batch_size: usize,
    generation: u64,
    next_request_id: u64,
}

impl ResumableFlowDisplay {
    /// Create a new resumable flow display engine for the given source.
    ///
    /// `batch_size` controls how many source lines are processed per step.
    pub fn new(source: &str, batch_size: usize) -> Self {
        Self::with_generation(source, batch_size, 1)
    }

    /// Create a new resumable flow display engine with an explicit initial generation.
    pub fn with_generation(source: &str, batch_size: usize, generation: u64) -> Self {
        let mut lines = VecDeque::new();
        let mut offset = 0usize;
        for segment in source.split_inclusive('\n') {
            let text = segment.strip_suffix('\n').map_or(segment, |line| {
                line.strip_suffix('\r').unwrap_or(line)
            });
            lines.push_back(SourceLine {
                text: text.to_owned(),
                start_offset: offset,
            });
            // Count the actual delimiter bytes, including CRLF and empty lines.
            // Searching for repeated line contents loses these offsets.
            offset += segment.len();
        }

        Self {
            source: source.to_owned(),
            lines,
            current_line: 0,
            blocks: Vec::new(),
            pending_assets: Vec::new(),
            resolved_assets: Vec::new(),
            open_fence: None,
            finished: false,
            batch_size: batch_size.max(1),
            generation,
            next_request_id: 1,
        }
    }

    /// Borrow the original Markdown source text.
    #[inline]
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Return the current document generation.
    #[inline]
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Update the layout generation without replacing the source document.
    ///
    /// A changed generation invalidates all resolved payloads and reissues
    /// requests for produced asset blocks. Request IDs stay stable; hosts can
    /// enumerate the refreshed requests through [`Self::unresolved_assets`].
    /// Reapplying the same generation is a no-op.
    pub fn set_generation(&mut self, generation: u64) {
        if generation == self.generation {
            return;
        }
        self.generation = generation;
        self.resolved_assets.clear();
        self.pending_assets.clear();
        for block in &mut self.blocks {
            if let DisplayBlock::UnresolvedAsset(asset) = block {
                asset.generation = generation;
                self.pending_assets.push(AssetRequest {
                    id: asset.id,
                    kind: asset.kind,
                    url: asset.reference.clone(),
                    source_offset: asset.source_offset,
                    generation,
                    estimated_width: asset.estimated_width,
                    estimated_height: asset.estimated_height,
                    alt_text: asset.alt_text.clone(),
                });
            }
        }
    }

    /// Configured batch size for step iterations.
    #[inline]
    #[must_use]
    pub const fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Borrow all unresolved asset requests.
    #[inline]
    #[must_use]
    pub fn unresolved_assets(&self) -> &[AssetRequest] {
        &self.pending_assets
    }

    /// Borrow all resolved asset responses.
    #[inline]
    #[must_use]
    pub fn resolved_assets(&self) -> &[AssetResult] {
        &self.resolved_assets
    }

    /// Check if a specific asset request has been resolved.
    #[must_use]
    pub fn is_asset_resolved(&self, id: AssetRequestId) -> bool {
        self.resolved_assets
            .iter()
            .any(|r| r.request_id == id && r.generation == self.generation)
    }

    /// Supply a host asset result.
    ///
    /// # Errors
    /// Returns [`FlowDisplayError::StaleAssetGeneration`] if `result.generation`
    /// does not match the engine's current generation.
    /// Returns [`FlowDisplayError::UnknownAssetRequest`] if `result.request_id`
    /// is not currently pending.
    pub fn provide_asset(&mut self, result: AssetResult) -> Result<(), FlowDisplayError> {
        if result.generation != self.generation {
            return Err(FlowDisplayError::StaleAssetGeneration {
                expected: self.generation,
                actual: result.generation,
            });
        }
        let pos = self
            .pending_assets
            .iter()
            .position(|req| req.id == result.request_id)
            .ok_or(FlowDisplayError::UnknownAssetRequest(result.request_id))?;

        self.pending_assets.remove(pos);
        self.resolved_assets.push(result);
        Ok(())
    }

    /// Process one bounded step, processing up to `batch_size` lines.
    ///
    /// Returns `Ok(None)` when the document is fully processed. An unfinished
    /// fence can yield an empty step with `has_more` set; callers must resume.
    pub fn step(&mut self) -> Result<Option<StepResult>, FlowDisplayError> {
        if self.finished {
            return Ok(None);
        }
        let mut step_blocks = Vec::new();
        let mut step_assets = Vec::new();
        let mut lines_consumed = 0usize;

        while lines_consumed < self.batch_size {
            let Some(line) = self.lines.pop_front() else {
                break;
            };
            self.current_line += 1;
            lines_consumed += 1;

            let (block, asset_req) = self.classify_source_line(&line);
            if let Some(b) = block {
                step_blocks.push(b);
            }
            if let Some(req) = asset_req {
                self.pending_assets.push(req.clone());
                step_assets.push(req);
            }
        }

        if self.lines.is_empty() {
            // An unterminated fenced block extends to EOF. Flush it even when
            // EOF coincides exactly with the current step's line budget.
            if let Some(fence) = self.open_fence.take() {
                step_blocks.push(fence.into_block());
            }
            self.finished = true;
        }
        if step_blocks.is_empty() && self.finished {
            return Ok(None);
        }

        self.blocks.extend(step_blocks.iter().cloned());
        Ok(Some(StepResult {
            blocks: step_blocks,
            unresolved_assets: step_assets,
            has_more: !self.finished,
            total_blocks: self.blocks.len(),
        }))
    }

    /// Process the entire document in one call (for oracle comparison).
    pub fn process_all(&mut self) -> Result<Vec<DisplayBlock>, FlowDisplayError> {
        let mut all = Vec::new();
        while let Some(step) = self.step()? {
            all.extend(step.blocks);
        }
        Ok(all)
    }

    /// All blocks produced so far (including partial steps).
    #[inline]
    #[must_use]
    pub fn blocks(&self) -> &[DisplayBlock] {
        &self.blocks
    }

    /// Whether the engine has finished processing.
    #[inline]
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Materialize a renderer-neutral [`DisplayList`] from all produced blocks.
    ///
    /// Contains NO Metal texture pointers, AppKit types, or FCB window IDs.
    #[must_use]
    pub fn to_display_list(&self) -> DisplayList {
        let mut dl = DisplayList::new();
        let mut y = 0.0f32;
        let line_height = 20.0f32;
        // Lookup only: output is always traversed in source order, never hash
        // iteration order. Multiple images do not rescan all resolved results.
        let resolved: HashMap<_, _> = self
            .resolved_assets
            .iter()
            .filter(|result| result.generation == self.generation)
            .map(|result| (result.request_id, result))
            .collect();

        for block in &self.blocks {
            match block {
                DisplayBlock::Heading { level, text } => {
                    let font_size = match level {
                        1 => 28.0,
                        2 => 22.0,
                        3 => 18.0,
                        _ => 16.0,
                    };
                    let height = font_size * 1.5;
                    let bounds = DisplayRect::new(0.0, y, 800.0, height);
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: text.clone(),
                        font_run: None,
                        color_role: "heading".to_string(),
                        source_span: SourceSpan::default(),
                        font_size,
                    }));
                    if let Some(slug) = block.slug() {
                        dl.push_item(DisplayItem::Anchor(DisplaySemanticAnchor {
                            bounds,
                            anchor_id: slug,
                            is_heading: true,
                            level: *level,
                            source_span: SourceSpan::default(),
                        }));
                    }
                    dl.push_reading_node(AccessibleReadingNode {
                        role: AccessibleReadingRole::Heading { level: *level },
                        text: text.clone(),
                        source_span: SourceSpan::default(),
                        bounds,
                        children: Vec::new(),
                    });
                    y += height + 10.0;
                }
                DisplayBlock::Paragraph { text } => {
                    let bounds = DisplayRect::new(0.0, y, 800.0, line_height);
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: text.clone(),
                        font_run: None,
                        color_role: "text".to_string(),
                        source_span: SourceSpan::default(),
                        font_size: 14.0,
                    }));
                    dl.push_reading_node(AccessibleReadingNode {
                        role: AccessibleReadingRole::Paragraph,
                        text: text.clone(),
                        source_span: SourceSpan::default(),
                        bounds,
                        children: Vec::new(),
                    });
                    y += line_height + 8.0;
                }
                DisplayBlock::CodeBlock { language: _, source } => {
                    let lines_count = source.lines().count().max(1);
                    let height = (lines_count as f32) * line_height;
                    let bounds = DisplayRect::new(0.0, y, 800.0, height);
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: source.clone(),
                        font_run: None,
                        color_role: "code".to_string(),
                        source_span: SourceSpan::default(),
                        font_size: 13.0,
                    }));
                    dl.push_reading_node(AccessibleReadingNode {
                        role: AccessibleReadingRole::CodeBlock,
                        text: source.clone(),
                        source_span: SourceSpan::default(),
                        bounds,
                        children: Vec::new(),
                    });
                    y += height + 12.0;
                }
                DisplayBlock::ListItem { ordered: _, text } => {
                    let bounds = DisplayRect::new(20.0, y, 780.0, line_height);
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: text.clone(),
                        font_run: None,
                        color_role: "text".to_string(),
                        source_span: SourceSpan::default(),
                        font_size: 14.0,
                    }));
                    dl.push_reading_node(AccessibleReadingNode {
                        role: AccessibleReadingRole::ListItem,
                        text: text.clone(),
                        source_span: SourceSpan::default(),
                        bounds,
                        children: Vec::new(),
                    });
                    y += line_height + 4.0;
                }
                DisplayBlock::Quote { text } => {
                    let bounds = DisplayRect::new(16.0, y, 784.0, line_height);
                    let bar_bounds = DisplayRect::new(0.0, y, 4.0, line_height);
                    dl.push_item(DisplayItem::Vector(DisplayVectorPath {
                        bounds: bar_bounds,
                        shape: VectorShapeType::CalloutAccentBar,
                        stroke_width: 4.0,
                        color_role: "accent".to_string(),
                        source_span: SourceSpan::default(),
                    }));
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: text.clone(),
                        font_run: None,
                        color_role: "quote".to_string(),
                        source_span: SourceSpan::default(),
                        font_size: 14.0,
                    }));
                    dl.push_reading_node(AccessibleReadingNode {
                        role: AccessibleReadingRole::BlockQuote,
                        text: text.clone(),
                        source_span: SourceSpan::default(),
                        bounds,
                        children: Vec::new(),
                    });
                    y += line_height + 8.0;
                }
                DisplayBlock::Rule => {
                    let bounds = DisplayRect::new(0.0, y, 800.0, 2.0);
                    dl.push_item(DisplayItem::Vector(DisplayVectorPath {
                        bounds,
                        shape: VectorShapeType::HorizontalRule,
                        stroke_width: 2.0,
                        color_role: "border".to_string(),
                        source_span: SourceSpan::default(),
                    }));
                    dl.push_reading_node(AccessibleReadingNode {
                        role: AccessibleReadingRole::ThematicBreak,
                        text: String::new(),
                        source_span: SourceSpan::default(),
                        bounds,
                        children: Vec::new(),
                    });
                    y += 16.0;
                }
                DisplayBlock::TableHeader { cells } | DisplayBlock::TableRow { cells } => {
                    let bounds = DisplayRect::new(0.0, y, 800.0, line_height);
                    dl.push_item(DisplayItem::Vector(DisplayVectorPath {
                        bounds,
                        shape: VectorShapeType::TableBorder,
                        stroke_width: 1.0,
                        color_role: "table-border".to_string(),
                        source_span: SourceSpan::default(),
                    }));
                    let text = cells.join(" | ");
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: text.clone(),
                        font_run: None,
                        color_role: "text".to_string(),
                        source_span: SourceSpan::default(),
                        font_size: 14.0,
                    }));
                    dl.push_reading_node(AccessibleReadingNode {
                        role: AccessibleReadingRole::Table,
                        text,
                        source_span: SourceSpan::default(),
                        bounds,
                        children: Vec::new(),
                    });
                    y += line_height + 4.0;
                }
                DisplayBlock::UnresolvedAsset(asset) => {
                    let result = resolved.get(&asset.id);
                    let width = result.map_or(asset.estimated_width, |r| r.width) as f32;
                    let height = result.map_or(asset.estimated_height, |r| r.height) as f32;
                    let bounds = DisplayRect::new(0.0, y, width, height);
                    let is_resolved = result.is_some();
                    dl.push_item(DisplayItem::Image(DisplayImage {
                        bounds,
                        request_id: asset.id.0,
                        destination: asset.reference.clone(),
                        alt_text: asset.alt_text.clone(),
                        is_resolved,
                        source_span: SourceSpan::new(
                            asset.source_offset,
                            asset.source_offset.saturating_add(asset.reference.len()),
                        ),
                    }));
                    dl.push_reading_node(AccessibleReadingNode {
                        role: AccessibleReadingRole::Image,
                        text: asset.alt_text.clone(),
                        source_span: SourceSpan::default(),
                        bounds,
                        children: Vec::new(),
                    });
                    y += height + 10.0;
                }
            }
        }
        dl
    }

    /// Classify a single source line into a display block and optional asset request.
    fn classify_source_line(
        &mut self,
        line: &SourceLine,
    ) -> (Option<DisplayBlock>, Option<AssetRequest>) {
        if let Some(mut fence) = self.open_fence.take() {
            if fence.is_closing(&line.text) {
                return (Some(fence.into_block()), None);
            }
            fence.append_line(&line.text);
            self.open_fence = Some(fence);
            return (None, None);
        }
        if let Some(fence) = FencedCode::open(&line.text) {
            self.open_fence = Some(fence);
            return (None, None);
        }

        let trimmed = line.text.trim();
        if trimmed.is_empty() {
            return (None, None);
        }

        // Image syntax: ![alt](url) -> UnresolvedAsset
        if let Some((alt, url)) = Self::parse_image_syntax(trimmed) {
            let req_id = AssetRequestId(self.next_request_id);
            self.next_request_id += 1;
            let request = AssetRequest {
                id: req_id,
                kind: "image",
                url: url.clone(),
                source_offset: line.start_offset,
                generation: self.generation,
                estimated_width: 320,
                estimated_height: 240,
                alt_text: alt.clone(),
            };
            let unresolved = UnresolvedAsset {
                id: req_id,
                kind: "image",
                reference: url,
                source_offset: line.start_offset,
                generation: self.generation,
                estimated_width: 320,
                estimated_height: 240,
                alt_text: alt,
            };
            return (Some(DisplayBlock::UnresolvedAsset(unresolved)), Some(request));
        }

        // Heading: 1-6 `#` followed by space.
        if trimmed.starts_with('#') {
            let level = trimmed.bytes().take_while(|b| *b == b'#').count();
            if (1..=6).contains(&level)
                && trimmed
                    .get(level..)
                    .is_some_and(|rem| rem.starts_with(' '))
            {
                let text = trimmed
                    .get(level..)
                    .map_or("", |rem| rem.trim())
                    .to_owned();
                if !text.is_empty() {
                    let level_u8 = u8::try_from(level).unwrap_or(1);
                    return (
                        Some(DisplayBlock::Heading {
                            level: level_u8,
                            text,
                        }),
                        None,
                    );
                }
            }
        }

        // Horizontal rule: ---, ***, ___ (3+).
        if trimmed.len() >= 3 {
            let first = trimmed.as_bytes()[0];
            if (first == b'-' || first == b'*' || first == b'_')
                && trimmed.bytes().all(|b| b == first)
            {
                return (Some(DisplayBlock::Rule), None);
            }
        }

        // Blockquote: > text.
        if let Some(rest) = trimmed.strip_prefix("> ") {
            return (
                Some(DisplayBlock::Quote {
                    text: rest.to_owned(),
                }),
                None,
            );
        }

        // Unordered list: - item or * item.
        if let Some(rest) = trimmed.strip_prefix("- ").or(trimmed.strip_prefix("* ")) {
            return (
                Some(DisplayBlock::ListItem {
                    ordered: false,
                    text: rest.to_owned(),
                }),
                None,
            );
        }

        // Ordered list: 1. item, 2. item, etc.
        let bytes = trimmed.as_bytes();
        if bytes[0].is_ascii_digit() {
            if let Some(dot_pos) = trimmed.find(". ") {
                let prefix = &trimmed[..dot_pos];
                if prefix.bytes().all(|b| b.is_ascii_digit()) && !prefix.is_empty() {
                    return (
                        Some(DisplayBlock::ListItem {
                            ordered: true,
                            text: trimmed[dot_pos + 2..].to_owned(),
                        }),
                        None,
                    );
                }
            }
        }

        // Table row: | cell | cell |. A single pipe is ordinary text, not
        // an empty row with a reversed [1..0] slice.
        if trimmed.len() >= 2 && trimmed.starts_with('|') && trimmed.ends_with('|') {
            let cells: Vec<String> = trimmed[1..trimmed.len() - 1]
                .split('|')
                .map(|c| c.trim().to_owned())
                .collect();
            if cells.iter().any(|c| !c.is_empty()) {
                let is_separator = cells.iter().all(|c| {
                    let clean = c.replace(['-', ':', ' '], "");
                    clean.is_empty()
                });
                if !is_separator {
                    return (Some(DisplayBlock::TableRow { cells }), None);
                }
                return (
                    Some(DisplayBlock::TableHeader {
                        cells: cells
                            .iter()
                            .map(|c| c.trim_matches(['-', ':', ' ']).to_owned())
                            .collect(),
                    }),
                    None,
                );
            }
        }

        // Default: paragraph.
        (
            Some(DisplayBlock::Paragraph {
                text: trimmed.to_owned(),
            }),
            None,
        )
    }

    /// Extract image markdown `![alt](url)`.
    fn parse_image_syntax(trimmed: &str) -> Option<(String, String)> {
        if trimmed.starts_with("![") && trimmed.ends_with(')') {
            let close_bracket = trimmed.find("](")?;
            let alt = trimmed[2..close_bracket].trim().to_string();
            let url = trimmed[close_bracket + 2..trimmed.len() - 1]
                .trim()
                .to_string();
            return Some((alt, url));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn fences_preserve_literal_content_across_every_step_boundary() {
        let cases = [
            ("```rust\nfn main() {}\n```", Some("rust"), "fn main() {}\n"),
            (
                "~~~text\n# literal\n![literal](private.png)\n~~~",
                Some("text"),
                "# literal\n![literal](private.png)\n",
            ),
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
                let blocks = engine.process_all().unwrap();
                assert_eq!(
                    blocks,
                    vec![DisplayBlock::CodeBlock {
                        language: language.map(str::to_owned),
                        source: code.to_owned(),
                    }],
                    "source={source:?}, batch={batch}"
                );
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
    fn invalid_fences_and_single_pipe_remain_ordinary_text() {
        for source in ["|", "    ```rust", "```info`", "~~", "\t~~~"] {
            let mut engine = ResumableFlowDisplay::new(source, 1);
            assert_eq!(
                engine.process_all().unwrap(),
                vec![DisplayBlock::Paragraph {
                    text: source.trim().to_owned(),
                }]
            );
        }
    }

    #[test]
    fn source_line_offsets_count_empty_lines_unicode_and_crlf() {
        let source = "é\r\n\r\né\r\n![image](x.png)\r\n";
        let engine = ResumableFlowDisplay::new(source, 1);
        let starts: Vec<usize> = engine.lines.iter().map(|line| line.start_offset).collect();
        assert_eq!(starts, vec![0, 4, 6, 10]);
        for line in &engine.lines {
            assert!(source[line.start_offset..].starts_with(&line.text));
        }
        let mut engine = engine;
        engine.process_all().unwrap();
        assert_eq!(engine.unresolved_assets()[0].source_offset, 10);
    }

    #[test]
    fn resolved_image_dimensions_reflow_following_content() {
        let mut engine = ResumableFlowDisplay::new("![image](x.png)\nafter", 8);
        engine.process_all().unwrap();
        let id = engine.unresolved_assets()[0].id;
        let before = engine.to_display_list();
        assert_eq!(before.reading_order()[1].bounds.y, 250.0);
        engine
            .provide_asset(AssetResult {
                request_id: id,
                generation: 1,
                width: 640,
                height: 480,
                bytes: Some(vec![1]),
            })
            .unwrap();
        let after = engine.to_display_list();
        let DisplayItem::Image(image) = &after.items()[0] else {
            panic!("first item must be the image");
        };
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
            request_id: id,
            generation: 1,
            width: 640,
            height: 480,
            bytes: Some(vec![1]),
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
        assert_eq!(
            engine.provide_asset(result.clone()),
            Err(FlowDisplayError::StaleAssetGeneration {
                expected: 2,
                actual: 1,
            })
        );
        assert_eq!(engine.unresolved_assets().len(), 2);
        let refreshed = AssetResult {
            generation: 2,
            ..result
        };
        engine.provide_asset(refreshed).unwrap();
        assert!(engine.is_asset_resolved(id));
        assert_eq!(engine.unresolved_assets().len(), 1);
    }
}
