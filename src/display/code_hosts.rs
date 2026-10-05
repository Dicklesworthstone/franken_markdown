//! Opt-in bundled-font and cached-shaper entrypoints. The existing render
//! methods remain unchanged; paint is never retained in the shaping cache.

use super::{DisplayList, FlowLayoutError, FlowLayoutOptions, ResumableFlowDisplay};
use crate::dep_invalidation::FlowShapeCache;
use crate::fonts::BundledFlowFonts;

impl BundledFlowFonts {
    /// Render styled flow with whole-fence syntax colors and bundled glyphs.
    ///
    /// Uses [`ResumableFlowDisplay::to_highlighted_display_list`], including its
    /// lexer and output admission limits. The engine must first be advanced with
    /// `step` or `process_all`. Unknown languages retain ordinary code colors.
    pub fn render_highlighted(
        &self,
        engine: &ResumableFlowDisplay,
        options: FlowLayoutOptions,
    ) -> Result<DisplayList, FlowLayoutError> {
        engine.to_highlighted_display_list(options, |text, size, role, style| {
            self.shape(text, size, role, style)
        })
    }
}

impl FlowShapeCache {
    /// Render syntax-colored flow while reusing only exact-key shaped runs.
    ///
    /// Lexer colors and source positions are recomputed for the current engine;
    /// they never become part of cached font output. Admission or paint failures
    /// publish no display list, although successful shaping may warm the cache.
    /// See [`ResumableFlowDisplay::to_highlighted_display_list`] for budgets.
    pub fn render_highlighted(
        &mut self,
        engine: &ResumableFlowDisplay,
        options: FlowLayoutOptions,
    ) -> Result<DisplayList, FlowLayoutError> {
        engine.to_highlighted_display_list(options, |text, size, role, style| {
            self.shape(text, size, role, style)
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::FontFamily;
    use crate::dep_invalidation::FlowShapeCacheLimits;
    use crate::display::DisplayItem;

    fn engine(source: &str) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, 1);
        engine.process_all().unwrap();
        engine
    }

    fn has_keyword(display: &DisplayList) -> bool {
        display
            .items()
            .iter()
            .any(|item| matches!(item, DisplayItem::Text(text) if text.color_role == "tok-kw"))
    }

    #[test]
    fn bundled_entrypoint_is_opt_in_and_matches_the_callback_api() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let engine = engine("```rust\nlet x = 42;\n```");
        let options = FlowLayoutOptions::default();
        let plain = fonts.render(&engine, options).unwrap();
        let highlighted = fonts.render_highlighted(&engine, options).unwrap();
        let explicit = engine
            .to_highlighted_display_list(options, |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })
            .unwrap();
        assert!(!has_keyword(&plain));
        assert!(has_keyword(&highlighted));
        assert_eq!(highlighted, explicit);
        assert_eq!(plain.reading_order(), highlighted.reading_order());
    }

    #[test]
    fn cached_entrypoint_reuses_metrics_without_caching_fence_colors() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut cache = FlowShapeCache::with_fonts(fonts, FlowShapeCacheLimits::default());
        let rust = engine("```rust\nlet x = 42;\n```");
        let plain = engine("```text\nlet x = 42;\n```");
        let options = FlowLayoutOptions::default();
        let highlighted = cache.render_highlighted(&rust, options).unwrap();
        let before = cache.stats();
        assert_eq!(
            cache.render_highlighted(&rust, options).unwrap(),
            highlighted
        );
        let ordinary = cache.render_highlighted(&plain, options).unwrap();
        let after = cache.stats();
        assert!(after.hits > before.hits);
        assert_eq!(after.misses, before.misses);
        assert!(has_keyword(&highlighted));
        assert!(!has_keyword(&ordinary));
        assert_eq!(
            cache.render(&rust, options).unwrap().reading_order(),
            highlighted.reading_order()
        );
    }
}
