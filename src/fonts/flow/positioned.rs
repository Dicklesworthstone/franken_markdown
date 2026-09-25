//! Bridge positioned Latin combining sequences into the reader's owned runs.
//!
//! Ordinary text keeps the established bundled-font path. Only a word carrying
//! U+0300..U+036F marks uses the strict OpenType shaper. Marks never move to a
//! different face from their base, and source text is never normalized away.

use super::{BundledFlowFonts, Face, FlowInlineStyle, FlowTextRole, MAX_RUN_SCALARS, MONO, SYMBOL};
use crate::text::shaping::{Feature, ShapeOptions};
use crate::text::{Direction, FontOrigin, OwnedTextRun, TextCluster, TextRunContext};

pub(super) fn is_mark(ch: char) -> bool {
    matches!(ch as u32, 0x0300..=0x036f)
}

// Whitespace remains a separate segment so tabs retain the established four-
// space behavior. The strict shaper accepts Latin, not Greek/Cyrillic/symbols;
// those still use the existing profile. A mark after a non-Latin base is thus
// refused as an unattached mark rather than assigned to a different script.
fn segment_kind(ch: char) -> u8 {
    if ch.is_whitespace() {
        0
    } else if matches!(ch as u32, 0x20..=0x024f | 0x0300..=0x036f) {
        1
    } else {
        2
    }
}

impl BundledFlowFonts {
    pub(super) fn shape_positioned(
        &self,
        text: &str,
        size: f32,
        role: FlowTextRole,
        style: FlowInlineStyle,
    ) -> Result<OwnedTextRun, String> {
        if text.chars().take(MAX_RUN_SCALARS + 1).count() > MAX_RUN_SCALARS {
            return Err("bundled flow shaping scalar budget exceeded".to_owned());
        }
        let code = style.code || role == FlowTextRole::Code;
        let bold = style.bold
            || matches!(role, FlowTextRole::Heading(_) | FlowTextRole::TableHeader);
        let primary = if code { MONO } else { usize::from(bold) + 2 * usize::from(style.italic) };
        let mut output = OwnedTextRun {
            context: TextRunContext {
                font_id: self.faces[primary].id,
                font_size: size,
                script: *b"DFLT",
                language: *b"dflt",
                direction: Direction::LeftToRight,
                font_origin: FontOrigin::BundledFace,
            },
            logical_text: text.to_owned(),
            clusters: Vec::new(),
            glyphs: Vec::new(),
            total_advance: 0.0,
        };
        let mut start = 0;
        let mut utf16_start = 0;
        let mut chars = text.char_indices().peekable();
        while let Some((_, first)) = chars.next() {
            let kind = segment_kind(first);
            while chars.peek().is_some_and(|(_, ch)| segment_kind(*ch) == kind) {
                chars.next();
            }
            let end = chars.peek().map_or(text.len(), |(offset, _)| *offset);
            let source = &text[start..end];
            let part = if source.chars().any(is_mark) {
                // Whole-sequence fallback only: attaching a mark using another
                // font's anchors would invent invalid geometry and identities.
                let covers = |face: Face| source.chars().all(|ch| {
                    let glyph = face.font.glyph_index(ch);
                    glyph != 0 && glyph < face.font.num_glyphs
                });
                let face = [self.faces[primary], self.faces[SYMBOL]]
                    .into_iter().find(|face| covers(*face))
                    .ok_or_else(|| format!(
                        "no bundled flow face covers the combining sequence at byte {start}; supply a host shaper"
                    ))?;
                positioned_part(face, source, size, code).map_err(|reason| {
                    format!("bundled flow combining segment at byte {start}: {reason}")
                })?
            } else {
                // This recursion cannot re-enter the positioned path: this
                // segment contains no combining marks. Existing shaping,
                // tab handling, unsupported-script checks and fallback remain.
                self.shape(source, size, role, style)?
            };
            append_part(&mut output, part, start, utf16_start)?;
            utf16_start += source.encode_utf16().count();
            start = end;
        }
        Ok(output)
    }
}

fn positioned_part(face: Face, text: &str, size: f32, code: bool) -> Result<OwnedTextRun, String> {
    if face.font.units_per_em == 0 {
        return Err("invalid bundled flow font metrics".to_owned());
    }
    let code_features = [
        Feature { tag: *b"liga", enabled: false },
        Feature { tag: *b"kern", enabled: false },
    ];
    let options = ShapeOptions {
        features: if code { &code_features } else { &[] },
        ..ShapeOptions::default()
    };
    let shaped = face.font.shape(text, &options).map_err(|error| error.to_string())?;
    // Preserve every glyph's anchor offsets and the shared base/mark cluster.
    // The font engine reports unsupported lookup/mark mechanisms explicitly;
    // no heuristic placement or unpositioned-glyph fallback is permitted.
    OwnedTextRun::from_shaped_run(
        TextRunContext {
            font_id: face.id,
            font_size: size,
            script: *b"latn",
            language: *b"dflt",
            direction: Direction::LeftToRight,
            font_origin: FontOrigin::BundledFace,
        },
        &shaped,
        size / f32::from(face.font.units_per_em),
    )
}

/// Append a local run while rebasing all three coordinate domains. Recompute
/// global pen positions from the actual glyph advances, avoiding accumulated
/// float differences between per-segment totals and the eventual painter.
fn append_part(
    output: &mut OwnedTextRun,
    part: OwnedTextRun,
    byte_start: usize,
    utf16_start: usize,
) -> Result<(), String> {
    let invalid = || "invalid positioned flow run".to_owned();
    let byte_end = byte_start.checked_add(part.logical_text.len()).ok_or_else(invalid)?;
    if output.logical_text.get(byte_start..byte_end) != Some(part.logical_text.as_str())
        || part.context.direction != Direction::LeftToRight
    {
        return Err(invalid());
    }
    let mut next_byte = 0;
    let mut next_utf16 = 0;
    let mut next_glyph = 0;
    for (index, cluster) in part.clusters.iter().enumerate() {
        if cluster.cluster_index != index
            || cluster.byte_range.start != next_byte
            || cluster.byte_range.end <= next_byte
            || cluster.utf16_range.start != next_utf16
            || cluster.glyph_range.start != next_glyph
            || cluster.glyph_range.end <= next_glyph
        {
            return Err(invalid());
        }
        let source = part.logical_text.get(cluster.byte_range.clone()).ok_or_else(invalid)?;
        let utf16_end = next_utf16 + source.encode_utf16().count();
        if cluster.utf16_range.end != utf16_end {
            return Err(invalid());
        }
        let glyphs = part.glyphs.get(cluster.glyph_range.clone()).ok_or_else(invalid)?;
        let cluster_index = output.clusters.len();
        let glyph_start = output.glyphs.len();
        let x_start = output.total_advance;
        for glyph in glyphs {
            if glyph.cluster_index != index || glyph.font_id != cluster.font_id
                || glyph.glyph_id == 0 || glyph.y_advance != 0.0
                || !glyph.x_advance.is_finite() || glyph.x_advance < 0.0
                || !glyph.x_offset.is_finite() || !glyph.y_offset.is_finite()
            {
                return Err(invalid());
            }
            let next = output.total_advance + glyph.x_advance;
            if !next.is_finite() {
                return Err(invalid());
            }
            let mut rebased = *glyph;
            rebased.cluster_index = cluster_index;
            output.glyphs.push(rebased);
            output.total_advance = next;
        }
        output.clusters.push(TextCluster {
            cluster_index,
            byte_range: byte_start + cluster.byte_range.start..byte_start + cluster.byte_range.end,
            utf16_range: utf16_start + next_utf16..utf16_start + utf16_end,
            glyph_range: glyph_start..output.glyphs.len(),
            x_start,
            x_end: output.total_advance,
            font_id: cluster.font_id,
        });
        next_byte = cluster.byte_range.end;
        next_utf16 = utf16_end;
        next_glyph = cluster.glyph_range.end;
    }
    if next_byte != part.logical_text.len() || next_glyph != part.glyphs.len() {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::text::{Font, FontId};
    use crate::theme::FontFamily;
    use std::sync::OnceLock;

    // This original OFL fixture has independent HarfBuzz reference positions
    // in fmd-font/fonts/test-shaping/harfbuzz-reference.tsv. Its legacy layout
    // cache is not used by any of these all-marked-word shaping calls.
    fn fixture_fonts() -> BundledFlowFonts {
        static FONT: OnceLock<Font> = OnceLock::new();
        let bytes = include_bytes!("../../../fmd-font/fonts/test-shaping/FmdShaping.ttf");
        let font = FONT.get_or_init(|| Font::parse(bytes.to_vec()).unwrap());
        let mut fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let original = fonts.faces[0];
        fonts.faces[0] = Face { font, bytes, id: FontId::from_font_data(bytes), ..original };
        fonts
    }

    #[test]
    fn positioned_marks_match_independent_harfbuzz_geometry_and_source_clusters() {
        let fonts = fixture_fonts();
        let run = fonts.shape("a\u{0301}b", 10.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        assert_eq!(run.logical_text, "a\u{0301}b");
        assert_eq!(run.glyphs.len(), 3);
        assert_eq!(run.clusters.len(), 2);
        assert_eq!(run.clusters[0].byte_range, 0..3);
        assert_eq!(run.clusters[0].utf16_range, 0..2);
        assert_eq!(run.clusters[0].glyph_range, 0..2);
        assert_eq!(run.clusters[1].byte_range, 3..4);
        assert_eq!(run.clusters[1].utf16_range, 2..3);
        assert_eq!(run.clusters[1].glyph_range, 2..3);
        assert_eq!(run.glyphs.iter().map(|g| g.glyph_id).collect::<Vec<_>>(), [2, 7, 3]);
        assert_eq!(run.glyphs[1].x_advance, 0.0);
        assert_eq!(run.glyphs[1].x_offset, -3.5);
        assert_eq!(run.glyphs[1].y_offset, 5.0);
        assert_eq!(run.total_advance, 10.0);
        assert_eq!(run.clusters[1].x_start, 5.0);
        assert_eq!(fonts.font_bytes(run.glyphs[1].font_id), Some(fonts.faces[0].bytes));
    }

    #[test]
    fn stacked_marks_share_one_atomic_selection_cluster() {
        let fonts = fixture_fonts();
        let run = fonts.shape("a\u{0301}\u{0307}", 10.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        assert_eq!(run.clusters.len(), 1);
        assert_eq!(run.clusters[0].byte_range, 0..5);
        assert_eq!(run.clusters[0].utf16_range, 0..3);
        assert_eq!(run.clusters[0].glyph_range, 0..3);
        assert!(run.glyphs.iter().all(|glyph| glyph.cluster_index == 0));
        assert_eq!(run.glyphs[2].y_offset, 6.5);
        for x in [0.1, 2.0, 4.9] {
            let hit = run.hit_test(x);
            assert!([0, 5].contains(&hit.caret.byte_offset));
            assert!([0, 3].contains(&hit.caret.utf16_offset));
        }
    }

    #[test]
    fn rebasing_keeps_mixed_faces_utf16_and_zero_advance_marks() {
        let ordinary = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let fixture = fixture_fonts();
        let prefix = "λ\t";
        let mut output = ordinary.shape(prefix, 10.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        let prefix_advance = output.total_advance;
        let clusters = output.clusters.len();
        let glyphs = output.glyphs.len();
        let suffix = "a\u{0301}";
        output.logical_text.push_str(suffix);
        let part = fixture.shape(suffix, 10.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        append_part(&mut output, part, prefix.len(), prefix.encode_utf16().count()).unwrap();
        let marked = &output.clusters[clusters];
        assert_eq!(marked.byte_range, 3..6);
        assert_eq!(marked.utf16_range, 2..4);
        assert_eq!(marked.glyph_range, glyphs..glyphs + 2);
        assert_eq!(marked.x_start, prefix_advance);
        assert_eq!(marked.x_end, prefix_advance + 5.0);
        assert_eq!(output.glyphs[glyphs + 1].cluster_index, clusters);
        assert_ne!(output.glyphs[0].font_id, output.glyphs[glyphs].font_id);
    }

    #[test]
    fn leading_marks_missing_glyphs_and_mark_budgets_do_not_fall_back_to_bad_geometry() {
        let fonts = fixture_fonts();
        for source in ["\u{0301}", "a\u{036f}"] {
            assert!(fonts.shape(source, 10.0, FlowTextRole::Body, FlowInlineStyle::default()).is_err());
        }
        let crowded = format!("a{}", "\u{0301}".repeat(65));
        assert!(fonts.shape(&crowded, 10.0, FlowTextRole::Body, FlowInlineStyle::default()).is_err());
        let over_budget = format!("{}\u{0301}", "a".repeat(MAX_RUN_SCALARS));
        assert!(fonts.shape(&over_budget, 10.0, FlowTextRole::Body, FlowInlineStyle::default()).is_err());
    }

    #[test]
    fn malformed_part_is_refused_instead_of_exposing_invalid_cluster_ranges() {
        let fonts = fixture_fonts();
        let good = fonts.shape("a\u{0301}", 10.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        for corruption in 0..5 {
            let mut part = good.clone();
            match corruption {
                0 => part.clusters[0].byte_range.start = 1,
                1 => part.clusters[0].utf16_range.end = 1,
                2 => part.glyphs[1].cluster_index = 99,
                3 => part.glyphs[1].y_offset = f32::NAN,
                _ => part.clusters[0].glyph_range.end = 99,
            }
            let mut output = good.clone();
            output.clusters.clear();
            output.glyphs.clear();
            output.total_advance = 0.0;
            assert!(append_part(&mut output, part, 0, 0).is_err());
        }
    }
}
