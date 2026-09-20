//! Bounded dependency-free JSON encoding for the browser session ABI.
use super::{BrowserFlowError, BrowserFlowSession, MAX_SNAPSHOT_BYTES};
use crate::display::{AccessibleReadingNode, AccessibleReadingRole, DisplayItem, DisplayRect, DisplayTextRun, VectorShapeType};
use crate::text::{CaretAffinity, Direction, FontOrigin, OwnedTextRun};
use crate::SourceSpan;
use std::fmt::{self, Write};
use std::ops::Range;

#[path = "reading_inline.rs"]
mod reading_inline;

struct Json { text: String, limit: usize }
impl Write for Json {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > self.limit.saturating_sub(self.text.len()) { return Err(fmt::Error); }
        self.text.push_str(value);
        Ok(())
    }
}
impl Json {
    fn string(&mut self, value: &str) -> fmt::Result {
        self.write_char('"')?;
        for ch in value.chars() {
            match ch {
                '"' => self.write_str("\\\"")?,
                '\\' => self.write_str("\\\\")?,
                '\n' => self.write_str("\\n")?,
                '\r' => self.write_str("\\r")?,
                '\t' => self.write_str("\\t")?,
                c if c < ' ' || matches!(c, '\u{2028}' | '\u{2029}') => write!(self, "\\u{:04x}", c as u32)?,
                c => self.write_char(c)?,
            }
        }
        self.write_char('"')
    }
    fn identity(&mut self, value: u64) -> fmt::Result { write!(self, "\"{value}\"") }
    fn number(&mut self, value: f32) -> fmt::Result {
        if !value.is_finite() { return Err(fmt::Error); }
        write!(self, "{value}")
    }
    fn rect(&mut self, rect: DisplayRect) -> fmt::Result {
        self.write_str("{\"x\":")?; self.number(rect.x)?;
        self.write_str(",\"y\":")?; self.number(rect.y)?;
        self.write_str(",\"width\":")?; self.number(rect.width)?;
        self.write_str(",\"height\":")?; self.number(rect.height)?;
        self.write_char('}')
    }
    fn span(&mut self, span: SourceSpan) -> fmt::Result {
        write!(self, "{{\"startByte\":{},\"endByte\":{}}}", span.start, span.end)
    }
    fn range(&mut self, range: &Range<usize>) -> fmt::Result {
        write!(self, "[{},{}]", range.start, range.end)
    }
}
fn encode(f: impl FnOnce(&mut Json) -> fmt::Result) -> Result<String, BrowserFlowError> {
    let mut json = Json { text: String::new(), limit: MAX_SNAPSHOT_BYTES };
    f(&mut json).map_err(|_| BrowserFlowError::SnapshotTooLarge)?;
    Ok(json.text)
}
fn header(w: &mut Json, state: &BrowserFlowSession) -> fmt::Result {
    w.write_str("{\"schemaVersion\":1,\"revision\":")?; w.identity(state.revision())?;
    w.write_str(",\"layoutRevision\":")?; w.identity(state.layout_revision())
}
fn pagination(w: &mut Json, range: &Range<usize>, count: usize) -> fmt::Result {
    write!(w, ",\"offset\":{},\"total\":{},\"nextOffset\":", range.start, count)?;
    if range.end < count { write!(w, "{}", range.end) } else { w.write_str("null") }
}

pub(super) fn snapshot(state: &BrowserFlowSession, range: Range<usize>, glyphs: bool)
    -> Result<String, BrowserFlowError>
{
    encode(|w| {
        header(w, state)?;
        write!(w, ",\"sourceLengthBytes\":{},\"sourceLengthUtf16\":{},\"pendingAssetCount\":{},\"readingNodeCount\":{}",
            state.source().len(), state.source_utf16_len, state.session.pending_assets().len(), state.session.display().reading_order().len())?;
        w.write_str(",\"shapingProfile\":\"bundled-simple-ltr\",\"sourceMapping\":\"enclosing-block-bytes\",\"totalBounds\":")?;
        w.rect(state.session.display().total_bounds())?;
        let options = state.session.layout_options();
        w.write_str(",\"options\":{\"viewportWidth\":")?; w.number(options.viewport_width)?;
        w.write_str(",\"bodySize\":")?; w.number(options.body_size)?;
        w.write_str(",\"codeSize\":")?; w.number(options.code_size)?;
        w.write_str(",\"lineHeight\":")?; w.number(options.line_height)?; w.write_char('}')?;
        let items = state.session.display().items();
        pagination(w, &range, items.len())?;
        w.write_str(",\"items\":[")?;
        for (relative, item) in items[range.clone()].iter().enumerate() {
            if relative != 0 { w.write_char(',')?; }
            write!(w, "{{\"index\":{},\"bounds\":", range.start + relative)?;
            w.rect(item.bounds())?;
            w.write_str(",\"enclosingSourceSpan\":")?; w.span(item.source_span())?;
            match item {
                DisplayItem::Text(text) => {
                    w.write_str(",\"kind\":\"text\",\"text\":")?; w.string(&text.text)?;
                    w.write_str(",\"colorRole\":")?; w.string(&text.color_role)?;
                    w.write_str(",\"fontSize\":")?; w.number(text.font_size)?;
                    w.write_str(",\"fontRun\":")?;
                    if let Some(run) = &text.font_run { font_run(w, run, glyphs)?; } else { w.write_str("null")?; }
                }
                DisplayItem::Vector(path) => {
                    w.write_str(",\"kind\":\"vector\",\"shape\":")?;
                    w.string(match path.shape {
                        VectorShapeType::HorizontalRule => "horizontal-rule",
                        VectorShapeType::TableBorder => "table-border",
                        VectorShapeType::CalloutAccentBar => "callout-accent-bar",
                        VectorShapeType::CheckboxOutline => "checkbox-outline",
                        VectorShapeType::CheckboxCheck => "checkbox-check",
                        VectorShapeType::DiagramBox => "diagram-box",
                        VectorShapeType::DiagramArrow => "diagram-arrow",
                        VectorShapeType::DiagramConnector => "diagram-connector",
                    })?;
                    w.write_str(",\"strokeWidth\":")?; w.number(path.stroke_width)?;
                    w.write_str(",\"colorRole\":")?; w.string(&path.color_role)?;
                }
                DisplayItem::Image(image) => {
                    w.write_str(",\"kind\":\"image\",\"requestId\":")?; w.identity(image.request_id)?;
                    w.write_str(",\"destination\":")?; w.string(&image.destination)?;
                    w.write_str(",\"altText\":")?; w.string(&image.alt_text)?;
                    write!(w, ",\"isResolved\":{}", image.is_resolved)?;
                }
                DisplayItem::Anchor(anchor) => {
                    w.write_str(",\"kind\":\"anchor\",\"target\":")?; w.string(&anchor.anchor_id)?;
                    write!(w, ",\"isHeading\":{},\"level\":{}", anchor.is_heading, anchor.level)?;
                }
                DisplayItem::Clip(clip) => write!(w, ",\"kind\":\"clip\",\"childCount\":{}", clip.child_count)?,
            }
            w.write_char('}')?;
        }
        w.write_str("],\"cache\":{")?;
        let stats = state.fonts.stats();
        w.write_str("\"hits\":")?; w.identity(stats.hits)?;
        w.write_str(",\"misses\":")?; w.identity(stats.misses)?;
        w.write_str(",\"evictions\":")?; w.identity(stats.evictions)?;
        write!(w, ",\"retainedEntries\":{},\"retainedPayloadBytes\":{}}}}}", stats.retained_entries, stats.retained_payload_bytes)
    })
}

fn font_run(w: &mut Json, run: &OwnedTextRun, glyphs: bool) -> fmt::Result {
    w.write_str("{\"coordinateSpace\":\"fragment-utf8-and-utf16\",\"fontId\":")?;
    w.identity(run.context.font_id.0)?;
    w.write_str(",\"fontSize\":")?; w.number(run.context.font_size)?;
    w.write_str(",\"direction\":")?;
    w.string(match run.context.direction { Direction::LeftToRight => "ltr", Direction::RightToLeft => "rtl" })?;
    w.write_str(",\"fontOrigin\":")?;
    w.string(match run.context.font_origin { FontOrigin::BundledFace => "bundled", FontOrigin::SystemFallbackFace => "system" })?;
    w.write_str(",\"script\":")?; w.string(&String::from_utf8_lossy(&run.context.script))?;
    w.write_str(",\"language\":")?; w.string(&String::from_utf8_lossy(&run.context.language))?;
    w.write_str(",\"totalAdvance\":")?; w.number(run.total_advance)?;
    write!(w, ",\"clusterCount\":{},\"glyphCount\":{}", run.clusters.len(), run.glyphs.len())?;
    if glyphs {
        w.write_str(",\"clusters\":[")?;
        for (i, cluster) in run.clusters.iter().enumerate() {
            if i != 0 { w.write_char(',')?; }
            write!(w, "{{\"index\":{},\"bytes\":", cluster.cluster_index)?; w.range(&cluster.byte_range)?;
            w.write_str(",\"utf16\":")?; w.range(&cluster.utf16_range)?;
            w.write_str(",\"glyphs\":")?; w.range(&cluster.glyph_range)?;
            w.write_str(",\"xStart\":")?; w.number(cluster.x_start)?;
            w.write_str(",\"xEnd\":")?; w.number(cluster.x_end)?;
            w.write_str(",\"fontId\":")?; w.identity(cluster.font_id.0)?; w.write_char('}')?;
        }
        w.write_str("],\"glyphs\":[")?;
        for (i, glyph) in run.glyphs.iter().enumerate() {
            if i != 0 { w.write_char(',')?; }
            write!(w, "{{\"glyphId\":{},\"clusterIndex\":{},\"fontId\":", glyph.glyph_id, glyph.cluster_index)?;
            w.identity(glyph.font_id.0)?;
            w.write_str(",\"xAdvance\":")?; w.number(glyph.x_advance)?;
            w.write_str(",\"yAdvance\":")?; w.number(glyph.y_advance)?;
            w.write_str(",\"xOffset\":")?; w.number(glyph.x_offset)?;
            w.write_str(",\"yOffset\":")?; w.number(glyph.y_offset)?;
            w.write_char('}')?;
        }
        w.write_char(']')?;
    }
    w.write_char('}')
}

pub(super) fn reading(state: &BrowserFlowSession, range: Range<usize>) -> Result<String, BrowserFlowError> {
    encode(|w| {
        header(w, state)?;
        let nodes = state.session.display().reading_order();
        let engine = state.session.engine();
        if nodes.len() != engine.blocks().len() { return Err(fmt::Error); }
        pagination(w, &range, nodes.len())?;
        w.write_str(",\"nodes\":[")?;
        for (i, node) in nodes[range.clone()].iter().enumerate() {
            if i != 0 { w.write_char(',')?; }
            reading_node(w, node, engine, range.start + i, None)?;
        }
        w.write_str("]}")
    })
}
fn reading_node(
    w: &mut Json, node: &AccessibleReadingNode,
    engine: &crate::flow_display::ResumableFlowDisplay, index: usize, cell: Option<usize>,
) -> fmt::Result {
    w.write_str("{\"role\":")?;
    w.string(match node.role {
        AccessibleReadingRole::Document => "document",
        AccessibleReadingRole::Heading { .. } => "heading",
        AccessibleReadingRole::Paragraph => "paragraph",
        AccessibleReadingRole::CodeBlock => "code-block",
        AccessibleReadingRole::List => "list",
        AccessibleReadingRole::ListItem => "list-item",
        AccessibleReadingRole::Table => "table",
        AccessibleReadingRole::TableHeaderRow => "table-header-row",
        AccessibleReadingRole::TableRow => "table-row",
        AccessibleReadingRole::TableHeaderCell => "table-header-cell",
        AccessibleReadingRole::TableCell => "table-cell",
        AccessibleReadingRole::BlockQuote => "blockquote",
        AccessibleReadingRole::ThematicBreak => "thematic-break",
        AccessibleReadingRole::Image => "image",
    })?;
    if let AccessibleReadingRole::Heading { level } = node.role { write!(w, ",\"level\":{level}")?; }
    w.write_str(",\"text\":")?; w.string(&node.text)?;
    w.write_str(",\"bounds\":")?; w.rect(node.bounds)?;
    w.write_str(",\"enclosingSourceSpan\":")?; w.span(node.source_span)?;
    reading_inline::fields(w, node, engine, index, cell)?;
    w.write_str(",\"children\":[")?;
    for (i, child) in node.children.iter().enumerate() {
        if i != 0 { w.write_char(',')?; }
        reading_node(w, child, engine, index, Some(i))?;
    }
    w.write_str("]}")
}

pub(super) fn assets(state: &BrowserFlowSession, range: Range<usize>) -> Result<String, BrowserFlowError> {
    encode(|w| {
        header(w, state)?;
        let assets = state.session.pending_assets();
        pagination(w, &range, assets.len())?;
        w.write_str(",\"requests\":[")?;
        for (i, asset) in assets[range].iter().enumerate() {
            if i != 0 { w.write_char(',')?; }
            w.write_str("{\"id\":")?; w.identity(asset.id.0)?;
            w.write_str(",\"generation\":")?; w.identity(asset.generation)?;
            w.write_str(",\"kind\":")?; w.string(asset.kind)?;
            w.write_str(",\"url\":")?; w.string(&asset.url)?;
            w.write_str(",\"altText\":")?; w.string(&asset.alt_text)?;
            write!(w, ",\"enclosingSourceByteOffset\":{},\"estimatedWidth\":{},\"estimatedHeight\":{}}}",
                asset.source_offset, asset.estimated_width, asset.estimated_height)?;
        }
        w.write_str("]}")
    })
}

pub(super) fn hit(state: &BrowserFlowSession, x: f32, y: f32) -> Result<String, BrowserFlowError> {
    encode(|w| {
        header(w, state)?;
        let display = state.session.display();
        w.write_str(",\"linkTarget\":")?;
        if let Some(link) = display.link_at_point(x, y) { w.string(&link.anchor_id)?; } else { w.write_str("null")?; }
        w.write_str(",\"hit\":")?;
        // Ignore heading/link/vector overlays for text caret hit testing.
        let found = display.items().iter().enumerate().rev().find(|(_, item)| {
            matches!(item, DisplayItem::Text(_) | DisplayItem::Image(_)) && item.bounds().contains_point(x, y)
        });
        if let Some((index, item)) = found {
            write!(w, "{{\"itemIndex\":{index},\"enclosingSourceSpan\":")?; w.span(item.source_span())?;
            if let DisplayItem::Text(text) = item {
                if let Some(run) = &text.font_run {
                    let hit = run.hit_test(x - text.bounds.x);
                    w.write_str(",\"coordinateSpace\":\"fragment-utf16\"")?;
                    write!(w, ",\"byteOffset\":{},\"utf16Offset\":{},\"clusterIndex\":{},\"isExact\":{},\"visualX\":",
                        hit.caret.byte_offset, hit.caret.utf16_offset, hit.cluster_index, hit.is_exact)?;
                    w.number(text.bounds.x + hit.caret.visual_x)?;
                    w.write_str(",\"affinity\":")?;
                    w.string(match hit.caret.affinity { CaretAffinity::Leading => "leading", CaretAffinity::Trailing => "trailing" })?;
                }
            }
            w.write_char('}')?;
        } else { w.write_str("null")?; }
        w.write_char('}')
    })
}

pub(super) fn selection(state: &BrowserFlowSession, text: &DisplayTextRun, index: usize, range: SourceSpan) -> Result<String, BrowserFlowError> {
    let run = text.font_run.as_ref().ok_or(BrowserFlowError::InvalidSelection)?;
    let copied = range.slice(&text.text).ok_or(BrowserFlowError::InvalidSelection)?;
    let rects = run.selection_rects(range.start..range.end, text.bounds.y, text.bounds.height);
    encode(|w| {
        header(w, state)?;
        write!(w, ",\"itemIndex\":{index},\"coordinateSpace\":\"fragment-utf16\",\"text\":")?;
        w.string(copied)?;
        w.write_str(",\"enclosingSourceSpan\":")?; w.span(text.source_span)?;
        w.write_str(",\"rectangles\":[")?;
        for (i, rect) in rects.iter().enumerate() {
            if i != 0 { w.write_char(',')?; }
            w.rect(DisplayRect::new(text.bounds.x + rect.x, rect.y, rect.width, rect.height))?;
        }
        w.write_str("]}")
    })
}

/// Errors cross wasm-bindgen as a compact structured JSON string. Only bounded
/// diagnostics are encoded; no source or asset bytes are attached.
#[cfg(any(test, feature = "wasm-bindgen"))]
pub(super) fn error_json(error: &BrowserFlowError) -> String {
    let mut out = Json { text: String::new(), limit: 4096 };
    let result = (|| -> fmt::Result {
        out.write_str("{\"code\":")?; out.string(error.code())?;
        out.write_str(",\"message\":")?; out.string(&error.to_string())?;
        out.write_char('}')
    })();
    if result.is_ok() { out.text } else { format!("{{\"code\":\"{}\",\"message\":\"flow operation failed\"}}", error.code()) }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn json_strings_escape_controls_without_changing_unicode() {
        let encoded = encode(|w| w.string("\"\\\n\r\t\0é😀\u{2028}")).unwrap();
        assert_eq!(encoded, "\"\\\"\\\\\\n\\r\\t\\u0000é😀\\u2028\"");
    }
    #[test]
    fn json_budget_is_enforced_before_each_append() {
        let mut w = Json { text: String::new(), limit: 3 };
        w.write_str("abc").unwrap();
        assert!(w.write_str("d").is_err());
        assert_eq!(w.text, "abc");
        assert!(w.number(f32::NAN).is_err());
        assert!(w.number(f32::INFINITY).is_err());
    }
    #[test]
    fn integer_identities_are_encoded_as_lossless_decimal_strings() {
        assert_eq!(encode(|w| w.identity(u64::MAX)).unwrap(), "\"18446744073709551615\"");
        assert!(error_json(&BrowserFlowError::InvalidIdentity).contains("\"code\":\"INVALID_IDENTITY\""));
    }
}
