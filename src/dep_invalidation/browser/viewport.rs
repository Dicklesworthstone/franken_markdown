//! Indexed browser pages use the exact existing snapshot item encoder. The
//! only per-item addition is an effective clip; original item IDs stay intact.

use super::{BrowserFlowError, BrowserFlowSession, display_item, encode, header};
use crate::display::{DisplayQueryError, DisplayQueryMode, DisplayRect};
use crate::flow_display::FlowLayoutError;
use std::fmt::Write;

pub(super) fn query_error(error: DisplayQueryError) -> BrowserFlowError {
    match error {
        DisplayQueryError::InvalidCursor | DisplayQueryError::InvalidLimit => BrowserFlowError::InvalidPage,
        DisplayQueryError::InvalidViewport => FlowLayoutError::InvalidOptions.into(),
        DisplayQueryError::ItemBudgetExceeded => FlowLayoutError::BudgetExceeded("display index items").into(),
        DisplayQueryError::ClipDepthExceeded { .. } => FlowLayoutError::BudgetExceeded("active display clips").into(),
        other => FlowLayoutError::Shaping(format!("invalid display geometry: {other}")).into(),
    }
}

impl BrowserFlowSession {
    /// Retrieve a bounded spatial page under one source/layout token. The
    /// cursor is an original display item index, not a visible-item ordinal.
    /// Reuse the same viewport and token until `nextIndex` is null.
    ///
    /// Text-line context includes horizontally offscreen/clipped text so a
    /// Canvas consumer can compute stable shared baselines. Every returned
    /// primitive carries `effectiveClip`; clip commands are not returned.
    /// The existing full-inventory snapshot and accessibility APIs are unchanged.
    ///
    /// Index construction is lazy and linear; subsequent vertical queries skip
    /// offscreen subtrees. Edits/reflow replace the list and invalidate the cache.
    /// Serialization uses the existing 16 MiB pre-append output budget.
    pub fn viewport_json(
        &self, revision: u64, layout_revision: u64, viewport: DisplayRect,
        after: usize, limit: usize, glyphs: bool,
    ) -> Result<String, BrowserFlowError> {
        self.check_snapshot(revision, layout_revision)?;
        let display = self.session.display();
        let page = display.viewport_page(viewport, after, limit, DisplayQueryMode::TextLines)
            .map_err(query_error)?;
        encode(|w| {
            header(w, self)?;
            w.write_str(",\"queryKind\":\"viewport-v1\",\"shapingProfile\":\"bundled-simple-ltr\",\"viewport\":")?;
            w.rect(viewport)?;
            w.write_str(",\"totalBounds\":")?; w.rect(display.total_bounds())?;
            write!(w, ",\"afterIndex\":{},\"total\":{},\"visitedEntries\":{},\"nextIndex\":",
                after, page.total_items, page.visited_entries)?;
            if let Some(next) = page.next_index { write!(w, "{next}")?; } else { w.write_str("null")?; }
            w.write_str(",\"items\":[")?;
            for (position, entry) in page.items.iter().enumerate() {
                if position != 0 { w.write_char(',')?; }
                display_item(w, entry.index, entry.item, glyphs, Some(entry.clip))?;
            }
            w.write_str("]}")
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::display::{DisplayItem, DisplayTextRun};
    use crate::flow_display::FlowLayoutOptions;
    use crate::SourceSpan;

    #[test]
    fn viewport_payload_keeps_the_exact_existing_item_encoding() {
        let item = DisplayItem::Text(DisplayTextRun {
            bounds: DisplayRect::new(0.0, 0.0, 10.0, 20.0), text: "é\n\"x\"".into(),
            font_run: None, color_role: "text".into(), source_span: SourceSpan::new(7, 19), font_size: 14.0,
        });
        let plain = encode(|w| display_item(w, 42, &item, true, None)).unwrap();
        let clipped = encode(|w| display_item(w, 42, &item, true, Some(DisplayRect::new(1.0, 2.0, 3.0, 4.0)))).unwrap();
        let expected = format!("{},\"effectiveClip\":{{\"x\":1,\"y\":2,\"width\":3,\"height\":4}}}}", &plain[..plain.len() - 1]);
        assert_eq!(clipped, expected);
    }

    #[test]
    fn browser_pages_fence_reflow_and_invalid_queries() {
        let mut session = BrowserFlowSession::new("# Title\n\nText\n", "sans", FlowLayoutOptions::default()).unwrap();
        let revision = session.revision();
        let layout = session.layout_revision();
        let view = DisplayRect::new(0.0, 0.0, 800.0, 600.0);
        let page = session.viewport_json(revision, layout, view, 0, 1, true).unwrap();
        assert!(page.contains("\"queryKind\":\"viewport-v1\""));
        assert!(page.contains("\"effectiveClip\":"));
        assert!(page.contains("\"nextIndex\":1"));
        assert_eq!(session.viewport_json(revision, layout, view, usize::MAX, 1, false).unwrap_err().code(), "INVALID_PAGE");
        assert_eq!(session.viewport_json(revision, layout, view, 0, 0, false).unwrap_err().code(), "INVALID_PAGE");
        let bad = DisplayRect::new(0.0, f32::NAN, 1.0, 1.0);
        assert_eq!(session.viewport_json(revision, layout, bad, 0, 1, false).unwrap_err().code(), "INVALID_OPTIONS");
        session.reflow(revision, layout, FlowLayoutOptions { viewport_width: 300.0, ..FlowLayoutOptions::default() }).unwrap();
        assert_eq!(session.viewport_json(revision, layout, view, 0, 1, false).unwrap_err().code(), "STALE_LAYOUT");
    }

    #[test]
    fn viewport_at_document_end_is_small_and_does_not_serialize_the_prefix() {
        let source = "A paragraph.\n\n".repeat(1000);
        let session = BrowserFlowSession::new(&source, "sans", FlowLayoutOptions::default()).unwrap();
        let display = session.session.display();
        let last = display.items().last().unwrap().bounds();
        let view = DisplayRect::new(0.0, last.y, 800.0, last.height);
        let expected = display.viewport_page(view, 0, 256, DisplayQueryMode::TextLines).unwrap();
        assert!(expected.visited_entries <= 64);
        let page = session.viewport_json(session.revision(), session.layout_revision(), view, 0, 256, true).unwrap();
        assert!(!page.contains("{\"index\":0,\"bounds\":"));
        assert!(page.contains(&format!("\"index\":{},", expected.items[0].index)));
        assert!(page.contains("\"nextIndex\":null"));
        assert!(page.len() < 16 * 1024);
    }
}
