//! Revision-fenced syntax paint for browser sessions. The mode belongs to the
//! transactional native session, not to a cached snapshot or JavaScript flag.

use super::{BrowserFlowError, BrowserFlowSession};

impl BrowserFlowSession {
    #[must_use]
    pub fn code_highlighting(&self) -> bool {
        self.session.code_highlighting()
    }

    /// Change code colors without replacing source, fonts or asset authority.
    /// Both source and layout identities are checked even for a no-op request.
    /// Failure leaves the prior display and mode intact. Success with a changed
    /// mode advances only the layout revision, fencing old item selections.
    pub fn set_code_highlighting(
        &mut self,
        revision: u64,
        layout_revision: u64,
        enabled: bool,
    ) -> Result<(), BrowserFlowError> {
        self.check_snapshot(revision, layout_revision)?;
        let fonts = &mut self.fonts;
        self.session
            .set_code_highlighting(enabled, |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::dep_invalidation::FlowAssetReuse;
    use crate::flow_display::{AssetRequestId, AssetResult, FlowLayoutOptions};

    const SOURCE: &str = "```rust\nlet café = 42;\n```\n\n![a](a.png)";

    fn session() -> BrowserFlowSession {
        BrowserFlowSession::new(SOURCE, "sans", FlowLayoutOptions::default()).unwrap()
    }

    fn snapshot(state: &BrowserFlowSession) -> String {
        state
            .snapshot_json(state.revision(), state.layout_revision(), 0, 100, true)
            .unwrap()
    }

    #[test]
    fn mode_change_publishes_real_token_roles_and_rejects_stale_item_reads() {
        let mut state = session();
        assert!(!state.code_highlighting());
        assert!(!snapshot(&state).contains("tok-kw"));
        state.set_code_highlighting(1, 1, true).unwrap();
        assert!(state.code_highlighting());
        let page = snapshot(&state);
        assert!(page.contains("tok-kw"));
        assert!(page.contains("tok-nu"));
        assert!(page.contains("fragment-utf8-and-utf16"));
        assert_eq!(state.source(), SOURCE);
        assert_eq!((state.revision(), state.layout_revision()), (1, 2));
        assert_eq!(
            state.snapshot_json(1, 1, 0, 100, true).unwrap_err().code(),
            "STALE_LAYOUT"
        );
        assert_eq!(
            state.set_code_highlighting(1, 1, true).unwrap_err().code(),
            "STALE_LAYOUT"
        );
        assert_eq!(
            state.set_code_highlighting(0, 2, false).unwrap_err().code(),
            "STALE_REVISION"
        );
        state.set_code_highlighting(1, 2, true).unwrap();
        assert_eq!(state.layout_revision(), 2);
        state.set_code_highlighting(1, 2, false).unwrap();
        assert!(!snapshot(&state).contains("tok-kw"));
    }

    #[test]
    fn mode_survives_edit_asset_completion_and_resize_with_live_font_cache() {
        let mut state = session();
        state.set_code_highlighting(1, 1, true).unwrap();
        state
            .provide_asset(AssetResult {
                request_id: AssetRequestId(1),
                generation: 1,
                width: 40,
                height: 20,
                bytes: Some(vec![1, 2, 3]),
            })
            .unwrap();
        state
            .replace_source(
                1,
                &SOURCE.replace("42", "123"),
                FlowAssetReuse::HostVerifiedUnchanged,
            )
            .unwrap();
        state
            .reflow(
                state.revision(),
                state.layout_revision(),
                FlowLayoutOptions {
                    viewport_width: 100.0,
                    ..FlowLayoutOptions::default()
                },
            )
            .unwrap();
        assert!(state.code_highlighting());
        assert!(snapshot(&state).contains("tok-kw"));
        assert_eq!(state.asset_bytes(1, 2).unwrap(), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn rejected_paint_budget_keeps_native_mode_and_published_output() {
        let options = FlowLayoutOptions {
            max_items: 8,
            ..FlowLayoutOptions::default()
        };
        let mut state = BrowserFlowSession::new("```rust\nlet value = 42;\n```", "sans", options)
            .unwrap();
        let before = snapshot(&state);
        assert_eq!(
            state.set_code_highlighting(1, 1, true).unwrap_err().code(),
            "BUDGET_EXCEEDED"
        );
        assert!(!state.code_highlighting());
        assert_eq!(state.layout_revision(), 1);
        assert_eq!(snapshot(&state), before);
    }
}
