//! One publication and one layout pass for a group of host-authorized images.
//! Resource validation remains in ResumableFlowDisplay; this module only owns
//! the editor transaction. No asset is fetched, decoded or authorized here.

use super::{
    AssetResult, FlowDisplayError, FlowInlineStyle, FlowSession, FlowSessionError,
    FlowTextRole, OwnedTextRun, completed, next_revision,
};
use std::collections::HashSet;

impl FlowSession {
    /// Complete several pending assets as one document transaction.
    ///
    /// All IDs must be distinct, pending, and from the current source/resource
    /// generation. IDs are checked before reparsing. The candidate applies the
    /// same per-image, dimension and total retained-byte limits as single-image
    /// completion, including assets already retained by this session.
    ///
    /// A nonempty successful batch reparses once, shapes once, and increments
    /// the layout revision exactly once. Source revision and other pending
    /// requests stay valid. Empty input is a no-op, even at the final revision.
    /// Duplicate IDs are reported as UnknownAssetRequest: a request may only be
    /// consumed once. The input order is retained in the accepted inventory;
    /// document drawing order remains the source order.
    ///
    /// Any stale/unknown request, invalid payload, budget failure or shaping
    /// error rejects the WHOLE batch. No prefix is published or consumed, so a
    /// host retaining its own payloads may retry. Callback side effects remain
    /// host-owned, just as with edit/reflow. Parsing is still whole-document,
    /// not incremental; batching avoids repeating it for every image.
    pub fn provide_assets<F>(
        &mut self, results: Vec<AssetResult>, shape: F,
    ) -> Result<(), FlowSessionError>
    where
        F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        if results.is_empty() { return Ok(()); }
        // The set is bounded by the already-admitted pending inventory, not by
        // caller input. Removing rather than looking up also rejects duplicates.
        let mut remaining: HashSet<_> = self.pending_assets().iter().map(|request| request.id).collect();
        for result in &results {
            if result.generation != self.revision() {
                return Err(FlowDisplayError::StaleAssetGeneration {
                    expected: self.revision(), actual: result.generation,
                }.into());
            }
            if !remaining.remove(&result.request_id) {
                return Err(FlowDisplayError::UnknownAssetRequest(result.request_id).into());
            }
        }
        let revision = next_revision(self.layout_revision)?;
        let mut candidate = completed(self.source(), self.engine.batch_size(),
            self.revision(), self.engine.limits())?;
        // Previously retained payloads are copied once for the entire batch,
        // never once per result. Nothing below mutates the published engine.
        for accepted in self.engine.resolved_assets() { candidate.provide_asset(accepted.clone())?; }
        for result in results { candidate.provide_asset(result)?; }
        let display = candidate.to_styled_display_list(self.options, shape)?;
        self.engine = candidate;
        self.display = display;
        self.layout_revision = revision;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::flow_display::{AssetRequest, AssetRequestId, FlowDisplayLimits, FlowLayoutOptions};
    use crate::fonts::BundledFlowFonts;
    use crate::FontFamily;

    const SOURCE: &str = "before\n\n![one](a.png)\n\n![two](b.png)\n\n![three](c.png)\n\nafter";

    fn session(limits: FlowDisplayLimits, fonts: &BundledFlowFonts) -> FlowSession {
        FlowSession::new(SOURCE, 2, limits, FlowLayoutOptions::default(),
            |text, size, role, style| fonts.shape(text, size, role, style)).unwrap()
    }
    fn payload(request: &AssetRequest, width: u32) -> AssetResult {
        AssetResult { request_id: request.id, generation: request.generation,
            width, height: width / 2, bytes: Some(vec![1, 2]) }
    }
    fn fonts() -> BundledFlowFonts { BundledFlowFonts::new(FontFamily::Sans).unwrap() }

    #[test]
    fn batch_has_sequential_geometry_but_only_one_shaping_pass_and_revision() {
        let fonts = fonts();
        let mut batched = session(FlowDisplayLimits::default(), &fonts);
        let mut sequential = session(FlowDisplayLimits::default(), &fonts);
        let results: Vec<_> = batched.pending_assets().iter().enumerate()
            .map(|(i, request)| payload(request, 20 + i as u32 * 10)).collect();
        let mut batch_calls = 0;
        batched.provide_assets(results.clone(), |text, size, role, style| {
            batch_calls += 1;
            fonts.shape(text, size, role, style)
        }).unwrap();
        let mut sequential_calls = 0;
        for result in results {
            sequential.provide_asset(result, |text, size, role, style| {
                sequential_calls += 1;
                fonts.shape(text, size, role, style)
            }).unwrap();
        }
        assert!(batch_calls > 0);
        assert_eq!(sequential_calls, 3 * batch_calls);
        assert_eq!(batched.display(), sequential.display());
        assert_eq!(batched.engine().resolved_assets(), sequential.engine().resolved_assets());
        assert_eq!(batched.engine().retained_asset_bytes(), 6);
        assert!(batched.pending_assets().is_empty());
        assert_eq!((batched.revision(), batched.layout_revision()), (1, 2));
        assert_eq!((sequential.revision(), sequential.layout_revision()), (1, 4));
        assert_eq!(batched.source(), SOURCE);
    }

    #[test]
    fn partial_batches_preserve_other_inflight_requests_and_prior_payloads() {
        let fonts = fonts();
        let mut doc = session(FlowDisplayLimits::default(), &fonts);
        let requests = doc.pending_assets().to_vec();
        doc.provide_asset(payload(&requests[1], 30), |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        let mut dimension_only = payload(&requests[2], 40);
        dimension_only.bytes = None;
        doc.provide_assets(vec![dimension_only], |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(doc.pending_assets(), &requests[..1]);
        assert_eq!(doc.engine().resolved_assets()[0], payload(&requests[1], 30));
        assert_eq!(doc.engine().resolved_assets()[1].bytes, None);
        assert_eq!(doc.engine().retained_asset_bytes(), 2);
        doc.provide_assets(vec![payload(&requests[0], 20)], |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!((doc.revision(), doc.layout_revision()), (1, 4));
        assert!(doc.pending_assets().is_empty());
    }

    #[test]
    fn bad_tail_requests_reject_the_batch_before_calling_the_shaper() {
        let fonts = fonts();
        for failure in 0..3 {
            let mut doc = session(FlowDisplayLimits::default(), &fonts);
            let before = doc.display().clone();
            let requests = doc.pending_assets().to_vec();
            let first = payload(&requests[0], 20);
            let mut bad = payload(&requests[1], 30);
            match failure {
                0 => bad.generation += 1,
                1 => bad.request_id = AssetRequestId(u64::MAX),
                _ => bad.request_id = first.request_id,
            }
            let error = doc.provide_assets(vec![first.clone(), bad], |_, _, _, _| {
                panic!("identity admission must precede shaping")
            }).unwrap_err();
            assert!(matches!(error, FlowSessionError::Input(
                FlowDisplayError::StaleAssetGeneration { .. } | FlowDisplayError::UnknownAssetRequest(_))));
            assert_eq!(doc.display(), &before);
            assert_eq!(doc.pending_assets(), requests.as_slice());
            assert!(doc.engine().resolved_assets().is_empty());
            assert_eq!((doc.revision(), doc.layout_revision()), (1, 1));
            doc.provide_assets(vec![first, payload(&requests[1], 30)],
                |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        }
    }

    #[test]
    fn late_dimension_and_byte_errors_leave_all_assets_reusable() {
        let fonts = fonts();
        for failure in 0..3 {
            let limits = FlowDisplayLimits {
                max_asset_bytes: 3, max_retained_asset_bytes: 5, ..FlowDisplayLimits::default()
            };
            let mut doc = session(limits, &fonts);
            let requests = doc.pending_assets().to_vec();
            doc.provide_asset(payload(&requests[0], 20), |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
            let before = doc.display().clone();
            let accepted = doc.engine().resolved_assets().to_vec();
            let pending = doc.pending_assets().to_vec();
            let mut tail = payload(&requests[2], 40);
            match failure {
                0 => tail.width = 0,
                1 => tail.bytes = Some(vec![0; 4]),
                _ => {} // 2 already retained + 2 + 2 exceeds the total 5.
            }
            assert!(doc.provide_assets(vec![payload(&requests[1], 30), tail],
                |_, _, _, _| panic!("payload admission must precede shaping")).is_err());
            assert_eq!(doc.pending_assets(), pending.as_slice());
            assert_eq!(doc.engine().resolved_assets(), accepted.as_slice());
            assert_eq!(doc.engine().retained_asset_bytes(), 2);
            assert_eq!(doc.display(), &before);
            assert_eq!((doc.revision(), doc.layout_revision()), (1, 2));
        }
    }

    #[test]
    fn late_shaping_failure_and_revision_overflow_publish_no_prefix() {
        let fonts = fonts();
        let mut doc = session(FlowDisplayLimits::default(), &fonts);
        let requests = doc.pending_assets().to_vec();
        let before = doc.display().clone();
        let results: Vec<_> = requests.iter().map(|request| payload(request, 20)).collect();
        let mut calls = 0;
        assert!(doc.provide_assets(results.clone(), |t, s, r, i| {
            calls += 1;
            if calls == 2 { Err("late unavailable face".to_owned()) }
            else { fonts.shape(t, s, r, i) }
        }).is_err());
        assert_eq!(calls, 2);
        assert_eq!(doc.display(), &before);
        assert_eq!(doc.pending_assets(), requests.as_slice());
        assert_eq!(doc.engine().retained_asset_bytes(), 0);
        assert_eq!(doc.layout_revision(), 1);
        doc.layout_revision = u64::MAX;
        assert!(matches!(doc.provide_assets(results, |_, _, _, _| panic!("revision overflow")),
            Err(FlowSessionError::RevisionExhausted)));
        assert_eq!(doc.display(), &before);
        assert_eq!(doc.pending_assets(), requests.as_slice());
    }

    #[test]
    fn empty_batch_is_a_true_noop_even_at_exhausted_revision() {
        let fonts = fonts();
        let mut doc = session(FlowDisplayLimits::default(), &fonts);
        doc.layout_revision = u64::MAX;
        let before = doc.display().clone();
        let requests = doc.pending_assets().to_vec();
        doc.provide_assets(Vec::new(), |_, _, _, _| panic!("no-op must not shape")).unwrap();
        assert_eq!(doc.layout_revision(), u64::MAX);
        assert_eq!(doc.display(), &before);
        assert_eq!(doc.pending_assets(), requests.as_slice());
    }

    #[test]
    fn arrival_order_does_not_change_drawing_order_or_source_spans() {
        let fonts = fonts();
        let mut first = session(FlowDisplayLimits::default(), &fonts);
        let mut second = session(FlowDisplayLimits::default(), &fonts);
        let mut results: Vec<_> = first.pending_assets().iter().enumerate()
            .map(|(i, request)| payload(request, 20 + i as u32 * 10)).collect();
        first.provide_assets(results.clone(), |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        results.reverse();
        second.provide_assets(results, |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(first.display(), second.display());
        for item in second.display().items() { assert!(item.source_span().slice(SOURCE).is_some()); }
        assert_eq!(second.engine().resolved_assets()[0].request_id, first.engine().resolved_assets()[2].request_id);
    }
}
