//! Ready-to-use, deterministic bundled-face shaping for continuous-flow output.
//!
//! This is a simple left-to-right profile: precomposed Latin/Greek/Cyrillic,
//! common punctuation and standalone symbols, real GSUB ligatures and GPOS pair
//! kerning. Combining marks, bidi/joining scripts and emoji sequences require
//! a host shaper and are refused, not emitted as unpositioned or missing glyphs.
//! Tabs retain their logical byte and occupy four space advances. Code uses the
//! bundled single upright mono face without ligature/kerning substitutions.

use super::{FontStyle, OpenTypeLayoutTables};
use crate::display::DisplayList;
use crate::flow_display::{
    FlowInlineStyle, FlowLayoutError, FlowLayoutOptions, FlowTextRole, ResumableFlowDisplay,
};
use crate::text::{
    Direction, Font, FontId, FontOrigin, OwnedTextRun, RunGlyph, TextCluster, TextRunContext,
};
use crate::theme::FontFamily;
use std::ops::Range;

const MAX_RUN_BYTES: usize = 64 * 1024;
const MAX_RUN_SCALARS: usize = 16 * 1024;
const MONO: usize = 4;
const SYMBOL: usize = 5;

#[derive(Clone, Copy)]
struct Face {
    font: &'static Font,
    layout: &'static OpenTypeLayoutTables,
    bytes: &'static [u8],
    id: FontId,
}

/// Reusable bridge from the bundled font registry to styled flow display lists.
///
/// Fonts/layout tables use the existing immutable registry caches. Construct
/// once and reuse across documents/reflows; no system discovery, I/O, or runtime
/// dependencies are needed. Every emitted glyph identifies its actual face;
/// `font_bytes` lets a host install precisely that face in its drawing backend.
/// The mono family has one upright weight, matching the bundled registry.
///
/// ```
/// use franken_markdown::{FontFamily, ResumableFlowDisplay};
/// use franken_markdown::flow_display::FlowLayoutOptions;
/// use franken_markdown::fonts::BundledFlowFonts;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let fonts = BundledFlowFonts::new(FontFamily::Sans)?;
/// let mut engine = ResumableFlowDisplay::new("# Guide\n\n**Bold** and [link](#guide)", 64);
/// engine.process_all()?;
/// let display = fonts.render(&engine, FlowLayoutOptions::default())?;
/// assert!(display.anchors().any(|anchor| !anchor.is_heading));
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct BundledFlowFonts {
    faces: [Face; 6],
}

impl BundledFlowFonts {
    pub fn new(family: FontFamily) -> Result<Self, FlowLayoutError> {
        let body = |style| -> Result<Face, FlowLayoutError> {
            let bytes = super::body_bytes(family, style);
            Ok(Face {
                font: super::body_font(family, style).map_err(|e| FlowLayoutError::Shaping(e.to_string()))?,
                layout: super::body_layout_tables(family, style).map_err(|e| FlowLayoutError::Shaping(e.to_string()))?,
                bytes,
                id: FontId::from_font_data(bytes),
            })
        };
        let mono_bytes = super::mono_bytes(FontStyle::Regular);
        let symbol_bytes = super::symbol_bytes();
        Ok(Self {
            faces: [
                body(FontStyle::Regular)?, body(FontStyle::Bold)?,
                body(FontStyle::Italic)?, body(FontStyle::BoldItalic)?,
                Face {
                    font: super::mono_font(FontStyle::Regular).map_err(|e| FlowLayoutError::Shaping(e.to_string()))?,
                    layout: super::mono_layout_tables(FontStyle::Regular).map_err(|e| FlowLayoutError::Shaping(e.to_string()))?,
                    bytes: mono_bytes,
                    id: FontId::from_font_data(mono_bytes),
                },
                Face {
                    font: super::symbol_font().map_err(|e| FlowLayoutError::Shaping(e.to_string()))?,
                    layout: super::symbol_layout_tables().map_err(|e| FlowLayoutError::Shaping(e.to_string()))?,
                    bytes: symbol_bytes,
                    id: FontId::from_font_data(symbol_bytes),
                },
            ],
        })
    }

    /// Retrieve the immutable bytes for an actual glyph/cluster font identity.
    /// Unknown IDs are not guessed or replaced with another family.
    #[must_use]
    pub fn font_bytes(&self, id: FontId) -> Option<&'static [u8]> {
        self.faces.iter().find(|face| face.id == id).map(|face| face.bytes)
    }

    /// Render already-emitted blocks, preserving styles, links and real metrics.
    /// The engine must first be advanced with `step` or `process_all`.
    pub fn render(
        &self, engine: &ResumableFlowDisplay, options: FlowLayoutOptions,
    ) -> Result<DisplayList, FlowLayoutError> {
        engine.to_styled_display_list(options, |text, size, role, style| {
            self.shape(text, size, role, style)
        })
    }

    /// Shape one explicitly left-to-right fragment using the bundled profile.
    /// Can also be used as the callback to `to_styled_display_list`. Missing
    /// glyphs and unsupported sequences return errors with codepoint/byte
    /// locations, without echoing the original document.
    pub fn shape(
        &self, text: &str, size: f32, role: FlowTextRole, style: FlowInlineStyle,
    ) -> Result<OwnedTextRun, String> {
        if !size.is_finite() || size <= 0.0 {
            return Err("bundled flow requires a finite positive font size".to_owned());
        }
        if text.len() > MAX_RUN_BYTES {
            return Err("bundled flow shaping byte budget exceeded".to_owned());
        }
        let code = style.code || role == FlowTextRole::Code;
        let bold = style.bold || matches!(role, FlowTextRole::Heading(_) | FlowTextRole::TableHeader);
        let primary = if code { MONO } else { usize::from(bold) + 2 * usize::from(style.italic) };
        let mut source = Vec::new();
        let mut utf16 = 0;
        for (start, ch) in text.char_indices() {
            if source.len() >= MAX_RUN_SCALARS {
                return Err("bundled flow shaping scalar budget exceeded".to_owned());
            }
            if !simple_ltr_scalar(ch) {
                return Err(format!("bundled flow needs a host shaper for U+{:04X} at byte {start}", ch as u32));
            }
            let tab = ch == '\t';
            let mapped = if tab { ' ' } else { ch };
            let mut face = primary;
            let mut glyph = self.faces[face].font.glyph_index(mapped);
            if glyph == 0 {
                face = SYMBOL;
                glyph = self.faces[face].font.glyph_index(mapped);
            }
            if glyph == 0 || glyph >= self.faces[face].font.num_glyphs {
                return Err(format!("no bundled flow glyph for U+{:04X} at byte {start}; supply a host shaper", ch as u32));
            }
            let units = ch.len_utf16();
            source.push(Scalar {
                bytes: start..start + ch.len_utf8(), utf16: utf16..utf16 + units,
                face, glyph, tab,
            });
            utf16 += units;
        }
        let mut run = OwnedTextRun {
            context: TextRunContext {
                font_id: self.faces[primary].id, font_size: size,
                script: *b"DFLT", language: *b"dflt", direction: Direction::LeftToRight,
                font_origin: FontOrigin::BundledFace,
            },
            logical_text: text.to_owned(), clusters: Vec::new(), glyphs: Vec::new(), total_advance: 0.0,
        };
        let mut cursor = 0;
        while cursor < source.len() {
            let first = &source[cursor];
            let face = self.faces[first.face];
            if first.tab {
                push_glyph(&mut run, face, first.glyph, first.bytes.clone(), first.utf16.clone(),
                    f32::from(face.font.advance_width(first.glyph)) * 4.0, size)?;
                cursor += 1;
                continue;
            }
            let mut end = cursor + 1;
            while end < source.len() && source[end].face == first.face && !source[end].tab { end += 1; }
            let gids: Vec<_> = source[cursor..end].iter().map(|scalar| scalar.glyph).collect();
            let shaped: Vec<_> = if code { gids.iter().map(|&gid| (gid, 1)).collect() }
                else { face.layout.lig.substitute_with_spans(&gids) };
            let mut consumed = cursor;
            for (index, &(gid, count)) in shaped.iter().enumerate() {
                let next = consumed.checked_add(count).filter(|&n| count > 0 && n <= end)
                    .ok_or_else(|| "invalid bundled ligature source coverage".to_owned())?;
                let mut advance = f32::from(face.font.advance_width(gid));
                if !code {
                    if let Some(&(following, _)) = shaped.get(index + 1) {
                        advance += face.layout.kern.pair(gid, following) as f32;
                    }
                }
                push_glyph(&mut run, face, gid,
                    source[consumed].bytes.start..source[next - 1].bytes.end,
                    source[consumed].utf16.start..source[next - 1].utf16.end, advance, size)?;
                consumed = next;
            }
            if consumed != end { return Err("incomplete bundled ligature source coverage".to_owned()); }
            cursor = end;
        }
        Ok(run)
    }
}

struct Scalar {
    bytes: Range<usize>,
    utf16: Range<usize>,
    face: usize,
    glyph: u16,
    tab: bool,
}

#[allow(clippy::too_many_arguments)]
fn push_glyph(
    run: &mut OwnedTextRun, face: Face, gid: u16, bytes: Range<usize>, utf16: Range<usize>,
    design_advance: f32, size: f32,
) -> Result<(), String> {
    if gid == 0 || gid >= face.font.num_glyphs || face.font.units_per_em == 0 {
        return Err("invalid bundled flow glyph or font metrics".to_owned());
    }
    let advance = design_advance * (size / f32::from(face.font.units_per_em));
    let end = run.total_advance + advance;
    if !advance.is_finite() || advance < 0.0 || !end.is_finite() {
        return Err("invalid bundled flow glyph advance".to_owned());
    }
    let cluster_index = run.clusters.len();
    let glyph_index = run.glyphs.len();
    run.clusters.push(TextCluster {
        cluster_index, byte_range: bytes, utf16_range: utf16,
        glyph_range: glyph_index..glyph_index + 1,
        x_start: run.total_advance, x_end: end, font_id: face.id,
    });
    run.glyphs.push(RunGlyph {
        glyph_id: gid, font_id: face.id, cluster_index,
        x_advance: advance, y_advance: 0.0, x_offset: 0.0, y_offset: 0.0,
    });
    run.total_advance = end;
    Ok(())
}

// Explicitly a simple LTR profile, not a Unicode bidi or mark-positioning
// implementation. Independent mathematical symbols are safe only when mapped
// by a supplied bundled face; unsupported scripts/sequences never get .notdef.
fn simple_ltr_scalar(ch: char) -> bool {
    if ch == '\t' { return true; }
    if ch.is_control() { return false; }
    matches!(ch as u32,
        0x20..=0x024f | 0x0370..=0x0482 | 0x048a..=0x052f |
        0x1e00..=0x1eff | 0x2000..=0x200a | 0x2010..=0x2027 |
        0x2030..=0x205e | 0x20a0..=0x20cf | 0x2100..=0x23ff
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::display::DisplayItem;

    fn style(bold: bool, italic: bool, code: bool) -> FlowInlineStyle {
        FlowInlineStyle { bold, italic, code, ..FlowInlineStyle::default() }
    }

    #[test]
    fn face_selection_and_drawing_bytes_use_actual_bundled_identities() {
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                let run = fonts.shape("text", 14.0, FlowTextRole::Body, style(bold, italic, false)).unwrap();
                let bytes = super::super::body_bytes(family, FontStyle::new(bold, italic));
                assert_eq!(run.context.font_id, FontId::from_font_data(bytes));
                assert_eq!(fonts.font_bytes(run.context.font_id), Some(bytes));
                assert!(run.glyphs.iter().all(|g| g.font_id == run.context.font_id && g.glyph_id != 0));
            }
            let heading = fonts.shape("Title", 28.0, FlowTextRole::Heading(1), FlowInlineStyle::default()).unwrap();
            assert_eq!(heading.context.font_id, FontId::from_font_data(super::super::body_bytes(family, FontStyle::Bold)));
            let mono = fonts.shape("code", 13.0, FlowTextRole::Body, style(false, false, true)).unwrap();
            assert_eq!(mono.context.font_id, FontId::from_font_data(super::super::mono_bytes(FontStyle::Regular)));
            assert!(fonts.font_bytes(FontId::new(0)).is_none());
        }
    }

    #[test]
    fn ligatures_and_kerning_match_existing_font_tables_and_preserve_source_domains() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let face = fonts.faces[0];
        let text = "office AV café";
        let gids: Vec<_> = text.chars().map(|ch| face.font.glyph_index(ch)).collect();
        let expected = face.layout.lig.substitute_with_spans(&gids);
        let run = fonts.shape(text, 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
        assert_eq!(run.logical_text, text);
        assert_eq!(run.glyphs.iter().map(|g| g.glyph_id).collect::<Vec<_>>(), expected.iter().map(|p| p.0).collect::<Vec<_>>());
        let mut byte = 0;
        let mut utf16 = 0;
        for cluster in &run.clusters {
            assert_eq!(cluster.byte_range.start, byte);
            assert_eq!(cluster.utf16_range.start, utf16);
            let part = &text[cluster.byte_range.clone()];
            utf16 += part.encode_utf16().count();
            byte = cluster.byte_range.end;
            assert_eq!(cluster.utf16_range.end, utf16);
        }
        assert_eq!(byte, text.len());
        let mut expected_width = 0.0;
        for (index, &(gid, _)) in expected.iter().enumerate() {
            let kern = expected.get(index + 1).map_or(0.0, |next| face.layout.kern.pair(gid, next.0) as f32);
            expected_width += (f32::from(face.font.advance_width(gid)) + kern) * (14.0 / f32::from(face.font.units_per_em));
        }
        assert_eq!(run.total_advance, expected_width);
    }

    #[test]
    fn code_tabs_and_symbol_fallback_keep_real_glyphs_and_logical_text() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let run = fonts.shape("a\tfi", 13.0, FlowTextRole::Code, FlowInlineStyle::default()).unwrap();
        assert_eq!(run.logical_text, "a\tfi");
        assert_eq!(run.clusters.len(), 4);
        assert_eq!(run.clusters[1].byte_range, 1..2);
        let face = fonts.faces[MONO];
        let space = f32::from(face.font.advance_width(face.font.glyph_index(' '))) * (13.0 / f32::from(face.font.units_per_em));
        assert!((run.glyphs[1].x_advance - 4.0 * space).abs() < 0.001);
        for ch in ['•', '⇒', '∑'] {
            let run = fonts.shape(&ch.to_string(), 14.0, FlowTextRole::Body, FlowInlineStyle::default()).unwrap();
            let glyph = &run.glyphs[0];
            assert!(glyph.glyph_id != 0);
            let bytes = fonts.font_bytes(glyph.font_id).unwrap();
            let font = Font::parse(bytes.to_vec()).unwrap();
            assert_eq!(font.glyph_index(ch), glyph.glyph_id);
        }
    }

    #[test]
    fn complex_sequences_invalid_sizes_and_budget_excess_are_explicit_errors() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        for text in ["a\u{0301}", "אב", "العربية", "東京", "😀", "a\u{200d}b", "x\u{202e}y"] {
            assert!(fonts.shape(text, 14.0, FlowTextRole::Body, FlowInlineStyle::default()).is_err(), "{text:?}");
        }
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(fonts.shape("a", size, FlowTextRole::Body, FlowInlineStyle::default()).is_err());
        }
        assert!(fonts.shape(&"x".repeat(MAX_RUN_BYTES + 1), 14.0, FlowTextRole::Body, FlowInlineStyle::default()).is_err());
    }

    #[test]
    fn complete_styled_flow_renders_without_custom_shaper_or_system_fonts() {
        let source = "# *Guide*\n\nText with **bold**, *italic*, `code`, ~~strike~~ and [a link](#guide).\n\n- list item\n\n| Name | Value |\n| --- | --- |\n| **entry** | `42` |\n";
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            let options = FlowLayoutOptions { viewport_width: 220.0, ..FlowLayoutOptions::default() };
            let mut whole = ResumableFlowDisplay::new(source, usize::MAX);
            whole.process_all().unwrap();
            let expected = fonts.render(&whole, options).unwrap();
            assert!(expected.anchors().any(|a| !a.is_heading && a.anchor_id == "#guide"));
            for item in expected.items() {
                if let DisplayItem::Text(text) = item {
                    let run = text.font_run.as_ref().unwrap();
                    assert_eq!(text.text, run.logical_text);
                    assert!(text.bounds.right() <= options.viewport_width + 0.001);
                    assert!(run.glyphs.iter().all(|g| fonts.font_bytes(g.font_id).is_some()));
                }
            }
            for batch in [1, 2, 8] {
                let mut engine = ResumableFlowDisplay::new(source, batch);
                engine.process_all().unwrap();
                assert_eq!(fonts.render(&engine, options).unwrap(), expected);
            }
        }
    }
}
