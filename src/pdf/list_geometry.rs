//! List gutters stop consuming space before their text leaves the page.
//!
//! Fitting lists retain the historical coordinates. When a complete marker
//! cannot leave one text em beside it, its own row precedes the item instead.
//! Wide ordinals wrap at the requested size; task markers remain one segment
//! so the writer emits exactly one checkbox and one accessible state.

use super::*;

pub(super) struct Geometry {
    pub indent: f32,
    pub content_indent: f32,
    pub hanging: bool,
}

pub(super) fn geometry(width: f32, indent: f32, marker_col: f32, size: f32) -> Geometry {
    let content_indent = indent + marker_col + 11.0 * (size / 11.0);
    if indent >= 0.0 && content_indent + size <= width {
        return Geometry {
            indent,
            content_indent,
            hanging: true,
        };
    }
    // Reclaim exhausted indentation, including enough room for an intact task
    // label. An ordinal wider than the whole body uses that full body measure.
    let indent = indent.max(0.0).min((width - marker_col.max(size)).max(0.0));
    Geometry {
        indent,
        content_indent: indent,
        hanging: false,
    }
}

/// Quotes, definitions and large headings can add indentation after entering
/// a list. Keep those children in the same physical body too. Non-list output
/// and ordinary fitting list descendants keep their exact old coordinates.
pub(super) fn contained_indent(indent: f32, size: f32, cx: &LayoutCx<'_>) -> f32 {
    if cx.list_stack.is_empty() {
        indent
    } else {
        indent.max(0.0).min((cx.page.content_w - size).max(0.0))
    }
}

pub(super) fn block_indent(block: &Block, indent: f32, cx: &LayoutCx<'_>) -> f32 {
    let size = match block {
        Block::Heading { level, .. } => cx.type_scale.h[(*level as usize).saturating_sub(1).min(5)],
        _ => cx.type_scale.body,
    };
    contained_indent(indent, size, cx)
}

fn hard_break() -> Tok {
    Tok {
        text: String::new(),
        slot: F_BODY,
        space: true,
        hard_break: true,
        literal: false,
        link: None,
        strike: false,
    }
}

pub(super) fn stacked_item(
    item: &crate::ast::ListItem,
    marker: Seg,
    indent: f32,
    group: u32,
    out: &mut Vec<Line>,
    cx: &mut LayoutCx<'_>,
) {
    let size = cx.type_scale.body;
    let mut toks = Vec::new();
    // Ordinary leading prose shares one paragraph with its marker. The forced
    // break keeps the marker above it, including every pagination alternative.
    // Figures, galleries and TOC markers keep their normal block paths.
    let leading = match item.blocks.first() {
        Some(Block::Paragraph(inlines))
            if toc_marker_depth(inlines).is_none()
                && standalone_image(inlines).is_none()
                && inline_image::gallery_images(inlines, cx.faces, size).is_none() =>
        {
            tokenize(inlines, false, false, false, None, &mut cx.links, &mut toks);
            apply_symbol_fallback(&mut toks, cx.faces);
            1
        }
        _ => 0,
    };
    if !toks.is_empty() {
        toks.insert(0, hard_break());
    }
    let flow = FlowSpec {
        group,
        kind: FlowKind::Paragraph,
    };
    if marker.width <= cx.page.content_w - indent {
        // A leading forced break emits an empty first text row. The existing
        // prefix painter fills that row with the intact marker and carries it
        // into each alternate shape, preserving task drawing and source order.
        layout_prefixed_inlines(
            toks,
            marker,
            PrefixSpec {
                content_indent: indent,
                size,
                gap_after: 2.0,
                flow,
            },
            out,
            cx,
        );
    } else {
        // Only unusually wide ordinals need multiple marker rows. A literal
        // body-face token supplies measured cluster breaks without inserting
        // hyphens, changing type size, or turning the label into code styling.
        toks.insert(
            0,
            Tok {
                text: marker.text,
                slot: marker.slot,
                space: false,
                hard_break: false,
                literal: true,
                link: None,
                strike: false,
            },
        );
        layout_inlines(toks, indent, size, 2.0, out, cx, flow);
    }
    for block in &item.blocks[leading..] {
        layout_block(block, indent, out, cx);
    }
}
