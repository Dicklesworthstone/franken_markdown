//! Syntax paint over the authoritative shaped flow. Lexing never chooses line
//! breaks, reshapes a token, or changes the accessible source transcript.

use super::{DisplayItem, DisplayList, DisplayRect, DisplayTextRun};
use crate::flow_display::{
    DisplayBlock, FlowInlineStyle, FlowLayoutError, FlowLayoutOptions, FlowTextRole,
    ResumableFlowDisplay,
};
use crate::highlight::{self, Span, Tok};
use crate::span::SourceSpan;
use crate::text::{OwnedTextRun, RunGlyph, TextCluster};

#[path = "code_hosts.rs"]
mod hosts;

impl ResumableFlowDisplay {
    /// Styled, font-shaped flow with the shared fenced-code syntax highlighter.
    ///
    /// This is an opt-in extension of [`Self::to_styled_display_list`]. The same
    /// callback, line breaks, fonts, glyph advances and reading transcript are
    /// retained. Color roles use the HTML highlighter's `tok-kw`, `tok-ty`,
    /// `tok-fn`, `tok-st`, `tok-nu`, `tok-cm`, `tok-op` and `tok-pn` names;
    /// unclassified code keeps `code`. Hosts map those roles to their palette.
    /// Unknown or absent fence languages retain the ordinary styled output.
    ///
    /// A fence is lexed as a whole, so multiline strings/comments survive line
    /// wrapping. Paint changes occur only at existing shaped-cluster boundaries.
    /// A cluster crossing a token boundary takes its first source byte's color.
    /// Each resulting run retains fragment-local UTF-8/UTF-16 and glyph indices,
    /// actual font identities, and the enclosing Markdown source span.
    ///
    /// Admission precedes shaping: each supported fence may contain at most
    /// `max_items` UTF-8 bytes, a conservative upper bound on lexer span count.
    /// Supported fences together may contain at most `max_total_shape_bytes`
    /// bytes (a separate allowance from shaping). Final paint items still obey
    /// `max_items`. These checks bound lexer allocation before it happens.
    /// Errors return no partial list and never mutate the source engine.
    pub fn to_highlighted_display_list<F>(
        &self,
        options: FlowLayoutOptions,
        shape: F,
    ) -> Result<DisplayList, FlowLayoutError>
    where
        F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        let mut bytes = 0usize;
        for block in self.blocks() {
            if let DisplayBlock::CodeBlock {
                language: Some(language),
                source,
            } = block
            {
                if highlight::is_supported(language) {
                    if source.len() > options.max_items {
                        return Err(FlowLayoutError::BudgetExceeded("syntax source bytes per fence"));
                    }
                    bytes = bytes
                        .checked_add(source.len())
                        .filter(|bytes| *bytes <= options.max_total_shape_bytes)
                        .ok_or(FlowLayoutError::BudgetExceeded("total syntax source bytes"))?;
                }
            }
        }
        let list = self.to_styled_display_list(options, shape)?;
        paint(self.blocks(), list, options.max_items)
    }
}

struct Fence<'a> {
    source: &'a str,
    language: &'a str,
    bounds: DisplayRect,
    span: SourceSpan,
    tokens: Option<Vec<Span>>,
    token: usize,
    byte: usize,
}

impl Fence<'_> {
    fn skip_newlines(&mut self) {
        while self.source.as_bytes().get(self.byte) == Some(&b'\n') {
            self.byte += 1;
        }
    }

    fn finish(&mut self) -> Result<(), FlowLayoutError> {
        self.skip_newlines();
        if self.byte != self.source.len() {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        Ok(())
    }

    fn paint(
        &mut self,
        mut text: DisplayTextRun,
        output: &mut Vec<DisplayItem>,
        max_items: usize,
    ) -> Result<(), FlowLayoutError> {
        self.skip_newlines();
        let end = self.byte.checked_add(text.text.len())
            .ok_or(FlowLayoutError::InvalidShapedRun)?;
        if self.source.get(self.byte..end) != Some(text.text.as_str()) {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        let tokens = self.tokens.get_or_insert_with(|| highlight::highlight(self.language, self.source));
        let run = text.font_run.as_ref().ok_or(FlowLayoutError::InvalidShapedRun)?;
        let mut logical: Vec<_> = run.clusters.iter().collect();
        logical.sort_by_key(|cluster| cluster.byte_range.start);
        let mut groups: Vec<(usize, usize, Tok, f32, f32)> = Vec::new();
        for (index, cluster) in logical.iter().enumerate() {
            let byte = self.byte + cluster.byte_range.start;
            while tokens.get(self.token).is_some_and(|token| token.end <= byte) {
                self.token += 1;
            }
            let token = tokens.get(self.token).filter(|token| token.start <= byte)
                .ok_or(FlowLayoutError::InvalidShapedRun)?;
            if let Some((_, last, kind, left, right)) = groups.last_mut() {
                // Logical neighbors must also touch visually. This keeps even
                // non-monotone host cluster orders in their original positions.
                if *kind == token.kind && (cluster.x_start == *right || cluster.x_end == *left) {
                    *last = index + 1;
                    *left = (*left).min(cluster.x_start);
                    *right = (*right).max(cluster.x_end);
                    continue;
                }
            }
            groups.push((index, index + 1, token.kind, cluster.x_start, cluster.x_end));
        }
        self.byte = end;
        if groups.len() <= 1 {
            if let Some((_, _, kind, _, _)) = groups.first() {
                text.color_role = kind.css_class().unwrap_or("code").to_owned();
            }
            return push(output, DisplayItem::Text(text), max_items);
        }
        if groups.len() > max_items.saturating_sub(output.len()) {
            return Err(FlowLayoutError::BudgetExceeded("highlighted display items"));
        }
        for (first, end, kind, left, right) in groups {
            let fragment = fragment(run, &logical[first..end], left, right)?;
            let item = DisplayTextRun {
                bounds: DisplayRect::new(
                    text.bounds.x + left, text.bounds.y, right - left, text.bounds.height,
                ),
                text: fragment.logical_text.clone(),
                font_run: Some(fragment),
                color_role: kind.css_class().unwrap_or("code").to_owned(),
                source_span: text.source_span,
                font_size: text.font_size,
            };
            push(output, DisplayItem::Text(item), max_items)?;
        }
        Ok(())
    }
}

fn fragment(
    original: &OwnedTextRun,
    logical: &[&TextCluster],
    left: f32,
    right: f32,
) -> Result<OwnedTextRun, FlowLayoutError> {
    let first = logical.first().ok_or(FlowLayoutError::InvalidShapedRun)?;
    let last = logical.last().ok_or(FlowLayoutError::InvalidShapedRun)?;
    let byte = first.byte_range.start;
    let utf16 = first.utf16_range.start;
    let text = original.logical_text.get(byte..last.byte_range.end)
        .ok_or(FlowLayoutError::InvalidShapedRun)?;
    let mut run = OwnedTextRun {
        context: original.context.clone(),
        logical_text: text.to_owned(),
        clusters: Vec::with_capacity(logical.len()),
        glyphs: Vec::new(),
        total_advance: right - left,
    };
    // Preserve glyph presentation order, independently of logical text order.
    let mut presentation = logical.to_vec();
    presentation.sort_by_key(|cluster| cluster.glyph_range.start);
    for cluster in presentation {
        let cluster_index = run.clusters.len();
        let glyph_start = run.glyphs.len();
        let glyphs = original.glyphs.get(cluster.glyph_range.clone())
            .ok_or(FlowLayoutError::InvalidShapedRun)?;
        for glyph in glyphs {
            run.glyphs.push(RunGlyph {
                glyph_id: glyph.glyph_id,
                font_id: glyph.font_id,
                cluster_index,
                x_advance: glyph.x_advance,
                y_advance: glyph.y_advance,
                x_offset: glyph.x_offset,
                y_offset: glyph.y_offset,
            });
        }
        run.clusters.push(TextCluster {
            cluster_index,
            byte_range: cluster.byte_range.start - byte..cluster.byte_range.end - byte,
            utf16_range: cluster.utf16_range.start - utf16..cluster.utf16_range.end - utf16,
            glyph_range: glyph_start..run.glyphs.len(),
            x_start: cluster.x_start - left,
            x_end: cluster.x_end - left,
            font_id: cluster.font_id,
        });
    }
    Ok(run)
}

fn push(output: &mut Vec<DisplayItem>, item: DisplayItem, max_items: usize) -> Result<(), FlowLayoutError> {
    if output.len() >= max_items {
        return Err(FlowLayoutError::BudgetExceeded("highlighted display items"));
    }
    output.push(item);
    Ok(())
}

fn paint(
    blocks: &[DisplayBlock],
    mut list: DisplayList,
    max_items: usize,
) -> Result<DisplayList, FlowLayoutError> {
    if blocks.len() != list.reading_order.len() {
        return Err(FlowLayoutError::InvalidShapedRun);
    }
    let fences: Vec<_> = blocks.iter().zip(&list.reading_order).filter_map(|(block, reading)| {
        let DisplayBlock::CodeBlock { language: Some(language), source } = block else {
            return None;
        };
        highlight::is_supported(language).then_some(Fence {
            source, language, bounds: reading.bounds, span: reading.source_span,
            tokens: None, token: 0, byte: 0,
        })
    }).collect();
    if fences.is_empty() {
        return Ok(list);
    }
    let mut fences = fences.into_iter();
    let mut active = fences.next();
    let input = std::mem::take(&mut list.items);
    let mut output = Vec::with_capacity(input.len());
    for item in input {
        while active.as_ref().is_some_and(|fence| item.bounds().y >= fence.bounds.bottom()) {
            if let Some(fence) = &mut active {
                fence.finish()?;
            }
            active = fences.next();
        }
        match (item, active.as_mut()) {
            (DisplayItem::Text(text), Some(fence))
                if text.color_role == "code" && text.source_span == fence.span
                    && text.bounds.y >= fence.bounds.y
                    && text.bounds.y < fence.bounds.bottom() =>
            {
                fence.paint(text, &mut output, max_items)?;
            }
            // The shaped emitter currently emits no clip commands. Do not
            // silently invalidate child counts if that contract changes.
            (DisplayItem::Clip(_), _) => return Err(FlowLayoutError::InvalidShapedRun),
            (item, _) => push(&mut output, item, max_items)?,
        }
    }
    if let Some(fence) = &mut active {
        fence.finish()?;
    }
    for mut fence in fences {
        fence.finish()?;
    }
    list.total_bounds = output.iter().fold(DisplayRect::default(), |bounds, item| bounds.union(item.bounds()));
    list.items = output;
    list.spatial = super::spatial::IndexCache::new();
    Ok(list)
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
