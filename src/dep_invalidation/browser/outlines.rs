//! Renderer-neutral glyph paths for the exact bundled face used by flow layout.
//! Uses the shared TrueType decoder; no browser shaping or font discovery.

use super::{BrowserFlowError, BrowserFlowSession, MAX_SNAPSHOT_BYTES};
use crate::flow_display::FlowLayoutError;
use crate::text::{Font, outline::{GlyphOutline, Point, Segment}};
use std::fmt::{self, Write};

const MAX_GLYPHS: usize = 256;
const MAX_COMMANDS: usize = 65_536;

impl BrowserFlowSession {
    /// Decode up to 256 glyph IDs from an immutable session font. Empty requests
    /// return just metrics. Results preserve request order, including duplicates.
    ///
    /// Commands are M/L/Q/Z in baseline-relative, y-up font design units, filled
    /// with the nonzero winding rule. Shaped advances/offsets still come from the
    /// display item, NEVER from an outline's nominal advance. Font metrics let a
    /// presentation backend align different faces on a common line baseline.
    ///
    /// This read does not require a document revision: a FontId identifies the
    /// same immutable face across edits/reflows. Unknown IDs are not substituted.
    /// A batch reparses that face once with the shared font reader; consumers
    /// should cache returned paths by (font ID, glyph ID), independently of layout.
    /// Decode work and serialized output are bounded; failure returns no partial
    /// response and cannot change source, assets, display or revision counters.
    pub fn glyph_outlines_json(&self, font_id: u64, glyph_ids: &[u16])
        -> Result<String, BrowserFlowError>
    {
        if glyph_ids.len() > MAX_GLYPHS {
            return Err(FlowLayoutError::BudgetExceeded("glyph outline batch").into());
        }
        let bytes = self.font_bytes(font_id)?;
        let font = Font::parse(bytes.to_vec())
            .map_err(|error| FlowLayoutError::Shaping(error.to_string()))?;
        if font.units_per_em == 0 || font.ascent <= font.descent {
            return Err(FlowLayoutError::InvalidShapedRun.into());
        }
        let mut writer = Bounded { text: String::new() };
        write!(writer,
            "{{\"schemaVersion\":1,\"fontId\":\"{font_id}\",\"unitsPerEm\":{},\"ascent\":{},\"descent\":{},\"lineGap\":{},\"glyphs\":[",
            font.units_per_em, font.ascent, font.descent, font.line_gap,
        ).map_err(|_| BrowserFlowError::SnapshotTooLarge)?;
        let mut commands = 0usize;
        for (index, &id) in glyph_ids.iter().enumerate() {
            let outline = font.glyph_outline(id)
                .map_err(|error| FlowLayoutError::Shaping(error.to_string()))?;
            for contour in &outline.contours {
                commands = commands.checked_add(contour.segments.len())
                    .and_then(|count| count.checked_add(2))
                    .filter(|count| *count <= MAX_COMMANDS)
                    .ok_or(FlowLayoutError::BudgetExceeded("glyph outline commands"))?;
                validate_point(contour.start)?;
                for segment in &contour.segments {
                    validate_point(segment.to())?;
                    if let Segment::Quad { ctrl, .. } = segment { validate_point(*ctrl)?; }
                }
            }
            if index != 0 { writer.write_char(',').map_err(|_| BrowserFlowError::SnapshotTooLarge)?; }
            write_glyph(&mut writer, id, &outline).map_err(|_| BrowserFlowError::SnapshotTooLarge)?;
        }
        writer.write_str("]}").map_err(|_| BrowserFlowError::SnapshotTooLarge)?;
        Ok(writer.text)
    }
}

fn validate_point(point: Point) -> Result<(), FlowLayoutError> {
    if !point.x.is_finite() || !point.y.is_finite()
        || point.x.abs() > 100_000_000.0 || point.y.abs() > 100_000_000.0
    {
        return Err(FlowLayoutError::InvalidShapedRun);
    }
    Ok(())
}

// Numeric/literal-only JSON. The writer checks BEFORE every append, so a large
// decoded composite cannot allocate an unbounded serialized response.
struct Bounded { text: String }
impl Write for Bounded {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > MAX_SNAPSHOT_BYTES.saturating_sub(self.text.len()) { return Err(fmt::Error); }
        self.text.push_str(value);
        Ok(())
    }
}

fn write_glyph(writer: &mut Bounded, id: u16, outline: &GlyphOutline) -> fmt::Result {
    write!(writer, "{{\"glyphId\":{id},\"commands\":[")?;
    let mut first = true;
    for contour in &outline.contours {
        if !first { writer.write_char(',')?; }
        first = false;
        write!(writer, "[\"M\",{},{}]", contour.start.x, contour.start.y)?;
        for segment in &contour.segments {
            match segment {
                Segment::Line { to } => write!(writer, ",[\"L\",{},{}]", to.x, to.y)?,
                Segment::Quad { ctrl, to } => {
                    write!(writer, ",[\"Q\",{},{},{},{}]", ctrl.x, ctrl.y, to.x, to.y)?;
                }
            }
        }
        writer.write_str(",[\"Z\"]")?;
    }
    writer.write_str("]}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::display::DisplayItem;

    fn session(source: &str) -> BrowserFlowSession {
        BrowserFlowSession::new(source, "sans", Default::default()).unwrap()
    }

    #[test]
    fn paths_decode_the_actual_shaped_font_and_glyph_id() {
        let state = session("office **AV** café");
        for item in state.session.display().items() {
            let DisplayItem::Text(text) = item else { continue; };
            let run = text.font_run.as_ref().unwrap();
            for glyph in &run.glyphs {
                let response = state.glyph_outlines_json(glyph.font_id.0, &[glyph.glyph_id]).unwrap();
                let font = Font::parse(state.font_bytes(glyph.font_id.0).unwrap().to_vec()).unwrap();
                let outline = font.glyph_outline(glyph.glyph_id).unwrap();
                let mut expected = Bounded { text: String::new() };
                write_glyph(&mut expected, glyph.glyph_id, &outline).unwrap();
                assert!(response.contains(&expected.text));
                assert!(response.contains(&format!("\"fontId\":\"{}\"", glyph.font_id.0)));
                assert!(response.contains(&format!("\"unitsPerEm\":{}", font.units_per_em)));
                assert!(!response.contains("NaN"));
            }
        }
    }

    #[test]
    fn spaces_have_empty_paths_and_batch_order_is_preserved() {
        let state = session("a b");
        // Use the emitted immutable run; outlining must not warm the shape cache.
        let run = state.session.display().items().iter().find_map(|item| match item {
            DisplayItem::Text(text) => text.font_run.as_ref(), _ => None,
        }).unwrap();
        let font_id = run.context.font_id.0;
        let font = Font::parse(state.font_bytes(font_id).unwrap().to_vec()).unwrap();
        let space = font.glyph_index(' ');
        let a = font.glyph_index('a');
        let result = state.glyph_outlines_json(font_id, &[space, a, space]).unwrap();
        let blank = format!("{{\"glyphId\":{space},\"commands\":[]}}");
        assert_eq!(result.matches(&blank).count(), 2);
        assert!(result.ends_with(&format!("{blank}]}}")));
        assert!(state.glyph_outlines_json(font_id, &[]).unwrap().ends_with("\"glyphs\":[]}"));
    }

    #[test]
    fn refused_requests_do_not_change_the_session() {
        let state = session("safe");
        let before = state.snapshot_json(1, 1, 0, 100, true).unwrap();
        let id = state.session.display().items().iter().find_map(|item| match item {
            DisplayItem::Text(text) => text.font_run.as_ref().map(|run| run.context.font_id.0),
            _ => None,
        }).unwrap();
        assert_eq!(state.glyph_outlines_json(0, &[1]).unwrap_err().code(), "UNKNOWN_FONT");
        assert_eq!(state.glyph_outlines_json(id, &vec![1; MAX_GLYPHS + 1]).unwrap_err().code(), "BUDGET_EXCEEDED");
        assert!(state.glyph_outlines_json(id, &[u16::MAX]).is_err());
        assert_eq!(state.snapshot_json(1, 1, 0, 100, true).unwrap(), before);
    }

    #[test]
    fn outline_wire_preserves_quadratic_controls_and_contour_closure() {
        let outline = GlyphOutline {
            contours: vec![crate::text::outline::Contour {
                start: Point { x: 1.5, y: -2.0 },
                segments: vec![Segment::Quad {
                    ctrl: Point { x: 3.0, y: 4.5 }, to: Point { x: 1.5, y: -2.0 },
                }],
            }],
            advance: 10, lsb: 0, rsb: 0, bbox: None,
        };
        let mut writer = Bounded { text: String::new() };
        write_glyph(&mut writer, 7, &outline).unwrap();
        assert_eq!(writer.text, "{\"glyphId\":7,\"commands\":[[\"M\",1.5,-2],[\"Q\",3,4.5,1.5,-2],[\"Z\"]]}");
        assert!(validate_point(Point { x: f64::NAN, y: 0.0 }).is_err());
    }
}
