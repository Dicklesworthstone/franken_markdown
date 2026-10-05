//! Session-owned syntax paint. Every edit, reflow and asset transaction uses
//! the same mode; failed paint admission cannot publish an uncolored fallback.

use super::{
    DisplayList, FlowInlineStyle, FlowLayoutOptions, FlowSession, FlowSessionError, FlowTextRole,
    OwnedTextRun, ResumableFlowDisplay, next_revision,
};

impl FlowSession {
    /// Whether successful session displays include shared fenced-code colors.
    /// New sessions keep the historical unhighlighted mode until enabled.
    #[must_use]
    pub const fn code_highlighting(&self) -> bool {
        self.code_highlighting
    }

    /// Change syntax paint atomically without reparsing or invalidating assets.
    ///
    /// A changed mode increments only the layout revision: item identities and
    /// fragment-local selection offsets can change even when metrics do not.
    /// Source, document revision, accepted assets and pending requests survive.
    /// Equal mode is a true no-op, including at the last possible revision.
    ///
    /// Highlighting uses the existing per-fence/total lexer and output budgets.
    /// Admission or shaping failure preserves the old mode, display and token.
    /// Later source edits, resizes and asset completions retain this mode and
    /// obey the same failure contract, never silently dropping syntax colors.
    pub fn set_code_highlighting<F>(
        &mut self,
        enabled: bool,
        shape: F,
    ) -> Result<(), FlowSessionError>
    where
        F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        if enabled == self.code_highlighting {
            return Ok(());
        }
        let revision = next_revision(self.layout_revision)?;
        let display = if enabled {
            self.engine
                .to_highlighted_display_list(self.options, shape)?
        } else {
            self.engine.to_styled_display_list(self.options, shape)?
        };
        self.display = display;
        self.code_highlighting = enabled;
        self.layout_revision = revision;
        Ok(())
    }

    // All session publication paths must pass here. Initial construction alone
    // uses the old renderer directly because its mode is always false.
    pub(super) fn render_candidate<F>(
        &self,
        engine: &ResumableFlowDisplay,
        options: FlowLayoutOptions,
        shape: F,
    ) -> Result<DisplayList, FlowSessionError>
    where
        F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        let display = if self.code_highlighting {
            engine.to_highlighted_display_list(options, shape)?
        } else {
            engine.to_styled_display_list(options, shape)?
        };
        Ok(display)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::FontFamily;
    use crate::dep_invalidation::FlowAssetReuse;
    use crate::display::DisplayItem;
    use crate::flow_display::{AssetResult, FlowDisplayLimits};
    use crate::fonts::BundledFlowFonts;

    const SOURCE: &str = "```rust\nlet value = 42;\n```\n\n![a](a.png)\n\n![b](b.png)";

    fn shape(
        fonts: &BundledFlowFonts,
    ) -> impl FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String> + '_
    {
        move |t, s, r, i| fonts.shape(t, s, r, i)
    }

    fn session(fonts: &BundledFlowFonts) -> FlowSession {
        FlowSession::new(
            SOURCE,
            2,
            FlowDisplayLimits::default(),
            FlowLayoutOptions::default(),
            shape(fonts),
        )
        .unwrap()
    }

    fn colored(session: &FlowSession) -> bool {
        session
            .display()
            .items()
            .iter()
            .any(|item| matches!(item, DisplayItem::Text(run) if run.color_role == "tok-kw"))
    }

    #[test]
    fn toggling_preserves_source_assets_and_reading_but_fences_item_identity() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut doc = session(&fonts);
        let before = doc.display().clone();
        let pending = doc.pending_assets().to_vec();
        assert!(!doc.code_highlighting());
        assert!(!colored(&doc));
        doc.set_code_highlighting(true, shape(&fonts)).unwrap();
        assert!(colored(&doc));
        assert_eq!(doc.source(), SOURCE);
        assert_eq!(doc.pending_assets(), pending.as_slice());
        assert_eq!(doc.display().reading_order(), before.reading_order());
        assert_eq!((doc.revision(), doc.layout_revision()), (1, 2));
        doc.set_code_highlighting(true, |_, _, _, _| panic!("same mode must not shape"))
            .unwrap();
        assert_eq!(doc.layout_revision(), 2);
        doc.set_code_highlighting(false, shape(&fonts)).unwrap();
        assert_eq!(doc.display(), &before);
        assert_eq!((doc.revision(), doc.layout_revision()), (1, 3));
    }

    #[test]
    fn editing_resizing_and_batched_asset_publication_keep_the_mode() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut doc = session(&fonts);
        doc.set_code_highlighting(true, shape(&fonts)).unwrap();
        let payloads = doc
            .pending_assets()
            .iter()
            .map(|request| AssetResult {
                request_id: request.id,
                generation: request.generation,
                width: 40,
                height: 20,
                bytes: Some(vec![1, 2]),
            })
            .collect();
        doc.provide_assets(payloads, shape(&fonts)).unwrap();
        assert!(colored(&doc));
        let source = SOURCE.replace("42", "123");
        let update = doc
            .replace_source(
                1,
                &source,
                FlowAssetReuse::HostVerifiedUnchanged,
                shape(&fonts),
            )
            .unwrap();
        assert_eq!(update.reused_assets.len(), 2);
        assert!(colored(&doc));
        assert_eq!(doc.engine().retained_asset_bytes(), 4);
        let options = FlowLayoutOptions {
            viewport_width: 80.0,
            ..doc.layout_options()
        };
        doc.reflow(options, shape(&fonts)).unwrap();
        assert!(colored(&doc));
        doc.reload_assets(doc.revision(), shape(&fonts)).unwrap();
        assert_eq!(doc.pending_assets().len(), 2);
        assert!(colored(&doc));
        assert!(doc.code_highlighting());
        let expected = doc
            .engine()
            .to_highlighted_display_list(doc.layout_options(), shape(&fonts))
            .unwrap();
        assert_eq!(doc.display(), &expected);
    }

    #[test]
    fn failed_toggle_and_edit_preserve_last_successful_mode_and_snapshot() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut doc = session(&fonts);
        let before = doc.display().clone();
        assert!(
            doc.set_code_highlighting(true, |_, _, _, _| Err("unavailable".into()))
                .is_err()
        );
        assert!(!doc.code_highlighting());
        assert_eq!(doc.display(), &before);
        assert_eq!(doc.layout_revision(), 1);
        doc.set_code_highlighting(true, shape(&fonts)).unwrap();
        let before = doc.display().clone();
        assert!(
            doc.replace_source(
                1,
                &SOURCE.replace("42", "7"),
                FlowAssetReuse::Invalidate,
                |_, _, _, _| Err("unavailable".into()),
            )
            .is_err()
        );
        assert!(doc.code_highlighting());
        assert_eq!(doc.display(), &before);
        assert_eq!(doc.source(), SOURCE);
        assert_eq!((doc.revision(), doc.layout_revision()), (1, 2));
    }

    #[test]
    fn lexer_budget_failure_is_atomic_and_explicit_disable_allows_recovery() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut doc = session(&fonts);
        let options = FlowLayoutOptions {
            max_items: 64,
            viewport_width: 10_000.0,
            ..doc.layout_options()
        };
        doc.reflow(options, shape(&fonts)).unwrap();
        doc.set_code_highlighting(true, shape(&fonts)).unwrap();
        let before = doc.display().clone();
        let revision = doc.layout_revision();
        let source = format!("```rust\n{}\n```", "x".repeat(200));
        assert!(
            doc.replace_source(1, &source, FlowAssetReuse::Invalidate, shape(&fonts))
                .is_err()
        );
        assert_eq!(doc.layout_revision(), revision);
        assert_eq!(doc.display(), &before);
        assert!(doc.code_highlighting());
        doc.set_code_highlighting(false, shape(&fonts)).unwrap();
        doc.replace_source(1, &source, FlowAssetReuse::Invalidate, shape(&fonts))
            .unwrap();
        assert_eq!(doc.source(), source);
        assert!(!colored(&doc));
    }

    #[test]
    fn mode_noop_at_revision_limit_does_not_mask_a_real_overflow() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut doc = session(&fonts);
        let before = doc.display().clone();
        doc.layout_revision = u64::MAX;
        doc.set_code_highlighting(false, |_, _, _, _| panic!("no-op"))
            .unwrap();
        assert_eq!(
            doc.set_code_highlighting(true, |_, _, _, _| panic!("overflow")),
            Err(FlowSessionError::RevisionExhausted)
        );
        assert!(!doc.code_highlighting());
        assert_eq!(doc.display(), &before);
    }
}
