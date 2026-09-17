//! Persistent, bounded browser/editor sessions over the real flow renderer.
//!
//! The native core stays independent of JavaScript. Its WASM adapter is a thin
//! feature-gated module. JSON uses decimal strings for all u64 identities, paged
//! display/asset inventories, and explicit source-byte versus fragment-local
//! reading-text coordinates. Nothing in this module fetches or opens a URL.

use super::{FlowAssetReuse, FlowSession, FlowSessionError, FlowShapeCache, FlowShapeCacheLimits};
use crate::display::DisplayItem;
use crate::flow_display::{AssetRequestId, AssetResult, FlowDisplayError, FlowDisplayLimits, FlowLayoutError, FlowLayoutOptions};
use crate::text::{FontId, utf16_to_byte};
use crate::{FontFamily, SourceSpan};
use std::fmt;

mod wire;
#[cfg(feature = "wasm-bindgen")]
mod wasm;

/// Browser admission happens before retaining source in the Rust session.
/// The generated WASM string conversion necessarily precedes this check; the
/// JavaScript facade also enforces the same limit before calling into WASM.
pub const MAX_BROWSER_SOURCE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_BROWSER_ASSET_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SNAPSHOT_PAGE_ITEMS: usize = 2048;
pub const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum BrowserFlowError {
    Session(FlowSessionError),
    InvalidFont,
    InvalidIdentity,
    InvalidSelection,
    StaleLayout { expected: u64, actual: u64 },
    InvalidPage,
    UnknownFont,
    SnapshotTooLarge,
}

impl BrowserFlowError {
    /// Stable machine-readable code, independent of diagnostic wording.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Session(FlowSessionError::StaleRevision { .. }) => "STALE_REVISION",
            Self::Session(FlowSessionError::InvalidEdit { .. }) | Self::InvalidSelection => "INVALID_SELECTION",
            Self::Session(FlowSessionError::SourceBudgetExceeded { .. })
            | Self::Session(FlowSessionError::Input(FlowDisplayError::BudgetExceeded(_)))
            | Self::Session(FlowSessionError::Layout(FlowLayoutError::BudgetExceeded(_))) => "BUDGET_EXCEEDED",
            Self::Session(FlowSessionError::Input(FlowDisplayError::StaleAssetGeneration { .. })) => "STALE_ASSET_GENERATION",
            Self::Session(FlowSessionError::Input(FlowDisplayError::UnknownAssetRequest(_))) => "UNKNOWN_ASSET_REQUEST",
            Self::Session(FlowSessionError::Input(FlowDisplayError::InvalidAssetDimensions { .. })) => "INVALID_ASSET_DIMENSIONS",
            Self::Session(FlowSessionError::Layout(FlowLayoutError::InvalidOptions)) => "INVALID_OPTIONS",
            Self::Session(FlowSessionError::Layout(_)) => "LAYOUT_ERROR",
            Self::Session(FlowSessionError::RevisionExhausted) => "REVISION_EXHAUSTED",
            Self::Session(_) => "FLOW_ERROR",
            Self::InvalidFont => "INVALID_FONT",
            Self::InvalidIdentity => "INVALID_IDENTITY",
            Self::StaleLayout { .. } => "STALE_LAYOUT",
            Self::InvalidPage => "INVALID_PAGE",
            Self::UnknownFont => "UNKNOWN_FONT",
            Self::SnapshotTooLarge => "SNAPSHOT_TOO_LARGE",
        }
    }
}

impl fmt::Display for BrowserFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => write!(f, "{error}"),
            Self::InvalidFont => f.write_str("font must be sans or serif"),
            Self::InvalidIdentity => f.write_str("identity must be a canonical unsigned 64-bit decimal string"),
            Self::InvalidSelection => f.write_str("selection is outside its text or splits a Unicode scalar"),
            Self::StaleLayout { expected, actual } => write!(f, "stale layout revision {actual}; current layout revision is {expected}"),
            Self::InvalidPage => f.write_str("page offset is outside the inventory or limit is outside 1..=2048"),
            Self::UnknownFont => f.write_str("font identity does not belong to this session's bundled registry"),
            Self::SnapshotTooLarge => f.write_str("snapshot exceeds 16 MiB; request fewer items or omit glyphs"),
        }
    }
}

impl std::error::Error for BrowserFlowError {}
impl From<FlowSessionError> for BrowserFlowError {
    fn from(error: FlowSessionError) -> Self { Self::Session(error) }
}
impl From<FlowLayoutError> for BrowserFlowError {
    fn from(error: FlowLayoutError) -> Self { Self::Session(FlowSessionError::Layout(error)) }
}

/// Decimal wire identities deliberately do not pass through JavaScript Number.
/// Reject signs, whitespace, leading zeroes and overflow instead of coercing.
pub fn parse_identity(value: &str) -> Result<u64, BrowserFlowError> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(BrowserFlowError::InvalidIdentity);
    }
    value.parse().map_err(|_| BrowserFlowError::InvalidIdentity)
}

fn utf16_span(text: &str, start: usize, end: usize) -> Result<SourceSpan, BrowserFlowError> {
    if start > end { return Err(BrowserFlowError::InvalidSelection); }
    let start = utf16_to_byte(text, start).ok_or(BrowserFlowError::InvalidSelection)?;
    let end = utf16_to_byte(text, end).ok_or(BrowserFlowError::InvalidSelection)?;
    Ok(SourceSpan::new(start, end))
}

fn page(offset: usize, limit: usize, len: usize) -> Result<std::ops::Range<usize>, BrowserFlowError> {
    if limit == 0 || limit > MAX_SNAPSHOT_PAGE_ITEMS || offset > len {
        return Err(BrowserFlowError::InvalidPage);
    }
    Ok(offset..offset.saturating_add(limit).min(len))
}

/// Fully shaped, persistent editor state with a bounded bundled-font cache.
///
/// Edits remain whole-document transactions, not an incremental parser. A
/// caller must explicitly opt into reuse of still-authorized unchanged assets.
/// Shaping uses the bundled simple-LTR profile; unsupported scripts/sequences
/// fail without replacing the last successful document. Use FlowSession with a
/// host shaper when the application requires a broader shaping profile.
pub struct BrowserFlowSession {
    session: FlowSession,
    fonts: FlowShapeCache,
    source_utf16_len: usize,
}

impl BrowserFlowSession {
    pub fn new(source: &str, font: &str, options: FlowLayoutOptions) -> Result<Self, BrowserFlowError> {
        let family = FontFamily::parse(font).ok_or(BrowserFlowError::InvalidFont)?;
        let limits = FlowDisplayLimits {
            max_source_bytes: MAX_BROWSER_SOURCE_BYTES,
            max_source_lines: 100_000,
            max_ast_nodes: 200_000,
            max_output_blocks: 100_000,
            max_output_bytes: 16 * 1024 * 1024,
            max_asset_requests: 1024,
            max_asset_bytes: MAX_BROWSER_ASSET_BYTES,
            max_retained_asset_bytes: 32 * 1024 * 1024,
            ..FlowDisplayLimits::default()
        };
        // Refuse huge input before initializing even the shared font registry.
        if source.len() > limits.max_source_bytes {
            return Err(FlowSessionError::SourceBudgetExceeded { maximum: limits.max_source_bytes }.into());
        }
        let mut fonts = FlowShapeCache::new(family, FlowShapeCacheLimits::default())?;
        let session = FlowSession::new(source, 64, limits, options,
            |text, size, role, style| fonts.shape(text, size, role, style))?;
        Ok(Self { session, fonts, source_utf16_len: source.encode_utf16().count() })
    }

    #[must_use]
    pub fn source(&self) -> &str { self.session.source() }
    #[must_use]
    pub fn revision(&self) -> u64 { self.session.revision() }
    #[must_use]
    pub fn layout_revision(&self) -> u64 { self.session.layout_revision() }

    fn check_revision(&self, revision: u64) -> Result<(), BrowserFlowError> {
        if revision != self.revision() {
            return Err(FlowSessionError::StaleRevision { expected: self.revision(), actual: revision }.into());
        }
        Ok(())
    }

    fn check_snapshot(&self, revision: u64, layout_revision: u64) -> Result<(), BrowserFlowError> {
        self.check_revision(revision)?;
        if layout_revision != self.layout_revision() {
            return Err(BrowserFlowError::StaleLayout { expected: self.layout_revision(), actual: layout_revision });
        }
        Ok(())
    }

    /// UTF-16 indices are directly compatible with textarea selections. Mid-
    /// surrogate offsets are rejected, never rounded or silently replaced.
    pub fn edit_utf16(&mut self, revision: u64, start: usize, end: usize, replacement: &str,
        reuse: FlowAssetReuse) -> Result<(), BrowserFlowError>
    {
        self.check_revision(revision)?;
        let span = utf16_span(self.source(), start, end)?;
        self.edit_bytes(revision, span, replacement, reuse)
    }

    pub fn edit_bytes(&mut self, revision: u64, span: SourceSpan, replacement: &str,
        reuse: FlowAssetReuse) -> Result<(), BrowserFlowError>
    {
        let fonts = &mut self.fonts;
        self.session.edit(revision, span, replacement, reuse,
            |text, size, role, style| fonts.shape(text, size, role, style))?;
        self.source_utf16_len = self.source().encode_utf16().count();
        Ok(())
    }

    pub fn replace_source(&mut self, revision: u64, source: &str, reuse: FlowAssetReuse)
        -> Result<(), BrowserFlowError>
    {
        let fonts = &mut self.fonts;
        self.session.replace_source(revision, source, reuse,
            |text, size, role, style| fonts.shape(text, size, role, style))?;
        self.source_utf16_len = self.source().encode_utf16().count();
        Ok(())
    }

    /// Fence a resize against both the document and its currently shown layout.
    pub fn reflow(&mut self, revision: u64, layout_revision: u64, options: FlowLayoutOptions)
        -> Result<(), BrowserFlowError>
    {
        self.check_snapshot(revision, layout_revision)?;
        let fonts = &mut self.fonts;
        self.session.reflow(options, |text, size, role, style| fonts.shape(text, size, role, style))?;
        Ok(())
    }

    pub fn provide_asset(&mut self, asset: AssetResult) -> Result<(), BrowserFlowError> {
        let fonts = &mut self.fonts;
        self.session.provide_asset(asset, |text, size, role, style| fonts.shape(text, size, role, style))?;
        Ok(())
    }

    pub fn reload_assets(&mut self, revision: u64) -> Result<(), BrowserFlowError> {
        let fonts = &mut self.fonts;
        self.session.reload_assets(revision, |text, size, role, style| fonts.shape(text, size, role, style))?;
        Ok(())
    }

    pub fn font_bytes(&self, id: u64) -> Result<&'static [u8], BrowserFlowError> {
        self.fonts.font_bytes(FontId::new(id)).ok_or(BrowserFlowError::UnknownFont)
    }

    /// Read a resolved payload only in the current document/resource generation.
    /// None means a known dimension-only result, not an unknown request.
    pub fn asset_bytes(&self, id: u64, revision: u64) -> Result<Option<&[u8]>, BrowserFlowError> {
        self.check_revision(revision)?;
        let result = self.session.engine().resolved_assets().iter()
            .find(|asset| asset.request_id == AssetRequestId(id))
            .ok_or(FlowSessionError::Input(FlowDisplayError::UnknownAssetRequest(AssetRequestId(id))))?;
        Ok(result.bytes.as_deref())
    }

    /// Explicit enclosing-source copy. This must not be labeled exact inline
    /// source mapping: fragment indices belong to rendered reading text.
    pub fn copy_source(&self, revision: u64, span: SourceSpan) -> Result<&str, BrowserFlowError> {
        self.check_revision(revision)?;
        span.slice(self.source()).ok_or(BrowserFlowError::InvalidSelection)
    }

    /// Immutable paged display data. Serialization is a read operation: even a
    /// snapshot-size refusal cannot mutate the last successfully rendered state.
    pub fn snapshot_json(&self, revision: u64, layout_revision: u64, offset: usize,
        limit: usize, glyphs: bool) -> Result<String, BrowserFlowError>
    {
        self.check_snapshot(revision, layout_revision)?;
        let range = page(offset, limit, self.session.display().items().len())?;
        wire::snapshot(self, range, glyphs)
    }

    /// Page accessible reading nodes separately from drawing items. Cell roles
    /// and child geometry remain hierarchical; generated text is not guessed.
    pub fn reading_json(&self, revision: u64, layout_revision: u64, offset: usize, limit: usize)
        -> Result<String, BrowserFlowError>
    {
        self.check_snapshot(revision, layout_revision)?;
        let range = page(offset, limit, self.session.display().reading_order().len())?;
        wire::reading(self, range)
    }

    pub fn pending_assets_json(&self, revision: u64, offset: usize, limit: usize)
        -> Result<String, BrowserFlowError>
    {
        self.check_revision(revision)?;
        let range = page(offset, limit, self.session.pending_assets().len())?;
        wire::assets(self, range)
    }

    /// Hit a text fragment or active link in a specific immutable layout.
    /// Caret byte/UTF-16 offsets are fragment-local, not original Markdown.
    pub fn hit_test_json(&self, revision: u64, layout_revision: u64, x: f32, y: f32)
        -> Result<String, BrowserFlowError>
    {
        self.check_snapshot(revision, layout_revision)?;
        if !x.is_finite() || !y.is_finite() { return Err(BrowserFlowError::InvalidSelection); }
        wire::hit(self, x, y)
    }

    /// Select/copy rendered text and return its cluster-safe rectangles. The
    /// requested UTF-16 range may select part of a ligature; its highlight covers
    /// the cluster while the copied text is exactly the selected logical text.
    pub fn select_item_json(&self, revision: u64, layout_revision: u64, item: usize,
        start: usize, end: usize) -> Result<String, BrowserFlowError>
    {
        self.check_snapshot(revision, layout_revision)?;
        let Some(DisplayItem::Text(text)) = self.session.display().items().get(item) else {
            return Err(BrowserFlowError::InvalidSelection);
        };
        let range = utf16_span(&text.text, start, end)?;
        wire::selection(self, text, item, range)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn session(source: &str) -> BrowserFlowSession {
        BrowserFlowSession::new(source, "sans", FlowLayoutOptions::default()).unwrap()
    }

    #[test]
    fn utf16_offsets_reject_surrogate_interiors_and_preserve_byte_domains() {
        assert_eq!(utf16_span("a😀éb", 1, 3).unwrap(), SourceSpan::new(1, 5));
        assert_eq!(utf16_span("a😀éb", 3, 4).unwrap(), SourceSpan::new(5, 7));
        for (a, b) in [(1, 2), (2, 3), (3, 2), (0, 99)] {
            assert!(utf16_span("a😀éb", a, b).is_err());
        }
    }

    #[test]
    fn edits_are_revision_fenced_and_failed_layout_preserves_source() {
        let mut state = session("café");
        state.edit_utf16(1, 3, 4, "e", FlowAssetReuse::Invalidate).unwrap();
        assert_eq!(state.source(), "cafe");
        assert_eq!(state.revision(), 2);
        assert_eq!(state.edit_utf16(1, 0, 0, "x", FlowAssetReuse::Invalidate).unwrap_err().code(), "STALE_REVISION");
        let before = state.snapshot_json(2, state.layout_revision(), 0, 100, true).unwrap();
        assert!(state.replace_source(2, "😀", FlowAssetReuse::Invalidate).is_err());
        assert_eq!(state.source(), "cafe");
        let after = state.snapshot_json(2, state.layout_revision(), 0, 100, true).unwrap();
        // Cache diagnostics may warm on a failed transaction; document output may not.
        assert_eq!(before.split("\"cache\"").next(), after.split("\"cache\"").next());
    }

    #[test]
    fn resize_keeps_document_identity_but_fences_old_geometry() {
        let mut state = session("one two three four");
        state.reflow(1, 1, FlowLayoutOptions { viewport_width: 80.0, ..FlowLayoutOptions::default() }).unwrap();
        assert_eq!(state.revision(), 1);
        assert_eq!(state.layout_revision(), 2);
        assert_eq!(state.hit_test_json(1, 1, 1.0, 1.0).unwrap_err().code(), "STALE_LAYOUT");
        assert!(state.hit_test_json(1, 2, 1.0, 1.0).unwrap().contains("fragment-utf16"));
        assert!(state.reflow(1, 1, FlowLayoutOptions::default()).is_err());
    }

    #[test]
    fn paged_snapshots_and_source_copy_keep_domains_explicit() {
        let state = session("# Title\n\n[link](https://example.com) **bold**");
        let first = state.snapshot_json(1, 1, 0, 1, true).unwrap();
        assert!(first.contains("\"nextOffset\":1"));
        assert!(first.contains("\"enclosingSourceSpan\""));
        assert!(first.contains("\"glyphs\""));
        assert!(state.snapshot_json(1, 1, 0, 0, false).is_err());
        assert!(state.snapshot_json(1, 1, usize::MAX, 1, false).is_err());
        assert_eq!(state.copy_source(1, SourceSpan::new(0, 7)).unwrap(), "# Title");
        assert!(state.copy_source(2, SourceSpan::new(0, 7)).is_err());
    }

    #[test]
    fn asset_completion_resize_and_reload_preserve_request_fences() {
        let mut state = session("![image](local.png)");
        let requests = state.pending_assets_json(1, 0, 10).unwrap();
        assert!(requests.contains("\"id\":\"1\""));
        state.provide_asset(AssetResult { request_id: AssetRequestId(1), generation: 1,
            width: 200, height: 100, bytes: Some(vec![1, 2, 3]) }).unwrap();
        assert_eq!(state.asset_bytes(1, 1).unwrap(), Some(&[1, 2, 3][..]));
        assert_eq!(state.revision(), 1);
        assert_eq!(state.layout_revision(), 2);
        state.reload_assets(1).unwrap();
        assert_eq!(state.revision(), 2);
        assert!(state.asset_bytes(1, 1).is_err());
        assert!(state.pending_assets_json(2, 0, 10).unwrap().contains("\"generation\":\"2\""));
    }

    #[test]
    fn identities_preserve_u64_precision_without_numeric_coercion() {
        for value in ["0", "1", "9007199254740993", "18446744073709551615"] {
            assert_eq!(parse_identity(value).unwrap().to_string(), value);
        }
        for value in ["", "01", "+1", " 1", "1.0", "18446744073709551616"] {
            assert!(parse_identity(value).is_err());
        }
    }
}
