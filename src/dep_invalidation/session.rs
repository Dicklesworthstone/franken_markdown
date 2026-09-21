//! Transactional source-edit, resize and asset-completion sessions.
//!
//! Parsing and layout are synchronous. The existing flow admission limits bound
//! input/projection and FlowLayoutOptions bound shaping/output. This is not an
//! incremental parser: edits build a candidate with the shared parser, then
//! publish source, assets and display together only after all stages succeed.

use super::{DependencyGraph, DocumentChangeSet};
use crate::display::DisplayList;
use crate::flow_display::{
    AssetRequest, AssetRequestId, AssetResult, DisplayBlock, FlowDisplayError,
    FlowDisplayLimits, FlowInlineStyle, FlowLayoutError, FlowLayoutOptions,
    FlowTextRole, ResumableFlowDisplay,
};
use crate::span::SourceSpan;
use crate::text::OwnedTextRun;
use std::collections::HashMap;
use std::fmt;

#[path = "asset_batch.rs"]
mod asset_batch;

/// Reusing an unchanged URL is unsafe unless the host also knows its bytes and
/// authorization are unchanged. The conservative default discards all assets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowAssetReuse {
    #[default]
    Invalidate,
    /// The host confirms its resource context and retained asset bytes remain
    /// valid. Only occurrences in source-and-AST-verified reusable blocks are
    /// eligible; requests are remapped to the new generation/occurrence IDs.
    HostVerifiedUnchanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowAssetRemap {
    pub old_id: AssetRequestId,
    pub new_id: AssetRequestId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowUpdate {
    pub previous_revision: u64,
    pub revision: u64,
    pub layout_revision: u64,
    pub source_changed: bool,
    pub changes: DocumentChangeSet,
    pub reused_assets: Vec<FlowAssetRemap>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FlowSessionError {
    StaleRevision { expected: u64, actual: u64 },
    InvalidEdit { span: SourceSpan, source_len: usize },
    SourceBudgetExceeded { maximum: usize },
    RevisionExhausted,
    Input(FlowDisplayError),
    Layout(FlowLayoutError),
}

impl fmt::Display for FlowSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(f, "stale document revision {actual}; current revision is {expected}"),
            Self::InvalidEdit { span, source_len } => write!(f,
                "edit [{}, {}) is not a valid UTF-8 range in {source_len} source bytes", span.start, span.end),
            Self::SourceBudgetExceeded { maximum } => write!(f, "edited source exceeds {maximum} byte budget"),
            Self::RevisionExhausted => write!(f, "document/layout revision counter exhausted"),
            Self::Input(error) => write!(f, "{error}"),
            Self::Layout(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for FlowSessionError {}
impl From<FlowDisplayError> for FlowSessionError {
    fn from(error: FlowDisplayError) -> Self { Self::Input(error) }
}
impl From<FlowLayoutError> for FlowSessionError {
    fn from(error: FlowLayoutError) -> Self { Self::Layout(error) }
}

/// An editor's last successfully rendered document. Source edits use optimistic
/// revision checks; layout/asset changes have a separate monotonically increasing
/// layout revision. An old selection can therefore be fenced by both counters.
///
/// All getters are immutable. A rejected edit/resize/asset result changes no
/// source, display, pending request, accepted asset or revision. Shaper callback
/// side effects are host-owned and cannot be rolled back by this type.
///
/// Snapshots currently parse once for flow projection and once for change
/// analysis. Neither parse is claimed incremental; the candidate's source must
/// first pass the flow engine's admission and AST/output budgets.
#[derive(Debug)]
pub struct FlowSession {
    engine: ResumableFlowDisplay,
    graph: DependencyGraph,
    display: DisplayList,
    options: FlowLayoutOptions,
    layout_revision: u64,
}

impl FlowSession {
    pub fn new<F>(
        source: &str, batch_size: usize, limits: FlowDisplayLimits,
        options: FlowLayoutOptions, shape: F,
    ) -> Result<Self, FlowSessionError>
    where F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        let engine = completed(source, batch_size, 1, limits)?;
        let display = engine.to_styled_display_list(options, shape)?;
        let graph = DependencyGraph::scan(source);
        Ok(Self { engine, graph, display, options, layout_revision: 1 })
    }

    #[must_use]
    pub fn source(&self) -> &str { self.engine.source() }
    #[must_use]
    pub fn revision(&self) -> u64 { self.engine.generation() }
    #[must_use]
    pub const fn layout_revision(&self) -> u64 { self.layout_revision }
    #[must_use]
    pub fn display(&self) -> &DisplayList { &self.display }
    #[must_use]
    pub fn engine(&self) -> &ResumableFlowDisplay { &self.engine }
    #[must_use]
    pub fn pending_assets(&self) -> &[AssetRequest] { self.engine.unresolved_assets() }
    #[must_use]
    pub const fn layout_options(&self) -> FlowLayoutOptions { self.options }

    /// Replace an exact UTF-8 source range, including insertion/deletion.
    /// Reject stale editors and oversized results BEFORE copying the new source.
    pub fn edit<F>(
        &mut self, expected_revision: u64, span: SourceSpan, replacement: &str,
        reuse: FlowAssetReuse, shape: F,
    ) -> Result<FlowUpdate, FlowSessionError>
    where F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        self.check_revision(expected_revision)?;
        let current = span.slice(self.source()).ok_or(FlowSessionError::InvalidEdit {
            span, source_len: self.source().len(),
        })?;
        if current == replacement { return Ok(self.no_change()); }
        let maximum = self.engine.limits().max_source_bytes;
        let required = self.source().len().checked_sub(span.len())
            .and_then(|len| len.checked_add(replacement.len()))
            .filter(|len| *len <= maximum)
            .ok_or(FlowSessionError::SourceBudgetExceeded { maximum })?;
        let mut source = String::with_capacity(required);
        source.push_str(&self.source()[..span.start]);
        source.push_str(replacement);
        source.push_str(&self.source()[span.end..]);
        self.replace_source(expected_revision, &source, reuse, shape)
    }

    /// Atomically replace source and its fully shaped display. References are
    /// resolved against the entire candidate, never stale parser state.
    /// Equal source is a no-op; use `reload_assets` for external changes.
    pub fn replace_source<F>(
        &mut self, expected_revision: u64, source: &str, reuse: FlowAssetReuse, shape: F,
    ) -> Result<FlowUpdate, FlowSessionError>
    where F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        self.check_revision(expected_revision)?;
        if source == self.source() { return Ok(self.no_change()); }
        let previous_revision = self.revision();
        let revision = next_revision(previous_revision)?;
        let layout_revision = next_revision(self.layout_revision)?;
        let mut candidate = completed(source, self.engine.batch_size(), revision, self.engine.limits())?;
        let graph = DependencyGraph::scan(source);
        let changes = self.graph.compare(&graph);
        let reused_assets = if reuse == FlowAssetReuse::HostVerifiedUnchanged {
            reuse_assets(&self.engine, &mut candidate, &changes)?
        } else { Vec::new() };
        let display = candidate.to_styled_display_list(self.options, shape)?;
        // The only publication point. Every fallible operation preceded it.
        self.engine = candidate;
        self.graph = graph;
        self.display = display;
        self.layout_revision = layout_revision;
        Ok(FlowUpdate { previous_revision, revision, layout_revision, source_changed: true, changes, reused_assets })
    }

    /// Resize/change typography without invalidating in-flight asset requests.
    /// Failed shaping leaves both the old options and display intact.
    pub fn reflow<F>(&mut self, options: FlowLayoutOptions, shape: F) -> Result<(), FlowSessionError>
    where F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        let revision = next_revision(self.layout_revision)?;
        let display = self.engine.to_styled_display_list(options, shape)?;
        self.display = display;
        self.options = options;
        self.layout_revision = revision;
        Ok(())
    }

    /// Accept a host asset and publish the resulting reflow atomically. A bad
    /// payload or shaper failure does not consume its pending request. The
    /// source revision stays fixed so other in-flight requests remain valid.
    pub fn provide_asset<F>(&mut self, result: AssetResult, shape: F) -> Result<(), FlowSessionError>
    where F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        self.provide_assets(vec![result], shape)
    }

    /// On success, discard retained assets and reissue requests in a fresh generation.
    /// Hosts use this when permissions, base URI, or external bytes change even
    /// though the Markdown is identical. Late old-generation results fail.
    /// As with other transactions, failure preserves the old snapshot; a host
    /// revoking authorization must stop presenting that snapshot immediately.
    pub fn reload_assets<F>(&mut self, expected_revision: u64, shape: F) -> Result<(), FlowSessionError>
    where F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        self.check_revision(expected_revision)?;
        let revision = next_revision(self.revision())?;
        let layout_revision = next_revision(self.layout_revision)?;
        let candidate = completed(self.source(), self.engine.batch_size(), revision, self.engine.limits())?;
        let display = candidate.to_styled_display_list(self.options, shape)?;
        self.engine = candidate;
        self.display = display;
        self.layout_revision = layout_revision;
        Ok(())
    }

    fn check_revision(&self, actual: u64) -> Result<(), FlowSessionError> {
        if actual != self.revision() {
            return Err(FlowSessionError::StaleRevision { expected: self.revision(), actual });
        }
        Ok(())
    }

    fn no_change(&self) -> FlowUpdate {
        FlowUpdate {
            previous_revision: self.revision(), revision: self.revision(),
            layout_revision: self.layout_revision, source_changed: false,
            changes: self.graph.compare(&self.graph), reused_assets: Vec::new(),
        }
    }
}

fn next_revision(current: u64) -> Result<u64, FlowSessionError> {
    current.checked_add(1).ok_or(FlowSessionError::RevisionExhausted)
}

fn completed(source: &str, batch: usize, revision: u64, limits: FlowDisplayLimits)
    -> Result<ResumableFlowDisplay, FlowDisplayError>
{
    let mut engine = ResumableFlowDisplay::try_with_limits(source, batch, revision, limits)?;
    // Do not collect another copy of all emitted blocks as process_all does.
    while engine.step()?.is_some() {}
    Ok(engine)
}

fn reuse_assets(old: &ResumableFlowDisplay, new: &mut ResumableFlowDisplay, changes: &DocumentChangeSet)
    -> Result<Vec<FlowAssetRemap>, FlowDisplayError>
{
    let spans: HashMap<_, _> = changes.reusable.iter().map(|block| (block.old_span, block.new_span)).collect();
    let accepted: HashMap<_, _> = old.resolved_assets().iter().map(|asset| (asset.request_id, asset)).collect();
    let mut old_counts = HashMap::<SourceSpan, usize>::new();
    let mut reusable = HashMap::new();
    for (index, block) in old.blocks().iter().enumerate() {
        let DisplayBlock::UnresolvedAsset(asset) = block else { continue; };
        let Some(span) = old.source_span_for_block(index) else { continue; };
        let ordinal = old_counts.entry(span).or_default();
        if let (Some(&new_span), Some(&result)) = (spans.get(&span), accepted.get(&asset.id)) {
            reusable.insert((new_span, *ordinal), (asset, result));
        }
        *ordinal += 1;
    }
    let mut new_counts = HashMap::<SourceSpan, usize>::new();
    let mut transfers = Vec::new();
    for (index, block) in new.blocks().iter().enumerate() {
        let DisplayBlock::UnresolvedAsset(asset) = block else { continue; };
        let Some(span) = new.source_span_for_block(index) else { continue; };
        let ordinal = new_counts.entry(span).or_default();
        if let Some(&(previous, result)) = reusable.get(&(span, *ordinal)) {
            if previous.reference == asset.reference && previous.alt_text == asset.alt_text {
                transfers.push((FlowAssetRemap { old_id: previous.id, new_id: asset.id }, result));
            }
        }
        *ordinal += 1;
    }
    let mut remaps = Vec::with_capacity(transfers.len());
    for (remap, result) in transfers {
        new.provide_asset(AssetResult {
            request_id: remap.new_id, generation: new.generation(),
            width: result.width, height: result.height, bytes: result.bytes.clone(),
        })?;
        remaps.push(remap);
    }
    Ok(remaps)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::fonts::BundledFlowFonts;
    use crate::FontFamily;

    fn fonts() -> BundledFlowFonts { BundledFlowFonts::new(FontFamily::Sans).unwrap() }
    fn session(source: &str, fonts: &BundledFlowFonts) -> FlowSession {
        FlowSession::new(source, 2, FlowDisplayLimits::default(), FlowLayoutOptions::default(),
            |text, size, role, style| fonts.shape(text, size, role, style)).unwrap()
    }
    fn payload(request: &AssetRequest) -> AssetResult {
        AssetResult { request_id: request.id, generation: request.generation, width: 20, height: 10, bytes: Some(vec![1, 2]) }
    }

    #[test]
    fn edits_publish_new_source_display_and_revision_together() {
        let fonts = fonts();
        let mut doc = session("before\n\nold\n\nafter\n", &fonts);
        let update = doc.edit(1, SourceSpan::new(8, 11), "new text", FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(doc.source(), "before\n\nnew text\n\nafter\n");
        assert_eq!(doc.revision(), 2);
        assert_eq!(doc.layout_revision(), 2);
        assert_eq!(update.changes.dirty_blocks, vec![1]);
        assert_eq!(update.changes.reusable.len(), 2);
        assert!(doc.display().reading_order().iter().any(|node| node.text == "new text"));
    }

    #[test]
    fn failed_edits_keep_the_entire_published_state_and_allow_retry() {
        let fonts = fonts();
        let mut doc = session("café\n\n![pic](x.png)", &fonts);
        let before = doc.display().clone();
        let requests = doc.pending_assets().to_vec();
        assert!(matches!(doc.edit(0, SourceSpan::new(0, 1), "x", FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)), Err(FlowSessionError::StaleRevision { .. })));
        assert!(matches!(doc.edit(1, SourceSpan::new(4, 5), "x", FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)), Err(FlowSessionError::InvalidEdit { .. })));
        assert!(doc.edit(1, SourceSpan::new(0, 1), "x", FlowAssetReuse::Invalidate,
            |_, _, _, _| Err("shaper unavailable".to_owned())).is_err());
        assert_eq!(doc.source(), "café\n\n![pic](x.png)");
        assert_eq!(doc.display(), &before);
        assert_eq!(doc.pending_assets(), requests.as_slice());
        assert_eq!((doc.revision(), doc.layout_revision()), (1, 1));
        doc.edit(1, SourceSpan::new(0, 1), "C", FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(doc.revision(), 2);
    }

    #[test]
    fn source_and_projection_budget_failures_are_atomic() {
        let fonts = fonts();
        let limits = FlowDisplayLimits { max_source_bytes: 10, max_output_blocks: 1, ..FlowDisplayLimits::default() };
        let mut doc = FlowSession::new("one", 1, limits, FlowLayoutOptions::default(),
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        let before = doc.display().clone();
        assert!(doc.edit(1, SourceSpan::new(0, 0), "too much text", FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)).is_err());
        assert!(doc.replace_source(1, "one\n\ntwo", FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)).is_err());
        assert_eq!(doc.source(), "one");
        assert_eq!(doc.revision(), 1);
        assert_eq!(doc.display(), &before);
    }

    #[test]
    fn equal_edits_are_noops_without_shaping_or_asset_invalidation() {
        let fonts = fonts();
        let mut doc = session("same", &fonts);
        let update = doc.edit(1, SourceSpan::new(0, 4), "same", FlowAssetReuse::Invalidate,
            |_, _, _, _| Err("must not shape".to_owned())).unwrap();
        assert!(!update.source_changed);
        assert_eq!((doc.revision(), doc.layout_revision()), (1, 1));
        assert_eq!(update.changes.reusable.len(), 1);
    }

    #[test]
    fn reference_edits_update_distant_click_targets() {
        let fonts = fonts();
        let mut doc = session("[link][r]\n\n[r]: https://old.example\n", &fonts);
        let source = doc.source().replace("old.example", "new.example");
        let update = doc.replace_source(1, &source, FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert!(update.changes.dirty_blocks.contains(&0));
        assert!(doc.display().anchors().any(|anchor| anchor.anchor_id == "https://new.example"));
        assert!(!doc.display().anchors().any(|anchor| anchor.anchor_id == "https://old.example"));
    }

    #[test]
    fn asset_completion_is_atomic_and_does_not_stale_other_requests() {
        let fonts = fonts();
        let mut doc = session("![one](a.png)\n\n![two](b.png)\n\nafter", &fonts);
        let requests = doc.pending_assets().to_vec();
        let result = payload(&requests[0]);
        let before = doc.display().clone();
        assert!(doc.provide_asset(result.clone(), |_, _, _, _| Err("failure".to_owned())).is_err());
        assert_eq!(doc.pending_assets(), requests.as_slice());
        assert_eq!(doc.display(), &before);
        assert_eq!(doc.engine().retained_asset_bytes(), 0);
        doc.provide_asset(result, |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        doc.provide_asset(payload(&requests[1]), |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!((doc.revision(), doc.layout_revision()), (1, 3));
        assert!(doc.pending_assets().is_empty());
        assert_eq!(doc.engine().retained_asset_bytes(), 4);
    }

    #[test]
    fn verified_asset_reuse_remaps_occurrences_after_an_earlier_insertion() {
        let fonts = fonts();
        let mut doc = session("before\n\n![same](a.png) ![same](a.png)\n", &fonts);
        let old = doc.pending_assets().to_vec();
        for (index, request) in old.iter().enumerate() {
            let mut result = payload(request);
            result.width += index as u32;
            doc.provide_asset(result, |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        }
        let source = "![new](new.png)\n\n![same](a.png) ![same](a.png)\n";
        let update = doc.replace_source(1, source, FlowAssetReuse::HostVerifiedUnchanged,
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(update.reused_assets.len(), 2);
        assert_eq!(update.reused_assets[0].old_id, old[0].id);
        assert_ne!(update.reused_assets[0].new_id, old[0].id);
        assert_eq!(doc.pending_assets().len(), 1);
        assert_eq!(doc.pending_assets()[0].url, "new.png");
        assert_eq!(doc.engine().resolved_assets()[0].width, 20);
        assert_eq!(doc.engine().resolved_assets()[1].width, 21);
        assert!(matches!(doc.provide_asset(payload(&old[0]), |t, s, r, i| fonts.shape(t, s, r, i)),
            Err(FlowSessionError::Input(FlowDisplayError::StaleAssetGeneration { .. }))));
    }

    #[test]
    fn default_reuse_policy_and_explicit_reload_drop_cached_assets() {
        let fonts = fonts();
        let mut doc = session("before\n\n![pic](x.png)", &fonts);
        let first = doc.pending_assets()[0].clone();
        doc.provide_asset(payload(&first), |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        doc.edit(1, SourceSpan::new(0, 6), "after", FlowAssetReuse::default(),
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(doc.engine().retained_asset_bytes(), 0);
        let second = doc.pending_assets()[0].clone();
        doc.provide_asset(payload(&second), |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        doc.reload_assets(2, |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(doc.revision(), 3);
        assert_eq!(doc.engine().retained_asset_bytes(), 0);
        assert_eq!(doc.pending_assets().len(), 1);
    }

    #[test]
    fn resizing_changes_layout_revision_not_document_or_request_generation() {
        let fonts = fonts();
        let mut doc = session("a paragraph with several words\n\n![pic](x.png)", &fonts);
        let request = doc.pending_assets()[0].clone();
        let options = FlowLayoutOptions { viewport_width: 100.0, ..doc.layout_options() };
        doc.reflow(options, |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!((doc.revision(), doc.layout_revision()), (1, 2));
        assert_eq!(doc.pending_assets()[0], request);
        let before = doc.display().clone();
        assert!(doc.reflow(FlowLayoutOptions { viewport_width: 0.0, ..options },
            |t, s, r, i| fonts.shape(t, s, r, i)).is_err());
        assert_eq!(doc.layout_options(), options);
        assert_eq!(doc.display(), &before);
        assert_eq!(doc.layout_revision(), 2);
    }

    #[test]
    fn revision_overflow_cannot_wrap_into_a_stale_generation() {
        assert_eq!(next_revision(u64::MAX), Err(FlowSessionError::RevisionExhausted));
        let fonts = fonts();
        let mut doc = session("before", &fonts);
        doc.layout_revision = u64::MAX;
        assert!(matches!(doc.edit(1, SourceSpan::new(0, 6), "after", FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)), Err(FlowSessionError::RevisionExhausted)));
        assert_eq!(doc.source(), "before");
        assert_eq!(doc.revision(), 1);
    }
}
