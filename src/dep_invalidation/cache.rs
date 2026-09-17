//! Bounded exact-key reuse of bundled-font shaping across editor reflows.
//!
//! Only pure font runs are cached, never source positions, link destinations,
//! document generations, asset authorizations or absolute layout coordinates.
//! The immutable font registry belongs to this cache; no arbitrary host shaper
//! is memoized without its missing context becoming part of the cache key.

use crate::display::DisplayList;
use crate::flow_display::{
    FlowInlineStyle, FlowLayoutError, FlowLayoutOptions, FlowTextRole, ResumableFlowDisplay,
};
use crate::fonts::BundledFlowFonts;
use crate::text::{FontId, OwnedTextRun, RunGlyph, TextCluster};
use crate::FontFamily;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowShapeCacheLimits {
    /// Bounds entry metadata and the linear exact-key lookup work.
    pub max_entries: usize,
    /// Retained string/vector allocation capacities. Fixed entry/allocator
    /// overhead is separately bounded by max_entries; shared fonts are excluded.
    pub max_payload_bytes: usize,
}

impl Default for FlowShapeCacheLimits {
    fn default() -> Self {
        Self { max_entries: 256, max_payload_bytes: 8 * 1024 * 1024 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlowShapeCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub retained_entries: usize,
    pub retained_payload_bytes: usize,
}

struct Entry {
    text: String,
    size: u32,
    role: (u8, u8),
    style: u8,
    run: OwnedTextRun,
    payload_bytes: usize,
    last_used: u64,
}

/// Reuse expensive bundled GSUB/GPOS work across documents, edits and resizes.
/// Exact text, float size bits, block role and every inline-style flag form the
/// key. The owned immutable font registry fixes all other shaping inputs.
///
/// Hits return an owned copy: mutating a returned run cannot poison the cache.
/// Errors and oversized entries are not retained; disabling cache retention
/// does not disable rendering. A failed document transaction may warm this
/// cache, but cannot cause stale source/link/asset state to be reused.
pub struct FlowShapeCache {
    fonts: BundledFlowFonts,
    limits: FlowShapeCacheLimits,
    entries: Vec<Entry>,
    payload_bytes: usize,
    clock: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl FlowShapeCache {
    pub fn new(family: FontFamily, limits: FlowShapeCacheLimits) -> Result<Self, FlowLayoutError> {
        Ok(Self::with_fonts(BundledFlowFonts::new(family)?, limits))
    }

    #[must_use]
    pub fn with_fonts(fonts: BundledFlowFonts, limits: FlowShapeCacheLimits) -> Self {
        Self { fonts, limits, entries: Vec::new(), payload_bytes: 0, clock: 0, hits: 0, misses: 0, evictions: 0 }
    }

    #[must_use]
    pub fn stats(&self) -> FlowShapeCacheStats {
        FlowShapeCacheStats {
            hits: self.hits, misses: self.misses, evictions: self.evictions,
            retained_entries: self.entries.len(), retained_payload_bytes: self.payload_bytes,
        }
    }

    /// Drop retained text and glyph runs, preserving cumulative diagnostics.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.payload_bytes = 0;
        self.clock = 0;
    }

    #[must_use]
    pub fn font_bytes(&self, id: FontId) -> Option<&'static [u8]> { self.fonts.font_bytes(id) }

    pub fn render(&mut self, engine: &ResumableFlowDisplay, options: FlowLayoutOptions)
        -> Result<DisplayList, FlowLayoutError>
    {
        engine.to_styled_display_list(options, |text, size, role, style| self.shape(text, size, role, style))
    }

    pub fn shape(&mut self, text: &str, size: f32, role: FlowTextRole, style: FlowInlineStyle)
        -> Result<OwnedTextRun, String>
    {
        let role_key = role_key(role);
        let style_key = style_key(style);
        if self.clock == u64::MAX { self.clear(); }
        self.clock += 1;
        // Borrowed exact-key lookup: no copy of a large rejected input, and no
        // temporary String allocation on a hit. Cardinality is policy-bounded.
        if let Some(entry) = self.entries.iter_mut().find(|entry| {
            entry.size == size.to_bits() && entry.role == role_key
                && entry.style == style_key && entry.text == text
        }) {
            self.hits = self.hits.saturating_add(1);
            entry.last_used = self.clock;
            return Ok(entry.run.clone());
        }
        self.misses = self.misses.saturating_add(1);
        let run = self.fonts.shape(text, size, role, style)?;
        if self.limits.max_entries == 0 || self.limits.max_payload_bytes == 0 {
            return Ok(run);
        }
        // Check a lower bound before allocating the retained copy. Then charge
        // the actual clone capacities, not just its logical payload lengths.
        let minimum = payload_size(text.len(), run.logical_text.len(), run.clusters.len(), run.glyphs.len());
        if minimum.is_none_or(|bytes| bytes > self.limits.max_payload_bytes) { return Ok(run); }
        let saved = run.clone();
        let key = text.to_owned();
        let weight = payload_size(key.capacity(), saved.logical_text.capacity(), saved.clusters.capacity(), saved.glyphs.capacity());
        let Some(weight) = weight.filter(|bytes| *bytes <= self.limits.max_payload_bytes) else { return Ok(run); };
        while self.entries.len() >= self.limits.max_entries
            || self.payload_bytes > self.limits.max_payload_bytes - weight
        {
            let oldest = self.entries.iter().enumerate().min_by_key(|(_, entry)| entry.last_used)
                .map(|(index, _)| index);
            let Some(index) = oldest else { break; };
            let entry = self.entries.swap_remove(index);
            self.payload_bytes -= entry.payload_bytes;
            self.evictions = self.evictions.saturating_add(1);
        }
        self.entries.push(Entry {
            text: key, size: size.to_bits(), role: role_key, style: style_key, run: saved,
            payload_bytes: weight, last_used: self.clock,
        });
        self.payload_bytes += weight;
        Ok(run)
    }
}

fn payload_size(key: usize, text: usize, clusters: usize, glyphs: usize) -> Option<usize> {
    key.checked_add(text)?
        .checked_add(clusters.checked_mul(std::mem::size_of::<TextCluster>())?)?
        .checked_add(glyphs.checked_mul(std::mem::size_of::<RunGlyph>())?)
}

fn role_key(role: FlowTextRole) -> (u8, u8) {
    match role {
        FlowTextRole::Body => (0, 0),
        FlowTextRole::Heading(level) => (1, level),
        FlowTextRole::Code => (2, 0),
        FlowTextRole::Marker => (3, 0),
        FlowTextRole::TableHeader => (4, 0),
        FlowTextRole::TableCell => (5, 0),
    }
}

fn style_key(style: FlowInlineStyle) -> u8 {
    // Exhaustive destructuring forces future style fields to be considered.
    let FlowInlineStyle { bold, italic, code, strikethrough } = style;
    u8::from(bold) | (u8::from(italic) << 1) | (u8::from(code) << 2) | (u8::from(strikethrough) << 3)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::dep_invalidation::{FlowAssetReuse, FlowSession};
    use crate::flow_display::FlowDisplayLimits;

    fn cache(limits: FlowShapeCacheLimits) -> FlowShapeCache {
        FlowShapeCache::new(FontFamily::Sans, limits).unwrap()
    }

    #[test]
    fn warm_and_cold_runs_are_equal_and_every_context_field_is_in_the_key() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut cache = cache(FlowShapeCacheLimits::default());
        for (size, role, style) in [
            (14.0, FlowTextRole::Body, FlowInlineStyle::default()),
            (15.0, FlowTextRole::Body, FlowInlineStyle::default()),
            (14.0, FlowTextRole::Heading(2), FlowInlineStyle::default()),
            (14.0, FlowTextRole::Heading(3), FlowInlineStyle::default()),
            (14.0, FlowTextRole::Code, FlowInlineStyle::default()),
            (14.0, FlowTextRole::Body, FlowInlineStyle { bold: true, ..FlowInlineStyle::default() }),
            (14.0, FlowTextRole::Body, FlowInlineStyle { italic: true, ..FlowInlineStyle::default() }),
            (14.0, FlowTextRole::Body, FlowInlineStyle { code: true, ..FlowInlineStyle::default() }),
            (14.0, FlowTextRole::Body, FlowInlineStyle { strikethrough: true, ..FlowInlineStyle::default() }),
        ] {
            let expected = fonts.shape("office", size, role, style).unwrap();
            let misses = cache.stats().misses;
            assert_eq!(cache.shape("office", size, role, style).unwrap(), expected);
            assert_eq!(cache.stats().misses, misses + 1);
            assert_eq!(cache.shape("office", size, role, style).unwrap(), expected);
            assert_eq!(cache.stats().misses, misses + 1);
        }
        assert_eq!(cache.stats().hits, 9);
        assert_eq!(cache.stats().retained_entries, 9);
    }

    #[test]
    fn least_recently_used_entries_are_evicted_with_bounded_retention() {
        let limits = FlowShapeCacheLimits { max_entries: 2, ..FlowShapeCacheLimits::default() };
        let mut cache = cache(limits);
        for text in ["first", "second", "first", "third"] {
            cache.shape(text, 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        }
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().evictions, 1);
        assert_eq!(cache.stats().retained_entries, 2);
        assert!(cache.stats().retained_payload_bytes <= limits.max_payload_bytes);
        cache.shape("first", 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        assert_eq!(cache.stats().hits, 2);
        cache.shape("second", 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        assert_eq!(cache.stats().misses, 4);
        cache.clear();
        assert_eq!(cache.stats().retained_entries, 0);
        assert_eq!(cache.stats().retained_payload_bytes, 0);
    }

    #[test]
    fn disabled_tiny_and_error_caches_do_not_change_shaping_results() {
        for limits in [
            FlowShapeCacheLimits { max_entries: 0, max_payload_bytes: 1000 },
            FlowShapeCacheLimits { max_entries: 10, max_payload_bytes: 0 },
            FlowShapeCacheLimits { max_entries: 10, max_payload_bytes: 1 },
        ] {
            let mut cache = cache(limits);
            let a = cache.shape("text", 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
            let b = cache.shape("text", 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
            assert_eq!(a, b);
            assert_eq!(cache.stats().hits, 0);
            assert_eq!(cache.stats().retained_payload_bytes, 0);
        }
        let mut cache = cache(FlowShapeCacheLimits::default());
        for _ in 0..2 {
            assert!(cache.shape("😀", 14.0, FlowTextRole::Body, FlowInlineStyle::default()).is_err());
        }
        assert_eq!(cache.stats().misses, 2);
        assert_eq!(cache.stats().retained_entries, 0);
    }

    #[test]
    fn caller_mutation_cannot_poison_a_retained_owned_run() {
        let mut cache = cache(FlowShapeCacheLimits::default());
        let mut a = cache.shape("text", 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        let expected = a.clone();
        a.logical_text.clear();
        a.glyphs.clear();
        a.clusters.clear();
        let b = cache.shape("text", 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        assert_eq!(b, expected);
        assert!(cache.font_bytes(b.context.font_id).is_some());
    }

    #[test]
    fn edited_session_reuses_shaping_but_never_old_link_targets_or_positions() {
        let mut cache = cache(FlowShapeCacheLimits::default());
        let source = "# Guide\n\nFirst office paragraph.\n\n[read][r]\n\n[r]: https://old.example\n";
        let options = FlowLayoutOptions { viewport_width: 220.0, ..FlowLayoutOptions::default() };
        let mut doc = FlowSession::new(source, 4, FlowDisplayLimits::default(), options,
            |t, s, r, i| cache.shape(t, s, r, i)).unwrap();
        let hits = cache.stats().hits;
        let next = source.replace("old.example", "new.example");
        doc.replace_source(1, &next, FlowAssetReuse::Invalidate,
            |t, s, r, i| cache.shape(t, s, r, i)).unwrap();
        assert!(cache.stats().hits > hits);
        let narrower = FlowLayoutOptions { viewport_width: 100.0, ..options };
        doc.reflow(narrower, |t, s, r, i| cache.shape(t, s, r, i)).unwrap();
        let mut cold = ResumableFlowDisplay::new(&next, 4);
        cold.process_all().unwrap();
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        assert_eq!(doc.display(), &fonts.render(&cold, narrower).unwrap());
        assert!(doc.display().anchors().any(|a| a.anchor_id == "https://new.example"));
        assert!(!doc.display().anchors().any(|a| a.anchor_id == "https://old.example"));
    }
}
